# 任务栏原生宿主协议

本文件落实 [任务栏显示设计](taskbar-display.md) 与 [IPC 契约](ipc-contracts.md) 的独立宿主边界。共享 Rust 协议库、独立 Windows 原生程序、受控命名管道、精确绘制、安全布局预留与恢复、后台管理器及正式 UI 已接入；Win10 19045 的实际 embedded 通过生产宿主状态确认，不把握手或配置成功当作嵌入成功。M13e1 的小窗回退由主应用负责，原生宿主仍不读取数据库或创建 WebView。原生鼠标 / 悬停 / 菜单与 Explorer 重建 / 兼容矩阵继续实施，详细阶段记录见本文及交付记录。

## 帧与身份

每帧为四字节小端无符号 JSON 长度，后接 UTF-8 JSON，最大 65536 字节。零长度、超限、截断、非法 JSON 和未知字段拒绝。接收在分配负载前检查长度；发送在序列化过程中限制缓冲区，超限时不向通道写入半个前缀。通道结束与截断分别表达，宿主运行器必须在错误或断连时停止并清除显示。

`Envelope<T>` 包含：

|字段|约束|
|---|---|
|protocol_version|当前为 1；不支持版本拒绝|
|host_instance_id|本次启动独立实例，匹配预先绑定值|
|nonce|启动时生成的 64 位十六进制字符串，匹配本次通道；不记录原值|
|sequence|精确十进制字符串，正数且在本方向严格递增；不能用 JS 浮点计数|
|body|受限消息，拒绝未定义字段与动作|

库层身份绑定不代替 OS 通道安全。M13b1 的 Windows 通道限制当前用户 ACL、校验两端内核报告的进程 ID，并以 Job 控制宿主所有权；读写和心跳有明确期限，详见下节。

## Windows 进程与通道

独立 `token-pulse-taskbar-host.exe` 仅接受本次实例、nonce 和主进程 ID 六个固定参数；没有 Shell、文件操作、账户读取或数据库入口，正常运行不输出原始参数。主进程仅启动本地绝对路径且文件名匹配的原生程序，启动前后检查路径，拒绝网络 / 设备路径和命令 shim。正式部署仍需把该程序打包并从受控安装位置解析，当前未接入 Tauri 管理器或安装包。

先建立 `\\.\pipe\TokenPulse.Taskbar.<随机实例>`，再以挂起状态创建宿主，加入 `KILL_ON_JOB_CLOSE` Job 后恢复自有主线程。启动失败、连接关闭或主进程对象释放都会结束自有宿主，不寻找或终止其他用户进程。[Microsoft Job 限制](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-setinformationjobobject)

管道使用保护 DACL，只允许当前进程用户 SID，句柄不继承；拒绝远程连接，限制一个实例，并拒绝同名管道抢先建立。自定义 ACL 避免使用包含其他访问者的默认安全描述符。[Microsoft 管道安全](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights) 两端在消息读取前用内核进程 ID 校验真实客户端 / 服务端，主端必须匹配刚启动的 PID，宿主端必须匹配启动参数中的主 PID；nonce 再绑定消息会话。[Microsoft 客户端进程 ID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)

连接与每次交换最多等待 5 秒。宿主连续 15 秒没有完整主端消息时退出；后续管理器按 5 秒发送心跳。当前 `HostConnection` 提供串行交换，不负责后台调度，使用 get_actions 拉取有界原生意图，主应用窗口执行继续接入。回复必须匹配消息种类、隐私值、身份和严格递增序号；错误立即断连并关闭自有进程，即使连接对象仍存活也不能继续交换。生产调用所有读取都有截止时间；半帧超时后关闭会话，不从半帧继续读取。

## 消息与展示投影

主进程消息已有 `hello`、`snapshot`、`privacy`、`heartbeat` 和 `shutdown`；宿主回复已有 `ready`、`heartbeat`、`privacy_applied`、`stopped` 与 `action`。动作仅为打开小窗、打开同范围统计、打开任务栏设置、修改隐私及停用任务栏，没有文件路径、任意窗口、Shell 或任意 RPC。布局偏好、实际嵌入状态与能力回复随原生适配器模块接入，不能用当前 ready 表达 embedded。

`TaskbarView` 仅由原子小窗用量快照及独立账户快照组成：精确数据 / 价格 / 设置修订、用量时间、统计时区、范围显示名称、总量与输入 / 缓存 / 输出分项、覆盖状态、分币种精确费用、已计价 / 未计价 Token，以及当前额度桶 / 实际周期。原始日志、SQL、真实目录、认证与聊天字段不在宿主契约中。

没有已确认消费且覆盖未知时，总量为 null；完整覆盖的真实零保留十进制 `"0"`。分项缺失保持 null，金额保持 15 位定点精度。账户窗口保持实际周期和零 / null，不按 primary / secondary 推断周或短周期。展示字段、币种、窗口数量 / 唯一性 / 百分比在接收端再次验证。

## 隐私与失效

`HostSession` 初始封闭，必须先收到正确 hello。新的 privacy 设置清除接收器持有的旧展示数据，再产生 privacy_applied；原生渲染器接入后，还须在发送该确认之前清除画面。新快照必须匹配当前隐私值，设置修订不能倒退。开启时快照不包含范围名称、费用或账户快照，Token 分项按既有应用策略保留。

M13b2 在正确握手后启动专属 Win32 UI 线程，创建隐藏、无激活、无任务栏按钮的顶层控制窗口。该窗口接收 TaskbarCreated、显示器、DPI、主题、设置与电源变更，重新读取探测结果；不向系统发送这些广播，不重启 Explorer。原生缓存通过容量 4 的私有 Rust 队列及无指针的 WM_APP 唤醒处理，外部同号窗口消息无法注入命令或载荷。快照、隐私与 shutdown 更新等待 UI 线程处理完成后才回复；关闭同时释放缓存、窗口、类和线程。M13c1 增加自有读数子窗口；M13c2 增加受控 Rust 队列的原生启用 / 禁用与安全布局租约。生产 wire 及主应用尚未发送启用配置，默认仍隐藏；实际挂接通过显式独立开发验收程序验证。

只读探测在受控 DPI 上下文中读取屏幕矩形并恢复调用线程原上下文，避免将虚拟化坐标混入物理像素。[Microsoft 窗口矩形与 DPI](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect) 目前只接受 Windows 10 build 19045，校验系统目录 explorer.exe、唯一类名、窗口 PID、父子关系、最多 256 个子窗口和区域包含 / 不交叠；额外工具栏、陌生版本、竖向 / 不安全几何返回明确能力错误。Windows 11 适配仍需后续实现与实际版本独立验证，当前拒绝不等于取消该范围。

纯空间计划接受实际测量的宿主宽度及任务按钮区最小宽度，返回两块不重叠矩形；它不调整任何 HWND，也不证明按钮区内全部内容已安全重排。真正嵌入前还要验证窗口代际 / 布局所有权、调用字体测量、安全调整并复核结果。读数父窗口与预留算法接入后方可报告 embedded。

旧实例 / nonce、倒退序号、错误状态、过期设置或敏感隐藏快照使会话关闭并清空缓存；随后不能靠另一条 hello 复用旧会话。shutdown 与显式断连也清空。主进程发送失败或不能确认清屏时立即停止宿主，M13b1 已实际验证进程退出。M13c1 清除原生布局 / 可访问名称并以背景重绘、GdiFlush 完成绘制提交后确认；绘制失败返回错误并退出宿主，不能忽略后发送成功确认。原生位图清除与自有窗口名称已实际检查，系统任务栏上的可见像素清除仍待嵌入后验证。

## 原生文字与画布

生产宿主只接收正式 TaskbarView，不链接夹具或 example。Token 缩写以 i128 商 / 余数舍入，最大整数不做 n×10；金额按 15 位定点舍入到分，避免浮点或接近上限时加数溢出。可访问名称保留完整整数 / 金额；隐私同时清除名称、费用、账户与所有原生布局文字。币种分开显示，重复币种拒绝，null / 真零 / 未计价分别表达。

额度角色只由实际 duration_mins 判断：10080 分钟为周，唯一最短小于周的周期为短周期；同周期多窗口歧义显示未知。0% 保留并使用警示色；旧快照标记旧值，重置到时显示待更新，不自行补成 100%。重置日期使用快照的配置时区；非法时区 / 超出可显示日期明确表达，不退回本机时间。

Windows 字体由 SystemParametersInfoForDpi 的系统消息字体创建；GetTextExtentPoint32W 测量每段文字，与实际 TextOutW 使用同一 HFONT。[Microsoft 文字测量](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-gettextextentpoint32w) 根据实际高度选择两行或单行，再逐级选择完整 / 精简 / 单项；仍不可读返回无布局，不裁切、覆盖或把字段偷偷改成零。颜色支持给定背景的明暗对比和当前系统高对比度；启用时从已验证的任务栏 ReBar 边缘读取一个背景像素，失败明确返回 BackgroundUnavailable，不扩大到桌面采样。主题 / 设置通知触发重新准备，但实际明暗 / 高对比度切换还需兼容验收。

画布是独立 Win32 子窗口，不含 WebView。状态通过稳定 UnsafeCell 分配交给窗口过程；控制窗口另设 busy 屏障，将原生跨进程调用期间重入的状态操作延后，防止再次形成可变状态引用。WM_NCDESTROY 标记本代窗口已销毁，析构不访问复用的 HWND。绘制使用有限区域和资源所有权，位图辅助只绘制本应用内容、限制最多 1,048,576 像素，不捕获其他窗口或写文件；文件输出只在显式开发 example 中。

## M13c2：线程所有的布局租约

仅支持已声明的 Win10 19045 水平主任务栏。探测保留 Explorer 进程句柄和创建时间，操作前后重新检查存活、类名、PID、父子关系及物理矩形；额外 ReBar 工具栏或预留区域内其他根子窗口拒绝挂接。布局互斥量按 Explorer PID / 创建时间命名，限一个 UI 线程持有；任务列表窗口上的随机非零 owner 属性绑定本次窗口代际，不作为可解引用指针。

先按真实文字测量宽度规划，至少保留 320 DIP 任务按钮空间；同步缩小 MSTaskSwWClass，复核任务列表子窗口已跟随缩小、通知区与任务栏容器不变后，才把自有 WS_CHILD 读数挂到 Shell_TrayWnd。子窗口只在已预留矩形内提升到 ReBar 背景之上，不创建桌面覆盖窗或激活窗口。[Microsoft SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos) `SetParent` 不自动修正窗口样式，跨进程 DPI 行为需要单独检查；本实现保持子窗口样式，挂接后再次进入物理坐标上下文并核对实际矩形。[Microsoft SetParent](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setparent)

普通数据 / 隐私更新使用原预留区域，SWP_NOMOVE / SWP_NOSIZE 防止画布重回父窗口原点；无需每次重新挂接。禁用、清空、失效或正常析构先隐藏并脱离自有画布，仅当 owner、Explorer 代际、当前缩小矩形、父容器尺寸和 DPI 仍匹配时恢复原客户端矩形。如果系统 / 其他程序已修改布局，返回 ExternalChange 并保留新布局；失去归属返回 IdentityLost。恢复失败明确报告 Failed，不宣称已恢复。

背景像素用 GetDCEx 的显式 clipping 选项读取验证过的 ReBar 小区域，并在同线程 ReleaseDC；不读取窗口标题或其他应用画面。[Microsoft GetDCEx](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdcex) 最初 GetDC 受容器裁剪而取色失败，改用此有界方法后本机挂接通过。

该模块完成正常路径的实际预留 / 显示 / 更新 / 脱离。M13c3a 已补充已结束宿主的跨进程记录与父端条件恢复，见下一节；仍须完成父端自身异常退出、原生清理有界执行与正式配置状态，因此生产管道尚不启用挂接。不能将正常 Drop 证明当作全部生命周期、Explorer 重启、Win11 或物理多屏 / 四档 DPI 验收，也没有运行性能测试。

## M13c3a：已结束宿主的布局归属记录与父端清理

调整窗口前，把 17 个 u32 字段写入本实例命名的窗口属性：原客户端矩形、缩小后矩形、容器尺寸 / DPI、宿主与 Explorer 的 PID / 创建时间。每个 u32 分成两个 16 位半字加一保存，零仍可表达，空句柄表示缺失；不向跨进程窗口写入可解引用指针。全局 owner 标记另绑定完整 128 位实例 ID，不能仅凭截取的短标记判断同一实例。完整记录及 Prepared 阶段先写入、owner 最后发布；同步调整返回后确认 Reserved。发布失败不执行几何修改；未知阶段、缺失字段、非法范围或不满足最小任务宽度的记录均拒绝使用。[Microsoft 窗口属性](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setpropw)

受控宿主的 UI 线程使用启动参数中的实例 ID，和已验证的管道会话保持一致。父端保留自己创建的 Child 内核句柄，关闭自有 Job 后等待该进程结束；恢复入口先以零等待确认该句柄已结束，再读取 PID 与创建时间，不按一个可能复用的裸 PID 寻找目标。[Microsoft 等待进程句柄](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)、[Microsoft 进程创建时间](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes)

重新探测当前 Explorer，取得布局互斥量后加载同一完整实例记录；宿主 PID / 创建时间、Explorer 身份、窗口代际、当前几何及归属仍匹配，才沿用 M13c2 的条件恢复。匹配记录清理后移除 owner、实例头及本实例属性，并核对删除结果；不删除其他实例的记录。重复清理为 NoRecord，外部变化保持 ExternalChange，错误保留真实失败。若宿主在 Prepared 阶段结束而几何仍为原值，尚不能判断是否存在待执行缩小，返回 Uncertain 并保留归属，不凭原值宣称完成清理。

HostProcess::stop 与 Drop、HostConnection 的错误关闭 / shutdown / Drop 已接入恢复，结果在连接仍存活时可检查。M13c3b 已把生产调用的恢复移入独立清理进程，并增加主进程异常退出监督与期限，见下节；底层同步恢复 API 仍供该隔离进程及显式开发检查使用。未知准备阶段仍需状态表达，不将条件恢复写成所有崩溃 / 无响应 / Explorer 重建已通过。

默认 tests 不调整 Explorer。显式 check_taskbar_exit 开发程序先把自己的子进程加入自有 Job，才允许它显示合成读数；关闭 Job 后没有发送 shutdown 或禁用，父端恢复该已结束进程的实际预留区域。拒绝存活 / 错实例 / 错已结束进程句柄、实际恢复、重复清理、新实例重挂接均在本机 Win10 19045 / 实际 150% DPI 通过，其他进程不被终止。没有运行性能测试。

## M13c3b：独立清理监督与期限

同一个原生宿主程序增加严格的 `--cleanup-guardian` 模式，不新增外部程序依赖。主端在宿主挂起并加入自有 Job 后，通过另一条当前用户受保护随机管道启动清理进程；清理进程位于宿主的 kill-on-close Job 外。双方校验内核管道 PID、实例、nonce 和序号。清理进程持有主端 / 宿主的内核进程句柄，核对宿主 PID、创建时间及与自身相同的原生程序路径，确认都仍存活后才确认 armed；主端收到确认后才恢复宿主线程。任何启动失败均不允许宿主先运行再补监督。

armed 后关闭引导管道，监督不依赖主端继续发消息。主端死亡使自有 Job 关闭、宿主终止；清理进程仍能用已持有句柄执行 M13c3a 的条件恢复。正常关闭也等待该进程的结果，主端不执行 Explorer 窗口调用。固定退出码分别表达恢复、外部变化、身份丢失、Prepared 歧义与错误；普通退出码 0 / 1、强制终止或未知退出码均不能冒充清理成功。

原生恢复在一次性清理进程中执行，独立计时线程在 5 秒期限到达时只终止自身，报告 CleanupTimeout，避免被卡住的跨进程调用和 DLL 退出回调拖住主应用。这个入口不接受可终止的外部 PID。[Microsoft TerminateProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess) 主端停止宿主最多等 2 秒，收集清理结果最多等 6 秒；后续管理器须在后台执行这些有界等待。超时不表示布局已经恢复，原归属记录与真实失败状态继续保留；主端结束而宿主未在期限内结束，也明确报告超时。

自动检查新增严格参数 / 退出码及不同原生程序拒绝，共 35 项通过，另一个被明确忽略的测试仅作为该拒绝测试的私有子进程 gate，不是遗漏验收。已有真实握手 / 关闭检查增加清理进程 PID 独立及最终退出断言。显式 check_taskbar_guardian 在 Win10 19045 / 实际 150% DPI 验证：先挂接开发合成读数，再强制结束自己创建的父进程，独立清理进程恢复全部原几何；另用模拟阻塞操作验证 5 秒超时。没有暂停或故意挂死 Explorer，也没有验证清理进程本身被外部强制终止、整棵进程树被终止、Win11 或物理多屏。生产 wire 仍默认不挂接，下一步接正式配置 / 实际状态、交互、Tauri 管理器及失败回退。

## M13d1：正式启用配置与实际状态

协议增加 `configure` / `get_status` 及 `configured` / `status`。配置携带精确设置修订、启用值和显示偏好（两行 / 单行、Token / 费用 / 额度 / 周重置），拒绝全部内容关闭、旧修订及同修订不同配置。较新配置清除旧设置快照；UI 线程先提交清屏 / 脱离，再应用偏好，确认之后才回复 configured。configured 只确认配置执行，不宣称已嵌入；默认未配置仍禁用，启用但尚无新快照表达 waiting_snapshot。

实际 status 从 UI 线程 receipt 获取，不从握手 / 配置成功推断。包含 nullable 配置修订、精确系统通知修订、disabled / waiting_snapshot / embedded / unavailable、受限失败枚举、实际嵌入密度及 nullable 上次布局恢复结果；不带 HWND、路径或账户字段。embedded 要求布局仍有效且自有读数可见，必须有实际密度；unavailable 必须有原因。画布绘制失败关闭会话，不发送成功状态；对方状态字段组合非法或回复种类不匹配也关闭会话。初始未知配置修订和未发生的恢复保持 null。

隐私修订与总体设置下限分别记录，使同一设置修订的配置和隐私更新可按任一顺序到达；已确认同修订隐私不同值仍拒绝。新配置清除旧快照，新的快照须满足总体设置下限及隐私值。生产仍为一问一答串行协议；鼠标 action / 悬停 / 菜单及后台管理器下一步接入，不能未经处理把 unsolicited action 混入心跳回复。

全套 37 项自动检查通过，包含配置幂等 / 冲突 / 旧快照清除、非法偏好 / 状态组合、nullable 值，以及真实原生进程默认 / 禁用配置状态。显式 check_taskbar_wire 在 Win10 19045 / 150% DPI 经正式 HostConnection、独立宿主与监督进程，验证等待快照、实际嵌入、单行仅 Token 的更窄空间、较新配置清屏、隐私 ACK 后脱离、禁用恢复与无焦点激活。开发程序只使用标记的合成夹具，不链接到生产宿主。读取自身窗口宽度前检查宿主 PID / 自有读数类；只读完整拓扑探测有意拒绝已经缩小的任务区，不把它用作挂接时的全局几何证明。

正式 wire 现在具备显式启用能力，Tauri 应用尚未调用它，因此普通应用仍默认不挂接。下一模块接主端管理器、真实快照 / 偏好、实际状态反馈、受限交互和失败回退；Win11、Explorer 重建与完整兼容矩阵保持待验收。

## 契约生成与当前验证

M13d3 在 Tauri 运行壳接入专属后台宿主所有者及主窗口专用 `get_taskbar_preferences`、`set_taskbar_preferences`、`get_taskbar_status`、`retry_taskbar_embed`，均进入 AppManifest / capability 白名单，mini 无权限。启动从应用可执行文件同目录解析固定原生宿主文件名，缺失时报告不可用；正式安装包仍须包含该文件。首次默认关闭不启动宿主；保存 enabled 后后台读取真实 SQLite 输入及独立可选账户快照，不把账户服务失败变成 Token / 费用失败。原始日志、聊天、账户认证或 Home 路径不进入任务栏 DTO。

后台单一线程拥有连接及 Tokio I/O，SQLite 读在可丢弃结果的阻塞任务中完成；正常每秒读取新快照，慢查询期间仍按 5 秒轮询实际宿主状态，维持心跳。读取代际屏障丢弃暂停 / 设置变更前的结果。主窗口隐藏不停止线程；休眠消息暂停并关闭自有宿主，恢复补读 / 重启。启动及实际挂接失败有限退避，最多 5 次后保持不可用，手动重试 / 新设置 / 系统代际变化重新开放尝试，不在 UI 线程做原生跨进程调用。

隐私和任务栏偏好修改先从阻塞调用者请求暂停发布，并等待实际原生清屏 / 脱离回复，再进入数据库 / 隐私协调事务；完成或失败后恢复查询。该屏障不持有 PrivacyState 锁等待后台，从而避免与序列化锁互相等待。清屏失败拒绝提交隐私设置，不发送“已开启”成功；此前阻塞读取即使后来完成也不能重新显示旧快照。实际状态事件带单调修订、nullable 应用设置修订、最后成功快照时间和真实清理结果，不用零或正常退出码伪装未知。仅在实际 embedded 时把任务栏作为可见账户入口。

明确退出首次请求保留主 UI 分发，由专用等待线程关闭后台所有者及原生宿主，再发最终退出请求；异常父进程退出继续由 M13c3b 独立监督处理。清理失败 / Prepared 歧义保留 typed last_cleanup，不宣称已恢复。失败回退的小窗行为及其实际可见性尚未接入，因此 fallback_visible 保持 null；位置 application_right 明确不可用，不静默使用其他位置。正式设置页与鼠标 / 悬停 / 菜单下一模块接入。

M13d2 增加正式应用的持久任务栏偏好 DTO（默认关闭、显示项目 / 布局、位置和失败回退），显示偏好类型从 core 单一来源导出到应用 TS / schema，并由原生宿主复用，序列化字段保持不变。设置沿用既有 v1 payload 与全局 settings_revision，在 Writer 同一事务保存；精确 CAS、无变化不递增、其他配置保留。旧 taskbar_enabled 仅在新配置缺失时读取，实际修改时统一保存 taskbar 并移除旧键；非法 / 未来配置报错，不用默认关闭掩盖读取失败。

Database.taskbar_input 在同一只读事务取得配置、隐私和既有 mini_usage，统计范围沿用小窗设置；禁用不查询用量。开启而统计时区尚未初始化时返回真实错误，不自行选 UTC 或零值。账户快照由管理器另取，不进入本地账本事务。位置偏好包括系统托盘左侧与应用图标右侧，但当前原生适配只验证前者；后续管理器须明确拒绝未实现位置，不能静默忽略用户选择。当前本步骤尚未接 Tauri 命令或启用 UI。

`schemas/taskbar-host-v1.json` 从同一 Rust 类型生成，包括主到宿主及宿主到主两个方向，nullable 字段按实际序列化契约生成。CI 已加入漂移检查：

```powershell
cargo run -p token-pulse-taskbar --example export_host_contract
cargo run -p token-pulse-taskbar --example export_host_contract -- --check
cargo test -p token-pulse-taskbar
cargo clippy -p token-pulse-taskbar --all-targets -- -D warnings
```

2026-10-02 完成 9 项协议功能测试：分片 / 连帧、截断 / 超限、未知字段 / 任意动作拒绝、大整数 / 金额 / null、身份与握手、隐私清空和迟到旧快照、序号 / 修订 / 关闭、非法额度、展示投影与隐私脱敏。首次测试发现 Serde 无字段枚举会忽略附加字段，已将这类消息改为严格空结构变体；相应拒绝测试通过。类型构建、严格 Clippy、格式及 schema 漂移检查通过，未执行性能测试。

M13b1 新增 1 项实际 DACL 检查和 5 项原生跨进程检查：独立程序握手 / 心跳 / 隐私 / shutdown / Drop 清理、同名管道与两端 PID 拒绝、错误父 PID / nonce / 截断帧拒绝、真实 15 秒无心跳退出，以及参数 / 本地路径约束。共 15 项检查通过；额外补充错误交换后连接对象仍存活时进程已经终止的断言并复跑通过。严格 Clippy、fmt 和未改动的 schema 漂移检查通过。首次断连检查因测试阻塞父端 I/O reactor 而超时，改用异步进程句柄等待后通过；这是功能期限检查，没有运行性能测试。

M13b2 增加 2 项实际 Win32 控制窗口检查和 3 项独立几何预期检查，共 20 项任务栏检查通过。100% / 125% / 150% / 200% 检查使用合成几何、负屏幕坐标、拥挤 / 超限 / 非法结构；不能算作四档实际系统 DPI 验收。跨进程检查补充真实宿主窗口隐藏、快照进入原生缓存后的隐私确认及退出窗口清理，定向复跑通过。系统通知检查只向自有窗口发送合成消息；首次异步发送带指针类型的系统消息被 OS 拒绝，改为带有效参数的有界同步发送，路由通过。

本机只读 example 实际识别 Win10 19045、144 DPI 的主任务栏，任务按钮区右界与通知区左界均为物理坐标 2116；360 DIP 的纯计划宽 540 像素，返回不交叠候选矩形。没有实际预留或 Explorer 布局改变，未执行性能测试。下一模块接入原生读数绘制、安全预留 / 脱离与交互，然后主端管理器、状态 / 回退和 Explorer 生命周期；真实嵌入、多屏 / 各档 DPI、Win11 仍需逐项验证。

M13c1 增加 5 项独立展示预期和 2 项真实 GDI 字体 / 位图检查，任务栏全套 27 项通过。包括最大整数 / 金额、半分舍入、null / 零、实际周期 / 时区、周歧义、旧值、15 种有效内容选择、隐私、测量精简与不足回退；原生检查请求四档字体 DPI、核对全部文字界限，并确认清除后的每个像素只含背景。已有自有窗口检查补充读数子窗口、真实测量宽度、完整可访问名称及隐私后名称 / 布局清空；真实宿主跨进程回归通过。严格 Clippy / fmt / schema 漂移通过。

显式开发 example 生成并人工查看 5 张同一 GDI 代码绘制的合成图：暗色两行、精简、浅色单行、0% 警示和隐私 / 大字号，无裁切。文件带 DEVELOPMENT-FIXTURE 标记，数值 / 价格 / 额度全部合成，不是实际账户或生产 UI 截图；默认 Python 没有 Pillow，直接用图像查看器检查原生 BMP，没有安装环境依赖。四档字体 API 检查不等于物理显示器 DPI 切换。后续继续安全布局 / Explorer 挂接与脱离、交互、主端管理器、背景适配及可见隐私；未执行性能测试。

M13d4 主窗口正式设置页和诊断接入 get_taskbar_preferences / set_taskbar_preferences / get_taskbar_status / retry_taskbar_embed。这四类响应只含无敏感配置或运行状态，沿用 plain Response 身份校验，不携带显示隐私 stamp；用量 / 价格和账户仍经既有受保护通路进入宿主，主窗口 UI 没有接收宿主原始载荷。taskbar_status_changed 仅触发重新查询，旧响应按查询序号与 DecimalInt 修订拒绝；设置使用最初草稿修订 CAS。实际设置确认不替代 embedded，fallback_visible=null 的阶段 UI 明确不承诺自动回退。


M13e1 主端回退最多一个异步工作，绑定发布代际及停止 / 暂停 / 休眠状态；显示前再次核对有效偏好和实际失败。小窗创建 / 原生显示发生在主应用，不发给宿主任意动作或路径；实际结果附在 TaskbarRuntimeSnapshot.fallback_visible / fallback_error，未知为 null，与原生失败分开。主线程非激活显示有 5 秒等待期限，超时取消迟到显示并表达失败，旧代际结果拒绝。Tauri 可见性与原生样式同步更新，不用直接 Win32 显示绕过框架缓存；恢复或关闭回退不会自动隐藏已有小窗。


## M13e2a：有界原生意图拉取

HostMessage.get_actions 是严格空结构；HostReply.actions 含 nullable settings_revision 与最多 4 项受限 HostAction。非空批次必须有配置修订，主端继续验证身份、序号和一请求一回复，拒绝未经请求的旧 action 帧。宿主在 UI 私有队列上验证实际嵌入 / 绘制正常后原子取走； disabled / 脱离时返回空批次，不把缓存意图带到新配置。

单击在 GetDoubleClickTime 的系统期限后只产生 OpenFloat；双击取消该待发单击，只产生 OpenStats，并忽略第二次抬起。动作在单调时钟 5 秒后失效、队列最多 4 项满后拒绝新项。普通同修订 snapshot 刷新保留待发和已排动作；配置修订 / 隐私改变及清屏、脱离、销毁、绘制失败清除计时器与全部意图。自有子窗按下事件不走默认 Explorer 父通知通路。此阶段只实现原生意图 / 通道，主应用执行仍待 M13e2b；开发鼠标整场受锁屏覆盖及前台状态异常限制，未记为系统验收通过，见交付记录。
