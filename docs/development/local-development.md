# 本地开发与运行

离线价格目录内嵌于 core；更新事实时开发者执行 `node scripts/update-offline-prices.mjs --write YYYY-MM-DD openai-text-VERSION`，脚本只读官方公开 Markdown，并强制审查输出差异后再提交。应用启动不使用 Node 或联网抓价格。脚本保留十进制字符串，各档位直接引用官方表，不用折扣推算其他模式。M09g1a 的 store 发布暂未接生产启动，条件匹配 / 正式目录界面继续实施；自动检查使用合成更新、Writer 故障及实际 SQLite 快照，不读取用户日志或账户，不运行性能测试。

M09g1b 已接正常应用启动和主窗口正式目录。运行 `pwsh -NoProfile -File scripts/native-smoke.ps1 -OfflinePrices`：在隔离库走正常目录启动发布，实际 main / mini WebView 验证 172 事实 / 37 可用规则、历史 null / 非法修订、搜索 / 模式 / 历史切换、mini 拒绝、共享隐私清空与重开新查询、重复发布不增修订和消费不变。Win10 19045 / 150% DPI 场景打印 NATIVE_OFFLINE_PRICES_OK，退出 0；WebView2 1412 保留。普通 debug / release 独立应用都使用内置目录；其他原生夹具仍保留显式空价格，避免混入市场价。定向浏览器 `npx playwright test tests/ui/offline-prices.spec.ts tests/ui/prices.spec.ts` 6 项通过；截图位于忽略的 test-results/offline-prices-*.png，合成 bridge 与公开事实用于检查界面，不冒充生产账户 / 用量。

M09g2a 的 `cargo test -p token-pulse-store valuation --lib` 使用合成价格 / 事件和真实 SQLite，8 项通过：持久重开 / 检查点保持、超 JS 安全整数的精确金额、估价模式 / 时点隔离、旧真实快照、镜像证据 / 来源筛选、跨 500 行事务取消、候选过期 / 新事件即时计算、发布失败保留旧缓存。601 事件只用于跨批次功能检查，无性能测量或报告。schema v5 的普通 store lib 为 179 通过 / 1 性能夹具 ignored；store / desktop strict Clippy、fmt 通过。缓存构建暂为内部 API，实际 Windows 目录场景仅是启动 / 既有 IPC 回归，不能证明正式后台重估 UI 已完成。

当前交付环境为 Windows 10 / 11 与本地来源，多屏与各档 DPI 兼容继续验收。macOS、WSL / 网络来源、开机启动、额外快捷键及旧格式自动重解析不再新增；诊断简化、安装打包做简单版，notify、自动更新与完整计价功能保留。详见[已确认范围](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)。

实现依据：[开发总入口](../design/development-design.md)、[实施计划](implementation-plan.md)。当前进度与真实验收结果见[交付记录](delivery-status.md)。

## 环境

M09g2b1：`cargo test -p token-pulse-store revalue_jobs` 8 项通过，`cargo test -p token-pulse-store --lib` 188 项通过 / 1 性能夹具 ignored。新增价格作业测试仅用合成日志事实与临时 SQLite；另验证缓存构建期间真实价格发布仍保持捕获版本。契约通过 `npm run contracts` 生成，独立执行线程 / 正式界面尚待接入，暂无此模块的实际 Windows UI 验收。

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

## 显式正式宿主管道配置与状态验收

```powershell
cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host
cargo run -p token-pulse-taskbar --example check_taskbar_wire -- --native-taskbar-wire-development-check
```

该开发程序启动上一步构建的正式原生宿主及清理监督，经当前用户受控管道发送合成 DTO 和显式配置。实际验证 waiting_snapshot → embedded、单行仅 Token 后读数空间缩小、新修订清除旧快照、隐私清屏 ACK 后任务栏恢复、重新显示与禁用，以及最后完整原几何恢复 / 前台焦点保持。只检查本次宿主 PID 与自有读数类的矩形，只在鼠标命中检查被其他窗口阻挡时输出命中窗口类 / 标题 / PID / 矩形用于定位，不截取其他应用、不使用真实账户或日志；默认 tests / CI 不运行布局验收。

本机 Win10 19045 / 150% DPI 已通过。生产 wire 具备显式启用能力，Tauri 管理器尚未调用它，普通应用保持默认禁用。该检查不代替原生鼠标、悬停 / 菜单、Win11、Explorer 重建或物理多屏 / 各档 DPI；没有运行性能测试。

## 显式正式应用任务栏管理器验收

```powershell
npm run build
pwsh -NoProfile -File scripts/native-smoke.ps1 -Taskbar
```

`-Taskbar` 选择独立任务栏场景，使用 debug 专用 `native-probe-<UUID>` 数据目录、真实两个 WebView、正式宿主及 SQLite DTO。初始数据库无来源，未知用量 / 额度保留未知，不读取已有开发来源或真实账户；通过主窗口命令保存启用配置，并验证实际 embedded、mini 命令拒绝、共享隐私屏障、主窗口隐藏后最后成功快照时间继续推进、合成休眠消息 / 恢复、禁用以及嵌入时关闭后台服务。最后原任务栏几何恢复，输出 NATIVE_TASKBAR_MANAGER_OK 并以 0 退出；默认 tests 和不带 -Taskbar 的综合原生场景不调整任务栏。

本机 Win10 19045 / 150% DPI 通过。电源检查仅向自有主窗口发送合成消息，不代表机器实际休眠；未做系统按钮实际点击、Win11 或完整物理多屏 / DPI。关闭时 WebView2 可能输出 Chrome_WidgetWin_0 注销错误 1412，本次场景断言及进程退出均成功，记录此诊断而不将其隐藏。当前综合原生回归另有真实键盘恢复失败，尚需复核，不能用任务栏独立通过代替综合回归通过。未运行性能测试。

M13d4 为 -Taskbar 增加真实设置页状态及保存关闭检查：通过实际 WebView DOM 事件操作正式 React 页面，读回 SQLite 偏好并核对原任务栏几何。这不替代原生鼠标与菜单验收。定向前端检查：`npx playwright test tests/ui/taskbar.spec.ts tests/ui/shell.spec.ts tests/ui/jobs.spec.ts --workers=1`；任务栏视觉截图在忽略的 test-results 目录中，仅为显式合成 DTO 检查。


M13e1 的 -Taskbar 场景新增非激活回退：使用本应用已明确拒绝的位置配置触发，不改系统任务栏形态；销毁本测试自有 mini 后自动创建，核对前台 HWND 与交互样式，再经正式命令隐藏 / 重试及设置 UI 关闭回退，验证不重复弹窗且恢复嵌入不关闭小窗。这里验证的是实际主端回退与原生窗口，尚不代替 Win11 / 实际拥挤 / 宿主异常及全兼容矩阵。


M13e2a 的 check_taskbar_wire 增加真实 SendInput 单击 / 双击、自有目标命中检查、同修订刷新保留动作和配置 / 隐私清除动作。只有确认 WindowFromPoint 属于本次宿主读数才发送鼠标输入；光标位置在结束 / 失败时恢复。须在已解锁且可交互桌面运行，系统面板 / 锁屏遮挡时拒绝输入并失败，不能改用合成消息称为鼠标验收。本轮锁屏覆盖 / 前台 HWND 异常使整场未通过；配置 / 隐私屏障的待发动作使用明确标记的窗口消息另行检查，不混同实际鼠标。


## 显式正式任务栏窗口动作通路验收

```powershell
npm run build
pwsh -NoProfile -File scripts/native-smoke.ps1 -TaskbarActions
```

该独立场景使用隔离无来源 / 无账户数据库与真实两个 WebView，向本次后台所拥有 PID / 自有读数类的窗口发送明确标记的鼠标消息，然后通过正式宿主 pipe / 后台执行窗口操作。实际核对 360×380 DIP 展开 / 持久状态 / 已有 WebView 刷新、双击同范围 / 实际时区统计且不打开 mini、shutdown 全部原几何恢复。本机 Win10 19045 / 150% 已通过，输出 NATIVE_TASKBAR_ACTIONS_OK、退出 0；仍有 WebView2 退出 1412 诊断。

这是自有消息通路的原生窗口检查，不能作为真实鼠标命中、锁屏时可点击、焦点保持、悬停或菜单的证据。真实 SendInput 场景仍使用上方 check_taskbar_wire，在被锁屏覆盖时拒绝输入；两项记录分开。开发鼠标检查光标保存 / 恢复使用明确的物理坐标 API，不混入线程 DPI 虚拟化坐标。
M13e3 扩展上述 -TaskbarActions：实际弹出独立宿主的标准 Windows 菜单，核对 PID / 菜单归属及文字，再用明确标记的自有 WM_CHAR 助记消息选择小窗、统计、设置、隐私和隐藏；核对共享隐私与 SQLite 持久关闭。重新启用后，在菜单打开时提交主窗口隐私，确认菜单结束 / 未产生导航；再次打开菜单后 shutdown，验证模态循环内退出不悬挂且原全部几何恢复。新增 NATIVE_TASKBAR_MENU_OK，仍须最终 NATIVE_TASKBAR_ACTIONS_OK / 退出 0 才算场景通过。

本机 Win10 19045 / 150% 已通过，仍有 WebView2 退出 1412。先前自有 Home / Down / Enter 消息未改变原生菜单选中项，改为助记字符做通路验证；真实鼠标、方向键、入口键盘可达性、焦点与屏幕阅读器分别待交互桌面验收。此场景不读取日志或登录凭据，不更改系统 DPI，不重启 Explorer。定向前端导航检查：npx playwright test tests/ui/overview.spec.ts tests/ui/taskbar.spec.ts，共 16 项；未运行性能测试。

M13e4a 的定向检查：cargo test -p token-pulse-taskbar --test details --test wire 和 cargo test -p token-pulse-store --lib settings::taskbar；宿主 schema 使用 cargo run -p token-pulse-taskbar --example export_host_contract 生成，附加 -- --check 核对漂移。详情全部合成夹具，独立预期验证完整整数 / 部分分项 / 精确金额 / 有界整数覆盖率 / 实际周期 / null 与零 / 到期与隐私；不接真实日志或账户。生产快照新增详情字段后仍以 -TaskbarActions 回归正式双进程通信；该场景没有悬停面板，不作为可见悬停验收。没有新增前端布局或执行性能测试。

M13e4b 原生详情定向检查：cargo test -p token-pulse-taskbar --lib；全套用 cargo test -p token-pulse-taskbar --all-targets。四档 DPI 使用系统字体和合成 DTO，独立检查完整字符 / 数值、布局边界、0 / unknown 条、隐私 / 主题，以及自有 HWND 的显示 / 滚动 / 清屏像素 / 销毁；不调整系统 DPI。可显式设置 TOKENPULSE_DETAILS_VISUAL_DIR 为仓库 test-results/native-details 后运行 details_window 过滤测试，输出开发合成 GDI BMP；该环境变量仅由测试模块读取，生产宿主不写图像文件。

pwsh -NoProfile -File scripts/native-smoke.ps1 -TaskbarActions 现在还检查正式独立宿主详情窗，以自有焦点 / 翻页 / Escape 消息显示与关闭，再用真实主端命令作两次隐私切换 / 新输入核对内容，并回归菜单、窗口及退出原几何恢复。需同时有 NATIVE_TASKBAR_DETAILS_OK / NATIVE_TASKBAR_MENU_OK / NATIVE_TASKBAR_ACTIONS_OK 和退出 0。Win10 19045 / 150% 已通过；WebView2 注销 1412 仍记录，不是本场景断言失败。自有消息不替代物理悬停 / 键盘可达性 / 焦点和屏幕阅读器；实际自动隐藏、Explorer、Win11 / 物理多屏 DPI 后续单独验收，已有综合恢复键回归失败仍待复核。未运行性能测试。

M13e5a 的定向检查为 cargo test -p token-pulse-taskbar --lib control。普通测试仅销毁自有隐藏子窗口并由 Inspect / TakeActions 恢复，不启用任务栏嵌入或重启 Explorer。-TaskbarActions 增加实际嵌入子窗口丢失场景：只向本次宿主 PID / 类确认的读数发送 WM_CLOSE，在详情 / 待确认点击和菜单期间分别检查同宿主新类重建、旧意图消除、新详情 / 现有动作及最终原几何恢复。需追加 NATIVE_TASKBAR_RECREATE_OK；Win10 19045 / 150% 已通过，真实 Explorer 退出 / 重启与物理兼容矩阵另行验收。

## M13e6：按钮几何与两位置回归

只读检查：cargo run -p token-pulse-taskbar --example inspect_buttons -- --inspect-buttons。只读取经过身份和结构校验的主任务列表子矩形 / PID / 可见性，输出区域 / 数量，不读取名称、预留布局或操作其他窗口。探测在有界无窗口 MTA 中执行；不支持 / 超时如实失败。

定向自动命令为 cargo test -p token-pulse-taskbar --all-targets、cargo test -p token-pulse-desktop taskbar_service --lib、cargo run -p token-pulse-taskbar --example export_host_contract -- --check、npx playwright test tests/ui/taskbar.spec.ts；任务栏 release check 另核对 debug-only 夹具隔离。四档 DPI 与空间 / 线程阻塞均为合成独立预期，不调整系统 DPI 或实际阻塞 Explorer。

-TaskbarActions 增加真实设置页保存应用图标右侧、实际自有独立分组测试窗口按钮增减、8 DIP 间距及 320 DIP 最小应用区域核对、两位置来回切换；随后窗口 / 菜单动作保持该位置。只创建和关闭本测试自有窗口，关闭前移除 AppUserModelID 属性。输出 NATIVE_TASKBAR_APPLICATION_POSITION_OK 后，仍需 DETAILS / RECREATE / MENU / ACTIONS 全部 OK 和退出 0。显示 / 隐藏窗口影响任务按钮时，等待实际矩形和发布稳定后才投递自有测试消息，旧几何手势继续按生产策略清除。

-Taskbar 失败触发已从原不支持位置改为 debug-only 隔离目录的缺失宿主工厂，生产初始化不变、无外部配置入口、不改安装文件；通过正式启动失败验证非激活小窗 / 用户隐藏 / 重试 / 恢复保留。Win10 19045 / 150% 两场退出 0，WebView2 1412 保留。物理输入 / 焦点、实际拥挤 / 自动隐藏、完整 Explorer 重建、Win11 和物理多屏 DPI 仍待独立验收。没有性能测试。

## 显式模型别名 IPC 验收

pwsh -NoProfile -File scripts/native-smoke.ps1 -PriceAliases 选择 debug-only、无来源 / 账户的隔离库及两个真实 WebView，实际核对 main-only create / replace / retire / 旧版本、CAS / 映射冲突、提交成功通知、隐私投影和 mini 拒绝。Win10 已输出 NATIVE_PRICE_ALIAS_OK、退出 0，WebView2 注销 1412 保留。此检查不启用任务栏、不读取真实日志 / 登录信息，不等同设置编辑器 UI 验收。定向自动检查 cargo test -p token-pulse-core --test pricing 和 cargo test -p token-pulse-store --lib pricing 均使用合成单价 / 模型、独立预期费用及消费不变；没有性能测试。

M09f2 的 -PriceAliases 追加真实 React 表单事件，经正式 command / SQLite 完成新增、替换、历史只读、退休、外部发布后的原修订冲突及草稿保留，随后共享隐私清掉草稿，关闭后不恢复。成功通知现在精确检查 1–8 共八次；最终 NATIVE_PRICE_ALIAS_OK 和退出 0 为通过。本机 Win10 19045 / 150% 已通过，WebView2 1412 保留。运行前 npm run build；定向浏览器 npx playwright test tests/ui/prices.spec.ts，4 项通过，深 / 浅及 960 宽截图位于忽略的 test-results/model-alias-*.png，仅为开发合成 DTO 视觉检查。普通价格规则回归保持，未运行性能测试。
