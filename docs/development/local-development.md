# 本地开发与运行

M13g19 来源状态验证：`cargo test -p token-pulse-taskbar --lib --tests --locked --offline` 为 73 passed / 1 默认 ignored；ignored 是 guardian 私有子入口，父测试实际显式运行它。`cargo clippy -p token-pulse-taskbar --all-targets --locked --offline -- -D warnings`、`cargo run -p token-pulse-taskbar --example export_host_contract --locked --offline -- --check` 和 `cargo check -p token-pulse-desktop --release --locked --offline` 通过。初次重复 schemars length 属性编译失败，移除旧属性后重跑通过，保留 target/taskbar-source-status-tests*.log；不放宽重复 / 上限校验。当前修正仅在源码，新主程序和宿主须成对打包；不单独替换已安装宿主。用户要求不弹出应用时，只继续无窗口编译 / 静态检查，不运行 native-smoke、可见宿主或鼠标键盘例程。

2026-10-06 M09h3b2c3 本机原生增量：`scripts/native-smoke.ps1 -PriceRevalue` 重跑实际退出 0 / NATIVE_PRICE_REVALUE_OK，使用应用自有隔离数据库、真实 Windows WebView / 正式 IPC 验证启动缓存金额 1300 原子、matched_price 同规则 / 原修订 / 规范模型、未知模式不补造、隐私隐藏依据但保留 110 Token；原 React 四费率编辑 / 重估 / 历史、mini 权限与检查点验证保持。首次新增检查错误地为无续页结果提交 null cursor 的关闭请求，正式接口正确拒绝 INVALID_QUERY；改为只在有实际续页时关闭后通过，生产校验未改。两个输出保存在 target/matched-price-native.log 与 target/matched-price-native-retry.log（本机忽略目录）。隔离恢复快捷键与正式实例冲突的 SHORTCUT_CONFLICT、原 WebView 退出 1412 提示保留，不影响场景实际退出 0；不将此检查当真实日志 / 账户 / 正式安装或日常目视验收。

M09h3b2c3 同快照计价解释：`cargo test -p token-pulse-core -p token-pulse-store --lib --tests --locked --offline` 461 passed / 1 原性能夹具 ignored；strict core / store all-target Clippy、契约生成 check、release-cfg desktop check、前端 build 通过。`npm test -- --run ui/src/shared/contracts.test.ts` 11 项和 `npx playwright test tests/ui/events.spec.ts --workers=1` 7 项通过；深 / 浅主题 960px 截图已查看。仅合成 / 临时数据参与模式和价格检查，真实 mode 未采集；正式安装与公开版本不随这些检查改变。

M09h3b2c2 自动检查：`cargo test -p token-pulse-core -p token-pulse-store --lib --tests --locked --offline` 458 passed / 1 原性能夹具 ignored；`cargo test -p token-pulse-store v4_and_v5 --lib --locked --offline`、core / store all-target strict Clippy 与 release-cfg desktop check 通过。合成模式仅进入应用自有临时库的内部求值入口，正式请求工厂 actual_tier=None；不使用用户日志或配置代替响应模式。源码 cache v6 / schema 14，原始 source_total 可空值参与指纹和完整关联；实际安装仍 0.1.6 / cache v4。候选后台生成核对同快照不可变父行与条件索引，缓存命中仍原修订 / SHA 门控；不能把这些自动检查称为真实条件费用 / 账户或新包验收。详见[第 17 节](../design/price-accounting.md#17-所选不可变规则核对与生产请求关联m09h3b2c2)。

M13g16 UIA 补试：正式 WebView select 暴露 ExpandCollapse 不保证能枚举选项；本机展开后没有可 Select / Invoke 的自有选项，Collapse provider 拒绝，失败保留。只能确认值仍原 notification_left 且重置草稿禁用，再通过自有 WM_CLOSE 恢复原主窗口隐藏。只读 settings 141 / 原 taskbar 偏好保持；不可将 ValuePattern 可读、UIA 导航、自有托盘命令或窗口未 cloak 写成真实位置输入通过。回执与范围见[交付记录](delivery-status.md)。

M09h3b2c1 请求费用指纹：运行 `cargo test -p token-pulse-store --lib --tests --locked --offline` 与 `cargo clippy -p token-pulse-store --all-targets --locked --offline -- -D warnings`，均通过；301 项自动测试 / 1 原性能夹具 ignored。`cargo check -p token-pulse-desktop --release --locked --offline` 通过。新用例在 valuation/tests/request_inputs.rs，只用应用自有临时库和合成必要证据，不打开真实日志 / 登录文件；白名单身份与原 source_total=null 关联跨汇总、明细和后台缓存保持一致。CACHE_VERSION=5、schema v14，无新 DTO；旧 v4 不参与读取但保留。当前正式安装 0.1.6 的 cache v4 不因 cargo check 改变，不把编译检查当安装或真实模式计价通过。详见[计价专题](../design/price-accounting.md#16-生产请求输入与费用缓存身份m09h3b2c1)。

2026-10-06 M13g16 输入门禁补记：正式任务栏另一位置应先确认自有主窗口 / WebView 所有权及实际 WindowFromPoint 命中。UIA IsOffscreen=false、DWM cloak=0 或 SetWindowPos 成功都不足以证明桌面点击点属于应用；SetForegroundWindow 后须读取实际前台，拒绝时不注入其他窗口、不借 Alt / 其他应用改焦点。保持 WTS / 输入桌面 / 已按住键与鼠标门禁，测试指针仅在用户没有移动时恢复。本轮三种拒绝均保留，未选择 / 保存位置，正式另一位置仍待验。当前已安装 0.1.6、任务栏 notification_left / two_rows / enabled；见[交付记录](delivery-status.md)。

2026-10-06 M16z4 当前正式安装 0.1.6-327183e：完整构建、原密钥签名与独立 native verifier / NSIS 实际退出 0，通过宿主 manifest 提取与安装字节验证。只读旧库前后 schema 14、data 1496 / price 1 / settings 138 保留，随后正式 UI 恢复采集。回执和逐张查看的真实截图在 `target/release/review/v0.1.6/`，包括 60 秒无捕获状态 / 真实物理输入；不要把窗口状态或捕获图替代普通桌面目视。独立试验普通桌面透明显示已确认，正式包目视答复仍待收到。公开仍 0.1.3、未重启或推送；范围与失败历史见[交付记录](delivery-status.md)。

M13g15 可用 `cargo build -p token-pulse-taskbar --example check_taskbar_layout` 后显式运行 `check_taskbar_layout.exe --native-taskbar-visible-check`。它使用明确的开发夹具 / 自有安全槽，120 秒内每 2 秒刷新并核对三处真实 WindowFromPoint 命中，完全不调用捕获函数，之后检查隐私更新、禁用 / 再挂接 / Drop 恢复；不修改正式库，也不把合成价格或账户当真实数据。应先让正式程序经自有托盘命令正常退出，结束后恢复正式程序；缺少独占预留不得并行挂接。长观察允许用户自行改变前台，记录前台差异而不据此宣称程序夺焦点，原短自动例程仍严格保持焦点判据。新 manifest 必须嵌入宿主、example 和原生测试，不能仅修改窗口样式；本机旧宿主无资源 / 无清单，远程样式试验实际错误 87 并恢复。2026-10-06 72 项自动 / strict Clippy 与无捕获检查实际退出 0 通过；失败历史 / 实际范围见[交付记录](delivery-status.md)。

2026-10-06 M16z3 本机已升级到签名 0.1.5-7cfb09e，实际安装退出 0 / schema 14 / 旧表行与 data、price、settings 修订保留；主程序 / 独立宿主 / notices 字节核对通过。当前包暂停 / 恢复标签修正已接入，任务栏浅底在捕获中已修正，但普通桌面不可见仍是用户确认的缺陷。安装、真实鼠标输入、失败历史与捕获证据的边界见[交付记录](delivery-status.md)，回执 `target/release/review/v0.1.5-db965d6/`；公开仍 0.1.3，未重启或推送。下面仍 0.1.4 的段落是历史阶段记录。

M15f5 已安装跨屏检查：使用 PowerShell 7 运行 `scripts/verify-installed-cross-monitor.ps1 -ApplicationId <实际 PID> -BaselineExecutable <对应 release 主程序> -PythonExecutable <本机 Python>`，默认只读检测。显式 `-Exercise` 才通过自有窗口原生移动与真实 React 按钮检查实际屏幕上的主窗口 / 两小窗及 SQLite 位置；不修改显示拓扑或 DPI。要求安装字节对应 NSIS 基线、进程起始时间保持、至少两屏，穿透已开启时拒绝测试；原位置、展开、隐藏和最大化分别尝试恢复，核对非位置设置摘要并以 CreateNew 写入 `target/native-cross-monitor/<UUID>/receipt.json`。测试位置必须完整落在工作区，恢复时保留用户原窗口即使其原先越界。2026-10-06 Win10 两屏 150% 六组实际通过，失败 / 成功回执与边界见[交付记录](delivery-status.md)。程序化移动不替代真实鼠标拖动、其他 DPI 或拓扑改变。

2026-10-06 M10k 状态修复可用 `npx playwright test tests/ui/sources.spec.ts tests/ui/diagnostics.spec.ts --workers=1` 复核（3 项），`npm test` 为 37 项，`npm run build` 与 desktop strict Clippy / fmt 通过。native SourceDialogs 新增暂停 / 异步恢复 / 历史保留状态标签和显式夹具日期；先构建 `cargo build -p token-pulse-desktop --features custom-protocol`，以 Start-Process / PassThru / WaitForExit 启动 `--native-smoke --native-source-dialogs-smoke`，不要把 Windows GUI 的启动返回当退出证据。本轮 PID 104944 实际退出 0，日志 target/release/review/v0.1.4-4e4e052/runtime-status-native-*.log，原 SHORTCUT_CONFLICT / 1412 警告保留。正式安装仍先前 4e4e052 构建，不包含本次状态修复，公开仍 0.1.3，详见[交付记录](delivery-status.md)。

2026-10-06 M16z2 当前源码 / 本机候选 0.1.4，schema v14 / cache v4 已实际安装；公开仍 0.1.3。完整 `npm run tauri:build`、同原密钥签名 / 正式版本绑定验签与本机 NSIS 升级通过，实际旧程序 / 安装退出均 0。45 张旧表行摘要、预期 schema 字段转换与修订保留分别核对，两张可重建逐事件费用缓存仅记录数量。真实托盘物理菜单通过；正式页面 / 两小窗截图、两个 150% 屏幕的只读尺寸与正常后台 v4 成功分别记录，不冒充跨屏 / 其他 DPI / 完整条件金额。当前本机回执 target/release/review/v0.1.4-4e4e052/，详见[交付记录](delivery-status.md)。已知恢复来源后顶部状态可短暂滞后，刷新可恢复，后续修正状态刷新与暂停文案。

2026-10-04 M16z1 源码版本为 0.1.4；Windows 前置可用 `cargo metadata --locked --offline --filter-platform x86_64-pc-windows-msvc --format-version 1` 核对。发布准备工具 6 项通过；不把版本同步当成本机 / 线上升级。当前正式安装 / 公开仍 0.1.3，完整构建及同密钥签名、本机数据保留后续执行，详见[交付记录](delivery-status.md)。

2026-10-04 02:28 已对正式安装 0.1.3 精确基线做一次只读窗口 / 显示 / WTS 核查：实际 144 DPI / 150%、单活动桌面表面、主窗 1280×860 / 隐藏紧凑小窗 280×220 DIP；WTS flags=0 仍锁屏，但本时点输入桌面可读 / 前台句柄存在，须分别报告，不能用旧失败值或少数前置项代替物理通过。回执位于本机忽略 review 目录，读取线程 DPI context 已恢复，没有系统缩放 / 输入 / 设置变化；见[只读复核记录](delivery-status.md#2026-10-04正式安装版的只读-windows-上下文复核)。实际其他 DPI / 多屏仍待验，不需要重复要求解锁来进行这些只读或独立开发检查。

2026-10-04 M09h3b2b2c 无新界面 / 契约 / DDL；源码 schema v14、费用缓存 v4。`cargo test -p token-pulse-core -p token-pulse-store --lib --tests` 的 450 项通过、1 性能夹具 ignored；`cargo clippy --workspace --all-targets -- -D warnings`、`cargo check -p token-pulse-desktop --release` 与 fmt 通过。日志位于 target/conditional-history-*.log。新增历史 / 旧平价 / 别名修订间隔 / 撤价 / 坏历史 / v3 缓存门控场景是合成请求与临时 SQLite，不代表实际模式采集或新安装；正式 0.1.3 未改。后续条件输入指纹 / 后台金额 / 公开依据继续，见[第 15 节](../design/price-accounting.md#15-固定目录历史与条件估价时点m09h3b2b2c)。

2026-10-04 M09h3b2b2b 源码升 schema v14，首次启动旧已发布目录会在原发布修订下补建条件身份；这是确定性参考元数据，不是猜模式或价格更新。自动复核仍用 core / store lib / tests（442 项全套）及新增容量测试 1 项，共 443 通过、1 性能夹具 ignored；workspace strict Clippy / release desktop check 与新增 store Clippy 通过。日志位于 target/conditional-rule-publication-*.log / target/conditional-rule-capacity-*.log。六组新临时库场景覆盖 v13 迁移、旧快照 / 幂等 / 回滚 / 外键 / 损坏拒绝 / 容量，不是用户真实库或正式安装验收。公开及本机仍原 0.1.3 / schema v13，生产条件金额继续；见[专题第 14 节](../design/price-accounting.md#14-不可变条件规则身份与普通规则隔离m09h3b2b2b)。

2026-10-04 M09h3b2b2a 配置核心条件金额已通过独立预期和原存储回归，复核命令为 `cargo test -p token-pulse-core -p token-pulse-store --lib --tests`（437 项通过，1 性能夹具 ignored）、`cargo clippy -p token-pulse-core -p token-pulse-store --all-targets -- -D warnings`；日志位于忽略目录 `target/conditional-engine-regression.log` / `target/conditional-engine-clippy-final.log`。最初测试金额字段 / 字符串与数字格式检查失败已修正，失败日志保留。本增量仅核心入口，不含生产持久条件金额或实际模式采集，没有新安装 / UI 验收；见[计价专题第 13 节](../design/price-accounting.md#13-配置计价入口的条件金额选择m09h3b2b2a)。

2026-10-04 M16y3 已公开 [0.1.3 候选](https://github.com/LiuYongXinn/token-pulse/releases/tag/v0.1.3)；三资产匿名下载 / 版本绑定验签、正式安装版生产渠道“当前已是最新版本”检查通过。本轮 0.1.3 安装为本地签名 NSIS，未重复线上安装；0.1.0→0.1.2 线上闭环保留其独立证据。最新限制及回执见[交付记录](delivery-status.md)。以下准备 / 公开 0.1.2 是此前阶段，不能覆盖本段当前状态。

2026-10-04 本机已从 0.1.2 正常退出并安装签名候选 0.1.3，schema v13 / 费用缓存 v3、四费率、明确来源重读和请求依据已进入安装产物；公开 GitHub 仍 0.1.2。完整 NSIS / release 宿主构建、同一密钥正式验签、安装文件 / 注册信息及旧列摘要保留通过；真实正式 UI 重读 176 个文件成功，候选验证后恢复可信用量，条件不全费用继续未计价。见[交付记录 M16y2](delivery-status.md)。重建不能累加各次重放的事件数。纯核心条件选择不能代替缺失的实际模式 / 地区证据，0.1.3 线上更新尚未验证；下方旧安装版本描述保留历史时点。

M09h2 的源码 / schema v12 / 四费率编辑器与费用缓存 v2 已通过 core + store 415 项（1 既有性能夹具 ignored）、Vitest 37 项、价格 / 重估 / 离线目录 Playwright 11 项和隔离 Win10 -PriceRevalue 原生检查。原生四项独立预期为普通 20×10 + 命中 60×5 + 写入 20×30 + 输出 10×20 = 1300 原子；真实表单将写入费率设 0 后后台补建为 700，历史费率保持、消费 / 检查点不变。可复核 `npm run build` 后 `pwsh -NoProfile -File scripts/native-smoke.ps1 -PriceRevalue`；必须等待原生进程实际退出并检查成功标记，不能把 GUI 启动返回当完成。日志位于忽略目录 target/cache-write-rates-*.log。正式安装 0.1.2 未升级到这些源码改动，实际日志 / 历史拒绝记录补读与条件匹配继续。

M09h1a 源码新增缓存写入必要数量，schema v11、rollup v2 和生成 DTO 同步；正式安装 0.1.2 仍为此前发布内容。`pwsh -NoProfile -File scripts/native-smoke.ps1 -SourceDialogs` 的隔离合成 Home 含写入 2 / 总量 17，经过真实目录选择 / 采集 / 暂停恢复 / 移除保留 / 精确明细 DTO / 总览 React 分项验证，退出 0。原生日志在忽略目录 `target/cache-write-native-source.log`，保留 SHORTCUT_CONFLICT（正式实例持有恢复键）与 WebView2 1412；不视为键盘、正式安装版或真实用户费用验收。定向 UI 截图 `test-results/cache-write-overview.png` / `cache-write-evidence-panel.png` 为合成桥接视觉检查。

2026-10-03 计价核查见[设计专题](../design/price-accounting.md)与[交付澄清](delivery-status.md#模型计费状态澄清2026-10-03)。别名编辑、离线目录发布 / 界面、持久费用缓存及后台补建 / 手动重估 / 进度 / 取消均已接入；下方 M09 早期“尚未启动 / 接入”是历史记录。51 模型 / 172 价格事实不等于完整计价，37 条仅为 Standard 参考规则。生产不在线更新价格；独立缓存写入、每请求长度 / 实际模式及地区证据继续补齐，工具 / 多模态分列扩展。

当前标准安装 / 公开发布版本为 0.1.2，已在本机 Windows 10 完成真正的 0.1.0→0.1.2 应用内固定 GitHub 渠道升级。正式 UI Automation InvokePattern / SelectionItemPattern 可在缺少物理输入桌面的条件下操作更新按钮，仍经过生产检查、完整下载、版本绑定验签和 NSIS，未使用 debug IPC / 自定义服务源。新应用自动启动、文件 / 宿主 / 注册版本及真实 SQLite 46 张非设置表摘要和 data / price revision 保持均核对，新版检查为最新；物理键鼠 / Narrator、非零消费、缺 WebView2 等边界单独记录。真实来源 cache_write_input_tokens 扩展当前被适配器拒绝，优先继续其语义与兼容修复。完整回执位于忽略目录 `target/release/review/v0.1.2-11ef9df/`，见[M16x](delivery-status.md#m16x修正版的正式-github-在线升级闭环)。

发布顺序以用户最新授权为准：仓库已公开并允许候选先发布，再做真实线上升级。首次 0.1.1 在线升级的应用内检查 / 下载 / 验签通过，实际 NSIS 的已退出父进程分支失败；修复同调用 `?e` 捕获错误后，`pwsh -NoProfile -File scripts/verify-update-hook.ps1` 实际覆盖父进程存活、已退出握句柄、已退出释放句柄三时序。此夹具不安装产品；仍须从正式安装版通过 UI 确认新签名 0.1.2，核对正常退出 / 文件 / 注册版本 / 新应用与数据库。见[本轮记录](delivery-status.md#m16w公开发布与已退出父进程的安装竞态)。

任务栏右侧电源验收：`pwsh -NoProfile -File scripts/native-smoke.ps1 -PowerTaskbarMessages -ApplicationRight` 仅运行隔离自有消息场景；`pwsh -NoProfile -File scripts/verify-power-resume.ps1 -Taskbar -ApplicationRight` 默认仅预检，追加 `-ActualStandby` 才实际让整机 S3 睡眠 / 唤醒，不重启。右侧标志不能与普通采集电源或其他场景混用。睡眠前后正式 settings IPC 核对位置与启用 / 回退，驱动要求该位置的成功标记。Win10 19045 / 150% 的同 M16v release 宿主与 debug 桌面右侧消息及真实 S3 均退出 0，3→10、新快照 / 详情、窗口和导航保持、宿主正常退休及完整几何 Exact；前一模块左侧的真实 S3 保持独立证据，详见[M13g13](delivery-status.md#m13g13应用图标右侧的真实-s3-恢复链)。混合验收不代替正式安装版物理输入或全包验收。

## 关键页面截图评审产物

2026-10-03 的[24 张截图画廊](../../target/page-review/2026-10-03-a32d1c6/index.html)和[来源 / 图像摘要清单](../../target/page-review/2026-10-03-a32d1c6/manifest.json)以当前源码 `a32d1c6` / 0.1.1 为基准，保存于忽略的 `target/page-review/`。主窗口 / 小窗使用正式 React 组件与浏览器测试桥中的合成 DTO，任务栏使用正式原生 GDI 渲染器的合成夹具；均不表示已安装应用的真实账户或消费实拍。长页保留完整滚动截图，区域图与原生详情首屏单独标注。截图场景 13 项和定向原生像素检查 1 项通过，详细范围与限制见[截图记录](delivery-status.md#2026-10-03关键页面截图评审)。本地清理构建产物会移除画廊，需要保留时复制整个目录；材料未上传 GitHub。

任务栏电源场景：`pwsh -NoProfile -File scripts/native-smoke.ps1 -PowerTaskbarMessages` 独占模式，使用标记的自有主 HWND 电源消息；`-PowerTaskbarResume` 只观察系统消息，不触发睡眠。真实驱动为 `pwsh -NoProfile -File scripts/verify-power-resume.ps1 -ActualStandby -Taskbar`，默认不带 ActualStandby 时仍仅预检。需已有 debug 桌面 / 前端及同目录宿主；驱动记录宿主摘要。先通过真实 WebView 固定会话 / 开启任务栏，再暂停并追加合成待处理数据，恢复前提前计入即失败；恢复后核对 10 Token 原生 caption / 详情、新快照与窗口 / 导航，正常退出后核对同一 Explorer 和完整当前任务区。只接受完全相同或严格仅共享通知边界变化，其他几何 / DPI / 缩短区域拒绝。本机最终 M16v 同包宿主 + debug 桌面实际 S3 退出 0、走 Exact；混合验收与历史失败范围见[M13g12](delivery-status.md#m13g12任务栏宿主的真实-s3-恢复链)。

真实电源验收：`pwsh -NoProfile -File scripts/verify-power-resume.ps1` 默认只预检，不启动观察应用或睡眠；输出新临时证据目录。显式追加 `-ActualStandby` 会暂停整机程序，要求已有 debug 桌面二进制与前端产物、当前没有其他 debug 实例、电源 / 唤醒 / 本进程已有权限 / 系统事件全部满足。匹配自有 READY 与内核进程身份后请求 S3，设定 UTC 40 秒唤醒期限，保留唤醒事件；不重启、关机、休眠、提权或更改电源策略。恢复后要求独立 System S3 睡眠 / 恢复记录与正式采集补扫断言，权限和 native 资源完整清理。本机一次实际退出 0，请求到返回约 66 秒，具体唤醒来源未证明；此测试使用隔离 debug SQLite，不能替代正式包 / 任务栏宿主整体验收。证据与限制见[M07r2](delivery-status.md#m07r2真实-s3-睡眠与恢复补扫)。

电源恢复验收入口：`pwsh -NoProfile -File scripts/native-smoke.ps1 -PowerMessages`，必须独占选择，需要已有前端产物。场景使用 UUID 隔离数据库与合成只读 Home，watcher 关闭 / 一小时轮询，准备阶段直接暂停采集并追加待处理记录；向自有主 HWND 发送标记电源消息后验证正式恢复补扫 3→10、重复消息不重复计数、源保持与隐藏主窗，当前实际通过。`-PowerResume` 只观察系统消息并等待最多 150 秒，不会触发睡眠，也不发送模拟消息；必须结合真实电源驱动与 OS 证据，观察入口存在不等于实际 S3 已验。

`pwsh -NoProfile -File scripts/inspect-power-resume.ps1` 默认只读电源 / 方案 / 唤醒策略，未检查 Timer 时字段和 Eligible 为 null；追加 `-CheckWakeTimer` 仅建立自有绝对 UTC 唤醒请求后立即取消 / 关闭，不调用睡眠、休眠或重启，不改变电源策略。本机 AC / S3 / 唤醒策略及定时器预检通过，硬件真实唤醒仍未验。正式 M16v 包不受这组 debug 验收与独立检查影响；证据和限制见[M07r1](delivery-status.md#m07r1隔离恢复补扫入口与电源前置检查)。

当前签名候选为 M16v：[0.1.1 安装器](../../target/release/publish/v0.1.1-hidden-a29a71055aba44faa365e53ea632fd39/TokenPulse_0.1.1_x64-setup.exe)、[本地清单](../../target/release/publish/v0.1.1-hidden-a29a71055aba44faa365e53ea632fd39/latest.json)、[实际验签报告](../../target/release/publish/v0.1.1-hidden-a29a71055aba44faa365e53ea632fd39/release-verification.json)。安装器 6,626,972 字节 / SHA-256 `e147e78311c46804324f8355b5e7bf3b2da0bd5eb0e2e2c63401b495678fc84a`，已包含 M13g10 通知区宽度与 M13g11 系统可见性修复；候选同目录宿主的两位置实际自动隐藏 / 数据及自有设置刷新 / 恢复、正常 Explorer 同宿主恢复均退出 0。未重启电脑、未安装升级 / 发布，原 0.1.0 保留；下方旧“需重建”已由此完成，“最新”按历史保留，标准托盘物理菜单和其他外部项不混记通过。详见[完整产物证据](delivery-status.md#m16v纳入通知区与系统可见性修复的签名候选)。

M13g11 新增失败现场只读样式 / 几何及有限普通重试诊断，原严格断言不变。显式自动隐藏例程可追加 `--own-setting-refresh`（可与 `--application-right` 合用），向唯一匹配自有宿主 Control 窗口发送有期限同步零载荷设置消息；不会广播或发送物理输入。两位置的真实自动隐藏 / 新 Snapshot / 自有设置刷新 / 恢复与退出几何已通过，69 项检查与严格 Clippy 通过，完整应用与 release 宿主跨正常 Explorer 恢复通过。中间 M16u 包遇到根 WS_VISIBLE 清除时误报的失败已保留，当前修复需重新完整打包；下方旧“最新候选”仅是历史，未安装升级 / 发布。详见[根因和证据](delivery-status.md#m13g11系统自动隐藏与读数自身可见性的区分)。

M13g10 已确认并修复下方 M13g9 的通知区宽度竞态：只有正常存活租约持有完整原拓扑且当前精确为自有预留时，允许通知区共用边界变化后的有条件释放；终止 guardian 规则不变。debug 场景可显式设置 `TOKENPULSE_ACCEPTANCE_HOST_DIAGNOSTICS=1` 取得有限恢复差异 / 成功日志，正式 release 不输出。68 项自动检查、严格 Clippy / fmt、完整 Tauri 动作回归和真实完整应用匹配失败形态后成功恢复均通过；参见[实际证据](delivery-status.md#m13g10通知区边界变化后的有条件预留释放)。M16t 旧签名包不含此生产修复，需从新提交重建并签名；历史“未收敛”描述不代表当前根因仍未知，不重启电脑或提前推送。

完整桌面应用的 Explorer 验收入口：`pwsh -NoProfile -File scripts/native-smoke.ps1 -TaskbarExplorerRestart`，需要已有 `npm run build` 前端产物；此开关必须独占，使用 UUID 隔离空数据库、真实 WebView / 生产后台 actor / 独立宿主。会通过既有资格校验后正常关闭 / 恢复 Explorer；不重启电脑、不强杀、不发送输入，也不属于普通 cargo test / 默认 smoke。`TOKENPULSE_ACCEPTANCE_PWSH` 可指定 PowerShell 路径。最终检查持有的同一宿主内核对象、新窗口代次、重启后新快照、主 WebView 状态 / 隐私屏障、禁用进程退出和新 Shell 的独立稳定几何。三轮完整成功与一次未收敛 ExternalChange 竞态分别保留；不能把本入口存在或成功轮次当成整个任务栏验收结束。详见[完整应用检查与剩余问题](delivery-status.md#m13g9完整-tauri-应用的真实-explorer-恢复入口与未收敛竞态)。

最新签名候选为 M16t：[0.1.1 安装器](../../target/release/publish/v0.1.1-receipt-9ce477bd01ed46b4af621bfb828e0387/TokenPulse_0.1.1_x64-setup.exe)、[本地候选清单](../../target/release/publish/v0.1.1-receipt-9ce477bd01ed46b4af621bfb828e0387/latest.json)、[实际验签报告](../../target/release/publish/v0.1.1-receipt-9ce477bd01ed46b4af621bfb828e0387/release-verification.json)。安装器 6,630,563 字节，SHA-256 `0de21c00e573b1df7826ad18b3c66587aac0d524e7b5937b05646e1262cd3878`，包含 M13g5–g8 的 Explorer / 空裁剪 / 自动隐藏 / 回执修复。候选同目录 release 宿主已通过两位置真实自动隐藏期间刷新与恢复、正常 Explorer 重启恢复三项原生复测。未重启电脑，未安装升级 / 发布，原 0.1.0 保留。下方旧候选按历史保留，默认 bundle 旁旧 `.sig` 不能用于新字节；详见[最新完整证据](delivery-status.md#m16t包含全部任务栏修复的签名候选与三项-release-原生复测)。

M13g8 已用 release 原生宿主实际通过两位置自动隐藏刷新；此前 05bede333b5a4d5ba71fb1f2ad2d53fd 完整候选在 release 验收失败，不作为最新可用包。修复后的完整包需重新构建 / 签名；已安装 0.1.0 保留。见[一次协调结果与实际 release 验收](delivery-status.md#m13g8release-回执的一次布局校验结果)。

M13g7 自动隐藏入口增加 `--application-right`：在原显式开发标志后追加此参数，使用正式应用按钮测量选择右侧位置；不追加时使用通知区左侧。两种位置均检查真正隐藏后再发送新 Snapshot 仍保持嵌入，并恢复设置 / 几何，本机实际均通过。此证据不替代真实鼠标触边、物理键盘 / Narrator 或多屏。见[两位置刷新记录](delivery-status.md#m13g7两位置自动隐藏后的实际数据刷新)。

实际任务栏自动隐藏验收：`cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host --example check_taskbar_autohide` 后运行 `target/debug/examples/check_taskbar_autohide.exe --native-taskbar-autohide-development-check`。显式开关才执行；要求初始未自动隐藏的已验证 Win10 主任务栏，期间正常启用自动隐藏并按 Shell 归属恢复，读取真实状态及物理窗口位移。普通 cargo test 不运行此 main、不更改设置。M13g6 实际通知区左侧隐藏保持嵌入 / 恢复通过，不代表鼠标触边 / 应用右侧物理场景。M16s 尚未包含本次平移修复；见[当前修复与证据](delivery-status.md#m13g6真实系统自动隐藏与布局平移恢复)。

最新签名候选为 M16s：[0.1.1 安装器](../../target/release/publish/v0.1.1-shell-7e36b18664984d51a9dcf27cb4dbeaa8/TokenPulse_0.1.1_x64-setup.exe)、[候选清单](../../target/release/publish/v0.1.1-shell-7e36b18664984d51a9dcf27cb4dbeaa8/latest.json)、[实际验签报告](../../target/release/publish/v0.1.1-shell-7e36b18664984d51a9dcf27cb4dbeaa8/release-verification.json)。安装器 SHA-256 `c5dda4ecc954660ae85f8c45fc7bf08b3f6d87c4edd8517d3494347a1c983937`，包含空裁剪 / Explorer 代次修复，候选 release 宿主真实恢复通过。旧 M16n 路径和哈希为历史候选；默认 bundle 路径旁的旧 `.sig` 不能与新字节配对，使用本次新目录内的一致安装器 / 签名 / 报告。未安装 / 发布，原安装 0.1.0 保留；不将实际验签或宿主恢复当成完整在线自动升级通过。见[本轮构建与验收](delivery-status.md#m16s纳入-explorer-修复的签名候选与-release-原生恢复)。

Explorer 重启资格只读检查：`pwsh -NoProfile -File scripts/inspect-explorer-restart.ps1`，默认不关闭窗口或结束进程。M13g5 已细化严格的 Shell-cloaked 空框架判断，并实际完成正常 Explorer 进程重启后的同宿主恢复。显式开发入口：先 `cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host --example check_taskbar_explorer_restart`，再运行 `target/debug/examples/check_taskbar_explorer_restart.exe --native-taskbar-explorer-restart-development-check`；会正常关闭 / 恢复资格核对后的 Explorer，不重启电脑、不强杀，也不属于普通 cargo test。PowerShell 路径可由 TOKENPULSE_ACCEPTANCE_PWSH 指定；TOKENPULSE_ACCEPTANCE_HOST_DIAGNOSTICS=1 仅在 debug 打开有限错误枚举 / GDI 状态输出，release 不接受此诊断开关。M16n 的旧 0.1.1 候选尚未包含本轮修复，正式安装 0.1.0 保留。见[实际恢复与修复](delivery-status.md#m13g5真实-explorer-重启恢复与空裁剪区域修复)。

锁屏下可运行受控原生功能检查：`pwsh -NoProfile -File scripts/native-smoke.ps1 -TaskbarActions`、`-RecoveryRoutes`、`-Notify` 分别验证任务栏完整消息通路、小窗交互恢复 / 透明度及 notify 配置 / headless / 采集。M16q 三项本机实际退出 0，使用真实 WebView / 原生实现及隔离开发数据库，不发送物理键鼠、不修改正式设置。实际输入 / Narrator、多屏 / DPI、真实休眠和账户变化、正式源完整升级分别保留独立验收边界，不能统称为锁屏无法测试。见[本轮方法与证据](delivery-status.md#m16q无需桌面输入的三项原生功能复测)。

M16p：用户要求直接测试后，标准安装 0.1.0 的后台窗口命令 / 单实例 / 正常退出 / 冷启动序列实际退出 0；46 张非设置表及数据 / 价格修订前后完全一致。统计 / 小窗恢复原隐藏状态，正式应用保留运行。此检查用本应用受限窗口消息，不发送物理输入，托盘弹出菜单仍未验；不需要锁屏解锁即可执行上述后台序列。证据和临时程序见[本轮实际验收记录](delivery-status.md#m16p正式安装版的后台窗口控制与冷启动复测)。

正式托盘受限验收：`pwsh -NoProfile -File scripts/verify-installed-tray.ps1 -BaselineExecutable <与已安装版本对应的release.exe>` 默认只读；追加 `-PhysicalInput` 才操作已验证的通知区展开按钮 / 本应用图标。当前已安装 0.1.0 使用 `target/release/baselines/v0.1.0-3f05cdf904034d139dd9fe8fe8c03207/desktop.exe`，不能使用新的 0.1.1 release 代替。只读 / 错基线 / 锁屏拒绝已通过检查，物理成功分支尚未验。需解锁并保留可交互桌面，脚本在每次输入前重新检查；已有展开面板、遮挡、按键按下、非 S_OK 图标矩形均拒绝，不改变用户显示偏好。见[当前入口与限制](delivery-status.md#m16o正式安装托盘的受限物理输入验收入口)。

M16n：0.1.1 本地升级候选已完整构建、使用同一项目密钥签名并实际验签，固定源清单已准备且未上传；已安装 0.1.0 保留。更新页原生检查已随编译版本验证，实际 main / mini WebView 场景退出 0。完整自动升级仍未验，发布顺序答复前不推送 / 发布。见[候选与验证记录](delivery-status.md#m16n011-本地签名升级候选与升版后的原生更新页)。

0.1.1 候选安装包：[TokenPulse_0.1.1_x64-setup.exe](../../target/release/bundle/nsis/TokenPulse_0.1.1_x64-setup.exe)，SHA-256 `6f50baa9cb7806382ab7b3628d7563729aab0278c8bb074e771ce86c33c3e13a`；[候选清单](../../target/release/publish/v0.1.1-20261003-ff6162077a1c4c4a844c94a87b65621a/latest.json)与[实际验签报告](../../target/release/publish/v0.1.1-20261003-ff6162077a1c4c4a844c94a87b65621a/release-verification.json)。它是未安装 / 未发布的本地候选，原 0.1.0 基线保留供完整升级验收；当前 release 验证程序绑定 0.1.1，不能将它用于生成旧版本的成功报告。

本地正式包验签使用 `pwsh -NoProfile -File scripts/verify-release-artifact.ps1 -Installer <安装包> -Signature <签名> -Report <新报告路径>`，父目录必须存在，报告不得已存在。入口明确等待本次 release 维护进程退出并检查它的退出码，不能直接调用 GUI exe 后读取可能残留的 LASTEXITCODE。安装验收的显式本地状态入口已复用此脚本；检查不启动 Tauri、安装器、网络或读取签名私钥。见[当前验收总表](delivery-status.md#当前范围核对2026-10-03m16n)及[本轮验证](delivery-status.md#m16m显式等待-release-验证进程与当前验收总表同步)。

M13g4：2026-10-03 用户解锁后，已有 `check_taskbar_wire` 真实鼠标 / 焦点 / 布局检查，以及 `check_taskbar_accessibility` 的显式 actions 标志均退出 0，五个原生菜单 Invoke 到宿主动作通过。合成动作不转发正式应用，物理键盘 / Narrator 与标准应用托盘菜单仍独立。见[当前原生验收记录](delivery-status.md#m13g4解锁后的真实任务栏输入与五项-uia-菜单动作)。

本机已按用户授权安装 TokenPulse 0.1.0，标准用户目录为 `C:/Users/Amin/AppData/Local/TokenPulse/`；正式安装后总览独立运行、文件 / 注册匹配通过，保留安装供使用。现有数据目录保持，基本安装 / 普通卸载已实测，不再以缺少独立虚拟机阻塞这两项；更高版本完整自动更新及物理系统项目仍需分别验收。见[本机安装记录](delivery-status.md#m16l用户授权的本机真实安装启动与普通卸载)。

在用户明确授权复用本地状态后，使用 `pwsh -NoProfile -File scripts/verify-installer.ps1 -UseExistingLocalState`。该标志只允许现有数据 / 无 exe 的保留产品路径键，不允许替换已有注册安装或正在运行的应用。测试前只保全 db / WAL / SHM 和旧产品键并核对实际 release 签名，保全文件留在 UUID Temp 目录，不自动恢复覆盖。当前正式安装存在时会先拒绝，普通用户直接通过系统安装器维护 / 卸载，测试脚本不会自动移除它。

本机首次默认菜单序列在第二次冷启动菜单未打开时失败，分步自有命令正常退出与卸载已完成；随后 `-UseExistingLocalState -OwnTrayCommands` 的完整序列退出 0，卸载前后数据库 SHA 相同。OwnTrayCommands 是明确的程序化自有命令场景，不算物理菜单 / 输入通过。窗口辅助定义共用 `scripts/installer-window-probe.ps1`；安装器仍为 M16j 同包，没有新增发布渠道或演示数据。

发布顺序按 2026-10-03 用户确认：全部验证完成后再推送到 GitHub。当前仅本地构建、签名、验签与验收，不推送代码或公开发布资产；待环境验证的项目不计通过。项目签名密钥用于开发者发布，用户安装 / 更新不需输入或验证密钥，应用自动验签。

2026-10-03 最新范围更正：继续进行 Windows 10 验收，Windows 11 适配 / 验收取消；后续历史 Win11 待办不再是交付门槛。用户已选择新项目更新签名密钥，公钥默认内嵌源码；M16h 及更早包没有此公钥，新签名包另作记录。

## 项目更新签名密钥与本机签名

用户明确选择新建后，在 `C:/Users/Amin/.tokenpulse/release-signing/459776be74444bae8b6037bcc8899b5b/` 生成加密 `tokenpulse-update.key` 和公钥副本，随机密码由当前 Windows 用户的 DPAPI 加密到 `password.dpapi`，本机选择记录在上层 `active.json`。根目录禁用权限继承，仅当前用户与 SYSTEM 可访问；没有打印私钥 / 密码，也没有放入 Git、应用资源或数据库。此目录用于后续版本签名，请保留。应用只内嵌跟踪的 `src-tauri/resources/updater-public-key.txt`，公钥正文去除尾换行后的 SHA-256 为 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`。

正常构建默认使用该项目公钥，不再要求手工设置环境。`TOKENPULSE_UPDATER_PUBLIC_KEY` 保留为显式构建覆盖，用于隔离签名夹具或受控发布；空 / 非法覆盖仍使更新不可用。正常应用和 release 验证入口共用同一个编译公钥函数。项目密钥使用本机已安装 Tauri CLI 生成，签名 / 公钥职责参见[官方更新说明](https://v2.tauri.app/plugin/updater/#signing-updates)。DPAPI 密码需要本机当前 Windows 用户上下文，应用运行无需读取这些本地签名文件。

本机签名：先 `npm run tauri:build`，再 `npm run release:sign`；后者默认签同版本 `target/release/bundle/nsis/TokenPulse_<版本>_x64-setup.exe`，输出旁边 `.exe.sig`。也可 `npm run release:sign -- -Installer <同版本同目标安装包>`。脚本核对目录权限、当前用户、有限登记 / 路径、公钥摘要与仓库公钥一致，拒绝 reparse point、超限文件、错误版本名或已有签名；CLI 输出捕获不打印，密码只进入签名子进程环境。签名绑定当前项目版本，安装包字节变化时拒绝发布。脚本不自动生成 / 覆盖密钥，不上传资产、不执行安装器。

签完后使用下方 `release:prepare` 的实际 release 程序复核签名、文件和版本，生成新的本地资产目录；正式公开发布与实际完整升级分别验收。若同名 `.sig` 已存在，签名脚本拒绝覆盖，需要明确使用新的发布产物目录；不要混用旧包 / 新公钥。下方旧“未配置公钥”记录表示当时包状态。

已安装 0.1.0 基线包为 M16j：[TokenPulse_0.1.0_x64-setup.exe](../../target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe)，6,624,606 字节，SHA-256 `9e3c22780697dee8c8d38b1699a592aa833469dfdce9809294a92e275133f250`。旁边 `.exe.sig` 是已验证的 Tauri 更新签名，应用自动更新时用它验签；用户安装和更新均无需手动输入密钥。正式前端 / release 宿主 / 桌面 / 第三方声明 / NSIS 全部通过，内嵌新项目公钥并保留位置 / 尺寸 / 同 DPI 工作区适配及 Tab 修复。下方旧包状态按历史保留；Windows 11 已取消，Windows 10 物理兼容与新包安装 / 完整升级仍继续。详见[最新构建记录](delivery-status.md#m16j新项目公钥下的安装包与本地签名发布资产)。

已验证本地发布候选：[latest.json](../../target/release/publish/v0.1.0-20261003-4a487957145b49a5ac0f1e748eec7e1b/latest.json)及[签名验证记录](../../target/release/publish/v0.1.0-20261003-4a487957145b49a5ac0f1e748eec7e1b/release-verification.json)。同目录包含可上传的安装包与签名。当前未上传，公开固定清单的未认证 HEAD 请求返回 HTTP 404，不能当作实际联网更新完成；这套 0.1.0 包是新公钥初始基线，完整自动升级还需要后续更高版本发布资产及对应本机升级验收；当前发布顺序问题答复前不上传。

## 主窗口位置冷启动验收

M15f3 已追加合成 960×600 DIP 工作区在真实主窗口上的尺寸适配 / 四边检查及 React 导航 / 页脚滚动检查，随后恢复本次普通窗口继续退出保存验证；不修改系统 DPI / 工作区。浏览器对应检查为 `npx playwright test tests/ui/overview.spec.ts --grep 'small work-area' --workers=1`，两项截图只用测试桥 DTO。M15f4 进一步用合成同 DPI 较大旧屏内存记录、真实超大窗口及普通 Moved 事件验证生产检测 / 适配，随后同屏事件保持原普通几何；脚本要求 SAME_DPI_TRANSITION_OK。没有物理多屏切换或 DPI 改动。M15f3 / M15f4 现均已进入 M16h 包。

先 `npm run build`，再运行 `pwsh -NoProfile -File scripts/native-main-window.ps1`。三个独立 debug 应用进程共用本次 UUID 隔离数据库，只接受 seed / restore / missing 阶段，不能输入任意数据路径。实际普通移动与快速最大化、最小化不覆盖位置、关闭隐藏 / 重新打开、下一进程启动恢复、合成原屏缺失时主屏工作区夹紧通过；冷启动预期在正式恢复前冻结，不依赖恢复后保存值；最后移动在数据库尚未保存时立即退出，下一进程必须读到独立已知的最终位置，验证退出保存。Win10 19045 / 150% SEQUENCE_OK、退出 0，1412 提示仍保留，无真实来源 / 账户或性能测试。

系统窗口检查为实际原生 API 和 WebView 程序化检查，不能替代物理拖动 / 断屏 / 多屏 DPI / Win11。详见[交付记录](delivery-status.md#m15f2主窗口原生捕获与冷启动恢复)。本模块提交时，下方 M16e 安装包尚未包含主窗口位置功能，新包另作记录。

## 任务栏 Tab 导航检查

`cargo test -p token-pulse-taskbar windows::canvas::tests --lib -- --test-threads=1` 包含可用按键条件及实际自有窗口的 IsDialogMessage / GetFocus 检查：Tab 进出、失焦关闭详情和 Enter 单次动作。四项通过，未向系统其他窗口发送输入；这是实际原生消息导航而非 Explorer 物理键盘验收。最新真实 wire 仍在自有前台夹具被 CoreWindow 覆盖时拒绝发送输入，历史一轮通过与本次限制分别记录。见[交付记录](delivery-status.md#m13g1任务栏按键请求与原生-tab-导航)。

## 任务栏 UI Automation 名称检查

先 `cargo build -p token-pulse-taskbar --bin token-pulse-taskbar-host`，再运行 `cargo run -p token-pulse-taskbar --example check_taskbar_accessibility -- --native-taskbar-accessibility-development-check`。显式场景会用合成固定夹具临时嵌入真实独立宿主、通过自有 WM_CONTEXTMENU 打开标准菜单，并只查询 UIA 名称 / 焦点属性 / Invoke 模式，不调用模式或发送输入。原生 Privacy ACK、私有字段移除和退出后的原任务栏几何必须通过，最后 NATIVE_TASKBAR_UIA_OK / 退出 0。Win10 19045 / 150% 已验证；实际 Narrator / 物理键盘 / Win11 保留。详见[交付记录](delivery-status.md#m13g2真实-ui-automation-名称与原生菜单模式)。

将唯一标志换为 `--native-taskbar-accessibility-actions-development-check` 才尝试实际 UIA Invoke / 五个受限意图，要求菜单真实可命中并处于当前输入 / 前台归属；不向正式应用转发动作。该模式在本机 CoreWindow 覆盖时调用前拒绝，退出 101，不计动作验收通过。默认只读模式保持独立，详见[动作入口与限制](delivery-status.md#m13g3受输入归属保护的-uia-菜单动作验收入口)。

当前最新完整 NSIS 包（M16e）：[TokenPulse_0.1.0_x64-setup.exe](../../target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe)，6,605,439 字节，SHA-256 `2e8eb45072009cd9ae862afb604a09d07a6e3429ba2dce6169896fbbfe63410b`。同次正式 TS / Vite、release 宿主 / 桌面、334 项第三方声明和 makensis 全部通过；旧段落中的包哈希按历史保留。包含最新正式空态说明，账户 / notify 选择器验收代码只在 debug 中。此轮未覆盖现有正式数据进行安装，未生成正式签名或发布；干净安装 / 卸载及完整更新不能由打包通过替代。

本轮按用户允许重试任务栏 wire，输入桌面可打开，但自有前台夹具仍被全屏 Windows.UI.Core.CoreWindow 覆盖，命中保护在发送输入之前拒绝 / 退出 101；不计真实输入通过、不绕过保护，历史 M13e7 通过证据保留。详见[本轮记录](delivery-status.md#2026-10-03本轮任务栏真实输入复测)。

## Windows notify 目录选择器验收

先 `npm run build`，再运行 `pwsh -NoProfile -File scripts/native-smoke.ps1 -NotifyDialogs`；只构建 debug 桌面应用，使用新的 UUID native-probe 数据库和自有 synthetic-notify-dialog-home。实际 React 按钮打开正式 Windows 文件夹选择器，canonical 目录限制在配置读取之前执行。取消 / 选择 / 关闭预览均不写配置，明确确认才启用；停用也必须先预览再确认，字节级核对原 notify、注释 / CRLF 与后来新增的用户设置。测试不注入能力、不读真实 Home 或认证，也不执行真实 Codex 回合。

Win10 19045 / 150% NATIVE_NOTIFY_DIALOGS_OK 退出 0，账户 / 来源选择器回归也通过。三项 `npx playwright test tests/ui/notify.spec.ts`、desktop strict Clippy / release / fmt 与脚本解析通过；1412 提示保留。此为真实系统控件的程序化验收，物理输入 / Win11 / 真实回合分开；没有性能测试或新安装器构建。详情见[交付记录](delivery-status.md#m15a8真实-notify-目录选择与确认撤销流程)。

## Windows 账户程序 / Home 选择器验收

先 `npm run build`，再执行 `pwsh -NoProfile -File scripts/native-smoke.ps1 -AccountDialogs`。脚本构建合成 quota-fixture 和 debug 桌面应用，不构建任务栏宿主；实际 React 设置按钮打开正式程序 / 文件夹选择器，不注入能力句柄。仅显式 native-smoke、开发应用身份及 UUID native-probe 隔离库允许；canonical 目标必须属于本次 synthetic-account-dialog-home，其他选择在 inspect / 签发前拒绝。默认入口不会检测或连接真实账户。

Win10 19045 / 150% 已通过四个取消 / 选择步骤、草稿保持、保存不启动、连接 / 75% 实际 DTO 渲染、受限刷新、断开清空并保留配置及源字节检查，NATIVE_ACCOUNT_DIALOGS_OK / 退出 0。既有来源选择器回归也通过。此为实际系统控件的程序化交互，物理输入 / Win11 分别验收；1412 提示保留，无性能测试。自动页面检查：`npx playwright test tests/ui/account-service.spec.ts` 四项通过。详情见[账户验证](account-quota-verification.md#m12m真实程序home选择器与普通连接流程)。此步骤未重新打包既有安装器。

## Windows 来源选择器验收

先 `npm run build`，再运行 `pwsh -NoProfile -File scripts/native-smoke.ps1 -SourceDialogs`。仅 debug、显式 native-smoke 及 UUID native-probe 数据目录可运行；辅助脚本打开并操作测试进程自己的真实 Windows 文件夹选择器，取消 / 输入 / 确认后检查 React 来源管理和只读后台导入。控件定位核对 PID / 标题 / 类别 / ID，使用有界原生消息，不进行全局输入、抢其他进程焦点或操作其他窗口。误选路径在正式回调签发句柄之前必须拒绝，源码中这一限制仅用于本显式验收场景。

Win10 19045 / 150% 已完整通过：NATIVE_SOURCE_DIALOGS_OK / 退出 0，17 Token、暂停 / 恢复、移除保留历史、真实总览 DTO 的未知分项 / 未计价及源字节检查一致；1412 退出提示仍保留。实际系统选择器的程序化操作与物理鼠标 / 键盘分别记录；此入口不验证账户程序 / Home 或 notify 的选择器，不读真实来源或认证，不运行性能测试。`npx playwright test tests/ui/sources.spec.ts` 是单独的合成 UI 回归。此源码修改尚未覆盖下方 M16d 安装包的既有哈希。

## 本地签名发布准备

`npm run release:prepare -- --desktop <同次构建的桌面exe> --installer <NSIS安装包> --signature <安装包.sig> --output <尚不存在的输出目录> --published-at <UTC时间> [--notes <UTF-8说明文件>]` 只生成本地资产，不上传 GitHub、不生成密钥、不读取认证或正式数据。UTC 时间采用 `2026-10-03T00:00:00.000Z` 形式；版本必须在 Cargo workspace、package.json、tauri.conf.json 一致，安装器名称必须为 `TokenPulse_<版本>_x64-setup.exe` 或对应 arm64。生成清单固定使用本仓库 `releases/download/v<版本>/`，无任意发布 URL 参数。

发布者先选择版本并同步上述三处与 package-lock 的根版本，再用 `cargo metadata --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc > $null` 同步 Cargo.lock，确认仅项目包版本变化；`--no-deps` 不会完成此锁文件同步。然后执行 `npm run tauri:build`，默认使用已经确认的新项目公钥；本机 `npm run release:sign` 解密 DPAPI 密码并签同版本包，无需把私钥 / 密码填入命令或聊天。必须使用这次构建的桌面 exe 与 NSIS 安装包，不混用旧产物。如确实使用另一套受控密钥，才显式设置 `TOKENPULSE_UPDATER_PUBLIC_KEY` 覆盖并由发布者提供相应签名；处理后恢复构建环境。准备工具不处理私钥。

桌面 exe 的维护入口 `--verify-update-release <安装包> <签名> <新报告路径>` 使用实际编译公钥、编译版本与 target。release 构建才能执行，debug 返回 13；参数错误 12，验证 / 文件失败 14，成功 0。签名全局验证成功后才读取可信 version 字段；缺失、重复或与当前版本不符拒绝。没有初始化 Tauri / 单实例 / 数据库 / 来源 / 账户或运行安装器，报告 create_new，不覆盖旧文件。该入口不是 renderer IPC，也不授予主窗 / 小窗文件或任意验证权限。

准备流程核对安装器 / 签名 SHA-256、精确字节、target、版本和公钥摘要，说明受相同 DTO 的 UTF-8 字节 / 控制字符限制，检查过程中改变桌面 exe 则失败。输出含安装包、原签名、latest.json 及验证记录；先放本次自有临时目录，再发布到新目录。已有目标或并发创建目标均拒绝，失败只清理已校验的自有临时目录。验证记录只能证明这些字节的签名、公钥和版本，不能独立证明任意安装包内含哪个应用；同次正式打包是发布操作的前提，实际安装 / 更新仍须另验收。

必要自动检查：`node --test scripts/prepare-update-release.test.mjs`；`cargo test -p token-pulse-desktop --lib release_verifier`；显式运行真实临时签名夹具 `cargo test -p token-pulse-desktop --lib release_verifier::tests::actual_signature_version_and_key_binding -- --ignored --exact`。后者调用已安装 Tauri CLI，临时私钥生成 / 签名输出被捕获且不打印，合成文件从不执行，临时目录自动清理；不生成正式项目密钥。项目公钥现已配置，发布资产准备就绪仍不代表已经公开发布或完成实际升级，不运行性能测试。

本步骤完整打包通过，最新 0.1.0 NSIS 为 6,605,578 字节，SHA-256 b44ca1fd19aa5c0d782ff905335b15240092d311e9a00a536a75fafdc7de1c62；仍无正式公钥 / 发布签名。Win10 实际新 release exe 缺配置返回 14 / 不创建报告，准备脚本返回 1 / 无发布或暂存文件，通过两项 NATIVE_RELEASE_*_REJECT_OK；未运行安装器或触碰现有正式数据。以下各步骤旧包大小 / 哈希仅为当时产物记录。

## 第三方声明生成与打包

`npm run notices` 为本地 x64 Windows 生成 src-tauri/resources/third-party-notices.txt；其他已支持的 MSVC 目标由 prepare-desktop 传入实际 target。需锁定 npm 依赖和已获取的 Cargo registry 源，生成器调用 cargo metadata --locked --offline，只读取依赖 / 审查许可文本，不读账户或来源日志。默认正式 `npm run tauri:build` 自动生成，并由 NSIS 放到安装目录 THIRD_PARTY_NOTICES.txt；普通运行无需 Node / Cargo / 编译器。此生成文件忽略，源码跟踪生成器及 scripts/third-party 的十份审查原文 / SHA-256 / 来源。

2026-10-03 M16c 完整 NSIS 重建通过，当时 0.1.0 本地包 6,603,716 字节（6.30 MiB），已包含声明资源及正常卸载指令。SHA-256 fd29f91b1728f4437022fba72d665af6398c45f7f4f77e302dab8d421797e158；仍无正式更新公钥 / 发布签名，不替代实际干净安装或更新。原文快照在 .gitattributes 标为 -text，保持不同 Git 换行设置下的原字节 / 哈希。

`node --test scripts/third-party-notices.test.mjs` 为五项必要校验，覆盖缺正文 / 未审查版本拒绝、源文本完整性、目标图 / npm 运行依赖和 vendor 声明。当前生成覆盖 334 项：327 Rust Windows 图含构建 / 测试、四前端运行依赖和 SQLite / NSIS / helper 三项；并非都实际链接进运行时。重复生成字节一致。新依赖缺许可正文、缓存工具变化或锁版本变化必须补齐上游原文 / 审查来源，不能用 generic MIT 代替作者许可。NSIS / helper 当前审查为 3.11 / 0.5.3，首次构建不要求旧缓存，已有缓存则验证。

verify-installer 检查已安装声明与准备资源的哈希相同，普通卸载移除声明同时保留 SQLite。必须满足原有干净配置条件才运行；已有正式数据或注册时保持拒绝，不为声明验证覆盖它们。完整构建与生成安装脚本可以验证资源进入打包指令，但不能替代实际安装 / 卸载。正式发布公钥 / 签名及实际更新仍待外部发布条件，无性能测试。

## 软件更新 UI 验证

2026-10-03 接入更新 UI / 原生提供方 / 安装退出钩子后，`npm run tauri:build` 完整重建通过；本地最新 0.1.0 NSIS 包为 6,493,332 字节（6.19 MiB），替换原早期 5.22 MiB 开发产物。最新包尚无正式发布公钥 / 签名，不能当作真实自动升级就绪。生成脚本已核对 update-hooks 在占用检查前执行，宿主仍 externalBin 同目标打包；此前 M16a 安装记录不自动适用于新包，需要另在干净配置验收，不覆盖当前正式数据或安装注册。

`npx playwright test tests/ui/updates.spec.ts` 使用测试文件内显式合成桥，检查阶段 / null / 精确修订与字节、签名失败无安装权、绑定确认、迟到 / 隐私 / 离页和重开。正式 UI 仅调用原生命令，无开发数据默认入口。开发截图位于忽略的 test-results/updates-*-review.png；深浅 / 960 宽已查看。

先 `npm run build`，再 `pwsh -NoProfile -File scripts/native-smoke.ps1 -Updates`，会通过真实 React 设置按钮检查当前编译公钥下的更新页、刷新、共享隐私公开版本及权限 / null 回归；合法公钥对应 idle / 尚未检查，未配置或非法显式覆盖对应 unavailable。合法公钥时不调用检查联网，未经候选的下载拒绝；mini 与直接 plugin 命令继续拒绝。需 NATIVE_UPDATES_IPC_OK / 退出 0。此前 Win10 19045 的未配置场景通过属于历史证据；新公钥场景另记，WebView2 注销 1412 保留。真实发布 / 安装仍需正式发布资产，不用隔离页检查替代。

## 更新安装生命周期验证

新增显式启动交接检查：`cargo test -p token-pulse-desktop --lib update_transport::install_acceptance::signed_nsis_handoff_uses_exact_arguments_and_requests_exit_once -- --ignored --exact --nocapture`。需要本机已安装 Node / Tauri CLI 与缓存的 NSIS 编译器；默认测试不执行。它在自有 Temp 编译只写标记的无界面 NSIS，用临时密钥签版本 99.0.0，再经过正式下载 / 验签 / 版本 / staging / CreateProcess 通路，独立核对真实参数、旧修订 / 重复安装拒绝和单次退出回调。Win10 NATIVE_SIGNED_NSIS_HANDOFF_OK 已通过；仅回调计数，不实际退出 Tauri，不安装产品或验证完整升级。测试原始签名输出不打印；正式 HTTPS / 固定发布源与安装门禁不变。详见[交接记录](delivery-status.md#m16k真实签名-nsis-的原生启动交接)。

`pwsh -NoProfile -File scripts/verify-update-hook.ps1` 用已下载 NSIS 编译器生成自有 Temp 最小夹具，仅写同目录测试标记。验证安装器在父进程存在时等待，父进程正常退出才继续；通过 NATIVE_UPDATE_HOOK_OK 和退出 0。脚本检查拥有的 UUID Temp 路径，最后只删除该夹具和关闭其自有进程。该检查不安装产品、读取账户或改变 Explorer，不能替代完整安装 / 更新。

`cargo test -p token-pulse-desktop --lib update_installer` 验证非可执行格式和系统 CreateProcess 拒绝，签名真实夹具进一步验证失败不请求退出。正式安装只允许 production NSIS 包，经 CAS 和验签对象核对后原生启动；debug 无安装能力。新 NSIS 包必须包含 windows/update-hooks.nsh，先等待本应用正常退出再做默认占用检查，避免在恢复任务栏时被强杀。当前完整正式更新仍待发布条件 / 系统升级验收；成功启动的公开签名安装器保留在系统 Temp。

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
