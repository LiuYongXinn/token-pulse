# 项目限制审查

审查日期：2026-10-10。范围包括桌面运行壳、六个 Rust 业务 crate、主窗口与小窗前端、安装更新及开发 / 原生验收脚本。以当前源码中的实际拒绝条件、截断位置、回退行为和既有范围约定为依据；这是静态审查与定向验证，不代表所有 Windows 版本或极端数据规模已实机验收。

## 1. 已修复的版本拦截

- 上一轮提交 `2541f6f` 移除了任务栏生产代码的 build 19045 白名单，并记录真实系统 build。
- 本轮提交 `9b3daa4` 移除了 5 个 PowerShell 入口和 2 个 C# 检测器里的同类拦截：`inspect-explorer-restart.ps1`、`inspect-power-resume.ps1`、`verify-power-resume.ps1`、`native-mini-placement.ps1`、`verify-installed-tray.ps1`、`power-resume-probe.cs`、`standby-resume-driver.cs`。
- 电源探测结果的 `Build` 从写死 19045 改为实际系统值。保留调用 Windows API 所需的平台检查、原生结构布局检查以及原有显式休眠开关。
- 验证：5 个 PowerShell 文件 AST 解析通过，2 个 C# 文件通过 `Add-Type` 编译；`TokenPulsePowerInspection.Inspect(false)` 返回实际 build，未调用休眠、休止、重启或策略修改。没有执行实际休眠、Explorer 重启或托盘物理输入验收。

## 2. 需要改进的限制

下表的高优先级表示会阻断正常使用或影响统计输入，中优先级表示会限制浏览、发现或配置。除第 1 节外，本轮保留现有行为，给出修改边界，避免把一次限制审查变成未经定义的数据语义或存储迁移。

|优先级|限制与实际触发条件|影响及建议|依据|
|---|---|---|---|
|高|应用启动、安装器、开发运行器及迁移入口直接拒绝 C 盘 / 系统盘。|只有一个本地盘的用户无法使用默认安装路径。这是盘符策略，不是权限或可写性检测。建议允许任意可写本地目录；继续保留本项目 `.local` 存储约定。现有文档专门要求数据留在项目目录并排除系统盘，本轮未改变该存储策略。|[运行路径](../../src-tauri/src/local_paths.rs)、[安装钩子](../../src-tauri/windows/update-hooks.nsh)、[JS 运行器](../../scripts/project-run.mjs)、[PowerShell 环境](../../scripts/project-env.ps1)、[迁移入口](../../scripts/migrate-local-data.ps1)、[现有存储约定](project-local-storage.md)|
|高|来源注册按 `SELECT COUNT(*) FROM sources` 限制为 32；“移除”只更新 retained / removed 状态，没有删除行。|累计添加 32 个不同来源后，即使全部移除也无法新增。应将当前管理容量与历史来源标识分开，历史数据不应消耗永久名额。不能只删写入处判断：事件 DTO、事件查询与费用查询也假设来源向量最多 32 项，必须一起调整。|[来源管理](../../crates/token-pulse-store/src/source_management.rs)、[事件契约](../../crates/token-pulse-core/src/query/events.rs)、[事件查询](../../crates/token-pulse-store/src/query/events.rs)、[计价查询](../../crates/token-pulse-store/src/query/pricing.rs)|
|高|用量 JSON 的 `usage()` 对象只允许 7 个确切字段；出现任意其他字段就整体返回解析失败。|上游新增不影响既有计数语义的附加字段，也可能令原本可用的用量进入诊断而不参与可信统计。应区分可忽略扩展与会改变计数含义的未知字段，保留已知字段类型、总量关系、缓存写入冲突校验，并提供明确的兼容诊断；不能无条件忽略所有未知计费字段。现有 `cache_write` 测试确实断言未知字段被拒绝，本轮定向运行通过。|[日志适配器](../../crates/token-pulse-core/src/adapter.rs)、[现有回归](../../crates/token-pulse-core/tests/cache_write.rs)|
|中|模型 / 项目分组只允许 50 / 100 / 200 项；后端 `GroupedUsageRequest.limit` 最大 200，没有后续分页入口。|200 项之后无法在同一视图继续浏览，只能缩小筛选。总数与总计仍存在，不是账本丢失。建议改为分页并保留当前排序、同快照总计。|[分组页面](../../ui/src/app/GroupedPage.tsx)、[查询契约](../../crates/token-pulse-core/src/query.rs)|
|中|高级筛选和小窗会话候选都在累计 1000 项后主动释放游标。|即使服务端还有下一页，前端也停止“加载更多”。搜索可以缩小候选，但不等于能完整浏览。建议采用分页或虚拟列表，不把渲染预算变成数据可访问上限。|[高级筛选候选](../../ui/src/app/useFilterOptions.ts)、[小窗会话候选](../../ui/src/mini/useMiniSessionOptions.ts)|
|中|会话标题索引超过 64 MiB，或任意行超过 64 KiB，整轮读取失败；`refresh()` 对错误直接跳过。|用量采集本身不因此停止，但标题可能一直保留旧值或回退为 ID，且没有对应的明确 UI 原因。建议增量读取、逐行跳过并记录异常，避免一个超长坏行影响其他正常标题；大文件采用分批同步。|[标题读取与刷新](../../crates/token-pulse-collector/src/session_titles.rs)|
|中|小窗透明度后端和生成契约均限定 70%–100%，前端滑块同步限制。|用户无法选择更低透明度。这是可读性取舍，不是平台版本能力。建议把较低透明度开放为用户设置，明确完全不可见时的恢复方式；不能只修改滑块而遗漏后端 / 持久化校验。|[透明度契约](../../crates/token-pulse-core/src/mini_opacity.rs)、[设置面板](../../ui/src/app/MiniOpacityPanel.tsx)|
|中|本地账户程序自动发现只检查候选 PATH 的前 256 项。|后续位置存在有效程序也会报“未检测到”，仍可手动选择。建议遍历去重后的本地路径，或以耗时预算加明确提示控制发现过程，而不是静默截断目录数量。|[程序发现](../../crates/token-pulse-quota/src/discovery.rs)|
|兼容性风险|账户可用性仅将 `account.type == "chatgpt"` 视为可读取额度，所有其他非空类型直接标记 Unsupported。|没有确认当前正常账户被此规则误拒绝，因此不直接认定为现有故障。未来返回新类型时会在实际额度请求前被挡住；建议结合服务能力或有界探测判断，同时保留“服务确实不提供额度”的结果。|[账户可用性](../../crates/token-pulse-core/src/quota/parse.rs)、[服务状态](../../crates/token-pulse-quota/src/service.rs)|

## 3. 有依据的边界与后续改进点

以下检查不能与版本白名单一起机械删除。

- **数值和账本完整性**：精确整数 / 金额范围、已知字段类型、总量关系、缓存写入别名冲突、配置修订 CAS、未来数据库 / 配置版本拒绝，防止生成错误统计或覆盖无法理解的数据。外部格式扩展兼容性应单独改善，不能取消核心一致性判断。
- **流式读取预算**：日志单行 8 MiB、单批 16 MiB / 500 条、待处理文件队列 4096 项属于处理预算。采集目录迭代没有 10000 个文件的总量截断，队列溢出会要求重新核对。后续可改进大行处理，但不能将单批容量误报为全部历史的最大容量。见 [读取器](../../crates/token-pulse-core/src/reader.rs)、[调度器](../../crates/token-pulse-core/src/scheduling.rs)、[目录枚举](../../crates/token-pulse-core/src/sources.rs)。
- **核算基线 128 条**：达到上限后走容量不足的待确认分支，避免淘汰仍需去重或累计差核算的基线。它可能影响超长或多流会话，应设计分层 / 持久化基线再放开，不能直接无限扩容或丢弃旧基线。见 [核算状态](../../crates/token-pulse-core/src/accounting.rs)。
- **日历分桶 2000 项**：前端长范围会自动从小时切日 / 月，并非只允许 2000 条用量。若需要长范围小时细节，应分页或下钻，保留夏令时与精确边界处理。见 [日历](../../crates/token-pulse-core/src/calendar.rs)、[粒度选择](../../ui/src/shared/main-filter.ts)。
- **SQLite 分页快照总时限 30 秒、空闲 10 秒**：用于释放读事务并控制 WAL 增长。主页面已有续查相关控制，小窗 / 筛选候选过期仍会要求重查，可以改进交互恢复，但不能直接无限延长所有事务。见 [快照租约](../../crates/token-pulse-store/src/leases/service.rs)、[主页面分页](../../ui/src/app/paged-usage-cache.ts)。
- **原生任务栏结构校验**：版本号放开后仍只适配现有 ReBar / MSTaskSw / MSTaskList 结构，主要实例定位主任务栏。不同结构、竖向任务栏等仍需实现相应布局，删除类名和区域校验不能替代适配。见 [任务栏探测](../../crates/token-pulse-taskbar/src/windows/topology.rs)。
- **更新来源、签名、安装渠道、PID 与 IPC 身份**：这些检查决定下载或操作的对象，保留。便携版不能直接走 NSIS 安装更新是渠道差异，需要单独提供便携更新路径；不是一个无理由的 OS build 白名单。
- **通知配置事务能力**：当前安全写入实现依赖系统事务 API，能力缺失时通知自动接入不可用、日志采集继续。需要其他文件系统兼容时应补写入实现与并发冲突处理，不应绕过后覆盖用户配置。见 [配置事务](../../crates/token-pulse-integration/src/notify_config/windows/transaction.rs)。
- **已确认产品范围**：macOS、WSL / 网络来源、开机启动和额外全局快捷键在现有计划中明确取消；本轮未把这些重新当作应实现功能。恢复交互快捷键仍保留。见 [已确认范围](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)。

## 4. 建议实施顺序

1. 将本机项目内存储约定与面向普通用户的安装盘符策略分离，并验证单盘可写目录启动 / 安装。
2. 修复来源历史占用容量，连同查询向量及契约一起调整，保证历史账本继续可读。
3. 定义日志附加字段兼容策略，补正常用量加新元数据字段、未知计费字段、缓存别名冲突等对照场景。
4. 分组和候选列表接完整分页 / 虚拟滚动，再处理标题索引增量同步及设置范围。

除已提交的原生检测修复外，本轮没有迁移用户数据、修改安装位置或发布新安装包。
