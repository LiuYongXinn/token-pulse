# IPC 与前端契约

M09h3b2b1 扩展 UnpricedCode / PriceOutcome 的合法原因 incomplete_pricing_conditions，区别“目录没有报价”与“目录有条件价格但必要条件 / 计价链路尚未完整确认”；主窗口共享 reasonNames 显示“计费条件尚未完整确认”。缺模式 / 地区时不返回 guessed Standard、金额零或虚构匹配规则。原明细 / 汇总 / 重估命令及隐私保持，Rust → TS / schema 同步；持久原因 / 固定历史与缓存 v3 见[计价专题第 12 节](price-accounting.md#12-条件未完整确认的正式未计价状态m09h3b2b1)。

M09h3b1 在 UsageEventRow 追加必需但可 null 的 request_input；已知值仅含 input_tokens（非负精确十进制字符串）及 binding（full_request / different_consumption）。unknown 返回 null，真实零返回字符串 "0"，不公开响应 ID、请求内部位置或任意 JSON。full_request 表示已保存响应向量与该事件原始可信消费完整一致，包括可空分项 / 原始报告总量；different_consumption 保留可靠请求输入，但不允许据此为部分消费选档。公开 DTO 不含实际模式 / 地区；full_request 也不表示完整条件计价。Rust / TS / schema 同步，复用原明细租约与权限，详见[计价专题第 10 节](price-accounting.md#10-公开请求输入与完整消费关联m09h3b1)。

M09h1d 增加正式 main-only `start_source_reread`，接收原 JobRequest（kind 必须 rebuild，scope 为 all / sources，request_key 幂等），返回沿用最新隐私策略的 Job；sessions 范围返回 INVALID_QUERY，其他 kind 返回 UNSUPPORTED_API。意图内部持久保存，不暴露任意文件路径 / SQL / 原生句柄。主窗能力白名单与命令注册同步，mini 没有权限。原 start_job 的保存观察重建保持；状态 / 进度 / 取消继续 get_rebuild_status / cancel_job，新增受控阶段 reading_source_files 转为“正在重读已启用来源”。实际 Win10 隔离 React / IPC 验证与源码同提交，不视为正式安装包已更新。

M09h2 PriceRule / PriceRuleDraft 新增 nullable cache_write_rate_atoms 精确原子字符串，沿用现有 main-only 价格查询 / 保存 / 历史命令和预期价格修订，不开放新的文件或通用权限。Rust → TS / schema 同步生成；空写入费率表示未知，字符串 "0" 是明确免费。用量 cache_write_input 继续单独保存覆盖，费率修改不填补事实中的 null。请求条件未知时不能以四项公式接入代替模式 / 分档证据，详见[计价专题](price-accounting.md#7-四费率估算m09h2)。

M09h1a 追加 TokenTotals.cache_write_input: TokenMeasure 及 RawUsageVector.cache_write_input: nullable 精确有符号字符串；Rust 生成 TS / schema 同步。总量不重复相加，未知不补零，隐私保留 Token 的既有规则不变。PriceRule / Draft 仍只有三费率，正数写入保持 insufficient_usage，四费率与请求条件后续接入。下条关于已发布 0.1.2 的核查保留，当前增量见[计价专题](price-accounting.md#6-缓存写入数量链路m09h1a)。

2026-10-03 计价状态澄清：规则 / 别名、离线目录和重估命令已注册并接正式 UI，Rust 生成契约为实际权威，后文早期“草案 / 未注册”按对应后续模块收敛。现有 PriceRule / PriceRuleDraft 仅含输入、命中、输出三费率，UsageVector / 公开用量尚未完整支持独立写入；请求长度、实际模式、端点地区未进入自动匹配契约。后续新增 nullable 写入分项、四费率与有出处的请求条件，连同价格缓存身份 / 历史版本同步演进；缺证据不回填假值。见[模型价格与计费条件](price-accounting.md)。

M06f4 不增加 DTO 或新命令。普通会话读取以已拥有活跃账本为发布边界；选择器排除未发布身份，直接详情 / 轮次 / 上下文及固定范围提交返回 INVALID_QUERY。父子工具内部键 / 名称与 child_count 仅包含已发布会话，已确认的 provider 父标识继续保留；真实空账本的上下文未知字段保持 null。选择器分页继续由原 SQLite 租约固定可见性，候选身份发布后只有新快照可见。

M06e2 沿用现有 Coverage DTO，不新增扫描历史接口。Complete 现在可由真实目录枚举与当前文件读取证明产生；无选中来源或缺失证明保持 Unknown，已知文件 / 格式 / 待归属缺口保持 Partial。source_issues 新增受控原因 `source_scanning`、`source_scan_interrupted`、`source_scan_changed`、`source_scan_incomplete`、`source_scan_pending`；查询在自己的 SQLite 快照验证证据，不能用最近成功时间或 ready 缓存标志替代。每个时间桶保留自身核算缺口，来源级缺口适用于所有桶，Token 分项完整性继续独立表达。DTO / TypeScript / schema 字段未变化。

2026-10-02 范围确认见[实施计划第 7 节](../development/implementation-plan.md#7-已确认的剩余功能范围2026-10-02)。不新增 manage_startup、额外快捷键、WSL / 网络来源或旧格式自动重解析命令；诊断仅保留基本状态、错误与定位。notify、计价 / 重估、账户、任务栏和更新契约继续保留。auto_connect 是软件启动后的账户连接偏好，与已取消的开机启动无关。后文已实现协议记录保留历史事实，不修改既有 schema。

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
|`query_diagnostics`|受控来源 / 类别 / 时间、cursor|简化错误列表、必要位置、重新检测与手动重建；不返回原文样本或完整继承证据|
|`get_price_rules`|revision 或当前、分页|规则版本、来源、匹配与生效时间|
|`get_offline_price_catalog`|revision 或当前；main-only|OfflinePriceCatalogSnapshot，固定价格修订的事实目录或 null|
|`get_job` / `list_jobs`|job_id / 状态与分页|持久进度与明确最终状态|
|`get_rebuild_status`|request_id；main-only|`Job|null`；当前执行中、最早排队、否则最近重建结果，只返回一条|

M10d2 注册正式 `query_diagnostics`：请求 `{ request: { source_id: string|null }, request_id }`，null 为全部，非空必须匹配已保存来源；不接受任意路径、时间 / 历史游标或原文参数。`DiagnosticsSnapshot { data_revision, issues, has_more }` 在同一只读快照选择当前保存的问题；每来源 / 位置 / 类别 / 错误码只返回一处最近代表位置，最多 20 处，额外一处仅决定 has_more。`DiagnosticIssue { issue_id, source_id|null, kind, code|null, path|null, byte_offset|null }` 的 issue_id 为受控来源类别与内部标识的 SHA-256，偏移为精确十进制；不存在的位置 / 错误码保持 null。kind 限定日志格式 / 用量记录、未确认用量、未归属累计段、缺失文件和目录核对。没有诊断原文、metadata / 继承 evidence、事件载荷或向量字段。

只选择未解决诊断和当前指针选定物理代次、活动账本的 pending / unattributed；候选、退役、未选定代次、历史账本及 inherited / duplicate 排除。缺失文件仍保留已保存事实与原位置；启用且根目录匹配的当前扫描错误使用来源根位置，不制造文件偏移。最新共享隐私在序列化时隐藏已知 path，null 仍为 null，mini 没有命令权限。正式 UI 接入来源筛选、代表位置、精确偏移和失败保留 / 旧响应丢弃；M10d1 的文件级接口待实现说明为历史阶段。

M10d1：正式诊断 UI 使用 `get_sources` 与 `get_rebuild_status`，不请求完整作业历史。重建状态在同一只读 SQLite 快照中先选择执行中作业，再选择最早 queued，均不存在时按 updated_at_ms 选最近终态；不受最近 50 条历史列表截断影响，也不返回其他 kind 的作业。null 表示没有重建记录，请求失败保持不可用 / 上次状态，不伪装成 null；复用精确十进制进度和 Job DTO，前端只展示状态、更新时间、文件 / 字节进度和可取消操作。旧 list / get 内部能力保持，mini 不获新命令权限，序列化沿用最新共享隐私策略。文件级必要错误定位仍需后续受限查询，不将设计表中的 `query_diagnostics` 当作已注册能力。

M09g1b：目录请求 `{ revision: string|null, request_id }` 使用十进制修订；只读实际 SQLite 快照固定当前或已发布历史版本，不接受超前修订。`OfflinePriceCatalogSnapshot { price_revision, catalog|null }` 的目录含 format_version、catalog_id、verified_at_ms、provider、currency、short_context_max_input、reference_basis 和 entries；条目含确切模型、处理模式、上下文档位、四种精确每百万单价及官方来源。可空单价保留 null。目录无写入 IPC，使用内嵌正式事实；共享隐私在序列化时将 catalog 清空，前端同时受显示策略与请求代次门禁。价格设置页固定 get_price_rules 返回的版本读取目录，独立失败重试不会阻止自定义规则编辑。

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

2026-10-02 用户取消 M14，快照租约仅用于交互查询与分页，不再设计导出副本作业。

## 3. 写操作与作业命令

M15a7 的正式通知设置消费者已接上述五命令，复用标准 request 身份 / 版本 / 显示策略门禁；prepare 的迟到或隐私拒绝响应会主动 release plan。关闭 / 离页 / 隐私变化释放预览，后台 plan / 文件条件与修订校验仍是最终写入依据。release 仅返回 Response<null>、无敏感数据，不要求显示戳；其响应身份仍校验。正式 UI 成功应用后重新读取真实当前状态，清理失败不丢掉已成功撤销结果。实际主 WebView按钮启用 / 停用与 mini 权限继续通过，系统目录选择器交互、真实 Codex 回合及其他卷验证未因此完成。

|命令|请求与效果|
|---|---|
|`choose_source_directory`|后端原生目录选择；返回一次性 selection_handle 和检测摘要|
|`manage_source`|add(handle)、pause、resume、detect、retain_remove；验证作用范围|
|`start_job`|import / reconcile / rebuild / price_revalue；scope 与 request_key；返回 job_id|
|`cancel_job`|job_id；只请求安全取消，返回 accepted / already_finished / too_late|
|`set_project_alias`|project_id、alias、expected_settings_revision；不修改消费|
|`save_price_rule`|完整规则、expected_price_revision；校验时间重叠、精度与优先级后发布新版本|
|`retire_price_rule`|rule_id、expected_price_revision；保留历史可追溯规则|
|`get_settings`|结构版本、revision、生效值|
|`update_settings`|受控 patch、expected_settings_revision；校验后返回生效值与冲突|
|`open_source_location`|diagnostic_id 或 source_id；后台定位，不接受任意路径|
|`get_notify_integrations`|主窗口：owner 状态、真实 nullable 监听数与登记状态；未知枚举保留 null|
|`prepare_notify_integration`|主窗口：enable_source(source_id, chain_original) / choose_home(chain_original) / disable(registration_id)；只读差异计划|
|`apply_notify_integration`|主窗口：仅 plan_id；条件修改与独立登记清理结果|
|`release_notify_preview`|主窗口：仅 plan_id，关闭内存预览；隐私开启时也可释放|
|`retire_notify_integration`|主窗口：仅 registration_id；证明已 inactive 后清理自己的登记，不写用户配置|

M15a6 已将上述五项接入实际 Rust handler、AppManifest 与 main capability；mini 不获权限，后端也验证 label=`main`。旧 `manage_notify_integration` 草案由这些具体命令替代，不开放任意程序、路径、nonce、完整通知 JSON 或配置文本输入。启用路径只能取现存未移除的本地 source_id，或后台带父窗口的系统目录选择器；不使用来源的 enabled 作为配置归属授权，不新增 WSL。选择取消返回 data=null；未知字段、无效来源、非 32 位小写十六进制 plan / registration 标识拒绝。

NotifyConfigPreview 包含 plan_id / registration_id、enable 或 disable、nullable home_path / before_notify / after_notify、creates_config、can_chain_original / chain_original、精确字符串 settings_revision、expires_in_seconds=120、redacted。路径、原 notify 与安装值在序列化时按最新共享隐私全部置 null；不返回 nonce、其他配置或正文。准备不修改配置，只有后台保留的不可变计划可 apply；计划绑定当时数据库设置修订及显示策略修订，任一变化后旧计划 StaleConfirmation 并释放，文件变化仍由完整字节 / 身份条件校验拒绝。应用与退休在共享隐私锁内检查最新策略并执行，隐私已提交时不能继续旧操作；系统目录选择等待不持有这些锁。

NotifyIntegrationsSnapshot 包含 ready、nullable listener_count / service_issue、nullable registrations / registry_issue 与 redacted；枚举失败和空列表分别表达。单条 NotifyIntegrationRow 的 configured / current_executable / chain_original 允许 null，坏记录保持自己的有限 issue，不污染健康条目；隐私只隐藏路径。NotifyApplyResult 包含 configured: boolean|null、retired、cleanup_issue，成功撤销但登记占用明确 configured=false / retired=false / cleanup_issue；单独退休已不存在登记不推断配置，configured=null / retired=true。成功操作 reload owner，读取仍是独立当前状态，不伪装监听已立即建立。

所有操作错误只传 NOTIFY_INTEGRATION_FAILED 和 details.notify_issue 的有限 NotifyIssue；Busy、配置变化、归属变化、provider 不支持、权限、过期 / 未知计划、清理失败分别表达，不回显 OS / TOML 文本或命令。当前 NTFS 事务 provider 限制见[采集设计](collection-accounting.md#8-notify-唤醒与恢复路径)。正式设置 UI、实际系统目录选择对话框、真实 Codex 回合继续验收，IPC 开放不代表 notify 完整交付。五命令实际主 WebView与 mini 权限、隐私 / 旧预览以及启用撤销已通过 Win10 合成 Home 场景，详见[交付记录](../development/delivery-status.md#m15a6主窗口-notify-ipc与共享隐私门禁)。

M15b2 的 notify 内部 CLI 不是前端 IPC：正式 exe 在 Tauri 创建前识别严格的 headless 参数，只有当前用户私有登记 ID 可选择能力，不接受通道 nonce、Home、程序或路径参数。错误输出固定有限代码，不包含原 JSON / 路径 / 参数；有效未支持事件与失效配置不创建消费。debug 原生测试仅允许 AppData 下 `native-notify-<32 hex>` 子目录名，release 排除该入口。该阶段的主端 IPC 待办已由 M15a6 五命令收敛，原命令 / owner / 文件操作分别由后续增量完成；不暴露任意命令执行或原始通知入口，正式设置 UI 继续实施。

selection_handle 绑定选择用途、当前窗口与 canonical target，5 分钟过期；消费一次，不能改成删除 / 任意执行目标。不提供 `choose_output_file`、`prepare_data_action` 或 `commit_data_action`，也不开放导出、手动备份、备份恢复与数据清除作业。账户程序选择与指纹确认继续使用独立受控契约。

对长操作 request_key 幂等：相同 key + 相同请求返回同 job；同 key 不同 payload 拒绝 REQUEST_KEY_CONFLICT。查询 request_id 只用于关联，不承诺写幂等。应用重启后作业状态可查，无法继续的任务明确 interrupted。

### 3.1 作业 DTO

```typescript
type Job = {
  job_id: Id;
  kind: 'import' | 'reconcile' | 'rebuild' | 'price_revalue';
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
|`manage_account_connection`|仅主窗口；connect / disconnect / select_limit，复用本地登录态与受控配置|
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
|DB_WRITE_FAILED / DISK_FULL|提交失败|停止推进检查点，显示存储错误与排查提示|
|DB_CORRUPT / MIGRATION_FAILED|库 / 升级不可用|显示普通错误提示，不提供恢复界面|
|SNAPSHOT_EXPIRED / CURSOR_INVALID|租约 / 游标无效|重新取第一页，说明列表已刷新|
|REVISION_CONFLICT / STALE_CONFIRMATION|配置或已确认账户程序已变化|重新读取差异，再评估操作|
|REQUEST_KEY_CONFLICT|幂等 key 复用不同请求|拒绝操作，不启动第二任务|
|PRICE_RULE_CONFLICT|同范围 / 同优先级规则有效时间重叠|保留旧规则，调整有效时间或优先级后重试|
|JOB_CANCELLED / JOB_INTERRUPTED|安全取消 / 重启中断|展示实际状态，提供可支持的恢复|
|QUOTA_DISCONNECTED / QUOTA_UNSUPPORTED|额度连接缺失 / 不支持|未知剩余，提供连接或能力说明|
|QUOTA_TIMEOUT / QUOTA_AUTH_REQUIRED|额度服务失败|保留同账户旧快照 / 引导授权|
|TASKBAR_UNSUPPORTED / TASKBAR_NO_SPACE / TASKBAR_EMBED_FAILED|宿主不可用|按偏好回退，不停止采集|
|NUMERIC_OVERFLOW|超出受支持数值域|保留诊断，不给截断 / 零结果|
|PERMISSION_DENIED|不允许窗口 / 原生调用|拒绝并脱敏记录，不升级权限|

AppError 不包含完整源记录、访问令牌、未经处理的系统错误串；message_key 由前端本地化。可安全重试的网络 / 文件失败与需要用户修改的配置错误必须分开。

## 8. 契约验证

M14 已取消，不定义 CSV / JSON 导出格式、公式注入验收、手动备份或数据恢复 / 清除契约。精确大整数、未知值、价格依据、隐私和快照一致性继续在查询 DTO 中验证。

契约检查：DTO schema round-trip、超大整数、非法区间 / 枚举 / 游标、窗口越权、序号乱序、通知丢失、快照过期、幂等 key 冲突、费用空值、额度 epoch 变化、隐私宿主断连。主页面同快照和分页租约需使用真实并发写入验证，不能只断言几个 revision 字符串相同。

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

WindowAction 增加 show_mini，由主窗口 perform_window_action 异步创建 / 显示独立 mini；托盘调用同一创建 / 恢复实现。MiniWindowAction 为 read / set_expanded { expanded } / set_pinned { pinned } / drag / hide，返回 MiniWindowState { expanded, pinned }，纯原生交互无敏感显示字段。只允许 mini；尺寸固定两组 DIP，无任意窗口标签 / 路径 / 外部 URL / 穿透参数。展开 / 置顶 / 位置现已持久化，见 2.20。

mini capability 允许 get_mini_scope / get_mini_usage / set_mini_scope / get_display_settings / set_display_privacy；后两项共享隐私协调与最新出口。主题 / 时区写入继续仅 main。小窗 frontend 采用相同显示 epoch / 延迟响应门禁、共享主题，完整 MiniUsageSnapshot 单体更新；隐藏不关闭后台采集。账户区当前明确未连接，没有本地推测的额度值。


### 2.18 明确打开同范围统计

open_mini_stats 为 mini-only，参数 MiniStatsOpenRequest { expected_settings_revision } 绑定用户正在看的范围版本，真实 mini 使用事务修订不同时返回 REVISION_CONFLICT，不发布导航。成功返回 MiniStatsRequest { request_id, mini_scope, calendar }，calendar.range 保留精确 UTC 半开毫秒边界（包含采样毫秒），heatmap_range 由相同采样时刻 / 时区日历解析。该 DTO 只有稳定 ID / 日期，无敏感名称 / 金额 / 账户字段，使用普通 Response。M13e3 将统计 / 设置导航统一到 get_main_navigation 与 main_navigation_changed；main-only get_mini_stats_request 保留兼容读取，当前最后意图不是统计时返回 null。旧 mini_stats_requested 不再发出。

主窗口按意图 ID 只应用一次；新意图明确重置来源 / 其他维度 / specified_time 并进入总览，显示精确范围替代整日日期控件。刷新与分页沿用此范围，恢复主日历或重置不回写 mini_scope；恢复可见不会重新应用已消费过的 ID。小窗数据之后更新不暗中改变已打开主统计范围，须再次点击打开。

### 2.19 已登记会话候选与明确小窗起点

query_mini_sessions 允许 main / mini，接收 MiniSessionsRequest { query: { search, page_size }, cursor }，返回 MiniSessionsPage { meta, options: [{ session_key, display_name }], next_cursor }。搜索最多 256 个 Unicode 字符，不允许控制字符；页长 1–100。候选来自全部已登记 canonical sessions，包含没有消费事件的会话，排除已经验证的镜像别名，不继承主窗口日期 / 来源筛选。参数化 Unicode 字面搜索、BINARY session_key 升序 keyset 与实际 SQLite 只读租约保证续页期间新登记 / 重命名不改变既有候选。配置时区和数据 / 价格修订也在同一事务固定；generated_at_ms 沿用首请求。

游标仍为 151 字符的已认证能力，绑定可信窗口标签 / mini_sessions 域 / 完整 query。末页、失败、取消、搜索改变和迟到响应清理租约；过期不自动混入新快照。close_query_snapshot 增加 mini_sessions 变体；mini 只能关闭该变体，其他统计查询仍只允许 main。名称通过最新 PrivateResponse 脱敏，稳定 key / meta / cursor 不改变。前端显示策略变化关闭编辑器并清理候选和搜索缓存；旧策略迟到页使用原 query 和返回游标释放。

小窗范围编辑器打开时保存 expected_settings_revision；后台刷新不重设未保存草稿的 CAS 基线。固定起点使用明确 UTC 毫秒输入，严格校验日历与未来时刻；今日模式按已保存统计时区零点推进。冲突保留草稿并提示取消后重新打开。主窗口详情可以明确固定该会话到今日或所选范围的精确起点，提交后显示小窗，不改变主筛选或账户服务。账户额度仍属于账户范围。

### 2.20 小窗原生偏好与位置恢复

现有 MiniWindowAction / State 不增加任意几何或屏幕写入能力；原生宿主采集工作区相对 DIP 偏移与可选 monitor 标识，仅在应用自有 SQLite settings payload 的 mini_window 保存 MiniWindowPreferences。每次内部修改只更新 placement / expanded / pinned 之一，读取最新 payload 并与全局 settings_revision 同事务提交，保留主题、隐私、时区和范围。同值不递增；错误 / 未支持版本不写默认配置。偏好变化发 settings_changed，小窗原生按钮成功后重读真实快照，避免沿用变化前范围 CAS 修订。

新建窗口从已保存偏好恢复两组固定 DIP 与置顶，按目标屏幕当前工作区 / 缩放换算位置；找不到原 monitor 时选择主屏，夹紧相对偏移。隐藏窗口的 Win32 中间客户区尺寸不作为新建位置计算依据；使用已确定的产品尺寸。已显示窗口展开后按实际区域校正。Moved / ScaleFactorChanged / WM_DISPLAYCHANGE 只调度一个合并 worker，250 ms 静止后采集当前位置，避免每个鼠标事件创建线程；新建 / 展开 / 隐藏 / 关闭隐藏 / 退出前同时保存最后位置。原生尺寸 / 置顶操作后 Writer 失败则撤销该操作，返回原错误，保持已确认状态。

位置 / monitor 标识不是前端 DTO，不开放任意路径、窗口标签或 native handle。实际多屏拖动 / 断屏 / 跨屏 DPI、完整进程冷启动和主窗口位置恢复仍需后续验收；自动布局预期、实际 WebView 重建与 SQLite 重开证据在交付记录分别列出。透明度、恢复快捷键、穿透尚未开放。

M15f2 增补独立主窗口 main_window 位置字段，仍不公开几何 DTO / 写入命令。原生事件记录工作区相对 DIP 与 monitor，保留最后普通位置，合并提交；最大化 / 最小化排除。初始化在首次显示之前恢复，按实际外框（含标题栏 / 边框）及目标屏工作区夹紧，原屏缺失回主屏；关闭隐藏 / 退出保存，工作区或缩放改变后重新检查。settings_changed 沿用既有全局修订，账户 / 小窗范围不改变。Win10 150% 实际三进程冷启动已通过，上述“主窗口位置恢复仍需后续”按本增量收敛；物理多屏 / 断屏 / 多档 DPI / Win11 不以合成缺屏代替。

### 2.21 已实现的恢复快捷键

RecoveryShortcut 包含 control / alt / shift 和 canonical key（A–Z、0–9、F1–F11），至少有 Ctrl 或 Alt；默认 Ctrl+Alt+Shift+T。Windows 键组合、F12、非规范 / 任意数值键拒绝，遵循 [RegisterHotKey 官方规则](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey)。RecoveryShortcutMutation 带精确 expected_settings_revision，Writer 单字段更新 recovery_shortcut 与全局修订，保留其他配置；同值不写，但可明确重试原生注册。

get_recovery_shortcut 允许 main / mini；set_recovery_shortcut 只允许 main。返回 RecoveryShortcutSnapshot { shortcut, registration, settings_revision }；registration 为 ready / conflict / unsupported / unavailable，持久配置不等于注册成功。DTO 只有控制配置与状态，使用普通 Response，无账户 / 路径 / 消费数据。未知或不可读配置报错，不伪造默认已注册。

原生注册在主 HWND 所属线程执行，使用两个自有 ID 与 MOD_NOREPEAT。更换先在空闲 ID 注册新组合，Writer 成功后释放旧 ID；冲突不写，Writer 失败释放候选、保留旧注册。启动读取保存键，冲突不终止应用；WM_HOTKEY 按自有 ID / 实际修饰键 / key 校验后异步显示小窗并解除鼠标忽略，不在 UI 消息回调创建 WebView。WM_NCDESTROY 注销自有 ID。设置页显示实际状态、修改 / 重试、CAS 冲突保留草稿与明确重置；订阅卸载与 StrictMode 清理已验证。透明度和正式穿透入口继续实施，不能把原生 probe 的鼠标忽略注入当作已交付穿透功能。

### 2.22 已实现的小窗原生透明度

get_mini_opacity 允许 main / mini，返回 MiniOpacitySnapshot { opacity_percent, supported, settings_revision }。set_mini_opacity 仅 main，MiniOpacityMutation 包含整数 opacity_percent（70–100）与精确 expected_settings_revision；默认 100，不支持平台明确返回 supported=false，读取失败不冒充已保存值。DTO 不含名称 / 金额 / HWND，属于普通控制响应。现有 MiniWindowState 不增字段。

内部 MiniWindowPreferences 增加可选兼容的 opacity_percent，只有旧配置缺少此字段才补 100；null、非整数、越界或未知字段拒绝。Writer 从最新配置窄更新透明度与全局 settings_revision，同事务提交，保留位置 / 置顶 / 展开 / 范围 / 主题 / 隐私 / 时区；同值不写，但仍校验 CAS。主设置页滑块与明确保存接实际 DTO，刷新不覆盖草稿，失败保留输入，明确重置使用最新确认值。

Windows 使用 [SetLayeredWindowAttributes 的整窗 alpha](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)，百分比四舍五入转换到 BYTE；全部 Win32 修改在 HWND 所属线程。应用创建锁序列化透明度修改与小窗显示；先应用、后 Writer CAS，提交失败恢复旧透明度。无窗口时保存偏好，下次创建应用。Tao 会在隐藏 / 显示 / 置顶等操作重建扩展样式，原生小窗 subclass 在 [WM_STYLECHANGING](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-stylechanging) 保留自有 WS_EX_LAYERED，其他样式仍交给原窗口管理；WM_NCDESTROY 移除 subclass。恢复交互入口解除 cursor-ignore 后重新应用持久 alpha。此模块不开放正式穿透；其授权与恢复校验继续实施。

### 2.23 已实现的受控鼠标穿透

get_mini_passthrough 允许 main / mini，返回 MiniPassthroughSnapshot { enabled, persisted_enabled, window_present, supported, recovery_shortcut, recovery_registration, settings_revision }。enabled 从真实 HWND 读取，persisted_enabled 是当前设置偏好，两者分开：无窗口意味着原生穿透未生效；原生读取失败报错，不返回假状态。恢复键与偏好 / 修订在同一 SQLite 读事务获取，实际注册匹配由原生线程的 RecoveryRuntime 校验。此 DTO 为不含路径 / 账户 / 金额的普通控制响应。

set_mini_passthrough 只允许 main，MiniPassthroughMutation { enabled, acknowledged_recovery, expected_settings_revision }。开启必须携带用户已明确确认的受限恢复键；关闭必须 acknowledged_recovery=null。后端重新验证全局修订、当前保存键和实际自有注册，未注册 / 冲突 / 不支持拒绝开启；不能只信前端 ready。未创建小窗时先通过正式显示动作创建，再确认开启。设置页说明鼠标将传给下方窗口，并展示实际恢复组合和托盘入口；确认绑定当时的 key / revision，刷新不重写，键变化要求重新确认。

mini_window 偏好新增 passthrough，旧配置缺字段默认 false，null / 非布尔拒绝。Writer 只改此字段与全局修订，同事务 CAS，保留所有其他配置；同值仍检查修订。创建锁串行化显示、透明度和穿透，实际鼠标忽略 / alpha 操作在 HWND 所属线程执行；恢复键锁只在原生线程持有，不由等待该线程的 worker 持有。开启先应用 native，再 Writer；提交失败撤销新穿透，异常回滚尝试恢复鼠标并返回受控错误。

关闭和恢复入口优先恢复鼠标：存储失败不会重新开启穿透。快捷键、托盘「显示悬浮窗 / 恢复交互」、正式显示动作均先解除 native ignore，再保存关闭偏好；已有小窗的坏配置 / 位置保存失败也不能阻止该恢复。透明度重设失败不阻止鼠标恢复；无可靠配置时保留当前 native alpha，记录错误，不伪造新的偏好。读取能区分已关闭但保存仍为 true，设置页允许重试保存。原生交互变化发 mini_interaction_changed 空载荷失效通知，成功持久变化另发 settings_changed；UI 重新读取实际状态，通知不携带统计值。

每次明确重新显示会关闭穿透；保存 true 不会绕过确认并自动开启新窗口。登录启动 / 自动显示偏好尚未交付，不能把本节作为完整启动恢复验收。任务栏后续复用该恢复入口，不开放任意 HWND / 原生消息 / 快捷键代码。

### 2.24 已实现的账户额度领域基础

QuotaSnapshot / QuotaWindow / QuotaLimit 沿用既有契约，新增受控错误 QUOTA_PROTOCOL_ERROR / QUOTA_SERVICE_UNAVAILABLE；M12a 仅实现后端解析和内存协调器，get_account_quota 等命令尚未注册。快照与本地 SQLite 消费 / 价格完全独立，不持久化账户额度或服务原始消息。

按照 [App Server 官方协议](https://learn.chatgpt.com/docs/app-server)，完整读取优先 rateLimitsByLimitId，null / 缺字段才兼容 rateLimits，空映射不回退。单桶 account/rateLimits/updated 按 limitId 合并而非覆盖其他桶；身份缺失通知仅能更新已证明缺少服务标识的旧版单桶，不能用本地 legacy 字符串冒充服务身份。桶标识冲突 / 非对象窗口等结构错误拒绝；未知数值 / 名称保留 null。只提取额度需要的字段，不保留邮件、订阅、credits、认证信息或未知消息内容。

普通读取令牌包含精确连接 epoch / 请求 ID；账户变化和断开清除全部缓存，未完成本 epoch 读取前拒绝通知。新通知优先于此前开始的查询回复，保留各桶自己的 fetched_at_ms；没有选定桶时 fetched_at_ms 为 null。quota_revision 为十进制字符串。服务 actor 必须在串行处理内部采样单调时间，10 秒请求期限 / 5 秒下限 / 失败退避 / 60 秒或 5 分钟轮询不受系统时间回拨影响。延迟回复只能发布、已超时或被忽略，不能恢复旧 epoch。

只按 actual window_duration_mins 识别唯一周窗口和短周期，未知时长不猜角色；reset_at_ms 从 Unix 秒 checked 转换。剩余百分比在已知 usedPercent 时计算并夹紧，缺失仍为 null；通知 / 超时 / 陈旧不改成本地推测百分比。多桶不加总，默认 codex 或唯一桶，否则留待明确选择；原选择消失不悄悄改选。实际 stdio、授权作业、IPC 权限及 UI 消费在后续模块接入。

### 2.25 已实现的独立账户 stdio 通信边界

token-pulse-quota 是独立后台库，尚未注册 WebView 命令。NativeService 不是序列化 DTO；可执行文件 / Home 后续由受控原生选择和明确用户连接操作产生，不将路径直接变成通用 shell 入口。Windows 只执行 canonical 原生 .exe，固定 app-server 参数。受限 AccountRequest 只开放 ReadAccount / ReadLimits；初始化由宿主内部执行，实验 API 关闭，成功响应之后才发送 initialized。

RpcToken 绑定 transport connection_epoch 与不可重用序列 ID，独立于账户领域快照 epoch；后续 owner 映射两者，不能用订阅或 PID 当作账户身份。ProtocolEvent 仅包含已净化的回复 / 超时 / 账户变化 / 桶通知；10 秒过期请求移除，旧回复不命中后续请求。account/updated 先清理该连接所有旧账户读取，再由领域 owner 清空快照并查询新身份。任何服务主动请求只收到受控不支持错误，未知通知不保留。

原始协议帧短暂经过有界后台管道，解析后只保留展示字段；不 Debug / 日志 / 持久化消息。原始 stderr 排空但不输出，只暴露安全字节计数。Windows 自有服务以 suspended → Job 约束 → 自有主线程恢复启动，断开回收自有进程树与所有 pipe worker；Job 失败拒绝启动。相关规则依据 [Windows Job 官方说明](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject)和 [ResumeThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-resumethread)。本节不代表授权流程或正式 get_account_quota 已可用；登录和 UI 接入继续按第 6 节设计实施。

### 2.26 已实现的持续账户服务与共享读取命令

get_account_quota / refresh_account_quota 正式注册为 main / mini 专用命令，后端再次验证窗口身份和 request_id，均返回最新 PrivateResponse。QuotaRefreshResult 包含 quota: QuotaSnapshot、status: started / in_flight / rate_limited / not_due、retry_after_ms: u32 | null。刷新不等待网络额度完成，不伪造值；单飞 / 限流返回相应受限状态，未连接 / 需要授权 / 不支持明确报错。嵌套 quota 的额度名称通过与原 QuotaSnapshot 相同的隐私脱敏，Token / 百分比 / unknown 不改变。

account_quota_changed 的 QuotaChanged 只含 connection_epoch / quota_revision / state，属于失效通知，不带账户信息或百分比。串行 owner 只在完整快照修订后发布；采集 usage_changed / 设置 settings_changed 与额度事件独立。transport RPC ID 映射到领域额度令牌 / 当时身份 epoch，通知身份变化时两层旧请求都失效。传输失败通过 connection_failed 明确标旧值陈旧；自动重连先清空新连接未证明的身份和值，不能沿用旧 epoch。

默认服务不连接外部程序；当前 WebView 没有 connect / authorize / select_limit / disconnect 命令。后续 manage_account_connection 仅 main，从原生受控选择 / 明确授权取得配置。持续服务内部已经提供 epoch 约束的连接 / 断开及 quota_revision 约束的桶选择，但内部 Rust API 不等于前端连接能力已交付。可见性位和 power flags 由原生生命周期设置，任务栏消费者接口不代表任务栏宿主完成。授权 / UI / 在线真实账户继续实施。

### 2.27 已实现的连接配置基础契约

AccountServiceConfigSnapshot 为展示配置，含 settings_revision、可空 executable_display_path / home_display_path / executable_sha256、configured 和 auto_connect；不返回内部执行目标。AccountServiceSelectionRequest / Selection 约束 executable / home / default_home 原生选择、base_selection_handle、原设置修订和期限；AccountServiceConfigMutation 只携带选择能力 / auto_connect / 预期修订。路径通过最新隐私脱敏，home null 不转成假默认路径。程序 SHA-256 是已选择文件的变更检查，不是认证信息或发布签名。

AccountConnectionRequest 为 connect（预期设置修订 / epoch / 确认指纹）、disconnect（预期 epoch）、select_limit（预期 epoch / quota_revision / 桶 ID）。严格未知字段拒绝，不含任意路径、参数、RPC、token 或登录输入；authorize 继续由后续实际登录模块定义。M12d1 仅实现契约、后台选择租约、持久 CAS 和启动指纹检查，管理命令尚未注册，不能将类型生成当作 UI / 授权完成。

内部 account_service 配置与全局 settings_revision 同事务保存，默认不配置、不自动连接。未来 / 损坏配置拒绝；旧字段缺失兼容。选择租约与来源目录分域且只允许 main，五分钟过期，16 条有界；实际启动重新验证程序指纹，Windows 文件句柄在检查与 spawn 期间阻止写入 / 删除。程序变更必须重新选择 / 确认，不继续执行旧确认的不同字节。


### 2.28 已实现的原生选择与正式连接管理

get_account_service_config / choose_account_service / cancel_account_service_selection / save_account_service_config / manage_account_connection 正式注册且仅 main；mini 只保留额度读取 / 刷新。所有后台命令再次验证窗口与 request_id。选择请求 kind 为 executable / home / default_home / current，后三者必须已有目标；current 不打开对话框但重新校验当前配置指纹，用于单独修改 auto_connect。预览路径来自原生选择并经过最新 PrivateResponse，Renderer 不提交路径。cancel_account_service_selection 仅接受独立 selectionHandle，返回普通 Response<null>，允许隐私开启后清理高熵能力；不能释放来源 / 查询能力。

选择能力五分钟有效，绑定选择时全局 settings_revision；再次选择只有成功插入新能力后才移除旧能力。原生对话框取消保留旧草稿，设置变化或程序字节改变拒绝。保存通过 CAS 窄更新账户服务偏好，失败保留旧配置与能力，成功移除能力并发送 settings_changed；保存本身不连接或替换现有账户。

connect 复核 expected_settings_revision、acknowledged_executable_sha256、expected_connection_epoch，从已保存内部目标构造 NativeService 后交由 owner；不存在任意可执行路径 / 参数 / RPC / token 接口。disconnect 只要求当前 epoch，不依赖 SQLite 可读，也不修改启动偏好；select_limit 额外要求 expected_quota_revision。管理响应是最新 PrivateResponse<QuotaSnapshot>。启动只执行已保存 auto_connect=true；配置 / 程序失效保留配置，目标验证失败发布 error / QUOTA_SERVICE_UNAVAILABLE，默认不建立外部连接。完整冷启动和在线账户仍待验收。

正式设置页获取配置与额度，不使用主筛选推导账户；选择 / 保存 / 连接由明确用户操作触发。原生私有响应迟到时隐私门禁拒绝并释放选择，当前可见缓存绑定隐私 epoch，事件属于失效提示。授权 / 取消命令继续在后续实际登录模块定义；本节取代前面历史模块中“管理命令未注册”的阶段描述。


### 2.29 本地现有登录态复用（2026-10-02 用户修订）

本节与开发总入口 §1.1 / §6.1 统一：不增加 authorize、浏览器登录、设备码登录或取消登录 IPC / 作业。manage_account_connection 保留已实现的 connect / disconnect / select_limit；connect 对用户确认的本地原生程序和 Home 建立受控服务，复用其现有登录状态。账户服务只发送 account/read（refreshToken=false）和 account/rateLimits/read，净化账户状态后才允许额度查询。TokenPulse 不获取或保存 access token、refresh token、原始认证文件或账户返回全文。

authorization_required 保留 DTO 枚举和 QUOTA_AUTH_REQUIRED 错误以兼容领域状态，正式文案为“本地登录态不可用”，引导改选已登录账户使用的 Home 并重新连接。该状态不是待实现新登录入口；它仍与 disconnected / unsupported / error / stale 分开，不伪造额度，也不阻止本地用量统计。当前及旧文档中“后续登录 / 取消”条目由此用户修订撤销；真实已有账户的额度读取、跨入口共享与冷进程启动仍按实际证据验收。


### 2.30 本地检测与共享账户显示

AccountServiceSelectionKind 新增 detect_local，继续调用 main 专属 choose_account_service；后端按本机 PATH / npm 原生包及 Home 规则读取程序元数据与 SHA 返回有期限草稿能力，不启动服务、解析认证或自动保存。已有显式 Home（含服务默认 null）保持；首次使用 CODEX_HOME / 已存在用户 .codex。保存继续重新校验并执行原设置修订 CAS。详情见[检测验收](../development/local-account-detection.md)。

总览与小窗共享 useAccountQuota，先订阅 account_quota_changed 再读取完整 QuotaSnapshot；事件只使旧缓存失效，epoch 切换 / 断开立即移除旧账户值。请求序号、生命周期、同连接精确修订和显示隐私 epoch 阻止迟到响应覆盖。可见轮询仅查询已有内存快照，用户刷新仍使用既有受限命令。无新增任意 RPC / 账户管理 / 路径权限，无账户与本地消费的联合范围请求；显示时区来自保存配置。实际周期、null、零、重置待更新及共享显示验收见[验证记录](../development/account-quota-verification.md)。

M12k：最新 display_policy.privacy=true 时，QuotaSnapshot 响应副本移除 windows，并将 fetched_at_ms / last_attempt_at_ms 置 null，已有桶名继续匿名化、未知桶名仍为 null。连接 epoch、quota_revision、状态、选桶 ID、有限错误和刷新控制结果保持；不把隐藏数据换成零或更改真实服务缓存。该规则同时用于 get_account_quota、连接管理结果、QuotaRefreshResult 和 MiniSnapshot 的嵌套额度。隐私开启前构造、开启后序列化的响应也遵循当前策略；关闭后重新查询权威缓存，不能由前端已隐藏 DTO 恢复数值。无 schema 字段变化。


### 6.1 M13a 实际宿主契约

任务栏跨进程契约权威位于 token-pulse-taskbar，从 Rust 生成独立 schemas/taskbar-host-v1.json。Envelope / HostMessage / HostReply / HostAction、TaskbarView 与 HostSession 的实际字段、64 KiB 分帧、严格空结构消息、实例 / nonce / 递增序号和隐私清空规则见[宿主协议](taskbar-host-protocol.md)。当前仅共享协议库和接收状态；第 6 节命名管道 ACL / 子进程验证 / 心跳期限 / 实际画面清除仍须在原生管理器实现，不属于已完成证据。


M13e1 扩展正式 TaskbarRuntimeSnapshot：fallback_visible 为实际回退观察 bool / null，fallback_error 为独立 ErrorCode / null。回退成功仍保留任务栏原生 unavailable / 原因，失败不能用 false 表示没有窗口。设置的 fallback_to_mini 已接正式开关和后台一次尝试策略；这类状态 / 开关响应不包含消费、费用、路径或账户标识。生成 TypeScript / protocol-v1 schema 已同步。


M13e2a 的独立宿主协议新增严格 get_actions / actions，最多 4 项受限动作及 nullable 精确配置修订；实际 UI 队列取走后才回复，不采用 unsolicited action 帧。普通刷新保持、配置 / 隐私 / 脱离清空，主端执行尚待接入；应用前端 IPC 未增加任意动作接口。权威字段 / schema 及验证边界见宿主协议。


M13e2b 扩展 TaskbarRuntimeSnapshot.action_error（nullable ErrorCode），为窗口执行失败的独立状态，下一次成功动作清除；不覆盖原生嵌入 / 回退结果。主端在受控宿主通道拉取受限意图并复用 mini_stats_requested / get_mini_stats_request，同范围统计保持原子快照与精确设置修订。mini_interaction_changed 只使交互状态失效，mini 重新调用 mini_window_action.read，并以查询序号拒绝旧响应；事件载荷不直接更新 UI。前端无新增任意动作 / 窗口 / 路径命令。
### M13e3：统一保留主窗口导航

新增 main-only get_main_navigation，参数仍为 request_id，无客户端写导航命令，返回普通 Response<MainNavigationSnapshot>。snapshot 为 { revision: DecimalInt, intent: MainNavigationIntent | null }；intent 严格标记为 mini_stats { request: MiniStatsRequest } 或 taskbar_settings {}。该 DTO 仅含稳定 ID / 日期和导航类型，不带名称 / 路径 / 费用 / 账户字段。修订是此应用实例中的单一单调域，溢出拒绝发布并保留旧意图。

mini-only open_mini_stats、原生双击 / 菜单统计和菜单设置经同一保留意图发布器递增后发送 main_navigation_changed，事件载荷不导航。主窗口先订阅再读取，以查询序号与 BigInt 修订拒绝迟到 / 低修订；重复事件和可见性恢复仅重读，不重新执行已接受意图。main-only get_mini_stats_request 保留兼容，当前最后意图不是统计则返回 null；mini_stats_requested 不再发出。

原生菜单隐私 / 隐藏不增加前端任意写接口，继续复用已有精确设置修订和发布清屏协调器。实际写失败使用独立 taskbar.action_error；自己的暂停取消不吞掉协调写入结果，退出 / 休眠仍拒绝旧结果。Rust / TypeScript / schema 同步，mini capability 不包含 get_main_navigation。

M13e4a 不增加前端命令。主端 taskbar_input 在既有 SQLite 快照中同时取得应用 theme 与隐私 / 用量 / 修订，作为独立宿主 HostDetails 投影输入；HostQuota 新增独立成功 / 尝试时间和受限错误码，账户查询仍走已有用户选择的本地服务。主题、来源成功时间、完整范围与分项完整性字段详见宿主协议，不能用查询生成时间替换来源 / 账户成功时间。隐私仍先清屏再发布，同一屏障将用于下一步原生悬停面板。

M13e4b 无新增 IPC / schema：既有 TaskbarView.details / quota 在原生宿主内生成只读面板和相同可访问全文。共享隐私的既有 Clear 屏障现在同时停止 / 隐藏详情、丢弃旧文本与布局、替换窗口名称并覆盖客户区；旧帧仍在绘制时拒绝成功清屏确认。普通快照不自动打开已隐藏面板，原生输入不形成任意客户端查询 / 写设置接口。

M13e6 不新增前端命令，既有 TaskbarPreferences.position 的 application_right 现在可由正式设置保存。独立宿主 HostConfiguration 增加严格位置枚举和旧内部配置缺省行为，schema 同步；同修订位置变化拒绝，新修订先清屏再采用新位置。只读实际按钮测量在宿主内执行，不给前端新增任意窗口 / UIA / 路径接口；配置成功不等同于 embedded，实际失败 / 回退仍分别通过运行 DTO 表达。

### M09g2b3：独立费用重估

main capability 新增 `get_price_revalue_status(requestId)`、`start_price_revalue(request: PriceRevalueRequest, requestId)`、`cancel_price_revalue(jobId, requestId)`。均返回 PrivateResponse 及最新显示策略戳；mini 无权限，处理函数也校验 main。请求 scope 使用既有 all / sources / sessions，basis 使用 event_time / specified_time 精确毫秒，expected_price_revision 与 request_key 为严格十进制修订 / 幂等标识。未知字段 / 不存在范围拒绝；新请求旧价返回 REVISION_CONFLICT，同键不同规范化载荷返回 REQUEST_KEY_CONFLICT，完全重复键返回原任务。

状态 DTO 为当前价格修订、活跃任务、最近任务及当前 event_time 缓存缺口；不提供完整历史 / 原始日志 / 名称路径 / 费用字段。任务包含固定价格版本、估价依据、queued / running / cancelling / succeeded / cancelled / failed / interrupted、十进制账本 / 记录进度、可取消状态与受限错误码。取消返回 accepted 或 already_finished，持久化取消先于 worker 信号；已发布缓存保留。`price_revalue_changed` 的空载荷只触发重新查询，不能直接填入 UI。前端保持显示策略 epoch 门禁、隐私卸载和有界串行重读，运行失败与未知值不转换为成功或零。

### M09f1：模型别名写入

主窗口专用 mutate_model_alias(request: ModelAliasMutation, expectedPriceRevision: DecimalInt, requestId)，request 为 create { draft } / replace { alias_id, draft } / retire { alias_id }，draft 包含 provider、alias、canonical_model，拒绝未知字段。返回 PriceRulesSnapshot（price_revision / rules / aliases）及最新隐私策略戳。成功提交发既有 price_rules_changed { price_revision, all_models: true }，失败不发；重复 / 链式冲突 PRICE_RULE_CONFLICT，旧价格修订 REVISION_CONFLICT，非法标识 / 非用户映射 INVALID_QUERY。mini capability 无此命令；不提供任意 SQL / 模型查询 / 路径接口。Rust / TS / schema 同步；设置编辑器随后接入，不将隐私下空数组解释为配置不存在。

M09f2 的前端 mutateModelAlias 继续复用响应身份、显示策略戳与 epoch 门禁，主窗口别名表单保持捕获的 expectedPriceRevision；刷新当前快照不悄悄改成最新写入基线。成功采用事务内完整 PriceRulesSnapshot，价格通知继续触发既有统计失效；错误保留草稿，隐私卸载并清空编辑器，关闭后重新查询。无新增 IPC 或 schema，历史 / 目录只读不是后台权限缺失。

### M16b1：更新状态与受限动作契约

Rust / TS / schema 新增 UpdatePhase、UpdateIssue、UpdateRelease、UpdateSnapshot 和 UpdateActionRequest。阶段为 unavailable / idle / checking / current / available / downloading / verifying / ready_to_install / installing / error；缺失发布配置表达 unavailable + publication_not_configured，不可当成 current。更新修订与下载字节用 DecimalInt 字符串，未开始下载的 downloaded_bytes 及未知 total_bytes 为 null，真正开始后已下载为 0 才是字符串零。成功检查时间和发布时刻独立且可未知，不在失败时刷新成功时间。

UpdateActionRequest 仅 expected_update_revision，deny_unknown_fields；网络结果与操作令牌只在 Rust 内部流转。新检查清除旧发布与进度，失败保留有限原因 / 已知进度但不能安装；迟到旧令牌被拒绝。元数据只含有界版本说明 / 日期，不返回认证、密钥、下载 URL、原生文件路径或底层错误原文；公开元数据不按用量隐私删除。该步骤仅为领域契约，未注册新的 Tauri 命令或 capability；真实网络 / 签名提供方、main-only IPC 与 UI 随后接入，不把状态机测试当作真实验签或安装证据。

### M16b3：受限安装命令

新增 main-only install_update，输入 requestId 和 request: UpdateActionRequest，响应 PrivateResponse<UpdateSnapshot>。严格拒绝未知字段、旧修订、非 ready / 无私有已驗证文件、并发安装；debug 或非 NSIS 正式安装版返回 UPDATE_UNAVAILABLE。新状态 installing 不表示安装已完成：启动器成功后请求正常退出，安装包等待旧进程结束再继续；失败通过 updates_changed 失效通知重读 Error / installer_unavailable 或 install_failed。前端不能传安装文件、启动参数或成功标志，mini / 通用插件权限继续关闭。

### M16b2：真实提供方与主窗口命令

|命令|输入|成功响应|权限|
|---|---|---|---|
|get_update_status|requestId|PrivateResponse<UpdateSnapshot>|main|
|check_for_updates|requestId|开始后的 Checking 快照；最终状态重新查询|main|
|download_update|requestId、request: UpdateActionRequest|开始后的 Downloading 快照；最终状态重新查询|main|

配置不可用返回 UPDATE_UNAVAILABLE，并可通过查询取得 unavailable + publication_not_configured；旧下载修订返回 REVISION_CONFLICT，已有操作返回 UPDATE_BUSY。非法 requestId 返回 INVALID_QUERY，未知请求字段反序列化拒绝。updates_changed 为空失效事件；前端必须重读，不从事件推定验签成功。元数据属于公开发布信息，但响应仍带最新共享隐私 stamp。mini 不登记以上权限，未授予通用 updater 插件命令权限。真实下载 / 签名 / 签名版本绑定在原生执行，ready 仅由实际成功的提供方生成；此阶段没有 install_update 命令。
