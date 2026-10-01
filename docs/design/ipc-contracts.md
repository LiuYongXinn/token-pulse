# IPC 与前端契约

总入口：[详细开发设计](development-design.md)。本文定义目标 `api_version=1`，为接口草案而非现有命令。Rust DTO 是运行时协议权威，生成 TypeScript 类型 / JSON Schema 并提交；禁止两端分别手写不一致的字段定义。

## 1. 传输与公共类型

Tauri command 用于查询和操作，event 只通知变化；长任务返回 job_id。生产 UI 只访问本地打包命令，无 HTTP 数据库服务，无任意 shell / SQL 接口。[Tauri 官方命令文档](https://v2.tauri.app/develop/calling-rust/)

DTO 采用 snake_case、UTC 毫秒数值、十进制大整数字符串。不透明 ID 视为字符串，不从 ID 解析目录或会话含义。Token / revision 字符串使用正则 `^(0|[1-9][0-9]*)$`，金额由后端严格校验符号与位数；毫秒必须在可表示日期范围内。

```typescript
type DecimalInt = string;
type DecimalMoney = string;
type EpochMs = number;
type Id = string;

type AppError = {
  code: string;
  message_key: string;
  retryable: boolean;
  correlation_id: Id;
  source_id: Id | null;
  job_id: Id | null;
  details: Record<string, string | number | boolean | null>;
};

type Response<T> = {
  api_version: 1;
  request_id: Id;
  data: T;
};

type SnapshotMeta = {
  snapshot_id: Id;           // bundle 的响应身份，不自动表示分页租约
  data_revision: DecimalInt;
  price_revision: DecimalInt;
  generated_at_ms: EpochMs;
  parser_versions: string[];
  accounting_versions: string[];
  display_timezone: string;
};

type DateRange = { start_ms: EpochMs; end_ms: EpochMs; timezone: string };
type DimensionSelection =
  | { kind: 'all' }
  | { kind: 'ids'; ids: Id[]; include_unknown: boolean };

type UsageFilter = {
  range: DateRange;
  sources: DimensionSelection;
  models: DimensionSelection;
  projects: DimensionSelection;
  sessions: DimensionSelection;
};

type PriceBasis =
  | { mode: 'event_time' }
  | { mode: 'specified_time'; specified_at_ms: EpochMs };

type TokenMeasure = {
  value: DecimalInt | null;
  covered_total_tokens: DecimalInt;
  complete: boolean;
};

type TokenTotals = {
  total_tokens: DecimalInt;
  input_total: TokenMeasure;
  cached_input: TokenMeasure;
  noncached_input: TokenMeasure;
  output_total: TokenMeasure;
  reasoning_output: TokenMeasure;
  session_count: DecimalInt;
  usage_event_count: DecimalInt;
  reliable_turn_count: DecimalInt | null;
  reliable_turns_complete: boolean;
};

type PricingSummary = {
  redacted: boolean;
  basis: PriceBasis;
  currencies: Array<{
    currency: string;
    estimated_cost: DecimalMoney | null;
    priced_total_tokens: DecimalInt;
  }>;
  priced_total_tokens: DecimalInt;
  unpriced_total_tokens: DecimalInt;
  reasons: Array<{ code: string; total_tokens: DecimalInt; event_count: DecimalInt }>;
  calculating: boolean;
};

type Coverage = {
  state: 'complete' | 'partial' | 'unknown';
  pending_observation_count: DecimalInt;
  unattributed_observation_count: DecimalInt;
  unattributed_total_tokens: DecimalInt | null;
  pending_file_count: DecimalInt;
  source_issues: Array<{ source_id: Id; code: string; last_success_ms: EpochMs | null }>;
  format_issues: Array<{ format: string; count: DecimalInt }>;
  breakdown_complete: boolean;
};
```

隐私生效时 PricingSummary.redacted=true，金额为 null，敏感名称由后台替换；UI 显示“已隐藏”，不能将其误判为未计价。完整价格匹配审计仍保存在后台账本。

隐私是最新显示策略，不能被旧查询租约中的 settings 快照覆盖。后端发送响应前按当前 PrivacyState 再处理；前端开启隐私时立即清理旧敏感缓存并递增请求序号，晚到的未脱敏响应不得重新显示。关闭隐私后重新查询，不从已被脱敏 DTO 恢复原值。

UI 计算百分比时必须从精确分子 / 分母得到有限显示比例，分母为零或未知则 null / 不适用。`complete` 仅指当前已配置来源范围；不能表示整个账户无遗漏。不同币种分别显示，只有一个 USD 结果时使用 `$`。

### 1.1 参数限制

range 左闭右开，start < end；时区必须合法 IANA 名称。ids 数量每维度 ≤100，page_size 默认 50、最大 200；趋势 bucket 总数 ≤2000；search 长度 ≤256；请求必要载荷 ≤256 KiB。未识别命令、字段或枚举返回 INVALID_QUERY，不进行默认降级执行。

列表 / 图表使用受控 bucket 或分页，不将全部事件交给前端。未知维度通过 include_unknown 表达，不与名为“未知”的真实项目混为一个 ID。

## 2. 查询命令

|命令|请求|响应 / 一致性|
|---|---|---|
|`get_app_status`|request_id|生命周期、版本、服务能力、来源 / 作业摘要|
|`get_filter_options`|filter、dimension、search、cursor、page_size|可搜索候选、未知项、受控 opaque key；按 facet 忽略本维度选择|
|`get_dashboard_bundle`|filter、price_basis、grain、heatmap_range|同事务 summary、series、heatmap、recent_sessions、coverage、meta|
|`get_grouped_usage`|filter、price_basis、dimension、sort、limit|同事务模型 / 项目统计，未知分类、覆盖、meta|
|`open_query_snapshot`|filter、price_basis|snapshot_handle、meta、expires_at；真实只读事务租约|
|`query_sessions`|snapshot_handle、sort、cursor、page_size|稳定分页、next_cursor、同 meta|
|`query_usage_events`|snapshot_handle、sort、cursor、page_size|稳定事件分页、必要向量 / 方法 / 质量 / 规则|
|`get_session_bundle`|session_key、filter、price_basis|同事务范围内消费、关系、回合摘要和最近上下文|
|`query_turns`|snapshot_handle、session_key、cursor、page_size|可靠 turn_id 聚合；不能把请求数标为回合数|
|`get_context_snapshot`|session_key|最近有效上下文、快照时间、容量、质量；不伪装历史上下文|
|`close_query_snapshot`|snapshot_handle|释放租约，重复关闭幂等|
|`get_sources`|request_id|配置、规范路径、能力、可读性、扫描 / 导入状态|
|`query_diagnostics`|受控来源 / 类别 / 时间、cursor|诊断分页、必要位置、可执行恢复动作|
|`get_price_rules`|revision 或当前、分页|规则版本、来源、匹配与生效时间|
|`get_job` / `list_jobs`|job_id / 状态与分页|持久进度与明确最终状态|

旧方案的 get_snapshot / query_series 被 bundle 包含；正式工程统一采用本表名字，避免并列两套近似协议。确需独立轻量 query_series 时只在同租约内实现，不能与主页面响应拼凑快照。

### 2.1 Dashboard 返回示例

```json
{
  "api_version": 1,
  "request_id": "req-1",
  "data": {
    "meta": {
      "snapshot_id": "bundle-1",
      "data_revision": "42",
      "price_revision": "3",
      "generated_at_ms": 1790836320000,
      "parser_versions": ["codex-v1"],
      "accounting_versions": ["accounting-v1"],
      "display_timezone": "Asia/Shanghai"
    },
    "summary": {
      "total_tokens": "683067",
      "input_total": {"value":"630630","covered_total_tokens":"683067","complete":true},
      "cached_input": {"value":"429566","covered_total_tokens":"683067","complete":true},
      "noncached_input": {"value":"201064","covered_total_tokens":"683067","complete":true},
      "output_total": {"value":"52437","covered_total_tokens":"683067","complete":true},
      "reasoning_output": {"value":"19926","covered_total_tokens":"683067","complete":true},
      "session_count": "4",
      "usage_event_count": "12",
      "reliable_turn_count": null,
      "reliable_turns_complete": false
    }
  }
}
```

该示例只展示 meta / summary 形状，是原型合成数据；完整响应还包含 pricing、coverage、series、heatmap 和 recent_sessions。例中时间戳只作 DTO 数值示意，不作为生产当前时间。测试应从 fixture 检查完整 schema，不用此局部 JSON 充当完整响应。

series bucket 包含 start_ms、end_ms、display_label、utc_offset、totals 与 coverage。heatmap_range 与主 filter.range 分开；同一事务继承维度。上下文 DTO：context_tokens / model_context_window 为 DecimalInt|null，percentage 为 number|null，observed_at_ms 与 quality 必填。

正式 `DashboardRequest` 将 filter、price_basis、grain、heatmap_range 放在同一个 request 对象内，两种日期范围使用同一显示时区。`DashboardBundle.summary` 为 TokenTotals，pricing / coverage 独立返回；series / heatmap 使用上述完整桶结构，各最多 2000 项。recent_sessions 最多 10 项，按筛选范围内最新消费时间降序、session_key 升序，包含该范围的消费 / 费用及最新实际事件的模型 / 项目；不会使用最终 cwd 回填。meta 的解析 / 核算版本覆盖主范围与热力图中实际活跃事实，空范围保留空列表。完整 Rust 生成契约为运行时权威。

模型筛选 key 包含 provider 与规范模型名，由后端生成和解析；显示标签不能直接拼入 SQL。项目 / 会话 / 来源使用已有 ID。候选分页返回 key、display_name、count、next_cursor 和版本；未知选项通过 include_unknown 表达。facet 查询只忽略当前维度，其他筛选继续生效；隐私时只替换标签，不改变 key。

reliable_turn_count 表示已识别回合；reliable_turns_complete=false 时 UI 标“已识别回合”，不把它称为完整用户回合总数。usage_event_count 也不能替代模型调用数。

### 2.2 快照租约与游标

分页在同一只读事务上完成。初始普通只读池 2 连接，另允许最多 2 个专用租约连接；全应用总共 ≤4 个查询连接。租约总寿命 30 秒，不因滚动无限延长；请求间空闲超过 10 秒可提前回收。后台 Writer 不等待用户翻页。

cursor 为服务端签发的不透明游标：snapshot / filter hash、sort、最后键值、完整性签名。签名使用进程内随机密钥，重启自动失效；不在游标夹带路径或正文。非法、跨筛选、跨窗口或过期游标返回 CURSOR_INVALID / SNAPSHOT_EXPIRED。

租约拥有窗口 / 作业身份；其他窗口不能持另一个窗口句柄调用。查询可取消，不在连接上并发执行两个语句。WAL 超过初始保护阈值 64 MiB 或全局写入压力时可提前回收并明确返回过期。前端清理旧列表后取新租约，显示刷新，不静默拼接分页。

导出不使用交互租约无限延期：作业取得固定只读视图和价格版本，生成当前用户临时目录的私有一致副本 / 等效物化数据后释放主库快照，在副本分页输出，结束清理。实际备份 / 物化成本单独压测。

## 3. 写操作与作业命令

|命令|请求与效果|
|---|---|
|`choose_source_directory`|后端原生目录选择；返回一次性 selection_handle 和检测摘要|
|`manage_source`|add(handle)、pause、resume、detect、retain_remove；验证作用范围|
|`start_job`|import / reconcile / rebuild / export / backup / price_revalue；scope 与 request_key；返回 job_id|
|`cancel_job`|job_id；只请求安全取消，返回 accepted / already_finished / too_late|
|`set_project_alias`|project_id、alias、expected_settings_revision；不修改消费|
|`save_price_rule`|完整规则、expected_price_revision；校验时间重叠、精度与优先级后发布新版本|
|`retire_price_rule`|rule_id、expected_price_revision；保留历史可追溯规则|
|`get_settings`|结构版本、revision、生效值|
|`update_settings`|受控 patch、expected_settings_revision；校验后返回生效值与冲突|
|`prepare_data_action`|clear_source / clear_all / restore 的受控范围；返回影响与 confirmation_token|
|`commit_data_action`|confirmation_token、request_key；再次核对影响版本，启动安全作业|
|`open_source_location`|diagnostic_id 或 source_id；后台定位，不接受任意路径|
|`manage_notify_integration`|prepare / apply / restore；显示保真配置差异，校验当前内容归属|
|`manage_startup`|enabled；返回实际系统结果，失败不保存为成功|

selection_handle 绑定选择用途、当前窗口与 canonical target，5 分钟过期；消费一次，不能改成删除 / 任意执行目标。导出和备份使用专门 `choose_output_file`，验证不是源日志文件、数据库或安装资源。

confirmation_token 绑定范围、统计 revision、影响计数、当前窗口，60 秒有效；底层数据变化时拒绝 STALE_CONFIRMATION 并重新评估。备份恢复文件需独立验证，不能把用户选择的任意文件当数据库打开写入。

对长操作 request_key 幂等：相同 key + 相同请求返回同 job；同 key 不同 payload 拒绝 REQUEST_KEY_CONFLICT。查询 request_id 只用于关联，不承诺写幂等。应用重启后作业状态可查，无法继续的任务明确 interrupted。

### 3.1 作业 DTO

```typescript
type Job = {
  job_id: Id;
  kind: 'import' | 'reconcile' | 'rebuild' | 'export' | 'backup' | 'restore' | 'price_revalue' | 'clear';
  state: 'queued' | 'running' | 'validating' | 'publishing' | 'cancelling'
       | 'succeeded' | 'cancelled' | 'failed' | 'interrupted';
  phase: string;
  discovered_files: DecimalInt;
  discovery_complete: boolean;
  processed_files: DecimalInt;
  processed_bytes: DecimalInt;
  accepted_events: DecimalInt;
  pending_observations: DecimalInt;
  can_cancel: boolean;
  error: AppError | null;
  created_at_ms: EpochMs;
  updated_at_ms: EpochMs;
};
```

discovery_complete=false 时不展示确定百分比。cancel accepted 不等于已经取消；UI 保持 cancelling 到最终通知。publishing 阶段不能强杀进程撤销事务。

## 4. 小窗、额度与任务栏命令

```typescript
type MiniScope =
  | { kind: 'today_all_sources' }
  | { kind: 'session'; session_key: Id; start: { kind: 'today' } | { kind: 'fixed'; start_ms: EpochMs } };

type QuotaWindow = {
  window_id: string;
  duration_mins: number | null;
  used_percent: number | null;
  remaining_percent: number | null;
  resets_at_ms: EpochMs | null;
};

type QuotaSnapshot = {
  connection_epoch: Id;
  quota_revision: DecimalInt;
  state: 'disconnected' | 'connecting' | 'authorization_required' | 'unsupported' | 'ready' | 'stale' | 'error';
  selected_limit_id: string | null;
  available_limits: Array<{ limit_id: string; display_name: string | null }>;
  fetched_at_ms: EpochMs | null;
  last_attempt_at_ms: EpochMs | null;
  windows: QuotaWindow[];
  error_code: string | null;
};

type MiniSnapshot = {
  usage_meta: SnapshotMeta;
  mini_scope: MiniScope;
  scope_display_name: string | null;
  usage: TokenTotals;
  pricing: PricingSummary;
  coverage: Coverage;
  quota: QuotaSnapshot;
  privacy: boolean;
  usage_last_success_ms: EpochMs | null;
};
```

今日 fixed session 口径随配置时区跨日更新；固定 start_ms 不随午夜改变。start 不能晚于查询截止时间。切换 mini 范围不改变主窗口筛选；从小窗“打开统计”显式带入对应范围和会话。

|命令|作用 / 权限|
|---|---|
|`get_mini_snapshot`|主窗口、小窗；返回独立用量与额度修订|
|`set_mini_scope`|主窗口、小窗；后台保存并通知所有入口|
|`get_account_quota` / `refresh_account_quota`|主窗口、小窗；刷新返回受限请求状态，保留旧快照|
|`manage_account_connection`|仅主窗口；connect / authorize / disconnect / select_limit，受控配置|
|`set_display_privacy`|主窗口、小窗、原生动作；返回 settings revision，所有入口同步|
|`perform_window_action`|主窗口、小窗；受限打开统计、展开、隐藏、置顶、恢复交互|
|`get_taskbar_status`|偏好与实际宿主状态、能力、版本、失败原因|
|`set_taskbar_preferences`|仅主窗口；启用、布局、项目、位置、目标屏和回退|
|`retry_taskbar_embed`|主窗口 / 托盘；返回请求状态，有限退避|

正式额度不固定 5h，remaining_percent 仅在 used_percent 有限有效时计算并 clamp 到 [0,100]。UI 的倒计时从 resets_at_ms 得出，不能修改 quota_revision 或剩余比例。

## 5. 事件与前端同步

|事件|载荷|用途|
|---|---|---|
|`usage_changed`|data_revision、affected_session_keys（限长）、affected_range 或全量标记|使相关用量查询失效|
|`price_rules_changed`|price_revision、受影响模型或全量标记|重估刷新|
|`settings_changed`|settings_revision、changed_keys|主题 / 隐私 / 范围等重新读取|
|`source_status_changed`|source_id、status_revision|来源状态 / 覆盖变化|
|`job_progress`|job_id、progress_revision、当前进度 / 状态|可被遗漏，重开 get_job|
|`account_quota_changed`|connection_epoch、quota_revision、state|重新获取当前连接快照|
|`taskbar_status_changed`|host_instance_id、host_revision、state、reason|显示已启用但不可嵌入等真实状态|
|`app_status_changed`|lifecycle_state、reason|迁移 / 恢复 / 退出反馈|

每类事件自身 revision 单调递增，重启带新的 runtime_id；不把几个无关计数器横向比较。变化通知合并 200–500 ms，小窗口查询轻量结果，长任务进度默认不超过 5 次 / 秒，最终状态立即发出。

前端挂载流程：订阅 → 记录收到的最大 revision → 查询 → 比较响应与挂载期间通知 → 必要时再查。新的筛选增加 request sequence，旧响应不覆盖新筛选。窗口卸载清理监听和租约；重开不信任上次缓存是最新。

事件载荷不包含全文、认证、全量事件、费用或敏感路径；敏感查询响应在后端按隐私处理。只有数字动画停止并不能保证隐私。

## 6. 权限与原生宿主协议

主窗口 label=`main`，小窗 label=`floating`。自定义命令通过 Tauri AppManifest / permissions 注册受限 allow 权限，并在 Rust 按 label 验证管理操作；动态创建任意 label 不向低权限窗口开放。Tauri 官方文档说明默认注册命令并不自动按窗口隔离。[Capabilities 官方文档](https://v2.tauri.app/security/capabilities/)

任务栏不是 WebView，使用独立当前用户命名管道。安全握手包含 protocol_version=1、host_instance_id、启动时生成 nonce、当前连接 epoch；ACL 限当前用户并验证启动子进程身份。帧使用长度前缀，最大 64 KiB；只有快照、心跳、受限动作和关闭消息。

```text
main → host: hello / mini_snapshot / display_preferences / shutdown
host → main: ready / capabilities / action / status / heartbeat
action: open_float | open_stats | open_taskbar_settings | set_privacy | disable_taskbar
```

宿主只收到显示必要数据，不接收原路径、账户凭据或 SQL。隐私切换先发送清空敏感显示指令，再发送新快照；发送失败且无法确认宿主清屏时停止 / 隐藏宿主，不能让旧费用持续留在任务栏。心跳 5 s，连续 3 次缺失标 disconnected 并有限恢复，重启实例拒绝旧回调。

## 7. 错误码与恢复动作

|错误码|场景|前端行为|
|---|---|---|
|INVALID_QUERY / UNSUPPORTED_API|参数 / 协议错误|保留可修正表单，记录 correlation ID|
|SOURCE_UNREADABLE|单来源读失败|局部旧数据与检测入口|
|UNSUPPORTED_FORMAT / AMBIGUOUS_USAGE|观察格式 / 推断问题|显示覆盖缺口，进入诊断|
|CHECKPOINT_CONFLICT|准备批次已过时|后台重新读取，不要求用户手工重试消费|
|DB_WRITE_FAILED / DISK_FULL|提交失败|停止推进检查点，提供存储恢复入口|
|DB_CORRUPT / MIGRATION_FAILED|库 / 升级不可用|停止写入，备份 / 恢复界面|
|SNAPSHOT_EXPIRED / CURSOR_INVALID|租约 / 游标无效|重新取第一页，说明列表已刷新|
|REVISION_CONFLICT / STALE_CONFIRMATION|配置或删除范围已变化|重新读取差异，再评估操作|
|REQUEST_KEY_CONFLICT|幂等 key 复用不同请求|拒绝操作，不启动第二任务|
|PRICE_RULE_CONFLICT|同范围 / 同优先级规则有效时间重叠|保留旧规则，调整有效时间或优先级后重试|
|JOB_CANCELLED / JOB_INTERRUPTED|安全取消 / 重启中断|展示实际状态，提供可支持的恢复|
|QUOTA_DISCONNECTED / QUOTA_UNSUPPORTED|额度连接缺失 / 不支持|未知剩余，提供连接或能力说明|
|QUOTA_TIMEOUT / QUOTA_AUTH_REQUIRED|额度服务失败|保留同账户旧快照 / 引导授权|
|TASKBAR_UNSUPPORTED / TASKBAR_NO_SPACE / TASKBAR_EMBED_FAILED|宿主不可用|按偏好回退，不停止采集|
|NUMERIC_OVERFLOW|超出受支持数值域|保留诊断，不给截断 / 零结果|
|PERMISSION_DENIED|不允许窗口 / 原生调用|拒绝并脱敏记录，不升级权限|

AppError 不包含完整源记录、访问令牌、未经处理的系统错误串；message_key 由前端本地化。可安全重试的网络 / 文件失败与需要用户修改的配置错误必须分开。

## 8. 导出契约与验证

导出 manifest 包含 demo=false、UTC 导出时间、snapshot / data / price revision、过滤条件、时区、估价模式、解析 / 核算版本、隐私选项。逐事件包含完整 Token 字符串、未知字段 null、规则 ID、currency、未计价原因与计算依据。

CSV 将可能被表格解释为公式的用户标签转为安全文本，数值列以精确字符串输出；路径隐藏由后台执行。JSON schema 显式区分零与 null。先写临时文件、完成校验再原子发布到用户选择目标，取消 / 失败清理临时产物。

契约检查：DTO schema round-trip、超大整数、非法区间 / 枚举 / 游标、窗口越权、序号乱序、通知丢失、快照过期、幂等 key 冲突、费用空值、额度 epoch 变化、隐私宿主断连与安全导出。主页面同快照和分页租约需使用真实并发写入验证，不能只断言几个 revision 字符串相同。

`get_grouped_usage` 的正式请求将 filter、price_basis、dimension（models / projects）、sort（total_desc / name_asc）及 limit（1–200）放在 request 对象内。响应为 GroupedUsageBundle：完整筛选 summary / pricing / coverage / meta，加 groups（每组 key|null、display_name、totals、pricing、coverage）、含未知分类的 total_group_count 和 truncated。limit 仅限制显示行，不改变整体汇总；同事务规则与全部来源证据用于估价，来源筛选不缩小规则匹配证据。模型 key 当前基于实际 provider / model，版本化别名规范化需在后续接口统一。

`get_filter_options` 的正式 request 为 `{ query, cursor }`。query 包含 filter、dimension（sources / models / projects / sessions）、search（最多 256 个 Unicode 码点）与 page_size（1–200）；cursor 首次为 null，续页必须保留整个 query。FilterOptionsPage 返回 meta、dimension、options（key|null、display_name、十进制可信用量事件 count）和 next_cursor|null；选项上限 200。facet_filter 仅忽略本维度选择，其他条件仍生效。候选 count 不是完整导入证明，缺口继续由统计 Coverage 表达。游标固定 151 字符，形状检查不替代 MAC / 窗口绑定 / 实际租约与服务器登记位置验证。

候选租约按稳定 key / null 首位分页，最后一页自动关闭；候选框关闭、换搜索或换筛选时调用 `close_query_snapshot`，正式 `CloseQuerySnapshotRequest` 支持 `{ kind: "filter_options", request: { query, cursor } }` 与 `{ kind: "sessions", request: { query, cursor } }`。cursor 必须是该完整 query 与可信窗口已签发的非 null 游标，响应 data 为 null。MAC / 绑定先校验；已过期或重复关闭同一合法能力幂等成功，伪造、跨窗口或改查询仍拒绝。该命令预留后续分页租约类型，不把可见 snapshot_id 字符串当关闭权限。主窗口独占这些命令的 capability，后台再次校验 label；其他窗口不直接枚举来源 / 模型 / 项目 / 会话。

### 已实现会话分页的正式 DTO

`query_sessions` 接收 `SessionsRequest { query: { filter, price_basis, sort, page_size }, cursor }`。sort 为 latest_desc / total_desc，page_size 1–200；cursor=null 在命令内取得真实租约并原子读取第一页，续页使用同 query 的已签发游标。对前端合并了“打开租约 / 读第一页”步骤，保留 §2.2 的真实只读事务、窗口绑定、TTL / WAL 保护；无需前端拿可见 snapshot_id 当授权句柄。终页释放租约，已取得 DTO 可继续显示。

`SessionsPage` 同时返回 meta、整个 filter 的 summary / pricing / coverage、sessions 和 next_cursor；不能把每页消费当总范围消费。每行包含范围内最新可信事件的时间 / 模型 / 项目、消费 / 费用 / 覆盖，以及独立的 latest_context。父会话 key / 显示名只使用已解析关系；未解析 parent_provider_id 保留，不猜测关联。child_count 为跨日期已登记、已解析的非镜像子会话数量，与选定范围的消费会话数不同。关系本身不证明继承扣除已经确认，继承依据后续由详情模块提供。

latest_desc 以最新选定事件时间降序、session_key BINARY 升序；total_desc 以精确非负十进制总量降序、session_key BINARY 升序。总量先补至相同 39 位文本宽度比较，不转 SQLite REAL 或限制为 i64；游标位置由后端登记。所有页的 facts、价格规则版本、项目标签、父子关系和最近上下文均来自同一事务，首请求 generated_at_ms 保持不变。排序 / 页长 / 范围 / 价格依据 / 窗口改变均不能重绑旧游标。

### 已实现明细分页的正式 DTO

`query_usage_events` 接收 `UsageEventsRequest { query: { filter, price_basis, sort, page_size }, cursor }`；sort 为 time_desc / total_desc，页长 1–200。首请求在命令内取得真实租约，续页绑定完整 query 与可信窗口；关闭使用 `CloseQuerySnapshotRequest` 的 usage_events 变体。事件自身的 total_tokens 为原始 i64，按 SQLite INTEGER 精确比较；时间或消费量降序后以 event_id BINARY 升序打破平局。游标 / TTL / WAL 与会话分页一致。

`UsageEventsPage` 返回整个范围的 summary / pricing / coverage、meta、events 与 next_cursor。行包含消费精确分项、总量、时间、会话 / 项目 / 实际模型 / 提供方、不同物理来源 ID、可靠 turn_id（可空）、calculation_method、quality_flags、parser / accounting 版本及同 price_revision 的 PriceOutcome（规则 ID / 分币种精确金额，或明确未计价原因）。来源以事件 provenance 去重，选择某个镜像来源不改变事实或价格身份。

原始 last / cumulative 只从必要观察的白名单向量提取，缺失保留 null；不向前端返回 normalized_json、正文、路径或任意原始 JSON。`RawUsageVector` 使用 RawTokenCount：原始 i64 经精确十进制字符串传输，允许诊断中的原始负数，Rust 拒绝越界、非规范字符串和 JSON number。这是原始证据的传输类型；已发布消费仍按对应 accounting_version 验证为非负，total_tokens 使用 DecimalInt。原始 last 无效但独立累计向量合法时，不用 null 掩盖无效原始值，也不把该负数纳入消费求和。

### 2.8 已实现的会话详情 bundle

get_session_bundle 接收 SessionBundleRequest（session_key、完整 filter、price_basis），在一笔真实 SQLite 只读事务返回 SessionBundle。目标 session_key 与 filter.sessions 取交集，并在事务中解析镜像别名；不扩大来源 / 模型 / 项目 / 日期选择。不存在的会话为 INVALID_QUERY；已登记但范围内无事件时返回零总量、未知分项、latest_selected_activity=null，仍可返回独立的最近上下文和关系。

meta、identity、summary、pricing、coverage、latest_selected_activity、latest_context、child_count、children、children_truncated、classifications 属于同一事务。identity 与 children 的父关系只使用已经解析的 canonical key，未解析 parent_provider_id 保留。children 跨日期，按 canonical session_key 排序，排除镜像别名，最多 100 个；child_count 是全部已解析子关系数，超过上限显式 children_truncated=true。

classifications 是当前 active ledger 的跨日期观察分类，按 kind / reason_code 汇总 COUNT(DISTINCT observation_id)，最多 64 组；kind 限 pending / inherited / duplicate / unattributed。它表示已保存的分类证据数，不是消费 Token、回合或完整继承证明。不同分类的数量不假定互斥，不能相加作为全部观察数。不发送 vector_json、evidence_json 或日志内容，不能把原始累计证据重新加到可信消费。该接口的详情独立获得新快照，UI 必须整体替换详情而非将其字段补到旧列表快照。

### 2.9 已实现的可靠回合分页

query_turns 接收 TurnsRequest（query.session_key / filter / price_basis / page_size、cursor），最多 200 条，固定按所选事件最大时间降序 / turn_id 的 BINARY 升序稳定 keyset 分页。目标会话与 filter.sessions 取交集，解析镜像别名；不使用模型、请求数、时间邻近或计数流猜测回合。仅明确非空 turn_id 分组，每组的 first_at_ms / last_at_ms / summary / pricing 都只覆盖所选日期与维度下的事件，不能称为该回合的完整生命周期消耗。

TurnsPage 的 summary / pricing / coverage 覆盖指定会话全部所选消费，包括没有回合标识的事件；unidentified_usage_event_count 单独报告 null / 空标识事件数量。turns 是已识别回合，不将未知身份事件折成伪回合，不把总范围汇总当作当前页合计。meta / 价格 / 回合成员在真实租约中固定，与详情 bundle 独立。游标绑定 owner / 完整 query / turns 域，close_query_snapshot 的 Turns 分支接受原 TurnsRequest + 非空游标，合法重复关闭幂等；任意 snapshot_id 不授权读取。末页和失败释放租约。

### 2.10 日历日期与 IANA 时区转换

resolve_calendar_selection（仅主窗口）接收 CalendarSelectionRequest：timezone 为受限长度的合法 IANA 时区；selection 为 today / last7 / last30 / custom。custom 必填严格 YYYY-MM-DD 的 start_date 与 end_date_inclusive，用户结束日包含在选择内，后台转成下一当地日期边界作为排他 UTC 截止。CalendarSelectionResult 返回 range、独立的近 182 当地日 heatmap_range、后台当前时刻在该时区的 local_today。固定自定义日期不随午夜改变，热力图仍为当前时刻的独立近 26 周。

所有边界复用 Rust / chrono-tz：重复当地午夜选择更早的 UTC 边界，午夜缺口采用其后的首个有效时刻；整日跳过导致空 UTC 区间则 INVALID_QUERY，不能凭空补 24 小时。反向区间、无效闰日、不规范日期、未知时区和未知字段拒绝。该命令只解析日历，不代表数据快照或价格修订，也不自行改变主窗口 / mini_scope；UI 应成功取得 range 后整体发起正式查询。

### 2.11 显示时区持久设置

get_display_settings / set_display_timezone 仅允许主窗口。DisplaySettingsSnapshot 返回 settings_version、精确 settings_revision 和 preferences.display_timezone；未初始化时保留 null，不当作 UTC 或系统值。TimezoneMutation.Initialize 只在没有有效已保存时区时记录经 Rust 验证的 system_timezone，重复初始化不覆盖用户选择；Set 必填经校验的 display_timezone 与 expected_settings_revision。配置采用独立版本 1 和全局 settings_revision，来源配置变化也可能引起 REVISION_CONFLICT；不能忽略冲突强写。

修改在 Writer 的同一事务中保存 payload_json / updated_at_ms 与 revision；相同值不递增，不触发通知。成功实际变更提交后发送 settings_changed，仅携带 settings_revision；失败 / 无变化 / 重复初始化不发送。读取旧真实 SQLite 事务仍看到旧配置，但该历史配置不得用于覆盖后续的最新隐私策略。

设置表已经包含主题、旧 privacy、小窗范围、任务栏和启动偏好。时区修改保留这些已识别版本 1 字段，只改 display_timezone，不用公开的 DisplayPreferences DTO 覆盖整个 payload。字段类型 / 时区损坏返回 DB_CORRUPT；不支持的 settings_version 返回 UNSUPPORTED_SETTINGS_VERSION，保留原配置，不写默认值。主题 / 隐私 / 小窗的实际应用仍由后续相应模块实现，此接口不会自行开启这些功能。

### 2.12 最新隐私响应出口（领域实现，实际 IPC 接入继续实施）

Response 增加可选 display_policy：DisplayPolicyStamp 包含 settings_revision / privacy。PrivateResponse 在实际 Serialize 时从共享 PrivacyState 读取最新策略，并在同一锁内序列化策略戳与经处理 DTO；不能在旧查询事务中固定隐私。commit_update 为持久设置事务提供同锁协调入口：失败不替换现有策略，已声称提交却返回倒退或同修订冲突策略时关闭显示发布，序列化只返回 DISPLAY_POLICY_UNAVAILABLE。普通迟到 publish 被拒绝，不改变当前策略。实际 Tauri 出口和前端世代门禁必须接入后才能把此领域能力视为隐私功能已交付。

PrivacyRedact 对每种公开 DTO 显式实现，不允许一个默认放行的泛型实现。会话 / 项目 / 来源使用稳定标识的 SHA-256 短替代标签，稳定 key / 精确 Token / 覆盖 / null / cursor / 账本 meta 不变；同标识跨列表、详情、子关系、明细保持同替代标签。PricingSummary.redacted=true，金额 null、费用原因清空；UsageEventRow.price 新增只用于显示的 redacted 标签，移除规则 / 金额 / 原子字段，不把隐私视为未计价或零金额。计价聚合拒绝 redacted 输入。

GroupedUsageBundle 没有 dimension 字段，必须使用携带可信请求 GroupDimension 的 PrivateResponse::groups；模型标签可保留，项目标签替换。缺少分组上下文时走保守替换，避免旧响应在策略切换后泄漏项目名。价格规则配置响应在隐私处理时移除 rules / aliases，策略戳明确表示显示限制；后续 UI 管理入口需按策略隐藏配置内容，而不能将其显示成无规则。开启后清旧敏感缓存 / 拒绝旧世代、关闭后重新查询，以及小窗 / 原生宿主同步仍在后续实施。

### 2.13 已接入的隐私 IPC

DisplayPreferences 公开 privacy；DisplayPrivacyMutation { privacy, expected_settings_revision } 使用十进制字符串 CAS，全局设置冲突与同值行为同 timezone。set_display_privacy 已注册主窗口权限；小窗权限随其实际实现增加。RuntimeState 初始化已保存策略，不可读配置先隐藏。通用 update_privacy 协调函数在 PrivacyState.commit_update 内提交数据库并发布策略，之后先发 display_policy_changed { settings_revision, privacy }，再发 settings_changed。同值 / 冲突不通知。

实际统计、来源、应用目录、价格规则、作业、显示设置出口均使用最新 PrivateResponse；GroupedUsage 明确传入模型 / 项目上下文。目录选择在已启用时拒绝，选择中途变化的返回结果仍由发送时策略处理。前端必须监听策略、丢弃旧缓存和迟到响应，并在关闭后重新查询；后台出口不能清除已经进入 WebView 的旧字段，该客户端门禁为后续必需实现。

### 2.14 主窗口显示策略门禁

主窗口要求敏感响应带 display_policy；DisplayPolicyGate 用精确十进制修订判定最新策略，并用独立 epoch 失效已显示 DTO。开启在发 IPC 前本地封闭，提交冲突 / 失败不自动恢复显示；明确关闭成功才重新查询。display_policy_changed 独立监听，settings_changed 继续只触发配置重读。已开启时空价格规则响应通过策略状态明确显示“已隐藏”，不解释为规则缺失。

响应请求捕获 epoch；返回旧策略 / 旧世代则拒绝数据。get_filter_options / query_sessions / query_usage_events / query_turns 的迟到拒绝必须用原 query 与返回 next_cursor 释放租约；close 不使用 snapshot_id。显示设置 / 关闭租约等无身份控制响应保留安全处理，纯日历 / 窗口动作没有显示字段。策略变化清空统计 / 来源缓存、候选和详情 / 价格编辑器；保留稳定筛选 ID / 日期 / 估价时点，旧会话 / 项目候选名字以通用文字替代，关闭后通过新查询恢复可显示值。

### 2.15 持久主题

DisplayPreferences 增加 theme: AppTheme（dark / light / system），缺少旧字段时读取既有深色默认。DisplayThemeMutation { theme, expected_settings_revision } 按全局修订 CAS 更新单一字段；get_display_settings / set_display_theme 返回完整显示配置与最新隐私戳。主题提交后 settings_changed，同值 / 冲突不通知，不发布隐私变化。主窗口权限已登记；共享 useAppTheme 根据保存值解析 system 并监听系统媒体变化，theme 与 themePreference 明确区分。原生应用窗口通过 Tauri set_theme 同步，任务栏宿主将按其自己的系统背景设计实现。

### 2.16 独立 mini 本地消费部分

MiniScopeMutation { mini_scope, expected_settings_revision } 使用全局 CAS；MiniScopeSnapshot 仅包含 settings_revision / mini_scope。get_mini_scope / set_mini_scope 主窗口权限先接入，成功更新发 mini_scope_changed 与 settings_changed；不带显示名称。固定会话须存在，start 不能晚于调用时点。

get_mini_usage 返回 MiniUsageSnapshot { meta, settings_revision, mini_scope, scope_display_name, range, usage, pricing, coverage }，在同一个读取事务中固定配置与数据 / 价格修订。today 按已保存时区当前日零点，fixed 是明确不漂移起点；range end 是捕获毫秒 + 1（包含当前采样毫秒，保持 SQL 半开）。Token / 费用范围相同，采用 event_time，与主窗口独立筛选 / 指定估价时点分离。缺少配置时区或未来固定起点明确失败，不默认范围。响应经最新 PrivateResponse。

这是完整 MiniSnapshot 的本地消费来源；账户数据由之后的独立服务快照组合，不在该 SQLite 事务内假定一致时间或计算额度。实际小窗 / 任务栏权限、原生操作和消费者随对应模块实施。


### 2.17 独立悬浮窗操作

WindowAction 增加 show_mini，由主窗口 perform_window_action 异步创建 / 显示独立 mini；托盘调用同一创建 / 恢复实现。MiniWindowAction 为 read / set_expanded { expanded } / set_pinned { pinned } / drag / hide，返回 MiniWindowState { expanded, pinned }，纯原生交互无敏感显示字段。只允许 mini；尺寸固定两组 DIP，无任意窗口标签 / 路径 / 外部 URL / 穿透参数。当前交互状态只在进程内保留，跨启动持久化后续实现。

mini capability 允许 get_mini_scope / get_mini_usage / set_mini_scope / get_display_settings / set_display_privacy；后两项共享隐私协调与最新出口。主题 / 时区写入继续仅 main。小窗 frontend 采用相同显示 epoch / 延迟响应门禁、共享主题，完整 MiniUsageSnapshot 单体更新；隐藏不关闭后台采集。账户区当前明确未连接，没有本地推测的额度值。


### 2.18 明确打开同范围统计

open_mini_stats 为 mini-only，参数 MiniStatsOpenRequest { expected_settings_revision } 绑定用户正在看的范围版本，真实 mini 使用事务修订不同时返回 REVISION_CONFLICT，不发布导航。成功返回 MiniStatsRequest { request_id, mini_scope, calendar }，calendar.range 保留精确 UTC 半开毫秒边界（包含采样毫秒），heatmap_range 由相同采样时刻 / 时区日历解析。该 DTO 只有稳定 ID / 日期，无敏感名称 / 金额 / 账户字段，使用普通 Response。main-only get_mini_stats_request 返回当前意图或 null，mini_stats_requested 只作为失效通知。

主窗口按意图 ID 只应用一次；新意图明确重置来源 / 其他维度 / specified_time 并进入总览，显示精确范围替代整日日期控件。刷新与分页沿用此范围，恢复主日历或重置不回写 mini_scope；恢复可见不会重新应用已消费过的 ID。小窗数据之后更新不暗中改变已打开主统计范围，须再次点击打开。

### 2.19 已登记会话候选与明确小窗起点

query_mini_sessions 允许 main / mini，接收 MiniSessionsRequest { query: { search, page_size }, cursor }，返回 MiniSessionsPage { meta, options: [{ session_key, display_name }], next_cursor }。搜索最多 256 个 Unicode 字符，不允许控制字符；页长 1–100。候选来自全部已登记 canonical sessions，包含没有消费事件的会话，排除已经验证的镜像别名，不继承主窗口日期 / 来源筛选。参数化 Unicode 字面搜索、BINARY session_key 升序 keyset 与实际 SQLite 只读租约保证续页期间新登记 / 重命名不改变既有候选。配置时区和数据 / 价格修订也在同一事务固定；generated_at_ms 沿用首请求。

游标仍为 151 字符的已认证能力，绑定可信窗口标签 / mini_sessions 域 / 完整 query。末页、失败、取消、搜索改变和迟到响应清理租约；过期不自动混入新快照。close_query_snapshot 增加 mini_sessions 变体；mini 只能关闭该变体，其他统计查询仍只允许 main。名称通过最新 PrivateResponse 脱敏，稳定 key / meta / cursor 不改变。前端显示策略变化关闭编辑器并清理候选和搜索缓存；旧策略迟到页使用原 query 和返回游标释放。

小窗范围编辑器打开时保存 expected_settings_revision；后台刷新不重设未保存草稿的 CAS 基线。固定起点使用明确 UTC 毫秒输入，严格校验日历与未来时刻；今日模式按已保存统计时区零点推进。冲突保留草稿并提示取消后重新打开。主窗口详情可以明确固定该会话到今日或所选范围的精确起点，提交后显示小窗，不改变主筛选或账户服务。账户额度仍属于账户范围。
