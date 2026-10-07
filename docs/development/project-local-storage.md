# 项目目录内的数据与工具缓存

TokenPulse 的可写数据集中在项目目录内。当前仓库位于 `E:\Documents\Code\token-pulse`，无需修改 Windows 的全局临时目录。`.local/` 已加入 Git 忽略规则，包含真实数据库、通知登记及签名私钥，不能提交、上传或作为源码归档的一部分。

| 路径（相对于项目根目录） | 用途 |
| --- | --- |
| `.local/data/release/` | 正式版数据库、SQLite WAL/SHM、迁移备份、通知登记、WebView 数据 |
| `.local/data/dev/` | 开发版数据及独立原生测试场景 |
| `.local/app/` | 此机器上迁移后的已安装程序 |
| `.local/tmp/` | 子进程、测试及更新安装包的临时文件 |
| `.local/tmp/migrated/` | 从旧系统临时目录迁入的 TokenPulse 验收记录及安装包 |
| `.local/tools/cargo/` | Cargo 代理程序、注册表源码及下载缓存 |
| `.local/tools/rustup/` | Rust 工具链及组件 |
| `.local/cache/npm/` | npm 包缓存及 npm 日志 |
| `.local/cache/npm/legacy/` | 迁入的历史 npm 缓存，避免覆盖当前缓存索引与日志 |
| `.local/cache/playwright/` | Playwright 浏览器 |
| `.local/cache/python/` | 项目子进程的 Python 字节码缓存 |
| `.local/cache/idea/IntelliJIdea2025.3/system/` | 本机 IDEA 索引、缓存、本地历史及日志 |
| `.local/config/idea/IntelliJIdea2025.3/` | 本机 IDEA 配置、项目任务上下文及用户插件，保留原有权限 |
| `.local/secrets/release-signing/` | 当前用户权限保护的本地发布签名材料 |
| `target/.tauri/` | NSIS 等安装器构建工具，Tauri 原生支持的项目内缓存 |

## 运行与开发

`npm run dev`、`npm run build`、`npm test`、`npm run test:e2e`、`npm run tauri:dev`、`npm run tauri:build` 和契约生成命令均经过 `scripts/project-run.mjs`。运行器设置子进程的 Cargo、Rustup、npm、Playwright、Python 缓存与临时目录，并使用绝对路径，避免工作目录改变导致数据写出项目。

直接从 PowerShell 使用 Rust 等工具前执行：

```powershell
. .\scripts\project-env.ps1
cargo test --workspace --locked --offline
```

项目的 PowerShell 开发和验收脚本会自行加载该环境。`.cargo/config.toml` 也为直接启动的 Cargo 测试设置项目内临时目录。`.npmrc` 将直接执行的 npm 安装缓存设在项目内。

桌面程序根据自身可执行文件向上定位仓库，主进程与无界面通知进程使用同一套路径规则。安装在 `.local/app/` 和构建在 `target/debug/`、`target/release/` 时，都使用当前项目的 `.local/data/`。也可通过 `TOKENPULSE_PROJECT_ROOT` 明确指定绝对根目录。程序在建立 WebView 前覆盖全部 Tauri 应用数据目录，不会回退到 AppData。Windows 下拒绝 C 盘、系统盘、网络路径和重解析到系统盘的数据位置。

Windows 程序入口在启动 Tauri、通知运行时和工作线程之前，将自身的 `TEMP`、`TMP`、`TMPDIR` 设置为 `.local/tmp/`。从快捷方式双击启动时也会执行，因此 WebView 和程序启动的子进程不会继承默认的 C 盘临时目录。

脱离仓库的便携程序将 `.local/` 放在可执行文件旁，因此必须放在可写的数据盘。更换仓库位置时，需要一并移动 `.local/`，并重新加载开发环境；用户环境中持久化的工具路径也需相应更新。

## 现有数据迁移

迁移前关闭 TokenPulse，并等待 Rust 构建退出：

```powershell
pwsh -NoProfile -File scripts/migrate-local-data.ps1 -CopyOnly
pwsh -NoProfile -File scripts/migrate-local-data.ps1
```

第一步逐文件复制并校验 SHA-256，保留源数据。第二步再次核对后只删除白名单中的明确源目录。目标存在不同数据、源路径被重解析或应用仍在运行时会拒绝迁移。Cargo 临时锁和可重建的全局缓存元数据不迁移；通知登记及签名材料保留原有权限。

旧 Windows 临时目录中的 `tokenpulse*`、`token-pulse*`、`token_pulse*` 和 `token.pulse*` 均会迁入 `.local/tmp/migrated/` 并校验。随机命名的 Rust `.tmp*` 目录仅在其中全部内容为 `token-pulse.db` 及其 WAL/SHM 文件时迁移，并先检查数据库未被占用。若只清理这些历史临时数据，可执行 `pwsh -NoProfile -File scripts/migrate-local-data.ps1 -TempOnly`，无需关闭当前程序；该模式不会迁移正在使用的数据库或工具链。迁移后仅在原 `.tokenpulse` 签名父目录为空时删除它。

历史 npm 缓存若位于其他目录，可通过 `-ToolsOnly -LegacyNpmCache '原缓存绝对路径'` 迁入 `.local/cache/npm/legacy/`。同样先加 `-CopyOnly` 校验，再执行实际迁移；源目录必须包含 npm 的 `_cacache`，迁移目标不会覆盖当前缓存。

迁移现有 NSIS 安装后，可直接编译并更新程序，不生成安装包。关闭 TokenPulse 后执行：

```powershell
. .\scripts\project-env.ps1
cargo build -p token-pulse-desktop --release --features custom-protocol --locked --offline
pwsh -NoProfile -File scripts/refresh-local-app.ps1
```

刷新脚本要求已有正式数据库和迁入的 `uninstall.exe`，按 Tauri 规则保留原安装类型标记，并验证程序其他字节与编译结果完全一致。脚本同步当前用户的安装登记、原有快捷方式，以及仍属于 TokenPulse 的 Codex 通知命令和恢复记录；用户自行更改的通知命令不覆盖。旧验收记录内的历史绝对路径保留原文。

本配置约束 TokenPulse 和本项目启动的工具写入。Windows 注册表、安装快捷方式、系统级 WebView2 运行时由 Windows 管理；Codex Home 与源会话日志属于外部数据源，不能作为项目缓存整体移动。

## 本机 IDEA 数据

本机用户已选择将 IntelliJ IDEA 2025.3 的缓存迁入项目。项目任务上下文原本位于 IDEA 配置目录，因此配置、插件和上下文也一起保留并迁入 `.local/config/idea/IntelliJIdea2025.3/`，避免继续写回原 C 盘目录。

正常关闭 IDEA 后，执行 `pwsh -NoProfile -File scripts/migrate-idea-data.ps1`；可先加 `-CopyOnly` 仅复制校验。脚本逐文件验证后清除两个明确的旧目录，保留配置权限，并通过当前用户的 `IDEA_PROPERTIES` 指向项目内的 `idea.properties`。配置中分别设置 `idea.config.path`、`idea.system.path`、`idea.plugins.path`、`idea.log.path` 和 `java.io.tmpdir`，保留其余自定义属性。原有自定义 `IDEA_VM_OPTIONS` 不修改。

该选择影响同一 IDEA 实例打开的其他项目，属于本机用户设置；其他协作者不需要执行。移动仓库后必须同步修改 `IDEA_PROPERTIES` 和上述绝对路径。配置目录含凭据与私有插件状态，随 `.local/` 忽略，不能提交或上传。路径配置使用 [JetBrains 官方支持的目录属性和自定义属性文件](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html)。
