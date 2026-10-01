# 数据与存储详细设计

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

价格限制在 `[0, 1000000]` / 百万 Token，所有乘加 checked i128；单价、费用整数以十进制 TEXT 存储。展示 / 导出最后统一四舍五入到所需精度，不能逐事件先舍入再累计。不同币种分别返回，不自动换汇。

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

来源筛选采用 `EXISTS` 判断事件证据，不 JOIN 后直接 SUM。单独查看两个镜像来源可能都包含同一个事件，其来源小计不是互斥分组；UI 不显示可相加的来源占比饼图。暂停来源仍保留历史可查询；选择清除该来源数据时，只清理它的证据及仅由其支持的派生结果，保留其他来源的共同事件并重建受影响会话。

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

- 规则行发布后不可原地修改费率；更新创建新 rule_id 并记录 introduced / retired price revision。
- 指定 source 的自定义规则优先于全局自定义，随后是明确 provider / model 的离线规则。别名只通过版本化受控映射匹配，不使用任意字符串相似度。
- 同优先级与相同范围的生效时间重叠在保存时拒绝；候选仍多于一个时标 ambiguous_rule，不能任选最便宜 / 最新。
- event_time 用 occurred_at；specified_time 用明确的估价时点，不能把“当前”隐含为每次查询的系统时间。
- 缓存字段缺失且价格需要拆分时为 insufficient_usage；已知缓存为零时不要求缓存单价。来源额外 cache_write 只有语义与单价明确时才能增加计价维度，否则保持未计价标记。
- `valuation_sets` 为缓存，不是事实账本；只使用 ready 且价格版本 / 估价模式匹配的项。新事件缺失缓存时按同一规则版本即时计算，不能回退为零。
- price_revision 改变通知前端刷新；查询响应同时带 data_revision 与 price_revision。构建期间显示重估状态，不能混用两个价格版本。
- 查询或租约捕获的 price_revision 固定规则与别名：`introduced_revision <= revision` 且 `retired_revision IS NULL OR retired_revision > revision`。内存 PricingService 必须按该版本取不可变规则，不能使用“最新规则”解释旧快照。

## 8. 迁移、备份、删除与恢复

迁移编号、checksum、schema 变更和回填记录进入迁移管理。启动检测未知较新 schema 时只读恢复提示，不能按旧程序写入。先一致备份再迁移，事务支持的结构改动一批完成；长回填有恢复检查点。

备份使用 Online Backup API 得到完整数据库快照后生成 manifest：app version、schema、data / price / settings revision、UTC 创建时间、库 checksum、字段类别。活动 WAL 库不能只复制 `.db` 主文件。[SQLite 官方备份接口](https://www.sqlite.org/backup.html)

恢复先停止新作业、提交安全批次、取消查询租约、关闭数据库；在临时目录验证 manifest / checksum / foreign_key_check / integrity_check。保留当前库，再切换已验证副本；失败回到旧库或恢复状态。跨设备恢复默认所有来源离线，重新授权后才补扫。

清除来源与全部数据使用后端生成的 confirmation token 绑定影响范围和 revision，见 IPC 契约；不使用前端字符串“已确认”作为唯一保护。受控删除仅针对应用数据目录和用户选定的本工具输出文件，Codex 原始日志不在可删除集合内。

数据库损坏停止写入、保留损坏副本。源日志与必要观察均缺失时明确不可恢复，不制造零历史。磁盘满、事务中断与备份取消的期望行为必须故障注入验证。

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
|备份 / 恢复 / 删除|一致恢复，无源文件写入，镜像共同证据不误删|

DDL 的语法和约束检查只能验证结构草案；不代替 Writer、重建、计价与系统部署验收。
