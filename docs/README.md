# 文档索引

## 需求

- [默认参考费用估算](requirements/reference-estimates.md)：实际模式未知时继续估算，说明默认价格与缺失条件的假设。

## 设计

- [日期切换与统计聚合复用](design/date-switching.md)：CC GUI / TokenTracker 对照、热力图缓存、快捷日期预读、精度边界与验证。

- [会话标题显示](design/session-titles.md)：读取 Codex 标题索引、改名更新、ID 回退、快照和隐私规则。

- [默认参考费用估算设计](design/reference-estimates.md)：Standard 默认估算、请求档位、未知写入假设、公开依据与缓存 v7。

- [详细开发设计（实施总入口）](design/development-design.md)：统一需求与 UI，明确架构、选型、模块边界、三种显示入口、额度接入、性能与安全。
- [数据与存储详细设计](design/data-storage.md)：数据库建表草案、身份与版本、事务、快照、精确金额、重建及既有存储实现；已取消数据维护与迁移保护扩展。
- [模型价格与计费条件](design/price-accounting.md)：区分目录、引擎、请求证据与验证；原安装版参考估算、后续四费率 / 明确来源重读 / 精确请求输入 / 条件选择核心、未接费用链路与证据缺口、扩展范围及官方核实来源。
- [采集与核算详细设计](design/collection-accounting.md)：字节读取、代次、计数流、去重、镜像、分叉和固定回归夹具。
- [IPC 与前端契约](design/ipc-contracts.md)：共享 DTO、命令、事件、分页、权限、错误和原生宿主协议。

- [TokenPulse 完整设计方案](design/token-pulse-design.md)：从原文档迁入，保留完整方案及参考源码的绝对路径，作为本项目独立实现的设计依据。
- [TokenPulse UI 设计](design/token-pulse-ui.md)：桌面主窗口、两种悬浮小窗、任务栏模式、视觉变量、页面交互和状态规则，包含可点击原型与预览截图。
- [UI 方向提案](design/ui-directions.md)：黑白银灰的银雾、碳素、纸墨三版选型记录，已选定银雾。
- [银雾完整 UI 设计与实施](design/silver-mist-ui.md)：已应用到正式界面的七页主窗、五个设置分区、两种小窗、按钮与应用图标，包含 23 个界面预览和验证记录。
- [账户额度显示设计](design/account-quota.md)：短周期和周剩余额度、重置时间、倒计时及独立账户数据源的接入规则。
- [任务栏原生宿主协议](design/taskbar-host-protocol.md)：M13a 有界帧、实例绑定、展示投影、隐私屏障与协议测试。
- [Windows 任务栏显示模式](design/taskbar-display.md)：类似 TrafficMonitor 的常驻读数、悬停详情、原生宿主、系统适配与失败回退。

## 开发与维护

- [项目限制审查](development/restriction-audit.md)：版本与盘符拦截、来源和列表容量、解析兼容性、必要边界及整改顺序。

- [项目目录内的数据与工具缓存](development/project-local-storage.md)：数据、WebView、临时文件、工具缓存和现有数据迁移的路径约定。

- [页签即时显示与统计更新开发方案](development/instant-navigation.md)：查询缓存、前台调度、分页快照恢复、设置与诊断状态共享、变更通知、后端复用及分阶段性能验收。

- [开发实施与验收计划](development/implementation-plan.md)：15 个有效实施步骤（M14 已取消，编号保留）、逐模块交付、环境与构建约定、统计正确性验证、A17–A22 验收与已确认的剩余功能范围。
- [账户额度共享显示与验证](development/account-quota-verification.md)：总览 / 小窗真实 DTO、周期与隐私规则、交互 / 视觉及 Windows 原生验收。
- [本地账户服务检测与验收](development/local-account-detection.md)：程序 / Home 检测、配置草稿边界与自动 / 原生验证记录。
- [本地开发与运行](development/local-development.md)：已建立工程的工具链、运行命令、开发数据隔离与原生运行壳检查。
- [实施与交付记录](development/delivery-status.md)：模块进度、自动与系统验证、待验收条件。

后续需求、设计和开发维护文档分别存放在 `requirements/`、`design/` 和 `development/` 下；各分类在有实际文档时创建。
