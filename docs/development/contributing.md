# 贡献与开发指南

欢迎通过 [Issues](https://github.com/LiuYongXinn/token-pulse/issues) 和 Pull Request 改进 TokenPulse。本文是新贡献者的环境与流程入口；专项原生验收和历史记录见[本地开发说明](local-development.md)及[交付记录](delivery-status.md)。

## 开始之前

当前发布代码在 `codex/release/1.0.0`，分支名称保留，实际应用版本以版本配置为准。修改当前发布功能时从这个分支开始，并在 Pull Request 中说明目标分支；不要把默认分支的旧代码误认成发布包源码。已发布版本的精确源码使用对应 `v<版本>` 标签定位。

建议先阅读[详细开发设计](../design/development-design.md)及涉及模块的文档。提交问题或功能建议时描述具体场景、复现步骤与预期行为；较大的数据模型或平台改动先说明方案。

## Windows 开发环境

| 工具 | 当前要求 |
| --- | --- |
| Git | 用于源码与提交管理 |
| Node.js | 使用 Node 22，版本至少 22.12；依赖由 `package-lock.json` 锁定 |
| PowerShell | PowerShell 7，可通过 `pwsh` 启动 |
| Rust | `rust-toolchain.toml` 固定 1.98.1，包含 rustfmt、Clippy 和 x64 MSVC target |
| C++ 工具 | Visual Studio Build Tools 的桌面 C++ 工作负载和 Windows SDK |
| WebView2 | 桌面运行所需的 Microsoft Edge WebView2 Runtime |

系统依赖的安装入口见 [Tauri Windows 前置条件](https://v2.tauri.app/start/prerequisites/#windows)。首次构建需要网络获取工具链和依赖；只有缓存齐备后才能使用 `--offline`。

## 检出与本地工具目录

在 PowerShell 7 中执行，检出路径需位于可写本地目录：

```powershell
git clone --branch codex/release/1.0.0 https://github.com/LiuYongXinn/token-pulse.git
cd token-pulse
. .\scripts\project-env.ps1
npm ci
```

环境脚本把 Cargo、Rustup、npm、Playwright 缓存和临时目录配置到仓库 `.local/`，只改变当前进程环境。直接运行 Rust 命令前也应加载此脚本。

新检出仓库的 `.local/tools/` 是空的。若已安装 `rustup`，在加载环境后执行以下命令，将项目所需工具链安装到上述本地目录：

```powershell
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
rustup target add x86_64-pc-windows-msvc --toolchain 1.98.1
```

如果还没有 Rustup，可从 [Rust 官方安装入口](https://rustup.rs/) 下载 Windows 安装程序；先加载项目环境，再运行安装程序。首次安装时检查 `CARGO_HOME`、`RUSTUP_HOME` 指向项目 `.local/tools/`，按官方说明选择 MSVC 工具链。详细安装选项见 [Rustup 安装说明](https://rust-lang.github.io/rustup/installation/index.html)。

## 运行与构建

| 命令 | 作用 |
| --- | --- |
| `npm run dev` | 在 `http://127.0.0.1:1420` 启动浏览器界面预览 |
| `npm run tauri:dev` | 启动桌面开发版，使用独立开发数据目录 |
| `npm run typecheck` | 检查前端类型 |
| `npm run build` | 类型检查并生成前端生产产物 |
| `npm run tauri:build` | 构建前端、原生任务栏宿主、第三方声明和 NSIS 安装包 |
| `npm run contracts:check` | 检查生成的应用契约是否与 Rust 定义一致 |

浏览器预览不读取真实桌面数据。开发版位于 `.local/data/dev/`，正式版位于 `.local/data/release/`。`.local/` 包含私有状态、数据库和本地工具，不应提交。

正式打包使用 `npm run tauri:build`，以确保主程序与同版本宿主成对构建。安装包构建不等于正式发布；签名和公开上传流程见[发布与更新包](releases.md)。

## 验证修改

根据改动运行相关检查。纯文档修改检查 Markdown、相对链接、图片和事实准确性；不需要为文案变更重跑安装器或操作用户真实数据。

前端修改的常用检查：

```powershell
npm run build
npm run test
node scripts/project-run.mjs playwright install chromium
npm run test:e2e
```

浏览器安装与测试都通过项目运行器，保证 `PLAYWRIGHT_BROWSERS_PATH` 一致。Rust 修改的常用检查：

```powershell
. .\scripts\project-env.ps1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
npm run contracts:check
cargo run -p token-pulse-taskbar --example export_host_contract -- --check
```

修改 DTO 时同步应用契约、原生宿主契约及相关客户端；新增桌面命令时同步 Tauri 命令清单和 capability。浏览器模拟与编译检查不能验证实际 WebView 权限。

需要真实窗口、任务栏和权限验证时，按专项说明运行原生检查，例如 `npm run test:native`。这些场景可能显示窗口并涉及桌面交互，执行前确认适合当前工作环境；合成数据、自动输入和真实系统验收分别记录，不混写为全部通过。

## 提交与文档约定

- 完成一个小功能模块就提交一次，不将多个模块积攒到同一提交。
- 保持改动围绕具体问题，Pull Request 说明行为变化、原因和实际验证结果。
- 使用本地临时或合成夹具验证，不修改真实 Codex 日志、认证文件或用户数据库来构造结果。
- 根 `README.md` 放项目介绍与快速开始；`docs/README.md` 放现有文档索引。
- `docs/requirements/` 放目标与验收，`docs/design/` 放实现设计，`docs/development/` 放使用、开发和维护说明。
- 文档使用小写英文短横线文件名；主题直接放在三个分类目录中，新增或移动时更新索引。图片统一放 `docs/images/`。
- 设计计划与已交付功能、自动检查与真实系统验收保持清楚的证据边界。

本项目的许可证声明为 [MIT](../../LICENSE)。新增第三方依赖应保留其原始许可证并更新声明生成所需资料；正式安装包的第三方声明由构建流程生成。
