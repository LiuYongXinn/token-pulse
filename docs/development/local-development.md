# 本地开发与运行

## 签名更新提供方验证

原生提供方固定本仓库 GitHub Releases `latest.json`，发布安装器仅接受同仓库 HTTPS `.exe`，需要通过 `TOKENPULSE_UPDATER_PUBLIC_KEY` 在编译时提供 Tauri 公开验证密钥。该变量仅为公钥，不能填写私钥；当前未配置时应用不发更新请求、正式状态为更新不可用。私钥不进入源码、前端、安装包或运行时配置。签名必须绑定发布版本，禁止降级；真正发布、安装入口及 UI 尚在实施。

普通定向测试：`cargo test -p token-pulse-desktop --lib update_transport`。显式真实签名夹具：`cargo test -p token-pulse-desktop --lib update_transport::tests::signed_local_download_checks_bytes_version_and_missing_version -- --ignored --exact`，需要已安装的 Node / 本仓库 Tauri CLI。只创建临时夹具密钥及自有 loopback HTTP，stdout / stderr 密钥输出不打印；下载文件是合成非可执行文本，不运行安装器，也不访问真实发布 / 账户。覆盖正确签名、篡改文件、声明版本不符、缺签名版本及原生 owner 状态 / CAS。

真实 WebView 入口：`pwsh -NoProfile -File scripts/native-smoke.ps1 -Updates`；必须在未配置正式公钥的构建运行，UUID 隔离库不登记来源、不连接账户、不构建或启动任务栏宿主。验证三个主窗口命令、mini / 直接 updater 拒绝、null 状态、非法字段、共享隐私；通过标记为 NATIVE_UPDATES_IPC_OK 和退出 0。此入口没有下载 / 安装验收，不能代替正式更新。Windows lib test 链接器生成公共控件 v6 清单，desktop bin 使用已有 Tauri 资源清单，关闭另生成清单以避免重复；两类最终文件均需实际执行核对。

## 简单 Windows 安装包

根目录运行 `npm run tauri:build`，加载 `src-tauri/tauri.bundle.conf.json`，编译前端 / 原生宿主并生成唯一 NSIS currentUser 安装包。默认 x64 文件 `target/release/bundle/nsis/TokenPulse_<version>_x64-setup.exe`。不要用裸 `npx tauri build` 代替此正式入口；基础配置不要求 externalBin，保持新检出后的普通 Cargo 验证可运行，开发数据隔离不变。

prepare 使用 TAURI_ENV_TARGET_TRIPLE / TAURI_ENV_DEBUG，默认本机 target / release；通过 cargo metadata 读取真实 target_directory，尊重 CARGO_TARGET_DIR。只接受 Windows MSVC 目标，目标工具链须已安装。本次实际验收仅 x64，不将 aarch64 配置支持记为系统检查。

`pwsh -NoProfile -File scripts/verify-installer.ps1` 要求无正式安装、生产数据和应用进程，发现既有内容即拒绝。脚本安装至 UUID 临时目录，核对注册 / 文件、真实前端 UIA / SQLite、隐藏 / 单实例、托盘小窗 / 正常退出、冷启动和普通卸载。数据库保留，不递归清理 AppData；重复完整验收应使用另一个干净 Windows 测试用户 / 环境，不得为了重跑删除数据。

显式 `-OwnTrayCommands` 是桌面无法弹出菜单时的缩减验收：锁定 muda 版本与源码三项菜单创建顺序，只投递该进程自有托盘窗口有限命令。它不证明可见菜单或物理输入；默认场景要求真实标准菜单文本与 ID。映射变化时拒绝，不试探任意 ID。

Win10 19045 / 150% 已实际安装 / 独立运行。初轮验收脚本修正后分步完成缩减托盘命令、小窗、两个退出码 0 的冷启动及正常卸载，数据库字节不变；没有在另一个干净配置一次重跑最终脚本。WebView2 缺失场景、可见菜单、Win11 和签名更新分开待验收，当前产物未签名。细节见[交付记录](delivery-status.md#m16a单渠道-windows-安装包与实际独立启动)。

M15a7：`npx playwright test tests/ui/notify.spec.ts tests/ui/sources.spec.ts tests/ui/account-service.spec.ts --workers=1` 共 8 项（新增通知 3）通过；`npm run test -- ui/src/shared/runtime.test.ts ui/src/shared/privacy-runtime.test.ts` 共 7 项（新增错误说明 1）通过。TS / 生产构建、desktop all-targets strict Clippy / release check、fmt / diff 通过。`test-results/notify-dark-preview.png`（1280）与 `notify-light-preview.png`（960）已查看，属于明确合成测试画面，不是产品演示数据；构建后的 Win10 `native-smoke.ps1 -Notify` 通过实际设置按钮启用 / 停用、IPC / 隐私 / mini 权限及正式采集 3→10→11，退出 0，WebView2 1412 提示仍记录。系统 Home 选择器已接入口但未实际交互验收，真实用户 Home 没有修改，无性能测试。详见[交付记录](delivery-status.md#m15a7正式-notify-设置与差异确认)。

M15a6 定向：`cargo test -p token-pulse-core --test notify_integration` 新增 3 项；`cargo test -p token-pulse-desktop --lib notify_commands::tests` 新增 2 项；`npm run test -- ui/src/shared/contracts.test.ts` 共 11 项（新增 notify schema 1 项）。既有隐私 5 / manager 6 回归、TS / 契约漂移、core / integration / desktop all-targets strict Clippy（integration/test-fixture）、desktop release check、fmt / diff 通过。`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify` 输出 NATIVE_NOTIFY_IPC_OK 和 NATIVE_NOTIFY_COLLECTOR_OK / 退出 0：实际主 WebView 五命令 / 私有路径隐藏 / 写门禁 / 旧预览失效 / 条件启用撤销 / active 退休拒绝 / mini 权限和原采集 3→10→11。WebView2 1412 继续单列，Home 为隔离合成目录、无性能测试。系统选择器只接入 backend，尚未交互验收；正式设置 UI 待接。详见[交付记录](delivery-status.md#m15a6主窗口-notify-ipc与共享隐私门禁)。

M15a5：`cargo test -p token-pulse-integration --features test-fixture` 合计 63 项，新增管理器 7 项 / 退休 4 项；`cargo test -p token-pulse-desktop --test notify_headless` 5 项，新增已退休旧回调忽略 / 无 GUI / 无 orphan marker。两包 all-targets strict Clippy（integration/test-fixture）、release check、fmt / diff 通过。`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify` 改用实际操作管理器：只读预览 → 默认保留原命令 → 条件启用 → 正式采集 3→10→11 → 保留新设置的撤销 → 登记退休 / 监听 0 → 旧回调忽略，NATIVE_NOTIFY_COLLECTOR_OK / 退出 0，WebView2 1412 仍记录。未改真实 Home，未做性能测试。任务栏用户允许复测后再次遇到全屏 CoreWindow，发送输入前拒绝、退出 101；本次不计通过。正式 notify IPC / UI 仍待接入，TxF 限制不变。详见[交付记录](delivery-status.md#m15a5配置操作管理器与登记退休)。

M15a4：`cargo test -p token-pulse-integration --lib notify_config::windows` 新增 6 项（5 实际事务 / 1 有限错误码）；`cargo test -p token-pulse-integration --test notify_config_file` 新增 4 项实际文件 / ACL / 条件修改检查。完整 `--features test-fixture` 52 项、正式 headless 4 项通过；两包 strict Clippy all-targets、fmt / diff 与 release check 通过。`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify` 现在在隔离合成 Home 通过真正文件计划启用 / 撤销，已通过采集 3→10→11 / 原命令 / 暂停 / 重复 / 源只读 / 主窗隐藏、用户新设置保留、reload 后监听 0 与旧 headless 无 marker。退出 0，WebView2 1412 仍记录。配置写入 provider 当前需要可用的本地 NTFS 事务，System32 动态加载；不支持时返回有限不可用，不运行不安全覆盖降级。TxF 的 Microsoft 限制、Win11 / 其他卷未验证及正式设置未接入均明确留在[交付记录](delivery-status.md#m15a4配置文件的条件启用与保真撤销)。未修改真实 Home，没有性能测试。

M15b4 定向：`cargo test -p token-pulse-integration --features test-fixture --test notify_original` 新增 5 项独立预期 / 实际 Win10 子进程检查，包含确切 JSON 尾参数、输出丢弃、失败保留 wake、相对路径、拒绝 dispatcher / 同盘硬链接、5 秒功能超时及自有子进程树结束。全 integration `--features test-fixture` 42 项；正式 `cargo test -p token-pulse-desktop --test notify_headless` 4 项通过。`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify` 现明确保留合成旧 cmd.exe / 固定 `exit 0`，真实采集仍为 3→10→11、隐私内容不入采集 DTO / 持久登记，源和配置不变 / 主窗隐藏，退出 0。不是自动套 shell，不改真实 Home，test-only 原命令 fixture 不打包。两包 all-targets strict Clippy / fmt / release check 通过；WebView2 1412 提示仍记录。文件启用 / 撤销和设置 UI 待实现，未做性能测试。详见[交付记录](delivery-status.md#m15b4受控保留原-notify-与独立唤醒结果)。

2026-10-03 按用户允许再复测任务栏 wire：全屏 Windows.UI.Core.CoreWindow 遮挡自有前台夹具，FOREGROUND_FIXTURE_REFUSED / 退出 101；输入尚未发送，本次不计通过，M13e7 之前的通过证据保留。不要通过发送自有窗口消息冒充本次真实桌面复测成功。

M15b3：`cargo test -p token-pulse-integration --test notify_service` 新增实际 Win10 4 项通过，全 integration `--features test-fixture` 总 37 项；`cargo test -p token-pulse-desktop --test notify_headless` 3 项通过。`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify` 实际运行正式主程序 / 子进程 / owner / CollectorService / SQLite，在独立 AppData `native-notify-<uuid>` 下验证 3→10→11、暂停 / 重复、源只读和隐藏主窗保持，NATIVE_NOTIFY_COLLECTOR_OK / 退出 0。该场景明确关闭 watcher / 一小时轮询，只用补扫请求触发追加处理；只准备合成配置，不改真实 Home，不能视为配置编辑 UI 已验收。strict Clippy 两包 all-targets / fmt / release check 通过，无性能测试；仍有已知 WebView2 退出提示 1412。详见[交付记录](delivery-status.md#m15b3主进程-notify-owner离线消费与实际采集贯通)。

M15b2：`cargo test -p token-pulse-integration --test notify_invocation` 4 项通过；`cargo test -p token-pulse-desktop --test notify_headless` 3 项实际启动正式 exe。后者只在 debug / Windows 执行，使用 LocalAppData/com.tokenpulse.desktop.dev 下独立 `native-notify-<uuid>`，不会改真实 Codex Home 或启动正常 GUI；夹具 config 的安装数组由测试准备，不能记录为配置编辑器通过。integration `--features test-fixture` 合计 33 项、两个包 strict Clippy all-targets / fmt、`cargo check -p token-pulse-desktop --release` 通过。原命令 runner 未就绪时 chain=true 有限拒绝，正式启用入口尚未开放；后续接主进程补扫 / 离线消费、原命令、保真文件操作和 UI。详见[交付记录](delivery-status.md#m15b2正式-exe-的只读-headless-唤醒入口)。

M15a3：`cargo test -p token-pulse-integration --test notify_registry` 新增 9 项通过（契约 2、真实 Win10 临时文件 7）。全模块 `--features test-fixture` 共 29 项，strict Clippy all-targets / fmt 通过。测试创建隔离 temp 应用目录与合成 Home，私有文件使用实际 Win32 owner / DACL / 不覆盖改名，不读取真实 auth.json。登记不可变，离线标记零字节；文件上限 / 16 条登记上限 / 8 写入并发为功能限制验证，没有做性能测试。正式进程尚未使用该库，启用 / 撤销和 UI 继续接入。详见[交付记录](delivery-status.md#m15a3私有持久登记与并发离线唤醒标记)。

M15b1：`cargo test -p token-pulse-integration --features test-fixture` 共 20 项通过，其中 M15a2 配置 10 项、最小唤醒协议 4 项，另 5 项真正创建 Win10 管道并检查 DACL / 去重 / 异常连接 / 超时 / 停止重开 / 旧客户端句柄，以及 1 项启动独立 feature-only 子进程发送。`cargo clippy -p token-pulse-integration --all-targets --features test-fixture -- -D warnings` / fmt 通过。测试 capability 经 stdin 传递，不从终端输出认证 nonce，也不运行真实 Codex 回合或改 config.toml。300 / 500 ms 等待检查用于协议功能，未做性能测试。当前是独立通道库，还没有生产 exe headless / 采集器 / 持久登记 / 离线标记 / 正式 UI。详见[交付记录](delivery-status.md#m15b1当前用户专属的有界-windows-唤醒通道)。

M15a2 定向检查：`cargo test -p token-pulse-integration` 10 项纯配置预期通过；`cargo clippy -p token-pulse-integration --all-targets -- -D warnings` 检查新模块全部目标。配置计划不直接访问文件，测试不读取真实 Codex Home / auth.json。原数组语法 / 其他设置 / BOM / 行尾恢复、配置摘要冲突和归属冲突均明确断言；持久记录仅保留 notify，加载拒绝损坏与注入。当前没有正式启用入口，后续文件操作应在隔离 Home 验证，不能将这些纯检查记录为原生修改或整套 notify 验收。详见[交付记录](delivery-status.md#m15a2notify-配置保真计划与受控撤销)。

M15a1 的定向命令：`cargo test -p token-pulse-core --test notify`，5 项纯合成载荷与独立预期通过；`cargo clippy -p token-pulse-core --all-targets -- -D warnings`、`cargo fmt --all -- --check` 和 `npm run contracts:check` 检查编译 / 格式 / 契约漂移。本增量只有允许字段读取器，尚无可启用的 notify 设置或 headless 通道，不修改真实 config.toml，不运行真实通知 / 性能验收。后续保真配置 / 撤销与链式执行应在隔离 Home 验证，避免覆盖用户后续改动。

M13e7 更新显式 taskbar wire 场景。先构建 `cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host`，再运行 `cargo run -p token-pulse-taskbar --example check_taskbar_wire -- --native-taskbar-wire-development-check`。场景会打开一个明确标题的自有前台测试窗口，以真实 SendInput 激活，核对读数宿主 PID / 命中后实际悬停 / 单击 / 双击，结束后关闭测试窗口并恢复光标 / 线程 DPI / 任务栏矩形。须让桌面可交互，不能有系统面板遮挡或已有按键；失败拒绝向其他窗口发送输入。它不是默认 CI / 性能测试。

Win10 19045 / 150% DPI 最终退出 0，actual_hover / actual_single_double / passive_focus_preserved / geometry_restored 均 true。悬停 / 自动刷新与配置 / 隐私 / shutdown 严格保持非空前台且无失活；点击按设计产生打开其他应用表面的动作，之后由自有真实输入重新建立前台，不对显式点击错误要求“全程旧前台不变”。原空前台 0→0 不能证明焦点保持。完整详情使用明确合成 HostDetails，未知源扫描时间保持 null。详见[交付记录](delivery-status.md#m13e7真实任务栏悬停与点击的独立前台验收)。此前焦点失败及锁屏覆盖是历史记录；正式应用真实点击开窗 / 右键 / 键盘 / 辅助功能与兼容矩阵继续验收。

M11h 原生验收更新：综合 `pwsh -NoProfile -File scripts/native-smoke.ps1` 保留真实键盘 / 鼠标断言，不自动切换桌面或改用消息。输入桌面不可读取、应用桌面不匹配、无前台或已有键按下时明确失败；先解锁并释放按键再运行。本机用户解锁后综合 NATIVE_SMOKE_OK / 退出 0，收敛下方历史键盘失败记录。Win10 19045 / 150% DPI，账户仍为合成配置 / 快照，电源为自有消息；不等同真实账户持续刷新、实际休眠、Win11 或多屏验收。

独立 `pwsh -NoProfile -File scripts/native-smoke.ps1 -RecoveryRoutes` 只检查自有 HWND 的明确合成 WM_HOTKEY、实际注册 / 冲突 / Writer 回滚及透明度 / 两个 WebView；NATIVE_RECOVERY_ROUTES_OK / 退出 0 明确不包含键盘 / 鼠标输入验收。本机通过。相关定向命令：`cargo test -p token-pulse-core --lib shortcuts`、`cargo test -p token-pulse-store --lib settings::shortcuts`、`npx playwright test tests/ui/display-settings.spec.ts --grep recovery --workers=1`，分别 1 / 2 / 5 项通过；没有性能测试。

解锁后的 `cargo run -p token-pulse-taskbar --example check_taskbar_wire -- --native-taskbar-wire-development-check` 真实单击 / 双击及动作屏障 / 几何断言通过，但最终前台为空 / 焦点保持断言失败，退出 101；仍待任务栏焦点复核，不以综合小窗通过代替。

M10d2 补充检查：相关契约 / 隐私运行时 Vitest 15 项、`cargo check -p token-pulse-desktop --release` 通过；新诊断 API 的生产配置编译正常，合成来源的专项启动器仅在 debug 编译。`cargo test -p token-pulse-store diagnostics --lib` 本次实际 7 项通过（新增查询 5 项及原暂存 / 发布回归 2 项）。没有扩展到性能或已取消的数据恢复专项。

M10d2：`cargo test -p token-pulse-core diagnostics --lib` 2 项；`cargo test -p token-pulse-store diagnostics::tests --lib` 5 项（diagnostics 过滤合计含旧事务 / 发布回归 7 项）；`cargo test -p token-pulse-collector --test diagnostics` 实际临时 JSONL 1 项通过。相关 `diagnostics.spec.ts jobs.spec.ts taskbar.spec.ts` Playwright 10 项（新增 2 项）通过；截图实际查看。core / store / collector / desktop strict Clippy、TS / 构建、契约生成检查和 fmt 通过。显式 `pwsh -NoProfile -File scripts/native-smoke.ps1 -Diagnostics` 在自己的 native-probe / 合成来源启动正式资源，NATIVE_DIAGNOSTIC_POSITIONS_OK / 退出 0：格式问题实际偏移、缺失文件、主窗口显示、最新隐私 / mini 拒绝、来源恢复后的替换发布消除问题、可信总量 9、原 / 修正源字节保持。首轮恢复使用小窗操作前的旧设置修订而拒绝，改为正式 get_sources 取最新修订后通过，未绕过 CAS。没有用户 Home / 账户、人工系统键盘 / Win11 / 物理多屏或安装 / 性能验收；此前综合键盘失败仍保留。

M10d1：`cargo test -p token-pulse-store --test jobs` 共 7 项，新增活跃作业被 55 条新历史遮挡的选择 / 优先级 / 终态错误检查；`npx playwright test tests/ui/jobs.spec.ts tests/ui/sources.spec.ts tests/ui/shell.spec.ts tests/ui/taskbar.spec.ts --workers=2` 10 项通过，新增诊断来源错误 / 空时间 / 同修订操作 / 进度读取失败保留 / 内部信息不展示；已有重建未知回复重试和取消保持。TS / 生产构建、store / desktop strict Clippy、fmt / 差异检查通过。实际查看 test-results 中深色 1280 与浅色 960 诊断截图，无横向溢出。`scripts/native-smoke.ps1` 实际 Win10 正式 WebView 输出 NATIVE_DIAGNOSTICS_OK；新命令 main-only，mini 实际拒绝。修正原生检查等待隐私提交 / 按钮可用后，小窗 / 位置各标记通过，但综合脚本在真实键盘恢复失败，退出 1，不能记为全套通过。没有用户 Home / 账户或性能检查；文件级错误定位及其他保留验收继续。

M06f8：`cargo test -p token-pulse-store file_candidate::location --lib` 7 项；`file_candidate --lib` 和 `rebuild --lib` 各 29 项通过。`cargo test -p token-pulse-collector` 全部 63 项，replacement-service 共 8 项（新增 2 项）通过。store / collector / desktop strict Clippy、fmt 通过。合成 SQLite 验证保留读取进度、过期 CAS / 冻结输入、占用路径、歧义身份、边界及作业 / 整组账本 / 位置更新回滚。Win10 临时文件覆盖 reading / ready / claimed / failed 归档移动、4816 Token 与单一逻辑文件，以及实际 watcher 的运行中冻结作业撤销 / 独立服务重新发布 24。全部源字节保持；不读用户 Home / auth.json / 账户，无性能测试或 UI / 安装验收。后台镜像 / 分叉、真实格式、Win11 与物理多屏继续验收。

M06f7：新增 `cargo test -p token-pulse-collector --test replacement-service` 6 项通过；collector 完整 61 项、store / collector / desktop strict Clippy 和 fmt 通过。Win10 实际 native watcher（轮询设到 1 小时以区分）、无 watcher 轮询、启动 / 唤醒、601 行跨读取 / 登记批次、未结束末行重开数据库续读、认领等待、整组依赖等待 / 失败后续读、读取中再次改写及发布后的真实 source_scan 确认。全部合成日志，不读用户 Home / auth.json，不测性能；人工窗口、Win11、多屏、真实格式覆盖及归档 / 替换交错组合继续验收。

M06f6：`cargo test -p token-pulse-store rebuild --lib` 29 项（新增替换清单 / 发布 6 项）、`file_candidate --lib` 22 项；`cargo test -p token-pulse-collector` 55 项（replacement-read 18 项，新增 8 项）通过。store / collector / desktop strict Clippy、fmt / Git diff 通过。合成 SQLite 检查整组发布回滚、真实旧快照、nullable 审计、跨来源旧 / 新关系、冻结输入过期、诊断 / 扫描门禁、无账本清单及 v1 兼容；Win10 临时 JSONL 检查真实只读读取、替换发布、追加及独立后台作业。源属性恢复原权限，不读取用户 Home / 账户 / auth.json；无 UI 变化、人工窗口 / 安装或性能验收。正常 CollectorService 自动触发仍待 M06f7。

M06f5 补充：`cargo test -p token-pulse-collector` 全部 47 项功能检查通过，其中 replacement-read 为 10 项；store / collector / desktop `cargo clippy --all-targets -- -D warnings`、fmt 与 Git diff 检查通过。

M06f5：`cargo test -p token-pulse-store file_candidate --lib` 22 项（新增所有权 / 登记 11 项）、普通采集 `batch::tests` 9 项及 `cargo test -p token-pulse-store --test jobs` 6 项通过；collector 新增 `readonly_replacement_to_owned_registration` 的真实临时日志组合检查通过。验证 128 记录及 16 MiB 登记边界、重开库继续、原子排队 / 竞争认领、header / 身份与旧游标拒绝、登记 / 取消失败回滚、终态释放、普通成功门禁及 v8 → v9 基本兼容。大指纹仅检查事务载荷边界，未测量性能；v8 夹具只验证本次正常 schema 增量，不扩展已取消的迁移保护或灾难恢复。尚未接正常替换重建发布，没有新 UI / 人工原生验收。

M06f4：`cargo test -p token-pulse-store publication_visibility --lib` 3 项通过，`cargo test -p token-pulse-store --lib` 229 项普通检查通过 / 1 性能夹具 ignored。collector 的 canonical-live / canonical-proof / proof-jobs / replay / ingestion / job-service / accounting-upgrade 共 25 项定向回归通过；store / collector / desktop strict Clippy、fmt / Git diff 通过。合成 SQLite 夹具包含 NULL 活跃指针的新身份、候选账本 / 文件 / 事件，检查选择器、详情 / 轮次 / 上下文、父子关系、固定范围 CAS、普通依赖组 / 自动证明及真实分页快照的可见性；指针切换为测试内模拟，不冒充实际替换发布。没有前端布局改变、人工窗口验收、用户日志 / 账户读取或性能测试。

M06f3 的 store / collector / desktop strict Clippy、fmt 与 Git diff 检查通过。

M06f3：`cargo test -p token-pulse-store rebuild::tests --lib` 11 项及 `cargo test -p token-pulse-collector` 46 项功能检查通过。新增合成 SQLite 夹具保存冲突的历史 / 候选计数，验证冻结清单排除、未选定观察引用拒绝、晚到未选定证据不使候选过期、当前指针 / 状态变化保留旧消费、暂停 / 缺失来源保留历史，以及镜像发布不改写未选定观察 / 绑定 / 上下文 / 检查点。collector 回归使用隔离临时真实文件和 Windows watcher；未读取用户日志或账户，未运行性能测试。该增量未新增 UI 或实际替换后最终发布验收。

M06f2：`cargo test -p token-pulse-collector --test replacement-read` 9 项、`cargo test -p token-pulse-store file_candidate --lib` 11 项通过；store / collector / desktop strict Clippy、fmt 通过。使用 Win10 临时真实日志及 SQLite 检查缩短 / 替换 / 同大小改写、重开库跨 500 行继续、追加 / 再次改写、未结束末行 / 超长行、只读属性 / 内容保持和原文不落库；大元数据及 17 MiB 超长行仅检查事务 / framing 边界，没有性能测量。独立接口尚未接正常后台调度与账本发布，不计作文件变化后最终统计或 UI 验收；没有读取用户日志或账户。

M06f1：`cargo test -p token-pulse-store file_candidate --lib` 9 项、`cargo test -p token-pulse-store batch::tests --lib` 9 项及 `cargo test -p token-pulse-store source_scan --lib` 8 项通过；store / collector / desktop strict Clippy 与 fmt 通过。合成 SQLite 夹具验证候选暂存独立于旧消费 / 会话、真实旧快照、候选进度与 EOF、再次缩短、输入过期及原子回滚。该模块尚未接实际目录读取或 UI，不计作 Windows 文件改写验收；没有读取用户日志 / 账户或运行性能测试。

M06e2 定向命令：`cargo test -p token-pulse-store query::coverage --lib` 14 项、`cargo test -p token-pulse-collector` 37 项、`cargo test -p token-pulse-core --test discovery_scheduling` 7 项通过；store lib 最近基线 211 通过 / 1 性能夹具 ignored。core / store / collector / desktop `cargo clippy --all-targets -- -D warnings` 和 fmt 通过。新增 scan-evidence 使用临时合成日志、真实 SQLite / Windows watcher，验证未完成末行、归档、删除树、启动 / 暂停 / 恢复 / 重启及 129 项跨批次完整登记；discovery_scheduling 用隔离实际 junction 验证不跟随目标。Windows 原生检查与 SQLite 自动检查见交付记录，不读用户日志或账户，不运行性能测试。文件代次变化后的候选重建尚待实施。

M06e1：`cargo test -p token-pulse-store source_scan --lib` 8 项通过，使用合成路径及真实 SQLite 检查枚举 / 读取分离、部分末行、旧回调与检查点、暂停 / 改根、已知 / 未知提示、中断重开、旧读快照及事务失败、缺失文件保留消费。store strict Clippy / fmt 通过；没有实际目录调度接入或新增 UI 验收，没有运行性能测试。

离线价格目录内嵌于 core；更新事实时开发者执行 `node scripts/update-offline-prices.mjs --write YYYY-MM-DD openai-text-VERSION`，脚本只读官方公开 Markdown，并强制审查输出差异后再提交。应用启动不使用 Node 或联网抓价格。脚本保留十进制字符串，各档位直接引用官方表，不用折扣推算其他模式。M09g1a 的 store 发布暂未接生产启动，条件匹配 / 正式目录界面继续实施；自动检查使用合成更新、Writer 故障及实际 SQLite 快照，不读取用户日志或账户，不运行性能测试。

M09g1b 已接正常应用启动和主窗口正式目录。运行 `pwsh -NoProfile -File scripts/native-smoke.ps1 -OfflinePrices`：在隔离库走正常目录启动发布，实际 main / mini WebView 验证 172 事实 / 37 可用规则、历史 null / 非法修订、搜索 / 模式 / 历史切换、mini 拒绝、共享隐私清空与重开新查询、重复发布不增修订和消费不变。Win10 19045 / 150% DPI 场景打印 NATIVE_OFFLINE_PRICES_OK，退出 0；WebView2 1412 保留。普通 debug / release 独立应用都使用内置目录；其他原生夹具仍保留显式空价格，避免混入市场价。定向浏览器 `npx playwright test tests/ui/offline-prices.spec.ts tests/ui/prices.spec.ts` 6 项通过；截图位于忽略的 test-results/offline-prices-*.png，合成 bridge 与公开事实用于检查界面，不冒充生产账户 / 用量。

M09g2a 的 `cargo test -p token-pulse-store valuation --lib` 使用合成价格 / 事件和真实 SQLite，8 项通过：持久重开 / 检查点保持、超 JS 安全整数的精确金额、估价模式 / 时点隔离、旧真实快照、镜像证据 / 来源筛选、跨 500 行事务取消、候选过期 / 新事件即时计算、发布失败保留旧缓存。601 事件只用于跨批次功能检查，无性能测量或报告。schema v5 的普通 store lib 为 179 通过 / 1 性能夹具 ignored；store / desktop strict Clippy、fmt 通过。缓存构建暂为内部 API，实际 Windows 目录场景仅是启动 / 既有 IPC 回归，不能证明正式后台重估 UI 已完成。

当前交付环境为 Windows 10 / 11 与本地来源，多屏与各档 DPI 兼容继续验收。macOS、WSL / 网络来源、开机启动、额外快捷键及旧格式自动重解析不再新增；诊断简化、安装打包做简单版，notify、自动更新与完整计价功能保留。详见[已确认范围](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)。

实现依据：[开发总入口](../design/development-design.md)、[实施计划](implementation-plan.md)。当前进度与真实验收结果见[交付记录](delivery-status.md)。

## 环境

M09g2b3：正式启动已运行独立费用服务；`pwsh -NoProfile -File scripts/native-smoke.ps1 -PriceRevalue` 在 Win10 19045 / 150% DPI 的隔离数据目录验证正式启动补建、精确 900 金额原子、实际 main IPC / React 指定时点重估 / 历史、mini 权限拒绝、共享隐私和检查点保持，NATIVE_PRICE_REVALUE_OK、退出 0。数据仅通过正式 Writer 写入合成夹具，不读用户日志或账户；WebView2 1412 退出提示保留。运行中取消 / 退出由同步线程 SQLite 检查，UI 取消由 Playwright 检查，不将终态取消 API 冒充人工长任务验收。`cargo test -p token-pulse-store --lib` 最新 196 通过 / 1 性能夹具 ignored，strict Clippy / fmt、release check、契约与前端构建通过。窗口安装、Win11 和物理多屏继续待验收。

M09g2b2 定向 `cargo test -p token-pulse-store revalue_service` 6 项通过，store all-targets Clippy（warnings denied）通过。线程测试使用真实临时 SQLite、合成消费和同步通道控制取消 / 退出 / 改价边界，短期等待仅是功能完成截止条件；没有性能测量。正式应用尚未启动此服务。

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

## 账户独立冷启动与真实持续读取

真实账户第三入口：`pwsh -NoProfile -File scripts/native-account-startup.ps1 -ExistingAccount -TaskbarAccount`。两开关同时显式启用，默认账户测试不嵌入任务栏。脚本编译独立宿主，要求 TASKBAR_EXISTING_ACCOUNT_OK、LocalTaskbar COLD_OK / EXISTING_COLD_SEQUENCE_OK、退出 0 和拥有的宿主 PID 已结束。正式 IPC 保存隔离偏好、原生可见全文与真实 DTO、共享隐私、隐藏两 WebView 后普通后台读取及停用几何恢复已在 Win10 19045 通过。自有 WM_SETFOCUS 仅验通路，不替代物理输入 / 像素 / Explorer / Win11 / 多屏验收；不输出实际账户数值或发起模型回合。详情见[账户验证](account-quota-verification.md)。

账户冷启动独立入口：`pwsh -NoProfile -File scripts/native-account-startup.ps1`。四个完整进程以同一 UUID 隔离库依次验证 seed / ready / disabled / changed，正常初始化读取已保存设置；脚本要求四阶段及 SEQUENCE_OK，并检查拥有的合成账户子进程已退出。不会打开用户账户、选择器或发送桌面输入。Windows 10 已通过，WebView2 注销 1412 保留；真实账户冷启动 / 持续读取及 Win11 分开记录，详见[账户共享显示验证](account-quota-verification.md)。

真实已有账户持续读取的显式命令为 `cargo run -p token-pulse-quota --features local-account-check --example local_account_check -- --observe-local-existing-account`，要求 initial_ready 和两次 subsequent_read_ready、退出 0。此命令实际连接检测到的已有本地账户，不属于 CI；不调用强制刷新、不发起登录或模型回合。Win10 本机已完成三次读取；只输出净化存在性和连接标记，不保存实际百分比 / 身份。合成领域 / 服务回归分开记录，真实账户切换 / 重置等条件保留，详见[账户共享显示验证](account-quota-verification.md)。

真实账户冷启动使用 `pwsh -NoProfile -File scripts/native-account-startup.ps1 -ExistingAccount`，必须显式选择；默认不运行真实分支。首进程检测现有原生服务 / Home 并保存，第二进程从隔离库正常冷启动；要求 LocalSeed / LocalReady、EXISTING_COLD_DISPLAY_OK / SEQUENCE_OK 与退出 0。Win10 已通过正式 main / mini IPC、周期和 progress DOM 对应真实 DTO。没有保存截图或真实额度值，不发起登录 / 模型回合，不代替物理输入、任务栏第三入口及 Win11 验收。

## 显式模型别名 IPC 验收

pwsh -NoProfile -File scripts/native-smoke.ps1 -PriceAliases 选择 debug-only、无来源 / 账户的隔离库及两个真实 WebView，实际核对 main-only create / replace / retire / 旧版本、CAS / 映射冲突、提交成功通知、隐私投影和 mini 拒绝。Win10 已输出 NATIVE_PRICE_ALIAS_OK、退出 0，WebView2 注销 1412 保留。此检查不启用任务栏、不读取真实日志 / 登录信息，不等同设置编辑器 UI 验收。定向自动检查 cargo test -p token-pulse-core --test pricing 和 cargo test -p token-pulse-store --lib pricing 均使用合成单价 / 模型、独立预期费用及消费不变；没有性能测试。

M09f2 的 -PriceAliases 追加真实 React 表单事件，经正式 command / SQLite 完成新增、替换、历史只读、退休、外部发布后的原修订冲突及草稿保留，随后共享隐私清掉草稿，关闭后不恢复。成功通知现在精确检查 1–8 共八次；最终 NATIVE_PRICE_ALIAS_OK 和退出 0 为通过。本机 Win10 19045 / 150% 已通过，WebView2 1412 保留。运行前 npm run build；定向浏览器 npx playwright test tests/ui/prices.spec.ts，4 项通过，深 / 浅及 960 宽截图位于忽略的 test-results/model-alias-*.png，仅为开发合成 DTO 视觉检查。普通价格规则回归保持，未运行性能测试。
