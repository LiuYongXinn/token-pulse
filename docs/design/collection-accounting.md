# 采集与核算详细设计

总入口：[详细开发设计](development-design.md)。表与事务见[存储设计](data-storage.md)，对外类型见[IPC 契约](ipc-contracts.md)。本模块的目标是可解释、可重放的日志用量账本，不是从文本长度推算 Token 或读取账户账单。

## 1. 标准输入与输出

```rust
// 领域草案：实际字段名和 serde 契约由共享类型固定。
struct UsageVector {
    input_total: Option<i64>,
    cached_input: Option<i64>,
    output_total: Option<i64>,
    reasoning_output: Option<i64>,
    reported_total: Option<i64>,
}

enum AdaptedRecord {
    SessionMetadata(SessionMetadata),
    TurnMetadata(TurnMetadata),
    UsageObservation(UsageObservation),
    ContextObservation(ContextObservation),
    Ignored,
    Unsupported(FormatDiagnostic),
    Corrupt(ParseDiagnostic),
}

struct UsageObservation {
    physical_position: PhysicalPosition,
    session_key: SessionKey,
    event_time_ms: Option<i64>,
    request_identity: Option<VerifiedRequestIdentity>,
    turn_id: Option<String>,
    stream_hint: Option<VerifiedStreamIdentity>,
    last: Option<UsageVector>,
    cumulative: Option<UsageVector>,
    effective_metadata: EffectiveMetadata,
}

struct AccountingBatch {
    confirmed_events: Vec<CanonicalUsageEvent>,
    classifications: Vec<ObservationClassification>,
    stream_updates: Vec<StreamUpdate>,
    context_snapshots: Vec<ContextSnapshot>,
    provenance_updates: Vec<EventProvenance>,
    diagnostics: Vec<Diagnostic>,
}
```

ReaderContext 保存会话、模型、provider、cwd、回合、父关系和支持的格式版本。metadata 变化只作用于后续观察，不修改之前消费；修订历史通过显式重建。缺失 cwd / 模型为未知分类，不回填会话最终值。

原始记录类型和嵌套路径通过版本化 CodexAdapter 识别，兼容清单与合成 / 脱敏夹具一起提交。完整日志格式不是本项目可控制的公共协议，未知变化返回 Unsupported，不按当前单个样本“宽松猜测”。

## 2. 来源发现与调度

### 2.1 来源注册

用户指定目录优先，再检测进程可见的 CODEX_HOME 和用户默认目录。UI 展示检测结果并允许修正；WSL 由用户开启后发现发行版与可访问路径。

规范化包含绝对路径、Windows 大小写规则、目录真实身份、符号链接 / reparse point。证明相同物理目录时复用 source；无法证明但内容相近时保留并给重叠警告。扫描不跟随目录环。

扫描 `sessions` 与 `archived_sessions` 中 JSONL，文件名只作候选过滤，实际会话身份来自记录。目录不存在为 awaiting_directory，可周期重试；单目录权限失败为 unreadable，不使其他来源停止。

### 2.2 Scheduler

优先级从高到低：系统恢复核对 / 正在活跃的追加、手动核对、近期导入、历史导入、缓存维护。取消 / 退出为控制信号，不排在普通文件任务之后。

```text
idle → dirty → queued → reading → committing → idle
                                  ↓             ↑
                               retry_wait ──────┘
reading 期间再次 dirty → 只记标记，当前提交后重新核对
身份或锚点失配 → invalid_generation → rebuild_job
```

默认防抖 300 ms，连续写入最长等待 2 s；读取并发 2，压测后可到 4。脏文件集合按物理文件合并，队列容量初始 4096；溢出时置 source_reconcile_required，不丢弃“需要核对”的事实。

同文件不并发读取，同会话核算和发布串行；多会话可以并行读。锁顺序固定为来源配置短锁 → 文件锁 → 会话键排序锁；DatabaseWriter 在提交期间不回调需要这些锁的服务。来源发现不长时间持有配置锁。

每个来源独立退避，I/O 重试初始 1 / 2 / 5 / 15 / 60 s，有抖动与最大次数；持续不可读进入降频核对。定时路径：近期与活跃文件 60 s、历史清单 10 min、启动 / 唤醒 / watcher 溢出立即核对。watcher 不作为消费证明。

## 3. 字节读取与代次判定

1. 打开只读句柄，记录物理身份、当前大小上界、mtime 与检查点锚点。
2. 校验 committed_offset 与代次；从该字节位置开始按 64 KiB 块读取，最多追到本次上界。
3. 以换行分割完整字节记录，再 UTF-8 解码；不在块边界独立解码半个汉字。
4. 保留末尾未结束行，持久偏移停在最后完整行末尾；重启从该处重读。
5. 完整合法记录生成规范观察；空行跳过；完整损坏行持久化错误位置并继续。
6. Writer 成功提交观察、派生结果、基线和完整行偏移，才视为处理成功。

CRLF 与 LF 均支持，偏移按原始字节计算，byte_end 包含真实行终止符。无末尾换行的 JSON 即使当前能解析也不提交，等待明确完整边界；避免把正在写入的 JSON 当作完整记录。

行上限 8 MiB。超限后进入 skip_until_newline，记录起始偏移和已跳过字节，直到终止符才安全越过；状态可持久恢复且不保存原文。若 I/O 中断则按已提交位置重读，内存始终受限。

### 3.1 替换、缩短与覆盖

|变化|动作|
|---|---|
|size 增长、身份 / 锚点连续|从检查点增量读取|
|size 缩短、身份改变、已读锚点失配|新物理代次与候选账本，重建，不相减为负消费|
|移动到归档且内容连续|更新位置 / 映射，保留逻辑会话与事件|
|mtime 变化但大小相同|验证分段内容指纹，发现差异则重建|
|文件删除|标记缺失，保留已采集账本和必要观察|

每次提交保存文件头、已读尾部和固定分段锚点的 hash 与范围；发现 mtime 变化时核对已读分段。定期完整清单核对可安排内容校验。仅头尾 hash 不能证明中间永远不改，默认不承诺检测外部程序刻意伪造所有元数据的修改；手动完整重建提供重新解释路径。

读取期间再次改写且不能证明本批来自连续代次时丢弃准备结果并重建。不要把候选观察提交到当前活跃版本；旧统计保留并标记 correction_pending。

## 4. Token 验证与上下文

```text
noncached = input_total - cached_input
validated_total = input_total + output_total
breakdown = noncached + cached_input + output_total
```

缓存是输入子项，推理是输出子项。各项必须非负，已知子项不能超父项；所有运算 checked。reported_total 与验证分解不一致时保留原始规范观察并隔离，不能悄悄修正 reported_total。

如果只缺缓存或推理，但输入 / 输出可信，可以确认总量，分解或计价按实际字段覆盖表达。只报告可信总量也可保留总量事件，但不伪造输入输出。若只有无法定位时间的累计锚点，则放 unattributed，不进入某天消费。

最近请求上下文由有效 last 与明确容量生成，保留快照时间、模型、来源和口径。它不是 cumulative，也不是整回合累加。缺可信容量则 percentage=null；无 last 时不拿累计替代。有效上下文可独立于消费歧义保留，但需标明其自身质量。

## 5. 计数流与增量计算

状态为 `(session_key, ledger_id, stream_key, episode_id)` 的累计向量与依据。来源可信 stream ID 优先；无 ID 时只用可验证的向量关系匹配，不能把模型名或回合 ID当作唯一计数流。

对 `current_total - current_last` 执行按已知字段整体比较：缺失项不是零；只有具有足够同构字段的候选可比较。仅 total scalar 相同不足以认定匹配。不同 stream / episode 中相同数值不能直接当重复。

|条件|处理|方法 / 状态|
|---|---|---|
|同物理位置已提交|不再核算|physical_duplicate|
|可靠请求 ID 已在同身份范围计入|增加镜像证据，不新增消费|verified_duplicate|
|同一已证明流 / episode 的重复快照|无新事件，更新必要状态|repeated_snapshot|
|total-last 唯一匹配完整基线|计入可信 last，推进该基线|last_with_baseline|
|total-last 合法，无候选且可证明新流头|计入可信 last；此前量记未定位锚点|last_new_stream|
|有可靠流 ID，仅 total 且前后连续|按已知分量差验证后计入|cumulative_delta|
|只有 last|物理与可靠语义去重后计入；连续性标注不可验证|last_only|
|首次只有 total|保存锚点，不算导入时消费|unattributed_anchor|
|向量下降且来源明确新计数周期|创建 episode，按新周期规则处理|episode_reset|
|多候选、重置不明、差值字段不足|待确认，不强行相减或计入|ambiguous_usage|

“无候选”不总等于“新流”：若有尚未完成的前序扫描、重放或父前缀候选，先保留 pending。基线初始容量 128，达到上限后保留诊断和候选，不静默淘汰仍可能使用的流；容量策略必须通过压力夹具验证。

一次核算为纯函数：输入规范序列、已有状态、明确规则版本，输出新状态和分类。Clock 只决定扫描与展示，不参与消费推断。固定序列按任意合法分块应得到相同最终事件集合。

### 5.1 独立验证示例

简化向量以 `(input,cached,output,reasoning,total)` 表示：

```text
A1 last=(100,60,10,2,110), total=(100,60,10,2,110)
A2 last=(20,10,5,1,25),   total=(120,70,15,3,135)
```

已验证 A 为独立新流时，可信消费为 135，非缓存输入 50，缓存 70，输出 15，推理 3 已包含在输出。A2 重新扫描不增加消费。

另一个可靠 B 流即使也产生 total=135，不与 A 合并。两个独立 last_only 记录即使时间和向量相同，也应保留；只有可靠请求身份、镜像序列对齐等充分证据才去重。

跨文件同会话序列的排序证据来自稳定源顺序 / ID / 可证明镜像前缀，不用时间戳强行排序所有记录。到达顺序影响候选匹配时隔离并安排统一重建；来源停止变化、核对与重建完成后应与完整导入收敛一致。

## 6. 语义去重、镜像与分叉

### 6.1 两层去重

物理位置防止同文件重读；语义证据防止镜像、归档、重放产生重复消费。payload fingerprint 仅用作候选检索，包含规范用量、稳定时间 / ID / episode 证据，不把可纠正模型和 cwd 编成不可变事件身份。

同指纹多候选时比较来源局部序列、前后锚点与累计连续性；无法唯一证明时记录歧义，不使用 timestamp-only UNIQUE。会话 ID 相同但内容不兼容时创建 identity_conflict，不按 ID 静默拼接。

镜像序列一旦对齐，为已确认事件增加 provenance；唯一部分按可靠序列继续核算。完整复制无需产生第二份 canonical event；来源过滤仍能追到它。当前批次推进基线仅针对有效消费一次，mirror 观察不再推进一次。

### 6.2 父子与继承

有 parent / fork 标识则保存关系；子前缀与父观察序列、事件 ID 和累计基线对齐。已验证 inherited 只记录继承分类，不计入子新消耗；子上下文可继续显示来自自身最近可信快照。

父文件缺失、边界不明、多个可匹配前缀时为 lineage_pending，候选量不进入可信消费。父数据后续补齐 / 修订触发受影响子会话重建。禁止用“记录相隔不到若干毫秒”排除全部继承历史。

## 7. 重建与恢复作业

作业持久化 scope、解析版本、输入 manifest、候选 ledger、批次位置和取消请求。状态：

```text
queued → running → validating → publishing → succeeded
              ↓          ↓
           cancelling → cancelled
              ↓
          interrupted → queued（校验输入后恢复）
任何发布前失败 → failed；旧账本保持活跃
```

Publishing 是短事务临界段，不接受强制中断；已进入该段的取消请求记录为 too_late，完成发布后返回明确结果。长任务取消在完整记录 / 批次边界执行。

重建来源时按该来源关联的逻辑会话，计算镜像 / 父子依赖闭包；不能只把一个镜像文件重新解析再叠加。原日志缺失时允许用必要规范观察重放，但未知元数据 / 从未采集记录不能恢复。

重建输入被覆盖、父版本变化或实时上界失效时 candidate_obsolete，重新规划或重试；不拿旧基线发布到新文件代次。详细短事务与版本切换见存储专题。

## 8. notify 唤醒与恢复路径

notify 是可选加速，不是数据源。headless 子命令仅提取 thread_id / turn_id 等允许字段，丢弃消息正文，再用当前用户本地通道发送 wake hint。主进程未运行时原子保存小型去重唤醒标记；后续启动仍以原始日志为证据。

启用 / 撤销集成必须保真编辑 TOML，显示具体差异，保留已有通知配置和恢复信息；不能以链式脚本执行任意未经用户授权的新命令。原配置后来改变时只提示冲突，不覆盖。

休眠前停止新读取、完成短批次；恢复后重建 watcher、核对代次 / 偏移、账户重查和任务栏恢复独立执行。完全退出后没有 watcher；启动补采的前提是日志或必要观察尚可读取。

## 9. 必须固定的回归夹具

|组|夹具 / 故障|独立预期|
|---|---|---|
|基本口径|缓存、推理、缺子项、reported 不一致|包含关系准确，未知与无效不置零|
|字节边界|每个可能切分点、中文 UTF-8、CRLF、半行|补齐只入一次，偏移按真实字节|
|损坏恢复|非法完整 JSON、超限无换行、后续合法记录|定位错误、限内存、继续处理|
|流|单流、两流交错、基线缺失、重置、同 total|唯一匹配正确，多重匹配 pending|
|身份|同时间不同调用、相同向量、镜像、session ID 冲突|不误吞，不重复，不静默合并|
|分叉|父可读、父迟到、缺父、前缀修订|继承不算消费，不明确就待确认|
|文件|追加、归档移动、缩短、替换、同大小覆盖|代次与重建准确，无旧新叠加|
|一致性|分批 / 完整导入、重启、通知丢失|最终事件集合一致，无检查点分离|
|故障|磁盘满、事务每步崩溃、取消 / 发布竞争|全提交或全回滚，安全恢复|

预期文件必须人工或独立简化 oracle 生成，不直接运行生产算法生成“金标准”。已明确单流夹具可用第二个简单核算器对账；复杂分叉与歧义应人工列明事件身份和分类依据。

## 10. 部分向量约束与核算版本升级

`accounting-v2` 在已有非负、子项不大于已知父项、完整 input + output 与 reported 相等的校验上，增加部分向量下界：输入侧下界取已知 input，否则取已知 cached；输出侧下界取已知 output，否则取已知 reasoning。两侧下界之和必须不大于已知 reported_total。比较使用 i128，避免 i64::MAX 的边界相加溢出；下界中的临时零不回填字段，缺少父项仍为 null，缺少 reported 且父项不完整仍不能制造总量。

例：input=100、output=null、reported=10 无效；cached=7、reasoning=8、两个父项未知、reported=10 也无效。input=100、cached=60、output=10、reasoning=2、reported=110 有效，缓存和推理已包含，不再加到总量。

新观察 / 事件 / 基线 / 候选严格使用当前版本。已发布 `accounting-v1` 事实的查询与 UTC 缓存按其不可变旧规则读取，在真实事务中捕获对应账本版本，不在升级失败前追溯改写旧结果。v1 → v2 用必要观察生成候选，验证通过后整组切换；无效事件转为带原向量 / 原 null / 核算依据的 pending，不补零或构造负消费事件。旧事件保留在历史账本，旧读取事务继续见到旧事实。旧版本基线不能继续追加。

自动升级只接受已声明的 v1 前驱，未知 / 未来核算版本不降级。解析器不匹配也不把已保存观察标成当前 parser；其重新解析路径仍待实施。查询未知核算版本返回 UNSUPPORTED_FORMAT，原始文件与数据库文件保留。
