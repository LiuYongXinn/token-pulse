# token-pulse

TokenPulse 是独立运行的 Codex 本地用量统计桌面工具，包含桌面悬浮小窗、Windows 任务栏显示模式、完整统计窗口和常驻后台采集器。

当前公开版本为 [0.1.10](https://github.com/LiuYongXinn/token-pulse/releases/tag/v0.1.10)，已在本机 Windows 10 完成签名安装、真实双屏 / 四档 DPI 检查点与混合 DPI 拖动、任务栏透明读数和真实输入验收。匿名下载 / 版本绑定验签、正式应用 0.1.9→0.1.10 线上升级、正常退出 / 自动启动、15,977 条既有消费事件及核对旧行保留、最新版本检查已通过；普通卸载沿用已完成的独立验收。物理拔线、实际账户身份变化 / 到期重置、缺失 WebView2 安装等外部场景仍明确未验。开发与运行命令见[本地开发](docs/development/local-development.md)，具体已验 / 未验范围见[交付记录](docs/development/delivery-status.md)。

内置 `openai-text-2026-10-02` 离线文本价格目录含 51 个模型、172 条价格事实，当前仅转换 37 条 Standard 参考规则，不能证明请求实际处理模式。自定义 / 来源规则、模型别名、历史价格版本、持久费用缓存和后台重估已接入；独立缓存写入数量与四费率估算已贯通；有可靠单请求证据时的上下文 / 模式选择、生产查询 / 持久缓存 / 后台重估及同快照公开依据已接入并用独立夹具验证。当前真实来源尚无可靠处理模式 / 地区证据；源码已为 `gpt-6.1-sol` 等接入默认 Standard 参考估算，可靠单次请求输入用于选档，缺少时暂用短档，未知写入量在估算中暂按零，具体假设在明细中显示。原始用量保持不变，自定义价格规则仍可覆盖。详见[默认参考费用估算](docs/design/reference-estimates.md)。实际模式 / 地区采集仍是核心证据缺口，工具 / 多模态 / 运行时联网价格更新另列扩展。价格目录不等于全部模型可自动计费，费用估算与账户额度分别计算。详见[模型价格与计费条件](docs/design/price-accounting.md)。

采集诊断页支持明确“重读已启用来源”，用候选验证后替换恢复旧版未识别的日志用量；普通“重建全部账本”继续核算已保存用量。原日志只读，取消 / 失败保留旧结果，暂停来源不打开文件。该增量已进入本机及公开 0.1.3，并以真实正式 UI 完成来源重读。

明细页显示严格关联的单次请求输入、真实零 / 未知、完整消费关联和同快照所选不可变价格规则。生产条件费用链路已经接通，实际模式 / 地区的可靠采集仍未具备；不按全天 / 会话累计或模型最大窗口猜测请求分档，不把目录收录或合成条件金额验证写成全部模型已自动计价。

## 快速开始

1. 从 [GitHub Releases](https://github.com/LiuYongXinn/token-pulse/releases/latest) 下载 Windows x64 安装包并安装；应用自行验证更新签名，无需用户输入密钥。
2. 启动 TokenPulse，在“设置 → 数据来源”检测 Windows 本地来源，或选择自定义 Codex Home。
3. 等待只读历史采集，在总览、模型、项目、会话和明细查看真实统计；未知费用保持未计价。
4. 小窗、任务栏显示及复用已有本地 Codex 登录的账户额度连接，可按需启用。旧版未识别记录可在支持该入口的版本中明确“重读已启用来源”。

当前验证范围为 Windows 10；开发环境、构建命令和具体限制见[本地开发说明](docs/development/local-development.md)及[交付记录](docs/development/delivery-status.md)。

全部已收录文档见[文档索引](docs/README.md)。

## 文档

- [详细开发设计（实施总入口）](docs/design/development-design.md)：统一需求与 UI，包含架构、数据、采集核算、接口与开发验收计划。
- [文档索引](docs/README.md)
- [完整设计方案](docs/design/token-pulse-design.md)：产品行为、采集与统计规则、架构、数据模型、窗口交互、部署和验收条件。
- [UI 设计与交互原型](docs/design/token-pulse-ui.md)：主窗口、悬浮窗与任务栏模式的视觉、布局及状态设计；可直接用浏览器打开 [HTML 原型](prototypes/token-pulse-ui.html)。

- [Windows 任务栏显示设计](docs/design/taskbar-display.md)：常驻读数、交互、原生嵌入与系统适配规则。

方案中的参考代码使用绝对路径定位到原项目 `E:/Documents/Code/jetbrains-cc-gui` 及本机 TokenTracker 源码，具体路径与定位方法见方案第 23 节。
