# 任务栏原生宿主协议

本文件落实 [任务栏显示设计](taskbar-display.md) 与 [IPC 契约](ipc-contracts.md) 的独立宿主边界。M13a 已建立共享 Rust 协议库 `token-pulse-taskbar`，M13b1 已提供独立 Windows 原生程序及受控命名管道；Explorer 嵌入与渲染尚未接入，不把协议握手成功视作嵌入成功。

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

连接与每次交换最多等待 5 秒。宿主连续 15 秒没有完整主端消息时退出；后续管理器按 5 秒发送心跳。当前 `HostConnection` 提供串行交换，不负责后台调度，也尚未接入原生动作通知。回复必须匹配消息种类、隐私值、身份和严格递增序号；错误立即断连并关闭自有进程，即使连接对象仍存活也不能继续交换。生产调用所有读取都有截止时间；半帧超时后关闭会话，不从半帧继续读取。

## 消息与展示投影

主进程消息已有 `hello`、`snapshot`、`privacy`、`heartbeat` 和 `shutdown`；宿主回复已有 `ready`、`heartbeat`、`privacy_applied`、`stopped` 与 `action`。动作仅为打开小窗、打开同范围统计、打开任务栏设置、修改隐私及停用任务栏，没有文件路径、任意窗口、Shell 或任意 RPC。布局偏好、实际嵌入状态与能力回复随原生适配器模块接入，不能用当前 ready 表达 embedded。

`TaskbarView` 仅由原子小窗用量快照及独立账户快照组成：精确数据 / 价格 / 设置修订、用量时间、统计时区、范围显示名称、总量与输入 / 缓存 / 输出分项、覆盖状态、分币种精确费用、已计价 / 未计价 Token，以及当前额度桶 / 实际周期。原始日志、SQL、真实目录、认证与聊天字段不在宿主契约中。

没有已确认消费且覆盖未知时，总量为 null；完整覆盖的真实零保留十进制 `"0"`。分项缺失保持 null，金额保持 15 位定点精度。账户窗口保持实际周期和零 / null，不按 primary / secondary 推断周或短周期。展示字段、币种、窗口数量 / 唯一性 / 百分比在接收端再次验证。

## 隐私与失效

`HostSession` 初始封闭，必须先收到正确 hello。新的 privacy 设置清除接收器持有的旧展示数据，再产生 privacy_applied；原生渲染器接入后，还须在发送该确认之前清除画面。新快照必须匹配当前隐私值，设置修订不能倒退。开启时快照不包含范围名称、费用或账户快照，Token 分项按既有应用策略保留。

旧实例 / nonce、倒退序号、错误状态、过期设置或敏感隐藏快照使会话关闭并清空缓存；随后不能靠另一条 hello 复用旧会话。shutdown 与显式断连也清空。主进程发送失败或不能确认清屏时立即停止宿主，M13b1 已实际验证进程退出；清除 HWND 画面留待原生渲染器接入后验收。

## 契约生成与当前验证

`schemas/taskbar-host-v1.json` 从同一 Rust 类型生成，包括主到宿主及宿主到主两个方向，nullable 字段按实际序列化契约生成。CI 已加入漂移检查：

```powershell
cargo run -p token-pulse-taskbar --example export_host_contract
cargo run -p token-pulse-taskbar --example export_host_contract -- --check
cargo test -p token-pulse-taskbar
cargo clippy -p token-pulse-taskbar --all-targets -- -D warnings
```

2026-10-02 完成 9 项协议功能测试：分片 / 连帧、截断 / 超限、未知字段 / 任意动作拒绝、大整数 / 金额 / null、身份与握手、隐私清空和迟到旧快照、序号 / 修订 / 关闭、非法额度、展示投影与隐私脱敏。首次测试发现 Serde 无字段枚举会忽略附加字段，已将这类消息改为严格空结构变体；相应拒绝测试通过。类型构建、严格 Clippy、格式及 schema 漂移检查通过，未执行性能测试。

M13b1 新增 1 项实际 DACL 检查和 5 项原生跨进程检查：独立程序握手 / 心跳 / 隐私 / shutdown / Drop 清理、同名管道与两端 PID 拒绝、错误父 PID / nonce / 截断帧拒绝、真实 15 秒无心跳退出，以及参数 / 本地路径约束。共 15 项检查通过；额外补充错误交换后连接对象仍存活时进程已经终止的断言并复跑通过。严格 Clippy、fmt 和未改动的 schema 漂移检查通过。首次断连检查因测试阻塞父端 I/O reactor 而超时，改用异步进程句柄等待后通过；这是功能期限检查，没有运行性能测试。

下一模块实现 Win32 窗口、实际任务栏探测与安全空间预留，再完成绘制、交互、Explorer 生命周期和明确回退。Windows 10 / 11、多屏 / DPI 和真实系统嵌入仍未验收。
