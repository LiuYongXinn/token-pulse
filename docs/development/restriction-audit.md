# 项目限制审查

审查日期：2026-10-10。范围包括桌面运行壳、六个 Rust 业务 crate、主窗口与小窗前端、安装更新及开发 / 原生验收脚本。以当前源码中的实际拒绝条件、截断位置、回退行为和既有范围约定为依据；包括静态审查、修复、自动回归及定向验证，不代表所有 Windows 版本或极端数据规模已实机验收。

## 1. 已修复的版本拦截

- 上一轮提交 `2541f6f` 移除了任务栏生产代码的 build 19045 白名单，并记录真实系统 build。
- 本轮提交 `9b3daa4` 移除了 5 个 PowerShell 入口和 2 个 C# 检测器里的同类拦截：`inspect-explorer-restart.ps1`、`inspect-power-resume.ps1`、`verify-power-resume.ps1`、`native-mini-placement.ps1`、`verify-installed-tray.ps1`、`power-resume-probe.cs`、`standby-resume-driver.cs`。
- 电源探测结果的 `Build` 从写死 19045 改为实际系统值。保留调用 Windows API 所需的平台检查、原生结构布局检查以及原有显式休眠开关。
- 验证：5 个 PowerShell 文件 AST 解析通过，2 个 C# 文件通过 `Add-Type` 编译；`TokenPulsePowerInspection.Inspect(false)` 返回实际 build，未调用休眠、休止、重启或策略修改。没有执行实际休眠、Explorer 重启或托盘物理输入验收。

## 2. 已完成的限制修复

用户确认将审查问题全部修改后，以下九项已落实到代码、契约和界面。每个功能模块单独提交；保留第 3 节的数据一致性及运行资源边界。

|原问题|当前行为|提交与依据|
|---|---|---|
|C 盘 / 系统盘被拒绝|启动、NSIS 钩子、开发运行器、迁移入口允许任意本地盘的绝对路径；可写性由实际目录 / 文件操作验证。|`f04c6ee`；[路径](../../src-tauri/src/local_paths.rs)、[存储约定](project-local-storage.md)|
|历史来源永久占用 32 个名额|移除注册总数和批量添加数量上限；事件来源 DTO、事件查询、计价查询同步支持更多来源，历史行继续保留。|`44def6d`；[来源管理](../../crates/token-pulse-store/src/source_management.rs)、[事件查询](../../crates/token-pulse-store/src/query/events.rs)|
|新增用量元数据导致整条拒绝|兼容描述性扩展字段；未知 Token / 用量计数维度仍进入明确诊断，已知计数、数值和缓存别名冲突校验保留。|`8d03b51` / `d43bbf2`；[日志适配器](../../crates/token-pulse-core/src/adapter.rs)、[回归](../../crates/token-pulse-core/tests/cache_write.rs)|
|模型 / 项目只能看前 200 类|增加前后翻页，200 是每页预算；游标绑定窗口、筛选、排序和价格口径，全部页沿用同一个 SQLite 快照及总计。显示缓存去除游标，恢复时重新读取；拒绝把继续分页请求写入展示缓存。|`9b60c66`；[分组查询](../../crates/token-pulse-store/src/query/groups.rs)、[页面](../../ui/src/app/GroupedPage.tsx)|
|候选超过 1000 项便停止|不再因为数量释放游标，继续加载后续候选；只保留最近 1000 项显示缓存，较早项可“从头浏览”，搜索和选择仍保留。|`c16c30a`；[高级候选](../../ui/src/app/useFilterOptions.ts)、[小窗候选](../../ui/src/mini/useMiniSessionOptions.ts)|
|大标题索引或一条坏行使整轮失败|移除文件大小上限，按偏移增量读取，每批最多 500 标题 / 4 MiB 软预算；未完成追加下轮重读，替换 / 改写重新扫描。行读取内存预算为 8 MiB，超过时分段丢弃该行并继续后续记录；大于原 64 KiB 的有效扩展元数据正常解析。异常展示索引路径、原因和偏移，修复后清除，不改变可信用量或价格修订。|`09da6f0`；[标题读取](../../crates/token-pulse-collector/src/session_titles.rs)、[诊断展示](../../ui/src/app/DiagnosticsIssues.tsx)|
|透明度仅 70%–100%|后端、持久化验证、生成契约及滑块统一支持 0%–100%。0% 完全透明，主窗口设置仍可恢复。|`f98ff72`；[契约](../../crates/token-pulse-core/src/mini_opacity.rs)、[设置](../../ui/src/app/MiniOpacityPanel.tsx)|
|PATH 只检查前 256 项|按 PATH 原顺序遍历全部去重后的有效本地目录，发现过程中不执行程序或脚本。|`f882830`；[程序发现](../../crates/token-pulse-quota/src/discovery.rs)|
|账户类型必须是 chatgpt|有账户对象或明确无需认证时实际调用额度 RPC，以响应判断支持；缺少所需认证时仍显示认证不可用，服务真正不提供额度时保留 Unsupported。|`54378cf`；[账户判断](../../crates/token-pulse-core/src/quota/parse.rs)、[服务验证](../../crates/token-pulse-quota/tests/service.rs)|

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

## 4. 验证与交付边界

定向验证包括本地盘符路径、开发运行器、真实 NSIS 更新钩子编译 / 执行、40 个来源移除后继续添加、单事件 41 个来源及末尾来源专属价格规则、附加日志元数据、206 个分组同快照分页和游标绑定、1051 个高级筛选候选、1026 个小窗会话候选、透明度 0 / 50 / 100、长 PATH、新账户类型额度探测，以及大索引 / 长行 / 部分追加 / 重写 / 标题诊断清除。标题元数据错误和修复不修改 Token、费用及其修订。

最终验证：`cargo test --workspace --all-features -- --test-threads=1` 为 799 passed / 0 failed / 5 ignored；Vitest 57 项通过，完整 Playwright 127 项通过，新增标题诊断展示用例另行通过。TypeScript、生成契约一致性、全工作区严格 Clippy、Rust 格式检查和前端生产构建通过。并行全量测试中，既有 60 ms 租约用例曾在申请阶段过期，最终用串行执行隔离测试间资源竞争并通过；生产租约时限保持原设计。

修改针对仓库源码和生成契约，未迁移用户数据或更新正式安装程序；不同 Windows 原生结构的实机适配仍按第 3 节实际能力处理。
