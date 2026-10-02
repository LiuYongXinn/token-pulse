# 任务栏原生宿主协议

本文件落实 [任务栏显示设计](taskbar-display.md) 与 [IPC 契约](ipc-contracts.md) 的独立宿主边界。M13a 已建立共享 Rust 协议库 `token-pulse-taskbar`，供后续主进程管理器与原生宿主共同使用；本阶段尚无宿主可执行程序、命名管道或 Explorer 嵌入，不把协议握手成功视作嵌入成功。

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

库层身份绑定不代替 OS 通道安全。后续 Windows 管道仍必须限制当前用户 ACL、验证启动子进程身份、控制进程所有权与有界读写 / 心跳；当前阶段不声称已完成这些检查。

## 消息与展示投影

主进程消息已有 `hello`、`snapshot`、`privacy`、`heartbeat` 和 `shutdown`；宿主回复已有 `ready`、`heartbeat`、`privacy_applied`、`stopped` 与 `action`。动作仅为打开小窗、打开同范围统计、打开任务栏设置、修改隐私及停用任务栏，没有文件路径、任意窗口、Shell 或任意 RPC。布局偏好、实际嵌入状态与能力回复随原生适配器模块接入，不能用当前 ready 表达 embedded。

`TaskbarView` 仅由原子小窗用量快照及独立账户快照组成：精确数据 / 价格 / 设置修订、用量时间、统计时区、范围显示名称、总量与输入 / 缓存 / 输出分项、覆盖状态、分币种精确费用、已计价 / 未计价 Token，以及当前额度桶 / 实际周期。原始日志、SQL、真实目录、认证与聊天字段不在宿主契约中。

没有已确认消费且覆盖未知时，总量为 null；完整覆盖的真实零保留十进制 `"0"`。分项缺失保持 null，金额保持 15 位定点精度。账户窗口保持实际周期和零 / null，不按 primary / secondary 推断周或短周期。展示字段、币种、窗口数量 / 唯一性 / 百分比在接收端再次验证。

## 隐私与失效

`HostSession` 初始封闭，必须先收到正确 hello。新的 privacy 设置清除接收器持有的旧展示数据，再产生 privacy_applied；原生渲染器接入后，还须在发送该确认之前清除画面。新快照必须匹配当前隐私值，设置修订不能倒退。开启时快照不包含范围名称、费用或账户快照，Token 分项按既有应用策略保留。

旧实例 / nonce、倒退序号、错误状态、过期设置或敏感隐藏快照使会话关闭并清空缓存；随后不能靠另一条 hello 复用旧会话。shutdown 与显式断连也清空。主进程发送失败或不能确认清屏时，应停止 / 隐藏宿主；该策略的进程与 HWND 行为留待管理器及渲染器实际验收。

## 契约生成与当前验证

`schemas/taskbar-host-v1.json` 从同一 Rust 类型生成，包括主到宿主及宿主到主两个方向，nullable 字段按实际序列化契约生成。CI 已加入漂移检查：

```powershell
cargo run -p token-pulse-taskbar --example export_host_contract
cargo run -p token-pulse-taskbar --example export_host_contract -- --check
cargo test -p token-pulse-taskbar
cargo clippy -p token-pulse-taskbar --all-targets -- -D warnings
```

2026-10-02 完成 9 项协议功能测试：分片 / 连帧、截断 / 超限、未知字段 / 任意动作拒绝、大整数 / 金额 / null、身份与握手、隐私清空和迟到旧快照、序号 / 修订 / 关闭、非法额度、展示投影与隐私脱敏。首次测试发现 Serde 无字段枚举会忽略附加字段，已将这类消息改为严格空结构变体；相应拒绝测试通过。类型构建、严格 Clippy、格式及 schema 漂移检查通过，未执行性能测试。

下一模块实现独立 Win32 宿主、受控进程 / 管道和实际任务栏探测，再完成安全空间预留、绘制、交互、Explorer 生命周期与明确回退。Windows 10 / 11、多屏 / DPI 和真实系统嵌入仍未验收。
