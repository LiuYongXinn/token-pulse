# 本地开发与运行

当前交付环境为 Windows 10 / 11 与本地来源，多屏与各档 DPI 兼容继续验收。macOS、WSL / 网络来源、开机启动、额外快捷键及旧格式自动重解析不再新增；诊断简化、安装打包做简单版，notify、自动更新与完整计价功能保留。详见[已确认范围](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)。

实现依据：[开发总入口](../design/development-design.md)、[实施计划](implementation-plan.md)。当前进度与真实验收结果见[交付记录](delivery-status.md)。

## 环境

2026-10-01 首次工程检查：Windows 10 Pro for Workstations 22H2，build 19045.6466，x64；Node 22.22.2、npm 10.9.7；Edge WebView2 154.0.4258.37。工程初始化前没有 Rust 或 C++ 工具链，已安装官方 Rust stable MSVC 1.98.1、Visual Studio 2022 Build Tools 17.14.41（C++ workload）及 Windows SDK 10.0.26100.0。

前端版本以 `package-lock.json` 为准，Rust 以 `rust-toolchain.toml` / `Cargo.lock` 为准。首次锁定 Tauri 2.12.1、React 19.3.0、Vite 8.3.1、TypeScript 6.0.3、Vitest 5.0.3、Playwright 1.63.0。发布程序不需要 Node 或开发工具。

Windows 前置条件可参照 [Tauri 官方说明](https://v2.tauri.app/start/prerequisites/)。安装 Rust 后新开终端；当前终端找不到 Cargo 时可以临时将 `$env:USERPROFILE\.cargo\bin` 加入 PATH。

## 命令

在仓库根目录执行：

```powershell
npm ci
npm run tauri:dev
```

`npm run dev` 仅预览前端布局，浏览器中没有桌面 IPC，不返回模拟用量。`npm run tauri:dev` 启动开发应用并使用 `com.tokenpulse.desktop.dev`。直接 Cargo debug 构建也强制使用开发标识，避免误写发布目录。

```powershell
npm run typecheck
npm run contracts
npm run contracts:check
npm run test
npx playwright install chromium
npm run test:e2e
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run tauri:build -- --no-bundle
```

开发标识对应 `%LOCALAPPDATA%\com.tokenpulse.desktop.dev`，发布标识为 `com.tokenpulse.desktop`。开发、发布的数据、WebView 与单实例命名空间独立。应用只读采集在设置中启用的 Codex 来源；自动测试使用临时目录和合成夹具。

## 桌面运行壳验收

```powershell
npm run build
cargo build -p token-pulse-desktop --features custom-protocol
& .\target\debug\token-pulse-desktop.exe --native-smoke
```

debug 专用 probe 每次在开发数据目录下创建独立 `native-probe-<UUID>` 数据库，不读取已配置的开发来源。它在真实 Tauri / Win32 运行时中验证冷启动、目录隔离、WebView 状态 / 来源 / 作业 IPC、托盘注册、合成电源消息路由、关闭转隐藏、第二实例激活和明确退出。成功输出 `NATIVE_SMOKE_OK` 并返回 0；失败输出错误并返回 1。应用内置静态前端，不依赖 Vite 服务。该入口不进入 release 构建。

自动 probe 不替代人工点击托盘、屏幕阅读器、DPI、锁屏 / 休眠或 Explorer 重启验收。后续功能逐模块补充相应原生验证，不能将浏览器检查记为系统验收。

DTO 权威位于 `crates/token-pulse-core/src/protocol.rs`，`npm run contracts` 从 Rust 生成 `ui/src/shared/generated/contracts.ts` 和 `schemas/protocol-v1.json`；CI 通过 `contracts:check` 检测漂移，不允许分别手写两端契约。JSON Schema 用序列化契约生成，包含必需的 nullable 字段。生成依赖使用 [ts-rs](https://docs.rs/ts-rs/latest/ts_rs/trait.TS.html) 和 [schemars](https://docs.rs/schemars/latest/schemars/)。

## 工程边界

数据库结构当前为 v2。升级既有 v1 库前，SQLite Online Backup 保存一致副本到应用数据目录的 `migration-backups/`，同时保存数据库 SHA-256 manifest；校验通过后才在事务中升级。首次空库直接初始化到当前版本，重开当前版本不重复备份。未知版本、checksum 不符或升级失败保留原库，不创建零历史覆盖。2026-10-02 用户取消 M14，不再实施用户备份管理、备份恢复与数据清除页面；迁移保护和专门故障恢复也不再追加开发或专项验收；上述既有内部代码与已完成测试仅记录现状，不作为新增待办。

`ui/` 为正式前端；`prototypes/` 保留设计原型，不进入生产包。`token-pulse-core` 无窗口依赖；`token-pulse-store` 仅写应用数据；`src-tauri` 装配生命周期与受限 IPC。主窗口自定义命令在 AppManifest 和 capability 中枚举，并在后台检查窗口 label；无通用 shell、SQL 或前端文件读写权限。

关闭窗口隐藏到托盘；托盘“打开统计”恢复窗口，“退出 TokenPulse”结束进程。单实例只激活统计窗口，忽略第二实例的其他参数。


## 复用本地已登录账户的显式验收

账户能力使用已有 Codex Home 登录状态，设计见[账户专题](../design/account-quota.md)和[统一开发设计](../design/development-design.md)。TokenPulse 不新增登录流程；所选 Home 不可用时改选已有登录 Home。生产应用通过设置页选择 / 保存程序与 Home 后连接，不依赖这个开发 example。

只有用户明确要求检查真实本地账户时运行以下可选验收；替换为已经选择的本机原生程序和已有 Home。该 feature 不属于默认测试 / CI，也不应放入自动验收脚本。只发送账户状态与额度读取，正常断开退出；验收程序本身仅输出净化后的额度 DTO 字段，不输出身份 / 路径 / 原始认证或服务消息；Cargo 的运行提示会显示传入的程序和 Home 路径。

```powershell
cargo run -p token-pulse-quota --features local-account-check --example local_account_check -- --read-existing-account 'C:\实际安装目录\codex.exe' 'C:\已登录的CodexHome'
```

该命令需要已支持 App Server 的本地程序和可用登录态；没有可用额度返回相应状态与非零退出码，不创建登录作业。单次成功不证明后台通知、跨账户、完整冷启动或其他系统版本已验收，实际结果见[交付记录](delivery-status.md)。

## 任务栏独立宿主开发验证

```powershell
cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host
cargo test -p token-pulse-taskbar
cargo run -p token-pulse-taskbar --example export_host_contract -- --check
cargo clippy -p token-pulse-taskbar --all-targets -- -D warnings
```

Windows 测试自动启动 Cargo 构建的独立宿主，使用本次随机命名管道和合成协议，验证 DACL / PID / 正常及异常退出；心跳期限检查等待真实 15 秒。这是功能检查，不是性能测试。默认测试不读取真实账户、日志或统计库，也不调整 Explorer。当前宿主已具备隐藏控制窗口、原生文字与画布；生产管道尚未启用 Explorer 布局。不要手工传入或记录 nonce 参数。正式 Tauri 管理器与安装位置解析随后接入，独立程序将随应用打包，生产不依赖 Cargo 或开发环境。

可显式运行以下只读探测，不调整系统布局、不读取窗口标题或账户数据：

```powershell
cargo run -p token-pulse-taskbar --example inspect_taskbar -- --inspect-taskbar
```

输出仅含系统 build、DPI、任务栏区域与纯候选计划；能力不足返回错误，不伪装成功。当前只接受 Win10 build 19045；Win11 适配随后单独落实。默认 tests 包含实际自有隐藏窗口 / 合成系统消息路由，以及四档合成几何；不会发送系统广播、切换 DPI 或重启 Explorer。

## 原生任务栏文字与开发视觉检查

宿主已创建自有读数子窗口并按真实 DTO 准备文字 / 系统字体布局，默认隐藏。以下显式开发命令只使用仓库合成夹具，不访问真实账户 / 日志，不改变任务栏：

```powershell
cargo run -p token-pulse-taskbar --example render_taskbar -- --render-development-fixtures
```

原生 GDI BMP 输出在被 Git 忽略的 test-results/taskbar-native-visual，文件名及 README 明确 DEVELOPMENT-FIXTURE。默认应用不链接这些值，正常测试不生成视觉文件；使用图像查看器检查，无需 Python / Pillow。实际绘制代码与宿主共用，但图片不证明系统任务栏嵌入、实际账户或物理 DPI 切换已经验收。

## 显式 Windows 任务栏实际布局验收

以下开发命令会短暂调整已支持的 Win10 19045 主任务列表宽度，显示合成读数约 5 秒，更新隐私，然后验证禁用、重挂接和正常析构恢复原布局。仅在需要实际原生验收时执行，默认 tests / CI 不执行，也不重启 Explorer、不读取账户或日志。

```powershell
cargo run -p token-pulse-taskbar --example check_taskbar_layout -- --native-taskbar-development-check
```

该独立开发程序只截取其自身已验证的原生读数窗口，生成 test-results/taskbar-native-attachment 下两张 DEVELOPMENT-FIXTURE BMP 和说明，不截取桌面、系统控件或其他应用。检查原生文字、背景、费用估算与隐私清除；矩形使用物理 DPI 上下文。程序显式禁用和正常作用域析构都执行条件恢复；强制杀进程不等于正常析构，独立的父端恢复检查见下文。此次本机实际 150% DPI 证据见[交付记录](delivery-status.md)，不代表 Win11 / 所有 DPI / 多屏或系统按钮交互全部通过。

## 显式已结束宿主的父端清理验收

```powershell
cargo run -p token-pulse-taskbar --example check_taskbar_exit -- --native-taskbar-exit-development-check
```

该开发程序启动自己编译出的子进程，先加入自有 Job 再通过私有 stdin 允许挂接。子进程只显示明确合成的任务栏夹具，父端关闭 Job 强制结束它，没有发送正常退出 / 禁用；随后使用所持 Child 内核句柄与本实例记录条件恢复。检查存活宿主、错误实例和其他已结束进程句柄不能恢复该区域；正确恢复后重复返回 NoRecord，新的自有实例可挂接并正常退出。只终止所创建的进程，不重启 Explorer、不读取真实账户 / 日志、不生成真实账户截图，默认 tests / CI 不执行此命令。

本机 Win10 19045 / 实际 150% DPI 通过。Job 强制终止也可能返回退出码 0，因此按内核句柄结束状态及仍存留的归属记录判断，不用“退出码非零”代替异常清理证明。该验收让父端保持运行；主进程自身异常退出与清理期限的补充验收见下文，不扩大到已取消的数据恢复专项。

## 显式父进程异常退出与隔离清理验收

```powershell
cargo run -p token-pulse-taskbar --example check_taskbar_guardian -- --native-taskbar-guardian-development-check
```

开发程序创建专用父进程、挂接合成读数的子进程及使用同一原生实现的独立清理进程。清理 armed 后才允许子进程挂接；外层仅强制结束自己创建的父进程，不调用正常析构。父进程的 Job 随之关闭，独立清理进程用持有的宿主内核句柄条件恢复，检查原任务栏全部几何一致及明确 Restored 结果。本机 Win10 19045 / 150% DPI 已通过；随后创建一次性模拟阻塞清理进程，检查 5 秒期限返回 CleanupTimeout。这是功能期限检查，不是性能测试，也未实际挂死 Explorer。

默认 tests / CI 不运行此布局检查、不读真实账户或日志、不重启 Explorer。正式原生启动器已使用相同 armed 流程，但生产 wire 仍不启用挂接，正式配置 / 状态与管理器下一步接入。清理进程是同一宿主可执行文件的受限模式，生产包不依赖此 example、Cargo 或当前对话。外部终止全部进程树及强杀清理进程不在本次成功证据内。
