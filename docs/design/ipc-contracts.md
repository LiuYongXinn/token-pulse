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
