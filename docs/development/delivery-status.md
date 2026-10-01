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
