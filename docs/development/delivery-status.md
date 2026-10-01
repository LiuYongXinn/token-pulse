# 实施与交付记录

任务依据：[实施计划](implementation-plan.md)。本文件区分已经实现、自动检查、真实 Windows 运行时检查及待验收项，不将原型效果或代码存在视为完整交付。

## 起始状态（2026-10-01）

初始 HEAD 为 `7e13fd0`，分支 `main`。仓库只有设计、截图和交互原型。用户已有内容：修改的根 `README.md`、未跟踪的 `AGENTS.md`、`docs/README.md`、`docs/design/token-pulse-design.md`。这些文件不混入功能模块提交。文档索引在原有内容之后追加实际工程文档链接，保留用户内容及其未提交状态。

## M01：工程与运行壳

已建立 npm / Cargo workspace、Tauri 2 / React / TypeScript / Vite、锁文件、Windows CI、独立开发标识和应用数据目录、窗口 / 托盘 / 单实例生命周期、显式命令权限及生产 CSP。七项文字导航与五项设置分类沿用原型色彩和尺寸方向，尚未实现的业务明确标记；无演示用量或价格。

自动检查：前端 strict typecheck、生产构建、浏览器预览拒绝伪造桌面状态的单元测试（1 项）、七项导航 / 五项设置交互测试（1 场景）已通过。Rust check、fmt、Clippy（warnings denied）和 workspace test 已通过；M01 Rust 尚无算法测试，不将零测试计为算法验收。

浏览器视觉检查：深灰背景、近黑面板、205 DIP 导航、蓝色操作、空态与连接错误清楚可辨；在 Codex 浏览器中完成截图审查。此结果只证明浏览器布局。

真实 Windows 10 / Tauri probe：2026-10-01，`cargo build -p token-pulse-desktop --features custom-protocol` 和独立 exe `--native-smoke` 返回 0。确认开发目录隔离、冷启动窗口、托盘注册、原生 close 事件隐藏且托盘保留、第二实例退出并重新显示已有窗口、明确 app.exit。使用内置静态前端，运行不需要 Vite / Node。退出时 WebView2 输出 class unregister 1412 警告，进程仍成功结束；该系统提示继续观察，不记为界面交互验收。

待验收：人工托盘点击与菜单、WebView 中真实 IPC 页面交互、DPI / 屏幕阅读器、Windows 11、release 构建与安装器 / 更新。M02–M16 的业务尚未完成；M01 的页面结构不计作 M10 完成。

## 后续依赖

M02 固定 Rust 领域类型、DTO / TypeScript / schema 生成和精度规则；随后 M03 建立数据库、单写线程和完整事务，M04 / M05 分别实现只读适配和可重放核算。每模块经必要验证后单独提交，后续模块继续沿用本文件记录。

## M02：领域与协议

以 Rust 定义标准用量向量、物理位置、白名单元数据 / 观察、质量分类、状态、错误、筛选、快照、Token 分解、费用 / 覆盖、额度、mini 范围与作业 DTO。使用 ts-rs / schemars 生成 TypeScript 和序列化 JSON Schema；前端运行状态已改用生成类型。后续模块新增 DTO 必须加入同一生成流程。

大整数和修订使用受校验的非负十进制字符串；金额为最多 15 位小数的定点字符串，原始 Token 保持 i64，聚合 checked i128。前端用 BigInt 格式化完整数值 / 缩写 / 有界百分比 / 最终金额舍入。未知值显式 null；超范围返回错误，不截断为零。时区、日期范围、维度数量、非法枚举与未知字段拒绝。带标签的空分支采用空 struct variant，避免 Serde unit variant 忽略额外字段。

`fixtures/usage-vectors.json` 包含 10 组人工列出预期的合成向量；manifest 记录版本与来源。标准观察拒绝正文和任意附加字段，未声明任何真实 Codex 格式已兼容。

验证：Rust 5 项测试（含 property test 和 10 组向量）、前端 5 项测试（含生成 schema / TS 样本、精确格式化）、七页导航浏览器场景、typecheck / 生产前端构建通过。fmt、workspace tests、Clippy 与协议生成差异检查通过。2026-10-01 原生 probe 返回 0，已在真实 WebView 调用 `get_app_status` 并核对协议版本、请求身份和开发状态，随后完成窗口 / 托盘 / 第二实例 / 退出回归。退出时同样有 WebView2 class unregister 1412 提示。

## M03a：迁移、写线程与读连接

已建立 v1 的 25 张业务表与迁移校验表，含活跃账本 / 文件代次组合外键、必要观察、事件、来源证据、基线、jobs / diagnostics 和维护基础表。写入集中于有界队列的专用线程；两个只读连接启用 query_only，bundle 回调在固定读事务内运行。初始化使用 foreign_keys / busy_timeout / WAL / FULL，校验迁移 checksum、quick_check 和 schema 版本；未知较新结构或损坏文件拒绝初始化，不创建零历史覆盖旧文件。

`sum_token_decimal` / `sum_money_atoms` 使用 checked i128，全部缺失返回 NULL。四项真实 SQLite 集成测试通过：建表 / 重开 / 禁止只读写入、并发提交时旧读事务仍固定、超 i64 总量及金额溢出、未知结构与损坏文件保留。fmt / Clippy 通过。本阶段还未接入采集整批事务，M03 完成需继续实现该部分。
