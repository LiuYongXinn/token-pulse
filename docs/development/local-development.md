# 本地开发与运行

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

开发标识对应 `%LOCALAPPDATA%\com.tokenpulse.desktop.dev`，发布标识为 `com.tokenpulse.desktop`。开发、发布的数据、WebView 与单实例命名空间独立。所有 Codex 来源仍只读；M01 尚未读取任何来源。

## 桌面运行壳验收

```powershell
npm run build
cargo build -p token-pulse-desktop --features custom-protocol
& .\target\debug\token-pulse-desktop.exe --native-smoke
```

debug 专用 probe 在真实 Tauri / Win32 运行时中验证冷启动、开发目录隔离、托盘注册、关闭转隐藏、第二实例激活、明确退出。成功输出 `NATIVE_SMOKE_OK` 并返回 0；失败输出错误并返回 1。应用内置静态前端，不依赖 Vite 服务。该入口不进入 release 构建。

自动 probe 不替代人工点击托盘、屏幕阅读器、DPI、锁屏 / 休眠或 Explorer 重启验收。后续功能逐模块补充相应原生验证，不能将浏览器检查记为系统验收。

DTO 权威位于 `crates/token-pulse-core/src/protocol.rs`，`npm run contracts` 从 Rust 生成 `ui/src/shared/generated/contracts.ts` 和 `schemas/protocol-v1.json`；CI 通过 `contracts:check` 检测漂移，不允许分别手写两端契约。JSON Schema 用序列化契约生成，包含必需的 nullable 字段。生成依赖使用 [ts-rs](https://docs.rs/ts-rs/latest/ts_rs/trait.TS.html) 和 [schemars](https://docs.rs/schemars/latest/schemars/)。

## 工程边界

`ui/` 为正式前端；`prototypes/` 保留设计原型，不进入生产包。`token-pulse-core` 无窗口依赖；`token-pulse-store` 仅写应用数据；`src-tauri` 装配生命周期与受限 IPC。主窗口自定义命令在 AppManifest 和 capability 中枚举，并在后台检查窗口 label；无通用 shell、SQL 或前端文件读写权限。

关闭窗口隐藏到托盘；托盘“打开统计”恢复窗口，“退出 TokenPulse”结束进程。单实例只激活统计窗口，忽略第二实例的其他参数。
