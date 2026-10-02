# 数据与存储详细设计

M10d2 不增加 schema：基础诊断只读现有 diagnostics、当前文件映射、活动账本 pending_usage 和 source_scan_state，同事务捕获 data_revision。未解决日志诊断只定位当前指针选定的 current 代次，NULL 代次没有文件位置；当前账本未确认 / 未归属观察按原代次精确偏移定位。退役 / 候选 / 无效 / 未选定代次及历史账本不会被误标到新位置，移动后使用同一代次的当前映射。缺失文件保留原保存位置及历史事实；目录错误仅使用启用且根仍匹配的当前扫描，未知偏移保持 null。按来源、位置、类别、码聚合一处最近代表项，固定排序并最多取 21 行（返回 20 行和 has_more），不将完整日志诊断历史 / metadata / 向量 / evidence 暴露给 UI。

M06f8 增加候选物理身份的位置迁移，不改变 schema。来源内按实际物理 identity 检索 reading / ready / claimed / failed 候选，原冻结文件、当前指针及路径仍须匹配；同一身份对应多个逻辑文件时拒绝合并。Writer 检查原路径、候选检查点修订、身份及来源 rollout 边界，目标已被其他逻辑文件占用则拒绝。reading / ready 仅更新冻结路径并递增候选检查点修订，保留观察暂存、上下文、锚点和游标；原活动检查点、账本及全局修订不变。

claimed 迁移在同一事务中将所属作业及整组候选账本 / 文件候选标记失败，再更新逻辑文件位置，冻结 manifest 保留原值；后续创建新候选重新读取，不复用旧作业。已失败候选仅用于识别原逻辑文件，旧进度不被重新激活。数据库失败回滚作业收尾与位置更新；所有迁移撤销该文件目录证明并要求重新扫描。物理路径消失和新路径实际身份由只读采集层验证。7 项新存储检查及 Win10 后台组合验证见交付记录。下述各阶段按 M06f8 / M06f7 收敛，不能把历史未接调度说明当作当前状态。

M06f7 正常采集已接替换流水线。存储提供真实快照的 `file_has_frozen_rebuild`，以活动作业 manifest 的 file_id 判断整组依赖输入等待；候选 claimed 与冻结依赖均由所属作业发布 / 失败后释放，普通后台读取不会在期间推进原输入。结构未封存仅保持候选进度和 correction_pending；正式发布后普通读取使用新代次重新确认 source_scan，达到实际 EOF 才恢复 complete。M06f6 下述尚未接正常调度为该提交历史阶段。

M06f6 schema v10 增加 `rebuild_manifests`，无会话 / 无账本的替换也能冻结输入，不制造占位身份。manifest v2 保存原文件冻结值、封存新代次及登记游标 / 完成、proposed header、ReaderContext；旧账本指针允许 null。闭包在普通已发布身份之外仅加入本作业 header，覆盖旧 / 新父关系、镜像及既有别名；回放移除被替换旧代次，保留其余当前输入。阶段写入、回放、验证和发布检查同一清单、作业账本集合与活动指针。旧 v1 清单继续兼容，不做全历史 parser 自动重解析。

核算分类及实际物理前缀验证通过后，同一 Writer 事务发布整组账本与身份、旧代次 retired / 新代次 current、文件 identity / 指针、candidate published、必要诊断 / 旧诊断失效、审计和作业成功；data revision 仅增一次，价格与设置不变。失败保留整组旧状态及旧读快照。原 canonical 文件被替换时，已发布 mirror provenance 也作为可信历史依据；冲突新副本隔离。结构 EOF 不恢复目录证明，发布清空单文件确认并保持 incomplete。下述 M06f5 未接 manifest / 发布为历史阶段；正常 CollectorService 自动发现 / 继续 / 排队仍待 M06f7。

M06f5 schema v9 增加 `file_rebuild_candidates` 与 `file_rebuild_sessions`：作业独占关联一个封存代次，冻结候选检查点修订，保存独立登记游标 / 完成标志和 proposed header 身份。排队 / 认领在同一 Writer 事务，精确重复请求复用作业，认领失败不能留下未关联队列项。登记每批最多 128 条 / 16 MiB（包含规范载荷、标识与指纹），复用普通观察写入 / 项目规范化逻辑，必要观察、绑定、NULL 活跃指针的新会话和登记游标一起提交；旧会话身份与活跃账本、消费、基线、文件指针 / 检查点、data / price / settings revision 不变。新的 proposed header 单独保存，不能提前改写已发布会话。取消 / 失败 / 启动中断同事务释放所属 claimed 并标记新代次 invalid；读取方不能撤销作业所有权。替换专用 manifest / 最终验证发布尚未接入，普通 manifest 和成功推进拒绝忽略已认领候选。

M06f4 明确会话发布门禁：`sessions.active_ledger_id IS NULL` 的暂存身份不属于普通查询 / 选择 / 手动与自动重建依赖组。详情、轮次、上下文直接读取拒绝该身份；父子关系与小窗固定范围不暴露或选中该身份。已发布的空账本会话仍可读取及选择，缺失用量 / 上下文继续为 null。普通采集的已读用量判定仅检查当前文件代次，自动重建的完整性及证据指纹复用 M06f3 的当前输入选择。替换作业后续须显式关联并授权自己的暂存身份，不能放宽普通入口来隐式纳入候选。

M06f3 收紧普通重建的物理输入：`file_session_bindings` 只表示历史证据，不能授权所有已绑定代次参与核算。准备与 freshness 校验仅选择 source_files 当前指针指向且 state=current 的代次；回放、规范序列规划、完整分类检查和镜像发布均限定在冻结清单内。retired / invalid / 未发布 candidate / 未被指针选择的 current 不参与重算，也不被镜像发布改写观察键、绑定、上下文或检查点。暂停或缺失来源的当前代次继续用于保存的历史。该增量尚未加入 replacement 候选的显式输入与原子发布。

M06f2 增加当前候选查找、冻结输入验证和 ready → reading 的 CAS 重开 API；重开不重置偏移或候选检查点。claimed 阶段不允许读方重开或失败撤销。collector 已调用这些接口进行实际只读候选读取 / 有界继续；正常调度、重建输入选择及文件 / 账本发布尚未接入。候选结构 EOF 与最终统计发布保持分开，追加可扩展同一候选，已读内容或身份失配须作废并另建代次。

M06f1 schema v8 增加替换文件候选的独立必要观察 / 诊断暂存。`file_read_candidates.base_json` 冻结旧文件 / 来源输入；候选使用 `file_generations.state='candidate'` 和独立 ReaderContext / 锚点 / 偏移 / 修订。阶段写入验证冻结输入及候选 CAS，每批最多 500 条观察 / 500 条诊断、16 MiB 必要载荷；观察、诊断与候选读取进度在同一 Writer 事务提交。候选观察不写活跃 observations，不创建新会话或项目；创建仅使文件覆盖 correction_pending 及撤销单文件扫描确认，旧消费 / 指针 / 检查点和全局修订不变。同一文件一个有效所有者，精确重复复用进度，失效 / 失败释放所有权。结构 EOF ready 不能替代物理验证和账本候选验证；实际文件 / 账本原子切换继续实施，尚未接正常采集。

M06e2 已接调度和覆盖查询。发布与读取使用同一个 `BAD_ENTRY` 判定，读取不能仅信任 ready 标志：在本次 SQLite 快照验证来源根目录、启用 / 可读状态及文件当前指针、路径、代次、检查点和 EOF。待读文件包含当前枚举中尚未注册或未确认的路径，与已登记缺口按来源 / file_id 去重。旧读快照继续保留整组旧证明；新扫描删除旧条目及更新代次在同一事务中提交。扫描状态及来源文件存在状态只改变采集覆盖，不修改 Token、价格或设置修订，也不推进检查点。M06e1 中“尚未接调度 / 查询”为该提交的历史阶段描述。

M06e1 追加 schema v7 的 `source_scan_state` / `source_scan_files`，保存当前目录扫描代次、来源根目录、枚举完成 / 中断 / 错误及每个路径的实际代次、检查点修订和已读上界。每批最多登记 128 个文件；开启新扫描、登记、确认和完成均通过 Writer 事务。完成要求枚举结束且所有文件的当前指针、路径、代次、检查点和 EOF 一致；未读完的末行、扫描错误、未知成员变化不能视为完整。旧扫描回调和已暂停 / 改根来源拒绝发布；旧 SQLite 快照保留旧证据。完整枚举可标记旧文件缺失，历史消费不删除。此提交仅落地存储 API，调度与查询接入继续实施，不能据此宣称来源完整。

2026-10-02 [确认范围](../development/implementation-plan.md#7-已确认的剩余功能范围2026-10-02)：持久费用缓存、后台重估、模型别名与完整离线价格目录继续实施；诊断只保留错误与必要定位，不新增原文样本或完整历史浏览器。旧格式自动重解析、macOS / WSL / 网络来源适配、开机启动与额外快捷键取消；已完成内部结构和历史 migration 不修改。

配套总入口：[详细开发设计](development-design.md)。本文定义目标 schema v1 的实体、DDL 草案和事务约束；不是已经存在的数据库迁移。正式落地将下列结构拆为版本化迁移，并用本专题场景验证。

## 1. 存储边界与数值

- SQLite 放在 Tauri 当前用户本地应用数据目录，文件名 `token-pulse.db`；开发应用使用独立应用 ID 与数据目录。
- 一个专用写线程持有写连接；初始普通只读池 2 个连接，分页租约另允许最多 2 个连接，有界等待。所有连接启用外键、busy timeout，并注册相同聚合函数。
- 写连接初始化 `journal_mode=WAL`、`synchronous=FULL`、`foreign_keys=ON`、`busy_timeout=5000`。只读连接打开只读模式并使用查询事务。
- 不将数据库放 UNC / 网络共享；WAL 使用共享内存并依赖同机连接，源日志读取与数据库位置分别处理。[SQLite WAL 文档](https://www.sqlite.org/wal.html)
- 时间保存 UTC Unix 毫秒整数；缺失事件时间保留 null，不能用导入时间或文件 mtime 冒充消耗时间。额度重置适配器将秒统一转换为毫秒。
- 单条原始 Token 为非负 i64；缺失为 SQL NULL，不能填 0。Rust 聚合 checked i128，IPC 使用十进制字符串。
- 质量区分 `confirmed`、`pending`、`inherited`、`duplicate`、`unattributed`。只有已确认且可定位事件进入日期统计；其他保留独立诊断和必要金额 / 向量信息。

金额不使用 FLOAT。价格每百万 Token，允许最多 9 位小数，取 `rate_atoms = price_per_million × 10^9`；费用以 `10^-15` 币种单位保存：

```text
cost_atoms = noncached_tokens × input_rate_atoms
           + cached_tokens × cached_rate_atoms
           + output_tokens × output_rate_atoms
cost_decimal = cost_atoms / 10^15
```

价格限制在 `[0, 1000000]` / 百万 Token，所有乘加 checked i128；单价、费用整数以十进制 TEXT 存储。展示时最后统一四舍五入到所需精度，不能逐事件先舍入再累计。不同币种分别返回，不自动换汇。

在查询层注册 `sum_token_decimal(INTEGER)` 与 `sum_money_atoms(TEXT)`，使用 checked i128 返回十进制字符串。不得用 SQLite `REAL` 或 JavaScript Number 替代；超过受支持范围返回 `NUMERIC_OVERFLOW`，不截断或显示零。SQL DDL 不依赖这两个函数，读查询需要注册。

## 2. 身份、版本与来源证据

|身份 / 版本|含义|
|---|---|
|source_id|一个配置目录；暂停与移除不改变事件身份|
|file_id|物理文件位置实体，路径可变化|
|file_generation_id|一次可验证的物理内容代次；偏移只在该代次内有效|
|session_key|本工具逻辑会话键；provider session ID 冲突时保持两个候选|
|ledger_id|会话的派生账本版本；会话只指向一个活跃版本|
|observation_id|必要规范观察；`file_generation_id + byte_offset` 为物理唯一位置|
|event_id|某账本中的规范消费事件；重建不要求复用旧 event_id|
|semantic_key|仅在适配器证明稳定事件身份时生成；推断指纹不能充当此键|
|data_revision|已发布账本与统计归属变更序号|
|price_revision / settings_revision|规则、配置各自的版本|

“文件代次”和“账本版本”不可混为一列。一个会话可以来自多份镜像文件；一个物理文件也可能含不止一个逻辑会话的记录。镜像文件的观察通过 `event_provenance` 指向同一个规范事件，避免全来源合计重复。

来源筛选采用 `EXISTS` 判断事件证据，不 JOIN 后直接 SUM。单独查看两个镜像来源可能都包含同一个事件，其来源小计不是互斥分组；UI 不显示可相加的来源占比饼图。暂停或停止采集来源仍保留历史、来源证据及共同事件可查询，不提供清除该来源数据的产品操作。

## 3. 核心建表草案

以下 DDL 使用应用生成的不透明字符串 ID。枚举、JSON 结构、i128 TEXT 与路径规范化由领域校验和数据库约束共同保证。跨表同会话关系必须检查，不能只因为 ID 存在就接受批次。

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE app_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  schema_version INTEGER NOT NULL,
  data_revision INTEGER NOT NULL DEFAULT 0 CHECK (data_revision >= 0),
  price_revision INTEGER NOT NULL DEFAULT 0 CHECK (price_revision >= 0),
  settings_revision INTEGER NOT NULL DEFAULT 0 CHECK (settings_revision >= 0)
);
INSERT INTO app_state(singleton, schema_version) VALUES (1, 1);

CREATE TABLE settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  settings_version INTEGER NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE sources (
  source_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  root_path TEXT NOT NULL,
  directory_identity TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('local','wsl','mirror')),
  enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
  retained INTEGER NOT NULL DEFAULT 1 CHECK (retained IN (0,1)),
  readability TEXT NOT NULL,
  capabilities_json TEXT NOT NULL CHECK (json_valid(capabilities_json)),
  created_at_ms INTEGER NOT NULL,
  last_scan_at_ms INTEGER,
  last_success_at_ms INTEGER
);
CREATE UNIQUE INDEX sources_physical_directory
  ON sources(provider, directory_identity) WHERE directory_identity IS NOT NULL;

CREATE TABLE projects (
  project_id TEXT PRIMARY KEY,
  canonical_cwd TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  user_alias TEXT,
  normalization_version INTEGER NOT NULL
);

CREATE TABLE sessions (
  session_key TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_session_id TEXT,
  identity_status TEXT NOT NULL,
  parent_key TEXT REFERENCES sessions(session_key),
  parent_provider_id TEXT,
  created_at_ms INTEGER,
  last_activity_ms INTEGER,
  active_ledger_id TEXT,
  FOREIGN KEY(session_key, active_ledger_id)
    REFERENCES ledger_generations(session_key, ledger_id)
);
CREATE INDEX sessions_provider_id ON sessions(provider, provider_session_id);

CREATE TABLE ledger_generations (
  ledger_id TEXT PRIMARY KEY,
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  state TEXT NOT NULL CHECK (state IN ('candidate','active','retired','failed')),
  parser_version TEXT NOT NULL,
  accounting_version TEXT NOT NULL,
  base_data_revision INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  activated_at_ms INTEGER,
  input_manifest_json TEXT NOT NULL CHECK (json_valid(input_manifest_json)),
  UNIQUE(session_key, ledger_id)
);
CREATE UNIQUE INDEX ledger_one_active
  ON ledger_generations(session_key) WHERE state = 'active';

CREATE TABLE source_files (
  file_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  canonical_path TEXT NOT NULL,
  file_identity TEXT,
  current_generation_id TEXT,
  status TEXT NOT NULL,
  last_seen_at_ms INTEGER,
  FOREIGN KEY(file_id, current_generation_id)
    REFERENCES file_generations(file_id, file_generation_id),
  UNIQUE(source_id, canonical_path)
);

CREATE TABLE file_generations (
  file_generation_id TEXT PRIMARY KEY,
  file_id TEXT NOT NULL REFERENCES source_files(file_id),
  state TEXT NOT NULL CHECK (state IN ('current','candidate','retired','invalid')),
  identity_json TEXT NOT NULL CHECK (json_valid(identity_json)),
  observed_size INTEGER NOT NULL CHECK (observed_size >= 0),
  committed_offset INTEGER NOT NULL DEFAULT 0 CHECK (committed_offset >= 0),
  checkpoint_revision INTEGER NOT NULL DEFAULT 0,
  anchor_json TEXT NOT NULL CHECK (json_valid(anchor_json)),
  reader_context_json TEXT NOT NULL CHECK (json_valid(reader_context_json)),
  parser_version TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  UNIQUE(file_id, file_generation_id)
);

CREATE TABLE observations (
  observation_id TEXT PRIMARY KEY,
  file_generation_id TEXT NOT NULL REFERENCES file_generations(file_generation_id),
  byte_offset INTEGER NOT NULL CHECK (byte_offset >= 0),
  byte_end INTEGER NOT NULL CHECK (byte_end > byte_offset),
  session_key TEXT REFERENCES sessions(session_key),
  kind TEXT NOT NULL CHECK (kind IN ('session_meta','turn_meta','usage','context')),
  observed_at_ms INTEGER,
  stable_record_id TEXT,
  turn_id TEXT,
  stream_hint TEXT,
  model TEXT,
  project_id TEXT REFERENCES projects(project_id),
  normalized_json TEXT NOT NULL CHECK (json_valid(normalized_json)),
  payload_fingerprint TEXT NOT NULL,
  format_version TEXT NOT NULL,
  UNIQUE(file_generation_id, byte_offset)
);
CREATE INDEX observations_session_order
  ON observations(session_key, observed_at_ms, file_generation_id, byte_offset);
CREATE INDEX observations_fingerprint ON observations(session_key, payload_fingerprint);

CREATE TABLE stream_states (
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  stream_key TEXT NOT NULL,
  episode_id TEXT NOT NULL,
  baseline_json TEXT NOT NULL CHECK (json_valid(baseline_json)),
  last_observation_id TEXT REFERENCES observations(observation_id),
  lineage_quality TEXT NOT NULL,
  state_revision INTEGER NOT NULL,
  PRIMARY KEY(ledger_id, stream_key, episode_id)
);

CREATE TABLE usage_events (
  event_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  origin_observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  occurred_at_ms INTEGER NOT NULL,
  semantic_key TEXT,
  episode_id TEXT NOT NULL,
  model TEXT,
  project_id TEXT REFERENCES projects(project_id),
  turn_id TEXT,
  input_tokens_total INTEGER CHECK (input_tokens_total >= 0),
  cached_input_tokens INTEGER CHECK (cached_input_tokens >= 0),
  output_tokens_total INTEGER CHECK (output_tokens_total >= 0),
  reasoning_output_tokens INTEGER CHECK (reasoning_output_tokens >= 0),
  source_total_tokens INTEGER CHECK (source_total_tokens >= 0),
  total_tokens INTEGER NOT NULL CHECK (total_tokens >= 0),
  calculation_method TEXT NOT NULL,
  quality_json TEXT NOT NULL CHECK (json_valid(quality_json)),
  CHECK (cached_input_tokens IS NULL OR input_tokens_total IS NULL
         OR cached_input_tokens <= input_tokens_total),
  CHECK (reasoning_output_tokens IS NULL OR output_tokens_total IS NULL
         OR reasoning_output_tokens <= output_tokens_total),
  CHECK (input_tokens_total IS NULL OR output_tokens_total IS NULL
         OR total_tokens = input_tokens_total + output_tokens_total),
  CHECK (source_total_tokens IS NULL OR source_total_tokens = total_tokens),
  UNIQUE(ledger_id, origin_observation_id)
);
CREATE UNIQUE INDEX events_stable_identity
  ON usage_events(ledger_id, semantic_key) WHERE semantic_key IS NOT NULL;
CREATE INDEX events_time ON usage_events(occurred_at_ms, ledger_id);
CREATE INDEX events_ledger_time ON usage_events(ledger_id, occurred_at_ms, event_id);
CREATE INDEX events_model_time ON usage_events(model, occurred_at_ms);
CREATE INDEX events_project_time ON usage_events(project_id, occurred_at_ms);

CREATE TABLE event_provenance (
  event_id TEXT NOT NULL REFERENCES usage_events(event_id) ON DELETE CASCADE,
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  relation TEXT NOT NULL CHECK (relation IN ('origin','mirror','replay')),
  PRIMARY KEY(event_id, observation_id)
);
CREATE INDEX provenance_observation ON event_provenance(observation_id, event_id);

CREATE TABLE pending_usage (
  pending_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  kind TEXT NOT NULL CHECK (kind IN ('pending','inherited','duplicate','unattributed')),
  reason_code TEXT NOT NULL,
  vector_json TEXT CHECK (vector_json IS NULL OR json_valid(vector_json)),
  evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
  UNIQUE(ledger_id, observation_id, kind)
);

CREATE TABLE context_snapshots (
  context_id TEXT PRIMARY KEY,
  ledger_id TEXT NOT NULL REFERENCES ledger_generations(ledger_id),
  observation_id TEXT NOT NULL REFERENCES observations(observation_id),
  observed_at_ms INTEGER NOT NULL,
  model TEXT,
  context_tokens INTEGER CHECK (context_tokens >= 0),
  model_context_window INTEGER CHECK (model_context_window > 0),
  usage_json TEXT NOT NULL CHECK (json_valid(usage_json)),
  quality_json TEXT NOT NULL CHECK (json_valid(quality_json))
);
CREATE INDEX context_latest ON context_snapshots(ledger_id, observed_at_ms DESC);

CREATE VIEW active_usage_events AS
  SELECT e.*, s.session_key
  FROM usage_events e JOIN sessions s ON s.active_ledger_id = e.ledger_id;
```

`normalized_json` 只允许对应 kind 的白名单字段：用量向量、必要元数据和稳定身份信息，不接收源 JSON 原文、消息数组或任意属性。session_meta / turn_meta 也必须保留必要规范观察，才能在源日志缺失后重放事件时刻的模型、cwd 与父关系。

source_total 不一致、缓存大于输入等观察留在 observations / pending_usage，不插入违反约束的 usage_events。零增量重复快照不需要插入零消费事件；真实零日由成功核对与覆盖状态确定。

### 3.1 价格、作业与维护表

```sql
CREATE TABLE price_rules (
  rule_id TEXT PRIMARY KEY,
  introduced_revision INTEGER NOT NULL,
  retired_revision INTEGER,
  provider TEXT NOT NULL,
  model_exact TEXT NOT NULL,
  source_id TEXT REFERENCES sources(source_id),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  effective_from_ms INTEGER NOT NULL,
  effective_to_ms INTEGER,
  priority INTEGER NOT NULL,
  input_rate_atoms TEXT NOT NULL,
  cached_rate_atoms TEXT,
  output_rate_atoms TEXT NOT NULL,
  origin TEXT NOT NULL,
  origin_reference TEXT,
  created_at_ms INTEGER NOT NULL,
  CHECK (effective_to_ms IS NULL OR effective_to_ms > effective_from_ms)
);
CREATE INDEX prices_lookup ON price_rules(provider, model_exact, effective_from_ms);

CREATE TABLE model_aliases (
  alias_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  alias TEXT NOT NULL,
  canonical_model TEXT NOT NULL,
  introduced_revision INTEGER NOT NULL,
  retired_revision INTEGER
);

CREATE TABLE valuation_sets (
  valuation_set_id TEXT PRIMARY KEY,
  price_revision INTEGER NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('event_time','specified_time')),
  specified_at_ms INTEGER,
  state TEXT NOT NULL CHECK (state IN ('building','ready','retired','failed')),
  created_at_ms INTEGER NOT NULL,
  CHECK ((mode = 'event_time' AND specified_at_ms IS NULL)
      OR (mode = 'specified_time' AND specified_at_ms IS NOT NULL))
);

CREATE TABLE event_valuations (
  valuation_set_id TEXT NOT NULL REFERENCES valuation_sets(valuation_set_id),
  event_id TEXT NOT NULL REFERENCES usage_events(event_id) ON DELETE CASCADE,
  rule_id TEXT REFERENCES price_rules(rule_id),
  currency TEXT,
  cost_atoms TEXT,
  status TEXT NOT NULL CHECK (status IN
    ('priced','unknown_model','missing_rule','ambiguous_rule','insufficient_usage','overflow')),
  PRIMARY KEY(valuation_set_id, event_id),
  CHECK ((status = 'priced' AND cost_atoms IS NOT NULL AND currency IS NOT NULL)
      OR (status != 'priced' AND cost_atoms IS NULL))
);

CREATE TABLE jobs (
  job_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  request_key TEXT UNIQUE,
  scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
  progress_json TEXT NOT NULL CHECK (json_valid(progress_json)),
  resume_json TEXT NOT NULL CHECK (json_valid(resume_json)),
  cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0,1)),
  error_code TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE INDEX jobs_state ON jobs(state, updated_at_ms);

CREATE TABLE diagnostics (
  diagnostic_id TEXT PRIMARY KEY,
  source_id TEXT REFERENCES sources(source_id),
  file_generation_id TEXT REFERENCES file_generations(file_generation_id),
  byte_offset INTEGER,
  session_key TEXT REFERENCES sessions(session_key),
  code TEXT NOT NULL,
  severity TEXT NOT NULL,
  metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
  dedup_key TEXT NOT NULL UNIQUE,
  occurrences INTEGER NOT NULL DEFAULT 1,
  first_seen_at_ms INTEGER NOT NULL,
  last_seen_at_ms INTEGER NOT NULL,
  resolved_at_ms INTEGER
);

CREATE TABLE rebuild_audits (
  audit_id TEXT PRIMARY KEY,
  job_id TEXT REFERENCES jobs(job_id),
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  old_ledger_id TEXT REFERENCES ledger_generations(ledger_id),
  new_ledger_id TEXT REFERENCES ledger_generations(ledger_id),
  reason TEXT NOT NULL,
  difference_json TEXT NOT NULL CHECK (json_valid(difference_json)),
  committed_data_revision INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL
);

CREATE TABLE source_scan_runs (
  scan_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  job_id TEXT REFERENCES jobs(job_id),
  scope TEXT NOT NULL CHECK (scope IN ('recent','full','manual')),
  state TEXT NOT NULL CHECK (state IN ('running','complete','partial','failed','interrupted')),
  discovery_complete INTEGER NOT NULL DEFAULT 0 CHECK (discovery_complete IN (0,1)),
  discovered_files INTEGER NOT NULL DEFAULT 0 CHECK (discovered_files >= 0),
  processed_files INTEGER NOT NULL DEFAULT 0 CHECK (processed_files >= 0),
  unreadable_files INTEGER NOT NULL DEFAULT 0 CHECK (unreadable_files >= 0),
  started_at_ms INTEGER NOT NULL,
  finished_at_ms INTEGER,
  manifest_hash TEXT
);
CREATE INDEX scans_source_time ON source_scan_runs(source_id, started_at_ms DESC);

CREATE TABLE scan_file_entries (
  scan_id TEXT NOT NULL REFERENCES source_scan_runs(scan_id) ON DELETE CASCADE,
  file_id TEXT NOT NULL REFERENCES source_files(file_id),
  file_generation_id TEXT REFERENCES file_generations(file_generation_id),
  state TEXT NOT NULL CHECK (state IN ('discovered','processed','unreadable','changed','missing')),
  target_size INTEGER CHECK (target_size >= 0),
  error_code TEXT,
  PRIMARY KEY(scan_id, file_id)
);

CREATE TABLE file_session_bindings (
  file_generation_id TEXT NOT NULL REFERENCES file_generations(file_generation_id),
  session_key TEXT NOT NULL REFERENCES sessions(session_key),
  first_offset INTEGER NOT NULL CHECK (first_offset >= 0),
  identity_evidence TEXT NOT NULL,
  PRIMARY KEY(file_generation_id, session_key)
);

CREATE TABLE notify_integrations (
  integration_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES sources(source_id),
  config_path TEXT NOT NULL,
  original_notify_json TEXT CHECK (original_notify_json IS NULL OR json_valid(original_notify_json)),
  owned_notify_json TEXT NOT NULL CHECK (json_valid(owned_notify_json)),
  current_value_hash TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('prepared','applied','conflict','restored')),
  updated_at_ms INTEGER NOT NULL
);
```

`source_scan_runs` 与逐文件 manifest 保存完整 / 部分扫描及错误；`file_session_bindings` 保存文件代次中的会话映射；`notify_integrations` 只保存恢复 notify 所需命令数组，不保存整个 Codex 配置文件。账户连接设置在 settings 中，只含连接描述与受控可执行文件路径；额度快照默认内存，不建立认证表。

不将 `jobs.resume_json` 变成无法校验的任意状态存储。coverage 必须依据扫描进度 / manifest、文件代次和待确认观察计算；缺乏完整来源记录时不能返回 `complete`。无法确定错误文件的时间范围时，将所选来源整体标 partial / unknown，不能因为时间戳缺失而忽略覆盖缺口。

## 4. 正常提交与并发冲突

ReadTask 开始取得 file generation、committed_offset、checkpoint_revision、active ledger 与流 state_revision。解析和核算不占写事务；生成的批次包含这些期望值。

```text
BEGIN IMMEDIATE
  检查 file generation / offset / checkpoint_revision 与预期一致
  检查会话 active ledger 与流 state_revision 未发生变化
  按物理唯一键插入必要观察
  插入已确认事件、来源证据、待确认项与上下文
  更新本批累计状态与 reader_context
  更新完整行结束偏移、内容锚点、checkpoint_revision
  更新元数据与覆盖信息
  若发布结果变化，递增 data_revision
COMMIT
返回 CommitReceipt，事务成功后通知
```

预期不一致返回 `CHECKPOINT_CONFLICT`，丢弃本批派生输出后重读当前持久状态；不能简单重试旧事件插入。因镜像关系新增而不改变 Token 的事务仍可能改变来源筛选结果，应增加 data_revision。

只读了无统计意义记录时仍可提交偏移，但不必广播 usage_changed；必要来源 / 作业状态使用独立状态事件。完整损坏行登记诊断并推进到下一行；I/O 或数据库失败不推进本批成功偏移。

单批上限初始 500 条完整记录 / 200 ms 准备时间，并设 16 MiB 必要数据总上限；达到任一限制安全提交并重新排队。具体取值由压测调整，不使一条超限记录阻塞事务。

## 5. 重建发布

1. 创建 candidate ledger、输入 manifest（物理代次与固定读取上界、父版本、解析 / 核算版本）。
2. 分批持久候选观察、事件、流状态；sessions.active_ledger_id 不变。
3. 校验序列完整性、重复 / 继承依据、Token 包含关系、事件数与质量分布，记录可解释差异。
4. 获取受影响会话锁，暂停其新实时提交，校验输入未被重写，有限追平当前完整记录上界。
5. 写事务先将旧版本标 retired、新版本 active，再更新会话指针、对应有效检查点、基线、audit 与 data_revision。
6. 释放锁，对切换上界之后的追加重新入队；失效旧查询缓存、价格缓存按 event_id 重建。

仅价格修改不创建新 ledger；仅别名修改不创建消费事件。会话或元数据纠正需要来源证据一起迁移，避免新账本事件缺少来源映射。

父观察前缀修订影响继承判定时，计算依赖子会话闭包。依赖版本一致的一组账本先全部建好，再事务发布；过大闭包可分组但必须将未完成子会话标为 lineage_pending，不能默默宣称完整。

retired 版本保留至快照租约结束及审计期限后，再后台清理；对应原始规范观察按保留策略决定，默认保留。FK 使用受控删除顺序，不通过禁用外键绕过失败。

## 6. 查询、快照与分页

所有消费查询以 `active_usage_events` 为事实视图。筛选采用 UTC `[start_ms,end_ms)`，项目 / 模型未知使用显式 null 分类。SQL 参数绑定，列名和排序来自枚举白名单。

来源过滤示例：

```sql
SELECT e.* FROM active_usage_events e
WHERE e.occurred_at_ms >= :start_ms AND e.occurred_at_ms < :end_ms
  AND EXISTS (
    SELECT 1 FROM event_provenance p
    JOIN observations o ON o.observation_id = p.observation_id
    JOIN file_generations g ON g.file_generation_id = o.file_generation_id
    JOIN source_files f ON f.file_id = g.file_id
    WHERE p.event_id = e.event_id AND f.source_id = :source_id
  );
```

主页面 bundle 由一个读事务获取 app_state 和所有查询；事务结束后响应仍是固定结果。账户 DTO 单独附 revision / fetched_at，不能伪造 SQLite 与远端额度的原子一致性。

事件分页按 `(occurred_at_ms,event_id)`，会话消费排序按 `(aggregated_total,session_key)`；同快照游标包含 filter hash、排序、最后键值、snapshot_id。不能只保存最后一条 timestamp，不能用不断变化的 OFFSET 承诺稳定分页。

用户改变时区只改变边界与分桶，账本 UTC 不改写。小时图支持夏令时重复小时，通过 UTC bucket start 唯一识别；展示包含 UTC offset。日 / 月边界由真实时区日历生成，不能以固定毫秒相加代替。

### 6.1 分解与覆盖

可空子项独立计算覆盖：例如输入字段可用事件的 Token 覆盖率，不因为 SUM 忽略 null 就声称完整分解。三个可验证分解项之和必须与可信总量一致；不完整时保留总量并标部分已知，不画填满 100% 的误导分解条。

价格覆盖分母为筛选内可信 total；分子为已得到完整价格估算的事件 total。所有事件无法计价时金额为 null；真实零消费且覆盖已核对时可显示零。混合币种返回各币种 subtotal 和各自 coverage，不能相加。

coverage 定义为“已配置来源中当前可读取、已完成扫描且可解释的记录覆盖”，不能推断未记录消费。明细包括 scanned / pending files、source errors、pending observations、unattributed、unknown timestamp 和 parser compatibility。

## 7. 价格版本与重估

2026-10-02 M09g1a：`token-pulse-core/data/offline-prices.json` 内置官方文本 Token 单价事实目录（51 个确切模型、172 条模式 / 上下文档位），逐条保留官方来源、十进制每百万价格、缓存写入价格及目录核实 UTC 日期。仅为全球 API 参考估算，不表示订阅实付金额；地区 / 合规加价、工具、图像 / 音频、微调和未来才开始计费的模型不纳入本目录。事实来源为[官方价格表](https://developers.openai.com/api/docs/pricing)与各 Codex 模型官方页。

schema v4 新增 `offline_price_catalogs`，保存不可变正文 / SHA-256 / 核实时间 / 发布修订。同一 ID、同一内容重复安装不推进修订；同 ID 不同内容拒绝，不能降级覆盖已核实较新目录。目录、可用规则与 `price_revision` 在同一 Writer 事务发布，失败保留旧内容，不改变 Token / data revision。新目录发布时，以新不可变历史行封闭旧参考价格的核实区间；已捕获旧修订的开放区间不变。历史 migration 不修改。

当前聚合用量仅能表示 37 条无需上下文档位 / 独立缓存写入维度的 Standard 规则。其余事实继续保存，但不能扁平套价；完整条件匹配与界面目录接入仍需后续实施。参考规则起点为核实日 2026-10-02 00:00 UTC，不臆造历史生效价格；更早事件保留未计价，用户可明确选择指定估价时点或发布自定义历史规则。开发更新脚本只读官方文档，生产程序内嵌目录且不联网抓价格。

M09g1b 已接正常启动发布（在 Collector 启动之前）和主窗口目录浏览。`get_offline_price_catalog` 使用固定价格修订的实际只读事务读取目录；历史版本 0 返回 null，未知 / 超前修订拒绝，不暗换当前目录。正式生产始终发布内置目录；debug 原生隔离场景保持各自夹具，只有显式 `-OfflinePrices` 使用正常启动价格发布。共享隐私序列化时清空目录，前端旧版本请求不能回填到新版本。完整请求条件匹配、缓存与重估仍待后续。

- 规则行发布后不可原地修改费率；更新创建新 rule_id 并记录 introduced / retired price revision。
- 指定 source 的自定义规则优先于全局自定义，随后是明确 provider / model 的离线规则。别名只通过版本化受控映射匹配，不使用任意字符串相似度。
- 同优先级与相同范围的生效时间重叠在保存时拒绝；候选仍多于一个时标 ambiguous_rule，不能任选最便宜 / 最新。
- event_time 用 occurred_at；specified_time 用明确的估价时点，不能把“当前”隐含为每次查询的系统时间。
- 缓存字段缺失且价格需要拆分时为 insufficient_usage；已知缓存为零时不要求缓存单价。来源额外 cache_write 只有语义与单价明确时才能增加计价维度，否则保持未计价标记。
- `valuation_sets` 为缓存，不是事实账本；只使用 ready 且价格版本 / 估价模式匹配的项。新事件缺失缓存时按同一规则版本即时计算，不能回退为零。

M09g2a 已接持久事件费用缓存读写基础：schema v5 使用既有 `valuation_sets` / `event_valuations`，追加 `valuation_cache_sets`（账本证据修订、解析 / 核算 / 缓存版本、数量、发布摘要）与 `valuation_cache_inputs`（事件的计价输入 SHA-256）。不修改历史 migration 或事实表。指纹含算法版本、核算版本、提供方、确切模型、全体真实来源 ID、事件时间和完整 Token 向量；UI 来源筛选不能缩小计价证据。读取同时匹配 price revision、event_time / specified_time 及确切估价时点；未知金额保留 null，存在真实零价时才存零。费用缓存不改变 data / price / settings revision 或采集检查点。

`build_event_valuation_interruptible` 在单一实际 SQLite 读事务捕获账本和价格，流式计算并每 500 行通过 Writer 写入 building 候选，无全历史事件向量驻留。发布前重新校验活跃账本、证据修订、解析 / 核算版本，复核持久行数量与完整候选摘要，再同事务设置 ready。取消、事实变化和发布失败留下不可读候选，先前 ready 结果保留。缓存版本或事件输入变化即失配；旧 SQLite 租约仍读取原始事实 / 规则 / 缓存。读取方每次查询创建并复用 CacheReader，没有匹配 ready 集合时仅检查一次后直接计价。金额使用十进制整数原子，不经过 SQLite REAL / JS Number。总览 / 分组 / 会话 / 回合 / 小窗共用 visit 读法，明细租约单独接同一读法。

本模块提供内部构建 / 进度回调 / 取消信号及启动中断处理；独立后台服务、正式重估作业与进度 / 取消 UI 尚未接入，不能将持久表和内部构建 API 视为完整 M09 重估交付。部分发布缓存和未命中事件始终以同一快照版本即时补算，不混用新旧价格。

M09g2b1 追加 schema v6 的独立 `price_revalue_jobs` / `price_revalue_plan`。价格作业保存规范化范围、估价模式、固定 price revision、请求幂等键、账本计划和十进制进度；不复用采集作业的文件计数或检查点。创建 / 领取 / 单调进度 / 完成 / 取消 / 启动中断均经 Writer 原子提交。手动请求使用价格修订 CAS，重复键先返回原结果，变更载荷拒绝；同一时刻只领取一个作业，手动队列优先。整个作业通过 `build_event_valuation_at_revision_interruptible` 固定规则版本，途中改价不混用金额，查询仍按自己的快照匹配缓存。

自动请求身份包含价格修订、缓存版本和全部活跃账本证据，排除缓存就绪状态。取消 / 失败后同一输入不自动反复启动；新价格 / 证据可以建立新任务，用户也可发新手动请求。重启把未结束任务标为 interrupted，已持久化的取消保持 cancelled；自动补建仅重排缺失的 ready 缓存，先前发布结果保留。状态中的 uncached_ledgers 专指当前价格版本的 event_time 缓存缺口，不代表指定时点的覆盖。此阶段尚未启动线程、暴露前端命令或进度界面。
- price_revision 改变通知前端刷新；查询响应同时带 data_revision 与 price_revision。构建期间显示重估状态，不能混用两个价格版本。

M09g2b3 已把服务接正常启动 / 退出、价格与别名提交、休眠恢复及正式 main-only 重估入口；上述 M09g2a / b1 / b2 的“未接正式应用”是历史阶段记录。周期检测新消费证据，只补缺失的 event_time 缓存；手动可指定明确估价时点。任务自身的基本状态与进度独立持久化，状态读取在同一 SQLite 快照取得当前价格修订、活跃 / 最近任务和 event_time 缓存缺口。查询缓存仍按其自身版本与输入匹配，任务完成不强行切换消费 / 查询快照。

M09g2b2 的 `RevalueService` 串行领取独立作业，自动缺口检测周期 5 秒，手动请求 / 取消与价格变化可发送有界唤醒信号。每个账本使用作业固定修订；进度回调仅经 Writer 提交，不持有额外读事务。退出设置独立 stop / 当前 cancel 信号、保存 interrupted 并 join，启动将残留 building 标为失败及未结束作业标为 interrupted，已请求 cancelling 保持 cancelled。失败 / 取消不反复重新排同一自动身份；当前服务还未接正式应用生命周期和前端。
- 查询或租约捕获的 price_revision 固定规则与别名：`introduced_revision <= revision` 且 `retired_revision IS NULL OR retired_revision > revision`。内存 PricingService 必须按该版本取不可变规则，不能使用“最新规则”解释旧快照。

## 8. 已取消的数据维护范围与既有存储实现

既有 migration、checksum、结构版本和事务代码保持现状，不修改历史 migration。不再追加迁移保护、长回填恢复检查点或专门故障恢复流程；相关专项验收取消。普通数据库错误明确提示，不承诺自动修复或恢复历史数据。

已有迁移备份使用 Online Backup API，相关已完成实现和测试记录保留；不继续扩展备份 manifest 或提供备份管理功能。[SQLite 官方备份接口](https://www.sqlite.org/backup.html)

2026-10-02 用户取消 M14：不提供手动备份、备份恢复、跨设备离线恢复、清除来源或清除全部数据的 UI / IPC。既有迁移备份实现和数据库结构继续保留；取消产品功能不修改历史 migration 或 checksum。后台派生数据回收与卸载数据处理仍受应用数据目录边界约束，Codex 原始日志不在可删除集合内。

数据库不可用时显示错误，不制造零历史或宣称已恢复。数据库损坏恢复、升级强杀、备份恢复与灾难恢复专项开发 / 验收已取消；现有事务正确性测试保留。

## 9. 最低存储验证集

|验证|必须得到的结果|
|---|---|
|core DDL / 外键|建表通过，错误 ledger / file 指针拒绝|
|重复物理位置|同 file generation + offset 不重复保存观察|
|相同推断指纹、不同调用|两条有效事件保留，不能被 UNIQUE 吞掉|
|两份镜像来源|全来源 total 一份，单来源各可查，多来源 OR 不翻倍|
|提交每一步崩溃|偏移、事件、基线全有或全无|
|新 ledger 发布|读事务只看到旧或新活跃结果，失败仍旧版本|
|NULL 子项|总量可显示，分解与计价标记实际覆盖|
|大整数 / 价格|超过 JS 安全整数仍准确，金额先总计后舍入|
|价格重估|Token 不变，price revision、规则和金额可追溯|

DDL 的语法和约束检查只能验证结构草案；不代替 Writer、重建、计价与系统部署验收。

## M09f1：别名不可变发布

已实现 model_aliases 用户 create / replace / retire，复用 price_revision CAS 和单写事务；introduced_revision / retired_revision 固定历史映射，事务内 rules_at 返回刚发布版本，写失败不退休旧行或推进修订。ID 的 alias-custom- 命名空间只由用户写入生成，导入目录不得使用该前缀，非用户 ID 拒绝编辑。活动映射上限 4096；provider / alias 精确匹配，canonical_model 可尚未有价格而保持未计价。新写入拒绝同键映射、链式两方向、循环和自映射，历史不明确映射继续按原计价歧义逻辑隔离。别名修改不创建消费、账本或 data_revision；旧查询仍采用捕获的价格版本，Token 不变。没有新增迁移或恢复范围。
