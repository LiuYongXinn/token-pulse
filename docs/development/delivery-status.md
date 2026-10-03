# 实施与交付记录

## M09h3a3：无效辅助计价证据不阻断合法消费

采集链复核发现，适配器对无效 token_usage_record 返回通用诊断，会使采集器清除独立会话头证明，连带影响随后有效 Token。现在对明确识别但不能使用的辅助记录只忽略证据，保留原消费证明；缺证据仍未知，不补造请求输入或模式，真正无效的消费记录和原有隔离保持。

新增真实临时来源 / SQLite 检查身份不匹配、未知用量字段、缺少输入量三种情况：合法消费各为 110 Token，一笔事件、零 pending、无 request_usage，原始文件字节不变，重复扫描不重复。core 请求 / 序列 / 写入 15 项和 collector 导入 / live canonical / replay 15 项定向回归通过；core / collector all-targets strict Clippy、fmt / diff 通过，记录在 target/request-usage-invalid-*.log。此验证为自动检查及实际 Windows 文件 IO，未更新正式安装包或声明原生交互验收。受控历史补读和 h3b / h4 条件绑定继续。

## M09h3a2：稀疏计价证据与核算签名分离

提交后兼容复核发现，将新响应 ID 写入已有 request_identity 会改变核算签名；缺辅助记录的旧镜像或分叉副本可能无法与有证据的规范序列对齐。本增量取消该映射，响应 ID 继续只保存在 request_usage 中，配对条件 / 检查点 / 消费原子保存保持，旧镜像 / 分叉的必要核算签名独立于计价证据覆盖。上一条 M09h3a 中“建立响应身份”的阶段描述按本节收敛，不是当前核算字段的状态。

新增独立检查证明同一用量有 / 无辅助证据的签名相同，镜像对齐与父子继承前缀成立，规范消费仍保留精确请求输入。core 请求 / 序列 / 写入 15 项与 collector 导入 / live canonical / replay 14 项定向回归通过，包含实际只读来源跨批次 / SQLite 重开 / 单笔消费；core / collector all-targets strict Clippy、fmt / diff 通过。未改 UI / 公共 DTO / 价格 / 安装包，后续 h3b 条件绑定不从缺证据的镜像猜测响应输入或实际模式。


## M09h3a：可靠单响应输入证据与跨批次检查点

核实官方固定 Codex 源码 b741e480e203f037ca726bc2a76d99a8e8668e66，普通 last 有窗口填充 / 重算路径，不能直接当请求输入；token_usage_record 保存完成响应 usage / response_id / 回合及线程累计，RawResponseCompleted 实时事件不进普通 rollout，配置 service_tier 不等于实际模式。依据与核实日见[计价专题第 8 节](../design/price-accounting.md#8-单响应输入证据的采集m09h3a)。

接入明确 token_usage_record 辅助格式，身份与向量验证后只保留必要字段；同代次位置 / 回合 / 完整 last 和 cumulative 都匹配才关联随后 token_count，并建立有明确提供方的响应身份。pending 随原 ReaderContext 事务保存，request_usage 随必要观察保存，跨批次 / 重启不丢；任意中间非空记录、无效行、元数据或覆盖不符均不沿用，重复计数不复用。辅助记录不另计消费，缺证明保留原 Token 和未知条件；旧 None 字段省略序列化，不自动重解析已跨过历史。

自动验证：core / store / collector lib + integration 42 组 484 项通过，1 既有性能夹具 ignored。新增 272000 / 272001 与累计 / 窗口区分、身份 / 向量 / 代次 / 位置 / 覆盖不匹配、坏记录、checkpoint round-trip 与旧字节；真实 Win10 临时只读来源在 500 条记录分界后重开 SQLite，证据保留且只产生一笔 272011 Token，原始字节 / readonly 属性保持、正文与无关身份不保存、再次核对不重复。测试发现规范会话键经采集器解析为别名，修正关联为来源线程 ID 而非假定内部键的字符串形式；较大的可空证据采用 Box，JSON 不变。workspace all-targets strict Clippy、release-cfg desktop check、契约漂移及 fmt / diff 通过，日志在忽略目录 target/request-usage-*.log。

本模块未改 UI / 公共 DTO / 四费率规则 / 安装包，不将自动真实文件 IO 冒充正式安装原生或用户日志验收。M09h3b 的公开精确输入 / 完整消费绑定 / 分档与混合汇总、M09h4 的实际模式及历史拒绝记录受控补读继续；37 条参考规则保持，gpt-6.1-sol 等条件模型仍未完整自动计价。原有未提交文件分离保留。

## M09h1b：镜像递进证明包含缓存写入

复核发现，弱会话头（没有可靠创建时间 / 请求身份）的镜像序列证明虽然逐条比较了含写入字段的签名，但“完整计数向量递进”辅助证明遗漏写入分项。现在同时要求 before.cache_write_input + last.cache_write_input = after.cache_write_input，nullable 覆盖也一致；无证据的下降、不同增量或已知变未知均不能作为该递进证明。旧全未知写入仍按原完整五项递进处理，不填零、不放宽单记录或身份规则。

独立合成序列覆盖正确 20→30 / last 10、错误 20→35、下降至 10、覆盖变未知及旧 all-null：core 序列 / 写入检查 11 项，store / collector 镜像相关回归 13 项通过；core all-targets strict Clippy、fmt / diff 通过。此模块只收紧必要证明，不改变数量公式、持久事实或 UI，不宣称原生 / 安装验收；没有触发旧格式全历史自动重解析。M09h3 的可靠单请求证据继续按固定官方格式核实。

## M09h2b：四费率规则表列宽收尾

截图复核发现新增写入列后，原六列表格的第 5 列宽度规则仍占用“输出”列，导致普通输入 / 命中标题窄列换行。价格规则表使用独立 class，将有效期宽度绑定实际第 6 列；表头保持完整文字，模型别名表不套用该规则。1280 深色与 960 浅色编辑 / 历史 / 真实零 / 精确写入价的原浏览器场景复测 1 项通过，typecheck / Vite 构建及差异检查通过；查看更新后的规则表截图，标题 / 数值无断行挤压，整体页面无横向溢出。计价算法 / 原生行为未改，本复测不扩大前段 Win10 原生验收结论。

本轮 3 张合成 UI 截图复制保留在忽略目录 [价格界面截图](../../target/page-review/2026-10-03-528b3fb/four-rate-list-dark.png)，同目录含 four-rate-editor-dark.png 与 four-rate-editor-light-960.png；避免后续 Playwright 清理 test-results 时丢失展示文件，不冒充正式安装版或真实收费。

## M09h2：四费率估算与价格编辑器

在 M09h1a 数量链路基础上接入 PriceRule / Draft 的 nullable 精确 cache_write_rate_atoms，增加 schema v12 价格列 / 规范十进制 / 费率上界约束。原迁移不改、旧行保持 null；自定义规则创建 / 替换仍不可变发布，来源 / 别名 / 优先级 / 生效期 / 估价基准复用已完成流程。费用使用普通输入 = 输入 - 命中 - 写入，三种输入类别分别收费，再加完整输出；写入与推理均不重复加到总量或费用中。

已知正数写入缺费率仍 insufficient_usage；真实零不要求该项价格。独立写入费率存在而数量未知、且该未知拆分可能改变金额时不计价；只有同普通输入价或已知其他包含项已占满全部输入才可求确定估算，事实中的 null 不填零。旧三费率规则对旧未报告写入用量保持原参考语义，不声称真实模式或写入零。条件离线目录仍仅生成原 37 条参考规则，gpt-6.1-sol 等条件模型没有因本模块改成固定默认自动价格。

费用 CACHE_VERSION 升至 2，集合 ID / 输入指纹 / 可用缓存过滤同步；旧 ready v1 集合不供新引擎使用，即时补算与独立后台补建复用原服务。真实并发旧 SQLite 读取事务、历史价格版本、重开以及旧缓存排除 / 新缓存补建均验证；价格修改保留 Token、基线及检查点。正式设置编辑器 / 表格增加独立写入费率，区分空未知 / 零免费，保留原布局、主题、历史只读与草稿冲突规则。

自动验证：core / store lib + integration 27 组共 415 项通过，1 个既有性能夹具 ignored；含四项类别 729 个独立精确预期、null / 0 / 正数、等价费率与零剩余输入、超 JS 精度整数、费率界限、v11→v12 旧值保持和实际快照 / 缓存生命周期。Vitest 37 项、价格 / 重估 / 离线目录 Playwright 11 项、类型检查 / Vite 构建、workspace all-targets strict Clippy、release-cfg desktop check、生成契约漂移及 fmt 通过。检查截图深 1280 / 浅 960 编辑器与规则表，无截断或主窗重排；合成 DTO 不冒充真实账户费用。

实际系统：Win10 19045 / 150% DPI，隔离 native-probe 数据，真实 WebView / SQLite / main-only IPC / React -PriceRevalue 场景退出 0。普通 20×10 + 命中 60×5 + 写入 20×30 + 输出 10×20 = 1300 原子；真实表单替换写入价为 0 后自动补建为 700 原子，旧修订费率 30 与三个原缓存保持；手动重估 / 基准 / 历史、幂等 / CAS、mini 拒绝、共享隐私及消费 / 检查点保持一并通过。已安装实例占用恢复键产生 SHORTCUT_CONFLICT，退出 WebView2 注销 1412 继续记录；此场景不替代物理键盘或正式包验收。

证据在忽略目录 target/cache-write-rates-*.log 与 test-results/four-rate-*.png。当前正式安装 / 已发布仍 0.1.2，不含 M09h1a / M09h2；本模块尚未打签名候选 / 安装验证。此前已拒绝历史记录的受控补读、每请求长度 / 实际响应模式及地区证据继续后续计划；工具 / 多模态 / 在线更新为已列扩展。用户原 README / 文档索引等未提交内容保留，相关受托说明更新继续留在其工作区修改中，不混入本模块提交。未读认证 / 聊天正文、未改原始日志、未重启电脑。

## M09h1a：缓存写入数量贯通与真实 Win10 来源链

文档核查 `68039c3` 后继续原任务，实现缓存写入数量链路，不以此宣称完整模型计费。UsageVector 增加 nullable 写入包含项；适配器支持官方 rollout `cache_write_input_tokens` 及明确定义的兼容拼写 `cache_write_tokens`，双字段不同值拒绝，其余未知字段仍拒绝。missing / null / 0 / 正数分别保留；非整数 / 超范围、负数、命中 + 写入 > 输入和总量下界矛盾隔离。累计增量、字段覆盖改变 / 下降、明确阶段重置、镜像 / 分叉必要签名均包含写入；未知写入省略必要 JSON，旧字节与签名不变。

schema v11 只追加 nullable 精确事件列及包含关系 CHECK，旧行 null，历史 migration 保持；观察 / 事件 / 基线 / 检查点仍同事务，重开及候选验证后切换保留写入。查询 / 明细 / 原始向量 DTO 与 Rust 生成 TS / schema 同步，汇总分项独立表达覆盖；rollup v2 排除旧五分项缓存，原始 / 缓存 / 旧快照一致。总览、会话详情、明细新增写入包含项，原 noncached_input 仍为输入 - 命中（含写入），改名“非缓存命中输入”。未知与零均不被 UI 混同，总量不重复加写入或推理。

现有规则仍只有三费率：已知正数写入明确 insufficient_usage，不按普通输入价默默收费；已知零保留独立预期的原三项精确结果。费用查询及持久缓存均复核该行为。四费率、每请求输入长度 / 实际模式与地区证据继续后续模块，当前安装 / 已发布 0.1.2 没有本增量。旧适配器已拒绝且检查点已跨过的历史不会自动重读；受控手动补读 / 实际格式兼容继续收尾，不恢复已取消的全历史自动重解析。

自动验证：core / store lib 与 integration 共 409 项回归通过，1 个既有性能夹具保持 ignored；随后新增费用查询 / 持久缓存的已知零与正数写入检查 1 项通过，Collector lib 1 项通过。新增核心独立预期含 729 组输入类别关系、missing / null / 0 / 正数、拼写冲突、计数流 / 重置与必要签名；SQLite 检查含事务失败保持、v10→v11 旧 null、重新打开、旧快照、rollup、明细原始 / 增量和候选切换。期间修正旧适配器“写入属于未知字段”夹具、旧 schema v8 仿真中未移除新增列，以及早期硬编码 35 表的存储检查；v10 / v11 都是 48 张实际表（含迁移记录表），新增列不增加表。最终相关检查通过，不改变真实用户库。

前端：36 项 Vitest、类型 / 生产构建、29 项总览 / 明细 / 会话 Playwright（包含新增写入展示 / null 与零检查）通过；证据面板截图重取检查另 1 项通过。已查看深色总览和明细面板新图，保留文字导航 / 筛选 / 左右分区，无切字或表格覆盖。截图是显式合成 DTO：`test-results/cache-write-overview.png`、`cache-write-evidence-panel.png`，不是实际账户消费或账单。workspace all-targets strict Clippy、fmt 与生成契约漂移通过；生产 release 配置编译另核对。

实际 Windows：Win10 19045 / 150% 的隔离 `-SourceDialogs` 使用自有合成 Home，经过真实原生目录选择器取消 / 选择、正式 React 添加 / 暂停恢复 / 移除保留、Collector / SQLite / IPC / 总览 DOM；写入 2、总量 17、未知输入、未计价 17、明细必要写入证据及源字节保持通过。NATIVE_SOURCE_DIALOGS_OK，退出 0；记录在 `target/cache-write-native-source.log`。同时保留 SHORTCUT_CONFLICT（运行中的正式实例已占用恢复键）及 WebView2 类注销 1412，不视为键盘恢复通过。没有读 auth.json、保存正文、改真实来源、替换安装程序或重启电脑；原有系统 / 外部条件待验项分别保留，未运行性能测试。

设计与后续方案见[计价专题第 6 节](../design/price-accounting.md#6-缓存写入数量链路m09h1a)和[实施计划第 8 节](implementation-plan.md#8-文本计价缺口与扩展计划2026-10-03)。完整交付继续进行。

## 模型计费状态澄清（2026-10-03）

本条依据已提交源码 `35bc703` / 已发布 0.1.2 追加澄清，保留 M09 历史过程与各自测试结果。工作区缓存写入补齐尚未完成，不能视为已交付。详细设计与官方核实（2026-10-03）见[计价专题](../design/price-accounting.md)，实施项见[计划第 8 节](implementation-plan.md#8-文本计价缺口与扩展计划2026-10-03)。

实际源码确认：

- 内置 `openai-text-2026-10-02` 有 51 个模型 / 172 条文本价格事实，`flat_standard_rules` 仅转换 37 条 Standard / All / 无独立写入价的规则。已接正常启动和正式目录浏览；目录中存在价格不等于全部模式 / 模型可自动计价。
- 已发布 `estimate_atoms` 使用 `(输入 - 命中) × 输入价 + 命中 × 缓存价 + 完整输出 × 输出价`，推理已含在输出中。没有第四种写入费率；没有按请求长度 / 实际模式 / 地区匹配。37 条规则只能表示 Standard 全球 API 参考价，未证明实际 Standard 请求。
- 自定义规则、按来源设置、别名编辑、优先级 / 生效时间、历史价格版本、事件时价 / 指定时点、持久费用缓存、独立后台自动补建、手动重估、进度与取消均已贯通。M09f2 / g1b / g2b3 的自动、浏览器和隔离 Win10 WebView 检查保留；早期 g1a / g2a / b1 / b2 中“尚未接入”是提交当时阶段，不再是当前待办，也不重复开发。

核心剩余缺口：独立缓存写入全链路、单次请求上下文长度及实际处理模式、地区 / 合规端点证据和规则。五字段适配器遇到 `cache_write_input_tokens` 或 `cache_write_tokens` 会拒绝用量；官方 rollout 与 API 字段位置不同，后续不能未经验证混用。写入目标公式必须先从输入扣除命中与写入，再分别套价。`gpt-6.1-sol` 目录已收录，完整自动条件计价未接通；固定自定义价只能估算，缺证据保留 Token 并说明费用无法完整计算。

工具、图片 / 音频 / 实时语音 / 微调费用与运行时联网更新价格均未实现，单列扩展计划，未自动扩大本次范围。当前生产使用随程序打包目录，开发者更新脚本不等于运行时自动更新。核查 README、文档索引和相关设计说明，未找到“全部模型可自动计价”的现存承诺；README 原“设计阶段”表述已在工作区同步修正，保留原用户未提交的其余内容；未跟踪索引增加实际存在的专题入口。

本次仅修正文档及核对源码 / 官方资料，不新增算法通过、真实费用账单或安装验收结果，不运行性能测试。缓存写入改动单独完成必要验证后另行提交。

任务依据：[实施计划](implementation-plan.md)。本文件区分已经实现、自动检查、真实 Windows 运行时检查及待验收项，不将原型效果或代码存在视为完整交付。

2026-10-03 用户已明确授权例外顺序：将 `LiuYongXinn/token-pulse` 改为公开，先推送已验证候选并发布资产，再进行固定发布源的真实线上升级验证。此授权替代此前“全部验证后才推送”的限制；环境受限验收仍不记为通过。

## M16x：修正版的正式 GitHub 在线升级闭环

从提交 `11ef9df2d6be4040fa5db24c8d06c97fcc863475` 完整构建 0.1.2，前端 typecheck / Vite、第三方声明生成、release 编译与 NSIS 打包通过，使用既有同一项目密钥签名。安装包 6,626,615 字节，SHA-256 `51aaf29a3bcc2ffd0f1367af086d4f3d9ac27b2c1ff826e3cc1556bff957c1e5`；签名摘要 `773a3be3b6af7e79cc84b7fa6290686e2aef176856e0c10ff980ee0e2daf20e8`。0.1.2 源码标签指向该构建提交，公开历史模式检查 2,393 个 blob 无命中；原未提交内容保留。Release 402493574 草稿上传三资产并核对 GitHub 摘要 / 大小后公开，匿名固定 latest 清单指向 0.1.2，匿名安装包重新下载及正式版本绑定验签通过。未覆盖 0.1.1 已发布资产。

Windows 10 19045 / 150% 的标准目录安装版 0.1.0（PID 93320）通过实际主 WebView 的设置 / 更新按钮，从固定生产 HTTPS 发布源检查、下载完整 6,626,615 字节、验签、安装前版本复核及确认。旧应用 UTC 12:05:39.084 正常退出 0；安装器自动启动新版 PID 86344（12:05:40.025），注册 DisplayVersion=0.1.2 / 原安装位置保持。实际安装主程序与该候选只存在精确 NSIS bundle marker 差异，安装文件摘要 `c84959f1dd96b75afa009a4458a7dd9073ab87e9b170173940fe1eb63558f071`；宿主与候选逐字节一致，摘要 `4efdfd2acb0fe9325a5631c57dfc313500461e7549e0e8bbed93a81939826f19`，第三方声明一致。新 WebView 暴露正式总览，更新页显示实际 0.1.2，再次检查显示“当前已是最新版本”。没有手动运行新安装包代替应用内更新，也没有更换发布源 / 公钥、跳过 TLS / 验签或重启电脑。NSIS 退出码未被捕获，保持 null，不推断为 0。

测试期间实际库已有 1 来源 / 202 会话元数据 / 175 文件代次 / 1,146 观察 / 34,913 诊断、37 规则及 1 目录，可信 usage_events 为 0。为比较真实稳定快照，通过正式来源按钮暂时暂停采集，升级后确认暂停状态仍保存，再恢复原启用状态。升级前后均只读同事务 quick_check=ok / schema 10；46 张非 app_state / settings 表逐表全部行摘要一致，data revision 629、price revision 1 保持，settings revision 55→56 为正常窗口持久化。随后正常恢复来源并保留全部现有记录。本轮证明这些既有数据的保留，不把零可信消费当成非零账本升级的实际验收。

首次 0.1.1 的失败安装器在正常关闭尝试后已消失，未执行强杀；旧文件精确匹配 0.1.0 基线后重新启动。失败安装器退出码未知，保留 null。0.1.1 失败与 0.1.2 成功分别记录。全部 UI 动作用自有进程 UI Automation InvokePattern / SelectionItemPattern，不需要输入桌面，但不替代鼠标 / 键盘或 Narrator 检查。证据为忽略目录 `target/release/review/v0.1.2-11ef9df/` 内匿名下载 / 实际验签、UI 状态、父退出、安装文件及数据库比较回执。

新发现的真实格式待办：来源中 INVALID_USAGE / UNSUPPORTED_FORMAT 占主导。只读定位两条 INVALID_USAGE 用量记录，只保留必要数字与字段结构，不保存 / 输出聊天正文或认证信息；确认 Token 向量含新增 `cache_write_input_tokens`，当前五字段白名单拒绝该字段。两条该字段均为 0，其他输入 / 缓存 / 输出 / 推理 / 总量关系可独立复核。需继续明确该扩展的核算 / 计价语义，用合成结构夹具修复及验证；没有用忽略未知正数或伪造消费的方式消除错误。此问题属于采集适配收尾，不能因自动更新通过而宣称完整应用交付完成。原物理输入 / 多屏 DPI、外部账户变化、缺失 WebView2 和真实 notify 条件仍独立待验。

## M16w：公开发布与已退出父进程的安装竞态

公开前检查将推送历史中的 2,383 个 blob；所检查的私钥、GitHub / OpenAI / AWS 令牌模式没有发现，当前跟踪文件没有 auth.json / 私钥 / 本地签名注册文件。原有 README 修改及五项未提交状态未进入推送。远端 main 推进至 `cc00dd2`，轻量标签 `v0.1.1` 指向已构建生产提交 `6714bb4`。仓库 ID 1399054634 经授权改为公开，草稿 release 402486864 三资产逐项比对 GitHub SHA-256 / 大小后于 UTC 11:44:41 公开。匿名固定 latest 清单 / 安装包下载成功，正式版本绑定验签通过；安装包摘要 `e147e78311c46804324f8355b5e7bf3b2da0bd5eb0e2e2c63401b495678fc84a`，没有更换密钥。

实际 Windows 10 已安装 0.1.0 / PID 92688 / 正式数据库，通过自有主窗口 UI Automation InvokePattern 进入设置及更新，SelectionItemPattern 选中软件更新；实际固定 HTTPS 发布源发现 0.1.1，下载 6,626,972 字节并显示“更新已验证，可以安装”。确认按钮启动正式安装器，旧应用 UTC 11:47:22 正常退出 0；NSIS PID 83296 却提示无法确认旧应用退出，注册版本仍 0.1.0。这轮完整升级判定失败，不能用下载 / 验签成功替代安装闭环；锁屏下程序化真实 UI 操作与物理鼠标 / 键盘验收分别记录。

根因是 NSIS System 插件两次调用间无法保留 OpenProcess 的 GetLastError。旧父进程对象已销毁时，后一次 GetLastError 没有读取该调用的 ERROR_INVALID_PARAMETER。改为同一次 System::Call 的 `?e` 立即捕获，并 Pop 到保存的寄存器；只接受 87，权限等其他错误仍拒绝。真实 NSIS 定向夹具覆盖父进程仍活着、已退出但握有句柄、已退出且句柄已释放三种时序。原钩子在第三种独立场景失败；修复后三种全部通过，夹具没有修改产品安装 / 注册表 / 数据 / 任务栏。正式版本提升至 0.1.2，后续从原 0.1.0 重新进行发布及升级，不覆盖已发布 0.1.1 资产。

发布与失败证据保存在忽略目录 `target/release/review/v0.1.1-cc00dd2/`。更新前 SQLite 只读同事务 quick_check=ok / schema 10 / 48 表，37 条离线规则 / 1 个目录，来源和用量事实为空；本轮尚不能证明有历史消费的真实升级保留。没有读取认证文件或原始日志，没有重启电脑或运行性能测试。完整线上升级仍待修正版包验证。

## M13g13：应用图标右侧的真实 S3 恢复链

扩展独占电源验收的 `--application-right`，仅允许与两个既有任务栏电源场景组合，其他 / 重复 / 混合参数拒绝。`native-smoke.ps1 -PowerTaskbarMessages -ApplicationRight` 只发自有消息；`verify-power-resume.ps1 -Taskbar -ApplicationRight` 默认仍仅预检，显式追加 `-ActualStandby` 才请求真实 S3。场景通过正式 WebView / settings IPC 保存 application_right，睡眠前后重新读取并证明 enabled / 无回退 / 位置不变；位置成功标记和驱动结果绑定所选位置，不能用左侧成功日志满足右侧门槛。生产 actor / 宿主源码和更新签名包没有改动。

Win10 19045.6466 / 单屏 / 150%：同一 M16v 候选宿主 SHA-256 `f5d07431def6a6a9a41680453a58ae5f5bdc96bdcb1bd299bb714fc43dc968de` 的右侧自有消息场景退出 0，宿主 97428→98880，3→10、正常退休与完整几何 Exact。随后真实 S3 的 `actual-s3-right/result.json` 为 Passed=true / TaskbarSleepAcceptance=true / position=application_right，debug 桌面 98896 退出 0、宿主 97108→99016；新快照与原生读数 / 详情 10、旧详情隐藏、主 / 小窗隐藏、原导航和设置修订保持，退出完整当前任务区 Exact。原 Explorer root / 内核对象保持，任务区右边界前后 2044；不凭 NoRecord 单独认定空间恢复。

系统 Kernel-Power 42 Record 53511 与 107 Record 53512、Power-Troubleshooter 1 Record 53518 均给出 state=4。驱动请求 UTC 11:20:45.2195071，返回 11:21:51.523470，约 66.304 秒为调用间隔，不等同精确睡眠时长；40 秒唤醒请求已按正常路径取消 / 关闭，策略与特权恢复成功，具体硬件唤醒来源仍未证明。证据目录 `C:\Users\Amin\AppData\Local\Temp\tokenpulse-taskbar-right-power-eee369e32cdc409aaa4e22e226169e74`，含预检、两场景日志、系统事件、二进制摘要及清理回执。桌面摘要 `1b6d4c4dcd95a2440d04f8f3c4a743bbeadd8f180277a6b49a0ced94fbaab937`；这是 debug 桌面 + 同包 release 宿主混合验收，不是正式安装桌面全包验收。

4 项定向电源 / 几何自动检查、桌面 strict all-target Clippy / fmt / release check 和两脚本 AST 通过。消息与真实系统证据严格分开，没有发送物理输入、重启电脑、重建签名包、安装升级或发布。自有验收进程全部退出，临时测试宿主已恢复原 debug 二进制并核对摘要；正式已安装应用和 Explorer PID / 启动时间保持。SHORTCUT_CONFLICT 和 WebView2 注销 1412 诊断保留，没有将其记作恢复键通过。两位置的任务栏 S3 链现均有独立真实证据，物理输入、多屏 / 各档 DPI 和外部账户 / 更新条件仍按当前范围分别记录。

S3 后再次复核同一正式 0.1.0 安装基线的标准托盘：只读入口退出 0，图标 HRESULT=1、有效矩形 `{2080,1380,2116,1440}`；物理入口在 InputReady 返回“Interactive session is locked, inactive or unknown”后退出 1，发送输入前拒绝，没有展开系统面板。两份日志保存在上述证据目录。这项仍未通过，不将后台 S3 成功合并为托盘真实点击；当前锁定不妨碍已经通过的后台恢复链，但不提供输入桌面解锁或菜单物理操作证据。

## 2026-10-03：关键页面截图评审

按用户要求生成并逐张查看 24 张 PNG，覆盖七页主导航、五类设置、会话详情 / 事件依据、账户与 notify 区域、模型别名、紧凑 / 展开小窗及额度详情、原生任务栏完整 / 精简读数与悬停详情首屏。[本地截图画廊](../../target/page-review/2026-10-03-a32d1c6/index.html)和[图像来源 / SHA-256 清单](../../target/page-review/2026-10-03-a32d1c6/manifest.json)保存在忽略的构建产物目录，未发布到远端。

源码基准 `a32d1c6` / 0.1.1。主窗口与小窗是正式 React 组件在隔离浏览器测试桥中的渲染截图，使用现有合成 DTO，不是已安装应用真实数据的桌面实拍；模型 / 项目 / 会话 / 明细中的超大数值是精度边界夹具，账户百分比和费用示例均不表示用户实际消费。离线价格来自已打包的 `openai-text-2026-10-02` 事实目录，本轮截图没有重新联网核价；更新页 0.1.0 为测试 DTO。任务栏三图由正式宿主使用的同一 Windows 字体 / GDI 渲染器生成合成画面，不包含 Explorer 背景，详情首屏保留原生滚动区域。

截图交互场景 13 项通过；定向原生像素检查 1 项通过。逐图检查布局、完整页与区域截图、未知值 / 零额度区分及两种小窗尺寸。补齐相邻模块的测试 DTO 后重新截图，避免缺失桥接命令造成的无关故障提示；没有改动生产界面代码、正式数据库、原始日志、真实账户、安装状态或签名候选，也未重启电脑。本轮是视觉评审材料，不将截图生成等同全部交付验收完成。

此前环境复核（本机安装限制已由下方 M16l 的新授权与实际验收更新）：当前 Windows 为 10.0.19045，一个活动显示器，正式数据目录仍存在。未发现可调用的 Get-VM / vmrun / VBoxManage、vmms 服务、Docker 命令或 WindowsSandbox.exe；这不证明没有其他可用设备。真实 wire 再次在自有窗口被全屏 Windows.UI.Core.CoreWindow 覆盖时、发送输入及启动宿主之前拒绝，未新增交互通过记录。物理系统矩阵仍按实际环境继续，不用窄夹具替代最终验收。

## M13g12：任务栏宿主的真实 S3 恢复链

扩展独占 debug 电源场景 `-PowerTaskbarMessages` / `-PowerTaskbarResume`，及 `verify-power-resume.ps1 -ActualStandby -Taskbar`。仍使用 UUID SQLite / 合成只读 Home，只有新增任务栏场景才启用正式 actor 与独立宿主；默认驱动只是预检。通过实际 main WebView IPC 选中唯一已导入会话 / 固定起点、关闭回退、开启通知区左侧显示，并核对 mini DTO 与原生 caption / 详情的 3 Token；先完成窗口 / 范围准备，再暂停采集与追加待处理 7。等待电源事件时若提前采集就失败，恢复后同时要求正式补扫 / 覆盖、10 Token 原生读数、恢复后新快照、原设置修订及无错误。没有演示价格或账户夹具。

睡眠前明确以自有消息打开原生详情，恢复后要求其隐藏，再以自有焦点消息核对新详情的 10；这些消息不是物理鼠标 / 键盘验收。主 / 小窗均保持隐藏，导航修订不变。握住旧宿主内核对象，允许正式 suspend 关闭后重启宿主，若替换则要求旧进程退出；不要求睡眠跨代次保持原宿主。退出正常 shutdown，再要求新宿主退出与完整当前任务区宽度，不能仅凭 guardian 的 NoRecord 算空间已恢复。

首次消息准备在 POWER_BEFORE_USAGE_MISMATCH 失败，未进入电源阶段；日志与自有数据库保留，首次原因尚未确定。添加合成 DTO 错误信息、将准备移至追加前，后续消息场景通过，早于电源事件的计入也有独立拒绝断言。前两次含同包 release 宿主的实际 S3 均在旧几何逐字段相等断言失败，保持 result.Passed=false；第二次诊断捕获仅共享边界 2044→2080：根 / build / DPI / 左侧与纵向 / 通知区外侧未变，task switch / list 均已经填满新的 ReBar 边界。这是测试将睡眠前通知区坐标固定后的误判，不据此修改生产写入或重建签名包。

补足验收判据：同时保有原 Explorer 内核对象和 root HWND / PID、重新严格读取完整当前 Win10 拓扑并等候稳定；只能接受 Exact 或 CurrentNotificationBoundary。后者要求两帧有效、所有任务区与列表都为完整宽度、共享边界对齐，且上述其他字段逐项相等。没有移动通知区或回写旧坐标。两项独立字面预期检查覆盖捕获的 2044↔2080 与原帧；拒绝 10 类未归还空间 / 缩短列表、DPI / 根 / 左侧 / 纵向 / 通知区外侧 / 共用边界 / OS 版本变异，另拒绝自相等但缩短的列表。连同独占模式与计数检查共 4 项自动检查通过；严格 desktop all-targets Clippy、fmt、release 编译、两个脚本 AST 通过。

最终实际使用 M16v 候选中的同一 release 宿主，SHA-256 `f5d07431def6a6a9a41680453a58ae5f5bdc96bdcb1bd299bb714fc43dc968de`；桌面为包含验收入口的 debug，SHA-256 `2dbc25014f70c30fe66067067629584d35e886a9eb29cacbf410109f3fc5b171`，明确为混合验收而非正式安装包整体。最终同包消息场景退出 0（96800→78684），完整 Exact 几何 / 3→10 / 详情 / 无意外动作与源保持通过。随后真实 S3 场景退出 0，观察桌面 96988，旧宿主 87288 正常退出、新宿主 73328 恢复，TaskbarSleepAcceptance=true（通知区左侧）；新 Kernel-Power 42 / 107（53502 / 53503）及 Power-Troubleshooter 1（53509）均记录 S3，主 power 计数 suspend=1 / automatic=1 / user=0、源保持 / 覆盖与原生读数通过。最终实际走 Exact（共享边界前后 2080），CurrentNotificationBoundary 的新判据本次没有在修正后的真实 S3 中命中，不混记两轮历史失败。

驱动请求 UTC 10:43:10.9196594、唤醒期限 10:43:50.8531288、返回 10:44:17.6863088，约 66.77 秒；不推定精确睡眠时间或具体定时唤醒来源。临时权限恢复读回、timer / token 取消关闭均通过，没有解锁 / 物理输入、电脑重启 / 关机或休眠请求。原安装 PID 92688 与 Explorer 87404 / 创建时间保持，退出后只读完整拓扑及开发宿主原文件摘要复核。SHORTCUT_CONFLICT、WebView2 注销 1412 保留。

所有失败、诊断、成功、编译与混合宿主备份 / 还原收据在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-taskbar-power-f4cfdccea37e4137a184f81e48a191ff/`，最终结果为 `system-s3-current-boundary-final/result.json`；开发目录宿主已经还原原摘要 `4715ad326d22d8c1143641aac26dcecb001521df88f47b03e1e975665906e67a`。本轮只有 debug 验收 / 独立驱动变化，正式运行逻辑及 M16v 签名候选不变。无需重复已通过场景或重签名，应用图标右侧的实际 S3、安装版物理托盘 / 键盘、真实账户 / notify、物理多屏 / DPI、WebView2 缺失与完整在线升级仍单列；未推送 GitHub、安装升级或运行性能测试。

## M07r2：真实 S3 睡眠与恢复补扫

新增 `scripts/verify-power-resume.ps1` / `standby-resume-driver.cs`。默认只完成电源 / 唤醒定时器、当前进程已有权限与 System 日志可读性预检，不启动观察应用或睡眠。只有显式 `-ActualStandby` 才进入真实验收：拒绝其他 debug 实例，启动自有独占 SystemObserver，匹配完整 READY 行、PID / 内核创建时间 / 路径 / 同会话；再次核对 AC / S3 / 唤醒策略，持有绝对 UTC 40 秒唤醒定时器，暂时启用本进程已有 SeShutdownPrivilege 并读回。先保存 intent，再以 `SetSuspendState(false, false, false)` 请求待机，保留唤醒事件；没有重启 / 关机 API、休眠请求、提权、策略写入或解锁 / 物理输入。返回后恢复原权限并读回、取消 / 关闭 timer 和 token；结果与各阶段分别留证。[Microsoft SetSuspendState](https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-setsuspendstate)、[Microsoft AdjustTokenPrivileges](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-adjusttokenprivileges)

Win10 19045.6466 / 现有会话 / 150% 无需解锁操作实际运行 `-ActualStandby` 退出 0。System 记录基线之后新增 Kernel-Power 42（RecordId=53475）与 107（53476），TargetState / EffectiveState 均为 4（S3）；观察程序 PID 7696 收到真实 suspend=1 / automatic_resume=1 / user_resume=0，NATIVE_POWER_RESUME_OBSERVER_OK、退出 0。隔离 SQLite 的待处理 7 Token 经正式电源路由恢复补扫，总量 3→10、完整覆盖 / 空队列、源字节 / 只读属性及隐藏主窗通过。没有作者发送的模拟电源消息；系统记录与应用断言分别成立。[Microsoft SYSTEM_POWER_STATE](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-system_power_state)

驱动请求 UTC 09:59:39.2514943、唤醒定时期限 10:00:19.2306191、返回 10:00:45.6192858，请求至返回约 66.37 秒；这不是精确睡眠时长，也不能凭期限推定由此 timer 唤醒。选定日志没有本轮 Power-Troubleshooter 唤醒来源记录，HardwareWakeSourceProven=false。TimerArmed / Cancelled / Closed=true，原权限禁用→请求时启用→原状态读回恢复=true、TokenClosed=true。ObserverStillRunning=false 是应用已经按正常成功路径退出，并非被终止。原安装 0.1.0 PID 92688 / Explorer PID 87404 与创建时间恢复后保持，没有重启电脑。

实际证据在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-system-power-actual-e76639827e3e4a0ab18fbe36b1762242/`，包含 preflight / intent / driver / 选定 System 字段 / stdout / stderr / result；观察程序 SHA-256 为 `5587a0ddb98b53d56f6dcd33126ef03ee043b32ff2c556bd83a372f6986e5f2e`。不保存完整系统事件消息。SHORTCUT_CONFLICT 与 WebView2 注销 1412 如实保留。默认预检、C# 编译、脚本 AST，以及无效身份 / 最终预检拒绝时不建立 timer、不启用权限且不睡眠的检查通过；随后补强权限读回失败路径的清理，默认与拒绝检查再次通过，未为该失败路径改动重复睡眠。只读任务栏拓扑复核仍为原完整几何。

此轮真实 S3 已覆盖采集恢复通路，**不是正式安装包全应用验收**：任务栏原生宿主睡眠链、额度服务真实账户状态、安装版托盘 / 键盘、多屏与其他外部项目仍分开记录。没有推送 GitHub、安装升级或重新签名；M16v 候选与原用户未提交内容保持。下方 M07r1 的“待真实睡眠”是其历史时点，由本节补充更新。

## M07r1：隔离恢复补扫入口与电源前置检查

上一轮 M13g10 / M13g11 与 M16v 已实际修复并验证任务栏，属于进展；当前正式候选及原用户未提交内容保持。本轮继续真实睡眠 / 恢复待验项，新增 debug-only 独占 `scripts/native-smoke.ps1 -PowerMessages` / `-PowerResume`。两个模式都使用新 UUID native-probe SQLite、合成只读 Home / JSONL、真实主窗口 HWND 与正式 power subclass / CollectorService；不读取原日志、认证或真实账户，不启动额度夹具 / 本轮任务栏宿主，也不进入安装器。

watcher 禁用、活动 / 目录轮询为一小时。先扫描独立预期 3 Token、完整 EOF / 空队列；直接暂停自有夹具采集器并等待 suspended 后追加 7，再恢复只读属性，确认恢复前仍为 3。直接暂停是准备动作，不冒充收到系统 suspend；这样避开每秒 proof rebuild 等其他补扫触发。主 HWND 的实际电源 subclass 对本场景管理的原子计数器记录 suspend / automatic resume / user resume，再沿用正式非阻塞服务标记与 reconcile。恢复后要求总量 10 / 完整覆盖 / 空队列、字节与只读属性不变且主窗口仍隐藏；消息模式再重复两个恢复消息，仍为 10。

`-PowerMessages` 仅向自有主窗口 SendMessageTimeout（2 秒）发送明确标记的电源消息，不广播、暂停电脑或发送物理输入。本机 Win10 19045.6466 / 150% 退出 0，NATIVE_POWER_MESSAGES_OK，suspend=1 / automatic_resume=2 / user_resume=1、3→10 和源保持通过。`-PowerResume` 是独立系统观察模式，不发送这些消息，也不调用睡眠；等待最多 150 秒内的新 suspend + resume，完成同样补扫断言。观察模式的消息计数无法单独鉴别发送者，必须配合外部真实系统驱动和独立 OS 电源证据，不能仅凭 OBSERVER_OK 宣称实际 S3 通过；此成功分支仍待真实验收。

两项自动检查通过：精确模式 / 混合、未知、重复与缺 gate 拒绝，以及未知电源事件不推进三类计数、已知事件分别累计。严格 desktop all-targets Clippy（custom-protocol）、fmt、release 配置编译及 PowerShell AST 通过；混合 `-PowerMessages -TaskbarActions` 在编译 / 开窗前退出 1，未运行混合场景。首次 Clippy 指出 set_readonly(false) 的 Unix 风险，已改为恢复新建夹具的原权限，保留失败日志，不降低严格规则。SHORTCUT_CONFLICT（原安装同时运行）及 WebView2 1412 如实保留。

新增 `scripts/inspect-power-resume.ps1` / `power-resume-probe.cs`，默认只读：System32 API 读取 Win10 x64 S3 能力、是否允许待机、当前电源来源 / 方案和对应唤醒策略。未知字段保持 null；未检查定时器时 Eligible / Timer 字段为 null。`-CheckWakeTimer` 只建立自有、无名、非继承、绝对 UTC 40 秒唤醒定时器，随即取消并关闭；没有睡眠 / 休眠 / 重启 API、权限调整或电源策略写入。采用绝对时间是因为 Windows 8+ 的相对定时器不计低功耗时间；定时器调用成功但返回 ERROR_NOT_SUPPORTED 仍判不可用。[Microsoft SYSTEM_POWER_CAPABILITIES](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-system_power_capabilities)、[Microsoft SetWaitableTimer](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-setwaitabletimer)

本机原生预检：S3=true / StandbyAllowed=true、AC=1、平衡方案、WakePolicyIndex=1；S4 固件支持但没有休眠文件，与 powercfg /a 的“休眠未启用”区分。唤醒请求受支持、TimerArmed / Cancelled / Closed=true，前置条件满足，**不表示已经实际睡眠或验证硬件真实唤醒**。默认与显式两分支均完成；所有日志 / JSON 在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-power-acceptance-30512a5f361b4678ab291b8b26f048bb/`。powercfg /waketimers 因非提升权限拒绝，不改为提升或冒充其已查询成功，受控自有定时器结果另记。

下一步是建立并验证有期限的真实 S3 驱动与独立系统证据，区分暂停本机程序与电脑重启；真实系统操作会暂停整机程序，需要在准备完成后明确说明。当前没有执行睡眠、休眠、电脑重启 / 关机或性能测试，也没有推送 GitHub。M16v 正式运行逻辑没有变化，不因仅 debug 验收 / 独立预检重建签名包；真实睡眠、安装版物理托盘 / 键盘、多屏 / DPI 和其他外部待验项没有改记通过。

## M16v：纳入通知区与系统可见性修复的签名候选

从提交 `6714bb4adafd7d12a71e36ca092970a715e9e54c` 完整执行 `npm run tauri:build`，退出 0：TS / Vite、334 项声明、正式 release 桌面 / 独立宿主、唯一 NSIS 安装器成功；构建期间没有修改生产源码，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-final-release-4be416448c804234a0c99d7da3c55e52/`。同次三文件及 source / hashes 收据在 `target/release/candidates/v0.1.1-hidden-a29a71055aba44faa365e53ea632fd39/`，包含 M13g10 / M13g11 及此前任务栏修复。M16u 的失败包、M16t 与更早候选及日志保留。

使用原同一密钥签名，不生成新密钥；实际 production release 验证器与本地资产准备均退出 0。资产在 `target/release/publish/v0.1.1-hidden-a29a71055aba44faa365e53ea632fd39/`：候选清单时间 `2026-10-03T09:08:31.509Z` 为本地准备日期，不表示发布。固定 v0.1.1 GitHub URL / TLS / 签名规则保持，没有推送、release 创建或上传。

- 安装器：6,626,972 字节，SHA-256 `e147e78311c46804324f8355b5e7bf3b2da0bd5eb0e2e2c63401b495678fc84a`。
- 签名：440 字节，SHA-256 `cd5c335228a941d56a3f56a3d2778dd2a7807523dfb396b22a7350b00a16a713`。
- 桌面：25,533,952 字节，SHA-256 `e66ec978966c95b05684232093b7ac04c54c471072aa7f8c3c3ef2b884c978bd`。
- 宿主：2,358,784 字节，SHA-256 `f5d07431def6a6a9a41680453a58ae5f5bdc96bdcb1bd299bb714fc43dc968de`。
- 项目公钥摘要保持 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`；安装和更新不要求用户输入密钥。

复制 release 验收例程到新候选 examples，核对 host 摘要后按例程默认同目录路径运行；例程不进入安装器。实际 Win10 19045.6466 / 150% 三项均退出 0，日志与 process-result.json 在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-final-native-f59ae236207a442fa7af699d8450c7ac/`。

1. 通知区左侧，同宿主 26780，真实自动隐藏 0→1→0 / top 1380→1438 / bottom 1440→1498；新 Snapshot 与隐藏期间自有设置刷新后 Embedded / failure=None，system_revision=1、last_restore=None，证明此受控刷新没有释放租约；恢复显示 / 空动作 / 设置 / 原几何通过。
2. 应用图标右侧，同宿主 94796，同样的真实隐藏 / 数据刷新 / 自有设置刷新 / 原设置与几何通过，system_revision=1、last_restore=None。设置消息只向自有 Control 窗口发送，不计真实广播或物理输入验收。
3. 正常 RM Explorer 94268→87404，Shutdown / Restart 状态 0、旧内核对象确认结束、ForceShutdown=false / ComputerRestart=false；同宿主 83372 恢复、新画布 / 修订推进 / 无旧动作 / 新 Shell 几何通过。

M13g11 的完整 Tauri 混合 Explorer 场景及完整动作回归均退出 0，使用宿主字节摘要与本候选完全相同；69 项 taskbar、严格 Clippy / fmt、独立祖先可见性 Win32 测试及原始失败日志保留，不为无新代码重复无关检查。混合桌面是 debug + 实际优化宿主 / 真实 WebView / 隔离 SQLite，仍不是正式安装桌面的物理端到端验收。

已安装 0.1.0 / PID 92688 保持。原数据库只读事务复核 schema=10 / quick_check=ok / 48 表、sources / sessions / usage_events=0；不读取源日志或认证。已安装托盘在新 Shell 下只读返回 S_FALSE(1) / `(2080,1380,2116,1440)`，物理场景在 InputReady 因会话锁定拒绝，退出 1，没有发送输入；证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-final-installed-tray-94e73a13db774a00b34521972aec6679/`。最终 ReBar / Switch / List right 均为 2080，仅 Explorer 87404 / 原安装保留，自有探针及宿主已结束。

M16v 更新“当前签名候选”的含义；包未安装升级或发布，不能宣称完整在线自动升级通过。标准托盘物理菜单、物理键盘 / Narrator、鼠标触边 / 拥挤 / 非底部任务栏、多屏 / 实际系统 DPI、实际休眠、账户外部状态、真实 Codex 回合、缺失 WebView2 和完整在线升级继续独立待验。Windows 11 与 M14 等取消范围不恢复；无电脑重启 / 关机、性能测试、密钥重新生成或 GitHub 推送，原未提交内容保持。

## M13g11：系统自动隐藏与读数自身可见性的区分

从 M13g10 提交 `49717c546fa9bf261488cf402f594b7903421af9` 完整构建并签名的中间候选 M16u 位于 `target/release/candidates/v0.1.1-notification-fcfc8cf4b4df4c73b50a6a6099d87bd9/`，本地资产位于同名 publish 目录。安装器 6,625,718 字节 / SHA-256 `b720799ebd0a48ef61b2c7dcd768b937c45c15d10afd76869ec8fdd75145e462`，实际 production release 验签退出 0；桌面摘要 `83c9049da2df5e9629224a0ba4b3cdfdb2a0e3c6faf68f8c65311679d067f1f1`、宿主摘要 `947c1139919a4e6cc6196e54b7dacd39862e8dea97da8af843b0eaebd4c3b04c`。构建证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-notification-release-0d51f88dd6d743c2ac067ec3b1509e48/`，使用原密钥、没有发布。**此包的自动隐藏验收存在失败，不作为最新验收完成的包，也没有安装升级。**

真实 release 失败在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-notification-release-native-cfdb4826d3da4f7cbba0f5ee54850c26/` 和 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-notification-release-followup-987973c1058244f6a94a163831c4209d/`记录保留；后者捕获应用右侧 Unavailable / UnexpectedStructure、system_revision=1 / last_restore=Restored。两位置各一次成功复测不抵消失败。新增仅显式例程的失败前状态、最多五次普通新 Snapshot 诊断、只读根 / 自有读数样式与几何，严格首次断言保持；`C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-visibility-e5af7007d2704a2fa1ced393c6347bda/` 在同一中间候选宿主 95028 捕获实际根 visible=0 / WS_VISIBLE=false，而自有读数 visible=0 / WS_VISIBLE=true、父窗口仍为根、位置 `(984,1438,1585,1498)` 正确，返回 Unavailable / Os。五次后续新快照恢复 Embedded，但原场景仍退出 101，未被改记通过。Windows 的 IsWindowVisible 检查全部祖先样式，因此这不是读数窗口自身被隐藏的证据。[Microsoft IsWindowVisible](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible)

修复严格区分两个状态：只有系统自动隐藏已开启、整个底部任务栏移出对应显示器（最多两像素边缘）、同一已校验租约与自有读数 WS_VISIBLE 保持时，Embedded 可与 effective readout_visible=false 同时成立。实际 readout_visible 字段仍保留真实值；自身显式隐藏不接受。在同一严格系统隐藏证明下，拓扑 / safe_slot 枚举使用子控件自身可见样式，避免根隐藏使整个结构“消失”；普通可见根继续原可见性过滤，隐藏且未满足系统证明的根拒绝，未知类 / 重复结构 / owner / DPI / 几何门禁不变。隐藏期间的系统刷新仅在租约仍严格有效时保留，先清交互、重新准备主题与数据；其他系统变化仍脱离并重查。共用隐藏判定从 layout 移到 topology，已有独立坐标拒绝测试保留。

新增实际 Win32 自有父 / 子窗口测试，分别验证初始隐藏、只显示子窗口而父隐藏、父子显示、再显式隐藏读数的可见性差异；完整 taskbar all-targets **69 项通过、1 个私有入口 ignored**，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-root-fix-d5ceee97b0cf43278387891815dc91fb/`。desktop / taskbar all-targets（含 custom-protocol）严格 Clippy 与 fmt 通过。

显式自动隐藏例程增加 `--own-setting-refresh`：仍必须提供原开发 gate，仅唯一枚举同宿主 PID / Control UUID 类，再核对归属，用 2 秒 SendMessageTimeout 向**自有窗口**发送零载荷 WM_SETTINGCHANGE；不是广播、Explorer 消息、系统真实广播或物理输入。非法 / 重复选项拒绝，例程不进入安装器。最初 PostMessage 返回 0 的例程失败日志在上述 fix 目录保留，不记为生产宿主失败；该类系统消息受指针参数规则约束，现使用有期限同步调用。[Microsoft PostMessage](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew)、[Microsoft SendMessageTimeout](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendmessagetimeoutw)

修复后的 release 宿主在真实 Win10 19045.6466 / 单屏 150% 两位置退出 0，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-root-message-fix-a5606bc0787d4bbfa5ac947452e7195a/`：通知区左侧同宿主 95416、应用右侧同宿主 89052，真实开关 0→1→0 / top 1380→1438 / bottom 1440→1498，新 Snapshot、隐藏期间自有设置刷新后 system_revision=2 / Embedded / failure=None、恢复显示 / 空动作 / 设置与原几何均通过。包含真实系统开关与自有消息两类证据，不能合并为物理菜单或真实系统广播全覆盖。

`C:/Users/Amin/AppData/Local/Temp/tokenpulse-hidden-root-system-d0cfe4a2f4cb4ef19bc74ef95ef6a256/`：正式 release 宿主正常 RM Explorer 11180→92524 / 同宿主 83192，退出 0；新画布 / 修订 / 无旧动作 / 退出几何通过。完整应用混合验收使用当前 debug 桌面及同一优化 release 宿主（分别记录摘要），隔离 SQLite / 真实 WebView / 正式 actor / IPC，在真实正常 RM 92524→94268 / 同宿主 92392 恢复后完成隐私、禁用 / 再启用 82960 及最终新 Shell 几何，整个应用退出 0；不是已安装 release 桌面的端到端验收。同一混合完整 `-TaskbarActions` 回归退出 0，两位置 / 按钮变化 / 原生详情 / 菜单 / Tauri 主小窗 / 隐私 / 退出几何通过。SHORTCUT_CONFLICT、托盘移除提示及 WebView2 1412 如实保留。

本次正式代码再次变化，需从新提交完整重建签名候选并核对同包宿主；M16u / M16t 保留为各自时点证据，不作为当前完整通过证明。电脑没有重启或关机，没有强杀 Explorer、推送 GitHub、修改原安装 0.1.0 / 数据或生成新密钥，没有性能测试；原用户未提交内容保持。标准托盘 / 物理键盘 / 鼠标触边、多屏 / 系统 DPI、实际休眠及外部账户 / Codex / 缺失 WebView2 / 完整在线升级继续独立待验。

## M13g10：通知区边界变化后的有条件预留释放

取得 M13g9 失败时的完整差异，根因已确认：`C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-layout-difference-dec8cc6cea474973b195d84f945af28b/` 退出 1，真实 Shell 11144→16460 / 同宿主 86936。当前任务区精确等于本实例 expected `(2,0,1289,60)`，但 Explorer 加载通知图标后 ReBar 客户端宽度从 1722 变为 1578；根窗口、左边界、垂直几何与 144 DPI 保持。旧恢复拒绝父尺寸变化并移除记录，留下预留宽度，后续完整基线检查返回 UnsafeGeometry。此失败日志和 M13g9 早期失败均保留。

仅正常存活租约新增窄恢复路径：同一已校验内核 Shell / 窗口类 / 父子关系 / owner、Reserved 阶段、当前任务区精确等于 expected、原完整拓扑有效，且只改变 ReBar 与通知区共用边界时，释放任务区到**当前**客户端宽度；不写回旧通知区或 ReBar。写入前要求同一完整拓扑帧和 safe_slot，写入后独立确认目标、父尺寸、根 / 通知区 / DPI 及完整右边界；帧再次变化返回 Uncertain，写入或核对失败返回 Failed，不虚报恢复。其他外部布局、DPI、根位置、归属和未知结构仍拒绝。只有终止元数据的 guardian 不拥有原拓扑证明，继续严格要求旧父尺寸；17 字协议不变。规则同步到[宿主协议](../design/taskbar-host-protocol.md#m13c2线程所有的布局租约)。

新增三项独立字面坐标夹具：实际缩小、反向增大及 19 个拒绝分支；完整 taskbar all-targets 68 项通过、1 个私有入口 ignored。desktop / taskbar all-targets 严格 Clippy（含 custom-protocol）与 fmt 通过。`TOKENPULSE_ACCEPTANCE_HOST_DIAGNOSTICS=1` 仅 debug 显式启用有限恢复差异 / 槽位拒绝 / 成功分支日志；release 移除这些输出，不包含账户、正文或认证数据。

实际 Win10 19045.6466 / 单屏 150% 的完整应用再次捕获**同一失败形态**并成功释放：`C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-notification-fix-eb17f12ebd8b4fe0ac61998eb88fbcfa/`，正常 RM Shell 94584→11180 / 同宿主 90836；父宽 1722→1578，current=expected，实际 `NATIVE_TASKBAR_NOTIFICATION_RESIZE_RESTORED` 与最终 `NATIVE_TASKBAR_EXPLORER_APP_OK`，退出 0。新快照 / 新窗口代次、主 WebView 状态、隐私清屏、无意外动作、禁用原宿主退出、再启用宿主 58060 及新 Shell 最终几何通过。此前失败后的正常 RM 基线恢复 16460→94584 也单独记录；没有猜测写入宽度、强杀 Explorer 或重启电脑。

完整 `-TaskbarActions` 回归退出 0，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-notification-fix-actions-dc407eb986da496a8fb9e7aefa6ddaf5/`：两位置 / 按钮变化 / 原生详情与菜单 / 白名单动作 / Tauri 主小窗 / 隐私 / 退出几何通过。场景使用自有窗口消息及真实 WebView / 隔离 SQLite，不替代安装版物理输入；SHORTCUT_CONFLICT（原安装同时运行）与 WebView2 21412 如实保留。结束只读 ReBar / 任务区 / 列表 right 均为 2080，仅原安装 0.1.0 / Explorer 留存。

M13g9 下方“未收敛”作为该时点历史保留，当前已完成上述根因修复和匹配场景验证；生产代码已变化，M16t 旧签名候选仍保留但不含此修复，下一步从本提交重新完整构建、签名并验证同包 release 宿主。标准托盘物理菜单、键盘 / Narrator、鼠标触边 / 拥挤 / 非底部、多屏 / 系统 DPI、实际休眠、账户外部状态、真实 Codex 回合、WebView2 缺失和完整在线升级继续独立待验。未推送 GitHub / 发布资产，未改原安装、密钥或用户原未提交内容，未运行性能测试。

## M13g9：完整 Tauri 应用的真实 Explorer 恢复入口与未收敛竞态

M16t 已验证同包 release 独立宿主，但原完整 Tauri 场景仅关闭自有读数窗口，没有真实重启 Explorer。本轮新增 debug-only `taskbar_explorer_smoke` 与 `scripts/native-smoke.ps1 -TaskbarExplorerRestart`：只能独占选择此场景；普通测试 / 正式程序不会执行，入口不进入安装器。复用既有只读资格检查及正常 Restart Manager 驱动，没有制造 TaskbarCreated、重启宿主来替代恢复、发送物理输入或重启电脑。

实际应用使用新的 UUID native-probe 空数据库、真实主 / 小窗 WebView、生产偏好 IPC / SQLite / TaskbarService / 管道 / 同目录原生宿主。先通过主 WebView 启用任务栏、打开自有详情，再正常重启唯一登记的 Explorer；持有原宿主的内核进程句柄，确认旧进程对象仍存活及管理器 PID 保持，防止 PID 复用伪装为同宿主。恢复必须同时观察新读数 / 详情 UUID 类、后台 Embedded / 精确偏好修订、驱动返回后真正发布的新快照；不能仅用旧缓存状态通过。主 WebView 读回正式状态，恢复后通过正式隐私 IPC 清屏、验证详情没有费用 / 账户私有字段；主 / 小窗不得收到意外动作。最后正常禁用等待原宿主内核进程退出、重新启用并关闭服务，对比独立采样的新 Shell 几何。

早期失败分别记录，没有删除或改记通过：`C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-whole-app-fc67a70a877a4202b7886057c5a4cea6/` 在旧缓存 Embedded / 新窗口尚未就绪时立即枚举失败；`.../tokenpulse-explorer-whole-app-0c4aeb22eb5448469a60558d3ad233d8/` 实际恢复期限后仍为 UnsafeGeometry，退出后只读几何发现任务区 right=1827 而 ReBar right=2080，随后用正常 RM 恢复 Shell 基线，没有猜测写入宽度。`.../tokenpulse-explorer-whole-app-diagnostic-ba33e31139f44d409acfa0a6d1f8e52a/` 在资格阶段因未知可见 Explorer 窗口拒绝；之后只读复核资格恢复，没有绕过白名单或关闭该窗口。

验收本身补强了两个观察边界：正常原生清理已移除记录时，guardian 可以返回 NoRecord；仅在 Disabled、原宿主已退出且独立检查为完整有效任务栏几何时接受此结果，不把 NoRecord 叫作 Restored。MSTaskListWClass 的异步布局必须取得同一 Shell 下连续 750 ms 稳定的独立基线及退出结果，再作完整几何比较；10 / 15 秒均为功能期限，没有运行性能测试。旧“必须 guardian=Restored”和立即比较子列表的失败分别在 d53d333fa3dd422e844776e60e225bdf / 2198380e4a4240d894678568ac78b4db 记录保留。

完整正向场景在 Win10 19045.6466 / 单屏 150% 三轮退出 0：带有限诊断的 d54028efdedb4e05bee08ee81281b1e5 / da9601fc753c44faa51dc816cb112f6e，及移除临时几何诊断、增加内核句柄校验后的最终证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-whole-app-final-e2f9e93039b64c3fb4c6365feb1ac541/`。最终 Shell 86296→34408，同宿主 94560 跨 Shell 恢复；禁用确认原宿主退出，重新启用宿主 54648；NATIVE_TASKBAR_EXPLORER_APP_OK / 整个应用退出 0，新快照、WebView 状态、恢复后隐私、没有意外动作和新 Shell 稳定几何比较均通过。恢复主体没有宿主重启，禁用 / 再启用按生产设计正常创建新宿主。

**仍有未收敛的真实布局竞态**：`C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-whole-app-diagnostic-ecd22ace07a243569d6137313de71cf4/` 在恢复后的隐私变更期间返回 ExternalChange，随后 UnsafeGeometry / 退出 1；该次失败不能由上述三轮成功抵消。有限诊断的成功场景还观察到新 Shell 通知区边界从 2311 变化为 2080、ReBar 宽度 1809→1578，证明启动后布局仍会继续变化，但尚未取得该 ExternalChange 时完整记录差异，因此不将这个观察当作已确认根因。临时 controller / layout 几何输出已移除，生产布局保护和正式代码未改；下一步需要定位并安全处理这个竞态，不能强制恢复旧几何或提前宣称整体验收结束。

严格 desktop all-targets Clippy / fmt、PowerShell AST / diff 通过；混合 `-TaskbarExplorerRestart -TaskbarActions` 在编译 / 输入 / 变更前拒绝，退出 1，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-scene-guard-9e8bad223eed4068b0e7ea9dfb30b8a0/`。SHORTCUT_CONFLICT（原正式应用同时运行）、托盘移除提示及 WebView2 1412 如实保留，本场景不证明标准托盘或物理快捷键。结束时只读任务区 / 列表 / ReBar 的 right 均为 2080，任务栏有效；仅 Explorer 34408 与原已安装应用 92688 保留，自有宿主已退出。原未提交内容、已安装 0.1.0、M16t 候选与密钥保持，不重打包仅有 debug 验收变化的生产程序；未推送 GitHub / 发布资产，无电脑重启或性能测试。

## M16t：包含全部任务栏修复的签名候选与三项 release 原生复测

从已提交源码 `a523c50f4ef5ec915bd6df5236a4f4efabfc5357` 完整执行 `npm run tauri:build`，退出 0：TypeScript / Vite、334 项第三方声明、正式 release 桌面 / 独立宿主及唯一 NSIS 安装器均成功。构建期间没有修改生产源码；证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-receipt-final-release-6649d039faa942b283a88e01af97f12f/`。同次安装器 / 桌面 / 宿主复制到新的 `target/release/candidates/v0.1.1-receipt-9ce477bd01ed46b4af621bfb828e0387/`，源码和文件摘要写入 build-receipt.json。旧候选与失败日志保留，默认 bundle 路径旁的旧签名没有混入新包。

使用同一已确认密钥签名，不生成第二套密钥；production release 验证器及发布准备入口均退出 0，新的本地资产目录为 `target/release/publish/v0.1.1-receipt-9ce477bd01ed46b4af621bfb828e0387/`。固定 GitHub v0.1.1 URL 保持；本地候选日期为 2026-10-03T07:32:29.326Z，不表示已发布。安装和更新无需用户输入密钥。

- 安装器：6,630,563 字节，SHA-256 `0de21c00e573b1df7826ad18b3c66587aac0d524e7b5937b05646e1262cd3878`。
- 签名：440 字节，SHA-256 `0d2711c87b00f4c25e27d80ca203a98a0f4f8be636cd56f568ffd57632775093`。
- 桌面：25,533,952 字节，SHA-256 `370be1582a9b4184109f6ad0c6ea5ac7f273fca662fd26edc079e063d3449e57`。
- 宿主：2,355,712 字节，SHA-256 `b538fe461a45012e025fe8ddb532417cb4cc648c6d2dced4850721616db12cc2`。
- 公钥摘要保持 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`。

重新构建 release 验收探针，放入候选目录 examples，核对宿主 SHA 后按默认同目录寻址运行。真实 Win10 19045.6466 / 单屏 150% 使用合成展示 DTO，三项依次退出 0；探针不进入安装器。

1. 通知区左侧：同宿主 84888，真实自动隐藏状态 0→1→0、窗口 top 1380→1438 / bottom 1440→1498；隐藏期间新 Snapshot 后仍 Embedded / failure=None，恢复显示、空动作、系统设置与原几何恢复通过。
2. 应用图标右侧：同宿主 93260，同样的实际隐藏 / 数据刷新 / 恢复与原几何检查通过。
3. 正常 Explorer 重启：旧 Shell 92648 → 新 Shell 71964，RmShutdown / RmRestart 均为 0，旧内核进程确认退出；同宿主 75904 保持，新画布代次 / system_revision 推进 / 旧动作为空 / 新 Shell 几何恢复通过。ForceShutdown=false / ComputerRestart=false，未重启或关闭电脑。

三项日志及 process-result.json 在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-final-receipt-native-399d299335e04298bad08c41d031aa05/`。此前 M13g7 的 65 项 taskbar 检查、严格 Clippy / fmt 和完整 Tauri 回归，以及 M13g8 受影响 lib 27 项与 release 两位置复测作为相关代码验证保留，不为仅签名打包重复运行无关测试。

新 Shell 后另复核原已安装 0.1.0 的通知区：只读检查退出 0，图标仍为 S_FALSE(1) / (2080,1380,2116,1440)；物理入口在 InputReady 因会话锁定 / 不可交互拒绝，退出 1，没有点击或移动指针，不计标准托盘菜单通过。证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-installed-tray-after-shell-2b1d5e392c4b44df89f7d3dbb8d8027c/`。此限制仅属于实际输入；本轮三项系统生命周期检查已正常执行。

M16t 替代旧段落中“最新候选”的含义，包含 M13g5–g8 的正式修复；失败的 05bede333b5a4d5ba71fb1f2ad2d53fd 包不能作为当前验收包。原安装 0.1.0 / PID 92688 保留，新候选尚未安装升级，不能宣称完整在线自动升级通过。鼠标触边、物理键盘 / Narrator、拥挤 / 非底部任务栏、多屏 / 各档系统 DPI、实际休眠、账户外部状态、真实 Codex 回合、WebView2 缺失与完整在线升级继续独立待验。没有推送代码、创建 release、上传资产或运行性能测试；用户原未提交内容保持。

## M13g8：release 回执的一次布局校验结果

M13g7 后的完整包已编译 / 签名为未发布本地候选，但 release 宿主实测首次隐藏刷新返回 Unavailable / Os，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-final-release-native-3ab8a3ae9a264d18aa501ea29998306b/notification-left.log`，退出 101，后续右侧 / Explorer 场景未执行，不能将此包视为验收完成。失败后系统自动隐藏按 guard 恢复，原安装保留。

原因是回执协调租约后又调用一次 valid：Explorer 重绘可在两次采样之间恢复任务区，产生新的失败状态却跳过隐藏 / 重预留。现在协调函数返回本次已校验 / 已安全重预留的结果；回执直接使用该结果，不在填字段时重新读取另一帧。校验不能成立时先脱离，再读取读数可见性，避免返回 unavailable 却遗留可见读数。不是忽略失败，校验与条件恢复规则不放宽；下一次请求继续重新检查实际布局。

正式 release 构建的原生宿主及同 profile 探针实际两位置均通过，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-release-receipt-d56c4777268d46d09adbfaa7d117022c/`：左侧同宿主 91284、右侧 83580，真实隐藏位移 / 新 Snapshot / 状态 Embedded、显示恢复 / 空动作 / 设置及原几何恢复均退出 0。M13g7 全套 65 项为此前检查，本轮受影响的 lib 27 项重新通过、严格 Clippy all-targets / fmt 通过。完整安装器需要从此新提交重建并签名，旧 05bede333b5a4d5ba71fb1f2ad2d53fd 候选保留为失败时点证据，不能推荐为最新验收包。未发布、未安装升级、没有电脑重启或性能测试。

## M13g7：两位置自动隐藏后的实际数据刷新

扩展显式自动隐藏探针的 `--application-right` 选项，并新增隐藏后再发送正式 Snapshot / GetStatus 的强断言。先前 M13g6 只检查首次隐藏状态与恢复；本轮更严格检查发现应用图标右侧在隐藏刷新后失去预留。多份失败证据保留，最后精确窗口诊断为 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-application-7004947f690b4befbfeeea4277e1e04c/`：读数仍在正确的移出屏幕位置，Explorer 重绘却将 MSTaskSwWClass / MSTaskListWClass 恢复为本实例登记的完整原始宽度。失败不计通过，所有场景均按 guard 恢复系统开关；临时全拓扑调试输出已移除。

正式修复分两层：仅当真实自动隐藏状态开启、完整底部任务栏在对应显示器外（最多保留两物理像素边缘）、租约仍通过验证时，暂缓 UIA 的应用按钮测量，因为此时系统正确返回按钮 off-screen；可见后恢复普通严格测量，不忽略普通隐藏 / 溢出按钮。若 Explorer 将预留恢复成完整原始宽度，只允许同一内核 Shell、同一根 / 读数类、同尺寸 / DPI、所有权属性仍匹配、当前任务区精确等于登记 original、读数位置及其他控件全部匹配时重新预留 expected。非原始几何、外部归属、显示状态、DPI / 尺寸 / 版本或未知系统状态均拒绝。原生准备、状态检查和生成回执前协调租约，失效但无法安全重预留时立即隐藏 / 脱离，不发布残留读数。

真实 Win10 19045 / 150%、合成展示 DTO：应用右侧证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-application-7d0b8f393c64444f8f26977e72457bc1/` 退出 0，同宿主 89844；通知区左侧最终复测 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-refresh-2cedc130b13b4c2eb7b4cc4b97c3e4e7/` 退出 0，同宿主 92604。两项均真实开关 0→1→0、观察 top 1380→1438 / bottom 1440→1498、隐藏及新快照刷新后仍 Embedded / failure=None；恢复显示、空动作、禁用 / shutdown、系统设置及完整原几何恢复通过。没有键鼠输入、Explorer 终止或电脑重启。

taskbar all-targets 65 项通过、1 私有入口 ignored；新增纯坐标预期拒绝可见 / 局部离屏 / 非完整宽度，只接受真实完全隐藏形态。严格 Clippy all-targets / fmt 通过；未知系统状态补强后 Clippy 及左侧实际场景再次通过。完整 `-TaskbarActions` 退出 0，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-final-regression-65f9432a30b743f89e9212201856af70/`，两位置 / 按钮增减 / 菜单 / 详情 / 画布重建 / Tauri 页面 / 隐私 / 退出原几何回归通过；SHORTCUT_CONFLICT / 1412 如实保留。底部两位置的系统自动隐藏与刷新已验，但鼠标触边唤出、其他任务栏方位、物理键盘 / Narrator、拥挤、多屏 / 各档系统 DPI、休眠和剩余外部场景仍独立。旧签名候选尚未包含本次修复，接下来从已提交源码重新打包；不推送 GitHub，无性能测试。

## M13g6：真实系统自动隐藏与布局平移恢复

使用新显式开发入口 `check_taskbar_autohide`，本机 Win10 19045 / 150% DPI 实际设置任务栏自动隐藏，读取真实状态并观察 Explorer 窗口移动；不发送键鼠、不向宿主伪造系统通知、不终止 Explorer 或重启电脑。仅接受初始非自动隐藏的受支持主任务栏；持有旧 Shell 内核进程句柄、核对 PID / 完整类名、固定物理 DPI 上下文。变更前记录 ABM_GETSTATE，恢复 guard 在正常返回 / 断言 unwind 时按同一代次与已知当前状态恢复。ABM_SETSTATE 总返回 TRUE，因此不以返回值作为成功证据。探针是开发验收入口，不进入安装器或普通测试。

实际首轮虽完成开关 / 恢复，但正常隐藏被旧屏幕绝对坐标检查误报 UnsafeGeometry。修复为：已附着租约只允许整个任务栏、ReBar 与通知区在同尺寸 / DPI / 内核归属下平移；读数必须严格跟随这一偏移、仍为同根子窗，预留区 / 任务子列表与其他系统控件仍逐项核对。应用图标右侧位置的按钮比较使用当前原点。尺寸 / DPI / 内部相对位置变化及溢出仍拒绝；构造阶段保持原严格屏幕坐标检查，不能在 SetParent / 显示前放宽竞态保护。

第二轮暴露恢复动画中一次挂接失败后沿用过期拓扑，使后续普通快照仍失败，证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-native-fefb9fddba8a44109b7b65636fa9d4cb/`，退出 101，设置已恢复，不计通过。现在没有有效租约的普通重试重新读取实际拓扑，而不等待额外系统广播；旧布局仍按归属条件释放，不能写回新的用户布局。

最终本机证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-native-43d5a1556db14844bad0882ef6fb0b9f/`，退出 0：ABM 状态 0→1→0；实际主任务栏 top 1380→1438、bottom 1440→1498，证明是真实隐藏移动而非仅改一个设置位。完全隐藏时同宿主 PID 96164 为 Embedded / failure=None；恢复正常位置后仍嵌入，动作为空；禁用 / shutdown 后系统状态及完整原始几何均恢复。此前修正后的 d57a12e64d7e4213b3a2f75491d69277 场景也退出 0；最终场景额外显式断言隐藏时仍嵌入。

自动 taskbar all-targets 64 项通过、1 私有入口 ignored；新增 2 项独立预期检查隐藏平移的固定像素与按钮原点，拒绝 DPI / 尺寸 / 局部移动 / 未知版本 / 整数溢出。最后坐标边界补强后相关 2 项再测通过，严格 Clippy all-targets / fmt 通过。完整 Tauri `-TaskbarActions` 退出 0，实际两位置 / 按钮增减、详情 / 画布重建、菜单 / 小窗 / 统计 / 设置 / 隐私 / 禁用 / 正常退出几何回归通过；证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-autohide-regression-368a6c05c7744aea84898f2deac8092b/`。SHORTCUT_CONFLICT / WebView2 注销 1412 保留。

实际隐藏场景使用通知区左侧位置与合成 DTO；系统开关 / 真正移动 / 恢复已验，不将其表述为鼠标触边唤出或所有位置的物理交互验证。应用右侧的实际自动隐藏、物理输入 / Narrator、拥挤 / 多屏 / 各档系统 DPI / 休眠等继续独立。本轮原安装 0.1.0 / 用户未提交内容保持，M16s 候选尚未包含本次新增平移修复；接下来重新构建候选。未推送、未改登录、无性能测试。系统接口依据 [ABM_GETSTATE](https://learn.microsoft.com/en-us/windows/win32/shell/abm-getstate) / [ABM_SETSTATE](https://learn.microsoft.com/en-us/windows/win32/shell/abm-setstate)。

## M16s：纳入 Explorer 修复的签名候选与 release 原生恢复

完整 `npm run tauri:build` 退出 0，包含 M13g5 / 源码提交 a72da26 的空裁剪修复及 HWND / Shell 代次检查。TypeScript / Vite、334 项第三方声明、正式 release 宿主 / 桌面和唯一 NSIS 渠道均完成。构建证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-shell-release-e84cd34c2993480a9f8b7666d6d194ba/`；旧 M16n 产物已在其发布候选目录保留，不覆盖旧签名，不改已安装 0.1.0。

将同次桌面、宿主及安装器复制到新自有候选目录 `target/release/candidates/v0.1.1-shell-7e36b18664984d51a9dcf27cb4dbeaa8/`，记录源码提交 / 构建证据。使用此前同一已确认项目密钥签名新目录的安装器；`release:prepare` 通过实际 production release 验证器核对版本 / 签名 / 字节，并生成新本地目录 `target/release/publish/v0.1.1-shell-7e36b18664984d51a9dcf27cb4dbeaa8/`。公钥摘要仍为 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`，未生成第二套密钥、未读取账户认证。清单仍使用固定 GitHub v0.1.1 资产 URL，候选时间不是发布证据。

- 安装器：6,623,455 字节，SHA-256 `c5dda4ecc954660ae85f8c45fc7bf08b3f6d87c4edd8517d3494347a1c983937`。
- 签名 SHA-256：`1dc1beac9096a1da4d74fdc05296226b83efc691cd05bb65322a243af36f2ac5`。
- 桌面：25,533,952 字节，SHA-256 `fb56225a243048034d3845da7f2510794136558168c6ec70fcd377d754197671`。
- 宿主：2,351,616 字节，SHA-256 `b6e61583b85db8800f1bf0e22393027c3f95f7bc3b629d5225e14bd885231e74`。

进一步构建 release 版 Explorer 验收探针，放入上述候选目录的 examples 后运行，以其默认同目录寻址实际使用候选 release 宿主（运行前核对上述 SHA），而非 debug 宿主。真实正常 RM 关闭 / 恢复再次退出 0：Shell 96104 → 92648，RmShutdown / RmRestart 均 0，同宿主 PID 94612 保持，画布换代、修订推进、旧动作清除、禁用 / 再启用 / 退出恢复新 Shell 几何通过。证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-release-7c244d46d8674c548a889164e2fe139c/`；没有电脑重启 / 关机或强杀，探针不进入安装包。

M16s 是最新本地候选，替代 M16n 的“最新”含义。尚未安装 / 发布新候选，不能据此宣称 0.1.0 → 0.1.1 完整自动升级通过；真实系统矩阵与其余待验项保持。本轮结束 Explorer 92648 和原已安装应用 92688 保留，临时宿主已退出。未推送代码 / 创建 release / 上传资产，无性能测试。

## M13g5：真实 Explorer 重启恢复与空裁剪区域修复

本机 Windows 10 19045 / 150% DPI 完成实际 Explorer 进程正常关闭 / 重启验收。新显式开发入口 `check_taskbar_explorer_restart` 启动同目录正式原生宿主，仅发送合成展示 DTO；在同一宿主持续心跳期间，由独立脚本通过 Windows Restart Manager 关闭、恢复唯一登记的 Explorer。没有伪造 TaskbarCreated，没有以重启宿主替代恢复，没有强杀 Shell，也没有重启 / 关闭电脑。

此前 M16r 的两个 ApplicationFrameWindow，经只读 DWM / 子窗口结构检查确认是 Shell-cloaked 且仅含同 Explorer 的标题 / 输入框架。资格检查只额外允许这一严格结构：DWM_CLOAKED_SHELL=2、有界非空后代、所有后代属于同 PID 且类名仅为 ApplicationFrameTitleBarWindow / ApplicationFrameInputSinkWindow；枚举后再核对归属 / cloak。文件窗口（含隐藏）、有应用内容的框架、未知可见类与不完整枚举仍拒绝。默认入口只读；显式 `-RestartShell` 再核对 PID / 创建时间 / 同会话、唯一 RM 进程、Restartable / RmExplorer / 零 reboot reasons，仅使用 RmShutdown 的 OnlyRegistered 标志 0x10，不使用 ForceShutdown。RmRestart 始终在 finally 执行；旧内核进程已结束但任务栏仍缺失时，才从系统路径隐藏启动 Explorer。共享 C# 位于 `scripts/explorer-restart-probe.cs`，默认只读脚本不调用变更方法。

早期七次实际恢复使宿主状态管道关闭，失败证据保留，不记通过；最后一份为 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-native-8cf1e45937ab4beba1fbafbdea1d5f2c/run.log`。增加仅 debug、opt-in 的有限错误输出后，用既有隐藏窗口测试独立复现：FillRect=0、GetLastError=0、GetClipBox=NULLREGION。实际原因是隐藏 / 脱离窗口没有可绘制区域，原代码将合法空区域误报为绘制失败；仅更换窗口资源不能修复这一错误。

正式读数及详情现在区分有效空区域与无效 DC：空区域跳过绘制，缓存帧 / 标题 / 动作仍按隐私屏障清除，可见时使用最新帧；无效 DC / 裁剪 API 失败仍报错。加强 HWND 当前进程 / UI 线程 / 完整 UUID 类名校验；TaskbarCreated 保留原通知类型到重入队列并重建窗口代次；独立 Explorer PID / 内核创建时间 / 根 HWND 变化也触发重建。旧租约只按原归属释放，不将旧几何应用到新 Shell。

实际正向证据：`C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-native-d1e8b0e8f4454191a0fc6cc868a37e65/run.log`，退出 0。旧 Shell PID 23564 → 新 Shell PID 96104，RmShutdown / RmRestart 均为 0；旧内核句柄确认结束，正常 RM 路径下退出码为 1（如实保留，不当作强杀判据）。同宿主 PID 84612 恢复 Embedded，新读数 UUID 类名、system_revision 增加、settings_revision 保持、旧动作为空；禁用恢复新租约，再启用 / 正常 shutdown 与独立记录的新 Shell 原几何相等。输出 NATIVE_EXPLORER_RESTART_OK，ForceShutdown=false / ComputerRestart=false。

自动 taskbar all-targets 62 项通过，1 项私有子入口 ignored；新增独立位图预期验证空区域不改变像素 / 调用方裁剪、恢复可见后重绘及无效 DC 仍失败。既有隐藏画布 / 详情清屏、Tab / 失焦检查通过，严格 Clippy all-targets / fmt、PowerShell AST / 默认只读 RM 查询及差异检查通过。完整 Tauri 回归 `-TaskbarActions` 也退出 0：详情、丢失画布重建、实际按钮位置、菜单、统计 / 小窗 / 设置 / 隐私 / 禁用与最后几何恢复通过；证据 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-taskbar-shell-regression-bfdd2460e1bd42eca1a58aa194d05e8e/run.log`。自有窗口消息不替代物理点击；SHORTCUT_CONFLICT 与 WebView2 注销 1412 保留。

本轮只完成本机 Win10 / 单屏 / 150% 的 Shell 生命周期。物理键盘 / Narrator、自动隐藏 / 拥挤、多屏 / 其他系统 DPI、实际休眠及标准安装托盘仍独立待验。已安装 0.1.0 与原未提交内容保留；M16n 的旧 0.1.1 签名候选尚未包含本轮源码修复，未推送 GitHub，无性能测试。原生规则依据 [GetClipBox](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-getclipbox)、[IntersectClipRect](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-intersectcliprect)、[RM_SHUTDOWN_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/ne-restartmanager-rm_shutdown_type)。下方 M16r 保留当时事实，以本轮证据更新当前状态。

## M16r：Explorer 正常重启资格的只读检查

继续为真实 Explorer 生命周期准备受控验收方法，新增 `scripts/inspect-explorer-restart.ps1`。此入口只有只读检查，不声明 / 调用 RmShutdown、RmRestart、系统重启、进程结束或窗口消息。限定 Win10 19045 x64；核对唯一主任务栏、系统 explorer.exe 路径、同一会话及内核创建时间，在检查结束再次验证 Shell PID / HWND 未改变。不读取窗口标题、用户名、域或认证。

通过 Windows Restart Manager 创建本次临时会话，只登记明确的 PID + 创建时间，不登记文件 / 服务或遍历其他应用；有界取得列表后核对唯一同一进程 / 会话、RmExplorer 类型、Restartable 和 reboot reasons，并在 finally 结束本次 RM 会话。未知字段保持 null，不把失败当作无重启原因。原生 ABI 大小 12 / 668 在调用前校验，枚举失败 / 多个主任务栏均拒绝。文件窗口（含隐藏窗口）与白名单外的可见 Explorer 窗口使 Eligible=false；默认输出成功代表检查完成，不代表重启验收成功。

本机实际退出 0，ShellPid=7552 / SessionId=1；RmGetList=0、RegisteredProcesses=1、ApplicationType=4、Restartable=true、RebootReasons=0，说明当前 RM 查询没有要求电脑重启。实际 FileWindowCount=0，但同进程有两个可见 ApplicationFrameWindow，用途尚未核实，Reason=UnknownVisibleExplorerWindowsPresent / Eligible=false。复核输出保存在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-explorer-inspection-a8a53722003d44ce8308cbdbd4405def/inspection.json`。没有关闭这些窗口、重启 Explorer 或重启电脑；正式应用 PID 92688 保留。用户同时明确“不要重启电脑”，已同步实施计划第 7 节，后续不主动触发 Windows 重启 / 关机。

此证据将限制从泛称“锁屏不能测”细化为实际进程重启资格与用户窗口状态。后续 Explorer 完整验收仍需在明确的可控 Shell 状态下绑定宿主、观察旧进程结束及新代次恢复；本入口不缩减或替代该场景。规则依据 [RM_PROCESS_INFO](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/ns-restartmanager-rm_process_info) 和 [RmGetList](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist)，实际改变进程时应遵循 [RmShutdown](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmshutdown) / [RmRestart](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmrestart) 的正常关闭、同会话及出错后恢复要求；当前没有执行这两项调用。

脚本动态 C# / 真实 RM 查询、PowerShell AST / diff 随提交核对；实际正向重启未验。生产程序、候选包 / 密钥与用户数据保持，未推送 GitHub，无性能测试。

## M16q：无需桌面输入的三项原生功能复测

用户指出不能把所有剩余场景都归因于锁屏。本轮直接运行既有受控原生入口，而非再次只检查锁屏或重复要求解锁。三项均使用 UUID 隔离开发数据库、真实 WebView / SQLite / 生产实现；自有 HWND 消息、DOM / IPC 和后台子进程不依赖 SendInput，没有删除输入归属保护或改变正式设置。复测后的 WTS 头为 SessionId=1 / ConnectionState=0 / SessionFlags=0；正式标准目录 0.1.0 的 PID 92688 保留，开发应用及原生宿主均已退出。

- `scripts/native-smoke.ps1 -TaskbarActions` 退出 0：NATIVE_TASKBAR_DETAILS_OK / RECREATE_OK / APPLICATION_POSITION_OK / MENU_OK / ACTIONS_OK。真实详情 / 原生菜单、同宿主窗口丢失重建、两位置与实际按钮增减、完整管道到展开小窗 / 同范围统计 / 设置导航 / 隐私切换 / 禁用持久化，以及退出恢复任务栏几何通过。动作由自有窗口消息产生，不把这些通过表述为真实鼠标操作或 Explorer 进程重启。
- `scripts/native-smoke.ps1 -RecoveryRoutes` 退出 0：NATIVE_RECOVERY_MESSAGE_ROUTE_OK / MINI_OPACITY_OK / RECOVERY_ROUTES_OK。实际设置 UI 的快捷键注册 / 全局归属、外部冲突与写入失败保持、消息恢复交互、窗口 alpha 255→204、失败回退及 WebView 重建读回持久偏好通过；不发送真实键盘输入。
- `scripts/native-smoke.ps1 -Notify` 退出 0：NATIVE_NOTIFY_IPC_OK / COLLECTOR_OK。真实主 WebView 五项命令、配置预览 / 启用 / 撤销及原内容保持、离线 marker / 重启排空、正式 headless 在线和重复提示、暂停来源、合成只读日志消费 3 / 10 / 11 通过；不执行真实 Codex 回合或读用户 Home / auth.json。

编译版本为当前 debug 0.1.1，未重打包 / 修改生产安装。各场景记录 SHORTCUT_CONFLICT（正式应用同时运行，未据此确定另一个注册者）及已有 WebView2 注销 1412，场景断言和实际进程退出均通过；恢复场景使用独立已验证按键完成自己的注册验证，不以启动冲突作为快捷键失败 / 成功的替代。

测试方法边界明确区分：锁屏下仍可验证后台、服务协议、原生消息与 UIA 属性 / 程序接口；真实输入设备 / Narrator 和全屏命中是交互验收，多屏 / DPI 是实际显示环境，账户过期 / 重置与 Codex 回合是外部状态，完整自动升级是发布源 / 真安装流程。不能把后三类全部归因于锁屏，也不能用本轮消息检查替代未发生的实际场景。当前范围和未验项不缩减，不创建远端发布，不运行性能测试；原未提交内容保持。

## M16p：正式安装版的后台窗口控制与冷启动复测

用户在明确得知锁屏下只能使用后台程序接口后要求“那你测吧”。先立即执行 M16o 物理入口，InputReady 在输入前拒绝，退出 1；不重复将该失败认作托盘通过。随后直接控制标准目录已安装的 0.1.0，先核对 HKCU 注册版本 / 路径、保存的 release baseline（只允许固定 NSIS marker 差异）、唯一应用进程及固定 muda 菜单顺序。通过本应用 PID / HWND 校验的 WM_COMMAND 与 WM_CLOSE 操作，不调用 SendInput，不展开通知区，不安装 / 卸载或修改系统设置。

实际序列退出 0，输出 INSTALLED_RUNTIME_COMMAND_SEQUENCE_OK：关闭统计隐藏窗口且保留进程，统计命令重新显示，小窗命令创建 / 显示后可关闭；第二次启动实际退出码 0 且唤回原统计窗口，始终只有一个应用实例。正式退出命令使原 PID 85580 正常退出 0；随后标准安装 exe 冷启动，新 PID 92688 的统计 / 托盘窗口建立，真实 WebView 的 UIA“总览”导航可读。finally 恢复测试前统计 / 小窗均隐藏的状态，保留正式应用运行。

数据库前后各用只读 SQLite 事务取得完整一致快照，46 张非 settings / app_state 表逐表核对行数与全行规范化摘要完全一致；独立比较 data_revision=0 / price_revision=1，quick_check 均为 ok。设置与 app_state 中的窗口保存 / settings 修订不使用全文件字节比较，避免将正常位置保存误认为数据丢失；不覆盖或回写原数据库。未添加来源 / 演示日志，不读取 auth.json 或真实账户。证据及本次有限 PowerShell / Python 程序在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-installed-runtime-71a4c0b38db2463780772abefe04c618/`，含 before.json、after.json、verification.json。

这是正式安装应用的程序接口控制、单实例与冷启动的新增实际证据，PhysicalInput=false / PopupObserved=false；仍不能证明通知区实际点击、弹出菜单、物理键盘 / Narrator 或多屏 / DPI 通过。测试后只读 WTS 头为 SessionId=1 / ConnectionState=0 / SessionFlags=0，仍锁定；正式应用 1 个，自有原生宿主 0 个。生产源码、0.1.1 候选及签名保持；原未提交内容保持，没有推送 GitHub 或性能测试。

## M16o：正式安装托盘的受限物理输入验收入口

本轮恢复时 WTS 头为 SessionId=1 / ConnectionState=0 / SessionFlags=1，已解锁，正式 0.1.0 仍运行。立即对同一已安装 PID 执行现有受限回调 / 菜单校验，仍在 20 秒内未观察到菜单，退出 1；不记为通过。随后只读诊断：实际内部 ID=2 的图标矩形仍返回 S_FALSE(1)，Explorer 的通知区展开按钮 enabled / 非 offscreen，但无 UIA Invoke。没有由 S_FALSE 单独认定图标丢失、隐藏或依赖库缺陷；下一步需要实际展开通知区后验证。

新增 `scripts/verify-installed-tray.ps1`：默认仅诊断；物理模式显式 `-PhysicalInput`，要求指定 release baseline 且已安装文件只允许 NSIS marker 差异。本次使用 M16n 保存的 0.1.0 基线；依赖 / 单一 main-tray builder / 无 GUID 映射改变均拒绝，不猜其他图标 ID。限定已审查的 Win10 19045 x64，验证 Explorer 文件身份、唯一通知区 Button、窗口 PID / 根祖先 / 类、物理坐标命中、输入桌面一致、当前会话 active / unlocked、有效前台和无已按下按键。S_FALSE 场景只允许点击验证过的展开按钮，拒绝已打开的面板；实际本应用图标必须取得 S_OK 矩形后才发送右键。未知矩形保留 null。

实际打开菜单后才核对本应用三项文字 / ID。取消仅向已核对 PID 的本应用菜单发送；仅在本次展开点击已接受后收起仍可验证的 Explorer 面板，指针未被用户移动时才恢复。会话重新锁定时不绕过输入保护。通过对象只在全部 finally 收尾成功后输出，不以早期菜单观察当作完整场景通过。此脚本用于正式已安装应用，不能把 Explorer 窗口说成本应用拥有的窗口；它只操作经过上述有限校验的通知区入口，不改系统显示偏好、不退出 / 重装应用或改数据。

本机新入口动态 C# 编译 / 只读场景退出 0，矩形为 (2080,1380,2116,1440)。当前 0.1.1 release 作为 0.1.0 已安装基线被拒绝。真正物理场景执行时 WTS 已重新锁定，初始 InputReady 拒绝，未保存 / 移动指针、未点击 / 展开；随后同条件负向检查同样拒绝。AST / diff / 文档链接随提交检查。物理输入成功分支仍未验，正式托盘保持待验；没有将源码存在或锁屏拒绝视为功能通过。

正式 0.1.0 的原 PID 85580 / 路径保持运行，新脚本没有启动应用 / 宿主 / 安装器。固定公开更新清单 HEAD 仍为 HTTP 404；发布顺序问题未答复，不推送 / 发布，也未改生产 0.1.1 签名候选。原未提交内容保持，Win11 与取消范围不恢复，无性能测试。

## M16n：0.1.1 本地签名升级候选与升版后的原生更新页

为完整自动更新准备高于已安装 0.1.0 的实际产品包，项目 / npm lock / Cargo workspace / Tauri 版本同步为 0.1.1，Cargo.lock 的七个项目包版本同步；独立比较确认第三方依赖图与其他字段未变。首次 beforeBuild 的 locked inventory 因旧 Cargo.lock 拒绝；完整 offline metadata 同步后构建成功，没有放宽 --locked 或拉取新依赖。发布流程补充这一步，不使用 --no-deps 的输出替代完整锁文件同步。

完整 `npm run tauri:build` 通过 TS / Vite、334 项第三方声明、x64 release 独立宿主 / 桌面和 NSIS。新 `TokenPulse_0.1.1_x64-setup.exe` 为 6,624,908 字节 / SHA-256 `6f50baa9cb7806382ab7b3628d7563729aab0278c8bb074e771ce86c33c3e13a`；`.exe.sig` 为 440 字节 / `cbe7459191e2082c7b826775c3284bcea6b7201936c8f7e01fecf3dca06e6080`。desktop 25,533,952 字节 / `0388337a0ddd0a9ae807657e4c2503864e47f7b4b1802cdd9897e9ed3856b32b`，host 2,345,984 字节 / `4d82c7723bb3b56b5e686a30d9864739d95e6465c86309a447c4f82a3e29a000`。声明 2,986,262 字节及原 SHA 保持。生产功能 / 数据契约没有新改动，版本为后续真实升版提供前提，不视为已经完成升级。

使用同一已确认密钥，`release:sign` 与实际 release 验证 / 准备入口通过；公钥 SHA 仍为 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`，不生成第二套密钥。新本地目录 `target/release/publish/v0.1.1-20261003-ff6162077a1c4c4a844c94a87b65621a/` 包含安装包 / 签名 / latest.json / 两份验证记录；清单为固定 GitHub 仓库的 v0.1.1 URL，候选时间 2026-10-03T04:30:14.003Z 不是发布证据。准备工具 6 项通过；当前 0.1.1 verifier 对同密钥的真实旧 0.1.0 包拒绝且不建报告，验证版本绑定。对应自有证据在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-upgrade-candidate-a141188b9b85497d81f6be668ccc3b3e/`。

发现 debug 原生更新页检查写死 0.1.0；改为 JSON 序列化本次编译版本作为独立预期，正式 DTO / UI 不提供预期值。实际 `scripts/native-smoke.ps1 -Updates` 在 Win10 / 隔离无来源无账户数据库退出 0：NATIVE_UPDATES_INITIAL_STATE: Idle / NATIVE_UPDATES_IPC_OK，实际 main / mini WebView 的版本 / null / 权限 / 注入拒绝 / 隐私通过，没有联网、下载、安装。记录 SHORTCUT_CONFLICT（当时正式应用也在运行）和既有 WebView2 1412；此场景不证明恢复快捷键，没有改正式偏好。debug 检查不进入已签名生产包；strict desktop all-targets Clippy / fmt / diff 随提交复核。

旧 release desktop / host 已在构建前按原 SHA 保存到忽略目录 `target/release/baselines/v0.1.0-3f05cdf904034d139dd9fe8fe8c03207/`，原 0.1.0 发布候选保持。当前标准目录仍安装并运行 0.1.0，与保存基线除 NSIS bundle marker 外字节一致。没有安装新候选或启动安装器，没有真实源 / 账户读取，没有推送 / 发布或性能测试。完整自动升级仍依赖发布顺序答复及实际升级；标准托盘、物理系统矩阵和其他未验项保持。

## M16m：显式等待 release 验证进程与当前验收总表同步

本轮核对发现，PowerShell 直接调用 Windows GUI-subsystem 的 release 维护入口可能尚未等待退出就读取调用者的旧 LASTEXITCODE；报告已生成成功也可能被误判。新增 `scripts/verify-release-artifact.ps1`，固定调用当前 release 程序的有限维护命令，通过本次 Process 句柄等待 / 读取实际退出码，超时只结束本次维护子进程，拒绝已有报告并要求新报告存在。`verify-installer.ps1 -UseExistingLocalState` 使用同一入口；没有启动安装器、覆盖正式安装或修改应用数据。

真实正式签名包检查通过：调用者旧退出码为 57 时仍正确识别验证进程退出 0，带空格报告路径可用，摘要与 M16j 一致；已有报告被拒绝且 SHA 不变；仅修改自有 Temp 安装包副本一字节后，实际验证失败被识别且不生成成功报告。两个 PowerShell 文件 AST 通过；已注册安装在默认 / 显式本地状态两个入口均被拒绝，正式 PID 保持。diff / 文档链接随提交检查。证据在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-release-wait-bbcf728b5a3d4965975e41fbe478f680/`。首次直接调用误报失败由上述旧退出码复现解释，不将其当作签名失败。

同步当前 13 项总表：移除已取消的 Win11 门槛、已解决的密钥 / 基本本机安装条件，纳入 M13g4 / M16i–l 实际证据，仍保留未验项目。正式应用继续运行，原未提交内容保持，生产程序 / 安装包无变更，不重跑真实安装 / 卸载，不推送 GitHub。线上完整升级与用户规定的发布顺序存在依赖，已提出明确顺序问题，答复前不进行依赖发布动作。

## M13g4：解锁后的真实任务栏输入与五项 UIA 菜单动作

2026-10-03 用户回复“已解锁，可以复测”后，直接运行现有显式原生例程，不修改输入归属保护。`check_taskbar_wire --native-taskbar-wire-development-check` 退出 0：WaitingSnapshot → Embedded、仅 Token 时 Embedded、关闭后 Disabled；真实 actual_hover / actual_single_double / revision_privacy_clear / geometry_restored / passive_focus_preserved 均为 true，cleanup 为 Ok(NoRecord)。这是新的当前 Win10 真实鼠标、焦点和几何证据，补充旧 M13e7，而非重复把锁屏拒绝计为通过。

随后 `check_taskbar_accessibility --native-taskbar-accessibility-actions-development-check` 退出 0，输出 NATIVE_TASKBAR_UIA_PUBLIC_OK / ACTIONS_OK / MENU_OK / UIA_OK。实际自有宿主的五项标准菜单 Invoke 分别产生 OpenFloat / OpenStats / OpenTaskbarSettings / SetPrivacy(true) / DisableTaskbar，精确设置修订与唯一消费通过；完整精度 / null / 0、可聚焦名称、隐私 ACK 清空 / 新隐私名称及恢复原任务栏几何全部通过。菜单动作由本次合成探针消费，没有转发到正式应用或改用户设置；此项证明原生菜单到生产宿主管道的动作通路，不把它表述为五个生产页面已经由物理点击打开。

标准安装应用的托盘回调检查仍未观察到菜单。锁屏元数据最初识别到同会话 LockApp / Windows.UI.Core.CoreWindow，DWM cloaked=0；源码核对确认消息 6002 正确，托盘库只有 GetCursorPos 和图标矩形可用后才进入菜单。补充当前运行时 GetCursorPos 成功。只读图标探针最初错误假设内部 ID=1，返回 E_FAIL；核对固定 tray-icon 0.25.1 的 Builder 消耗唯一 ID 后，实际内部 ID=2，查询返回 S_FALSE(1) 与矩形。它不满足库内 S_OK 比较，不能沿用错误 ID 的结果认定图标丢失，也未将非 S_OK 矩形用于输入。真实通知区域 / 隐藏图标场景仍需单独验证；没有强制改用户图标显示偏好或给其他应用发送消息。

后续只读 WTS 当前会话头字段为 SessionId=1、ConnectionState=0、SessionFlags=0，按[Microsoft WTS 会话标志定义](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/ns-wtsapi32-wtsinfoex_level1_w)该时点已锁定；查询仅解析头字段，不读取用户名 / 域 / 登录时间。没有在此状态继续发送输入，没有关闭 / 绕过 LockApp。不能由后来锁屏反向否定本轮已退出 0 的输入和菜单动作检查。

未改生产 / 验收代码或重建安装包；正式标准目录中的 0.1.0 应用保留运行，两个自有原生探针 / 宿主均退出。基本安装 / 卸载仍由 M16l 证明；物理键盘 / Narrator、标准应用托盘交互、实际 Explorer 重启 / 自动隐藏 / 拥挤、多屏 / 四档系统 DPI、真实账户外部状态和完整高版本自动更新仍保留，不计取消的 Win11 或性能测试。GitHub 保持未推送 / 未发布。

## M16l：用户授权的本机真实安装、启动与普通卸载

2026-10-03 用户明确要求“直接在本机安装测试”，授权复用已有本地数据；不再将独立虚拟机作为基本安装 / 卸载的前置条件。初检没有卸载注册、正式安装目录或快捷方式，仅有旧保留的安装路径键与 SQLite 数据。原数据 schema v10，来源 / 会话 / 消费事件均为 0；没有清空、迁移回旧库或添加演示来源。

`verify-installer.ps1` 新增显式 `-UseExistingLocalState`：允许保留的数据和不含应用 exe 的旧安装路径记录；已有注册安装、快捷方式或正在运行的应用 / 宿主仍拒绝，默认入口仍拒绝旧数据。只向受校验的 UUID Temp 测试目录复制既有 db / WAL / SHM 并逐文件核对 SHA，同时留存旧产品注册键；不复制 WebView 缓存、认证或源日志，不自动覆盖恢复数据库。正式 release 维护入口核对实际 NSIS / `.sig` / 项目公钥 / 版本后才安装。窗口检查提取到 `installer-window-probe.ps1`，验收和失败后的受限收尾共用原有自有 HWND / PID / 菜单校验。

首次实际安装通过文件 / 注册核对、真实 UIA 总览、SQLite、关闭隐藏、第二调用重开、实际程序化托盘菜单、小窗及首次正常退出。第二个完整应用进程冷启动时托盘菜单未打开，脚本退出 1；因此该次默认序列未记为完整通过。核对同一自有安装路径 / 注册 / PID 后，使用已存在的显式自有托盘命令入口正常退出 0，普通 NSIS 卸载 0，程序 / 宿主 / 声明 / 卸载注册移除，卸载前后数据库 SHA 相同。该分步收尾记录在 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-install-cba0c080e7f043b584dc406a6147d938/recovery-verification.json`；首次数据副本和真实签名报告在同目录。

随后完整运行 `pwsh -NoProfile -File scripts/verify-installer.ps1 -UseExistingLocalState -OwnTrayCommands`，退出 0，INSTALLER_EXISTING_STATE_OK / FILES_OK / RUNTIME_OK / TRAY_COMMAND_ONLY / UNINSTALL_OK / SEQUENCE_OK 全部出现。实际普通安装和卸载、全部四项安装文件 / 当前用户注册、嵌入前端、单实例、小窗、两次正常退出及卸载保留数据库通过。TRAY_COMMAND_ONLY 明确表示自有窗口消息检查，不替代物理鼠标 / 键盘或真实弹出菜单。第二次证据目录为 `C:/Users/Amin/AppData/Local/Temp/tokenpulse-install-8f15285724084d68a1717b36a8b26c13/`，包含数据副本 / 签名报告；没有脚本递归清除数据。

最后将同一 M16j 签名包安装到标准用户目录 `C:/Users/Amin/AppData/Local/TokenPulse/`，NSIS 退出 0，0.1.0 注册路径、实际程序（只允许 NSIS 固定 bundle marker 的差异）、独立宿主和声明字节全部匹配正式构建。实际已安装应用的 UIA 总览通过，INSTALLER_STANDARD_USER_OK；本次保留安装并运行供用户使用，报告在上述第二次证据目录的 `standard-install-verification.json`。既有数据库只读 quick_check 为 ok，schema 仍 v10，来源 / 会话 / 消费事件仍为 0；运行正常保存窗口 / 设置，不能把启动前后整个数据库字节变化误记为数据丢失。

脚本 PowerShell AST / diff 通过；当前真实安装存在时，默认入口和 `-UseExistingLocalState` 均在动作前拒绝，不打扰已安装进程，INSTALLER_EXISTING_INSTALL_GUARD_OK。标准安装后的实际菜单观察也在期限内未打开；窗口辅助入口现以 finally 仅取消本应用菜单，真实失败路径检查输出 INSTALLER_MENU_LIMIT_CONFIRMED / 退出 0，无残留可观察弹出菜单，已安装进程仍在。不能把被捕获的菜单超时记成菜单通过，也未据此判定产品故障的原因。没有生产代码变更或重建 / 重签包，安装包仍为 M16j 的 6,624,606 字节 / SHA `9e3c22780697dee8c8d38b1699a592aa833469dfdce9809294a92e275133f250`。未读取 auth.json、启动 Codex 回合或运行性能测试，原未提交内容保持。

本机基本安装 / 独立运行 / 普通卸载已收敛；该安装仍为 0.1.0 初始公钥基线，不能称为更高版本完整自动升级。真实更新检查 / 高版本下载安装、缺 WebView2 实际条件、完整原生菜单 / 物理输入 / Explorer 生命周期以及物理多屏 / 四档 DPI 分别继续。Win11 取消，GitHub 按“全部验证后再推送”保持未推送 / 未发布。

## M16k：真实签名 NSIS 的原生启动交接

新增独立 opt-in 检查 `update_transport::install_acceptance::signed_nsis_handoff_uses_exact_arguments_and_requests_exit_once`，只在测试中使用新的自有 Temp 目录、缓存 NSIS 编译器及安装的 Tauri CLI。最小 NSIS 为无界面的用户级夹具，仅向同目录写参数和完成标记；不包含 TokenPulse 安装文件、注册表、快捷方式、父进程等待或用户数据操作。编译 / 签名子进程隐藏且超时有界，原始输出不打印；使用临时签名密钥，不读取正式项目私钥。

实际更新插件通过只允许本次 127.0.0.1 端口和固定两个路径的测试发布提供方下载，验签并核对签名版本 99.0.0 后，生产 UpdateService / staging / CreateProcess 通路启动该 NSIS。夹具独立读到参数 `/P /UPDATE /R /TOKENPULSE_PARENT=<本次测试进程 PID>`，退出回调恰好一次；旧修订安装返回 RevisionConflict，安装中的重复请求返回 UpdateBusy，不产生第二次退出。真实 HTTP 请求恰好两次，验签前没有参数 / 完成标记或退出请求。

Win10 19045.6466 / 150% 显式检查输出 NATIVE_SIGNED_NSIS_HANDOFF_OK、退出 0；transport 普通检查 2 项通过，两个 opt-in 场景默认忽略，desktop all-targets strict Clippy / fmt 与 diff 通过。这是自动测试调用真实 Windows 原生启动器的窄范围检查：退出回调被测试计数器替代，未实际退出 Tauri / 清理任务栏，也未安装或升级 TokenPulse；现有独立父进程等待检查与这项启动交接分开，不能合并宣称完整产品升级通过。HTTP 仅属 debug 隔离夹具，生产 HTTPS / 固定发布源 / 公钥及安装门禁均未改动。成功启动的公开签名夹具按生产 staging 行为保留在系统 Temp，本次自有测试目录与临时密钥在结束后清理。

未重新打包：该变更仅测试代码与文档，M16j 的正式安装包和签名保持。新包干净安装 / 普通卸载、完整高版本更新、原生菜单 / 输入 / Explorer 生命周期以及物理多屏 / 四档 DPI 等 Windows 10 待验项仍保留。Win11 取消，现有正式数据与原未提交内容保持。

## M16j：新项目公钥下的安装包与本地签名发布资产

M16i 对应源码（提交 `545d61a`）完整执行 `npm run tauri:build`，TS / Vite、334 项第三方声明、x64 release 宿主、桌面及 NSIS 均退出 0。新包默认内嵌项目公钥，用户安装 / 更新应用不需要输入、复制或手动验证密钥；应用下载后自动验签。密钥文件只用于开发者发布，本轮没有新增产品密钥输入界面。

当前 `target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe` 为 6,624,606 字节 / SHA-256 `9e3c22780697dee8c8d38b1699a592aa833469dfdce9809294a92e275133f250`；旁边 `.exe.sig` 为 440 字节 / `c134dd2a860c511be9cb016bf3d4e0944a83bacdef888a4d1fde01cf73bedcbb`。同次 desktop.exe 为 25,533,952 字节 / `2607d12e8f149730323ea997ae75d42d9fded43d535a2859638959732a7986fa`，host.exe 仍为 2,345,984 字节 / `175bb507a4e0d6efba2bdce1c6c800c027283adf13c4678baa6685d0dbf41a10`。声明仍为 334 项 / 2,986,262 字节 / `2dad741130f6a68d7fcf87fba744043a1ba838e9d0cf8bfc846630ee006001e0`。本条替代旧包的最新产物含义，旧包哈希和历史验收保持。

本机 `release:sign` 使用新的加密私钥与当前用户 DPAPI 密码通过，签名绑定版本 0.1.0；`release:prepare` 调用这次正式桌面 exe 的真实维护验证入口成功，报告公钥摘要与项目公钥 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`、安装包字节 / 摘要和目标均一致。已在新的忽略目录 `target/release/publish/v0.1.0-20261003-4a487957145b49a5ac0f1e748eec7e1b/` 生成同字节 NSIS、原 `.sig`、固定 GitHub 发布 URL 的 `latest.json` 和 `release-verification.json`；清单候选时间 2026-10-03T03:03:55.963Z。未上传 / 创建远端 release，候选时间不作为已经发布的证据。

2026-10-03 对固定生产 `releases/latest/download/latest.json` 的未认证 HEAD 请求实际返回 HTTP 404；它证明本次公开请求不能取得清单，不能据此判断仓库不存在或私有状态。新公钥的配置及本地签名验证已经完成，公开发布、安装这一新公钥基线和随后新版本的完整更新仍需分别完成。此版本与已有 0.1.0 版本号相同，是首次项目公钥基线，不伪装成高版本升级。Tauri `.sig` 是应用更新验签，Windows Authenticode 实测 NotSigned；没有新建证书签名渠道或将它加为交付门槛。

Windows 10 验收继续；已有正式数据、原未提交内容保持，未覆盖安装 / 普通卸载，不读取认证或用户日志，不运行性能测试。Win11 实现 / 验收按用户更正取消。所有本次自有应用 / 宿主进程已退出；本轮真实输入受 CoreWindow 覆盖的拒绝仍为待验，不被签名资产准备代替。

## M16i：Windows 10 范围更正与新项目更新公钥

2026-10-03 用户更正“验证 Win10，不管 Win11”，并明确选择生成新密钥。继续 Windows 10 功能及物理兼容验收，Windows 11 实现 / 验收取消，不再作为发布门槛；原多屏 / 主屏切换 / 断屏 / 四档 DPI、原生输入和 Explorer 生命周期仍保留。下方 M16h 范围表及更早 Win11 待办表示当时范围，以本条和[实施计划第 7 节](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)的新确认优先。

已在仓库外生成一套新的加密 Tauri 签名密钥，目录仅当前用户 / SYSTEM，随机密码用 Windows 当前用户 DPAPI 加密保存；没有输出私钥 / 密码，没有复制到仓库或安装包。跟踪公钥文件为 `src-tauri/resources/updater-public-key.txt`，去除尾换行后的公钥 SHA-256 `f1143c37e0960c8e80ddbff531c59290d33739133c3d0894bfe937815dd3a0e1`。正常应用与 release 本地验签共用默认项目公钥，保留显式编译环境覆盖；构建不需要私钥，运行不读取签名目录。

新增 `npm run release:sign`，本机登记 / 权限与公钥匹配后，通过受控子进程解密 DPAPI 密码并用官方 CLI 签同版本 NSIS；拒绝已有签名 / 错误文件名 / 超限 / reparse，签后检查安装包没有改变，原始 CLI 输出丢弃。不上传、安装、生成第二套密钥或更改用户数据。新公钥下 native -Updates 同时检查 configured idle 和旧 unavailable 分支，合法公钥时不联网，保持下载 / 安装及 mini 权限门禁；新包 / 签名资产与实际检查另列，M16h 不作为已内嵌公钥的包。

验证：普通 release verifier / transport 检查 3 项通过，显式真实临时签名的版本 / 错密钥 / 篡改及本地下载验签两项通过，发布准备 Node 6 项通过；desktop all-targets strict Clippy / fmt、签名脚本 PowerShell AST 和 diff 通过。Win10 19045 / 150% 的正式主 / 小窗检查输出 NATIVE_UPDATES_INITIAL_STATE: Idle 与 NATIVE_UPDATES_IPC_OK、退出 0，默认项目公钥确实初始化生产更新 owner；检查按钮可用，未产生联网 / 下载 / 安装，未经候选仍拒绝下载，安装 / mini / 直接 plugin 权限仍受限，1412 提示保留。

本机实际 `release:sign` 通过加密私钥 / DPAPI 密码签新包，随后 production release verifier 在准备工具内验证项目公钥与版本成功；重复签名拒绝且原 `.sig` SHA 未变，实际新 release verifier 对不相关、明确不可执行的合成字节返回 14 / 不建报告。最初合并负向测试与递归临时目录清理的命令被自动审核拒绝，未执行且无具体原因；改用分离的只读签名保护检查和只在新 Temp 目录写入合成文本的检查，无递归清理 / 安装包修改。临时签名夹具的篡改检查与新项目签名正 / 负证据分别记录，不把未执行命令计作通过。

同轮再次运行已构建的真实 wire，仍在自有前台夹具被全屏 CoreWindow 覆盖时、发送输入 / 启动宿主之前拒绝，退出 101。Windows 10 原生输入、完整 Explorer 生命周期、物理多屏 / 四档系统 DPI 保留，Windows 11 已取消。未读取认证 / 用户日志、未改现有正式数据，未运行性能测试。

## 当前范围核对（2026-10-03，M16n）

本节按当前目标的 13 项保留功能核对实际实现与最新证据，作为当前待办入口；下方历史段落仅保留当时检查结果。[已确认范围](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)及后续用户更正优先：Windows 11 已取消，Windows 10 多屏 / DPI 仍保留；基本安装 / 卸载已授权在本机复用本地状态实测，不再要求独立虚拟机。当前公开发布和本机正式安装版本为 M16x 的 0.1.2，已纳入 M13g5–g11 及 M16w 安装竞态修复；实际固定 GitHub 发布源的 0.1.0→0.1.2 更新 / 正常退出 / 自动启动 / 文件和数据库核对通过。M16v 同包宿主的各项系统验收保留其二进制时点边界，不冒充 0.1.2 整包全部系统实测。新增 cache_write_input_tokens 真实日志兼容待办，完整交付尚未达成。

|保留功能|已有实现与证据|剩余工作或实际验收边界|
|---|---|---|
|1. Windows 本地来源|目录检测、原生选择、暂停 / 恢复、停止后保留历史；M06g 正式 React / IPC / SQLite 普通流程通过|选择器程序化原生操作已验，物理输入仍单独记录；不新增 WSL / 网络适配或 Win11 验收|
|2. 历史与增量采集|监听、核对、启动及唤醒补扫；M06e / M06f8 实际 Win10 追加、替换 / 改写候选与归档移动检查；M07r2 真实 S3 / 系统事件与恢复补扫 3→10 通过|M16x 真实来源发现 cache_write_input_tokens 新字段被白名单拒绝，优先修复及验证其语义；合成日志不等同覆盖所有实际格式。S3 使用 debug 隔离程序，不合并为安装包全应用验收|
|3. Token 核算|累计流、重置、缓存 / 推理包含、镜像与继承、歧义隔离；M05 / M07 独立夹具与事务发布检查|未知和隔离量保持独立，不混入可信消费；候选验证后切换保留旧结果|
|4. 七个统计页面|真实 DTO、统一筛选、真实 SQLite 快照 / 分页、会话详情与固定范围；M10 / M11 浏览器及 Win10 WebView 验证|主题及实际布局已检查，小工作区导航 / 设置 / 页脚由 M15f3 补验；物理 UI 与多屏兼容仍独立|
|5. 简化诊断|来源状态、扫描时间、错误原因 / 必要定位、重新检测、暂停恢复和基本重建进度；M10d1 / M10d2|不增加完整任务历史、原文 / 继承证据浏览器或导出|
|6. 计价与重估|精确三费率原子金额、自定义 / 来源规则、版本化别名及价格、51 模型 / 172 档位事实、37 条 Standard 参考规则；M09g2b3 已贯通持久缓存、独立服务、自动补建、手动进度 / 取消|实际 Standard 未识别；单请求长度选档、实际模式、独立写入全链路、地区 / 合规条件仍缺，见计费专项澄清。工具 / 多模态 / 运行时在线更新单列扩展；早期后台服务 / UI 未完成描述已收敛|
|7. 小窗|280×220 / 360×380 DIP、主题、费用 / Token / 账户、置顶 / 透明度 / 穿透与恢复键、位置保存 / 缺屏回退；M11 原生及 UI 检查|物理多屏拖动、主屏切换、断屏和四档系统 DPI 尚未完成；合成几何不能替代这些检查|
|8. 本地账户额度|用户选择程序 / Home 后复用已有登录，无新增登录；M12i 真实持续读取、M12j 冷启动、M13f1 三入口 / 后台刷新、M12m 普通选择 / 保存 / 连接通过|真实账户身份变化、通知、登录过期 / 实际重置仍需对应外部状态；已有合成领域 / 服务 / UI 检查，不主动切换或登出用户账户|
|9. 原生任务栏|独立宿主、Win10 两位置 / 实际按钮重排、详情 / 菜单 / 白名单动作、隐私、清理与回退已贯通；真实悬停 / 单双击 / 焦点及五项 UIA 动作通过；完整 Tauri 回归、M13g5–g11 修复与 M16v 同包 release 真实 Explorer 同宿主恢复、两位置实际自动隐藏 / 新 Snapshot 刷新 / 显示恢复 / 原设置与几何恢复通过；M13g12 / M13g13 同包宿主 + debug 桌面两位置各自实际 S3 恢复后新读数 / 详情 / 退出完整几何通过|UIA 由探针消费，Tauri 回归使用自有消息，宿主展示为合成 DTO；不合并为安装版物理点击或正式桌面二进制验收。M13g10 通知区加载的父宽修复、M13g11 完全隐藏的祖先可见性修复均已纳入 M16v；M13g12 最终 S3 命中 Exact，当前通知边界判据通过独立预期但修正后实际未命中，历史失败保留。鼠标触边、物理键盘 / Narrator、拥挤与其他任务栏方位仍未验，Win11 取消|
|10. Windows 10 / 多屏 / DPI|主窗口位置与外框尺寸适配、同 DPI 工作区检测已进入 M16v 候选；Win10 150% 三进程冷启动 / 合成小屏与缺屏通过，100 / 125 / 150 / 200% 独立几何预期通过|当前只有一个活动显示器；Win10 物理跨屏 / 主屏切换 / 断屏和各档系统 DPI 待实际条件，Win11 不作为门槛|
|11. 系统集成|托盘、单实例、明确退出、主 / 小窗位置、休眠消息路由及恢复键已有实现；M15a8 notify 选择 / 预览 / 确认 / 撤销和原配置保持通过；M16p 标准安装版后台窗口命令 / 单实例 / 正常退出 / 冷启动与账本保持通过；M07r2 真实 S3 / 自动恢复与采集补扫、M13g12 / M13g13 同包原生任务栏宿主两位置各自的真实 S3 链通过|标准已安装应用真实托盘菜单尚未通过；后台命令不替代物理点击，混合电源场景不合并为正式包整体。真实账户状态 / Codex 回合 notify 分别待验；定时唤醒具体来源未证明。不开机启动，不新增额外快捷键|
|12. 软件更新|M16x 公开固定生产源的正式 0.1.0→0.1.2 检查 / 下载 / 版本绑定验签 / 确认 / 正常退出 / 安装 / 自动启动及文件、注册版本、真实库保留通过；新版再次检查为最新。首次 0.1.1 父进程退出竞态的失败与 M16w 修复分别保留|本轮 UI Automation 程序化操作不是物理输入检查；实际库可信消费为 0，不扩张为非零账本保留证明。NSIS 退出码未知。发布顺序已有用户明确授权，无需再询问|
|13. 单渠道安装|M16l 本机普通安装 / 启动 / 单实例 / 小窗 / 退出 / 普通卸载保留数据库通过；M16x 0.1.2 正式线上升级已完成，包含程序、前端、独立宿主及 334 项声明，标准目录保留使用|冷启动真实托盘菜单与自有命令分开；WebView2 缺失实际环境仍待验。已有本地数据保持，不删除来制造干净环境|

M16m 时点的只读复核：标准安装仍为 0.1.0，运行程序与 release 输出仅有预期 NSIS bundle marker 差异，正式应用 1 个 / 自有验收宿主 0 个。SQLite schema v10、quick_check=ok，sources / sessions / usage_events 均为 0；不读取源日志或认证。实际签名验证进程重新退出 0，安装包 / 签名 / 项目公钥摘要与 M16j 一致。WTS 当前会话头为 SessionId=1、ConnectionState=0、SessionFlags=0，即该时点锁定，未发送输入。

下一步仍是标准通知区域托盘、物理输入 / 鼠标触边 / 拥挤 / 多屏 / DPI、账户外部状态、真实 Codex 回合、WebView2 缺失和高版本完整升级；正常 Explorer 独立宿主恢复与底部两位置自动隐藏 / 刷新 / 自有设置消息已由 M16v 同包实际通过，M13g10 / M13g11 的根因修复已纳入；完整应用混合证据与同包宿主摘要一致，两位置任务栏 S3 链已分别由 M13g12 / M13g13 通过并记录其范围。缺少对应条件的检查保持未验；不重做已完成模块，不重复生成密钥或构建无生产变化的包，不恢复取消功能或自动运行性能测试。

## M16h：包含同 DPI 工作区切换检测的安装包

在 M15f4 / M13g2 提交后完整执行 `npm run tauri:build`，TS / Vite、334 项第三方声明、x64 release 独立任务栏宿主、正式桌面应用及 NSIS 均退出 0。包已包含普通移动事件触发的同 DPI 工作区切换检测，以及此前的位置恢复、较小工作区尺寸适配和任务栏 Tab 修复。M13g2 / M13g3 是独立 opt-in 开发验收例程，不编入生产应用；后续 M13g3 未修改生产源码，无需仅因验收例程更新重建同一个包。

当前 `target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe` 为 6,624,629 字节（6.32 MiB），SHA-256 `ed28d76646d0d3b14f03e26d7d3851c590a9bf324c7c08cda22bd9399a06d212`。同次 desktop.exe 为 25,524,736 字节 / `394655b846e397ade6ae3e1ae257bdafcd532f32e8eda82148dcee95646692af`，taskbar-host.exe 为 2,345,984 字节 / `175bb507a4e0d6efba2bdce1c6c800c027283adf13c4678baa6685d0dbf41a10`。第三方声明保持 334 项 / 2,986,262 字节 / `2dad741130f6a68d7fcf87fba744043a1ba838e9d0cf8bfc846630ee006001e0`。本条更新最新产物，先前包大小 / 哈希及其当时的验收证据按历史保留；M15f4 提交时未入包的限制按此次构建收敛。

未覆盖已有正式数据安装，未生成正式密钥、签名或发布资产，不以打包成功替代干净安装 / 卸载或实际完整更新。当前构建环境未提供正式 `TOKENPULSE_UPDATER_PUBLIC_KEY`，生产更新保持未配置状态。Win11 任务栏原生适配尚未实现，物理多屏 / DPI / 输入以及真实账户变化仍分别待验收。2026-10-03 在用户再次允许测试后复测 M13g3，UIA 公共名称检查通过；菜单项实际命中仍为 Windows.UI.Core.CoreWindow，Invoke 前拒绝 / 子进程退出 101，不新增菜单动作通过证据。所有自有验收进程退出，用户原未提交内容保留，未运行性能测试。

## M13g3：受输入归属保护的 UIA 菜单动作验收入口

为 M13g2 追加独立显式 `--native-taskbar-accessibility-actions-development-check`，原只读标志仍只查询属性 / 模式。动作模式只允许本次自有合成宿主的五个命名菜单项，通过 UIA Invoke 后要求正式管道返回唯一对应白名单意图及精确设置修订，再读取必须为空；探针消费意图，不向正式 Tauri 应用转发、不改变用户配置。此动作路径尚未完成环境验收，不能把以下代码存在或模式存在视为菜单可执行通过。

首次直接 Invoke 返回 0x80131509（[UIA_E_INVALIDOPERATION](https://learn.microsoft.com/zh-cn/windows/win32/winauto/uiauto-error-codes)），未产生成功证据。最初只验证自有菜单 PID / 名称，未先检查真实输入命中，保护不足；已修正。参考[官方开源 WindowsMenu 代理](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/UIAutomation/UIAutomationClientSideProviders/MS/Internal/AutomationProxies/WindowsMenu.cs)的 Invoke 会经过 SetFocus / Enter 输入，新探针在调用前还要求可读输入桌面、项可见 / 启用、线程 PMv2 下实际 WindowFromPoint 命中自有 popup、菜单线程 hwndMenuOwner 匹配自有入口以及前台归属；调用后仍检查 HWND / PID。只读属性查询不需要这些输入条件。

本机最终动作模式明确输出 NATIVE_TASKBAR_UIA_INPUT_REFUSED / hit_class=Windows.UI.Core.CoreWindow，Invoke 前拒绝、退出 101；没有绕过遮挡、选择其他系统菜单或用自有消息伪造 Invoke 成功。首次 HRESULT 的具体 OS 根因未据此完全证明，完整可交互桌面下仍需再验证。默认只读模式回归与 strict Clippy / fmt / diff 随提交检查；所有自有宿主及菜单退出，几何按既有 owner / guardian 恢复。生产菜单、管道与 DTO 未改，无真实来源 / 账户 / 性能测试。Narrator、真实键盘、Windows 11 和物理多屏仍保留。

## M13g2：真实 UI Automation 名称与原生菜单模式

新增显式 `check_taskbar_accessibility --native-taskbar-accessibility-development-check`，默认测试 / CI 不嵌入 Explorer；使用正式独立宿主 / 受控管道和合成固定 fixture。查询线程不拥有窗口，以 COM MTA 初始化，IUIAutomation2 连接 / 事务各 1000 ms、AutoSetFocus=false，COM 对象不跨线程，查询按[官方 UIA 线程模型](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading)执行。读数必须唯一嵌入已验证的任务栏，查询前后检查自有 PID / 类，UIA ProcessId 同样匹配；不枚举其他应用名称、不读取真实日志或账户。

实际 Win10 19045 / 150% 查询到原生 Pane（50033）、IsKeyboardFocusable=true / 非 offscreen，Name 包含独立期望的完整 683100、输入未知 —、缓存真实 0、USD 0.565000000000001 和合成范围 / 账户桶。不是复用 accessible_text 格式化器自行证明。通过自有 WM_CONTEXTMENU 展示真实原生菜单，UIA 查询五个有名称 / 启用的 MenuItem，均提供 Invoke 模式；只查询模式存在，不调用 Invoke 或 SetFocus。共享 Privacy ACK 返回时菜单已关闭、真实读数 provider Name 已变为 TokenPulse、原任务栏几何恢复；随后新的隐私快照 Name 不含旧金额 / 范围 / 账户名。明确 shutdown 再次验证整组原几何恢复。

NATIVE_TASKBAR_UIA_PUBLIC_OK / MENU_OK / UIA_OK、退出 0；strict all-targets Clippy / fmt / diff 随提交复核。此模块只增加 opt-in 开发验收例程和文档，未改生产提供器或增加依赖 / IPC，未运行性能测试。实际 Narrator 朗读、Explorer 的物理键盘导航、菜单方向键、CoreWindow 遮挡下的真实输入、Win11 / 物理多屏仍独立验收；UIA 属性和模式存在不被记录为全部辅助技术使用体验完成。

## M15f4：同 DPI 显示器工作区切换检测

补普通移动到同 DPI、较小工作区显示器时没有 ScaleFactorChanged 的入口。运行时保存上次成功适配的 monitor / 工作区坐标 / 尺寸 / 比例，仅在内存中；初始化恢复及 fit_current 成功后更新。普通 Moved / Resized 合并稳定后读取当前区域并比较，变化才重用 M15f3 尺寸 / 外框 / 位置适配。无需强制 DPI 事件；同一工作区的普通移动只保存位置，不在移动消息内同步夹紧。最大化 / 最小化不当作区域适配成功，失败不覆盖最后成功记录，原生 API 调用不持区域 mutex。没有新配置字段、前端几何 IPC、尺寸持久化或主窗口 UI 改动。

显式三进程冷启动场景新增一项：只在 UUID 隔离开发应用暂时停用保存等待旧 worker 收尾，写入合成较大旧屏 / 相同 DPI 的内存记录，将真实自有窗口变大并确认实际外框超过当前区域；再恢复运行并通过真实普通 set_position 产生 Moved，未直接请求 fit=true 或伪造 DPI 消息。生产 worker 检测变化并缩小 / 夹紧当前窗口，实际 DPI 不变、四边可见，成功记录对应真实当前区域；恢复本次普通尺寸 / 位置后，同屏消息保持原几何且 SQLite 保存正确。输出 NATIVE_MAIN_WINDOW_SAME_DPI_TRANSITION_OK，原小工作区、快速最大化 / 最小化、关闭隐藏 / 再开、退出保存及后续两次独立冷启动继续通过，退出 0；1412 提示保留。

desktop strict all-targets Clippy、release check / fmt 和 PowerShell AST / diff 通过。没有修改系统显示器配置或 DPI，没有真实来源 / 账户 / 性能测试；此为合成旧屏输入与真实原生移动事件，不替代物理跨屏 / 断屏 / 主屏切换 / Win11 验收。模块提交时 M16g 安装包尚未包含本次检测，后续构建另记。其他已确认交付范围保持。

## M16g：包含工作区适配与 Tab 修复的安装包

在 M15f3 / M13g1 两个独立提交之后完整执行 npm run tauri:build，通过 TS / Vite、334 项第三方声明、x64 release 任务栏宿主、正式桌面应用及 NSIS。包包含主窗口较小工作区尺寸适配、首次恢复后的原偏移重新夹紧及任务栏只请求自身可用按键的处理。debug 自有窗口 / 合成工作区验收不进入生产能力，没有生产演示 DTO。

最新 `target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe` 为 6,623,768 字节（6.32 MiB），SHA-256 `65a53f0b0181762cdd8ee58cbed6bceb4ea115c8b7756758f687cd4497009c4e`。同次 desktop.exe 为 25,519,616 字节 / `4c58cb1e5335bd4e290653a6ccad9d99f59b0e9bf4ebd2a5512342206bc49ed3`，taskbar-host.exe 为 2,345,984 字节 / `9e604330b2b8751933cfba47f75cdd189c0f00d62bfcc3822fef0a274da97e83`。第三方声明保持 334 项 / 2,986,262 字节 / `2dad741130f6a68d7fcf87fba744043a1ba838e9d0cf8bfc846630ee006001e0`。此前各包的哈希保留历史，此条更新最新产物；M15f3 / M13g1 提交时尚未入包的限制按此次构建收敛。

没有覆盖现有正式数据安装、生成正式密钥或发布 / 上传资产，不以完整打包替代干净安装、卸载或实际升级。正式更新公钥 / 签名、Win11 任务栏适配与实际物理多屏 / DPI / 输入仍需推进。源码核对发现同 DPI 跨到较小工作区不能依赖 DPI 事件，后续补显示器切换检测；不据当前包宣称完整跨屏兼容。最终桌面应用 / 宿主 / 合成账户服务数量为 0，原用户未提交内容保持原状，未运行性能测试。

## M13g1：任务栏按键请求与原生 Tab 导航

修正画布无条件 DLGC_WANTALLKEYS 的处理，避免吞掉系统焦点导航按键。只在入口有可用内容 / 未打开菜单时请求 Enter、Space、菜单键或 Shift+F10；详情可见才请求方向、翻页、首尾、Escape。Tab、其他字符和通用查询返回 0；关闭 / 无数据入口也不请求消息。未修改原有动作、鼠标双击队列、焦点 / 隐私屏障、绘制或管道协议。

两项新增检查及两项既有 canvas 检查通过。纯预期覆盖 Tab / 字符 / 查询放行、Shift+F10、内容关闭及详情显示条件。实际 Win10 19045 / 150% 自有普通窗口和真实画布 / Button，使用 IsDialogMessageW 与 GetFocus 检查 Tab 移出 / 进入、详情随失焦关闭且无动作，以及 Windows 分发 Enter 后仅一次 OpenFloat；没有直接调用窗口过程来代替此 Tab 检查。首次 Enter 断言误认为 IsDialogMessage 返回 0；按实际返回和官方契约修正夹具，已处理消息不二次分发，最终四项通过。strict all-targets Clippy / fmt / PowerShell AST / diff 通过。

正式 `scripts/native-smoke.ps1 -TaskbarActions` 回归通过：隔离无来源数据库、真实 Tauri / 独立宿主 / WebView，详情 / 两次共享隐私屏障、实际自有画布丢失重建、两种嵌入位置 / 按钮增减、菜单、受限小窗 / 统计动作及菜单期间退出恢复全部通过，NATIVE_TASKBAR_ACTIONS_OK、退出 0；1412 提示保留。这仍为自有消息通路，不覆盖下述真实输入限制。完整 release / NSIS 构建后单列产物证据。

按用户许可再次执行真实 wire：输入桌面检查通过，但本次自有前台夹具仍被全屏 Windows.UI.Core.CoreWindow 覆盖，命中保护在 SendInput 和宿主启动前拒绝，退出 101。本次不计真实鼠标 / 键盘完成，不自动操作该系统面板或绕过保护。历史 M13e7 一轮通过保持；Explorer 真实键盘导航、右键 / 方向键菜单、辅助技术、Win11 / 物理多屏 DPI 继续独立验收。无真实来源 / 账户、无性能测试；模块提交时安装包仍为 M16f。

## M15f3：较小工作区中的完整主窗口适配

修正只夹紧左上位置、过大的客户区仍可能落在屏幕外的问题。按实际显示器工作区 / DPI、客户区及标题栏 / 边框差值计算可用客户区，通常保持 960×680 DIP 下限；工作区不足时只在对应轴临时降低下限并缩小当前窗口，工作区变大后恢复通常下限。跨屏启动恢复先移到目标屏，再测量实际外框并适配，最后按原保存偏移重新夹紧，避免第一次使用过大外框夹紧丢失仍可恢复的偏移。最大化 / 最小化排除保持原状，不新增尺寸持久化、几何 IPC 或用户设置。

核心几何以可用物理像素向下取整、外框占用向上取整，避免 125% 等非整数 DPI 下右 / 下边界越出半像素。独立合成预期覆盖 100 / 125 / 150 / 200%、负坐标工作区、客户区临时下限 / 回屏恢复及非法数值 / 外框大于工作区，两项新增与两项既有位置测试通过。Playwright 两项 944×560、1264×649 客户区测试通过：总览左右分区保留、页面不横向溢出、完整导航、任务栏草稿编辑 / 重置、保存按钮和页脚滚动可达。查看深色总览和设置截图，无裁切；没有修改现有正式 CSS / 原型布局，合成 DTO 仅在测试桥中。

实际 Win10 19045 / 150%：三个独立进程冷启动检查继续通过，Seed 在真实自有主窗口调用生产适配代码及合成 960×600 DIP 工作区，检查实际外框 / 四边完整落在工作区，正式 React 导航 / 设置 / 页脚可达，再恢复本次普通尺寸 / 位置并执行原退出保存检查。NATIVE_MAIN_WINDOW_SMALL_WORK_AREA_OK / COLD_SEQUENCE_OK、退出 0；1412 提示保留。此检查没有修改系统 DPI / 工作区，没有读取真实来源或账户，不能替代物理 DPI / 多屏 / 断屏 / Win11 验收。严格 Clippy、TS / Vite 和 release check / fmt 随最终代码复核；未运行性能测试。提交时安装包仍为 M16f，包中尚无本次工作区尺寸适配，后续构建另记。

## M16f：包含主窗口位置恢复的正式安装包

M15f2 提交后完整执行 npm run tauri:build：TS / Vite、334 项第三方声明、x64 release 独立任务栏宿主、桌面应用和 NSIS 均通过。本次包已包含主窗口首次显示前恢复、普通位置合并保存、最大化 / 最小化排除及退出保存；debug 冷启动场景 / 夹具不进入生产能力。没有运行性能测试或再次覆盖已有正式数据安装。

最新 `target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe`：6,619,447 字节，SHA-256 `20491ccdb3dce83219f536125f82ea767bdea62619ddb06e2190eb8d55ff3270`；同次桌面 exe：25,517,568 字节，SHA-256 `9d7f8ae1defcbcf2b817b15710dbf9951ee9c5694242c4db109d0ec764cee520`。第三方声明仍为 334 项 / 2,986,262 字节 / `2dad741130f6a68d7fcf87fba744043a1ba838e9d0cf8bfc846630ee006001e0`。M16e 及其他旧包哈希按历史保留，本记录替代它们的“最新产物”含义。

这次是完整生产打包结果，不能替代新的干净安装 / 卸载或实际完整升级。正式更新公钥 / 签名发布仍未具备，不上传资产；Win11 原生适配仍未实现，实际物理拖动 / 断屏 / DPI / 系统输入保持独立范围。最终应用 / 宿主 / 合成服务进程数量 0，原用户未提交内容保持原状。M15f2 模块提交时“包未包含本功能”的说明按此构建收敛。

## M15f2：主窗口原生捕获与冷启动恢复

主窗口配置改为初始隐藏；正常 setup 在 RuntimeState 就绪后读取独立 main_window 偏好，按已保存 monitor 的当前工作区 / DPI 恢复，再显示。原屏缺失选主屏并夹紧；以真实 outer_size 计算标题栏 / 边框，较小工作区仍保留标题栏可达。主窗口移动 / 调整尺寸记录普通位置，300 ms 合并保存；缩放、WM_DISPLAYCHANGE、SPI_SETWORKAREA 变化调度重新检查，重新打开统计先恢复最小化并校正位置。关闭继续隐藏，在隐藏前和正常退出前保存，失败只报告有限码，不替换坏配置或阻断普通显示。

最大化和最小化不捕获其外框 / 哨兵坐标。原生事件先保留最后普通位置于内存，避免移动后立即最大化丢失尚未提交的位置信息；后台和退出可提交这一普通位置。退出停用调度并递增代次，避免继续创建保存 worker。通过既有 settings_changed 失效通知，未新增 IPC / native handle / renderer 位置写入，范围 / 价格 / 账户 / 小窗偏好分离。

新增显式 `scripts/native-main-window.ps1`，三个完整应用进程共享新的 UUID native-main-placement 开发数据库，参数严格要求 native-smoke、UUID 与 seed / restore / missing 有限阶段。生产读取 / 恢复 / 保存代码原样运行，验收不直接调用启动恢复。第二、三阶段在 initialize 之前只读捕获原始偏好作为独立预期，不能用恢复后重新保存的数据库值给自己证明。首阶段实际原生移动 / 保存、移动立即最大化、最小化 / 恢复、关闭隐藏 / 重新打开通过；首阶段最后把独立预期写入本次自有夹具文件，实际再移动并确认数据库尚未保存，立即退出；第二阶段在 initialize 前的数据库值必须等于该独立预期，随后核对原生位置，证明生产退出保存及独立进程冷启动恢复；再显式写入合成缺失显示器 / 8000 DIP 输入供第三进程验证主屏和实际外框夹紧。合成输入阶段关闭本次保存，仅为保留下一轮已知输入，不当作真实显示器拔插或退出保存证据。

Win10 19045 / 150% 三个 NATIVE_MAIN_WINDOW_COLD_OK / SEQUENCE_OK、退出 0；正式 main DOM / IPC 确认无来源、账户断开。scene 参数一项自动检查、desktop all-targets strict Clippy / release check / fmt、PowerShell AST / diff 通过；来源 / 账户选择器实际回归也退出 0。1412 提示保留。没有物理拖动 / 断屏 / 其他 DPI / Win11 或视觉截图通过声称；本轮不读取真实日志 / 账户，不运行性能测试。模块提交时安装包仍为 M16e，源码 / debug 产物已含本功能，后续打包记录另列。

## M15f1：独立主窗口位置持久契约

核对实际代码后确认原有位置恢复仅覆盖 mini，主窗口尚未实现。新增内部 MainWindowPreferences 与 SQLite settings.main_window 字段，缺省 placement=None；重用已校验的 monitor / 工作区相对 DIP 坐标。读取及写入不接受未知字段、非有限坐标或未来配置版本，不写默认覆盖坏记录。Writer 仅修改最新 payload 的 main_window 并与全局修订原子提交，保留小窗、显示、账户与其他设置；同值无修订变化。没有新增 IPC / renderer 路径或几何能力、数据表或迁移保护工作。

三项独立存储预期通过：未保存 null、保存 / 同值 / 精确修订 / reopen、小窗和显示偏好保持；revision Writer 故障保留原配置；坏字段 / 未来版本读取和修改均拒绝且原 payload 不变。现有工作区坐标两项与小窗存储两项回归、store lib / tests strict Clippy 通过，fmt / diff 随提交复核。仅合成临时库，无真实日志 / 账户 / 性能测试。主窗口原生事件、启动恢复、关闭隐藏及退出捕获继续由 M15f2 实施，当前不能声称原生恢复已完成。

## M16e：选择器收尾后的正式 Windows 安装包

完成 M12m / M15a8 后执行 `npm run tauri:build`（当前进程 PATH 需包含用户 .cargo/bin），真实 beforeBuild 完成 TS / Vite 生产构建、Windows release 独立任务栏宿主准备和第三方声明生成；随后正式 Rust release 与 NSIS 打包退出 0。本包包含 M06g 的真实空态说明；本轮 debug 验收驱动 / 测试场景不编入生产能力，没有新增运行时 Node / Cargo / 当前对话依赖。

最新 `target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe`：6,605,439 字节，SHA-256 `2e8eb45072009cd9ae862afb604a09d07a6e3429ba2dce6169896fbbfe63410b`；同次桌面 exe：25,482,240 字节，SHA-256 `43d34fd3cd2c9d9d7f42bfc4e96da736ad992623092e31526e716ad973f02865`。第三方声明仍为 334 项 / 2,986,262 字节 / `2dad741130f6a68d7fcf87fba744043a1ba838e9d0cf8bfc846630ee006001e0`，前端产物 index-i7C1Clkf.js。下方旧安装包大小 / 哈希属于历史构建，不替换其历史验收证据。

此步骤是当前源码的实际完整打包检查，不是本轮新的干净安装 / 卸载或完整更新通过证明。本机正式目录状态仍存在两项，保持原状，没有绕过 verify-installer 的现有数据保护或清理用户目录。尚未配置正式更新公钥、产生正式签名或上传发布资产；实际完整升级仍需该条件。最终 TokenPulse 应用 / 宿主 / 合成账户 / 输入测试进程数量 0，原用户未提交内容保留，无性能测试。

## 2026-10-03：本轮任务栏真实输入复测

按用户“可以测试”的持续授权再次运行 `cargo run -p token-pulse-taskbar --example check_taskbar_wire -- --native-taskbar-wire-development-check`，使用新构建宿主、自有前台测试窗口和原有输入命中保护。OpenInputDesktop 检查已通过，但实际位置 (180,100,540,240) 命中全屏 Windows.UI.Core.CoreWindow (0,0,2560,1440)，在发送输入 / 启动宿主之前拒绝，退出 101。本轮不计真实 hover / 点击 / 双击 / 键盘通过，历史 M13e7 已通过证据继续保留；没有绕过命中检查或操作覆盖面板，也不反复要求用户解锁。其他可实现模块继续，原有未提交内容保留。

## M15a8：真实 notify 目录选择与确认撤销流程

新增 `native-smoke.ps1 -NotifyDialogs`，独立 native-probe UUID 数据库 / synthetic-notify-dialog-home；真实 React 设置按钮打开正式 Windows 文件夹选择器，经共享有界 PID / 标题 / 控件类别 / ID 驱动执行取消、选择。取消返回 null 与无改动提示；选择仅生成只读预览，默认保留原通知命令；关闭预览不登记或改写文件；第二次选择后明确确认才修改根级 notify。验收未直接操作管理器、注入预览或选择能力，使用正式五命令和实际 WebView。

仅显式 debug 场景在 prepare 回调、配置读取 / 预览生成之前校验 canonical Home，拒绝其他目录及来源启用入口，release 排除限制 / 场景。测试原 notify 使用明确合成的系统 cmd.exe / exit 0 命令，不调用此命令或真实账户回合；预览的原命令和恢复内容在 UI 核对。启用后核对实际配置保留注释 / CRLF / 模型设置与 registry configured / chain_original / current_executable。随后只在合成目录追加 new_key 用户设置，经停用预览再确认，实际文件完整等于原字节加新设置，登记被退休，无残留可见接入。测试不读取 auth.json 或真实 Home，不运行性能测试。

Win10 19045 / 150% 最终三个 NATIVE_NOTIFY_DIALOG_STEP_OK 与 NATIVE_NOTIFY_DIALOGS_OK、退出 0；共享驱动扩展后的账户程序 / Home 四步骤与来源完整流程也各自退出 0。三项 notify Playwright、desktop all-targets strict Clippy / release check / fmt、PowerShell AST / diff 通过，结束应用 / 宿主 / 合成服务数量 0。WebView2 注销 1412 保留。这是实际系统控件和 WebView 的程序化交互，不等同物理鼠标 / 键盘；真实 Codex 回合、Win11 / provider 条件及正式签名更新仍独立验收，未重新打包安装器。

## M12m：真实账户程序Home选择与普通连接流程

新增显式 `native-smoke.ps1 -AccountDialogs`，复用有界自有 Windows Common Item Dialog 驱动，分别核对程序 / Home 的准确标题、应用 PID、可见原生控件类别 / ID、输入值及关闭。实际程序选择器 Edit / ComboBox 使用 1148，文件夹选择器使用 1152；不以 Shell 列表项或泛化 UI Automation 角色代替定位。失败仅记录有限错误码和最多十二个控件类别 / 数字 ID，不输出路径 / 控件文字。驱动保持隐藏、等待 / 回收自有辅助进程，不发送全局输入。

UUID native-probe 隔离库中的 synthetic-account-dialog-home / synthetic-codex.exe 是唯一允许目标；正式选择回调在 inspect / 程序指纹读取及能力签发之前校验 canonical 目标，显式场景也拒绝本地检测。实际 UI 未注入任何选择能力：取消程序选择保持未配置 / 修订；选择程序产生带 SHA 的草稿；取消 Home 选择保留原草稿；选择 Home 保留程序指纹；保存才推进持久配置，保存前后服务未启动。点击连接后实际合成 stdio 服务返回 ready / 75% / 五小时周期并由 React 显示；点击刷新走正式限流逻辑，断开清除旧额度 / 成功时间并保留配置修订。

Win10 19045 / 150% 最终四个 `NATIVE_ACCOUNT_DIALOG_STEP_OK` 及 `NATIVE_ACCOUNT_DIALOGS_OK` 退出 0，原始 fixture-mode 和程序字节一致，测试结束应用 / 宿主 / 合成服务进程数量 0。这里验证实际 Windows 选择器和 WebView 的程序化交互，不等同物理输入，也不替代真实账户 / Win11 条件。刷新检查允许正式限流返回，只证明真实刷新入口和连接保持，不宣称本轮额外网络刷新成功。既有真实账户读取证据仍见账户专题。WebView2 注销 1412 提示继续保留。

初轮文件选择器没有找到 1152；只读观测控件类别 / ID 后改为文件框的 1148。后续断开断言误将保留的空布局容器判为旧额度，改为核对零条窗口与权威 disconnected DTO。失败轮次没有计成功或放宽额度 / 修订 / 未启动服务断言。最终来源选择器完整回归也退出 0，17 Token / 未知 null / 未计价 / 源只读仍一致；四项账户页面 Playwright、desktop all-targets strict Clippy / release check / fmt 通过。PowerShell 解析 / diff 检查随提交复核，无性能测试。notify 选择流程继续独立推进，未重新打包安装器。

## M06g：真实文件夹选择与来源普通流程

新增显式 `native-smoke.ps1 -SourceDialogs`，使用 UUID native-probe 隔离库及自有合成 Home，从实际 React 设置按钮打开正式 `choose_source_directory` 的 Windows Common Item Dialog；取消返回 null，不增加来源或推进配置。选择通过实际原生 Edit / Button 控件消息完成，核对应用 PID、精确对话框标题、控件类别 / ID、输入值及对话框关闭。辅助进程只操作本测试的窗口、不发送全局输入，失败尝试关闭自己的对话框，父端只收有限错误码，并等待 / 收集自己的辅助进程；不依赖真实账户或生产数据。

仅此 debug 验收场景在正式选择回调内核对 canonical 目录等于本次 synthetic-dialog-home，然后才签发原有不透明句柄；误选真实目录不能进入来源登记或采集。没有增加 renderer 路径参数 / 新 IPC / 生产依赖，release 排除场景和门禁。返回存在性 / 匹配标记不输出所选路径或日志内容；首次运行空态的旧开发进度文案改为实际导入、持续更新及独立账户连接说明，主窗口布局保持。

实际 Win10 19045 / 150% DPI：最终完整场景输出 NATIVE_SOURCE_DIALOG_CANCELLED / SELECTED、FIXTURE_MATCH true、NATIVE_SOURCE_DIALOGS_OK，退出 0。经正常后台只读导入合成 17 Token，React 暂停 / 恢复 / 移除保留历史按钮通过；移除后正式总览 DTO 在明确 UTC 日期范围仍返回 17 Token、输入分项 null、未计价 17 Token，直接 SQLite 及源完整字节复核一致。这里是实际系统选择器和 WebView 的程序化交互，不是物理鼠标 / 键盘或其他系统版本的验收。WebView2 注销类 1412 提示继续保留。

初轮驱动错误地按 UI Automation 数字 ID 匹配 Shell 列表项，且当前标准控件投影为 Pane；改为核对实际原生窗口控件。跨进程 Edit 读取改用有界 WM_GETTEXT，符合 [Microsoft GetWindowText 说明](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw)。随后来源数量断言误把账户配置卡片计算在内；数据库已证实一个正确来源后，修正为仅来源列表。失败各轮未计成功，也未放宽目录 / 精确统计 / 源只读断言。

自动：来源页面一项合成 DTO Playwright 回归、TS / Vite 生产构建、desktop all-targets strict Clippy、release check、fmt 通过。辅助脚本解析 / diff 检查随后记录。未运行性能测试，不恢复取消范围。账户 / notify 自身选择器、物理输入 / Explorer / Win11 / 多屏 DPI、正式签名发布与实际完整升级仍分别待验收；最新本地安装包仍是 M16d 当时的构建，本步骤尚未重新打包该空态文案。

最终两份 PowerShell 脚本 AST 解析及 diff 检查通过，探测结束 TokenPulse 应用 / 宿主进程数量 0。原用户未提交文件保持原样，验证脚本和文案 / 文档作为同一普通来源流程模块提交。

## M16d：本地签名发布资产准备

新增 release-only 维护入口，实际桌面 exe 在初始化前核对编译公钥 / 版本、全局签名及可信版本字段，缺失 / 重复 / 版本不符拒绝；create_new 验证记录包含编译 target、公钥摘要、安装包 / 原签名哈希及十进制字节，不包含私钥、认证、来源或本机路径。不启动桌面服务、数据库、网络或安装器，debug 构建没有可用发布验证入口；主窗与 mini 未增加 IPC 权限。已有 notify 维护入口和正常运行顺序保持。

本地 release:prepare 核对三处版本、生产 ID、受支持 Windows target、规范安装文件名、实际编译验签记录和输入字节，说明按后端 UTF-8 字节 / 控制字符约束，发布时间显式 UTC。清单固定本仓库 GitHub v版本资产 URL。输出新目录包含安装包、签名、latest.json 和公开验证记录，旧目录 / 并发新建目录拒绝，失败仅清理身份与父目录已核对的自有临时目录；不上传 / 自动签名 / 生成正式密钥。要求桌面 exe 和安装包来自同次正式构建，不将签名验证当成任意安装包的内部应用身份提取。

自动：Rust 无效材料 / 读取边界检查通过；显式真实 Tauri CLI 临时密钥签名夹具通过，验证正确签名 / 错误公钥 / 改字节 / 不同版本 / 缺版本、精确报告及已存在报告保留。六项 Node 定向检查通过，独立预期核对固定 URL / 版本 / 日期 / target、字节与哈希差异、Unicode 字节 / 控制字符、成功资产原字节、失败 / 桌面变更清理及实际 Windows 并发创建空目标目录不覆盖。脚本工作流使用显式合成 verifier，密码学行为由真实签名 Rust 夹具验证，均不冒充配置正式公钥的生产 exe 成功发布。desktop all-targets strict Clippy 通过；完整构建与实际缺配置入口验证随后记录。未运行性能测试，正式签名发布 / 完整升级仍待发布条件，整体目标继续。

完整 `npm run tauri:build` 成功，最新 0.1.0 本地 NSIS 包 6,605,578 字节，SHA-256 b44ca1fd19aa5c0d782ff905335b15240092d311e9a00a536a75fafdc7de1c62；已包含维护入口、既有宿主 / 前端 / 第三方声明，仍未配置正式公钥或签名。声明仍为 334 项 / 同一 2,986,262 字节及哈希，五项声明生成回归通过；fmt 与 updater 两项边界回归通过，实际 HTTP 签名场景保持显式 ignored，此步没有重复执行。

实际 Win10 19045：新的 release exe 通过精确进程参数启动本维护入口，缺公钥返回 14，不创建报告；正式准备脚本调用该 exe 返回 1，无发布文件或残留暂存目录，NATIVE_RELEASE_CONFIGURATION_REJECT_OK / NATIVE_RELEASE_PREPARATION_REJECT_OK。不执行合成安装器，不启动应用 / 任务栏服务，探测结束 TokenPulse 两类进程数量 0。组合式探测命令一度被工具策略拦截（未给具体原因），改为可审查的分步准备 / 探测 / 非递归清理后完成；没有覆盖正式数据或安装注册。此证据是缺配置拒绝，不是正式签名成功发布或真实升级。

最终代码实际 `-Updates` 回归通过，NATIVE_UPDATES_IPC_OK / 退出 0，真实 main / mini WebView 检查缺配置 / null、严格请求、主窗更新权限、通用插件拒绝及共享隐私，既有正常启动入口未受维护命令影响。WebView2 注销类 1412 提示继续记录；此场景没有下载 / 安装，不替代正式更新验收。

## M16c：第三方声明与安装包资源

新增按锁文件和目标生成的第三方声明，覆盖 Windows cargo resolve.nodes 中的 327 个 registry 依赖（含构建 / 测试，不声称全部进入运行时）、四个 npm 运行依赖、SQLite amalgamation、NSIS 和安装辅助插件，共 334 项。保留完整 LICENSE / NOTICE / COPYRIGHT / AUTHORS 文本，递归包含 ring 内部 once_cell / fiat 等许可，公开源包精确版本下载地址及 SHA-256；不写入本机绝对路径、账户信息或认证。MPL 依赖给出原版本完整源包入口，SQLite 另取实际 amalgamation 的源声明和版本，不把 Rust wrapper 的 MIT 当作 SQLite 本身许可。NSIS COPYING 包含压缩器条款与原有 LZMA 例外：[NSIS 原文](https://nsis.sourceforge.io/License) / [SQLite 原文](https://www.sqlite.org/copyright.html)。

九个发布 crate 未附顶层完整许可时，采用所记录上游 Git 提交的许可文本；selectors 的实际源码声明指向 MPL 2.0，补入 Mozilla 官方文本。已审查文本在 scripts/third-party 保存来源及哈希，升级到未审查版本不沿用旧文本。安装辅助 DLL 的实际哈希对应官方 nsis_tauri_utils-v0.5.3 发布资产，使用该精确提交的原 MIT 文本；不猜测版本或替换作者声明。生成阶段无网络，使用已安装 / 已获取的锁定依赖；缺许可 / 校验失败 / 未审查来源或安装工具版本则停止准备，首次构建不要求事先存在 NSIS 缓存。

prepare-desktop 在正式打包前生成独立资源，NSIS 映射到安装目录 THIRD_PARTY_NOTICES.txt。生成文件保持忽略，跟踪生成器、审查文本和来源；安装验收脚本增加文件存在 / SHA-256 一致和卸载移除检查，保留原有正式数据 / 注册拒绝门禁。没有新增运行时 CLI、外部工具依赖或 UI 布局变化。

自动五项定向检查通过：目标图过滤 / 递归 vendor notice、未知新包缺正文拒绝、固定文本哈希 / 路径约束、npm 运行依赖与安装版本漂移、十份原文快照完整性。实际当前依赖连续生成相同结果：2,986,262 字节，SHA-256 2dad741130f6a68d7fcf87fba744043a1ba838e9d0cf8bfc846630ee006001e0。PowerShell 解析 / diff 检查通过；完整 NSIS 重建另记录。当前已有正式数据 / 产品位置注册，未重跑干净安装脚本，不以静态脚本断言替代实际安装 / 卸载。真正签名发布 / 完整升级与 Windows 条件继续推进，无性能测试。

完整正式 `npm run tauri:build` 通过，最新 0.1.0 本地 NSIS 包 6,603,716 字节（6.30 MiB），SHA-256 fd29f91b1728f4437022fba72d665af6398c45f7f4f77e302dab8d421797e158。核对最终安装指令 File 包含 THIRD_PARTY_NOTICES.txt / 原生宿主，卸载 Delete 对应声明，原 preinstall 等待钩子保留。最新资源覆盖前一步 6.19 MiB 开发包；仍未配置正式签名公钥 / 发布签名，完整安装和真正升级证据未由重建自动获得。许可快照用 Git -text 保留原字节，防止 Windows 换行转换导致校验在克隆后失败；生成器 / 检查不修改用户原有未提交文件。

## M16b4：正式软件更新设置页

设置新增软件更新页签并接四项正式 IPC；无演示数据 / 价格。显示当前 / 可用版本、实际成功时间、发布时刻、精确字节、未知长度和有限错误说明。按阶段限制动作；EOF / verifying 没有安装入口，ready 内联确认绑定显示版本 / 精确修订，状态变化后旧确认禁用，正式安装版才允许确认。版本说明以 React 文字显示，未知不显示最新或零。监听先登记后读取，事件仅失效；BigInt 拒绝低修订、独立查询序号与生命周期拒绝迟到结果，隐私 stamp 变化重读公开元数据，离页释放订阅 / 定时读取。

浏览器四项合成 IPC 检查通过：缺配置 / 验签失败无安装权或假最新、说明不执行 HTML、超安全整数修订和字节 / 0 与 null、旧读取 / 假事件载荷 / 隐私、绑定确认与旧版本禁用、离页清理和重开后拒绝旧响应。相关价格四项 / 任务栏六项共 14 项通过；runtime / 隐私 / 契约 19 项通过、TS 与 Vite 生产构建通过。查看 test-results/updates-dark-review.png 与 updates-light-review.png，960 宽无裁切，原主题 / 文字布局保持；这些图片仅为明确合成的开发测试 DTO。

Windows 10 19045 / 150% 的 `-Updates` 正式两个 WebView 场景追加真实 React 设置按钮：进入更新页、显示真实 unavailable / 编译版本 / null、检查禁用、无进度 / 安装按钮、刷新和共享隐私下公开版本仍显示，实际正式 IPC / 权限场景继续通过，退出 0；1412 保留。这里没有正式公钥，不能替代真实 GitHub 下载 / 安装。完整包重建与真正签名发布仍分别记录，无性能测试。

按用户“可以测试”再次尝试真实 taskbar wire 输入；本次仍在发送输入前检测到 Windows.UI.Core.CoreWindow 覆盖自有焦点窗口（2560×1440），脚本拒绝继续、退出 1。未发送到系统覆盖层，不计通过；该环境条件和 Win11 / 物理多屏 DPI 等仍单列。测试结束未留下应用 / 宿主进程，其余实现继续推进。

最终 `npm run tauri:build` 完整 release / 宿主 / NSIS 重建通过，最新本地包仍为 0.1.0，6,493,332 字节（6.19 MiB）。核对生成安装脚本包含原生宿主和 update-hooks，并在默认进程占用检查前插入等待；正式 release 资源提取确认公共控件 v6 清单，生产构建没有链接重复资源，strict Clippy / fmt / diff 通过。该最新包尚无正式更新公钥 / 发布签名，重建不代替签名发布或完整安装更新测试；M16a 早期 5.22 MiB 产物被本次开发构建替换，早期安装验收证据仅适用于当时包。

## M16b3：NSIS 安装门禁与正常退出收尾

新增 main-only install_update，精确 ready 修订 / 已验签私有对象再次核对；仅非 debug 正式 ID 的 NSIS 安装版可以启动安装。原生将已验证内存写入安全创建的 Temp `.exe`，格式无效或 CreateProcess 失败发布普通错误、保留应用服务，不调用退出；成功以固定 /P /UPDATE /R + 自有父 PID 启动后请求 Tauri 正常退出。复用既有任务栏恢复和后台停机，不使用 updater.install 的直接 std::process::exit。失败删除本次拥有的文件；成功保留公开签名安装器至系统临时目录，不持有认证 / 数据备份。

NSIS overlay 增加 preinstall hook，在默认 Restart Manager 占用检查之前等待父进程退出（30 秒功能期限），无法确认或超时普通中止；没有主动强杀旧应用。实际 Windows 10 `scripts/verify-update-hook.ps1` 使用 NSIS 编译器 / 自有无注册写入的最小夹具，确认进入钩子后父进程尚存时不继续、父进程正常结束后完成，NATIVE_UPDATE_HOOK_OK / 退出 0。该夹具不安装产品、不改任务栏 / 注册 / 数据，不能代替真实完整升级。

自动安装格式 / CreateProcess 失败检查通过；显式真实 HTTP / 签名 owner 夹具追加有效签名非可执行文本后的安装失败 / 不退出、旧修订及失败后无旧安装权，通过。Win10 两真实 WebView `-Updates` 追加主窗安装不可用和 mini 安装拒绝，通过 / 退出 0，1412 保留。strict all-target Clippy、release check、fmt / diff 通过，无性能测试。最终完整 NSIS 包尚需随正式 UI 重建；正式密钥 / GitHub 发布、实际更新替换与重启继续推进，不能宣称真实升级已验收。

## M16b2：真实签名提供方与正式更新 IPC

接入锁定的 Tauri updater 2.12.0，发布清单固定为本仓库 GitHub Releases 的 `latest.json`，安装文件仅接受同仓库 release download 的 HTTPS `.exe`；重定向仅允许 GitHub 发布资产域，限制次数 / 期限，不关闭证书验证、不携带认证。构建时读取公开的 `TOKENPULSE_UPDATER_PUBLIC_KEY`，缺失或无效时不发起请求，保持 unavailable / publication_not_configured。当前尚未配置正式公钥，不能宣称实际发布通道就绪。

原生 UpdateService 独占候选及验签后的不可序列化字节，使用真实提供方进行 SemVer 严格升级比较、minisign 文件签名和签名版本绑定；声明版本不符 / 旧无版本签名均拒绝。下载结束先表达 verifying，只有库实际校验成功且字节数 / 版本匹配才能 ready_to_install。新检查丢弃旧候选，繁忙操作及旧修订拒绝；回调失败不会在真实下载尚未结束时释放操作。失败只返回有限原因，不返回底层原文、密钥、URL 或路径。

注册 main-only `get_update_status` / `check_for_updates` / `download_update` 和空载荷 `updates_changed` 失效通知；请求仅携带 requestId 及下载的 expected_update_revision。mini 无权限，通用 updater 插件没有前端 capability；最新隐私响应 stamp 与其他正式 IPC 一致。安装命令和正式更新 UI 尚未接入，不能将 ready 状态当作安装通过。

自动验证：两项提供方边界 / 有限错误测试通过。显式 ignored 的本地签名验收单独执行并通过：临时夹具密钥、仅自有 127.0.0.1 端口、真实 HTTP 和 Tauri 签名库，分别验证正确文件、篡改字节、声明 / 签名版本不符、无签名版本；进一步通过真实异步 owner 检查 busy、精确 CAS、进度、Ready / Error、204 Current 清空旧候选及非法发布不刷新成功时间。夹具是明确合成的非可执行文本，未启动安装器，未使用真实发布密钥，也不替代正式 HTTPS 发布 / 实际安装验收。

Windows 10 19045 / 150% 实际 `-Updates` 场景使用 UUID 隔离无来源 / 无账户库与两个真实 WebView，验证缺配置 / null、严格非法字段拒绝、旧修订拒绝、主窗三命令、mini 和通用插件拒绝、共享隐私及失败无状态 / 价格修订变化。NATIVE_UPDATES_IPC_OK、退出 0；既有 WebView2 注销 1412 保留。测试程序最初缺公共控件 v6 清单而在入口加载失败；构建脚本为 lib test 生成清单，desktop bin 明确关闭链接器另生成清单，继续使用已有 Tauri resource.lib，避免重复资源。正式 bin 与测试程序均重新构建和实际启动核对，提取正式资源确认公共控件 v6 / 原图标。严格 all-target Clippy、TS、契约前端 11 项、fmt / diff 和 release check 通过。无性能测试。下一步接正式 UI、安装退出收尾及签名发布，不扩展取消范围。

## M16b1：更新状态、精确进度与安装门禁

新增 pure UpdateWorkflow 和五项 DTO，区分网络下载结束、正在验签及真正可安装；未知长度保持 null，字节与修订全程精确十进制。操作令牌不可从前端反序列化，一次仅一个操作；新检查废弃旧候选 / 进度，迟到回调拒绝，安装必须匹配刚发布 ready 修订。元数据限长 / 控制字符拒绝，请求不接受 URL / 公钥 / 原始文件 / verified 标志。缺少发布配置与实际 current 分开，检查失败不刷新先前成功时间。

六项独立领域测试通过：固定 JSON null / 零及缺配置、EOF 无安装权 / 验签失败、超 JS 安全整数进度与独立预期、无副作用的错误长度 / 变长 / 截断 / 空文件拒绝、旧候选 / 迟到结果 / 并发 busy、非法元数据与越界请求。测试用假提供方完成状态动作，不执行签名算法，也不是系统安装验收；提供方、正式 main-only IPC / UI、签名发布及实际更新继续 M16。不会改动消费事实 / 账户或取消范围，没有性能测试。

Rust / TS / schema 同步生成；core all-targets strict Clippy、TypeScript typecheck、既有契约前端 11 项、fmt 与 diff 检查通过。该提交不新增 Tauri 命令，不启动网络 / 安装器。

## M16a：单渠道 Windows 安装包与实际独立启动

正式入口 `npm run tauri:build` 加载独立 bundle overlay；普通 Cargo 验证不要求生成宿主。beforeBuild 编译前端及同目标 / profile 的任务栏宿主，通过 cargo metadata 定位真实 target_directory，复制带 target triple 的 externalBin 并检查 SHA-256。NSIS currentUser 安装主程序、嵌入前端及同目录 `token-pulse-taskbar-host.exe`；只有一个安装渠道，不依赖 IDE、开发服务器或对话工具。中文 / 英文随系统选择，不增加语言选择页。WebView2 已存在时直接使用，缺失时下载 Microsoft bootstrapper 并展示安装交互；配置与生成 NSIS 检测脚本均核对。依据：[Windows installer](https://v2.tauri.app/distribute/windows-installer/) / [externalBin](https://v2.tauri.app/develop/sidecar/)。

构建：TS / Vite、release 宿主和 Tauri 完整代码生成、NSIS helper 下载哈希验证及打包通过。首个产物 `target/release/bundle/nsis/TokenPulse_0.1.0_x64-setup.exe`，5,472,699 字节（约 5.22 MiB）；最终 prepare 脚本重跑及验收脚本解析 / diff 检查通过，无性能测试。

实际 Win10 19045 / 150%：无既有正式安装 / 数据，安装到 UUID 临时目录，HKCU 版本 / 位置和三个安装文件正确。宿主 SHA-256 相同；主程序独立字节比较仅有固定 `__TAURI_BUNDLE_TYPE_VAR_UNK` 到 `NSS` 的三字节差异，其他字节完全相同。Tauri 在打包后恢复未补丁构建输出，故不能直接比较其原始哈希，也不能放宽为任意差异：[官方 bundle 源码](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle.rs)。

安装版创建正式 SQLite，UIA 在自有 PID 主窗找到嵌入前端“总览”；关闭主窗保留进程，第二次启动唤回原窗。可见托盘菜单未弹出，因此显式 OwnTrayCommands 缩减场景核对锁定版本 / 源码顺序后投递自有 HWND 有限命令：实际小窗创建及正常退出通过，后续两个完整冷启动取得真实退出码 0。修正了共享文件读取、托盘初始化等待及进程退出状态句柄保持；初轮失败后分步恢复验证，未宣称最终脚本在干净配置一次完整通过，不冒充真实菜单点击。

正常卸载移除应用 / 宿主及卸载注册，正式数据库 SHA-256 不变，最后 COLD_MINI_EXIT_OK / UNINSTALL_OK / SEQUENCE_OK、退出 0。NSIS 默认保留的产品安装位置注册及新建正式数据仍保留；没有删除 AppData、覆盖开发数据或混入用户未提交内容。可复用入口和干净环境要求见[本地开发](local-development.md#简单-windows-安装包)。

本机已有 WebView2，缺失运行时实际安装、可见菜单 / 物理点击、Win11、签名更新与第三方声明仍待交付。当前安装包未签名。同期重跑物理 taskbar wire 仍被全屏 Windows.UI.Core.CoreWindow 覆盖，发送输入前拒绝并退出 1，不计通过，不阻塞可实现部分。

## 当前交付状态（2026-10-03，M15a7 / M15a6 / M15a5 / M15a4 / M15b4 / M15b3 / M15b2 / M15a3 / M15b1 / M15a2 / M13e7 / M11h / M10d2 / M06f8）

M13f1 已通过当前真实账户到正式独立原生任务栏的第三入口：两个实际应用进程冷启动、真实 DTO 到可见读数 / 详情全文、三入口共享隐私、仅任务栏可见时普通后台新读取及原生成功时间更新、停用原几何恢复、退出拥有的宿主结束，最终 native 场景退出 0。默认 / 真实 / 真实任务栏入口分开，新增 scene 门禁、details 8 / wire 13 / scene 3 及 strict Clippy / release / fmt / diff 通过。过程未保存实际额度 / 身份、未发起登录 / 模型回合 / 性能测试，1412 保留。自有 WM_SETFOCUS 不冒充物理输入，真实通知 / 切换 / 过期 / 重置、Explorer / Win11 / 物理兼容及安装更新继续。详见[账户第三入口验证](account-quota-verification.md)。

M12k 已将额度隐藏补到共享响应序列化：不再只匿名化桶名，隐私下窗口为空、成功 / 尝试时间 null，连接 / 状态 / 精确修订 / 控制保留；owner 真值不变，关闭后重查。新增独立隐私预期、privacy 6 / quota 14 及前端相关 6 项回归通过；core / desktop strict Clippy、release / fmt / diff 通过。无需 schema 变更，原生 quota 移除和 UI 即时隐藏继续保持。真实任务栏账户第三入口正在单独验收，完整范围继续，详见[账户验证](account-quota-verification.md)。

M12h 新增四个独立 Tauri 进程的真实 Win10 冷启动验收：同一隔离 SQLite 配置从保存到自动连接，关闭自动连接及程序指纹变化后的拒绝均通过；账户空值 / 正式 IPC / 拥有的服务退出检查通过，四阶段 COLD_OK、SEQUENCE_OK、退出 0。合成程序 / Home 仅在 debug UUID 隔离目录，无真实认证读取；参数单测、strict Clippy 通过，WebView2 1412 保留。详见[账户共享显示验证](account-quota-verification.md)。真实账户持续读取及其他已确认范围继续推进。

M12i 补齐现有本地账户持续读取：显式观察入口使用生产 owner，首次 ready 及两次后续尝试 / 成功时间前进，同一连接、实际周期 / 百分比提供，断开 / shutdown 后退出 0。没有强制刷新、时钟改动、登录 / 登出或模型回合，输出仅净化存在性标记，不记录真实百分比 / 身份 / 路径 / 认证。领域 17 / 服务 8 项回归、quota 全目标全 feature strict Clippy / fmt / diff 通过，官方 account/read / updated 协议同步核对。真实身份变化 / 通知、过期 / 重置、真实账户冷启动及三入口 / 兼容 / 部署仍分别推进；不记为性能或无限期验收。详见[账户验证](account-quota-verification.md)。

M12j 进一步通过真实账户冷启动：单独 -ExistingAccount 明确启用，两个完整应用进程共享隔离库，首进程保存不连接，第二进程按正常初始化连接现有登录。main / mini 的正式 ready DTO、同一 epoch、成功时间及真实周期 / progress 渲染全部通过，EXISTING_COLD_DISPLAY_OK / SEQUENCE_OK、退出 0；不输出实际数值 / 身份，不直接读取认证，不修改用户 Home。默认合成流程保留；scene 2 项、desktop strict Clippy / release check 通过，1412 保留。真实账户冷启动从待办收敛，物理 UI、通知 / 切换 / 过期 / 重置、第三入口和物理兼容矩阵仍保留。详见[账户验证](account-quota-verification.md)。

本轮按用户“可以测试”复测：正式 `native-smoke.ps1 -TaskbarActions` 的 DETAILS / RECREATE / APPLICATION_POSITION / MENU / ACTIONS 全通过、退出 0。随后真实 SendInput 场景仍发现全屏 Windows.UI.Core.CoreWindow 覆盖自有前台夹具，在发送输入前拒绝，退出 101；不记为真实输入通过，不覆盖 M13e7 的历史成功证据。

M15a7 已将 notify 接入正式“数据来源”设置，来源 / 其他 Home、默认保留原命令、配置差异、确认启用 / 停用、清理重试与隐私 / 草稿释放可用。UI 使用真实 DTO，没有加入产品演示数据。来源旧缓存路径按当前隐私即时隐藏；通知迟到预览释放、Busy 保留可重试、清理失败不掩盖撤销成功。新增 UI 3 / unit 1 项通过，相关 Playwright 共 8 / runtime 共 7、TS / 生产构建及 desktop strict Clippy / release check / fmt / diff 通过。深色 1280 / 浅色 960 实际预览截图已查看；正式 Win10 WebView 按钮启用 / 停用以及 IPC / 采集 3→10→11 通过，退出 0、1412 仍记录。下述 UI 待办按此收敛，系统目录选择交互、真实 Codex 回合 / Win11 / 其他卷限制仍保留，完整应用交付继续，无性能测试。

M15a6 已接 notify 主窗口五项 IPC、真实 DTO / schema / TS、main capability 和 Rust label 检查。已有本地 source_id 或后台系统目录选择器选择 Home，前端没有路径 / 程序 / nonce 输入。共享隐私在序列化时隐藏路径与差异，在文件操作期间阻止已提交隐私后的旧写入；计划绑定设置和显示修订，恢复显示后旧计划仍失效。未知状态与清理失败分别表达。新增 core 3 / app 2 / schema 1 项和相关回归 / TS / 契约 / 三包 strict Clippy / release check / fmt / diff 通过；实际 Win10 main WebView 五命令、mini 权限、隐私 / 旧预览、启用 / 撤销 / active 退休拒绝与采集 3→10→11 通过，退出 0，1412 仍记录。下述 IPC 待办按此收敛；正式设置 UI、实际系统目录选择交互和真实 Codex 回合继续，不改真实 Home、不运行性能测试。

M15a5 已完成配置操作管理器和受控登记退休，原命令默认保留；启用复核 / 暂存配置后登记、提交，失败不发布半份配置。撤销配置成功和后续清理失败分别表达、可重试。私有跨进程锁阻止晚到 marker 与退休交错，已领取对象释放不复活 wake；坏登记隔离、未知配置保持 null。新增 11 项，integration 总 63 / 正式 headless 5 项通过，两包 strict Clippy / release check / fmt / diff 通过。实际 Win10 `-Notify` 管理器启用 → 正式采集 3→10→11 → 保留用户新设置的撤销 → 登记退休 / 监听关闭 / 旧提示无 marker 通过，退出 0；WebView2 1412 仍记录。登记退休 / 操作管理器已收敛，正式 main-only IPC / 共享隐私 / 差异预览 / 设置 UI 继续，TxF 和其他环境限制保持。真实 Home 未改，无性能测试。

用户允许复测后，本轮再次执行任务栏真实输入场景，仍命中全屏 `Windows.UI.Core.CoreWindow`（2560×1440）而非自有前台窗口；发送输入前拒绝，退出 101。本轮不记为通过，不修改已通过 M13e7 的历史证据，也不以通知原生验收替代任务栏输入验收。

M15a4 已接 Windows 配置文件预览计划及真实条件启用 / 撤销，绑定目录 / 文件身份、缺省 vs 空文件、完整字节摘要，保留 BOM / 行尾 / 原 notify 语法 / 其他新设置与原 ACL。现有文件在可选 NTFS 事务内验证及提交，不落地全配置备份 / 临时副本，竞争普通写入 / 替换拒绝，不支持的 provider / 卷返回有限错误，普通采集仍可运行。新增 10 项（9 项实际 Win10 文件 / 事务、1 项有限码）通过，integration 全 52 / 正式 headless 4 项及两包 strict Clippy / release check 通过。正式 `-Notify` 已通过实际文件启用 → 原命令 / owner / SQLite 3→10→11 → 保留用户新设置的撤销 / 监听关闭 / 旧 headless 无 marker，退出 0；WebView2 1412 诊断保留。

当前文件写入 provider 使用 TxF，Microsoft 不建议新应用依赖且未来可能不可用；只按本机 Win10 / NTFS 已验证能力记录，不冒充 ReFS / EFS / Windows 11 通用写入支持。System32 动态加载仅在文件应用时发生，失败保持有限不可用错误，不影响其他应用功能。详见下方实际边界与[配置设计](../design/collection-accounting.md#8-notify-唤醒与恢复路径)。登记退休和正式 main-only IPC / 差异预览 / 设置 UI 仍待交付；此前文件操作尚未实现的历史描述按本增量收敛。真实用户 Home 未修改，没有性能测试，完整交付保持进行中。

M15b4 已接正式 headless 的原 notify 受控执行：只执行不可变登记中明确 chain=true 的原参数，完整通知 JSON 仅作为内存 / OS 的单个尾参数原样转发；不进入采集 DTO、数据库、登记或日志。先核对当前 exe / 配置归属，执行前再次核对，用户已改变 notify 时两条通路均不执行。继承调用者工作目录而不信任 JSON cwd；只解析本地 exe / 有界 PATH，不自动套 shell 或解释批处理。子进程先暂停创建、加入自有非继承 Job，再恢复；等待 5 秒，正常 / 失败 / 超时均关闭 Job 并收集本次子进程，后代不遗留。原程序输出丢弃，错误仅固定码。原命令失败不撤销已经提交的唤醒 / 离线标记，两项结果分别表达。主 owner 不再拒绝 chain=true 的健康登记。

新增原命令合成 / 实际 Win10 检查 5 项，integration 全 42 项、正式 headless exe 4 项及两个包 strict Clippy / release check 通过。正式 `native-smoke.ps1 -Notify` 在隔离合成 Home 中明确保留旧 cmd.exe 的固定 `exit 0` 命令，经真实子进程 / 主 owner / SQLite 的 3→10→11、重复 / 暂停 / 隐藏主窗 / 源只读回归全部通过，退出 0；这不是自动加入 shell 或真实用户通知配置验收。WebView2 注销 1412 提示继续单列。M15b2–b3 中“原命令 runner 未就绪 / chain=true 拒绝”的历史限制由本增量收敛；保真配置文件启用 / 撤销、登记退休、main-only IPC / 设置 UI 仍未交付，真实 Home 未修改，没有性能测试。

本轮按用户允许复测，再运行任务栏 `check_taskbar_wire`：全屏 `Windows.UI.Core.CoreWindow` 覆盖自有前台夹具，输出 FOREGROUND_FIXTURE_REFUSED，测试退出 101，发送输入之前已经拒绝。本次不计通过、不覆盖 M13e7 之前已通过的证据；正式 Tauri 真实输入到窗口、其他任务栏输入与兼容矩阵仍待实际桌面条件。

M15b3 已接正式主进程 notify owner / CollectorService 原子补扫标志与离线 claim 消费，初始化未知侦听数保持 null，错误 / reload / 单 owner / 正常关闭就绪。新增实际 Win10 服务 4 项通过；正式 `native-smoke.ps1 -Notify` 从 headless 子进程经真实主 owner 到 SQLite 的 3→10→11、重复不重计 / 来源暂停、隐藏主窗不激活、源只读 / 配置不变全部通过，退出 0。integration 总 37 项、正式 headless 3 项及 strict Clippy / release check 通过；尚未改真实用户配置，文件启用 / 撤销、原命令 runner 和设置 UI 继续实现。下方 M15b2 的“主进程 / 离线消费待接入”按本增量收敛，完整目标保持进行中。

M15b2 已接正式 exe 在 Tauri 之前的严格 headless 唤醒入口：当前 exe / 当前配置所有权核对、允许字段 / 有界只读配置、在线通道 / 离线零字节标记。新增 integration 4 项、正式 exe Win10 跨进程 3 项通过，integration 合计 33 项 / desktop headless 3 项；两模块 strict Clippy all-targets / fmt / release check 通过。原命令链 runner、正式主进程补扫 / 标记消费、配置文件启用 / 撤销和 UI 继续实施，chain_original=true 明确拒绝而不静默跳过或执行新程序。真实用户配置未修改，完整目标仍进行中。

M15a3 已实现当前用户私有的不可变登记与离线 dirty bit，原 notify 恢复值 / 本地 capability / 显式原命令链选择持久保存，其他配置不复制。领取改名 / 完成 / 未完成释放保证新提示保留；新增合成 2 项、Win10 实际文件 7 项通过，integration 合计 29 项和 strict Clippy / fmt 通过。原子写入、权限拒绝、并发合并、损坏 / 超限与登记上限实际检查通过，没有改真实 Codex Home。正式 headless、采集服务、配置启用 / 撤销和 UI 继续接入，完整目标保持进行中。

M15b1 已增加当前用户专属 Windows notify 管道层，最小帧 / 随机 capability、有限 IO 等待、去重和可取消关闭。新增协议合成 4 项、Win10 实际管道 5 项及独立子进程 1 项通过；连同 M15a2 配置 10 项共 20 项，strict Clippy all-targets / test-fixture / fmt 通过。真实连续连接曾失败，已改为新管道实例与收讫确认，并保留超时确认未知的语义。正式 exe headless / CollectorService、配置文件写入、持久登记 / 离线标记和 UI 尚未贯通，不宣称 notify 整体完成；真实用户配置未修改。

M15a2 已实现 notify 保真编辑与撤销的纯计划层：仅改根级数组，保留其他字节；旧预览配置变化即拒绝，撤销保留其他新设置但拒绝修改 / 删除过的通知命令。恢复记录不保存完整配置并在加载时验证。10 项独立预期检查、integration strict Clippy all-targets / fmt 通过；尚未写真实文件或接 headless / 通道 / 正式 UI。没有改用户 Codex 配置，完整目标继续。

M15a1 已开始 notify 正式实现：新增纯领域允许字段读取器，只保留线程 / 可选回合标识，正文 / cwd / 认证和未知字段跳过，不创建 Token / 费用。5 项独立合成检查及 core strict Clippy all-targets 通过；尚未接 headless / 唤醒通道 / 配置编辑 / UI，继续实施原配置保留与受控撤销。没有改真实 Codex config.toml 或启用用户通知。

M13e7 最新原生输入增量：增加自己真实激活的前台窗口及完整详情夹具，Win10 19045 / 150% DPI 的真实 300 ms 悬停 / 详情刷新不抢焦点、单击仅小窗动作 / 双击仅统计动作、配置 / 隐私屏障与原几何恢复全部通过，退出 0。原脚本对点击也要求维持旧前台，超出设计中显式打开应用窗口的行为；已改为严格验证被动行为，拒绝空前台的 0→0 假通过，正式宿主逻辑保持原样。下述 M11h 的任务栏焦点失败按本节收敛；正式应用的真实输入到窗口动作、右键 / 键盘 / 辅助功能及兼容矩阵仍单独保留。

M11h 最新原生回归：原综合键盘失败时，输入桌面 OpenInputDesktop 返回访问拒绝 5、前台为空，WM_HOTKEY 未到达。增加 debug 输入前置检查与显式独立消息通路；用户解锁后，真实键盘恢复 / 鼠标穿透、透明度、合成账户显示 / 配置、托盘 / 单实例 / 电源路由综合全部通过，NATIVE_SMOKE_OK / 退出 0。下方旧“键盘恢复失败待复核”是历史结果，由本次收敛。新增 taskbar wire 真实单击 / 双击动作通过，但前台变为 0，最终焦点保持失败（退出 101）；单列继续排查。

M10d2 最新 UI 增量：必要文件位置与受限问题摘要已接正式诊断。当前物理代次 / 活动账本、未解决记录、缺失与当前扫描问题按同类 / 位置收敛，一处代表偏移、来源筛选、20 处上限，未知值、精度和共享隐私保持。新增 core 2 / SQLite 5 / 实际 JSONL 1 项、相关 Playwright 10 项通过；Win10 专项正式 WebView 验证从格式问题 / 文件缺失到来源恢复 / 替换发布后的问题消除和 9 Token，退出 0。下述 M10d1 文件级位置待办由本增量完成；此前综合真实键盘恢复失败、其他保留功能与环境验收继续推进。

M10d1 最新 UI 增量：诊断页面已有正式来源状态 / 错误 / 扫描时间及基本检测 / 暂停恢复，重建使用独立单条状态接口，移除完整历史、内部编号 / 阶段及固定账户未连接文案，来源设置移除 WSL 入口。SQLite 作业生命周期 7 项、相关 Playwright 10 项、TS / 构建 / strict Clippy / fmt 通过；深色 1280 / 浅色 960 截图已查看。Win10 正式诊断空态 / IPC 与小窗权限通过；综合原生检查仍在真实键盘恢复失败，不能记为全套通过。文件级受限错误位置和其余功能继续实现。

M06f8 最新增量：正常采集在候选新物理文件移入归档时保留同一逻辑文件，reading / ready 继续进度，claimed 整组作废后重建，failed 只用于识别旧位置；迁移和作业失败同事务，实际目录证明重新核对。新增存储 7 项及 Win10 后台 2 项通过，collector 全部 63 项；原先 4822 的错误夹具现在为独立预期 4816，不叠加旧 6。M06f7 的正常调度和 M06f6 的核验发布保持；后台镜像 / 分叉、真实格式覆盖和其余保留交付继续推进，整体目标仍进行中。

下述 M06f1–f4 / M06e 等“最新增量”及未完成描述记录各提交当时状态；后续 M06f5–f8 已完成的所有权、输入登记、替换核验发布和正常调度不再列作当前缺失。

M06f4 最新增量：NULL 活跃指针的暂存会话在选择器、详情 / 轮次 / 上下文、父子关系与固定范围保持不可见，普通依赖组与自动证明排除该身份；已发布空会话继续可用。3 项跨入口检查、store lib 229 项普通检查 / 1 性能夹具 ignored、采集定向 25 项及 strict Clippy / fmt 通过。替换作业所有权、必要观察有界登记、显式候选输入 / 新身份核验、文件 / 账本原子切换及正常调度仍待完成，完整交付继续进行。

M06f3 最新增量：普通重建按当前文件指针选择输入，回放、规范规划、完整分类验证与镜像发布统一限定于冻结清单。退役 / 无效 / 未发布候选与未选定 current 代次不会混入核算或被发布改写；暂停 / 缺失来源仍保留当前已保存历史。重建 11 项、collector 46 项功能检查通过。封存替换代次的显式加入、不同身份及整个依赖组核验、文件 / 账本原子发布与正常采集接入仍待实现。

M06f2 最新增量：独立只读 `read_replacement_file` 已接候选暂存 API，实际临时文件验证缩短 / 物理替换 / 同大小改写、跨批次重开库继续读取、未结束末行 / 超长行、封存后追加及再次改写。旧消费与检查点保持，未注册新会话或混入活跃观察。候选读取 9 项、存储 11 项和 strict Clippy / fmt 通过。此接口尚未接正常调度或账本发布；文件代次变化后的完整统计仍待重建输入选择及原子切换。

M06f1 最新增量：schema v8 已落地替换文件候选的隔离暂存区、旧输入冻结、读取进度 CAS、结构 EOF 封存和失败撤销。候选读取不注册新会话、不进入活跃观察 / 消费 / 基线，不切换当前文件或活跃账本。9 项候选存储测试及原事务 / 扫描回归通过。正常采集尚未调用该 API，新代次读取、账本输入选择及验证发布继续实施；不能把暂存层视为文件改写处理完成。

最新采集增量 M06e1 / e2：schema v7 当前扫描证据已接有界目录枚举、逐文件读取确认、启动 / 退出 / 休眠 / 唤醒、监听提示 / 溢出及周期核对。查询在真实 SQLite 快照重新验证当前根目录、代次、检查点及 EOF；符合条件的来源可显示完整，缺失证据仍为未知，已知文件 / 格式 / 核算缺口仍为部分。实际文件代次变化的候选重建仍待实施。

- 独立 Tauri / React 工程、SQLite / 原子采集事务、持久化作业、只读适配、镜像与分叉核算、候选重建及已知旧核算版本升级已接入；当前目录扫描证据及快照覆盖判断已落地，实际 Codex 格式覆盖及文件代次变化后的正确统计仍有待办；旧 parser 自动重解析与 WSL / 网络来源适配已取消，不能宣称采集已经完整覆盖。
- 总览、模型、项目、会话、明细五个统计页面使用正式 DTO。会话 / 明细稳定分页使用真实 SQLite 租约；范围消费与最近上下文分开。会话详情的消费 / 关系 / 账本分类摘要使用原子 bundle，子关系支持跳转；可靠回合分页已接入详情；已保存时区、自定义日期和热力图日期跳转已接统一筛选；来源状态、错误原因、必要定位与基本重建进度的简化诊断及设置仍需收尾；完整任务历史、原文采样与继承证据浏览器不再实施。
- 自定义价格不可变发布 / 版本快照、精确分币种估算、覆盖 / 未计价、成功变更通知与完整快照刷新已实现；指定估价时点已接统一筛选、详情和回合查询。模型别名版本化写入、main-only IPC 和正式编辑界面已贯通。已内置官方 51 模型 / 172 条档位事实目录、正常启动原子发布和正式目录浏览；37 条可核算 Standard 单价接自动估算，条件不足的其他档位保持未计价。持久事件费用缓存、独立重估线程、启动 / 用量 / 改价自动补建及正式主窗口进度 / 取消 / 指定时点入口已贯通。任务固定价格修订、缓存未命中时按真实查询快照即时补算，Token 不因重估变化。日志未提供的请求档位 / 缓存写入维度继续保持未知。
- Win10 独立开发应用已验证实际 WebView 查询 / 价格写入、五个统计页面空态与部分控件、托盘注册 / 关闭隐藏 / 单实例激活 / 电源消息路由。主窗口持久隐私、深 / 浅 / 系统主题与最新 IPC / 前端缓存门禁已实现；持久小窗独立范围 / 原子用量、实际两尺寸 / 置顶 / 隐藏恢复及跨 WebView 主题 / 隐私通过；精确同范围统计跳转、已登记会话选择及 UTC 毫秒 / 每日零点起点实测通过，主窗口详情可固定会话。小窗位置 / 展开 / 置顶已持久化，实际 WebView 重建 / 缺失屏幕标识回退通过。主窗口位置、冷启动 / 真实断屏 / 跨屏 DPI 完整验收、本地账户真实持续刷新、任务栏、notify、简单打包 / 自动更新与 Win11 未完成；开机启动和额外全局快捷键已取消。
- 默认恢复快捷键 Ctrl+Alt+Shift+T 与用户修改已接原生注册、持久 CAS、设置页实际状态；Win10 真实键盘输入恢复小窗 / 解除鼠标忽略、外部冲突和 Writer 故障保留旧注册通过。小窗 70%–100% 原生透明度已接持久 CAS / 设置页，Win10 实际 alpha、置顶样式保持、Writer 回滚、恢复交互与 WebView 重建通过；正式穿透入口已接明确确认 / 后端实际恢复键门禁 / 独立持久状态；真实鼠标穿透和快捷键解除、启用写入故障撤销、恢复保存失败仍可交互通过。额外全局快捷键与开机启动已取消，保留已有交互恢复键。
- 此前已完成基线（最新增量验证见 M13e2b）：core 99 项、quota 23 项（6 协议 / framing / 指纹 / 检测、8 持续服务、9 管道）通过；store lib 最近基线为 159 项普通检查、1 项性能夹具 ignored。32 项 Vitest、65 项 Playwright、契约 / 类型 / 生产构建及 Clippy / fmt 通过。Win10 隔离库的新连接配置 / 实际 WebView / 合成账户 exe 及已有原生回归退出 0。浏览器合成 DTO、真实 SQLite 合成夹具与 Win10 系统检查分别记录；未运行性能测试，仍留待全部功能完成后由用户决定。

任务栏已实现实际 Win10 嵌入、正式配置 / 状态、后台生命周期和按偏好的非激活小窗回退；单击展开小窗、双击同范围统计、五项原生菜单及 340 DIP 只读详情已接正式通路。M13e5a 补齐已销毁画布代次的重建，M13e6 接通应用图标右侧位置及实际按钮增减后的安全重排，Win10 19045 / 150% DPI 两位置切换和相关窗口 / 回退回归通过。300 ms 悬停已通过原生 API 接入，但实际鼠标整场 / 键盘可达性 / 焦点保持 / 屏幕阅读器、完整 Explorer 退出 / 重启、实际拥挤 / 自动隐藏、Win11 与物理多屏 DPI 仍未完成。整体交付目标保持进行中。

2026-10-02 用户取消 M14：导出、用户手动备份、备份恢复、跨设备离线恢复与应用内数据清除不再作为待办或发布验收项。F10 / A13 停用，后续模块编号保留；迁移保护、专门故障恢复、升级强杀和灾难恢复专项开发 / 验收也已取消；既有内部迁移 / 备份、事务和检查点代码及已完成测试保持现状。基本采集正确性、重建和源日志只读仍有效。

2026-10-02 剩余功能 1–14 已由用户确认，详见[实施计划第 7 节](implementation-plan.md#7-已确认的剩余功能范围2026-10-02)。notify、自动更新、费用缓存与重估、模型别名与完整离线价格、Windows 11 / 多屏 / DPI、任务栏、额度展示与真实日志统计继续交付；诊断简化、打包做简单版。旧格式自动重解析、额外快捷键、开机启动、macOS / WSL / 网络来源适配取消。历史待办按该确认范围收敛，已完成测试记录保留。

下方按模块记录实现和当时的验证，早期“待实现”说明以本节及相应后续模块为准；完整交付尚未完成。历史 M14 / 导出副本及迁移保护 / 专门故障恢复待办已由上述范围修订撤销，既有实现和测试记录保留。

账户服务已注册主窗口专用原生程序 / Home 选择、草稿释放、配置保存、连接 / 断开和额度桶选择命令，设置页接入实际 DTO、显示隐私门禁与精确修订。默认不连接，已保存的明确 auto_connect 偏好接启动钩子；程序变化拒绝，配置保存不替换当前连接。用户已确认复用本地已登录账户，新增登录 / 设备码 / 取消登录不在交付范围。本机已通过选定程序和已有 Home 读到真实 ready / 周额度，未发起新登录。主总览与小窗完整额度内容、真实账户持续刷新 / 身份变化、完整冷进程自动连接及 OS 文件选择对话框交互仍待后续；不以单次读取证明全部账户验收。

## M15a7：正式 notify 设置与差异确认

新增 `NotifySettingsPanel` 放在既有设置“数据来源”账户配置后，不改变主导航 / 顶部统一筛选 / 总览布局。来源选择复用当前 SourcesSnapshot，仅现存本地来源；“选择其他 Home 并预览”调用后台系统选择器。默认保留并调用原命令，明确替换时恢复值仍保存。真实 owner / 登记状态与 unknown / issue 显示，既有文件未处于本次配置归属时不开放停用覆盖；已停用记录可重试清理。根级 notify 前后值在内联确认区展示，只确认按钮应用后台计划；停用预览恢复内容，同样明确确认。前后值宽屏并列、960 上下排列，长字符串换行且限高滚动，按钮 / 字体 / 主题沿用原样。

预览关闭 / 离页 / 隐私 epoch 变化释放内存计划；迟到 prepare 被标准响应门禁丢弃时也主动释放 capability。状态读取与动作分开使用序号 / 生命周期 / epoch，旧数据不能恢复路径 / 差异；私有状态不开放配置操作，原来源卡片同时按当前策略立即隐藏旧缓存路径。Busy 保留原预览重试；过期 / 内容变化要求重新预览。停用成功但登记被占用明确提示“通知已停用；接入记录待清理”，可单独重试，不把它报成停用失败。有限错误解释包含占用、变化、归属、权限与安全写入不可用；未知错误不回显诊断 / 命令。UI 倒计时只用于提示，后台期限 / 文件与修订条件仍最终验证。

新增 Playwright 3 项：默认保留 / 预览前不应用 / 启用与停用都确认恢复内容，Busy 保留预览与清理失败独立重试，隐私时迟到预览主动释放 / 恢复显示不复活 / 来源路径即时隐藏；新增 runtime 1 项检查有限错误说明 / 不回显未知内容。相关通知 / 来源 / 账户共 8 项、runtime / privacy 共 7 项、TS / 生产构建与 desktop strict Clippy all-targets / release check / fmt / diff 通过。最初来源旧断言匹配卡片和新增 select option 两个相同路径，按验证目的收敛为来源卡片，未改变产品来源行为。深色 1280 / 浅色 960 预览截图已实际查看，无横向溢出，合成命令只在测试 bridge。实际系统选择器取消 / 选择尚未交互验收，截图不计为 native picker 验收。

正式 Win10 19045 `native-smoke.ps1 -Notify` 从实际 main WebView DOM 点击“设置” → 选择已登记合成来源 → 预览 / 确认启用 → 已启用真实登记 → 预览 / 确认停用 → 原通知恢复 / 无登记。最终源日志与无关用户新设置仍保持，原五 IPC / 共享隐私 / mini 权限及采集 3→10→11 回归继续通过。NATIVE_NOTIFY_IPC_OK（settings UI clicks）/ NATIVE_NOTIFY_COLLECTOR_OK、退出 0；已知 WebView2 1412 单列。该 native 场景使用 WebView DOM 点击，不能冒充真实 SendInput / 键盘或用户真实 Codex 通知。未修改真实 Home，无性能测试。真实 Codex 回合、系统目录选择器实际交互、Windows 11 / 其他卷及整体其他保留功能仍继续实施 / 验证。

## M15a6：主窗口 notify IPC与共享隐私门禁

已新增 `get_notify_integrations`、`prepare_notify_integration`、`apply_notify_integration`、`release_notify_preview`、`retire_notify_integration` 的 AppManifest / 自动权限 / main capability / handler。mini 没有 capability，后端统一校验 main 标签与请求标识。只读准备从现存未移除本地来源获取 Home，或后台父窗口系统目录选择器；前端只提交来源或登记标识与可选原命令链选择，不接受路径 / 程序 / nonce / 配置文本 / 完整通知。系统选择等待不持有隐私 / 管理器锁，取消返回 null，返回前重新检查隐私修订。

后台绑定 plan_id 与数据库设置 / 显示策略修订，配置真实文件条件检查仍保留；设置变化使计划 StaleConfirmation 并释放。过期 / 未知计划和 release 走既有有界管理器，修订映射按实际有效计划清理。准备、应用与退休在最新共享隐私锁内执行，已提交隐私时拒绝动作；release 在隐藏状态也允许关闭草稿。PrivateResponse 序列化时再按最新策略隐藏 Home 与 before / after notify，防止慢响应带出旧路径；nonce、全配置与正文不进入 DTO。

状态枚举失败 registrations=null / registry_issue，空列表与其区分；单条坏登记 / 配置 configured=null，不制造零或 false。监听数保持真实 nullable owner 快照，不宣称 apply 返回时监听已就绪。结果 configured=true / false 表示真实配置变更，retired / cleanup_issue 单独表达；对已不存在登记的单独退休 configured=null，不能据没有记录推断用户配置。错误仅 NOTIFY_INTEGRATION_FAILED + 有限 details.notify_issue，Busy / provider 不支持 / 文件变化 / 归属 / active / 清理等不同原因没有 OS / parser 文本。成功应用和退休 reload owner。TxF 限制继续保留，不将本机支持推断到其他环境。

新增 core 3 项验证严格输入拒绝任意路径 / 命令 / nonce、无效标识、延迟隐私差异与路径脱敏 / 精确修订 / null 保留、隐藏时操作不执行；app 2 项验证主标签与请求标识 / 有限错误与 retryable；schema 1 项验证 null 枚举 / 状态、有限 issue、严格输入和成功撤销但清理失败结果。core 既有隐私 5、manager 6、schema 总 11 项通过；TS / generated 契约漂移、core / integration / desktop strict Clippy all-targets（test-fixture）、desktop release check / fmt / diff 通过。

实际 Win10 19045 `native-smoke.ps1 -Notify` 在合成 Home 继续原采集 3→10→11 验收，并从真实 main WebView 调用五项命令：来源 / 额外路径拒绝、只读前后值 / 默认原命令、release 后不可 apply、隐私路径隐藏 / 禁止准备与写入、恢复显示旧计划失效、条件启用 / active 退休拒绝、保真撤销 / 登记退休、已不存在登记配置 null。mini 真实 WebView 对五命令均被 capability 拒绝；最终源日志字节与用户新设置仍保持。NATIVE_NOTIFY_IPC_OK / NATIVE_NOTIFY_COLLECTOR_OK、退出 0；已知 WebView2 1412 单列。无真实用户 Home 修改或性能测试。正式设置 UI / 视觉检查、系统目录选择器实际交互、真实 Codex 通知及 Windows 11 / 其他卷继续验收，整体目标仍进行中。

## M15a5：配置操作管理器与登记退休

Windows `NotifyManager` 持有实际 exe、私有 registry 与最多 8 个 120 秒内存计划。启用 / 撤销准备只读，不写配置或登记；返回 notify-only 前后值、是否创建配置及原命令链选择，不暴露 nonce 或完整配置。应用只接受不透明计划标识；过期拒绝并释放，Busy / 过期内容失败可重新检查，成功消费。None 链选择默认沿用已有原程序，明确 false 才替换，原值仍用于撤销。状态独立读取当前配置，缺省为已知 inactive，损坏 / 不可读保持 null，坏记录不阻止健康 Home；状态查询不启动 KTM 或创建缺省文件。

启用先清理同 Home 的已知 inactive 旧登记、stage 配置事务，再持久登记、commit 配置。登记失败释放未提交事务；commit 失败尝试退休刚写登记，无法安全清理时保留记录与有限组合错误。撤销先保真还原最新配置，再退休；清理被其他进程占用时返回 configured=false 与单独 cleanup_error，后续 retire_inactive 重试，不能将撤销成功报成未发生。

退休与创建 / 标记 / claim / 完成使用同一当前用户私有跨进程锁，最多等待 250 ms。保持实际配置只读句柄；缺省配置通过不提交事务保留名称，释放不创建文件。当前 notify 仍归属该登记时拒绝退休。预验证记录内容 / capability、私有 ACL / 种类 / 单链接、全部零字节 wake 与合法 claim，再通过已打开的 DELETE 句柄删除；部分 IO 失败保留登记以便重试。已退休 claim 完成幂等、释放不重建标记。旧 headless 缓存回调遇到不存在登记忽略，不启动 GUI、调用原程序或创建孤儿 wake。此处仅清理自己通知元数据，不新增 M14 或数据库恢复。

新增 11 项自动功能验证：管理器 unit 1 项检查过期 / 容量 / 释放无文件副作用；外部 6 项覆盖默认保留 / 只读预览 / 一次应用 / 最新新设置撤销、Busy / stale 不分配记录及同计划重试、登记失败配置回滚、撤销成功但退休占用后重试、18 次普通启用撤销不耗尽 16 条登记槽位、坏配置 null 与坏记录隔离。退休 4 项覆盖 active / foreign 拒绝、claim 与新 wake 清理 / Drop 不复活、非零 marker 预验证不删除 / 缺省配置保持缺省、8 个 marker writer 与退休交错不留下 orphan。这是功能边界验证，没有运行性能测试。全 integration 63 项与正式 headless 5 项通过；后者新增已退休旧通知的退出 0 / 无输出 / 无 GUI / 无 marker 检查。两包 all-targets strict Clippy、release check、fmt / diff 通过。

实际 Win10 19045 原生应用场景 `native-smoke.ps1 -Notify` 已改用管理器完成只读启用预览 / 默认保留合成原 cmd.exe → stage / 登记 / commit → headless / owner / CollectorService / SQLite 3→10→11 / 重复 / 暂停恢复 / 源只读 / 主窗隐藏 → 用户新设置保真撤销 / 登记退休 → reload 监听 0 / 旧 headless 忽略。NATIVE_NOTIFY_COLLECTOR_OK、退出 0，已知 WebView2 unregister 1412 提示保留。使用隔离 AppData 与合成 Home，未改变用户真实配置；正式设置 IPC / UI、真实 Codex 回合和 Windows 11 / 其他卷能力仍待接入或验证。

## M15a4：配置文件的条件启用与保真撤销

`ConfigFilePlan` 保留非序列化 / 无 Debug 的编辑内存、根目录物理身份、文件物理身份或确切缺省状态、原字节摘要、恢复记录及 notify-only 前后值。prepare 是只读操作；apply 重新打开并保持根目录句柄，检查同一 Home / 文件、实际种类与完整摘要。文件被换成相同字节也过期；缺省文件被用户创建为空文件也过期。restore 从最新配置仅在 notify 仍属于本登记时准备，其他用户新设置保留；如果准备后再改动，必须重新预览，不能依据旧结果覆盖。

实际文件写入采用动态加载的 Windows NTFS 事务：预览后所有复核与写入 / 截断 / 同步在同一事务发生，关闭文件句柄后仍保留事务预留，保持 Home 到 commit。成功一次性发布，正常放弃或失败释放不发布中间字节；普通外部原位写入 / 替换在保留期间被拒绝，不存在“最终检查后关闭句柄再无条件替换”的覆盖窗口。原文件 owner / group / DACL、文件身份及其他配置保留，缺省 config 只在 CREATE_NEW 成功后提交。不创建全配置备份 / 临时文件，不访问 auth.json 或源日志；撤销删除自己的根键后保留 config 文件和用户设置，不删除用户 Home。

技术限制：provider 仅在需要写配置时从 System32 加载 KTM 并动态查系统入口；KTM / API / 文件系统或 EFS 不支持时有限 Unsupported，Busy / 权限 / 过期 / 归属错误分别表达，没有不安全写入降级。[Microsoft 的 TxF 文档](https://learn.microsoft.com/en-us/windows/win32/fileio/transactional-ntfs-portal)明确建议替代方案并说明未来可能不可用，因此本模块的实际证明限定 Win10 19045 的当前 NTFS；ReFS / 不支持卷、EFS 和 Windows 11 能力不能从本轮推断。配置事务 timeout 设置 5 秒，未把它当所有底层 I/O 的硬实时期限。其他独立采集 / 统计仍可用，此限制需在后续正式 UI 中明确显示。

定向自动检查新增 10 项：6 项库检查中的 5 项实际事务，覆盖多扇区新数据仍只对外显示旧字节、外部原位写入与 MoveFileEx 替换拒绝、普通 Drop 保留原文、缺省创建事务中不可见 / 名称保留 / 释放无文件、双事务 Busy 与正常 retry、根目录保持至提交与 Home 同内容替换过期；另 1 项有限错误码，不视为实际不支持卷验收。4 项外部 API 检查覆盖多行原 notify / BOM / CRLF / 中文 / 嵌套 profile 不变、owner / group / DACL 对照、重复 apply 过期、用户其他编辑后的保真撤销、缺省 vs 空 / 相同字节换文件、notify 归属改变拒绝、硬链接 / 只读 / 目录 / 1 MiB 上限 / 占用 writer / UNC。首轮目录对象打开返回权限错误，改为句柄打开并先验证真实种类，再读正文；定向与全套均通过。integration 合计 52 项、正式 headless 4 项及两包 all-targets strict Clippy / fmt / release check 通过，无性能测试。

实际应用：本机 Win10 19045，`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify` 使用隔离 debug AppData / 合成 Home。在 fixture 原配置 / 显式保留原命令上准备实际文件计划，先写私有恢复登记再 apply；经正式 headless 子进程 / 主 owner / CollectorService / SQLite，仍完成 3→10→11、重复不重计 / 来源暂停 / 原日志只读 / 隐藏主窗保持。其后模拟用户增加无关新设置，实际 restore 保留该设置与原 notify 字节，reload 后实际监听数 0；调用旧 headless 被忽略，没有新 marker。NATIVE_NOTIFY_COLLECTOR_OK、退出 0。WebView2 unregister 1412 继续保留。这是实际配置写入器的开发夹具证明，尚非用户 Codex 回合或正式配置设置 UI；真实 Home 未修改。

下一个模块是登记退休及配置操作管理器，再接主窗口 IPC / 差异预览 / 设置 UI；本轮不增加 M14 / 数据故障恢复等取消功能，不据此标记完整 notify 或整体交付完成。

## M15b4：受控保留原 notify 与独立唤醒结果

正式 headless 分支调用统一 dispatch，在只读当前配置证明完整安装参数归属后发送最小 wake；有明确 chain_original=true 才读取恢复记录中的原参数并执行。执行前重新读取配置，期间已改动则不调用旧程序。未知事件仍忽略，原载荷必须重新通过有界领域读取且身份与 invocation 相同；raw JSON 不加到 NotifyInvocation 或其他可持久类型，错误不回显原命令 / 正文 / 路径。原程序沿用其既有完整单参数通知内容，不把正文提取成 TokenPulse 数据；不选择 JSON 的 cwd / 程序或任何其他命令字段。

程序允许本地绝对 exe、调用进程目录下相对路径或最多 128 个本地绝对 PATH 目录的 exe 查找。裸名称只补 `.exe`，不使用 PATHEXT 隐式 cmd / bat / 脚本解释；原配置若就是明确保留的 cmd.exe / 解释器，则按其原数组调用。无法执行时返回有限不可用错误，不临时换用别的包装命令。规范路径大小写与 Win32 卷 / 文件身份检查拒绝当前 dispatcher 及其硬链接别名；程序文件只读句柄阻止普通写入 / 替换直到执行结束。继承实际调用者工作目录 / 环境，不从 payload 修改 Home 或 cwd。

原进程 CREATE_SUSPENDED / CREATE_NO_WINDOW、null 三个标准流，在运行任何原代码前设置唯一非继承 Job 的 KILL_ON_JOB_CLOSE 并加入，再仅恢复本次 PID 唯一主线程。功能等待 5 秒，退出 / 错误 / 超时均关闭自有 Job，停止与收集子进程有界；这属于通知进程生命周期，不新增已取消的灾难恢复。成功、程序不存在、启动失败、非零退出与超时均有限表达。wake 和原程序结果独立，先提交的在线提示 / 零字节标记不因后续原程序失败而撤销；本地唤醒失败但配置归属仍明确时，也不静默吞掉已经选择保留的旧程序。

自动检查新增 5 项：feature-only 独立 exe 接收含空格 / 引号 / 中文的原参数和确切一个完整合成 JSON、131072 字节输出被丢弃且 wake DTO 只有允许标识；程序失败保留离线标记与 chain=false 从不执行；用户修改配置 / 不同 payload 身份 / 当前 exe 大小写与同盘硬链接 / 隐式 cmd 文件拒绝；5 秒超时结束本次实际子进程树并保留 marker；相对程序基于继承目录、chain=true 登记得到真实监听。首轮相对路径检查错误假定 Cargo 测试 cwd 为仓库根，改为从实际工作目录构造相对路径；硬链接夹具最初位于另一磁盘，改为同盘临时目录。两者是夹具错误，修正后全 integration 42 项通过。新增 fixture 只供 test-fixture，不是生产安装资源，不保存通知正文。

正式 exe 检查从 3 项扩展为 4 项：在线最小通道 / 离线合并继续通过，明确原程序不存在时有限错误但 marker 保留，显式合成旧 cmd.exe / 固定 exit 0 成功同时补扫，不产生 SQLite / Tauri 初始化。actual `native-smoke.ps1 -Notify` 使用正式主程序 / headless / owner / CollectorService / SQLite，watcher 关闭及一小时轮询，明确保留合成旧程序后仍完成 3→10→11 / 重复不计 / 暂停恢复 / 主窗隐藏 / 原日志只读 / 配置不变，NATIVE_NOTIFY_COLLECTOR_OK、退出 0。本机 Win10 19045；原有 WebView2 unregister 1412 提示保留，尚未发布环境复核。两个包 all-targets Clippy warnings denied、fmt / diff、desktop release check 通过。没有真实用户 Codex 回合 / 配置启用操作、UI 视觉或性能测试。

后续继续保真文件应用 / 撤销与登记退休，再接 main-only IPC / 差异预览及正式设置；此前各阶段的 runner 不可用描述按本增量收敛，不据此宣称完整 notify 或全应用交付。

## M15b3：主进程 notify owner、离线消费与实际采集贯通

正式 setup 在采集器可用时建立 notify 服务，后台唯一 owner 管理配置归属与私有管道；退出在采集器之前停止该 owner / listeners。入口 worker 仅合并标志，真正 callback 只调用 `CollectorService::reconcile` 的原子请求，没有直接读日志、写 Token 或费用。监听数初始 None，枚举后返回实际有界数；失败保留有限原因，不用零冒充未知。来源暂停仍由原采集器生效。

有界登记 ID 枚举不读整份恢复记录，逐个读取 / 配置核对，一份损坏记录可单独失效，其他健康来源仍侦听。新登记 / 显式 reload 更新实例，失败有限重试；配置不可读时离线 claim 恢复，不在错误下确认丢弃。普通 owner 启动恢复先前遗留的零字节 claim 并与新 `.wake` 合并，headless 不执行恢复；独立 `.service.lock` 的分享禁止保证不同时领取。登记 Home / 应用目录拒绝 UNC，只保留 Windows 本地磁盘路径。

新增自动服务检查 4 项（真实 Win10 管道 / 文件 / 线程）：启动 marker 与 online / 重复 hint、reload / 正常关闭重开、第二 owner 失败的 null 计数、遗留 claim + 新 marker 合并、配置锁定保留 / 解锁补扫、失效配置清除旧提示、坏记录隔离与修复 / 新登记重新加载。integration 总 37 项与正式 headless 3 项通过；两个包 strict Clippy all-targets / fmt / diff 和 release check 通过。

实际应用验收：`pwsh -NoProfile -File scripts/native-smoke.ps1 -Notify`，Win10 19045，NATIVE_NOTIFY_COLLECTOR_OK / 退出 0。明确 debug 目录、合成只读 Home / JSONL 和测试准备的安装数组，watcher 禁用、活动 / 目录轮询设为一小时以区分本次 wake 与普通补扫。空来源先完成扫描；停止 notify owner 后追加 3 并调用正式 headless，SQLite 仍 0、零字节 marker 保存；正常重开 owner 消费后变为 3；追加 7 后正式 headless 在线唤醒变为 10，重复提示仍 10；暂停来源追加 1 / 提示仍 10，恢复采集为 11。通知载荷中虚构 999999 Token 未参与核算；隐藏的真实主窗口始终隐藏、源内容与 readonly 属性 / config 字节不变，正常退出完成。WebView2 仍输出与此前相同的 Chrome_WidgetWin_0 unregister 1412；不是通知失败，发布环境继续复核。此场景不是真实用户 Codex 回合或保真配置写入器验收，未运行性能测试。配置启用 / 撤销、受控原命令 runner、main-only IPC / 设置 UI、其余保留范围继续推进。

## M15b2：正式 exe 的只读 headless 唤醒入口

生产 `main` 先识别 notify 参数，再进入正常 Tauri：严格旗标 / 32 hex ID / 单一 JSON、缺失或额外参数 / 旗标错位拒绝，错误均有限且不回显输入。纯读取返回 nullable hint，raw JSON 不持久保存，未支持事件不访问登记 / 初始化 GUI。Windows 按与当前 Tauri 相同的 dirs LocalAppData + profile identifier 定位私有登记；读取能力不接受外部路径 / nonce。debug 原生验收目录仅允许固定前缀 + 32 hex 单个子目录名，release 不含该环境入口。

唤醒前检查登记的安装 exe 与当前 exe 相同，再只读明确 Home/config.toml，拒绝重解析点 / 目录或硬链接，文件 / 读取均限 1 MiB。当前根 notify 的完整解码参数属于该登记才发送；旧预览但未启用、用户改过或删除 notify 均不唤醒。在线提示成功返回，连接 / 确认失败将零字节 dirty bit 合并，日志仍为核算依据。当前只有 wake-only 路由：chain_original=true 明确返回有限不可用错误，受控原命令 runner 尚需接入，不能开放有旧通知授权链的正式启用 UI。

新增检查：integration 4 项（严格模式 / 最小身份 / 错误与无效 UTF-16 / 实际只读配置 / 所有权 / 硬链接和文件上限）。正式 `cargo test -p token-pulse-desktop --test notify_headless` 3 项，实际启动本项目正式 exe，经 debug-only 隔离 AppData 目录，在线把最小 DTO 送到真实管道、离线两次仅一个零字节标记、失效配置不标记、错误 exe / 原命令链选择 / 多余参数有限退出。源 config 字节不变、无 SQLite / 正常 Tauri 启动产物；没有调用正常 GUI 路径，不等同手工 UI / 真实 Codex 回合验收。合成配置由夹具写入，不是配置启用编辑器的证明。未改真实 Home / 账户状态；integration 合计 33 项、desktop 3 项，两个包 strict Clippy all-targets、fmt / diff 和 release check 通过，无性能测试。主进程 listener / 补扫 / 标记消费、原命令 runner、配置文件操作和正式设置继续实施。

## M15a3：私有持久登记与并发离线唤醒标记

`NotifyRegistration` 将 Home、capability 和原 notify 恢复记录关联到同一注册，原命令链明确保存 bool，不能省略成默认授权；没有可用原命令时拒绝 chain=true。版本 / 陌生字段 / Home / 身份错配在反序列化阶段拒绝，无 Debug 或原始配置快照。记录最大 3 MiB，最多 16 个，路径只使用已校验随机注册 ID。

Windows `NotifyRegistry` 只写已存在应用目录下的 `notify` 子目录，创建明确当前用户 owner 与 protected DACL、唯一用户 full-access ACE。既存宽权限目录 / 文件拒绝，不自动改权限；打开 reparse point 本身并检查实际句柄，不读取重解析或硬链接目标的正文。登记先写私有唯一 temp / sync，再 MoveFileEx 不覆盖改名，重名登记拒绝；共享禁止的应用私有 allocation guard 将跨进程数量检查和创建串行化。登记读取有界、字段 / 路径 ID 再核对，损坏记录不改写。

离线 dirty bit 是受控注册的零字节私有文件，重复提示合并，nonce 不符 / 未登记拒绝，不保存源路径、线程 / 回合、原通知或费用。标记领取先改名到唯一 claim，再交给补扫调用者；完成只删除该 claim，新 `.wake` 不受影响。未完成对象正常 Drop 时恢复标记或与新标记合并；失败不删剩余 claim。真实日志补扫 / 启动扫描仍为权威，不把标记作为消费依据。

验证新增 9 项：2 项独立记录契约；7 项 Win10 实际临时文件检查，覆盖私有写入 / 重开 / 不覆盖 / 源 config 字节不变、合并与领取后新到达、未完成恢复 / 合并、8 个并发写入只有一个 Created、错误 capability、宽目录 / 普通替换文件 / 硬链接拒绝、损坏 / 超限记录 / 非空标记及 16 条上限 / 无 temp 遗留。`cargo test -p token-pulse-integration --features test-fixture` 总 29 项及 strict Clippy all-targets / fmt 通过。文件检查是自动执行的真实 Win32 API，不是手工正式应用验收；未改用户配置 / 登录态、没有性能测试。正式 exe headless / 采集补扫、文件启用 / 撤销、UI 与原命令执行继续接入。

## M15b1：当前用户专属的有界 Windows 唤醒通道

新增 integration `notify_channel` 协议与 Windows 独立 worker。capability 由随机 UUID 组成的注册标识 / nonce 构造，持久表示加载拒绝无效形状 / 陌生字段；不提供 Debug。帧只含版本 / capability / 线程 / nullable 回合，1 KiB 上限、必要 nullable 字段不可省、版本 / nonce / 标识与额外字段均检查，错误不回显原载荷。反序列化后的身份复用 core 唯一受控 hint 构造器；没有路径、正文、Token 或费用。

命名管道使用当前 SID 摘要与注册标识，实际 protected DACL 只有当前用户一个 allow ACE、句柄不可继承、拒绝远程连接。首次实例保证同名所有权；最多两个活跃服务端句柄、Win32 总槽位 8，等待旧 IO / 句柄清理时仍能停止。worker 串行、只交给快速排队回调，最近 128 个提示 / 60 秒去重；客户端连接 / IO 总等待 300 ms、服务端交互 500 ms。停止取消部分帧、等待连接与槽位，关闭释放所有服务端句柄。

原生初测第一条成功后连续连接失败；增加 ACK 收讫仍暴露重用已断开 Tokio / Mio 实例的旧 IO 状态。现每个客户端使用新实例并先保留下一等待实例，客户端在收讫后等待服务端断开，旧 IO / 内核槽位未释放时受控等待。修正后全部定向检查通过。停滞连接测试还证实客户端超时后已写入的有效提示可稍后到达：确认未知不等于取消，补扫提示不产生消费，重复按有界去重处理。测试明确允许这项正确语义，同时拒绝非法 / 未授权提示。

验证：`cargo test -p token-pulse-integration --features test-fixture` 共 20 项通过（配置 10、纯协议 4、真实 Win10 管道 5、真实独立子进程 1）。系统检查包括实际 DACL / 槽位配置、连续连接 / 去重 / 同名拒绝、错误 nonce / 超长帧 / 停滞和后续正常提示、部分帧停止 / 重开、保留已断开客户端句柄后继续发送，以及 feature-only 子进程通过 stdin 接收测试 capability 并发出最小提示。子进程不是生产 exe / 安装资源，不依赖 GUI。strict Clippy all-targets / test-fixture、fmt / diff 通过；未运行性能测试。未改真实 Codex 配置 / 登录态；没有真实 Codex 回合、正式 exe headless、CollectorService 唤醒、离线持久标记或设置 UI 的完整验收，继续接入。

## M15a2：notify 配置保真计划与受控撤销

独立 `token-pulse-integration` crate 先实现纯编辑器，不依赖 Tauri 或读取日志。TOML 解析仅用于确定根级 key / value 的原始字节跨度及命令数组，不重新序列化整份配置。缺省与显式空数组分开；新增值位于所有表之前，BOM / CRLF / 中文保持。已存在的 TokenPulse notify 拒绝再次接管，避免未来原命令链递归。

应用计划核对全部配置字节摘要；撤销重新解析最新配置，安装路径、旗标与注册标识等完整参数必须相同，修改或删除则返回冲突。其他设置、最新键格式和外部注释继续保留。原值的多行 / literal 字符串及内部注释保存原样，恢复信息只含该值和受控安装命令，没有完整 config 副本。记录反序列化拒绝缺字段、陌生字段 / 版本、非绝对 exe / 非法标识、原值附加设置 / 表与 TokenPulse 自链。

自动验证：`cargo test -p token-pulse-integration` 10 项独立预期通过，覆盖只改值、表前插入、BOM / EOF、空数组、不同步预览、后续修改 / 归属冲突、原命令自链、非法 TOML / 类型 / 参数与文件边界、命令转义和记录损坏。`cargo clippy -p token-pulse-integration --all-targets -- -D warnings`、fmt / diff 检查通过。均为纯合成配置，不是真实系统写入或性能检查。持久登记、原子配置操作、原命令的明确授权链、headless 唤醒及正式设置仍需接入；真实用户配置未修改。

## M15a1：notify 载荷的纯领域读取边界

使用 OpenAI Docs 工作流实际读取[官方高级配置通知章节](https://learn.chatgpt.com/docs/config-file/config-advanced#notifications)：notify 命令接收一个 JSON 参数，支持完成回合事件；载荷可能包括输入消息、助手正文及 cwd。core 新 `notify` 模块直接反序列化三个允许字段，unknown 内容使用 serde IgnoredAny 跳过，不构造原始 Value / 正文容器。输出 `NotifyWakeHint` 只含 thread_id 与 nullable turn_id，私有字段只能由受控读取器构造；没有源文件读取、配置写入、命令执行、Tauri 或持久存储依赖。

先核对 64 KiB 字节上限再解析，必要标识 1–128 个 ASCII 字节且限制安全表示；它们是 opaque provider ID，不能据此访问路径或执行命令。完成事件缺失线程 / 无效回合拒绝，回合缺失保持 null。合法未知事件返回 None。重复允许字段、错误类型 / JSON / UTF-8 / 尾随载荷拒绝；三个错误只返回固定标识，不包含 serde 原错误或用户载荷。无法读取的通知不影响正常 watcher / 周期 / 启动扫描的统计依据。

自动检查：新增 `cargo test -p token-pulse-core --test notify` 5 项，通过官方形状合成载荷、嵌套正文 / 认证 / 数值 / cwd 全部剔除、未知与 null、危险 / 控制 / 过长标识、重复键与错误编码、确切 64 KiB 边界及超限先拒绝；预期序列化只含两个允许标识。core strict Clippy all-targets / fmt 与契约漂移检查通过。未运行性能测试，没有真实通知 / Windows headless 验收；该模块不新增 IPC / UI schema。

下一模块接配置保真编辑的 prepare / apply / restore：保留已有 notify，明确受控链式执行，当前内容不再属于本工具时拒绝撤销覆盖。headless 程序、当前用户唤醒通道 / 小型标记及正式设置入口继续实施；本次没有读取 auth.json，没有改变用户 config.toml，没有启用通知，也不将此读取器记为 M15 完整交付。

## M13e7：真实任务栏悬停与点击的独立前台验收

显式 `check_taskbar_wire` 新增独立可见测试窗口，用真实 SendInput 点击本次自有 HWND 使其成为非空前台，并记录全部 WM_ACTIVATE 失活次数。初始嵌入、真实悬停、详情普通快照刷新、配置 / 隐私 / 脱离和宿主 shutdown 均要求前台仍为这个窗口且失活次数为 0。点击按设计验证动作，不要求显式打开另一个应用表面时仍保留旧前台；点击后通过本次自有窗口的真实输入重新建立前台再验证被动配置变更。这不是移除焦点验证，也不调整生产激活逻辑。

读数和详情均核对宿主 PID / 自有类名；真实移动到已确认的自有读数，300 ms 原生 hover 后实际详情可见、保持焦点，普通刷新保持面板，点击关闭面板。新增完整 HostDetails 合成夹具；最初旧 fixture 没有 details，收到实际 WM_MOUSEHOVER 也正确不显示，补齐后检查通过，不将缺失详情伪造为生产故障。单击仅一次 OpenFloat、双击仅 OpenStats 且无延迟单击；配置 / 隐私屏障清除旧动作，token-only 实测宽度缩小，退出所有任务栏矩形恢复。

早期沿用旧前台跨点击断言失败，另一次空前台 0→0 得到“通过”但证据不足，均不计本模块通过。新独立前台检查复现点击失活，按 [任务栏交互设计](../design/taskbar-display.md#4-鼠标与键盘交互)区分显式点击和被动显示。所有临时宿主消息 trace / stderr 变更及样式试验已还原，没有改生产绘制 / 激活 / pipe / DTO。物理坐标使用仅线程级 PMv2 RAII，退出和断言失败恢复原 DPI 上下文及原光标，不调整系统 DPI；输入桌面不可访问或自有窗口被其他系统窗口遮挡时拒绝发送输入。

最终 Win10 19045 / 150% DPI 用户允许复测后，重建正式宿主，显式 wire 场景输出 actual_hover=true / actual_single_double=true / passive_focus_preserved=true / geometry_restored=true，退出 0；taskbar strict Clippy all-targets / fmt / Git diff 通过。期间一轮最终复测遭全屏 Windows.UI.Core.CoreWindow 覆盖而拒绝输入，用户恢复交互桌面后通过。测试窗口与内容都是明确开发夹具，不读取用户日志 / 账户 / 外部窗口标题，不运行性能测试。

此场景验证通知区左侧真实宿主动作，尚未把这些输入贯通正式 Tauri 开窗，也没有实际右键 / 键盘导航、滚动 / 屏幕阅读器、应用图标右侧物理输入、真实拥挤 / 自动隐藏 / 完整 Explorer、Win11 或物理多屏 / DPI 矩阵；这些检查继续保留。整体目标继续进行。

## M11h：区分输入环境与小窗恢复通路的原生验收

没有改写正式恢复快捷键实现。debug-only 输入检查只读打开当前输入桌面，比较应用线程桌面名称、检查前台和已按下的修饰键 / 目标键；不可交互时明确报错，不自动解锁、切换桌面、释放用户键或用合成消息冒充键盘。查阅 [OpenInputDesktop](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openinputdesktop) 和 [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) 的系统契约。

新增显式 `scripts/native-smoke.ps1 -RecoveryRoutes`，隔离 native-probe 数据库与正式两个 WebView。只向自有主 HWND 投递明确标记的 WM_HOTKEY：失效注册号、错误修饰键 / 虚拟键不触发；当前注册恢复隐藏小窗并清除忽略鼠标样式。实际设置 UI 保存 / 全局占用 / 旧键释放、外部注册冲突与 SQLite Writer 失败保留原注册及修订 / 草稿；随后检查实际 HWND alpha、置顶 / Writer 回滚、恢复交互保持透明度和重建 WebView。NATIVE_RECOVERY_MESSAGE_ROUTE_OK / MINI_OPACITY_OK / RECOVERY_ROUTES_OK、退出 0；输出明确未做真实键盘 / 鼠标验收。

本机 Win10 19045 / 150% DPI，用户明确解锁后重跑综合场景：真实 SendInput → WM_HOTKEY 恢复成功；真实鼠标穿透到独立测试 HWND，键盘解除穿透同时持久化。启用写入失败撤销样式、恢复保存失败仍可交互并显示重试、mini 只读权限、alpha 保持、账户合成周期及断开、本地程序检测 / 合成账户配置、关闭隐藏 / 托盘、单实例激活和合成电源路由均通过，NATIVE_SMOKE_OK / 退出 0。MINI_PASSTHROUGH_RECOVERY_SAVE_FAILED 来自显式故障检查；WebView2 注销 1412 仍记录。

自动定向 core 快捷键 1 项、store 快捷键 2 项、相关恢复设置 Playwright 5 项，以及 desktop strict Clippy all-targets / release check / fmt 通过。没有真实账户登录、人工按键、OS 文件选择对话框、Win11 / 物理多屏或性能验收。

额外任务栏 wire 真实鼠标场景的单击 OpenFloat、双击仅 OpenStats、配置 / 隐私清除动作与几何断言通过；前台 baseline 非空，点击后为空，最终焦点保持失败 / 退出 101。此项不计作通过，下一模块排查；整体目标继续进行。

## M10d2：必要位置的受限诊断查询与正式显示

相关契约 / 隐私运行时 Vitest 15 项及 desktop release check 通过；专项合成来源启动器不进入 release。诊断存储定向本次合计 7 项通过（新增 5 项、原暂存 / 发布回归 2 项）。

新 main-only query_diagnostics 使用受控 source_id|null，非空必须为保存的来源；严格 DTO 不含任意文件读取、原文样本、时间 / 历史游标、metadata / 继承 evidence 或用量载荷。真实 SQLite 快照统一读取未解决日志诊断、活动账本未确认 / 未归属观察、当前缺失文件及启用且根匹配的扫描错误。物理位置只来自当前指针选定的 current 代次；候选 / 退役 / 无效 / 未选定代次、历史账本和重复 / 继承分类均不混入。移动使用同一代次的新映射，替换后旧诊断不被误定位到新字节。

按来源 / 位置 / 类别 / 码选择一处最近代表项，确定排序后最多取 21 行（返回 20 处与 has_more），多条同类记录不形成历史列表。issue_id 为受控来源类别和内部标识的 SHA-256；偏移遵循精确 DecimalInt，没有位置或错误码时保持 null。目录错误用根位置，不制造文件偏移。新 Rust / TS / JSON schema、最新 PrivacyRedact 和主窗口权限同步，mini 实际拒绝。隐私仅隐藏已知位置，未知值不被变成“已隐藏”。

正式 UI 新增问题摘要与来源筛选，分别解释未确认用量、累计基线未归属、缺失文件、目录失败和不支持 / 无效 / 歧义 / 溢出记录，保留原导航与主题。切换来源立即移除旧结果，轮询串行；失败保留同范围上次状态，不伪装成空列表。隐私切换卸载旧视图，响应代次 / 显示策略阻止旧路径恢复，关闭后重新读取；空列表明确不能证明完整来源覆盖。深色 1280 与浅色隐私 960 全页截图实际查看，没有横向溢出。

自动检查：新增 core 2 项验证请求边界、无样本字段、序列化时最新隐私与未知 / 大整数；新增 SQLite 5 项验证代次 / 解决状态 / 活动账本边界、移动映射、代表位置、来源 / 上限 / 未知值、无 metadata / evidence 和真实旧读事务。diagnostics 过滤合计含已有暂存事务 / 替换发布回归 7 项；实际临时只读 JSONL 1 项核对适配后的确切偏移、必要代码、没有原文持久化及源字节不变。相关 Playwright 10 项（新增 2 项）通过，覆盖来源切换、精确 / 未知偏移、错误保留及隐私后延迟回复不能恢复路径；strict Clippy / TS / 构建 / 契约生成检查和 fmt 通过。

Windows 实际系统检查：显式 scripts/native-smoke.ps1 -Diagnostics 在 Win10 19045 / 150% DPI 使用隔离 native-probe 下自己的合成来源、真实只读 adapter、SQLite 与正式两个 WebView。暂停来源仍显示实际格式问题偏移及缺失文件；主命令与 UI 位置准确、隐私隐藏、mini 权限拒绝。首轮恢复来源时小窗操作已推进设置修订，旧 UI 快照 CAS 被拒绝；以正式 get_sources 取得最新修订再调用真实 manage_source 后，通过后台替换发布消除旧问题，可信总量为 9，原文件和修正文件字节保持，NATIVE_DIAGNOSTIC_POSITIONS_OK / 退出 0。检查使用真实恢复命令和诊断 UI，来源按钮 / 冲突用户交互由 Playwright 检查；不将此专项当作此前综合键盘恢复通过。WebView2 class unregister 1412 提示保留。

没有读取用户 Home / auth.json / 账户，没有完整历史、原文采样、继承证据浏览器、性能测试、人工键盘 / Win11 / 物理多屏或安装验收。简化诊断基础范围已接入；继续后台镜像 / 分叉组合、真实键盘恢复、账户 / 任务栏、notify、签名更新 / 安装和兼容验收，整体目标仍进行中。

## M10d1：简化来源诊断与单条重建进度

正式诊断页面共享已有 SourceSummary / SourcesSnapshot 和来源操作，展示可读性、已暂停 / 历史保留、最近扫描 / 成功时间、来源路径及错误原因，检测 / 暂停 / 恢复按真实 settings_revision 调用。诊断不提供新增目录、移除来源或账户配置；来源设置移除 WSL 启用入口，既有内部代码保留。时间使用保存的统计时区，未知时间明确无记录；来源查询失败保留上次状态，不能把读取失败视作无来源。原“账户额度未连接”硬编码移除。

新 main-only `get_rebuild_status` 在真正 SQLite 快照中选执行中重建、最早 queued、否则最近终态，只返回 Job|null，排除其他任务种类，不受最近 50 条历史限制。前端只展示一条基本进度 / 结果和文件 / 精确字节数、更新时间、错误原因，内部阶段映射为普通文案，未知阶段回退当前状态；不显示完整历史、内部作业编号或观察 / 事件计数。保留手动重建、终态前取消、未知提交结果同 request_key 重试；活跃时禁止重复新建，状态读取失败禁用依赖当前状态的操作并保留上次结果。旧内部历史 API 未拆除，mini 不获新权限。

自动检查：新增真实 SQLite 数据检查，55 条新历史不能遮挡较早 active；cancelling 仍优先，结束后显示 queued，再显示最近终态错误，其他 kind 不干扰，空库为 null。作业生命周期共 7 项通过。相关 Playwright 10 项通过，新增来源未知时间 / 失败原因 / 暂停恢复 / 检测 / 同修订、内阶段 / 编号不展示、精确大整数、读取失败保持及禁用取消；原未知提交重试及取消流程继续通过。strict TS / 生产构建、store / desktop strict Clippy、fmt / Git diff 通过。

视觉检查：实际查看深色 1280×860 和浅色 960×860 全页截图，完整导航 / 原主题保持，来源和重建在任务栏状态前，长整数换行及宽度没有横向溢出。截图和桥均为明确合成测试，不是生产演示数据。

Windows 实际系统检查：Win10 19045 / 150% DPI，隔离 native probe 启动正式打包资源与真实 WebView，通过新 IPC 的 null 空态、实际来源 / 重建空页面，输出 NATIVE_DIAGNOSTICS_OK；mini 对新命令实际拒绝。首次综合脚本停在小窗隐藏，第二次停在 SHARED_PRIVACY_NOT_COMMITTED。检查脚本原先在隐私乐观隐藏后即读取持久设置、点击尚禁用的隐藏按钮；修正等待提交 / 按钮可用，并在新诊断检查后恢复原主筛选页面。第三次通过 NATIVE_MINI_SCOPE_OK / NAVIGATION_OK / PLACEMENT_OK / MINI_OK，随后在真实键盘恢复步骤失败，综合退出 1；不能记为全套通过。WebView2 class unregister 1412 提示保留。没有实际来源选择 / 用户 Home / 账户、Win11 / 物理多屏、安装或性能验收。

下一步：加入必要文件位置与受限错误查询，补验后台镜像 / 分叉组合，继续账户与任务栏持续交互、真实键盘恢复问题、notify、更新安装与 Windows 兼容。诊断基础 UI 已收敛，但不代表文件级错误定位或全部系统交付完成。

## M06f8：替换候选归档迁移与整组作业撤销

实际回归先复现了新物理文件在候选读取期间移入 archived_sessions 后被当作独立文件导入：预期 602 × 8 = 4816，实际 4822，叠加旧 6 且覆盖仍为部分。现在按同来源的新物理 identity 查找冻结原输入仍有效的候选，原路径消失后执行受控位置迁移，原逻辑 file_id 保持。路径仍存在、身份歧义、目标占用或输入已变均拒绝，不合并副本。

reading / ready 更新候选冻结路径和检查点修订，保留暂存观察、必要诊断、上下文、锚点和读取游标，使旧位置读取的提交失效。claimed 在同一 Writer 事务失败所属作业及其整组候选账本 / 文件候选，再迁移位置，旧 manifest 不改写；新候选重新读取与核验。已失败候选可用于位置识别，失败检查点不被激活。原活动账本 / 文件代次 / 检查点与全局修订保持至正常验证发布，失败保留旧消费。迁移撤销扫描证明，调度启动完整补扫；普通单批 API 拒绝把候选的新位置独立登记。

自动检查：新增 7 项合成 SQLite 检查，通过保存进度与过期读取拒绝、来源隔离 / 多逻辑身份歧义、queued / running 清单作业整组撤销、位置写入失败完整回滚、已失败候选不复用、目标占用 / 冻结输入变化、rollout 边界 / 非匹配身份 / 修订溢出。候选存储和重建相关各 29 项通过；collector 全部 63 项（replacement-service 共 8 项）及 store / collector / desktop strict Clippy、fmt 通过。

Windows 实际系统检查：新增 2 项 Win10 19045 临时合成文件用例。四状态 reading / ready / claimed / failed 逐一迁移新物理文件到归档；跨 500 条读取及末行续写得到 4816，其余得到 24，数据库只有一份逻辑文件，读取阶段沿用候选代次，源字节保持。实际 watcher 将轮询设为 1 小时，运行中已冻结清单收到归档事件后失败旧作业、保留旧 6，独立 JobService 重新发布 24 并以归档位置取得 complete。

没有读取用户 Home / auth.json / 账户，没有新增 UI、人工窗口或安装验收，没有运行性能测试。后台镜像 / 分叉及实际格式覆盖继续补验；简化诊断、账户 / 任务栏持续交互、notify、更新 / 安装、Win11 和物理多屏仍按确认范围推进。

## M06f7：正常后台替换采集与发布后确认

正常后台单批读取遇到检查点冲突后，先使用只读物理探测区分实际代次变化与 Writer 竞争。只有真实失效才建立 / 继续候选；结构完整后原子排队并由独立 JobService 核算发布。reading / ready 候选继续自身进度；claimed 等待所属作业，已冻结 manifest 的整组其他文件也暂停后台读取，避免活动输入被推进。等待不重复建作业，不将正常候选阶段当作源不可读，也不确认旧代次。

未结束末行保持旧消费及缺口，后续新行继续同一候选；读取中再次改写作废旧候选，下一轮从新代次读取。作业成功后自动使用新活动代次补读并确认 source_scan，达到真实 EOF / 本次目录证明才恢复 complete；失败释放等待并保留旧结果。启动、native watcher、无 watcher 的周期核对及休眠恢复共用此通路，无需手动 executor 或界面重建。

Windows 实际系统检查：新增 replacement-service 6 项通过。Win10 19045 的原生 watcher 与轮询分别覆盖截断、不同物理身份 / 新 provider 替换、追加及完整证明；watcher 用例将轮询设为 1 小时。启动检测 601 次 × 8 = 4808，休眠期间替换保留旧值，唤醒后发布 45；未结束末行跨数据库重开后继续原候选并发布 602 次 × 8 = 4816。认领等待时旧检查点 / 消费不变且只有一个持久作业；整组其他文件的追加在冻结期间不推进，失败释放后最终总数为 28；第二次改写仅发布最新 9。源日志前后字节检查、发布后代次 / 检查点 / 上界与目录证明一致。

collector 全部 61 项及 store / collector / desktop strict Clippy、fmt / Git diff 通过。没有新增 UI，没有读取用户 Home / auth.json / 账户或运行性能测试；系统检查使用合成临时文件和实际 Win10 API，不计作人工桌面 / Win11 / 多屏 / 安装验收。下一步补验后台整组镜像 / 分叉及归档和替换交错，再继续简化诊断、账户 / 任务栏、notify 与部署。下述阶段的尚未接正常调度说明按本增量收敛。

## M06f6：替换依赖组核验与文件账本原子发布

schema v10 持久 manifest v2 保存新代次、原文件冻结值、登记完成 / 游标 / header / ReaderContext 及活动账本指针。新身份的旧账本为 null，无会话输入不创建占位账本；冻结后拒绝继续登记或认领。闭包仅加入本作业的新身份，同时覆盖旧 / 新父子关系、镜像和别名，其他暂存身份继续隔离；回放移除被替换旧代次，保留其他当前证据。旧 v1 普通清单兼容，不做 parser 全历史自动重解析。

正式 executor 先有界登记，再完成镜像、继承、流核算、分类和对齐验证，最后只读复核物理身份及锚点。替换缺失 / 暂停 / 再次改写拒绝，普通保存的缺失 / 暂停历史保留；封存后追加先按冻结前缀发布，再增量读取尾部。可信选择加入已发布 mirror provenance，原 canonical 文件被替换后仍采用确认副本，冲突替换观察保持 pending。

整组账本 / 新身份、canonical header / 父关系 / 别名、文件代次 / identity / 指针、candidate published、必要诊断 / 旧诊断失效、审计及作业成功在同一 Writer 事务切换。data revision 只增一次，price / settings 不变，原检查点和观察保留。发布故障整组回滚，真实旧读快照维持旧文件 / 身份 / 消费 / 修订。发布清空来源单文件证明并保持 incomplete，结构 EOF 不能替代后续真实确认。

自动检查：新增 6 项合成 SQLite 检查，重建相关 29 项通过；候选存储 22 项通过。覆盖发布回滚 / 真实旧快照、nullable 审计、跨来源旧 / 新父关系 / 镜像、外来 NULL 身份、header / 登记游标 / context / 原检查点 / 路径 / 启用状态 / 活动账本 / 作业账本集合过期、无账本清单门禁、诊断 / 扫描事务和旧 v1 清单。已有 v8 正常 schema 夹具随本次正常升到 v10，没有新增取消范围内迁移保护 / 灾难恢复检查。

Windows 实际系统检查：collector 55 项通过，replacement-read 新增 8 项 / 共 18 项。Win10 19045 临时合成 JSONL 完成同 provider 改写 / 截断、新 provider、空文件 / 真正无会话来源、已确认镜像保留 / 冲突隔离、改父后只计续段、封存后追加、只读属性及重开数据库继续采集。子会话夹具独立预期全来源 23 → 25 → 32 Token；镜像冲突保留 6；独立后台 JobService 从持久队列自动发布 130 次 × 8 = 1040，无 renderer 或手动 executor。源字节保持，只读测试恢复原属性。物理缺失 / 改写 / 暂停及取消保留原消费 / 检查点。

store / collector / desktop strict Clippy、fmt / Git diff 通过。未运行性能测试，未读取用户日志 / auth.json / 账户，没有新增 UI，也没有人工窗口、Win11、物理多屏或安装验收。接下来 M06f7 将正常 CollectorService 的失效代次接入候选读取、跨批继续、封存排队、所有权等待及发布后确认；随后继续保留的简化诊断、账户 / 任务栏与部署交付。下述 M06f5 等未完成说明为历史阶段。

## M06f5：候选作业所有权与有界必要输入登记

collector 完整功能回归 47 项通过（replacement-read 10 项）；store / collector / desktop strict Clippy、fmt / Git diff 检查通过。没有运行性能测试。

追加 schema v9，不改旧 migration。`file_rebuild_candidates` 关联封存代次和独占作业、冻结候选检查点修订并保存登记游标；`file_rebuild_sessions` 保存该代次的 proposed header，避免改写已发布身份。只有覆盖所属来源的 rebuild 可以认领 ready 候选，精确重复复用所有者，其他作业 / 旧修订拒绝。原子排队接口把 durable job 与 claimed 同时提交，后台不能先取得一个尚未关联候选的请求；认领失败回滚队列插入，终态精确重试返回原结果。

作业 running 且尚未冻结账本输入时，按 128 条 / 16 MiB 上限登记必要规范观察。载荷预算包含标识与指纹，重开库按游标继续；规范观察、项目规范化、会话绑定、新会话 NULL 活跃指针、独立 header 和登记游标在同一事务保存。复用普通采集观察写入，保留物理位置、时间缺失和必要字段，不读取日志正文 / auth.json。登记不会写活跃事件或基线、创建空活跃账本、推进原 / 候选读取检查点或改变全局 data / price / settings revision。需要账本核验后才能发布。

queued 取消、运行中取消后的失败收尾、普通失败和启动中断同事务标记所属候选 failed / 新代次 invalid，释放文件所有权并保留旧活跃结果；读取方无法重新打开或撤销 claimed。登记失败原子回滚新会话、项目、观察、绑定和游标；取消撤销失败也不会让 job 与 claim 分裂。普通 manifest / 直接成功推进拒绝静默忽略认领的新代次，不能以旧输入重建成功替代替换发布。

自动检查：新增所有权 / 登记 11 项合成 SQLite 功能测试，候选存储合计 22 项通过；普通采集事务 9 项、持久作业生命周期 6 项通过。覆盖 131 观察跨 128 条、持久重开 / 幂等 / CAS、完整载荷上限 / 空候选、header / 来源 / parser 与所有权门禁、排队竞争 / 失败回滚、登记与撤销回滚、各终态以及保留旧身份 / 消费 / 检查点。v8 真实表 / 校验和夹具正常升到 v9 后继续原 ready 进度，验证基本 schema 兼容，没有增加已取消的迁移保护或灾难恢复专项。

Windows 实际系统检查：Win10 19045 临时合成 JSONL 通过正式 readonly reader / adapter、原子队列与运行中登记，在同 provider 和新 provider 两种场景重开数据库跨 128 条继续；必要观察 131 条登记完成，缺失 created_at 保留 null，新身份活跃指针为空，原消费仍为 6 Token。源文件字节前后一致；中断收尾保留旧消费与检查点。测试未启动正式 JobService 自动执行替换，没有读取用户 Home / 账户，不计作最终替换账本发布或人工窗口验收。

限制与下一步：本次只完成受控所有权和必要输入登记，接口尚未接普通后台调度。继续显式选择封存新代次、移除被替换旧代次、处理新 provider 与整个镜像 / 分叉依赖组、物理验证以及文件 / 活跃账本同事务发布，再接正常采集。没有新增 UI 或性能测试。

## M06f4：未发布会话的查询与依赖门禁

以活跃账本指针作为普通会话入口的发布边界。暂存身份即使已有候选用量事件，也不会进入小窗会话选项或主统计；直接读取详情、轮次和上下文拒绝，父子关系不会显示尚未发布的名称 / 工具内部键或增加 child_count。已发布子会话自身已确认的 parent_provider_id 仍保留，未解析父关系为 null；已发布但没有消费的会话保持可选择和未知上下文。小窗固定范围拒绝选取暂存身份且不增加 settings revision。

普通重建的 All / Sources / Sessions 及镜像 / 父子依赖闭包排除未发布身份。普通相关会话检测、自动证明触发也排除暂存 peer / parent；采集已存在用量判定和自动作业输入证据使用当前物理代次。自动证明共用 M06f3 选择条件，未选定候选的未结束末行不会阻碍当前输入的证明或产生新的证据指纹。

自动检查：新增 3 项跨入口合成 SQLite 测试，验证候选账本 / 文件 / 事件与 NULL 会话隔离、名称和父子关系、详情 / 轮次 / 上下文直接读取、固定范围拒绝 / CAS 修订保持、普通依赖组及自动证明。分页测试使用真实 SQLite 租约，在测试 Writer 内模拟身份指针发布，新查询可见新身份，旧 cursor 保持发布前的选项与 data_revision。store lib 229 项普通检查通过 / 1 性能夹具 ignored；collector 的镜像、证明、分叉、暂停来源重放、普通采集、后台作业及既有核算版本共 25 项定向回归通过；store / collector / desktop strict Clippy、fmt / Git diff 通过。

限制：没有新增前端布局或 DTO，也未执行新增人工 UI / 原生安装验收；夹具不读用户日志或账户，不运行性能测试。测试内身份指针切换只用于查询快照验证，尚未实现真正的封存替换作业发布。后续继续受控关联作业与候选、必要观察有界登记、选定新代次和新身份依赖组验证、文件 / 活跃账本同事务切换与正常采集接入。

## M06f3：普通重建的选定代次边界

store / collector / desktop 的 `cargo clippy --all-targets -- -D warnings` 及 fmt / Git diff 检查通过。

修正已有重建把全部历史 file_session_bindings 当作输入的行为。普通重建只选取 source_files.current_generation_id 指向且 state=current 的物理代次；准备和 freshness 使用同一选择条件。分页回放、规范物理序列规划、用量完整分类验证均限定在这份冻结清单内，未选定的观察引用仍被 stage / ID 回放拒绝。暂停或缺失来源保留当前代次的存储历史，不依赖再次打开源文件来保留统计。

镜像发布也只改写选定输入的观察会话键、绑定和读取上下文 / 检查点，未选定的历史 / 候选内容保持原状。新增未选定证据不会无故废弃已经冻结的重建；选定指针、代次状态或检查点变化仍拒绝发布，旧活跃结果与真实旧 SQLite 快照保持。

自动检查：重建模块 11 项通过（新增 4 项，扩展既有镜像发布场景），collector 全部 46 项通过。合成 SQLite 夹具故意让未选定代次含 910 Token、当前代次为 110 Token，验证发布仍只有 110 且所有历史观察保留；检查排除 retired / invalid / candidate / 未指向 current、外来引用拒绝、晚到证据、指针 / 状态过期、暂停 / 缺失历史及镜像发布范围。collector 回归包含隔离 Windows 真实文件、监听、归档、候选只读读取及镜像 / 分叉；不读用户 Codex Home 或账户，没有性能测量。

限制：本增量完善普通重建的输入边界，尚未将封存 replacement 暂存观察纳入候选账本，也没有切换文件指针。候选会话建立、身份 / 镜像 / 分叉依赖组验证、原子发布及正常 CollectorService 接入继续实施；没有新增 UI 或人工原生验收，不计作文件变化后的最终统计已完成。

## M06f2：实际只读候选读取与有界继续

collector 新增独立 `replacement::read_replacement_file`。先验证来源启用、rollout 树及实际目录边界，确认原代次真实失配后才创建新候选，Writer CAS 冲突或普通追加不能制造替换代次。使用随机新代次 ID、空上下文和独立偏移，只调用正式只读 reader / adapter；规范身份是暂存的 proposed identity，不注册会话、解析活跃别名或继承旧核算基线。每次最多 500 个物理记录，再按实际有效元数据占用缩小事务片段，原文仅在 framing / adapter 中短暂存在。

重新打开数据库后按候选上下文、锚点、偏移及修订继续，不重新混入旧观察。尚未结束末行或超长行跳过状态保持未完成；结构 EOF 可封存，但不代表账本已经验证。已封存文件若连续追加，经原子 CAS 重新打开同一候选继续读取；物理身份、已读内容或上界再次失配时废弃候选，后续重试生成新所有者。进入未来 claimed 账本阶段的候选不能被读取方重新打开或取消。

自动与实际 Windows 文件检查：`cargo test -p token-pulse-collector --test replacement-read` 9 项通过；`cargo test -p token-pulse-store file_candidate --lib` 11 项通过（新增活跃查找 / 原输入校验 / ready 重开及 claimed 门禁）。Win10 19045 临时真实文件 / SQLite 检查缩短、替换、同大小改写、601 条用量跨 500 记录边界 / 重开库、未结束末行与封存后追加、再次改写 / 新候选、只读属性 / 内容保持、正文 / 未知原始字段不落库、来源暂停 / 树外路径、有效元数据缩小片段以及跨 16 MiB 批次的超长行跳过；旧 consumption / data / price / settings revision / 检查点保持（用户主动暂停的设置修订另按正常 CAS 改变）。原始日志均为合成夹具，不读用户 Codex Home、auth.json 或账户。候选 / collector / desktop strict Clippy、fmt 通过；只读属性场景在保存原属性后恢复测试夹具。没有性能测量；大文件与记录数量只验证读取边界功能。

限制：接口没有接正常 CollectorService 的 InvalidGeneration 分支，没有新增前端或人工窗口验收。尚需把封存新代次代替旧代次纳入整个依赖组重建，核验新 provider 身份、镜像及分叉，再原子发布文件与活跃账本；不能用本次暂存读取冒充最终修正后的统计交付。

## M06f1：文件新代次的隔离读取暂存

追加 schema v8 的 `file_read_candidates` / `file_candidate_observations` / `file_candidate_diagnostics`。候选 `file_generations.state='candidate'` 使用自己的偏移、锚点和 ReaderContext，每批最多 500 条规范观察及 500 条受控诊断，总必要载荷不超过 16 MiB。规范观察不写入活跃 `observations`，不为未知的新 provider ID 提前创建会话、项目或基线；只保存既有白名单规范字段，不保存源 JSONL 正文或认证信息。候选观察、诊断、上下文、偏移及修订在同一个 Writer 事务保存。

创建时冻结原文件指针、代次 / 检查点 / 锚点 / 上下文 / parser、来源根与启用状态及位置，后续阶段要求仍匹配；同一文件最多一个 reading / ready / claimed 候选。精确重复请求复用原进度，其他所有者或不同请求载荷拒绝。开始候选将文件覆盖标记为 correction_pending 并撤销单文件扫描确认，但不改变旧指针、旧检查点、消费及 data / price / settings revision。旧真实 SQLite 快照仍保留原状态。

未结束末行、超长行跳过状态及再次缩短不能封存为读完；结构 EOF 封存仅表示候选读取位置完整，仍需实际物理锚点验证和整个依赖组账本核算验证才能发布。失败原子标记 candidate failed / generation invalid 并释放所有权，不回退或清除旧消费，也不假装原文件已经恢复可信。

自动验证：9 项合成 SQLite 候选检查通过，覆盖隔离 / 重开、重复 / 竞争创建、未完成末行 / 超长行、再次缩短、原输入变化、候选 CAS / 封存、观察 / 诊断 / 检查点一起回滚、错误位置 / 重叠 / 上界 / 批次数及失败后新候选和旧快照。原采集事务 9 项与扫描存储 8 项回归通过；store / collector / desktop strict Clippy、fmt 通过。测试不读用户目录或账户、不运行性能测试。本提交没有新增 UI 或实际 Windows 读取接入，不计作原生文件替换 / 同大小改写验收。

下一步：接只读候选读取及进程重启后有界继续读取；重建输入须排除被替换的旧代次并加入封存新代次，核验整个依赖组，再在同一事务切换文件指针与账本。不同 provider 会话、镜像、分叉及读取途中再次改变仍属于该完整功能范围。

## M06e1 / e2：当前扫描证据、采集调度与快照覆盖

schema v7 仅保存当前来源扫描代次及有界批次的文件证据，不增加完整扫描历史界面。枚举与文件读取分开登记；新发现尚未注册的文件也计入待读缺口，已知文件通过身份去重计数。完成要求完整目录枚举、无遍历错误 / 未知成员变化、全部当前文件的路径 / 代次 / 检查点修订 / 上界和 EOF 一致；来源可读且没有待归属 / 格式等其他缺口时，查询才返回 Complete。分项 Token 完整性独立表达，空来源选择仍为 Unknown。模型 / 日期筛选不能遮蔽整个来源的文件缺口。

启动及退出、暂停 / 恢复和显式检测使旧证明失效；文件提示及主动核对在重读前撤销对应确认，未知路径或删除目录触发补扫。完整枚举后移除已不存在的队列路径与重试，避免归档后继续轮询旧位置；数据库保留历史消费，缺失文件保持覆盖缺口。周期补扫等待上一轮有界读取队列完成，避免反复重开扫描导致确认无法收敛。日志树内 reparse point 不跟随，明确保留扫描缺口，不能把跳过的目录视为完整。

自动验证：store lib 211 项普通检查通过 / 1 性能夹具 ignored；扫描存储 8 项、覆盖查询 14 项（新增 7 项）定向检查通过，包括旧真实快照、过期检查点、暂停 / 改根、未知成员、未注册文件和分时桶覆盖。collector 全部 37 项功能检查通过（新增 6 项）；发现 / 调度 7 项通过。core / store / collector / desktop strict Clippy、fmt 与 Git diff 检查通过。没有运行性能测试，129 文件仅验证跨 128 项登记批次的功能完整性。

Windows 实际系统检查：Win10 19045 上的隔离临时日志和真实 SQLite，经原生文件监听验证追加末行结束、归档移动、删除日志树；无 watcher 的核对、启动 / 暂停 / 唤醒 / 重启也验证消费不重计，原始日志内容保持。真实 junction 夹具验证目标文件未被读取或修改、扫描保留缺口。检查由自动化调用真实 Windows API 完成，不计作人工 UI / Win11 / 多屏验收。本模块未改变前端布局或 DTO，没有新增视觉验收。

限制与下一步：InvalidGeneration 仍保留旧消费并显示缺口；截断、替换或改写后的新代次候选读取 / 验证 / 原子切换尚未实现，不能宣称该场景统计已完成。简化诊断页面、其他原生兼容及部署待办继续推进。

## M09g2b3：正式重估生命周期、IPC 与进度界面

应用启动独立 RevalueService，价格规则 / 别名成功写入和休眠恢复发送有界唤醒，周期核对补建新增用量；退出保存中断并等待线程结束。main-only 的状态 / 启动 / 取消命令返回最新隐私策略戳，事件仅使状态失效。价格页接真实 DTO、基本进度、取消和两种估价依据；历史版本只读、旧价格版本需刷新后提交、状态读取失败保持未知。单次失败重试保留请求幂等键，修改依据 / 隐私卸载清除草稿；前端合并读取避免叠加状态请求，事件之外保留周期刷新。未增加完整任务历史、导出或恢复入口。

自动检查：4 项重估浏览器测试及既有 6 项价格 / 别名 / 离线目录回归通过，覆盖指定毫秒时点、大于 JS 安全整数的精确进度、取消 / 重试、版本冲突、历史只读、读取失败、最新隐私卸载与提交响应丢失后保持原幂等键。深色 1280 / 浅色 960 完整截图已查看，无水平溢出。store lib 最新共 196 项普通检查通过 / 1 性能夹具 ignored；新增定向来源 / 会话范围及进度发布失败回滚验证。strict Clippy / fmt、契约与前端类型 / 生产构建通过；未运行性能测试。

实际系统：Win10 19045 / 150% DPI 的 `-PriceRevalue` 隔离独立进程返回 NATIVE_PRICE_REVALUE_OK、退出 0。通过正式启动 + Writer 原子合成消费，自动 event_time、手动指定 1500ms 与 React 明确时点分别发布 900 金额原子的 USD 缓存；实际 main WebView 验证幂等、CAS、无效范围、终态取消、历史只读、mini 三命令拒绝、共享隐私卸载 / 重开，消费修订及文件检查点不变。运行中取消 / 退出边界由同步线程 SQLite 测试、取消界面由浏览器检查验证，未冒充人工长任务原生操作。WebView2 1412 退出提示保留；Windows 11 / 安装器仍未验收。

## M09g2b2：独立费用执行服务

`RevalueService` 已建立自己的单线程及有界唤醒信号，启动检查 interrupted 作业 / building 缓存、按固定价格修订处理队列，周期检测当前缺口并保存每批 / 每账本进度。手动请求支持幂等和取消，停止时传递取消信号并等待线程结束，正常中断可在下次启动自动补建。取消后同一输入不自动重复排队，已经 ready 的缓存保留；失败保存受限错误码。服务不持有来源路径或 parser 状态，不写消费事实 / 检查点。

自动检查：6 项线程集成测试通过，包含启动 / 新事件 / 新价格补建、取消未发布候选、退出与重启、途中版本变更、手动指定时点 / 空工作、发布失败保留旧缓存。取消与退出使用同步屏障取得确定边界；所有测试均为功能检查，无性能报告。store strict Clippy / fmt 通过。当前为可测试的独立服务，正式应用启动 / IPC / 主窗口进度与实际 Windows 验收随后接入。

## M09g2b1：固定版本的持久费用作业

schema v6 已建立独立价格作业及账本计划，支持幂等请求、价格修订 CAS、串行领取、手动优先、原子单调进度、取消及重启中断。自动身份排除缓存就绪状态，取消 / 失败不会因部分缓存变化而反复启动；价格 / 证据变化建立新工作，自动 interrupted 请求可补建剩余项。缓存构建新增明确价格修订参数，多账本重估与途中改价使用同一固定版本，不触碰 Token / 数据修订 / 检查点。

自动检查：8 项价格作业状态 / 重开 / 队列 / 幂等 / 缓存覆盖测试及 1 项实际改价过程中固定旧价的缓存测试通过。store lib 共 188 项普通检查通过，1 性能夹具 ignored；未执行性能测试。新 DTO 同步 Rust / TypeScript / schema。后台线程、正式 IPC / 界面与原生重估验收接下来继续，不能将本模块视为完整重估交付。

## 起始状态（2026-10-01）

初始 HEAD 为 `7e13fd0`，分支 `main`。仓库只有设计、截图和交互原型。用户已有内容：修改的根 `README.md`、未跟踪的 `AGENTS.md`、`docs/README.md`、`docs/design/token-pulse-design.md`。这些文件不混入功能模块提交。文档索引在原有内容之后追加实际工程文档链接，保留用户内容及其未提交状态。

## M01：工程与运行壳

已建立 npm / Cargo workspace、Tauri 2 / React / TypeScript / Vite、锁文件、Windows CI、独立开发标识和应用数据目录、窗口 / 托盘 / 单实例生命周期、显式命令权限及生产 CSP。七项文字导航与五项设置分类沿用原型色彩和尺寸方向，尚未实现的业务明确标记；无演示用量或价格。

自动检查：前端 strict typecheck、生产构建、浏览器预览拒绝伪造桌面状态的单元测试（1 项）、七项导航 / 五项设置交互测试（1 场景）已通过。Rust check、fmt、Clippy（warnings denied）和 workspace test 已通过；M01 Rust 尚无算法测试，不将零测试计为算法验收。

浏览器视觉检查：深灰背景、近黑面板、205 DIP 导航、蓝色操作、空态与连接错误清楚可辨；在 Codex 浏览器中完成截图审查。此结果只证明浏览器布局。

真实 Windows 10 / Tauri probe：2026-10-01，`cargo build -p token-pulse-desktop --features custom-protocol` 和独立 exe `--native-smoke` 返回 0。确认开发目录隔离、冷启动窗口、托盘注册、原生 close 事件隐藏且托盘保留、第二实例退出并重新显示已有窗口、明确 app.exit。使用内置静态前端，运行不需要 Vite / Node。退出时 WebView2 输出 class unregister 1412 警告，进程仍成功结束；该系统提示继续观察，不记为界面交互验收。

待验收：人工托盘点击与菜单、WebView 中真实 IPC 页面交互、DPI / 屏幕阅读器、Windows 11、release 构建与安装器 / 更新。M02–M16 的业务尚未完成；M01 的页面结构不计作 M10 完成。

## 后续依赖

M02 固定 Rust 领域类型、DTO / TypeScript / schema 生成和精度规则；随后 M03 建立数据库、单写线程和完整事务，M04 / M05 分别实现只读适配和可重放核算。每模块经必要验证后单独提交，后续模块继续沿用本文件记录。

## M02：领域与协议

以 Rust 定义标准用量向量、物理位置、白名单元数据 / 观察、质量分类、状态、错误、筛选、快照、Token 分解、费用 / 覆盖、额度、mini 范围与作业 DTO。使用 ts-rs / schemars 生成 TypeScript 和序列化 JSON Schema；前端运行状态已改用生成类型。后续模块新增 DTO 必须加入同一生成流程。

大整数和修订使用受校验的非负十进制字符串；金额为最多 15 位小数的定点字符串，原始 Token 保持 i64，聚合 checked i128。前端用 BigInt 格式化完整数值 / 缩写 / 有界百分比 / 最终金额舍入。未知值显式 null；超范围返回错误，不截断为零。时区、日期范围、维度数量、非法枚举与未知字段拒绝。带标签的空分支采用空 struct variant，避免 Serde unit variant 忽略额外字段。

`fixtures/usage-vectors.json` 包含 10 组人工列出预期的合成向量；manifest 记录版本与来源。标准观察拒绝正文和任意附加字段，未声明任何真实 Codex 格式已兼容。

验证：Rust 5 项测试（含 property test 和 10 组向量）、前端 5 项测试（含生成 schema / TS 样本、精确格式化）、七页导航浏览器场景、typecheck / 生产前端构建通过。fmt、workspace tests、Clippy 与协议生成差异检查通过。2026-10-01 原生 probe 返回 0，已在真实 WebView 调用 `get_app_status` 并核对协议版本、请求身份和开发状态，随后完成窗口 / 托盘 / 第二实例 / 退出回归。退出时同样有 WebView2 class unregister 1412 提示。

## M03a：迁移、写线程与读连接

已建立 v1 的 25 张业务表与迁移校验表，含活跃账本 / 文件代次组合外键、必要观察、事件、来源证据、基线、jobs / diagnostics 和维护基础表。写入集中于有界队列的专用线程；两个只读连接启用 query_only，bundle 回调在固定读事务内运行。初始化使用 foreign_keys / busy_timeout / WAL / FULL，校验迁移 checksum、quick_check 和 schema 版本；未知较新结构或损坏文件拒绝初始化，不创建零历史覆盖旧文件。

`sum_token_decimal` / `sum_money_atoms` 使用 checked i128，全部缺失返回 NULL。四项真实 SQLite 集成测试通过：建表 / 重开 / 禁止只读写入、并发提交时旧读事务仍固定、超 i64 总量及金额溢出、未知结构与损坏文件保留。fmt / Clippy 通过。本阶段还未接入采集整批事务，M03 完成需继续实现该部分。

## M03b：采集事务与运行接入

已提供受控来源 / 会话 / 文件注册和强类型 WriteBatch。提交前核对活跃文件代次、偏移 / checkpoint revision、活跃账本、流 state revision；观察、事件、来源证据、待确认项、上下文、基线、诊断、reader context / anchors 和检查点在同一事务内提交。跨会话关系由组合外键及 Rust 校验共同保护，发布修订在 commit 后返回，零增量不创建消费事件。

运行壳已在独立数据目录打开真实 SQLite；数据库失败保留文件并返回明确错误状态，仍可显示诊断界面。不会用空数据库或零用量覆盖损坏历史。

六项批次测试通过：六个提交边界的故障 / 重开检查、正常批次及过期重试不翻倍、SQL 约束失败整批回滚、跨会话和错误组合指针拒绝、真实 SQLite max_page_count 磁盘满、相同推断指纹的不同调用保留。另有来源目录保持原字节 / 无额外文件的隔离测试。以上故障是受控错误注入与真实数据库容量限制；尚未把“强杀进程崩溃 / 实际磁盘写满”列为通过，该类灾难验收在后续模块继续补充。

分页专用租约、重建发布、迁移前备份、维护 UI 与作业状态机仍分别由 M07 / M08 / M14 / M16 交付；v1 初始化没有旧版本需要升级，不以结构表存在宣称相关功能完成。

## M04：只读 Reader 与 CodexAdapter

已实现只读句柄、Windows 卷 / 文件索引 / 创建时间身份、固定大小上界、64 KiB 块、完整 LF / CRLF 字节位置、半行重读、8 MiB 行限制和 16 MiB / 500 记录批次限制。超长行保存起点、扫描位置与内容锚点，跨批次直到换行才安全越过。读取后重新核对实际消费字节与路径身份；缩短、替换及锚点失配返回代次冲突，不推进旧检查点。

头尾和固定 8 MiB 分段保存完整区间 SHA-256；mtime 改变时验证已读分段，稳定文件可复用已封闭分段。此实现优先保证普通中段覆盖可检测；持续大文件追加的哈希开销尚未完成性能验收，M06 / 性能数据集需测量后优化，不能通过移除连续性检查提高指标。保留设计中对刻意伪造元数据修改的限制。

适配 `session_meta`、`turn_context`、`event_msg.token_count`；必要元数据和 last / cumulative / 容量进入白名单观察，正文与认证内容不进入持久结构。未知提供商、时间、模型和数值保持缺失；非法字段 / UTF-8 / JSON、未知类型产生无原文诊断，负数观察交由核算层隔离。模型 / cwd 的缺失保留前值，显式 null 清除，非法元数据不会部分修改上下文。没有已验证格式证据的请求 ID 和周期标志不自行推断。

八项文件 / 适配测试通过：UTF-8 跨块与半行追加、超长行恢复、记录上限准确偏移、中段覆盖 / 截断 / 替换 / 归档移动、auth.json 在打开前拒绝、合成布局独立预期、损坏 / 未知字段隔离、不同分块与持久上下文重放一致。workspace 测试、fmt / Clippy 与协议差异检查通过。夹具 manifest 明确这是确认设计对应的合成布局，真实 Codex 版本兼容验收尚待完成；M06 尚未连接来源发现和后台采集。

## M05a：纯核算流与周期分类

已实现可序列化的最多 128 个基线、唯一同构向量匹配、可信流累计差、明确周期重置、仅 last 消费、首次 total 未定位锚点、无时间用量隔离及可信重复分类。缺失分量不当零，不凭模型 / 回合 / 时间相同建立流；不明确的下降、多候选、差值掩码不一致和超容量保留待确认，不淘汰旧基线。无效累计不推进状态，仍可独立返回有效 last 上下文。物理重复、可靠身份 / 已对齐镜像及继承通过明确证据入口分类，不重复推进基线。

`accounting-streams.json` 有 12 组人工列出每步分类、事件向量和总量的合成场景。七项核算测试通过，包含所有夹具逐事件比对、任意批次边界 / 状态重载一致、数值溢出 / 缺失、重复 / 继承证据保护、128 流容量和随机单流独立加法 oracle（proptest）。fmt / Clippy 通过。完整镜像序列对齐、父子前缀证明、身份冲突和来源调度证据生成尚需 M05b / M06，证据入口本身不代表这些流程已完成。

## M05b：镜像、身份与继承证明

已增加按原始序列顺序比较的 UsageSignature、仅作候选检索的指纹、完整镜像 / 前缀与后续部分校验、相同会话 ID 的创建时间 / 内容冲突、未扫描完整前序隔离、唯一父关系下的继承前缀与多个父候选诊断。可纠正模型 / cwd 不参与不可变指纹；没有强头部身份时要求可信请求身份或完整分量的有序累计进展，不凭相同时间 / scalar total 合并。

已将继承证明与父前缀结束时的基线连接到核算：子会话继承基线而不计入继承消费，后续唯一增量可正常计入。镜像累计差也可引用原规范观察，避免只支持带 last 的记录。可靠请求身份按会话 / namespace 隔离，同 ID 的用量不一致返回冲突。

六项序列测试通过，含 11 组人工预期的镜像 / 父子场景、元数据修正指纹稳定、请求身份冲突、未知 / 未完成证据，以及父 110 + 25、子仅新增 13 的联动事件集合验证。累计差镜像和请求重放时间变化也已验证；core 全套测试与 Clippy 通过。实际多来源发现、数据库候选查询、父数据迟到触发重建和候选版本发布仍由 M06 / M07 接入，尚不宣称完整跨来源导入已验收。

## M06a：来源发现与有界工作队列

已提供显式目录 / CODEX_HOME / 用户默认目录按优先级去重、来源 / 可读性 / 能力 DTO、仅 sessions 与 archived_sessions 的可恢复目录迭代器；不打开 auth.json 或其他根目录文件。单目录缺失 / 无权限不停止其他目录，子目录 reparse / symlink 不跟随，目录深度与同时打开的句柄有上限。WSL UNC 仅接受显式 WSL 来源，本地来源拒绝网络 / 设备目录；未探测能力使用 not_probed。

工作队列最多 4096 个不同文件，合并重复 watcher 提示、实时优先、300 ms 可配置防抖并限制连续提示最多延期 2 s；溢出设置必须补扫的标志。六项来源 / 调度测试、Clippy、生成协议、TS typecheck 和前端单元测试通过。这里只是调度基础，不代表 watcher、WSL 发行版连接或后台采集已开始运行；接下来连接持久来源、读取 / 核算 / 事务及实际文件监听。

## M06b：读取、核算与原子提交集成

已建立独立 collector crate，将启用来源校验、只读读取、上下文恢复、适配、纯核算和 WriteBatch 串联。真实快照读取文件检查点与活跃基线，批次只写最终流更新；提交失败不推进源偏移。相同物理身份的归档移动更新位置并复用检查点。来源路径必须属于已注册目录的两个 rollout 树，解析后再次校验范围；reader 在 Windows 以 OPEN_REPARSE_POINT 打开并拒绝 reparse 文件，不通过文件链接打开认证内容。

每个独立物理序列先建立独立逻辑身份；其他相同 provider session ID 或 parent 标记序列进入 lineage_pending，不在完成完整证明前叠加消费。M07 将接入候选重建 / 镜像归一及父子发布。无时间的最近请求只保留必要观察，不伪造 context 时间。元数据沿用并复制到必要观察时，按最大有效元数据尺寸降低记录预算，保证 Writer 的 16 MiB 上限不会导致永远重试同一批。

六项真实文件 / SQLite 集成测试通过：导入 / 重启 / 半行补齐 / 追加 / 归档移动、镜像与缺父隔离、截断后旧结果和检查点保留、1001 个同时间同向量调用跨 500 记录边界恢复、认证 / 范围路径在打开前拒绝、超长有效元数据自动减小批次。workspace 回归、Clippy、fmt 和协议检查通过。此时尚未在运行壳启动后台线程 / watcher；完整 M06 仍需持续调度、来源 UI / 能力探测与休眠恢复。持久 mtime 优化及大文件性能验收尚待完成，当前每批完整验证已读分段以保证连续性。

## M06c：持续后台服务与电源生命周期

运行壳已启动独立后台 collector，按启用来源启动补扫；有界 watcher 提示、300 ms 防抖 / 实时优先、近期 60 s 核对、历史 10 min 清单、溢出补扫和文件独立 1 / 2 / 5 / 15 / 60 s 带抖动退避已接入。目录扫描分片推进，队列繁忙时保留扫描位置；空目录与部分目录故障有真实可读性，能力只在实际 watcher / reader 成功后标 available。当前正式应用尚未通过 UI 注册来源，因此首次启动仍正确显示未配置。

暂停 / 恢复 / 补扫 / 停止使用原子控制标志，不等待满提示队列。关闭主窗口继续采集，明确退出等待当前短批次结束并关闭 worker。Windows 主窗口 subclass 接收 WM_POWERBROADCAST，休眠暂停新批次，恢复重新建立 watcher 并立即核对。

两项后台集成测试通过：禁用 watcher 后的启动 / 周期补漏 / 暂停恢复 / 重启，以及实际 Windows 原生文件 watcher 在核对间隔设为一小时后仍采集追加。workspace 全套、Clippy、fmt、协议检查通过。2026-10-01 Tauri native probe 返回 0，增加真实 HWND 的合成 suspend / resume 电源消息路由检查，并通过 IPC / 窗口 / 托盘 / 第二实例 / 退出回归；退出仍有 WebView2 class unregister 1412 提示。合成消息不等于机器实际休眠，实际睡眠、断网 WSL 阻塞取消、Windows 11 和大数据性能仍待系统验收。来源选择 / 暂停 UI 与 WSL 能力路径继续在 M06d 接入。

## M06d：来源管理与原生目录凭据

设置中的数据来源页已连接真实 DTO，可检测 CODEX_HOME / Windows 默认目录、使用原生目录选择器添加本地或显式 WSL 来源、暂停 / 恢复，以及停止来源但保留历史。来源目录去重、最多 32 项、配置修订冲突和非法来源身份均有后端校验；能力更新不改变配置修订，不覆盖 origin 或移除状态。未探测、失败、没有成功记录分别显示实际状态或 null，不用零占位。

目录选择在 Rust 侧执行并校验本地 / WSL 边界；后端保存绑定主窗口、五分钟到期、单次消费、最多 16 项的目录凭据。前端不能直接提交任意路径，没有获得通用文件系统权限；选择取消返回 null。页面有加载、错误保留和操作忙状态，定期读取真实来源状态。

验证：全部 53 项 Rust 测试、fmt / Clippy、协议生成差异检查、5 项前端单元测试、生产构建通过。两个浏览器交互场景通过，其中来源场景使用明确的合成 IPC fixture 验证未知能力、暂停 / 恢复、保留历史；浏览器连接失败页面完成视觉审查。2026-10-01 独立 Tauri native probe 返回 0，真实 WebView 新增 get_sources 协议校验，并通过既有电源路由 / 生命周期检查；退出时仍有 WebView2 class unregister 1412 提示。

待验收：人工原生目录选择与 WSL 发行版实际连接、WSL 能力降级 / 断连取消、首次启动默认来源自动注册、持久 mtime 与大数据性能、真实睡眠。来源页与后台采集已可运行；镜像 / 分叉仍保守隔离，需 M07 候选重建与发布后才完成多来源准确性验收。

## M07a：持久作业状态机

已提供强类型作业范围、规范化请求与持久 request_key 幂等。相同 key 与相同请求返回原 job，不同请求拒绝；作业查询与最近列表使用真实读事务。进度采用十进制字符串，检查发现 / 处理计数、单调推进和已完成发现状态；resume 使用版本化、拒绝未知字段的 checkpoint，包含解析 / 核算版本、批次位置与候选账本 ID，不接受任意 JSON 状态。

Worker 的状态推进带 expected state；排队取消直接完成，运行取消保留 cancelling 至完整批次边界，晚到进度不能覆盖取消。进入 publishing 与取消由单写线程串行竞争，取消只返回 accepted 或 too_late，最终状态明确。运行壳在启动后台采集前将未完成作业标 interrupted，候选标 failed，旧活跃账本保留；未校验输入的任务不会自动继续。

五项 SQLite 集成测试通过：请求规范化与重开幂等、两类取消、20 轮并发发布 / 取消竞争、超 i64 进度 / 退回拒绝、重启中断与旧指针保护。全部 58 项 Rust 回归、Clippy、fmt、DTO 生成与前端生产构建通过。此阶段尚未连接执行器、候选重放 / 发布、作业 IPC 和诊断页面；状态机通过不代表 M07 已完成。

## M07b：依赖闭包与候选发布事务

已实现从来源 / 会话范围扩展镜像 ID、父与子依赖闭包，固定旧活跃账本、身份、物理代次、消费上界、检查点修订和内容锚点 manifest。候选按 500 条事件 / 待确认上限和 16 MiB 批次写入，复用正式消费、上下文、来源证据与基线校验；候选写入不推进活跃统计修订。完整 manifest 仅持久一次，其他组成员保存引用，避免按会话重复大 manifest。

验证和发布再次核对闭包、绑定文件、身份与检查点；输入变化返回 CANDIDATE_OBSOLETE。观察必须有消费或明确分类，允许可信 last 与未定位前置锚点并存。发布在同一事务退役旧账本、启用所有候选、切换会话指针、重算活动时间、写精确差异审计 / data_revision 并完成 job。旧版保留，读事务不会看到组内混合版本；失败 / 取消只标候选失败。

六项新增存储测试通过：候选不可见及并发旧快照、发布 COMMIT 前故障整组回滚 / 重开、缺失分类 / 输入推进拒绝、镜像与父子闭包及迟到身份、取消与锚点分类、活跃账本 / 过期基线保护。全部 64 项 Rust 回归、Clippy、fmt、DTO 生成、5 项前端单元测试与生产构建通过。故障为受控错误注入，尚未进行发布期间真实强杀。规范观察执行器、完整镜像 / 继承归一、文件替换后的新代次、内容重新校验和作业 UI 继续实现；仅存储事务不计为完整重建验收。

## M07c：规范观察重放与迟到父关系

独立执行器已按固定 manifest 分页重放必要观察，每页最多 256 记录 / 16 MiB，候选基线带修订校验、批次进度持久化，完整记录边界检查取消。发布前对仍存在的源文件只读核对身份与已读内容锚点；物理变化使候选失效。源文件缺失时允许从必要观察重建已采集历史，不恢复从未采集内容。

父子按依赖顺序处理，通过唯一父头部和完整有序前缀证明，将父前缀的流基线逐次种入子会话，继承本身不消费。缺父、多个候选、环和不完整证明保持 pending。多个物理镜像目前保留既有可信主序列，其余仍隔离，完整镜像映射与跨来源共同证据尚未接入。签名和被子会话引用的父结果分别设 128 MiB 序列预算（必要字段序列化大小加结构开销）；超限保留旧账本。实际峰值内存仍需测量，不以预算代替大数据性能验收。

五项文件 / SQLite 执行器测试通过：原样重放与来源缺失恢复、子先到 / 父后到后父 110 + 25 与子仅 13 的精确事件集合、取消 / 物理覆盖保留旧结果、600 个相同时间调用跨分页与零量、父关系环不误判继承。全部 69 项 Rust 回归（最后新增环测试独立运行）、Clippy、fmt 与协议差异检查通过。执行器仍未连接后台 job worker / IPC，持续追加的有限追平、镜像归一、文件新代次及恢复策略继续实现。

## M07d：后台作业与诊断入口

独立 JobService 已在运行壳启动，按持久队列串行执行重建并在结束后请求 collector 核对；有界唤醒提示、最多 32 个未完成作业，回复丢失后的相同 request_key 重试不占新位置。退出在记录 / 批次边界返回 interrupted，用户取消返回 cancelled，publishing 完整提交。无目标的重建明确成功，不伪造消费或新增 data_revision。

主窗口新增 start_job / get_job / list_jobs / cancel_job 受限 IPC 和“恢复作业”面板，可查看真实进度、错误、请求取消及发布不可取消状态。操作错误在轮询中保留；未连接时禁用重建，保留明确失败状态。当前 start_job 只开放已实现的 rebuild，其他作业类型在对应模块接入，拒绝创建无人执行的请求。

新增三项后台执行器 / 一项队列集成测试与作业浏览器场景；全部 73 项 Rust 测试、Clippy、fmt、生产构建和三个浏览器场景通过。浏览器作业场景明确使用测试 fixture，验证超 i64 字节、回复丢失的幂等键、取消 accepted 至最终 cancelled。诊断页连接失败状态完成浏览器视觉检查。2026-10-01 独立 Windows 10 native probe 返回 0，真实 WebView 新增 list_jobs 检查及既有生命周期 / 电源路由回归；probe 现在每次使用新临时应用数据子目录，避免读取已启用的开发来源。退出仍有 WebView2 class unregister 1412 提示。

尚需 M07 后续：完整镜像规范身份 / 共同来源证据、替换文件候选代次、持续追加追平、自动重建触发及强杀恢复。作业 UI 的接入不代表七个统计页面已完成；M08 查询与 M10 页面继续按真实 DTO 实施。

## M07e1：安全 v2 升级与显式周期指针

增加不可变 0002 migration，v1 SQL / checksum 保持原样。升级前 SQLite Online Backup 分页复制已提交 WAL，检查 integrity / 结构版本，再关闭并以只读连接复核，流式计算 SHA-256 和同步保存 manifest；备份锁等待有五秒无进展上限。v1 → v2 DDL、迁移 checksum、user_version / app_state 和外键校验在一个事务中完成。新空库直接初始化 v2，不制造旧版备份；当前版重开不重复备份。

stream_frontiers 用组合外键固定每个流当前有效周期，迁移按已证明的旧观察顺序回填。正式批次与候选批次在基线同事务更新指针，读取不再遍历并覆盖所有历史周期。当前物理单序列更新以观察顺序拒绝倒退；跨镜像规范序列随后使用已准备的规范 ordinal / cursor。v2 另加入规范别名、候选别名、规范序列 / 文件游标结构及父关系 / 绑定查询索引，这些结构尚未代表镜像业务已完成。

五项迁移测试和一项周期指针测试通过：一致备份 / 配置保留 / 幂等重开、三个升级故障边界、checksum / 备份目录失败、旧周期回填 / 外键、未 checkpoint 的已提交 WAL，以及逆序写入仍选择新周期。Rust 全套 78 项通过后新增 WAL 场景单独通过，总计 79 项；Clippy、fmt 和协议差异检查通过。2026-10-01 Windows 10 native probe 在新 v2 隔离库启动、真实 IPC 与生命周期回归通过。实际升级强杀和安装升级仍待灾难 / 部署验收。

## M07e2：镜像规范规划与首次导入竞争保护

新增纯物理序列规划器，优先保留唯一既有规范序列 / 可信历史的稳定逻辑会话键。镜像逐项验证有序前缀；规范前缀使用稳定主来源元数据，唯一可证明的较长后续使用对应来源。两个与短主序列一致但互相冲突的后续都隔离，不凭到达时间或长度选择。未知身份、未完成前序、未知时间和创建时间冲突保持明确分类，不通过指纹相同直接合并。

首次导入先经单写线程注册会话，再核对相关身份，避免两个并发来源都在注册前看到空集合并各自计费。关系暂未证明时允许双方都 pending，由候选归一收敛。十轮真实文件 / SQLite 并发测试均未产生双重消费；四项纯规划测试独立列出规范来源与冲突预期。全部 84 项 Rust 测试、fmt 和 Clippy 通过。此阶段规划器尚未接入别名发布 / 共同来源证据和持续镜像游标，仍不宣称完整镜像导入已交付。

## M07e3：存储快照中的镜像证明与候选写入授权

存储层直接从固定 manifest 内的必要观察按逻辑身份 / 物理代次恢复序列，依据实际头部、完整消费上界和既有可信消费选择规范规划。签名与观察 ID 总预算 128 MiB，进入纯规划器时转移签名所有权；不复制正文或全部观察元数据。证明摘要保存成员数量 / SHA-256，不按每个别名重复整张成员列表。

后端自行计算候选别名，再经单写线程重新验证作业状态 / 输入版本后写入；调用者不能提交任意别名对。部分文件冲突的同一逻辑身份禁止局部合并。候选账本仅在 running 状态、当前作业拥有该 candidate、且别名证明指向其 owner 时，允许引用其他来源观察；活跃账本仍保持严格会话边界。取消和失败不发布别名或改变消费。已写入派生记录后禁止重新规划。

两个合成 rollout / 实际 SQLite 集成测试通过：稳定主身份与较长后续、证明幂等 / 候选不可见 / 无关观察拒绝 / 取消保护，以及冲突后续隔离 / 输入推进失效。全部 86 项 Rust 测试、fmt 和 Clippy 通过。规范重放、共同来源 provenance、别名原子发布和持续游标仍在下一模块接入；目前正式重建执行器继续原有保守隔离行为。

## M07e4：规范重放、共同来源证据与别名原子发布

正式后台重建已接入存储证明。按规范 ordinal 选取稳定主来源前缀和已证明的较长后续；观察 ID 分页保持调用者顺序、每页 256 条 / 16 MiB。逐条核算后保存包含无消费分类的规范序列，其余镜像观察记 duplicate；实际消费事件通过 provenance 保留各来源证据。归一后的父候选参与既有分叉继承证明，镜像父不会制造多个父候选。冲突 / 不完整物理序列单独 pending。

完成对齐时重新从读快照计算证明，Writer 再核对 manifest，逐项验证规范来源、保存文件游标，并按规范 ordinal 重建当前周期指针，避免跨来源观察写入先后影响周期选择。完成后封存候选，拒绝追加派生记录或 ordinal。镜像发布要求完整规范序列与对齐完成，别名的候选账本必须为空。

发布在已有组事务内切换账本、规范别名、必要观察的派生会话键、父键与 reader_context。保留原物理文件绑定并增加规范绑定；原始用量指纹、物理位置、旧账本 / 原 provenance 和审计保留。旧读事务同时保留发布前的别名 / 观察键。共享账本的实时追加暂时进入 pending，待持续 ordinal 对齐模块接入后即时归类；手动 / 后台已排队重建可以收敛这些追加，不重复消费。

新增两个执行器场景与一个事务故障场景通过：主 110 + 25 / 镜像新增 13，总量 148、三项消费与五项来源证据、来源过滤 135 / 148、原文件字节未变、重复重建稳定、追加后重建仅新增 7；镜像父 / 子继承及冲突后续；发布 COMMIT 前故障同时回滚别名 / 观察键 / 绑定 / 审计 / 指针、真实并发旧快照，以及封存后写入拒绝。Rust 全套 88 项通过后新增故障场景单独通过，总计 89 项；fmt / Clippy / 协议差异检查通过。故障为受控错误注入，实际强杀仍待验收。

2026-10-01 Windows 10 独立 native probe 使用隔离 v2 库通过真实 WebView IPC / 原生电源消息路由 / 托盘注册 / close-to-hide / 单实例激活 / 明确退出，返回 0。退出仍有 WebView2 class unregister 1412 提示。该检查不代替实际睡眠、任务栏嵌入、Windows 11 或安装升级验收。

## M07e5：已证明镜像的持续 ordinal 对齐

collector 在同一个读事务取得活跃账本、当前基线 / 修订、文件游标、规范长度和最多 500 项待追上的签名。已对齐来源的后续可推进共享基线；落后的来源逐项比较已有 ordinal，重复只补充 provenance / duplicate 分类，不重复消费或改变基线。缺父且未种入继承基线仍为 pending。分歧设置 rebuild_required，此后新记录继续 pending，保留已确认前缀与基线。正式采集中的零量也有明确分类，不产生消费事件。

CanonicalProgressWrite 与观察 / 消费 / 基线 / 物理检查点在同一 Writer 事务；比较 active ledger、期望规范长度和游标，复制再次核对实际原始用量签名 / 对应 ordinal，并由后端找到真正事件添加来源证据。规范指针按 ordinal 更新。来源 / 小窗统计可继续采用 provenance EXISTS 过滤，不进行会重复消费的 JOIN SUM。

别名发布现在还原子递增受影响当前文件的检查点修订，使发布前读取的旧 ReaderContext 不能写回已归一会话。collector 恢复持久上下文时解析规范别名；修订溢出拒绝发布。匿名身份使用独立类型标记，避免与真实 provider ID 的字符值形成同一规划桶。

新增一项事务故障测试、两项真实文件 / SQLite 增量场景，并扩展别名发布的旧上下文检查。四轮双线程 / 双来源各追加 600 调用、跨 500 记录边界竞争 / 重试，都得到总量 710、601 项消费、1202 项来源证据，零量 / 再次重建稳定；分歧不推进错误基线，后续继续 pending；注入错误 / 过期长度使消费 / ordinal / 游标 / 检查点一起回滚。自动重建触发、文件新代次与持续追加作业的有限追平继续实施，不以已对齐增量代替这些功能。

全部 92 项 Rust 测试、fmt / Clippy 和协议差异检查通过。此模块未改变前端协议或页面；Windows 10 运行壳的实际系统检查沿用上一模块记录，尚未增加实际 WSL / Codex 版本或大数据性能验收。

## M07e6：自动证明作业与无变化核对

扫描与工作队列处理完成后，collector 每秒最多评估一次镜像 / 已出现父候选 / rebuild_required 的待确认范围。依赖闭包固定规范代表，真实物理身份、已读上界、内容锚点与身份关系生成持久幂等键；不使用 data_revision、active ledger 或空核对的检查点修订作为新证据。闭包成员只计算一次，最多处理 32768 个候选身份；全局已有未完成作业时不额外排自动作业。后台 JobService 通过既有持久队列执行，完成后重新核对来源。

同输入的成功、明确失败或用户取消不反复排队。candidate_obsolete / interrupted 最多自动重试三次总尝试；输入证据变化则有新幂等键。原父关系不存在时不空转，父来源后来采集会扩大闭包并自动重建。停止 / 暂停的来源只用已保存必要观察，物理校验在访问路径前检查 enabled，不访问被停用的 WSL / 本地目录。

无变化核对仍校验代次、active ledger 与预期检查点；当全部派生记录为空、大小 / 偏移 / 锚点 / ReaderContext 都未变时，保持原检查点和数据修订。真实部分尾部、上下文或源内容变化继续更新版本，避免轮询空提交反复使候选失效。

六项新增测试通过：真实后台启动镜像自动收敛并保持源字节、迟到父自动得到父 135 / 子 13、未证明输入在四轮空核对 / 发布后只产生一个作业、用户取消 / 新证据 / 三次失效上限、无变化 / 上界 / 上下文 / 过期修订，以及停用源被覆盖后仍从必要观察恢复 110。既有候选失效测试改用真实新部分尾部上界，不再把空提交假设成输入变化。全部 98 项 Rust 测试、fmt / Clippy 和协议差异检查通过。

M07 仍需：被覆盖 / 截断 / 替换文件的候选物理代次与重新解析、真实持续追加时的有限追平 / 短冻结、强杀与大数据验收。已有核算 / 候选发布 / 自动归一接口可供 M08 查询使用；这些待办保留在完整交付范围内。

## M08a：日历分桶与时区边界

新增后端 hour / day / month 受控分桶与生成契约，UTC 半开范围保持精确毫秒，首尾截取不改变本地日历标签。重复小时使用不同 UTC 区间与 offset；半小时夏令时跳变按实际边界拆分，历史秒级 offset 保留秒数。日 / 月使用本地日历，跳过不存在的日期，不以固定 24 小时 / 30 天替代；按范围预限与最终 2000 bucket 上限拒绝异常请求。

八项独立预期覆盖纽约春季 23 小时 / 秋季 25 小时与重复小时、Lord Howe 双向半小时变化、尼泊尔 +05:45、历史巴黎 +00:09:21、闰年月份 / 半开端点、Apia 跳过日期、2000 上限 / 非法时区 / 不支持日期。此阶段尚未接入统计汇总与前端页面，不宣称主页面统计已可用。

全部 106 项 Rust 测试、fmt / Clippy、生成协议差异检查与 TypeScript 检查通过。此模块只增加纯算法和契约，未新增原生系统验收。

## M08b1：精确汇总与维度筛选

增加可在同一真实只读事务中复用的总量 / 模型 / 项目查询。所有筛选值绑定参数，来源筛选通过 provenance EXISTS，不因多份镜像证据放大消费；会话别名在当前事务解析。模型 key 由实际 provider（允许未知）与模型名生成，显示名称不参与 SQL 拼接，未知项与名为“未知”的实际维度保持不同 key。

Token 分项分别返回已知值、对应已覆盖消费和完整性；空集分项保持 null，分项缺失不填零。非缓存输入仅对 input / cached 都已知的事件相减。回合计数按 session / turn 组合去重，部分已识别回合明确不完整。金额尚未进入此查询模块。分组采用十进制长度 / 数字排序，保持超 i64 / 2^53 的正确次序。

六项 SQLite 集成场景验证部分 / 空向量、完整分项 / 回合、镜像三份 provenance、SQL 字符串筛选、同名不同 provider / 未知 provider、真实“未知”名称、超 i64 汇总与排序、半开时段、别名解析 / 并发提交中的旧快照和项目 alias。当前是后端查询原语，bundle、覆盖证明、租约 / 分页与正式 UI 继续接入。

全部 112 项 Rust 测试、fmt / Clippy、协议差异与 TypeScript 检查通过；本次未新增原生系统或生产规模性能验收。

## M08b2：同事务趋势与查询基准

趋势查询使用受控日历边界和当前只读连接专用的不可变二分定位函数，一次聚合当前范围事实事件。函数在所有 SQL statement 结束后释放，同连接切换范围 / 时区不复用上次边界；未匹配的空桶保留分项 null。查询计划检查确认事件有时间范围索引查找，不逐桶重复扫描活跃会话。总量、趋势和独立热力图范围可在 bundle 的同一读事务复用，仍待正式 bundle / coverage 组装。

针对 30 万事件实测，增加合成关系数据 benchmark：100 会话、10 模型、每分钟一条已知向量、209 个 UTC 日桶，无用户源文件或认证数据。先将 15 次逐分项聚合调用合并为一次五字段精确向量聚合；允许未知 / 已知零，仍验证包含关系、checked i128 总量和各项覆盖。无需模型筛选的查询不读取 origin 元数据；模型分组在 provider / model 原始身份上聚合后生成 opaque key，未知模型统一 null 分类。独立 SQL 场景核对超过 i64、真实零、空 / 部分字段和非法向量拒绝。

初版 release 实测热 P95：总量 4042 ms、模型 4411 ms、日趋势 2253 ms。本机 Windows 10 19045、i5-12600KF、31.82 GiB；首次读取与五次热样本分别记录，P95 使用 nearest rank（五个样本等于最大值）。这些是合成数据库查询测量，不是 Windows 11 / 四核 / 16 GiB 标准验收，也不包含完整 bundle、mini、峰值内存 / CPU、1 GiB 日志 / 1 万文件、WSL、分页或并发写入压力。未清空操作系统缓存，因此“首次读取”不等于磁盘冷读。

最终查询版本的同一数据集测量：

|查询|首次读取 ms|热 P50 ms|热 P95 / 最大 ms|
|---|---:|---:|---:|
|总量|1146.010|1089.289|1116.224|
|10 模型汇总|3343.981|3305.962|3392.449|
|209 日趋势|1166.175|1172.075|1230.861|

测量前 WAL 226764832 字节（约 216.26 MiB），不代表交互租约可允许的大小。正常 WAL 回收与超过 64 MiB 的租约回收继续在租约服务实施。数字独立预期通过，查询性能尚未达到 mini P95 ≤150 ms / 主页面 ≤500 ms：需要依据总设计第 9 节增加可重建 UTC 小时汇总缓存，并对筛选 / 镜像来源 / 重建切换 / 可靠回合单独验证，不能用此报告宣称性能验收通过。

重测命令：`cargo test -p token-pulse-store --release query::tests::benchmark_300k -- --ignored --nocapture`。普通回归跳过耗时 benchmark；测量必须明确单独执行，避免把 ignored 记作普通自动通过。

117 项普通 Rust 测试、fmt / Clippy、协议差异与 TypeScript 检查通过；30 万事件 release benchmark 已单独执行并通过精确预期。未新增原生系统检查，正式主页面仍待 bundle / coverage / IPC 接入。

## M08c1：可重建汇总的版本与 v3 存储

新增不可变 0003 migration，保持 v1 / v2 SQL 与 checksum 原样。账本级 usage revision 由 SQLite trigger 在事件 / provenance 的插入、更新、删除，以及来源观察 provider 改变时同步推进；重复 provenance 的 DO NOTHING 不变更版本。候选 / 正式批次仍使用既有事实事务，版本与观察 / 消费 / 基线 / 检查点一起回滚。空核对不更新版本；旧只读快照保持当时的缓存输入版本。

新增独立 usage_rollup_sets、UTC 小时 / 元数据 / 来源集合 cohort、可靠 turn membership 表。候选必须显式 building，ready 必须有发布时间，小时边界、JSON 类别、事件 / 回合数量和组合外键有数据库约束；候选删除级联清理派生行，不更改事实账本。缓存按 evidence revision、parser / accounting / cache version 识别，不以当前 settings 覆盖旧事实。

本模块只提供缓存版本 / 候选存储基础，尚未构建或使用 ready cache，查询延迟仍沿用 M08b2 实测，不宣称性能目标已达成。下一模块实现有界构建、候选验证 / 原子发布与同事务读取，保留原始查询回退。

新增 v2 → v3 一致备份 / 原账本配置保留、三个失败边界、版本与真实旧快照、无效候选与 cohort / turn 外键场景，并把原有全部批次故障注入扩展为同时验证缓存版本回滚。v1 → v3 的既有迁移回归一并通过；实际升级强杀 / 安装升级仍需后续灾难和部署验收。

全部 121 项普通 Rust 测试、fmt / Clippy 与协议差异检查通过。2026-10-01 19:13 Windows 10 独立 native probe 在隔离 v3 库通过真实 WebView IPC、电源消息路由、托盘注册、关闭隐藏、单实例激活和明确退出，返回 0；退出的 WebView2 class unregister 1412 提示仍存在。本次不包含实际睡眠、WSL、任务栏、Windows 11 或安装验收。

## M08c2：缓存候选构建、核对与发布

新增独立后端 UTC 小时构建器。一个事实读快照固定活跃账本、evidence revision 和算法版本；流式读取消费，按小时 / provider / 模型 / 项目 / 去重排序来源集合聚合，保存精确分项、覆盖、事件数及可靠 turn membership。未知模型的 provider 不制造多个未知分类。负时间使用欧几里得小时边界。独立 SQL total / count 校对输出，空账本显式构建零行缓存，不制造消费。

候选元数据与行采用既有 Writer，每任务最多 500 行 / 16 MiB，turn membership 单独分批；32768 个 cohort 和 64 MiB 内容 / 元数据预算防止无界积累。读取每 256 记录与阶段间检查停止标志；开始发布后完成当前原子事务。每批核对活跃账本与 evidence revision，版本变化放弃候选。发布在单事务再次核对版本、持久行 / turn 的完整内容 SHA-256 后标 ready；失败不使用部分候选。内存预算为保守记账和内容预算，实际峰值仍需性能测量，不能把 64 MiB 数值当实测进程峰值。

ready_set 在当前读事务按精确输入版本 / parser / accounting / cache version 查询。旧快照看不到后来发布的缓存；后续证据变化使旧 ready 不再匹配。重复构建已完成输入直接返回已有结果，building 冲突拒绝并发接管。程序启动在 worker 前将遗留 building 标 obsolete，同输入重试先删除部分派生行；不修改事实、源文件或 data_revision。

七项合成 SQLite 场景通过：精确分项 / 可靠 turn / 来源集合、候选篡改拒绝 / 过期版本、停止 / 冲突 / 重试、602 个 cohort 跨 500 行、超过 i64 与未知向量、负时间 / 空候选、COMMIT 前错误 / 新旧真实快照，以及共享镜像证据只保留一份 110 消费。这些为自动验证，实际构建强杀、内存峰值、生产大数据与后台取消仍待后续测量。

此模块可构建缓存并安全查询 ready 标识，尚未把统计查询切到缓存或启动后台构建服务。保留 M08b2 的原始查询回退和性能待办；下一模块接入同快照缓存 / 事实边界合并、自动构建与重测，不以可构建候选代替页面性能交付。

全部 128 项普通 Rust 测试、fmt / Clippy 与协议差异检查通过。2026-10-01 19:34 Windows 10 独立 native probe 通过隔离 v3 数据库启动、真实 WebView IPC、电源消息路由、托盘、关闭隐藏、单实例激活和明确退出，返回 0；退出仍出现既有 WebView2 class unregister 1412 提示。尚无安装、真实账户、任务栏 / Windows 11 或实际休眠的新验收。

## M08c3a：缓存分项的精确合并

增加 sum_usage_projection 与可复用分项合并器，为完整 UTC 缓存区间与事实边界合并提供运算。总量 / 各项值 / 已覆盖消费使用 checked i128；不同 fragment 的完整性逐项保留，空 fragment 不把未知变为零。缓存内容按严格内部格式解码，覆盖超过总量、完整项无值、无事件却存在分项、负事件数、破坏格式和数值溢出明确拒绝。

两项独立 SQL 场景验证完整 110 + 未知 7 + 空 fragment 得到 117 / 已知输入 100 / 覆盖 110 / 分项不完整，独立未知仍 null、完整单片仍 complete，以及两个 i64::MAX / 已知缓存零 / 各类损坏拒绝。存储层全套回归、fmt / Clippy 通过；此次只增加运算原语，没有改变页面或启用缓存读取，前一模块实际 Windows 10 运行壳检查仍适用。

更严格的溢出断言发现 SQLite 将回调错误转换为 SQLITE_ERROR / 精确错误码消息。StoreError 现在仅在该受控路径解析已知 ErrorCode，保留 NUMERIC_OVERFLOW；普通 SQL 错误仍返回 DB_WRITE_FAILED，不把任意数据库消息带到 UI。新增断言验证两个 i128::MAX 的溢出代码与普通未知表错误的区分。

修复错误映射后完整 130 项普通 Rust 测试与 Clippy 通过，耗时 benchmark 仍单独运行；此次没有新增页面 / 系统验收。

## M08c3b：同快照总量缓存与事实边界

总量查询只采用同一读取事务中活跃账本、证据修订、解析 / 核算版本均匹配的 ready 缓存。完整 UTC 小时合并缓存，首尾不足一小时及未缓存 / 已失效账本从事实读取；没有可用缓存时保留原始查询。来源筛选使用已证明 provenance 的集合，模型使用 provider / model 键，所有筛选仍绑定参数；会话别名在该快照内解析。

缓存与事实片段通过 checked i128 合并，分项未知与覆盖独立保留。会话 ID 与 (session_key, turn_id) 分别 UNION 去重，避免跨小时重复回合或不同会话相同 turn ID 被误合并。事件数 / 已知回合事件数使用十进制字符串聚合，不转换 REAL。

四项缓存与原始事实 DTO 的独立对照通过：不足一小时的首尾 / 半开上界、共享镜像来源 / 模型未知 / 参数注入、并发追加后的新旧真实快照与缓存重建，以及已缓存 / 未缓存账本混合的会话和回合计数。全部 134 项普通 Rust 测试与 Clippy 通过。时间序列 / 分组查询及自动后台缓存构建仍待接入；尚未进行缓存路径的 30 万事件性能重测，M08b2 的性能未达标记录继续有效。此模块没有新增 UI 或原生系统验收。

## M08c3c：模型与项目分组的缓存合并

总量与分组查询共享事务内的缓存 / 事实划分和分项合并。模型键保留 provider 边界，未知 model 合并为独立 null 组；项目键独立于显示名，用户别名在读快照内取得。按总量排序继续比较十进制长度与数字，按名称使用确定的二进制顺序，最后以稳定键打破同值排序。每组的会话与回合分别去重，空缓存不创建虚构分组。

现有四组缓存测试扩展为模型 / 项目、两种排序的全部 DTO 对照；新增两项独立预期验证两倍 i64::MAX 排序、同名不同 provider、真实会话别名、两个同名“未知项目”、空 ready 缓存、负 epoch 和不足小时边界。18 项查询测试、完整存储层回归及 workspace Clippy 通过；当前普通 Rust 场景累计 136 项，本模块没有重新运行全部 collector 场景或新增系统验收。自动后台构建、时间序列缓存和性能重测继续实施。

## M08c3d：时间序列缓存与本地日历边界

小时 / 日 / 月时间序列复用分区缓存合并。临时 SQLite 函数用不可变本次日历边界判断完整 UTC 小时是否落在同一个本地时间桶；只有符合条件的小时使用缓存，其余保留事实。时间桶首尾、分数小时偏移、DST 切换不会用一个小时的总数近似拆分。两个函数均限定当前独占读取事务，作用域结束移除；重复请求不沿用上次时区 / 范围。

新增两项多场景测试，独立 Windows DateTimeOffset 转换固定事件时间，逐桶 DTO 对照原始事实。覆盖纽约重复小时的 0 / 17 / 23 / 0、Lord Howe 半小时 DST、Kathmandu 45 分钟时区且小时缓存不可用、半开上界、日 / 月混合边界与来源 / 未知模型过滤；并发追加后旧快照仍为一月 24 / 二月 23，新快照一月 29。20 项查询测试、完整存储层回归及 workspace Clippy 通过，累计普通 Rust 场景 138 项。后台缓存构建、快照租约 / bundle 和性能仍待完成，没有新增 UI / 系统验收。

## M08c4：串行后台缓存与分批清理

桌面运行层启动独立缓存 worker，默认空闲每 5 秒检查，每次只从 SQLite 取一个需构建的活跃账本；不维护无限队列 / 全部账本的内存副本。持久重建作业优先，已有 queued / running / publishing 时不另构建。历史缓存任务间让出最多 250 ms；退出先发停止信号并 join 缓存 worker，再结束作业与 collector，构建仍使用既有安全边界和版本校验。

确定性的超限 / 损坏 / 溢出构建失败在实际输入版本下保存 failed 元数据，空核对不重复尝试；新证据使用新输入键，显式构建可重试。旧 / 失败缓存每个 Writer 任务先删最多 500 个 turn 成员，再删最多 500 个 cohort，最后最多清理 100 个过期空元数据；不删除消费、观察或检查点。保留当前失败标识，遗留 obsolete 部分内容清完后才自动重试，避免一次大 FK cascade。

三项服务测试通过：自动首次构建与追加后只构建一次、重建作业优先 / 幂等关闭、603 个 cohort / turn 的 500 / 103 / 500 / 103 / 1 清理边界、旧真实读取快照仍可用 / 原事实 603 项与 712 Token 不变、失败标识保留 / 显式重试，以及 32769 小时超限后稳定 failed / 实际时间证据改变后成功。完整 141 项普通 Rust 测试、fmt / workspace Clippy 通过。

2026-10-01 20:32 Windows 10 独立 native probe 通过隔离 v3 库 / 新 worker 启动、真实 WebView IPC、原生电源消息路由、托盘、关闭隐藏、单实例激活及明确退出，返回 0。退出仍有既有 WebView2 class unregister 1412 提示。此检查没有模拟真实账户、WSL、安装、任务栏或睡眠；内存峰值 / 强杀与 30 万查询性能继续待测。

## M08c5：30 万事件缓存路径重测

同一 ignored release benchmark 增加显式 `TOKENPULSE_BENCH_CACHE=1` 开关；事件时间、100 个交错会话、10 个模型、300000 项消费、独立总量 / 每模型 / 每日预期及六次读取不变。构建 100 个当前账本缓存用时 24419 ms，产生 300000 个小时 cohort 行，未压缩该夹具的行数。第一次读取及随后五次热读取如下（毫秒；P95 为五个热样本的 nearest rank / 最大值）：

|查询|第一次读取|热 P50|热 P95|
|---|---:|---:|---:|
|总量|1715.888|1663.668|1800.779|
|模型分组|2527.224|2592.705|2644.175|
|UTC 日序列|2073.091|2097.308|2107.587|

缓存构建与查询的精确预期通过；该配置尚未达到主页面 500 ms / mini 150 ms。总量 / 日序列相比 M08b2 原始查询反而更慢，不能将“命中小时缓存”当作性能验收成功。后续需依据压缩密度选择查询路径，并增加较大时间粒度 / 汇总与安全查询缓存；性能交付仍未完成。WAL 测量为 226781312 字节；这是现有机器上已打开读池的第一次读取与五个热样本，没有 OS 冷缓存、标准 4 核 / 16 GiB Windows 11、完整 bundle / mini、私有内存或空闲 CPU 验收。

## M08d：最近上下文查询与正式 IPC

`get_context_snapshot` 已实现，按真实读事务内的会话别名与 active ledger 读取最近上下文；相同时间按稳定 ID 排序，不读取 retired 值，不用日期消费累计量代替上下文。已有会话但无上下文时，Token / 容量 / 百分比 / 时间均 null，quality 为 unknown。已知零保留零；缺容量时百分比 null；占用超过容量显示实际近似百分比，精确计数仍以十进制字符串返回。

Rust 命令在主窗口权限清单和调用 label 双重限制，阻塞查询运行在 spawn_blocking，TypeScript 通过生成 DTO 接入调用；参数与错误不携带源内容 / SQL。新增三项多场景自动测试验证上下文 1000 / 2000 = 50% 与消费 110 独立、字段缺失 / 已知零、超过 2^53 的精确字符串、并发修改别名与快照值、150% 占用、确定同时间顺序及损坏 / 非法请求。完整存储层回归、workspace Clippy、TS 类型检查和协议差异检查通过；普通 Rust 场景累计 144 项，未在本模块重复全部 collector 回归。

2026-10-01 20:42 Windows 10 独立 native probe 使用隔离库内无来源的合成会话，真实 WebView 调用新命令得到完整 null DTO，并通过既有启动 / 电源消息 / 托盘 / 关闭隐藏 / 单实例 / 退出检查，返回 0。测试没有配置真实来源或读取用户日志，仍有既有 class unregister 1412 提示。会话详情 UI、覆盖状态与 bundle 后续接入。

## M08e：查询覆盖缺口与未归属量

覆盖与同事务内可信消费分别计算。active ledger 的 pending / unattributed 观察按日期与维度绑定筛选；无观察时间的缺口始终保留，半开上界 / 其他日期、candidate / retired、duplicate / inherited 不成为本范围待确认消费。未归属向量只有全部可解释的 total 才以 checked i128 相加，任一缺失 / 无效向量使总量 null，不计入可信消费。

待读文件、来源可读性与未解决格式问题按所选来源整体报告；未知事件日期 / 模型的文件错误不能因筛选某个模型而消失。格式发生次数采用精确聚合；最近成功时间可缺失。已完整解释的分项仍报告自身 complete，与来源覆盖分别表达。当前 Collector 尚未发出可验证的完整扫描 manifest，查询没有 complete 路径：存在明确缺口为 partial，否则为 unknown，并报告 scan_evidence_missing 等原因。后续需落地扫描 manifest，不能把正常读取或零消费当作完整扫描证明。

四项多场景测试验证缺 manifest / 已知分解 / 空来源选择、未知时间与半开边界、候选隔离、同一调用的继承 / 镜像分类、两倍 i64::MAX 的未归属量 / 任一未知或无效向量、实际源字节上界落后、跨日期格式错误 / 解决状态、模型过滤下来源缺口及并发修改前后的真实快照。完整存储层回归、fmt / workspace Clippy 通过，累计普通 Rust 场景 148 项；本模块没有新增 UI / IPC 命令或系统验收，覆盖函数供后续同快照 bundle 使用。

## M09a：精确价格核心（提前满足 bundle 依赖）

为 M08 完整 dashboard / grouped DTO 的 pricing 字段，先实现不依赖 IO 的 PriceCatalog。规则 / 受控模型别名在捕获的 price revision 下固定；索引按实际 provider / 模型匹配，不做相似字符串猜测。event_time 与指定估价时点明确分开，生效上界不包含；来源自定义高于全局自定义，再高于离线规则，各层内按 priority。同一层 / priority 的多个命中返回 ambiguous_rule，镜像实际所有证据来源参与选择，不随 UI 来源筛选改变规则身份。

费率按每百万的 10^-9 原子解析，单事件费用用 checked i128 的 10^-15 原子计算。仅非缓存输入 / 缓存输入 / 完整输出收费，推理包含在输出内。缓存未知且两种输入费率相同可计算；需要拆分时保留 insufficient_usage。已知缓存零不要求缓存单价；缺少必要父分项、模型、规则或发生溢出各有明确 unpriced 状态。未计价 DTO 没有虚构零金额。

九项纯核心测试及 15246 个独立整数公式组合通过：9 位费率边界 / 最小 10^-15 金额、40 普通输入 + 60 缓存输入 + 10 完整输出的合成估价 0.000220500000000、未知缓存 / 已知零、半开时间 / 价格版本、三层规则 / 镜像歧义、受控别名、超过 2^53 的精确金额、损坏 / 溢出与字段校验。合成价格只存在测试；生产未装入任何虚构价格或官方价格种子。

完整核心回归、workspace Clippy、TS 类型与六项 Vitest 契约测试通过；新增 Rust DTO 已生成 TypeScript / JSON Schema，累计普通 Rust 场景 157 项。本模块没有新增系统 / 页面验收。存储侧不可变价格写入、范围重叠拒绝、实际规则来源 / 覆盖、重估、同快照 bundle 和价格 UI 仍须继续实现。

## M09b1：价格规则的不可变保存与版本快照

新增价格草稿、受限创建 / 替换 / 退休变体和规则快照 DTO。草稿不接受 rule_id、origin 或发布修订；后端生成规则 ID，用户写入固定为 custom，不能把自定义标为离线官方规则。保存以 expected price revision CAS 开始，在同一事务退休旧行、验证新范围、插入新行并递增 price_revision；不会改写旧费率或 data_revision / Token 事实。

同 provider / model / source / origin / priority 的有效期重叠返回独立 PRICE_RULE_CONFLICT；相邻半开区间、不同优先级与来源覆盖可共存。来源必须已存在，活跃规则最多 4096；版本快照同样以显式边界读取，不静默截断。当前 / 旧 price revision 按 introduced / retired 条件分别加载规则与受控别名。规则管理的正式 IPC / UI、别名写入与离线实际规则导入仍未接入。

七项存储测试通过：替换保留原费率及真实旧读取快照 / 在新事务按旧 revision 复算 900 原子、当前 revision 40500 原子；重叠 / 邻接 / 来源 / 优先级；验证失败、SQL 插入失败与 COMMIT 前受控错误一起回滚退休和价格修订；两线程编辑 CAS 仅一个成功；退休 / 过期请求；i64 修订溢出 / 损坏货币；旧别名版本与 4096 活跃规则上限 / 满额替换。

完整存储层回归、fmt / workspace Clippy 通过；新增专用冲突码后七项价格场景再次通过。TS 类型和七项 Vitest 契约测试通过，验证草稿不能冒充 offline / 指定 introduced revision、不能给退休动作附草稿、priority 范围受控。累计普通 Rust 场景 164 项，没有新增系统验收，测试价格始终为明确的合成夹具。

## M09b2：正式价格命令与事务内发布响应

get_price_rules / save_price_rule / retire_price_rule 已加入主窗口 capability 与命令 label 检查。读取支持当前或不晚于当前的历史 price revision；保存仅接受 create / replace，退休使用独立受限命令。修订以十进制字符串传输并检查 i64 边界，阻塞任务移出 WebView 线程。保存返回发布事务中取得的规则配置快照，避免成功写入后另读一个并发版本；提交后发送只含 price revision 的 PriceChanged 事件，通知失败不回滚配置。

新增存储场景验证响应保留自身发布状态、随后替换不改变响应中的规则 / 修订、合法历史 revision 与非法未来 / 负 revision。八项价格存储测试、workspace Clippy、TS 类型 / 七项 Vitest 与 DTO 生成通过，累计普通 Rust 场景 165 项。

2026-10-01 21:20 Windows 10 独立 native probe 在随机隔离库中通过真实 WebView 的空规则读取、明确合成价格创建、重叠 PRICE_RULE_CONFLICT、替换、按旧 revision 读取原费率、退休为空，以及既有上下文 / 启动 / 电源消息 / 托盘 / 关闭隐藏 / 单实例 / 退出检查，返回 0。合成规则没有写入生产目录，未读取真实来源 / 账户；仍有既有 class unregister 1412 提示。价格设置 UI、规则别名写入 / 离线目录、消费定价汇总和重估仍待接入。

## M09c：正式价格设置页

保留原型七项主导航与五项设置页签，在价格页接入正式 get / save / retire DTO。默认空规则，不预填虚构单价；支持自定义提供方 / 确切模型、来源覆盖、货币、显式 UTC 半开有效期、优先级、价格依据，新增 / 不可变替换 / 退休以及历史版本只读查看。编辑始终使用开启编辑时的 price revision；手动刷新不会改写未保存字段或悄悄升级写入前提，CAS / 重叠冲突保留表单。

前端使用 BigInt 在每百万单价与 10^-9 原子间转换，9 位小数与 i128 上界精确验证；空缓存价保留 null，已知零单独显示。UTC 日期拒绝被 Date.parse 正常化的非法日期，替换保留毫秒边界；Token / 金额不转浮点。长单价保持完整一行，小尺寸表格在自身容器滚动。首次浏览器检查发现 StrictMode 清理后旧请求导致加载锁保留的问题，已用请求序号与挂载清理修复并通过回归。

2026-10-01 21:34 TS 类型、10 项 Vitest、5 项 Playwright 与生产 Vite 构建通过。明确合成 IPC 夹具验证 9007199254740993.000000001 精确往返、null / 零、UTC .123、替换 / 历史读取 / 退休、版本冲突 / 重叠拒绝及未保存内容保留；查看 1280×860 编辑 / 列表与 960×680 列表 / 空态截图，页面没有横向溢出。此模块 UI 验证使用浏览器测试桥，不代表原生窗口逐控件验收；真实 Windows 10 价格命令流程已在 M09b2 验证。本模块没有修改 collector / 账本，别名写入、实际离线价格目录、费用汇总、重估与其他设置仍待实现。

## M09c2：补齐已确认的单价业务上限

复核 data-storage.md §1 时发现 M09a / M09c 只限制了 i128 / 9 位小数，遗漏既定每百万单价 `[0,1000000]` 上限。现已在 Rust rate_atoms、PriceRule / Draft、PriceCatalog 与 UI 输入一致拒绝超限；SQLite 载入超限行作为 DB_CORRUPT，不继续估算。没有调整需求或改写原始 Token。

新增核心 / 存储测试验证最大允许单价、超过一个 10^-9 原子、输入 / 缓存 / 输出三项、创建不推进 price revision、替换完整回滚退休及旧费率、损坏行拒绝。单事件溢出夹具改用输入 + 输出超过 i64 的非法向量，大 Token 费用精度场景仍保留。M09c 先前的超大单价 UI 场景已改成拒绝且不发布，合法 999999.123456789 保留 9 位精度，空缓存 / 零与 UTC 毫秒等回归不变。

2026-10-01 21:39 完整 workspace 167 项普通 Rust 测试、fmt / Clippy、生成契约差异检查，TS 类型 / 10 项 Vitest / 5 项 Playwright 与生产构建通过。没有重新进行原生系统检查（生产 IPC 仍使用同一已测写入入口），本模块不宣称计价 / 整体交付完成。

## M09d1：分币种精确费用汇总核心

新增纯 PricingAccumulator，流式累计事件估价的原子金额、对应可信 Token 与五类未计价原因 / 次数，不保留全部事件。币种使用独立 subtotal；全部未计价或空范围不创建零金额，已得到真实零费用的币种仍保留零。最终金额保留 15 位小数，展示前不逐事件舍入。金额 / 覆盖 / 次数 checked i128，任一步溢出返回 NUMERIC_OVERFLOW 且当前事件不改变已有汇总。

三项独立预期自动测试通过：USD 1 + 2 原子为 0.000000000000003、EUR 真实零单独返回、超过 2^53 的 Token 与原因次数、全部未计价、隐私 redacted / 金额 null，以及合法单价上限 × i64::MAX 消费累计至 i128 界限后的真实溢出 / 失败状态不变。完整核心测试与核心 Clippy 通过，累计普通 Rust 场景 170 项。尚未接入 SQLite 查询、估价缓存或 UI 统计；隐私处理只是纯 DTO 能力，最新跨窗口 PrivacyState 仍待实现。

## M09d2：真实读取事务中的费用查询

新增 SQLite 流式计价访问器，复用消费查询的绑定谓词与同事务会话别名，读取 active event 的确切 provider / model、必要向量和全部来源证据；规则 / 模型别名按同一捕获的 price revision 加载。来源筛选仅选中事件，不缩小规则匹配使用的来源集合，重复镜像观察不倍增费用。每条事件立即估价 / 累计，不把全部事实装入内存或前端；证据来源按已实现 32 项配置边界读取，第 33 项显式 DB_CORRUPT，避免静默截断改变匹配。

五项多场景测试通过：可信 158 Token 分为已计价 127 / 未计价 31，USD 900 原子 / EUR 真实零各自返回，三个未计价原因次数、模型 / 来源 / 半开日期筛选、两份镜像在单来源或合并筛选下都应用实际 source override、同级镜像规则歧义、真实旧 SQLite 快照在并发规则替换 / 模型别名与会话别名发布 / 新事件后仍保留旧费用、指定时点半开规则、全未计价与空范围、9007199254740993 原子精度及 33 来源越界拒绝。测试别名需真实 session / evidence job 外键，不绕过约束。

完整存储层 89 项普通测试（74 内部 + 15 集成；30 万 benchmark ignored）、fmt / workspace Clippy 通过，累计普通 Rust 场景 175 项。本模块仅新增可复用查询，没有新 IPC / 页面或系统验证；未计价缓存、price_revalue 作业、完整 bundle、分页与性能交付继续待实施，不能把流式计算等同 30 万事件性能通过。

## M08f1：时间桶内的实际覆盖缺口

为 dashboard 的趋势 / 热力图提供按桶 Coverage：已知时间 pending / unattributed 只进入实际 UTC 半开桶，未知时间使用独立累加器合并至每个桶；任一未知向量令对应未归属金额 null。来源不可读 / 暂停、格式错误和待读文件等整体缺口保留在每个桶。分项 complete 根据该桶消费计算，空桶不伪装完整扫描。扫描 manifest 未实现的限制仍保持 unknown / partial，不创建 complete 路径。

实现先获取公共来源状态，再用一次有绑定筛选的必要观察查询分桶，不循环 2000 次数据库调用或按标签归属日期；校验时间桶数量、连续性及完整范围。三项多场景测试验证局部小时缺口、UTC 上界排除、未知时间覆盖全部桶 / 未归属 5 与 11 的分开合并、null 向量 / 真实并发读取快照、来源缺口穿过空桶、非法桶序列拒绝，以及纽约夏令时两次 01:00 的 -04:00 / -05:00 与毫秒边界。7 项覆盖回归、fmt / workspace Clippy 通过，累计普通 Rust 场景 178 项；尚未新增总览 IPC / UI 或系统检查。

## M08f2：完整同事务总览 bundle

新增正式 DashboardRequest / DashboardBundle、扁平时间桶和 RecentSession DTO，并生成 TypeScript / Schema。一个 SQLite 读取事务获取修订、可信总量、价格 / 来源覆盖、趋势、独立日期范围的日热力图和最近 10 会话；主范围 / 热力图继承同一维度及显示时区，时间桶各最多 2000 项。价格目录加载一次，主范围一次流式估价同时累计总费用及最近会话费用，验证各价格覆盖 Token 与相应可信总量完全一致。

最近会话按范围内实际事件时间降序、session_key 升序；同时间最新事件按稳定 event_id 选取，项目显示取该事件的项目 / 当前别名，不用会话最终 cwd 或最终模型替代。meta 的 parser / accounting versions 来自主范围与热力图实际活跃事实，空范围为空；snapshot_id 是响应身份，没有假冒分页租约。

五项自动测试验证主范围 110 / 热力图另一日 7 / 空小时 null、所有组件来源筛选、同事务并发规则替换 / 新事件仍完整保留旧 JSON 和 meta、真实新快照的 117 与费用覆盖、12 会话限定稳定 10 项 / 超过 2^53 的精确消费、相同时间最新事件与项目别名、空范围 / 非法时区 / 超过 2000 桶 / 非法身份。完整存储层 97 项普通测试、fmt / workspace Clippy、契约生成差异、TS 类型及 11 项 Vitest 通过（含完整 bundle 必填 / null / 桶上限 Schema 检查），累计普通 Rust 场景 183 项。本模块未接入新的 IPC / 页面或原生检查，完整 bundle 性能仍待实际压测与优化。

## M08f3：总览正式 IPC 与 Windows 10 验证

get_dashboard_bundle 接入正式主窗口命令、capability 与 label 校验；request 在后台验证，阻塞查询移至 spawn_blocking，响应身份用于固定 bundle 标识。TypeScript 调用直接使用生成的 DashboardRequest / DashboardBundle，没有 HTTP / 任意 SQL 或浏览器伪造数据入口。

2026-10-01 22:11 Windows 10 独立 native probe 在随机隔离数据目录中通过真实 WebView 调用：空 Token 的可空分项、实际 price revision 3、同响应 snapshot_id、24 小时趋势 / 2 日热力图及各桶 unknown 覆盖、无消费会话不进入最近列表、空解析版本，以及不一致显示时区返回 INVALID_QUERY。既有上下文 / 价格版本 / 托盘 / 关闭隐藏 / 电源消息路由 / 单实例 / 明确退出检查继续通过，返回 0，仍有既有 WebView2 class unregister 1412 提示。

fmt / workspace Clippy、TS 类型 / 11 项 Vitest 和生产 Vite 构建通过。本模块没有访问真实 Codex 来源 / 账户，没有验证实际睡眠、WSL、任务栏或安装；主总览 UI / 筛选 / 查询租约、重估缓存及性能目标仍需继续实现。

## M10a：真实总览与共享日期 / 来源筛选

保持七项完整文字导航、顶部筛选、总览左侧消费 / 分解 / 趋势 / 活动与右侧最近会话 / 口径 / 账户区域，使用正式 DashboardBundle，无生产演示数据。可信 Token、分币种费用、未计价原因、null 分项、可靠回合、缺口与修订来自同一 bundle；分解不完整时不绘制完整条，缓存占比仅在输入与缓存覆盖完整时计算。账户尚未连接时明确显示状态，不从消费推算额度。

主窗口今日 / 近 7 日 / 近 30 日及实际来源状态跨导航共享。系统本地日历边界使用 calendar 日期运算，纽约 23 / 25 小时日与跨日刷新有独立前端预期；热力图使用独立 182 日范围。当前只支持系统时区，设置指定时区、自定义日期和三个高级维度候选仍待实施。刷新 / 可见时轮询通过请求序号与过滤身份阻止过期响应；切换范围立即隐藏旧范围，当前范围刷新失败保留旧快照并明确标记。趋势和每日活动可聚焦 / 选择查看完整整数及覆盖。

2026-10-01 22:42 生产构建、TS 类型、12 项 Vitest 和 7 项 Playwright 通过，浏览器检查已知 / 未知、共享筛选、日期粒度、热力图键盘详情、刷新失败及延迟旧回复。明确合成 IPC 夹具只在 tests/ui 中；查看 1280×860 与 960×680 全页截图，无横向页面溢出，不代表真实日志或费用算法验证。

22:43 Windows 10 随机隔离库的独立应用 native probe 验证这次正式 UI 构建的无来源引导、七项导航、日期默认值、无虚构消费，以及既有真实价格 / dashboard / context IPC、托盘、关闭隐藏、电源消息路由、单实例与退出，返回 0。仍有既有 WebView2 class unregister 1412 提示；实际睡眠 / WSL / 账户 / 任务栏 / 安装没有在此模块验收。模型、项目、会话、明细页面与查询租约继续待实施，不能据此宣称完整主窗口交付。

## M08g1：同事务模型 / 项目消费与费用分组

新增正式 GroupedUsageRequest / GroupedUsageBundle / PricedUsageGroup，保留既有精确 Token 分组查询，补齐同事务整体与每组的计价 / 覆盖 / meta。模型由提供方与实际模型形成 opaque key，未知模型聚成一个 null 分类；项目取实际事件项目和当前别名。流式计价一次累计整体与已返回组，校验价格覆盖 Token 与相应总量一致。每组覆盖独立查询必要观察，来源整体健康缺口不被维度掩盖。

limit 明确验证 1–200；实际 total_group_count 包含未知分类，truncated 报告未显示分类，整体汇总始终覆盖完整筛选，不把可见前 200 组当全量。当前排序支持可信 Token 降序 / 名称升序及稳定 key；尚无候选分页 / 组内钻取 / 费用排序 / IPC / 页面。模型别名规范化仍待后续契约完善。覆盖查询最多 200 组，未宣称满足 30 万事件性能目标。

2026-10-01 22:49 四项多场景自动测试验证整体 146、已计价 110 / 未计价 36、独立合成 USD 220 原子、未知 16 / provider 隔离、按组 pending、来源 / 日期 / 模型 / 会话筛选、项目别名 / truncation 与完整 128、真实旧 SQLite 快照在并发价格替换 / 新事实 / 别名修改后保持完整旧 JSON、18014398509482106 总量与 9007199254740993 排序 / 稳定并列、非法 limit / identity 和注入字符串不扩展筛选。完整存储层 101 项普通测试、workspace Clippy、契约差异、TS 类型及 12 项 Vitest 通过；Schema 验证必填费用 / 分类总数、行数 200 和 limit 范围。累计普通 Rust 场景 187 项，没有新增原生验收。

## M08g2：分组正式 IPC 与 Windows 10 独立验证

get_grouped_usage 已接入主窗口 capability / label 校验、请求身份、后台参数验证与 spawn_blocking；TypeScript 使用生成契约直接请求同事务模型 / 项目 bundle，没有动态 SQL / 自定义文件路径入口。

2026-10-01 22:51 Windows 10 隔离库的真实 WebView 分别验证 models / projects：实际 price revision 3、响应 snapshot_id、零可信总量的 null 输入、未知覆盖、空币种、零分类 / 未截断，以及 limit 201 拒绝 INVALID_QUERY；既有 UI 引导 / dashboard / 价格版本 / context / 单实例 / 托盘 / 关闭隐藏 / 电源消息路由 / 退出继续通过，返回 0。保留既有 class unregister 1412 提示。workspace Clippy、TS 类型和生产构建通过，本模块没有验证真实 Codex 来源或统计页交互。

## M10b：真实模型表格与项目列表

模型页保留原型表格，项目页保留列表，接入正式 GroupedUsageBundle。共享主窗口日期 / 来源，显示整体消费、分币种费用、消费会话、数量 / 排序、每组占比、独立费用 / 未计价覆盖与来源覆盖；模型缓存占比要求输入 / 缓存分项完整。未知模型 / 项目独立保留，空范围不填假费用。limit 支持 50 / 100 / 200，truncated 明确解释汇总含全部范围。价格覆盖按钮进入实际价格设置。项目路径详情、别名编辑 / 会话钻取、三个高级维度候选与费用排序尚未接入，本模块没有以无效控件模拟已完成操作。

总览与分组复用完整快照请求 Hook，维持筛选变更隐藏旧结果、同范围失败保留 / 标记快照、可见时轮询以及乱序防护；费用组件统一 null / redacted / 单 USD / 其他币种与精确金额 tooltip。redacted 只证明页面尊重 DTO，跨窗口隐私策略与服务端最新 PrivacyState 仍待实现。

2026-10-01 23:02 TS 类型、12 项 Vitest、9 项 Playwright、生产 Vite 构建通过。新增明确合成 UI 夹具验证超过 2^53 消费 tooltip、USD / EUR 分开及 EUR 真实零、未知缓存 / 未计价、主筛选跨模型 / 项目、空来源范围、真实规则入口、201 组显示 50 / 整体总量不截断和 redacted 所有金额 / tooltip。查看 1280×860 模型、960×680 项目 / redacted 模型截图，页面无横向溢出，模型表格自身容器可滚动；修正表内“未计价”与全局标题样式优先级使文字大小一致。

23:01 随机隔离库的 Windows 10 独立应用 native probe 用真实 UI 导航到模型 / 项目，两页查询完成后实际显示零可信总量 / 未计价 / 空范围，并返回总览无来源引导；既有正式 IPC / 系统路由检查通过，退出 0。随后只有表内字体样式调整并重新构建 / 浏览器检查；既有 class unregister 1412 提示仍存在。此模块没有验收真实 Codex 格式、WSL、实际睡眠、账户 / 任务栏 / 安装，不宣称主窗口整体完成。

## M08h1：进程随机密钥的受签名游标核心

新增 CursorSigner / QueryBinding / CursorClaims。OS 随机 256 位 HMAC-SHA256 密钥只在内存生成，快照身份为独立随机 128 位；固定 151 字符无 padding base64url token 使用用途域分隔，签名覆盖版本、快照、绑定哈希和最后位置哈希。绑定由可信窗口身份与完整请求（包括筛选 / 排序 / 价格依据）生成；真实最后键值留在租约内，token 不携带路径、显示名、正文或可解码的消费数字。

解析先限制大小 / 版本 / 编码，再验证完整性和窗口 / 请求绑定，非法统一 CURSOR_INVALID；窗口身份 64 字节、绑定序列化 128 KiB、最后 tuple 1 KiB 有界。依赖锁定 hmac 0.12.1 / subtle 2.6.1，base64 0.22.1 与 getrandom 0.4.3 使用当前锁文件版本。没有把游标签名当实际 SQLite 快照，租约连接 / TTL / WAL 回收、keyset 分页、候选与页面下一步接入。

2026-10-01 23:06 四项自动测试通过：确切超过 2^53 最后位置、不同最后 key / 筛选 / 排序 / 价格依据 / 窗口、所有 113 个载荷或签名字节逐个改写、重新生成密钥失效、空 / 十万字符 / padding / 截断 / 非法 / 非规范低位编码拒绝、绑定与位置界限、256 个随机快照身份不重复，解码载荷不出现夹具路径或原消费数字。workspace Clippy 通过，累计普通 Rust 场景 191 项；没有新增生产 IPC / 原生系统验收。

## M08h2：真实有界 SQLite 读取租约

Database 管理两条普通只读连接和两条专用租约 actor，应用查询连接总数固定四条；各 actor 独占连接，BEGIN DEFERRED 后首先读取 app_state，持有真实 Transaction 跨查询，而非仅复用 revision 数字。连接只读 / query_only，队列有界 8 项；两条租约占满时明确 SNAPSHOT_EXPIRED，正常 bundle 仍使用普通池。可信 owner + 完整查询绑定在租约侧再次校验，跨窗口 / 请求返回 CURSOR_INVALID。此阶段只提供内部 Rust 租约 API，签名 cursor 到服务器最后位置、分页 DTO / IPC 尚待整合。

总寿命固定 30 秒，空闲 10 秒；actor 每不超过 100 ms 核对并回滚 / 释放，刷新不延长总寿命。应用 WAL 文件超过 64 MiB 时提前终止；SQL progress handler 在查询执行时核对绝对期限 / WAL，并响应取消或退出，完成后的响应再次检查过期，避免返回越期结果。释放先取消 / interrupt，再排队结束事务；分配锁确保释放中断不会碰到复用该槽的新租约。清除 progress handler 后回滚，中断不会留下一个可复用的打开事务。

2026-10-01 23:25 完整 workspace 196 项普通 Rust 测试、契约差异检查和 Windows 10 隔离库 native probe 通过。随后新增执行中总期限场景，六项租约测试 / fmt / workspace Clippy 全部通过，累计普通场景 197：真实旧行 / 数量 / data-price-settings 三修订在并发 Writer 提交后不变、新普通查询可见新结果、两槽与普通池独立、只读写入拒绝 / 跨窗口 / 伪快照、客户端无请求时空闲回收及持续请求无法续总寿命、实际 WAL 写入压力、长递归 SQL 中取消 / 60 ms 测试总期限、中断后槽重用与活跃租约退出。测试总 / 空闲期限可缩短，只存在 cfg(test) 构造器；正式值未改变。

WAL 压力夹具首次在已打开连接上截断初始 WAL 得到 SQLite DATABASE_LOCKED；改为应用启动前用单连接完成夹具迁移并关闭，再打开正式四连接、执行实际增长验证，没有放宽阈值或替换保护实现。原生 probe 验证四连接启动与既有 UI / IPC / 系统路由，仍有 class unregister 1412 提示；没有页面租约 IPC 的系统测试。

WAL 缩小 / 后台 checkpoint 恢复机制、全局写入压力信号、按窗口批量取消、服务器最后位置生命周期与分页仍需后续接入。当前按物理 WAL 长度保护，超过阈值后新租约也拒绝，需完成维护回收以恢复分页；不能把当前基础服务视为候选 / 会话 / 明细分页已交付。

## M08h3：WAL 压力后的真实回收与租约恢复

新增 Writer 专属 checkpoint 封装，直接调用 SQLite sqlite3_wal_checkpoint_v2 针对 main 库；没有手动删 / 截 WAL 或替换主库。只在事务外调用，维护期间 busy timeout 为零，结束恢复正常五秒。BUSY / LOCKED 返回 Deferred，帧数负哨兵保留 null；其他失败返回受控存储错误并保留文件。退出用实际 PASSIVE 核对替换旧 PRAGMA 入口。

Writer 在空闲及持续作业之间至少按 500 ms 节奏检查物理 WAL；超过正式 64 MiB 时尝试 TRUNCATE。活跃普通读者仍可阻止截断，延后不等待前端；租约的压力终止释放后可在后续 tick 回收，恢复新租约。检查发生在受限任务之间，不打断或分割当前写事务。后台 checkpoint 状态 / 失败的诊断上报与额外全局写入压力仍待接入。

定位原 M08h2 夹具在当前运行库上 PRAGMA 截断的 DATABASE_LOCKED：同一 Writer 是 autocommit、无活跃语句，直接 main C API 返回 SQLITE_OK 并实际回收。临时仅检查应用语句的诊断代码已撤回；没有据此宣称 SQLite 版本缺陷或修改原始日志。

2026-10-01 23:37 两项自动场景通过：旧普通读取事务在 Writer 写入 100 KiB / 修订后仍读旧状态，checkpoint Deferred 不等待、WAL 不变；释放后 Complete / 0 帧 / 实际 WAL 零、新读取完整保留内容。后台低阈值仅测试配置验证实际增长 → 旧租约 SNAPSHOT_EXPIRED → 回收 → 新租约见完整新内容。完整存储层 113 项普通测试（98 内部 + 15 集成，30 万 benchmark ignored）、workspace Clippy 与 Windows 10 隔离库 native probe 通过，累计普通 Rust 场景 199 项。原生既有 UI / IPC / 系统路由退出 0，仍有 class unregister 1412；未重新压测 30 万性能，过去的性能失败仍有效待复测 / 优化。签名最后位置注册与实际候选 / 会话 / 明细分页继续待实现。

## M08h4：签名游标关联真实租约及有界最后位置

LeaseService 新增 issue_cursor / resolve_cursor：签发前从实际 actor 核对捕获修订，最后 tuple 留在租约的内存登记表，token 仍只包含其摘要。解析先验证 MAC / 完整请求与窗口，再查真实租约和已登记位置，读取 actor 再确认可用与旧修订；即使内部拥有有效签名，未登记位置也不能当作分页权限。重复同一游标可重试，旧页位置不会随着下一页被覆盖。API 必须在页面查询完成后调用，不在同 actor 回调里同步重入。

每个位置序列化限制 1 KiB，每租约最多 4096 个不同位置（两租约）；重复位置不再分配。达到上限明确 SNAPSHOT_EXPIRED 并释放事务，过期 / 释放后登记随租约清理；这不替代导出的一致副本作业。没有把最后标签 / 路径 / 消费数字编码给前端。

2026-10-01 23:43 三项新增自动场景和共九项租约测试 / workspace Clippy 通过，累计普通 Rust 场景 202：超过 2^53 最后 tuple、Writer 改变 data / price revision 后重复 resume 仍读旧状态、释放后明确失效、有效签名但未登记位置 / 伪修订 / 跨窗口 / 不匹配 tuple 类型拒绝、已签名不存在快照过期、实际登记 4096 个位置 / 重复不扩展 / 第 4097 个触发释放 / 新租约可用。此模块没有新生产 IPC 或原生检查；候选 / 会话 / 明细的 keyset SQL、DTO、命令和 UI 仍待接入。

## M08i1：正式筛选候选分页契约

新增 FacetDimension、FilterOptionsQuery / Request、FilterOption 与 FilterOptionsPage，生成 TS / Schema。query 包含完整 UsageFilter、四维度、可搜索文本和 1–200 page_size；cursor 单独放在 request，不进入稳定查询身份。页面返回实际 SnapshotMeta、候选 key|null / display_name / 十进制 count 与 next_cursor|null。count 定义为所选可信用量事件数，不代表导入完整性；未知分类的 key 保留 null。

facet_filter 只重置本维度为 All，来源 / 模型 / 项目 / 会话之外的原选择、半开日期和时区保持一致，原 filter 不改写。搜索限制 256 个 Unicode 码点并拒绝控制字符，Rust 与 JSON Schema 均允许 256 个中文字符；cursor 先验证固定 151 字符 / base64url 字符集，MAC / 登记 / 租约仍由存储层验证，形状合法不授予权限。

2026-10-01 23:51 两项核心多场景、契约差异、workspace Clippy、TS 类型与 13 项 Vitest 通过，累计普通 Rust 场景 204：逐一验证四个 facet 只忽略自身 / 原选择保留、页边界 / 中文上限 / 控制字符 / 非法日期、cursor 长度 / 字符、未知候选和超过 2^53 count 保真、候选上限与必填 next_cursor。尚无候选 SQL / IPC / 页面，没有新增原生验收；下一个模块接入实际来源 / 模型 / 项目 / 会话搜索与同快照 keyset 分页。

## M08i2：同租约的真实候选搜索与 keyset 分页

存储层 filter_options 使用已验证的完整查询 / owner 绑定与真实 SQLite 租约；模型按实际 provider / model 分开，项目取当前快照内别名，会话取 canonical key 与 provider session id。只忽略正在选择的维度，其他筛选及半开日期继续限制可信事件计数。来源候选包含已登记但当前范围零事件的来源、暂停标识；COUNT DISTINCT event_id 防止同一来源多条镜像证据放大计数。

搜索为 Unicode 小写后的字面子串，不把 % / _ 当通配符，不拼接输入为 SQL；参数绑定。分页按稳定 opaque key 的 BINARY 顺序、null 首位，使用最后 key 而非 OFFSET / 可变显示名。同租约保留事实、标签、修订和首请求生成时间；最后一页立即释放事务，先前游标随后明确 SNAPSHOT_EXPIRED。失败也释放租约。元数据版本表示 facet_filter 的候选范围，没有把搜索结果数宣称为导入完整性。

2026-10-02 00:06 五项新增多场景、完整存储层 121 项普通测试（106 内部 + 15 集成，30 万性能 benchmark ignored）与 workspace Clippy 通过，累计普通 Rust 场景 209：未知候选 / 同名不同 provider / 多事件计数、多页身份不变与终页槽重用、中文 / ASCII 大小写 / SQL 字面搜索、自身选择忽略而其他筛选保留、日期右端排除、真实项目别名 / 会话名、零及暂停来源 / 同来源镜像去重、Writer 更新事实及价格修订后旧页不变 / 新页可见、跨窗口 / 搜索 / 页长 / 来源重绑拒绝、伪游标 / 超长存储 key 错误不泄漏槽。尚无生产候选 IPC / 页面及新原生验收，继续接入命令与高级筛选。

## M08i3：候选 IPC 与合法能力的幂等关闭

主窗口 capability / 后台 label 双重限制 get_filter_options 与 close_query_snapshot，使用正式请求身份 / 参数校验 / spawn_blocking；TypeScript 使用生成 DTO。CloseQuerySnapshotRequest 用明确 filter_options 变体携带原完整 query 和非 null cursor，先校验 MAC / 绑定 / 登记，再释放真实事务；合法已过期或重复关闭幂等成功，伪造及改查询拒绝。未开放按任意 snapshot_id 关闭其他读取的入口，后续会话 / 明细租约将扩展这个正式关闭命令。

2026-10-02 00:12 六项候选存储场景（新增关闭 / 拒绝跨 owner 和查询 / 重复关闭 / 释放后明确过期）、workspace Clippy、TS 类型与 13 项 Vitest 通过，累计普通 Rust 场景 210。Win10 随机隔离库真实 WebView 验证四种候选命令的 api / request 身份、price revision 3、实际 query snapshot、空列表 / null cursor，逐种验证 201 页长 / 伪 cursor 查询 / 伪 cursor 关闭分别 INVALID_QUERY / CURSOR_INVALID；既有真实价格 / 分组 / 总览 / context / UI 导航 / 原生系统路由继续通过，退出 0，保留 class unregister 1412 提示。本次原生夹具没有真实用量或授权来源，多页和合法关闭由存储测试验证；顶部交互下一步接入。

## M10c：顶部真实可搜索高级筛选

顶部模型 / 项目 / 会话改为正式候选搜索，每页 50 条，可续页；已选名称保留、未知候选以 ids=[] / include_unknown=true 区别全部。日期 / 来源 / 三维度组合传给同快照总览及分组，跨页面保持，重置恢复今天 / 全部维度。沿用七项文字导航、顶部布局与深灰蓝色视觉；搜索 / Esc 焦点恢复、输入下键 / 候选上下键 / Home / End、原生按钮 Enter / Space 支持键盘操作。

候选 Hook 串行查询 / 关闭 / 续页，请求完成后清理迟到结果；换搜索 / 维度 / 日期 / 来源 / 页面释放原 query 的合法 cursor，失败依靠服务器有界 TTL 回收。250 ms 搜索防抖，准备新搜索时隐藏旧候选；续页校验完整 meta / dimension，拒绝重复已显示 key，过期不拼接新快照并允许重新查询。单框最多显示 1,000 条并提示缩小搜索，同时释放其租约。当前为每维度单选，来源仍使用已登记来源 select；自定义日期 / 价格依据 / 配置时区 / 隐私最新策略 / 会话页面与钻取继续待实现。

2026-10-02 00:20 TS 类型、13 项 Vitest、完整 13 项 Playwright（新增四项高级筛选多场景）、生产 Vite 构建通过。新增场景覆盖 >2^53 候选计数 / tooltip、同页续接、未知项 / 组合条件 / 跨页保持 / 重置、准确原 query 关闭且先于新查询、Esc / 搜索焦点 / 键盘移动、过期重新查询、迟到响应串行清理、空搜索、257 中文字符拒绝及页面卸载释放。截图检查发现 960 宽下全局 span 宽度使名称竖排，已修正并新增名称宽度断言；重新跑四项 / 构建，查看 1280 与 960 图片确认候选正常横排、页面无横向溢出。

00:22 Win10 独立应用随机隔离库真实 WebView 逐一打开三维度候选框，实际查询空候选 / 输入焦点，Esc 关闭并返回原触发器焦点；既有候选 / 关闭错误拒绝、统计 / 价格 / context / UI 导航 / 原生系统路由通过，退出 0，仍有 class unregister 1412 提示。此检查没有真实 Codex 来源、多页桌面数据、账户、任务栏、实际睡眠或安装测试。

## M07g1：核算版本升级的候选恢复与基线隔离

session accounting 读取及实时提交事务均再次校验活跃账本的 parser / accounting 版本；不匹配返回 CANDIDATE_OBSOLETE，拒绝当前引擎读取旧基线或将预先准备的 batch 混写到旧语义。只影响实时核算入口，已有账本指针 / 观察 / 事件 / 检查点保留。JobService 启动时、每项作业完成后和空闲十秒周期检查核算版本变化，按依赖闭包创建既有候选重建任务；不是直接 UPDATE 版本字段。

自动任务以旧 ledger / 目标版本、必要文件检查点 / identity / anchors / 来源可用性计算幂等输入指纹，不要求文件已到 EOF，冻结已提交前缀，新增日志在成功切换后继续采集。busy 作业不并发，取消 / 普通失败相同输入不重复；interrupted / candidate obsolete 最多三次，输入变化才开启新指纹。当前候选选择有 32,768 seed 上限，沿用闭包 / manifest / batch 界限，未完成大规模启动升级性能验收或后台升级检查失败诊断。

解析器版本变化必须重新解析原始必要数据；该路径尚未实现。自动核算升级跳过 parser 不匹配组，prepare_rebuild 也在候选创建前拒绝旧 parser 为 UNSUPPORTED_FORMAT，防止手动或 proof 重放把旧解析结果无条件标为当前版本。

2026-10-02 00:35 新增三项存储 / 两项采集集成及完整 workspace 215 项普通 Rust 测试、workspace Clippy 通过。检查旧版本拒绝读取 / 预制 batch 原子回滚但总量保留、不同 parser 候选零写入、部分文件上界升级 / busy / 取消 / 来源状态变化、三次过时重试界限；真正关闭应用后在隔离 fixture 库植入旧版本和损坏基线，再启动后台 worker 从必要观察生成新基线，原子发布后继续读取已追加原始日志，消费从 120 到 130；修改原始前缀导致重建失败，旧指针 / 总量 / committed offset 全不变，原始文件均未被应用写入。本模块尚未改变 ACCOUNTING_VERSION 或部分向量规则，下一步同时更新规则和旧版本读取兼容。

## M05c：部分向量下界与 accounting-v2

修正缺失父项时 known input / cache 与 known output / reasoning 可证明的下界大于 reported_total 却被接受的边界；比较用 i128，两侧父子只取一次下界，不叠加缓存或推理。未知字段 / 总量保持原 null。ACCOUNTING_VERSION 更新 accounting-v2；新观察核算、写入事件 / 基线 / 候选均严格执行新规则。

已发布事实使用明确只读 published_total(accounting_version)；v1 保留已发布规则，v2 严格规则，未知版本 UNSUPPORTED_FORMAT。原始统计 / 缓存混合查询从当前真实事务内的每个 ledger 取对应版本，UTC 预聚合也保持账本版本；价格读取不因旧部分向量使整个页面失败，此类缺分项费用仍未计价。候选成功前旧 v1 结果与旧真实读取事务不变；新候选把矛盾向量分类 pending、保留原向量 / null / invalid_usage 依据，切换后从可信消费中排除，旧事件留在历史账本。自动升级进一步限定已知 v1 前驱；未知 / 未来核算版本及旧 parser 不降级重放。

2026-10-02 00:45 新增六项多场景、完整 workspace 221 项普通 Rust 测试、workspace Clippy、契约差异检查通过。三项核心测试独立枚举 1,280 个 nullable 小向量的所有具体完整补全，检查包含关系 / 总量约束；另列 i64::MAX + 子项 / 零 reported / 仅子项有值 / 全字段包含案例，last 或 cumulative 无效不推进状态，合法 last 上下文独立保留。两项存储验证新无效 batch 全事务回滚、旧 v1 矛盾事实原始 / UTC 缓存 / 真实旧快照可读、新版本正确结果可见、未知未来版本拒绝且不启动升级。新增实际 SQLite / 合成原始日志集成从离线 v1 矛盾事件重建，发布前旧读取见 10、发布后可信消费 0 / 一条 invalid_usage pending / 旧事件仍保留 / 检查点及原始日志不变；既有启动 worker 升级后消费 120 → 130 与前缀失败保留继续通过。

同时间 Win10 随机隔离库独立应用 native probe 继续通过实际价格 / 候选 / 总览 / 分组 / context / 高级筛选焦点与关闭 / 原生路由，退出 0，仍有 class unregister 1412 提示；原生 probe 的库为空，不当作实际旧 Codex 数据升级验收。未重新运行 30 万性能，原先性能未达标仍待优化；旧规则读取新增版本查询的性能也未单独验收。

## M09e：价格发布通知与完整快照刷新

成功提交价格规则后发出设计规定的 price_rules_changed，载荷仅含精确 price_revision 和 all_models 全量标记；冲突及失败不通知。总览 / 分组订阅后重新读取完整 bundle，不用事件直接改旧快照的费用或修订。隐藏窗口等待恢复可见时刷新，原有周期核对保留；查询条件改变 / 页面卸载清理监听，包括 StrictMode 下迟到的异步订阅。

2026-10-02 01:01 14 项 Vitest、三项总览 Playwright（新增通知 / 隐藏恢复 / 切换条件和卸载释放场景）、生产 Vite 构建、workspace Clippy 和契约差异检查通过。Win10 隔离库真实价格创建 / 替换 / 退休产生且仅产生三条正式通知，修订依次 1 / 2 / 3、全量标记为 true，失败的重叠规则没有通知；既有真实 IPC / UI / 系统路由通过，退出 0，保留 class unregister 1412 提示。

用户于 2026-10-02 明确调整优先级：先实现完整功能，完成后是否进行性能测试由用户决定。后续不安排新的性能测试；已有报告保留为历史证据，功能正确性、交互和原生验收继续开展。别名、实际离线价格目录、独立重估作业和持久化费用缓存仍待实现。

## M08j：同事务会话分页与真实消费 / 上下文 / 关系

query_sessions 正式 Rust / TS / JSON Schema、主窗口 capability 与后台 label 校验接入。首请求内部取得真实租约，绑定完整 filter / price_basis / sort / page_size，续页严格 keyset；终页 / 错误 / 合法关闭释放。close_query_snapshot 增加 sessions 变体，合法重复关闭幂等。总量排序比较同宽规范十进制字符串，支持 i128 聚合；时间排序稳定 key 打破平局，无 OFFSET。

页面汇总覆盖整个 filter，各行保存选定范围消费 / 费用 / 覆盖、最新选定事件的模型 / 项目，另外返回同事务最近上下文。父 key / provider id 区分已解析与未解析；跨日期已解析子会话数不冒充范围内会话数，也不宣称继承已核算。没有读取聊天正文、账户信息或原始源文件。会话详情 bundle / 可靠回合页 / 关系展开、明细与 UI 继续推进。

2026-10-02 01:13 四项新增存储多场景与完整 workspace 225 项普通 Rust 测试通过；workspace Clippy、14 项 Vitest / 完整 14 项 Playwright、TS 类型及生产 Vite 构建通过。验证 3 × i64::MAX + 110 的精确汇总、超 i64 行总量 / 相同时间平局分页、请求数 2 而可靠回合 1、未知回合 / 容量、日期右端排除、事件发生时的项目 / 模型、范围外最近 context、父子关系 / 来源 / 模型筛选；真实旧 SQLite 页在 Writer 改消费 / 价格 / 父关系 / 显示名 / 上下文后保持不变，新查询可见改变；跨窗口 / 排序 / 页长 / 来源 / 价格依据重绑拒绝，关闭 / 过期 / 终页释放有效。

同时间 Win10 随机隔离库独立应用真实 WebView 会话命令返回实际 query snapshot、price revision 3、未知分项 / 空列表 / null cursor，201 页长与伪 cursor 分别拒绝；已有通知 / 价格 / 候选 / 总览 / 分组 / 系统路由通过，退出 0，仍有 class unregister 1412 提示。此原生库没有用量，会话多页 / 消费及关系由实际 SQLite 合成夹具验证；尚无会话控件实际验收。

## M10d1：正式会话列表与同列表快照详情抽屉

会话导航接入 query_sessions，支持最近活跃 / 消耗最多、50 / 100 / 200 条分页、上一页缓存 / 下一页。页级汇总覆盖整个筛选，不随当前页改变；精确 tooltip、未知项目 / 模型 / 上下文、事件数 / 已识别回合分别显示。条件或排序改变先串行清理原租约；迟到响应也用原 query / cursor 关闭。续页校验完整 meta / 汇总 / 费用 / 覆盖及重复位置，拒绝混合；过期保留已取得旧页并明确重新查询，重新查询替换全部页面。客户端仅缓存最近 10 页；分页浏览保持固定快照，手动刷新、恢复可见和价格通知重新读取，不以周期轮询打断用户翻页。

右侧 445 DIP 抽屉使用当前列表同一 DTO，分开显示范围累计消费与最近上下文、精确分项 / null、模型 / 项目 / 可靠回合 / 父子关系。可从抽屉将本会话或已解析父会话加入主窗口筛选，不编造候选计数；高级筛选选定值改成 key / display_name，候选行仍使用实际 count。模态背景 inert、Tab / Shift+Tab 循环、Esc / 背景关闭、返回原按钮焦点；关闭触发范围变化时回到会话筛选。抽屉不加载正文。get_session_bundle、可靠 turn_id 列表、完整继承依据、子会话关系展开与来源定位仍待实现，当前抽屉不是完整会话详情交付。

2026-10-02 01:26 TS 类型、14 项 Vitest、完整 17 项 Playwright（新增三项会话多场景）、生产 Vite 构建与契约差异检查通过。验证 >2^53 完整数 / tooltip、55 会话的 50 + 5 页 / 缓存返回 / 终页禁用下一页、未知分类 / 未解析父关系、日期范围外 context / 未知容量、2 事件与 1 回合、抽屉键盘 / inert / 焦点恢复、父会话钻取 / 共享筛选、过期不拼接 / 整体替换、排序先关闭旧 query、来源空页 / 隐藏旧数据、迟到请求离开页面后的原能力清理。查看 1280 / 960 列表及抽屉顶部 / 下部截图；发现全局未知费用字体覆盖表格样式，已增加表格作用域优先级。列表自身可横向滚动，主页面没有横向溢出。

同时间 Win10 随机隔离库实际 WebView 打开会话页，真实 query_sessions 返回 0 / 未计价 / 空态，排序从最近活跃改消耗最多、页长从 50 改 100，分页按钮实际禁用；全部既有 IPC / 通知 / UI / 系统路由通过，退出 0，仍有 class unregister 1412 提示。此原生检查为空库，没有真实用量、多页或实际抽屉控件操作；这些界面场景来自明确浏览器合成 DTO 桥，实际账本一致性来自 M08j 的真实 SQLite 测试。没有执行性能测试。

## M08k：真实明细分页与白名单向量 / 价格依据

query_usage_events 接入 Rust / TS / JSON Schema、主窗口 capability 和后台权限；time_desc / total_desc 按精确 i64 事件值和 event_id BINARY 稳定 keyset。首请求真实租约，续页绑定完整条件 / 价格依据 / 排序 / 页长 / 窗口，终页 / 错误释放；close_query_snapshot 增加 usage_events 的合法能力关闭。整页同时返回整个筛选汇总 / 费用 / 覆盖与版本，各事件的归属 / 来源 / 可靠 turn_id / 核算方法 / 质量 / PriceOutcome / 解析和核算版本可复核。

原始 last / cumulative 仅提取归一化观察中的五项向量，不返回整个 JSON、聊天或任意字段；缺失向量和分项保持 null。RawTokenCount 原始 i64 使用规范字符串，保留无效原始负数作证据；已发布消费独立按账本版本检查为非负，DecimalInt 总量不变。不同镜像 provenance 来源去重，镜像筛选不使事实倍增。明细 UI、单事件价格规则展示及完整会话详情 / 回合页继续待实现。

2026-10-02 01:40 新增一项核心 / 四项存储多场景及完整 workspace 230 项普通 Rust 测试通过；workspace Clippy、契约差异、15 项 Vitest、TS 类型和生产 Vite 构建通过。验证 i64::MIN / MAX 与 >2^53 的精确字符串往返、number / -0 / 前导零 / 越界拒绝；相同时间平局、9007199254740992 与 9007199254740993 精确消费排序、原 last -1 与已计入 100 分别保留、不返回测试植入的私有任意字段；旧分页原始向量 / 方法 / 会话名 / 价格不随 Writer 改变、新查询可见价格 / 原向量修订、mirror 的两份 provenance 只返回一事件且来源 DISTINCT、半开范围、未知分项、游标绑定 / 幂等关闭 / 错误行释放槽。明细 schema 独立检查页数 / 来源数 / 质量数限制、必填 nullable 向量和任意原始 JSON 拒绝。

同时间 Win10 隔离库真实 WebView 逐种调用 time_desc / total_desc 明细，返回实际 query snapshot / price revision 3 / 空列表 / null cursor / 未知分项，伪 cursor 拒绝；既有原生会话控件 / 价格通知 / 总览 / 分组 / 系统路由通过，退出 0，仍有 class unregister 1412 提示。本次原生库无真实事件，原始向量、计价、多页及镜像由实际 SQLite 合成夹具验证；没有把它当作实际 Codex 格式或明细控件验收。没有执行性能测试。

## M10e：真实明细界面与可展开核算依据

明细导航接入 query_usage_events，时间 / 消耗排序、50 / 100 / 200 条分页、精确输入 / 缓存 / 输出 / 推理 / 总量，绿色分币种估算、零费用与明确未计价分别显示。记录时间显示至毫秒，time 元素保留完整 ISO 时间。点击会话进入共享日期 / 来源 / 模型 / 项目条件下的会话页并显式加入会话筛选。

单事件展开显示已计入增量、原始 last、原始累计五项向量，null 保持未知，负数原始证据以提示色显示；展示实际核算方法、质量、可靠 turn_id、不同来源 ID、解析 / 核算版本、匹配规则 ID、精确金额与计价原子。没有重新把缓存 / 推理或原始累计加入总量，也不把一条事件称为一次模型调用或完整回合。会话和明细共用有界 usePagedUsage 控制器，保留原有串行读取 / 清理、同快照验证、十页缓存、过期重新查询与价格变更整体替换。

2026-10-02 01:56 TS 类型、16 项 Vitest、完整 20 项 Playwright（新增三项明细多场景）、生产 Vite 构建与 workspace Clippy / fmt 通过。新增原始计数显示测试验证 signed i64 全范围 / >2^53、null / 零、非法数值拒绝；浏览器验证 53 事件的 50 + 3 页 / 缓存返回、精确 tooltip、USD / EUR 分开和真实零、未知分项 / 未计价、负数原始向量 / 大累计 / 匹配规则 / 15 位金额 / 原子、分页 meta 改变拒绝而保留旧页、过期不拼接、会话定位、价格通知整体换新价格与清理租约 / 监听。合成价格示例按 1 → 2 个 rate atom 更新对应精确 cost_atoms / 金额，不将这些费率作为生产目录。查看 1280 / 960 列表及依据展开截图，主页面无横向溢出，表格内部允许滚动。

同时间 Win10 隔离库实际独立应用打开明细页，真实查询显示 0 / 未计价 / 空态，时间排序改消耗排序、50 改 100 条，分页按钮禁用；所有既有原生命令 / 通知 / 会话控件 / 系统路由继续通过，退出 0，仍有 class unregister 1412 提示。原生库没有实际事件，向量展开 / 多页 / 费用展示由明确浏览器合成 DTO 场景验证；真实事实 / 价格一致性由 M08k 的 SQLite 测试验证。实际源日志定位、规则完整详情、共享隐私、完整诊断 / 回合 / 关系 / 窗口与维护仍待完成。未执行性能测试。

## M08l：会话详情的原子快照与分类摘要

get_session_bundle 正式 DTO / command / 主窗口权限 / runtime 已实现，同一个只读事务包含指定会话的范围消费、精确估算、覆盖、最近所选事件、独立最近上下文、规范父子关系和当前账本分类摘要。指定会话与原 sessions 选择取交集，镜像别名在快照内解析；无所选事件仍返回已登记身份和上下文，分项为 null。子列表排除镜像别名，跨日期总数与最多 100 条的显式截断分别展示。分类最多 64 组，只返回种类 / 原因 / DISTINCT 观察数，不发送任意证据 JSON 或累计量，不将分类量计入消费。

2026-10-02 02:11 新增 4 项 SQLite 合成场景和全部 8 项会话测试通过，契约 8 项 Vitest、TS 类型、workspace Clippy / fmt / 生成差异通过。验证已有关系与未知上下文、>2^53 计数、所选无事件 / 其他会话 / 空来源不扩大消费、Writer 在固定读取事务后变更事件 / 价格 / 身份 / 分类 / 上下文仍读取旧结果，下一事务整体见新值；101 子列表上限 / 非法原因返回 DB_CORRUPT / 镜像父别名归一与镜像子排除。WebView 中实际调用已登记无事件会话详情，收到 price revision 3、null 上下文 / 活动、真实零消费和空关系；不存在的会话正确 INVALID_QUERY。既有原生 probe 全部通过退出 0，仍有 class unregister 1412 提示。这是接口与 SQLite 合成证据验收，详情控件 / 可靠回合列表 / 源日志定位仍待实现。未执行性能测试。

## M10d2：完整详情快照、子关系跳转和账本分类

445 DIP 抽屉改为 get_session_bundle，初次读取时不把旧列表行冒充详情；消费 / 价格 / 上下文 / 关系 / 分类整体来自自己的快照，底部明确标注详情版本与列表独立。支持重新读取详情、错误 / 读取状态、父会话及子会话共享筛选跳转，跨日期子关系总数与列表截断提示分别显示。继承 / 待确认 / 重复 / 未归属的分类观察数和已保存原因展示，明确不是 Token，不能跨分类相加，也不证明全部继承边界已经确认。可靠回合列表和逐项原始继承证明、源定位仍待后续实现。

2026-10-02 02:18 全部 234 项普通 Rust、16 项 Vitest、21 项 Playwright（新增详情多场景）、TS 类型和生产构建通过。新增浏览器合成 DTO 的详情与列表故意采用不同数据 revision / 总量，验证不拼接；刷新整体改变消费 / 模型 / 版本，失败无列表伪兜底，可重新读取；3 个关系只返回 1 条时明确截断；子跳转关闭并加入统一筛选；全部详情请求延迟期间 Esc 关闭 / 焦点恢复，晚到响应不重开、不恢复背景 inert。1280 / 960 分类与关系截图已查看，445 DIP 内无横向溢出。此次普通 cargo test 没有启用 ignored 性能夹具。

同时间 Win10 内置新前端的独立应用原生命令 / 详情空库 IPC / 五页控件与系统路由全部通过，退出 0；仍有 class unregister 1412 提示。实际原生库无消费会话，因此本次没有实际 WebView 打开非空详情抽屉，抽屉交互证据来自明确浏览器合成 DTO，关系与消费一致性来自真实 SQLite 合成测试。未执行性能测试。

## M08m：可靠回合的稳定分页与精确估算

query_turns / TurnsQuery / TurnsRequest / TurnsPage / TurnRow 与 close_query_snapshot 的 turns 分支已接入；主窗口独占命令权限。完整筛选、目标会话和价格基础绑定实际 SQLite 租约，固定最近时间 / turn ID 稳定分页。明确非空 turn_id 按同一规范会话分组，分别聚合精确消费与当时规则估算，来源筛选沿用 provenance 去重。范围总量包含未知回合的可信消费，单独返回 unidentified_usage_event_count；回合行是所选范围片段，不宣称完整回合消费。未知 / 空 ID 不制造回合，不把事件数改名为请求或回合数。

2026-10-02 02:27 新增 4 项 SQLite 多场景、所有 12 项会话查询测试通过；workspace Clippy / fmt、契约生成差异、8 项契约 Vitest、TS 类型通过。覆盖两次 i64::MAX 同回合精确和、事件 7 条 / 已识别回合 3 个 / 未识别事件 3 条、同时间平局与时间范围片段、其他会话同身份不混入、固定旧页成员 / 时间 / 价格、新事务可见变更、会话选择交集与空来源、全部未知时保留 110 消费而回合数 null、跨窗口 / 目标 / 页大小 / 价格基础游标拒绝、幂等关闭、非法 turn_id 连续失败后读槽仍可复用。最初复用测试辅助函数时其 ledger 命名与既有 session 夹具不一致，改用已有事件夹具修正后通过，不修改生产逻辑绕开失败。

实际 Win10 独立 WebView 调用已登记无事件会话的回合查询返回真实 query meta / price revision 3、null 回合数 / 不完整标记、0 未识别事件、空页 / null cursor；伪造游标正确拒绝。所有旧原生检查通过退出 0，仍有 class unregister 1412 提示。回合 UI 和非空原生交互尚未验收；数据聚合证据来自合成 SQLite 夹具。未执行性能测试。

## M10d3：可靠回合列表的按需分页

会话抽屉按需展开可靠回合列表，query_turns 正式 DTO、20 个回合 / 页、同快照缓存返回、明确过期与重新读取、收起 / 关闭释放租约。回合卡片显示实际 turn_id、当前筛选片段内首末时间（毫秒）、精确 Token tooltip、用量事件数和分币种估算；不存在回合标识的事件保留在所选会话总量中，数量单独报告，明确回合识别不完整。回合分页版本与详情快照分别标明，不将两种数据拼成同一快照。没有读取或显示聊天正文。

2026-10-02 02:34 全 workspace 238 项普通 Rust、16 项 Vitest、22 项 Playwright、TS 类型和生产构建通过；原有 ignored 性能夹具保持未启用。新增浏览器场景验证 23 个回合的 20 + 3 页、2 个事件在一个回合行、未识别事件 3 条与 23 个已识别回合分别显示、跨 i64 精确和 tooltip、分页自身 data revision 10 与详情 revision 8 分开、末页禁用 / 缓存上一页、过期保留原 20 行并禁止追加、重新读取清除错误、收起准确增加一个 capability close。初始测试错误地要求已经由服务器末页释放的租约还要额外 close，修正为收起前后精确 +1 的独立预期。1280 / 960 回合截图已查看，445 DIP 抽屉不溢出。

同时间 Win10 内置新前端的独立应用原生详情 / 回合空库命令、五页控件及系统路由通过退出 0，仍有 class unregister 1412 提示。真实原生库没有消费会话，因此非空回合列表 / 关系控件实际 WebView 验收仍待进行，不将合成浏览器截图等同于该验收。未执行性能测试。

## M10f1：统一日历选择后端

新增 CalendarSelection / Request / Result 与主窗口独占 resolve_calendar_selection。今日 / 近 7 日 / 近 30 日 / 自定义包含结束日的日期选择，按请求 IANA 时区转换半开 UTC 范围；热力图独立近 182 当地日；后台时钟决定该时区当前日。重用现有本地边界转换，明确处理午夜重复 / 缺口，整日跳过导致无区间返回 INVALID_QUERY；不使用固定 24 小时或浏览器系统时区猜测非系统时区。尚未接日期控件和持久时区设置，不将此接口提交计为完整筛选 UI。

2026-10-02 02:41 新增 4 项日历多场景、所有 12 项日历测试通过；workspace Clippy / fmt、契约差异、17 项 Vitest、TS 类型通过。独立预期验证纽约秋季 04:00Z → 次日 05:00Z / 春季 23 小时、7 日起点、同 UTC 时刻夏威夷与 UTC 的不同当地日、闰日包含结束日、Sao Paulo 午夜缺口首个有效时刻、Apia 整日跳过拒绝、182 当地日热图、反向 / 无效 / 不规范日期 / 未知时区 / 未知字段拒绝。Windows 10 实际 WebView 调用自定义纽约日期得到上述 25 小时 UTC 范围，非法 2026-02-29 得到 INVALID_QUERY；既有原生检查通过退出 0，仍有 class unregister 1412 提示。此前完整 238 项 Rust / 22 项浏览器交互已通过，本模块另做 12 项日历针对性检查；没有把合成数据当作真实日志验收，未执行性能测试。

## M15a：持久显示时区与配置并发控制

独立 DisplayPreferences / DisplaySettingsSnapshot / TimezoneMutation / SettingsChanged 契约和主窗口权限的 get_display_settings / set_display_timezone 已实现。首次有效系统 IANA 时区初始化幂等；用户改动走全局 settings revision 乐观并发；相同值无写入 / 无通知。Writer 同事务提交设置 payload / 时间 / revision，提交后才发送 settings_changed，公开响应只含必要显示字段。保留已有主题、旧隐私、小窗范围、任务栏 / 启动偏好，不用只有时区的 DTO 重写整份配置。不支持的新配置版本保留并明确报 UNSUPPORTED_SETTINGS_VERSION，损坏配置不回退默认。

2026-10-02 02:56 新增 6 项 SQLite 设置测试通过，workspace Clippy / fmt / 契约差异、18 项 Vitest 与 TS 类型通过。验证首次 null / 初始化一次 / 重复初始化不覆盖 / 重启保存、两个并发首次请求只发生一次变更且结果一致、来源增加引起配置冲突而时区不变、旧只读事务固定原设置、强制 Writer revision 更新失败同时回滚 payload、未来版本 / 无效时区不覆盖 / 数据和价格 revision 不变 / i64 revision 溢出无写入，以及已有 light theme / privacy true / 固定会话 scope / 启动和任务栏偏好逐字段保真。初次检查未发现初始化迁移已有完整设置 payload，新增读取最初误判为损坏；改为校验并保留既有字段后通过，没有改迁移清空旧字段来绕开失败。

Win10 独立 WebView 实际读取未初始化显示配置，初始化 New York / 改为 UTC / 重复初始化保持 UTC / 同值保存不增加 revision / 旧 revision 拒绝。Rust 侧实际监听恰好两条 settings_changed，revision 1、2；冲突和无变化没有通知。全部既有原生检查通过退出 0，仍有 class unregister 1412 提示。时区设置 UI 和主日期控件尚未接入；mini / 最新隐私策略的应用仍待完成。没有执行性能测试。

## M10f2 / M15b：保存时区接入正式页面与设置编辑

五个统计页面通过 useMainCalendar 读取已保存配置，首次 null 才请求初始化有效系统 IANA 时区；今日 / 近 7 日 / 近 30 日与独立热力图范围全由 Rust 日历接口解析。浏览器本地日期转换代码已移除，选择变化先隐藏原范围数据，失败明确提示，不猜测时区或查询假日期。分钟检查和恢复可见会重新解析当前日；同范围刷新保留上一次完整响应，最新配置通知重新读取。设置页支持有效 IANA 输入，提交携带编辑开始时的精确 revision；刷新 / 外部变更不覆盖草稿和原 revision，显式重置后基于新配置编辑。

2026-10-02 03:14 全部 248 项普通 Rust、18 项 Vitest、26 项 Playwright、TS 类型、生产构建、workspace Clippy / fmt / 契约差异通过。新增四项浏览器合成桥场景验证保存非系统时区后原样发送后台 25 小时范围、大于 2^53 的 revision 不舍入、事件监听只有一份、跨页保存日期条件、配置冲突保留草稿 / 原 revision、无效时区无变更、未来配置版本失败时不发日期或统计请求、首次初始化恰好一次及新日期解析失败清除旧范围。测试首次错误假定 StrictMode 查询只有两次和浏览器系统时区必为 UTC，改为验证实际新范围并明确配置 QA 浏览器 UTC；未改生产逻辑绕开失败。原有价格监听断言限定 price_rules_changed，新增全局 settings_changed 不混算。1280 / 960 设置页截图已查看，无横向溢出。

Windows 10 内置新前端的独立应用首次实际保存系统时区；重复初始化不覆盖、两次 IPC 修改及一次实际设置表单 UTC → Asia/Tokyo 保存，使修订从 1 到 4。数据库读取确认 UI 保存结果，返回总览的实际筛选显示 Asia/Tokyo；Rust 侧恰好收到后续三条变更通知 2 / 3 / 4。五页空库控件和既有系统路由通过，退出 0，仍有 class unregister 1412 提示。生产数据 / 用户日志未读，此原生检查不证明真实非空消费会话或 WSL。自定义日期、热力图日期跳转、隐私 / 主题仍待继续实现。没有执行性能测试，ignored 历史性能夹具未启用。

## M10f3：自定义日期、热力图跳转与长范围趋势

顶部日期控件使用正式 CalendarSelection，包含开始 / 结束当天。编辑弹层只改变草稿，显式应用才更新所有统计页面；取消、Esc、外部点击保留原范围。预填和日期标注只做配置时区格式化，不生成 UTC 边界；后台解析仍是权威。已应用自定义范围显示完整起止日期，可再次编辑；即使后台解析失败也能重新编辑已有自定义选择。热力图按钮由桶 start_ms 在配置时区确定当天，更新统一筛选，保留维度条件和热力图独立范围。

长范围为小时 / 日桶保留低于 2000 项上限的时区边界余量，自动使用日 / 月趋势，并禁用过细粒度，保持原日期 / 维度和总量范围。此选择只影响趋势粒度，不改变核算结果；极端超过月桶上限的日期仍由后台明确拒绝，不裁剪日期冒充成功。

2026-10-02 03:23 TS 类型、生产构建、20 项 Vitest、29 项 Playwright、workspace Clippy / fmt 通过；此前完整 248 项普通 Rust 已通过，本模块未修改领域或存储实现，未重复执行全部 Rust。新增日期标签独立预期验证 New York DST 边界、包含结束日最后一毫秒；新增粒度检查验证一年日桶 / 十年月桶 / 25 小时仍按小时 / 日期原值不变。三项浏览器合成场景覆盖未应用不请求、反向日期拒绝、Esc / 焦点恢复、闰日包含范围、跨页 / 重新编辑 / 重置、热图选择保留来源与粒度 / 独立 182 日，以及一年完整 365 桶不缩小范围。最初结束输入 aria-label 未包含界面说明导致测试定位失败，统一标签后通过。1280 / 960 日期弹层截图已查看，无横向溢出，主窗口分区未重新排列。

Win10 独立 WebView 实际在 Asia/Tokyo 打开日期控件、填写 2024-02-28 至 2024-02-29、点击应用，弹层关闭并显示相同包含日期；切换模型页保留日期 / 时区，实际查询空库汇总完成。既有 IPC 日历 DST / 非法日期、设置表单、五页及系统路由通过，退出 0；仍有 class unregister 1412 提示。此空库原生检查没有实际非空热力图点击；热图跳转为明确浏览器合成 DTO 场景，账本日历边界仍由 Rust 合成独立预期验证。没有执行性能测试。

## M10f4：明确估价时点与完整查询联动

统一筛选新增 event_time / specified_time，按指定时刻模式首次选择显式捕获本机当前 EpochMs 并显示完整 UTC ISO；刷新 / 分页沿用明确时点，只有编辑或“取当前时刻”才改变。UTC 编辑器支持毫秒，严格规范日期 / 时刻与实际 ISO 往返校验，不把非法闰日或 24:00 自动转下一天；1970 年前的有效有符号时点保留。应用、取消 / Esc 和焦点恢复已接入，重置筛选同时还原事件发生时价格。五页、会话详情与回合继续使用相同 PriceBasis；价格规则的选择和精确计算仍在 Rust 内完成，不改 Token 或账户范围。后台持久化重估作业 / 缓存仍未完成，不将前台查询重估视为该作业的交付。

2026-10-02 03:32 TS 类型、生产构建、22 项 Vitest、31 项 Playwright、workspace Clippy / fmt 通过；本模块未改领域 / 存储，沿用此前完整 248 项普通 Rust（包括真实 SQLite specified_time 半开价格规则、真实零 / 未计价、游标条件绑定用例），没有重复全套 Rust。新增 UTC 解析独立预期覆盖分钟 / 秒 / 1 至 3 位毫秒、闰日、负时点和零；拒绝非法日期 / 时刻 / 不规范或带额外时区的 UTC 本地编辑值。新增两项浏览器合成场景验证金额从明确夹具 0.87 → 2.32 而 683067 Token 与范围不变、精确时点 / 刷新保持 / 重置还原 / Esc 焦点，以及 session page / bundle / turns 请求均携带同一 123 ms 时点。合成价格只证明界面联动，精确核算仍以 SQLite / Rust 独立预期为依据。最初新会话测试使用了不存在的控件和 bridge 名，读取现有 DOM / fixture 后修正定位；没有改产品标签绕开检查。1280 / 960 时点编辑截图已查看，无横向溢出。最终焦点与负时间支持调整后再做 6 项总览检查、22 项 Vitest 和生产构建通过。

Win10 独立应用在模型页实际选择指定时刻、编辑 2024-02-29T00:00:00.123Z 并应用，关闭编辑后空库查询完成；切换项目页保留精确时点。既有日期 / 设置实际表单、所有原生 IPC / 五页控件与系统路由通过，退出 0，仍有 class unregister 1412 提示。原生库没有用量，本次不宣称实测非空费用重估或可靠回合面板；相应传参和 UI 为明确合成浏览器证据，价格数学和真实读取一致性为 SQLite 合成测试。未执行性能测试。

## M15c1：最新隐私响应与类型化脱敏基础

已实现 DisplayPolicyStamp、PrivacyState、PrivateResponse 与显式 PrivacyRedact。响应构造后切换策略，序列化仍采用最新值；策略戳与处理后的载荷在同锁内输出。commit_update 可协调真实设置写入，失败保持原策略；不可能的已提交倒退值使显示出口不可用而不发送原始字段。普通旧修订发布拒绝。来源 / 应用路径、会话 / 项目 / 父子名称使用基于稳定 key 的替代标签；金额 null，费用原因 / 事件匹配规则 / 原子移除，保持 Token / meta / 分页能力 / 未知值。分组上下文缺失保守脱敏，模型维度可明确保留公共模型标签。新增显示专用 PriceOutcome.redacted 和明细渲染，计价聚合拒绝该状态。

2026-10-02 03:56 全 workspace 255 项普通 Rust（新增七项隐私多场景）、22 项 Vitest、TS 类型、生产构建、workspace Clippy / fmt / 契约检查通过。五项集成场景验证构造时关闭 / 发送前开启、原 DTO 不被改写、金额及规则无残留、跨入口稳定标签、null / signed raw / 大整数 / cursor / meta 不变、项目分组和模型上下文、来源别名、旧修订不恢复显示、聚合拒绝已隐藏价格且累积器不变。两项领域内部检查验证 Serialize 期间锁仍持有，以及失败更新保留原策略 / 不可能提交停止发布。普通测试未启用 ignored 性能夹具。

四项明细浏览器检查通过，新增明确合成 redacted DTO 场景确认正文 DOM / tooltip 没有旧金额 / 规则，而精确 Token 保留；这不是全局隐私切换或实际 Windows 隐私验收。初次新测试误用价格聚合私有模块路径与控件文本定位，按已有公开导出和 aria 名修正后通过；未改产品规则绕开失败。时区 / 主窗口现有正式 IPC 尚未换到 PrivateResponse，隐私持久设置、实际出口、前端缓存门禁、托盘 / 小窗 / 任务栏控制仍待接入，因此不能把此基础提交称为隐私完整交付。未执行性能测试。
## M15c2：持久隐私配置与 Writer 协调

DisplayPreferences 正式公开 privacy，缺少该字段的旧 v1 配置按既有默认关闭读取，已有 true 原样保留；非法类型 / null / 未来版本拒绝而不重写。DisplayPrivacyMutation 必须携带精确全局 settings revision。独立隐私写入只修改 privacy 字段，配置 payload / 时间 / revision 同事务发布，同值无写入；不改变数据和价格 revision。

新增四项 SQLite 多场景检查通过（全部十项 settings 测试通过），覆盖持久化 / 重启 / 同值 / 关闭、保留主题 / 时区 / 小窗范围 / 原生偏好、真实旧读取事务的 DTO 仍按最新 PrivacyState 脱敏且来源表不改、配置冲突 / 强制 Writer 失败保持原策略和数据库、未来版本 / 损坏类型 / revision 溢出无写入。契约生成、TS 类型和 22 项 Vitest 通过。实际 Tauri 出口和前端切换仍待下一个模块；未执行性能测试。
## M15c3：正式 IPC 最新隐私出口与切换协调

实际 Tauri 的统计 / 分页 / 会话 / 来源 / 目录选择结果 / 应用路径 / 价格规则 / 作业及显示设置全部改用 PrivateResponse；模型 / 项目分组明确携带维度。独立日历转换和纯窗口动作没有敏感数据，保留普通响应。启动策略从持久设置读取，数据库或配置不可用时先隐藏；错误读取仍明确报错，不覆写配置。set_display_privacy 主窗口权限列入 manifest / capability，后台通用协调函数在策略锁内完成 Writer 提交，再通知 display_policy_changed 和 settings_changed，同值不通知。已开启时拒绝打开带路径的系统目录选择器；选择中途切换时返回值在序列化出口脱敏。

workspace Clippy / fmt 和实际 Win10 独立 WebView 验证通过，原生探针新增开启 / 关闭及 no-op / 旧 revision 冲突检查：策略修订 5 true / 6 false，恰好两条策略事件、合计五条显示设置事件 2 / 3 / 4 / 5 / 6。实际 IPC 开启后应用目录已隐藏，历史价格规则 / 别名为空且带策略戳，总览和会话价格 redacted，Token / meta 不变；目录选择被拒绝，关闭后新响应恢复真实隔离目录和测试历史规则。初次探针误用隐藏文案和目录 kind custom（正式枚举为 local / wsl），按已有契约修正后通过，未改产品绕开失败。既有单实例 / 托盘 / 休眠消息 / 关闭隐藏等原生检查通过退出 0，仍有 class unregister 1412 提示。

原生库仅有合成身份和价格规则，不含用量或用户日志；非空金额 / 名称脱敏为核心和 SQLite 合成证据。前端全局门禁、已显示 DOM / 分页缓存清除和实际开关 UI 尚待完成，此模块仅证明后台出口；小窗 / 任务栏权限和宿主共用协调在对应模块继续。未执行性能测试。
## M15c4：主窗口隐私开关与全局显示门禁

显示设置已接正式隐私开关。DisplayPolicyGate 使用精确 settings revision，与用量 / 价格快照修订分开；通过 IPC 策略戳和唯一 display_policy_changed 监听接受新策略。用户开启先改变显示世代并隐藏，再等待 Writer；保存冲突 / 失败继续保守隐藏，提示未保存，允许明确关闭后恢复。旧 / 同修订冲突策略不能恢复敏感显示。关闭通过成功响应确认，所有统计重新读取。

主窗口按显示世代销毁统计页 / 候选 / 详情抽屉和价格编辑器，实际清空分页缓存、状态 / 来源缓存及候选名称 / 搜索。统一日期、来源 ID、会话 / 项目 ID、计价时点继续保留；旧标签替换为无身份文字，不能在 title / aria 属性留下原值。价格管理在开启时明确隐藏，不将空 rules 当作未配置；来源目录选择控件隐藏。响应出口要求有效策略；迟到旧世代统计响应拒绝，分页响应在拒绝时按原查询与认证 next_cursor 释放，不用 snapshot ID 或新筛选。原生通用切换入口可供后续小窗 / 任务栏复用，实际两个宿主仍待实现。

2026-10-02 04:35 最新完整 259 项普通 Rust、29 项 Vitest、37 项 Playwright、TS 类型、生产构建、workspace Clippy / fmt / 契约检查通过。新增七项策略 / IPC 多场景单元检查：大于 2^53 精确排序、开启前封闭 / 失败保持 / 显式恢复、订阅清理、非法修订不默认、原已序列化旧分页拒绝并释放原查询、缺少策略拒绝。新增五项浏览器合成场景验证外部开启关闭真实抽屉 / inert 恢复 / 页面重读、迟到未隐藏分页释放、实际 checkbox 保存冲突保持保护后关闭、候选搜索 / 名称清除而 ID 筛选保留、总览金额 / 15 位精确金额 title / 原始路径 / 会话与项目名称在 DOM 全部移除，同时精确 Token 和来源 ID 保留，关闭新查询恢复。旧浏览器桥只有查询没有事件订阅，引入强制显示同步后失败；补全测试专用策略戳和事件 API 后通过，没有放松生产门禁。新增候选夹具最初误用 event_count 而不是正式 count，修正为契约字段后通过。1280 / 960 隐私设置截图已查看，无横向溢出；沿用现有主窗口布局。

实际 Win10 独立新构建应用：SDK 开启 5、真实 WebView checkbox 关闭 6 / 开启 7 / 关闭 8，读回 SQLite 配置 / 响应策略与实际控件一致。恰好四条隐私通知 5 true / 6 false / 7 true / 8 false，七条显示设置通知 2 至 8；同值 / 冲突无额外通知。后台路径 / 历史价格脱敏与关闭新查询恢复，以及既有日历 / 价格 / 五页空态 / 单实例 / 托盘 / 电源路由通过退出 0，仍有 class unregister 1412 提示。原生库无用量，非空金额 / 名称 / 候选 / 迟到分页为明确浏览器合成 DTO 和 Rust / SQLite 独立证据，未宣称真实用户日志或 WSL 验收。性能测试未执行，旧 ignored 夹具保持跳过。
## M15d：持久应用主题与统一配色

AppTheme / DisplayThemeMutation 正式支持 dark / light / system，深色默认。Writer 只修改 theme，精确全局修订 CAS / 同事务 payload 与 revision / 同值不通知，保留隐私、时区和原生偏好。set_display_theme 列入主窗口 manifest / capability；成功后 settings_changed 同步所有已实现窗口。初始化和修改通过 Tauri set_theme 更新应用原生窗口外观；未来小窗仍须接相同配置和 hook。

主窗口使用共享 useAppTheme，深 / 浅色应用全套语义颜色变量，保持布局、趋势分区和蓝色操作 / 绿色费用。system 才监听 prefers-color-scheme 变化；显式模式不被系统覆盖。外观保存失败保留已确认主题，不提前改变 UI。主题变化不清空时区草稿或统计范围。增加 forced-colors 下边线 / 文字 / 焦点系统色基础支持，实际辅助技术和 OS 高对比度仍需相应验收。

2026-10-02 04:45 完整 261 项普通 Rust、29 项 Vitest、40 项 Playwright、workspace Clippy / fmt / 契约 / TS / 生产构建通过。新增两项 SQLite 多场景验证默认 / light / system 重启保存 / 同值 / 过期修订 / 保留隐私时区 / 数据与价格修订不变，以及 Writer 强制失败同时回滚、未来或损坏主题配置不替换。三项浏览器主题场景验证真实 CSS 前景 / 表面变化、明确合成持久桥 reload、系统媒体变化不写新设置、显式模式忽略系统、外部主题更新保留时区草稿并使旧编辑冲突、保存失败保持原主题。1280 / 960 浅色设置截图已查看，无横向溢出。此截图范围是设置表单，复杂非空统计 / 详情在浅色下进一步视觉检查仍待继续；未将 CSS 存在等同完整主题可访问性验收。

Win10 新构建独立应用实际设置 select light / system / dark，持久修订 9 / 10 / 11，时区 Asia/Tokyo 和隐私 false 保留；前端 themePreference 与明确模式 theme 一致。加上前序设置 / 隐私，共十条 settings_changed（2 至 11），四条隐私事件不增加；主题同值 / 冲突无额外通知。既有原生检查通过退出 0，仍有 class unregister 1412 提示。未更改 OS 个人外观设置来模拟实际系统切换；媒体变化为浏览器功能证据。未执行性能测试。
## M11a：持久小窗范围与原子本地用量快照

MiniScopeMutation / MiniScopeSnapshot / MiniUsageSnapshot 已实现，today_all_sources 为默认；固定会话须登记存在，today 按配置时区当天起点，fixed 保留明确 EpochMs。修改使用精确全局配置 CAS，原 payload / scope / 时间 / revision 同事务提交；同值不写，不变更数据或价格 revision。主窗口查询不会修改该范围。未来起点、缺失会话、非法 ID、旧修订拒绝，失败保留旧配置。

mini_usage 在一个真实 SQLite 读取事务中取配置 / 时区 / scope、Token、价格 catalog 修订 / 精确估算、覆盖和实际 parser / accounting 版本，输出明确 range。价格口径固定 event_time；当前采样毫秒包含在统计内，以 end_ms = generated_at_ms + 1 的半开边界表示，未来时间记录排除。缺失时区先报 INVALID_QUERY，不猜 UTC。MiniUsageSnapshot 是本地消费部分，未合并假额度；后续 MiniDisplayController 用独立账户快照组合完整 MiniSnapshot。发送通过 PrivateResponse，旧本地快照仍按最新策略隐藏名称 / 金额，不修改原 DTO 或数据库。

get_mini_scope / get_mini_usage / set_mini_scope 正式 Tauri 主窗口权限已注册。成功变更发 mini_scope_changed（仅修订与 scope，无名称 / 路径）及 settings_changed；无变化 / 冲突无通知。小窗权限与真正窗口一起开放，当前不授权不存在的窗口。两个小窗尺寸、窗口操作、账户快照组合和原生任务栏消费者仍待下一模块实现，不能将数据接口视为 M11 完整交付。

2026-10-02 04:56 完整 267 项普通 Rust、29 项 Vitest、TS / 契约 / workspace Clippy / fmt 通过；此前主题模块 40 项浏览器与生产构建通过，本模块未改 UI 实现，不重复全套浏览器。新增六项多场景：纽约 DST 前后同一今日起点 / 次日推进 / 固定不漂移 / 非法时区及未来起点 / 明确截止包含当前毫秒，SQLite 独立主筛选 / 未来记录排除 / null 分项 / 固定范围重启，真实旧事务在 scope 和价格替换提交后所有字段保持一致（独立预期 110 Token → USD 0.000110000000000），无效 / 缺失 / CAS / 强制 Writer 失败不覆盖，旧 mini DTO 最新隐私脱敏而原标签 / Token / 范围 / scope ID 保留。

Win10 新构建独立 WebView 实际读取默认范围和 Asia/Tokyo 用量快照，切换固定会话起点 0 后 revision 12、范围 start 0，再恢复全部今日 revision 13。当前 Token 0，输入仍 null，真实空库不伪造分项。未来起点 / 旧修订拒绝，同值不通知；恰好两条 mini_scope_changed，全部显示设置通知累计 12 条（2 至 13），隐私事件仍四条。回到项目页保留主窗口 2024-02-28 至 02-29 与明确估价时点 2024-02-29T00:00:00.123Z，证明独立范围。既有原生检查通过退出 0，仍有 class unregister 1412 提示。此证据为主窗口 WebView 调用正式 mini IPC，不是实际小窗 / 任务栏或非空用户日志验收。未执行性能测试。


## M11b：独立原生悬浮窗与真实用量显示

主导航底部与托盘新增悬浮窗入口，独立受限 mini WebView 复用生产静态前端，首次按 280×220 DIP 创建，展开为 360×380 DIP。Windows 创建路径在独立任务线程执行，避免同步 WebView2 死锁；创建锁避免并发重复。专用标题拖动区与按钮分开；置顶、展开、隐藏通过类型化 mini_window_action，原生失败保持已确认操作状态。关闭事件隐藏并保留窗口，托盘显示 / 恢复入口解除可能的鼠标忽略并复用已有窗口。没有开放未完成的穿透操作。

小窗只读取同事务 MiniUsageSnapshot，以完整 DTO 更新范围 / Token / 费用 / 覆盖；轮询、设置与价格失效通知、恢复可见时重新读取。小窗主题采用共享 useAppTheme；隐私采用同一持久 Writer / PrivacyState / DisplayPolicyGate，世代变化清除缓存和费用详情，旧迟到响应拒绝。mini capability 只开放范围 / 本地用量、显示配置读取、隐私变更、受控窗口操作与事件，不开放来源 / 价格管理 / 主窗口动作。主窗口 theme / timezone 写入仍保持其权限。

显示缓存占比时须具备完整输入 / 缓存分项；输出标明含推理。未知证据不展示可信零或 $0.00；确认零、未计价、部分已计价与隐私隐藏分开。费用展开显示精确六位显示估算、计价 Token / 未计价 Token / 价格修订，仍保留 Rust 完整精度。失败保留最近成功完整 DTO，并明确旧快照与时间含义。账户模块尚未连接，短周期 / 周百分比和周重置显示 — / 未连接；不从本地消费推测额度。当前展开内容可查看固定起点和返回今日全部，尚无完整会话选择器或同范围打开统计入口。

2026-10-02 05:12 类型、生产构建、29 项 Vitest、5 项新增小窗浏览器功能检查、9 项相关主窗口回归、workspace Clippy / fmt 通过。小窗浏览器桥仅使用明确合成数据，覆盖两尺寸 / 费用和覆盖、未知账户、深浅主题、隐私清 DOM / tooltip / 详情及迟到旧响应、旧快照、精确大修订和今日重置、普通浏览器不虚构值、未知 / 确认零 / 未计价区别。280×220 深色、360×380 深色 / 浅色截图已查看；页面无横向或整体竖向溢出，展开详情内部允许滚动。领域只新增窗口枚举 / 状态，无采集或核算变更，沿用此前 267 项普通 Rust，不重复全套。

独立 Win10 exe 的 NATIVE_MINI_OK / NATIVE_SMOKE_OK 返回 0：真实主页面点击创建第二 WebView；实际内尺寸按 scale factor 转 DIP 为 280×220 / 360×380；真实 Win32 WS_EX_TOPMOST 置顶标志；受限命令拒绝；主窗口切浅色后小窗同步；小窗实际隐私按钮提交后主窗口 IPC 路径隐藏且筛选保持；隐藏 / 显示复用、收起 / 取消置顶、关闭隐藏全部通过。恢复调用的是托盘同一实现，未模拟人工托盘菜单点击。拖动命令已接入，尚未人工鼠标拖动验收；多屏 / 断屏 / DPI 变化、跨启动位置与尺寸 / 置顶偏好、恢复快捷键 / 冲突 / 透明度 / 穿透尚未完成。原生 probe 没有真实日志或用量，非空费用为浏览器合成 + 既有 SQLite 数学证据；不宣称非空原生消费或账户验收。仍有 WebView class unregister 1412 提示；未执行性能测试。


## M11c：从小窗打开精确范围统计

MiniStatsOpenRequest 携带小窗实际快照 expected_settings_revision；open_mini_stats 在真实读取事务中再次读取 mini 使用范围并比较修订，旧范围拒绝，不导航到与显示不同的新会话。成功形成 MiniStatsRequest，保留固定会话 key、精确半开 UTC 毫秒范围，使用采样时刻和同一时区解析日历活动范围；事件时点价格 / 全部来源 / 无其他维度筛选为明确默认。意图只含稳定 ID 与日历，不发送会话名称 / 路径 / 金额 / 账户字段。main-only getter 保留最新意图，mini-only opener 显示主窗口并只发失效通知。

主窗口独立 useMiniStatsRequest 监听、启动读取、恢复可见读取，序列与 request_id 防止旧读取 / 同一意图覆盖用户后续筛选。应用新意图显式进入总览并清理原来源 / 其他维度 / 指定计价时点，显示完整年 / 日 / 秒 / 毫秒范围且注明不含结束时刻，避免整日筛选器冒充精确范围。用户改用主窗口日期、热力图日期或重置只改变主页面；不写 mini_scope，账户服务也不参与。小窗按钮串行且失败保留当前数据，范围修订冲突可以刷新后重试。

2026-10-02 05:31 五项 mini SQLite 专项通过，新增精确起点 1501 ms / 截止 2001 ms / 会话事件预期 7 Token、独立日历活动范围、无额外设置写入及旧修订打开拒绝。之前硬编码夹具修订导致新检查冲突，改为读取真实夹具设置版本后通过，没有改业务 CAS 规则。29 项 Vitest、TS / 生产构建、workspace Clippy / fmt / 契约检查通过；浏览器 13 项小窗 / 总览与 22 项时区 / 会话 / 筛选回归通过。新增主窗口场景验证明确导航保留 .123/.457 毫秒、会话 / 价格 / 来源一同切换、刷新不漂移、恢复旧主日期、重复可见不再应用同一意图、后续全来源意图与重置。1280 主窗口截图、更新后两尺寸小窗截图已查看。

真实 Win10 NATIVE_MINI_NAVIGATION_OK / NATIVE_MINI_OK / NATIVE_SMOKE_OK 退出 0：实际小窗按钮打开主窗口，固定会话 start 1709179200123 原样进入真实 get_dashboard_bundle filter，结束时刻和时区与后台意图一致；旧设置修订显式拒绝；主窗口恢复自身日期不修改小窗固定起点。最初检查尝试替换 Tauri invoke 捕获参数，因其属性只读而未生效；按本机 Tauri 源码改为仅 --native-smoke 的 debug 后端请求观察，release 不包含观察字段或分支，再复核通过。原生身份 / 库仍为隔离合成，未读取真实日志，不宣称非空原生价格或账户支持。WebView 1412 提示仍存在。完整选择器、自选起点 UI、位置 / 偏好恢复、快捷键 / 透明度 / 穿透与账户 / 任务栏仍待后续模块，未执行性能测试。

## M11d：会话选择、自选起点与主窗口固定入口

已登记会话候选使用正式 MiniSessionsRequest / Page，不要求范围内已有消费。排除已验证镜像别名，保留同名不同 key；真实 SQLite 租约固定候选、名称、时区和修订，Unicode 字面搜索 / BINARY keyset / 已认证能力分页与其他查询分域。owner / 完整搜索 / 页长绑定，主窗口和小窗不能互用游标；末页和失败释放。mini 的 close_query_snapshot 只授权 mini_sessions，不能关闭主查询。

360×380 小窗范围编辑器提供全部今日、搜索 / 分页、固定会话、每日统计时区零点或明确 UTC 毫秒起点。打开保存 CAS 基线，后台变化不覆盖未保存选择，冲突保持草稿并将错误滚入可视区域。支持焦点限制 / Escape / 取消返回原按钮；显示策略变化清除搜索、名称和编辑器，迟到页使用原能力清理。主窗口会话详情增加今日 / 所选精确起点固定按钮，成功后显示小窗；主页面筛选与账户范围保持独立。

2026-10-02 06:00 完整 272 项普通 Rust、29 项 Vitest、51 项 Playwright、workspace Clippy / fmt / 契约 / TS / 生产构建通过；错误可见性调整后 9 项 mini 再次通过。新增四项 SQLite 多场景验证无消费会话 / 同名身份 / 镜像排除、写入期间冻结分页、owner / 搜索 / 页长 / MAC 绑定与过期清理、Unicode 和 SQL 符号字面匹配、无效 / 未知字段 / 时区失败释放、旧 DTO 最新隐私脱敏；新增四项 mini 和一项主详情浏览器场景验证分页、固定 .123 / .456 起点、未来拒绝、CAS 草稿、搜索 / 关闭清理、焦点、迟到隐私响应及独立主筛选。深浅小窗编辑器、冲突提示可见与主详情固定入口截图已查看，无整体溢出；候选列表与内容区允许内部滚动。既有抽屉 Shift+Tab 预期更新为新增最后一个固定按钮。

真实 Win10 NATIVE_MINI_SCOPE_OK / NATIVE_MINI_NAVIGATION_OK / NATIVE_MINI_OK / NATIVE_SMOKE_OK：独立小窗实际选择隔离库中没有消费的已登记会话，输入 2024-02-29T01:02:03.123 UTC 后 SQLite 读回毫秒一致，再切换今日起点；正式候选 DTO 可读取，小窗关闭主查询返回 PERMISSION_DENIED，主筛选保持。随后实际同范围跳转仍传递精确起点。验收脚本初次计价依据写错 kind 而非 mode，按契约修正；连续同名会话不同起点导致旧名称检查过早，改等实际起点 .123 显示后点击，未改变业务规则。原生身份和库为明确隔离合成，没有读取真实用户日志；非空价格和主详情固定为浏览器 / SQLite 证据，实际账户、任务栏和 Win11 仍待后续。WebView class unregister 1412 提示仍存在；未执行性能测试。

## M11e：小窗持久原生偏好与工作区位置恢复

新增纯领域 WindowPlacement / WorkArea / MiniWindowPreferences。保存屏幕标识和相对工作区的 DIP 偏移；恢复使用目标屏当前工作区和 DPI，不保留已失效物理坐标。找不到旧屏幕时选择主屏并夹紧；超小工作区至少保留窗口标题入口。坐标 / 缩放 / 屏幕标识与 JSON 结构严格校验，NaN / Infinity / 过大偏移拒绝。位置为内部原生配置，不对 WebView 开放任意坐标 / 窗口 / 路径命令。

settings payload 的 mini_window 包含展开、置顶和可空位置；缺少旧字段使用已有紧凑 / 置顶默认，新结构损坏或未来配置版本返回错误。Writer 只改一个原生字段，配置与全局 settings_revision 同事务提交，不写 data / price revision；同值不写，保留主题 / 时区 / 隐私 / mini_scope。原生按钮成功后重读实际快照。尺寸 / 置顶原生操作后 SQLite 提交失败则回滚该操作，已确认状态保留。新建前读取持久配置；显示 / 展开 / 隐藏 / close-to-hide / 退出保存位置。拖动 / DPI / WM_DISPLAYCHANGE 由单个合并 worker 在 250 ms 静止后采集，无逐事件线程创建。

2026-10-02 06:17 新增 2 项领域检查与 2 项 SQLite 多场景通过。独立数值预期覆盖负坐标、100% → 200% 相同 DIP 偏移、125% 工作区边界、缺屏 / 负偏移 / 超小区域与无效数值；SQLite 真实旧事务在偏好写入后保持旧字段、同值无 revision、主题 / 时区 / 默认范围保留、drop 后重开偏好、无效 / 未来配置 / Writer 故障不覆盖。完整 store lib 151 项普通检查通过，既有 1 项性能夹具 ignored；此前完整 Rust 272 项基线仍在，本次没有重新运行采集全套。29 项 Vitest、9 项 mini 浏览器功能、Clippy / fmt / 契约 / TS / 生产构建通过；没有 UI 布局变化，沿用上一模块已审查截图。

真实 Win10 当前 150% 缩放：NATIVE_MINI_PLACEMENT_OK / SCOPE / NAVIGATION / MINI / SMOKE 退出 0。移动到工作区相对 (80,90) DIP、持久展开 / 取消置顶后销毁真实 WebView 并清空进程内状态，重新创建从 SQLite 恢复 360×380、位置与置顶按钮。debug 隔离库注入修订写入故障，实际置顶请求返回 DB_WRITE_FAILED，SQLite / RuntimeState 仍为 false，Win32 WS_EX_TOPMOST 确认回滚。保存不存在屏幕标识与超大偏移，重建窗口确认整窗处于真实 2560×1380 工作区。首次检查暴露隐藏 Win32 窗口中间客户区高度比完成后少 30 DIP，已改为新建时使用确定的产品 DIP 尺寸计算，复核通过。所有原生身份 / 目录均为隔离 probe，故障注入连接只存在 debug 验收，不开放应用 Writer 或通用 SQL。

SQLite 重开与实际 WebView 重建分别是持久化 / 原生恢复证据，不冒充完整应用进程冷启动、人工多屏拖动 / 物理拔屏 / 跨屏 DPI 验收。WM_DISPLAYCHANGE 已接恢复调度，但本次没有改变 OS 显示配置。主窗口位置、透明度、恢复快捷键与穿透仍待实施；穿透继续不开放。窗口位置保存故障仅打印受控错误码，后续完整诊断需接入。WebView class unregister 1412 退出提示仍存在；未执行性能测试。

## M15e / M11 前置：持久恢复快捷键与原生注册

恢复键默认为 Ctrl+Alt+Shift+T，支持 Ctrl / Alt / Shift 与 A–Z / 0–9 / F1–F11 的受限结构。至少 Ctrl 或 Alt；F12、Windows 键、任意 native code、非规范大小写 / 空 / 控制字符拒绝，JSON Schema 限制 key 形状。正式 DTO 区分持久组合与 ready / conflict / unsupported / unavailable 的实际注册状态；不可读配置不显示假默认注册。main / mini 可读，只有 main 可以修改，普通 Response 不带敏感字段。

Windows 主 HWND 所属线程注册两个自有槽位；替换先占新槽位，CAS Writer 成功后注销旧槽位。外部冲突不改配置 / 旧键；Writer 或并发修订失败撤销候选注册。startup 冲突保持应用与托盘可用；WM_HOTKEY 验证拥有的 ID / modifiers / key 后异步恢复小窗并解除鼠标忽略，WndProc 不同步创建 WebView。WM_NCDESTROY 注销自有键。设置页提供真实状态、编辑、重新注册、刷新和明确重置；刷新 / 冲突保留草稿基线，至少一个修饰键的无效选择在 IPC 前拒绝。主题 / 时区 / 隐私 / 原生小窗偏好及统计范围保持。

2026-10-02 06:44 新增 1 项领域与 2 项 SQLite 多场景，涵盖虚拟键独立字面预期、保留键 / 非规范结构 / 超大修订验证、同值、CAS、重开、其他配置保留、Writer 回滚及损坏 / 未来配置不覆盖。核心库与 core 全套普通检查、完整 store lib 153 项普通检查通过，既有 1 项性能夹具 ignored。29 项 Vitest、完整 53 项 Playwright（新增 2 项快捷键）、Clippy / fmt / 契约 / TS / 生产构建通过。既有状态定位缩小到显示设置保存结果；新增注册状态有独立 accessible name。StrictMode 曾使旧订阅异步完成后借用已复活 mounted ref，造成监听泄漏，已改为各 effect 独立 live 标志；在设置内监听数为全局 + 本面板两条，离开后只剩全局，反复进入验证无泄漏。960 设置截图已查看，无横向溢出，键组合与按钮可读。

实际 Win10 NATIVE_RECOVERY_SHORTCUT_OK 及此前 MINI / SCOPE / NAVIGATION / PLACEMENT / SMOKE 全部退出 0：实际设置 select 改 U 并保存，第二 native 线程 RegisterHotKey 同组合返回 1409 证明全局拥有，旧拥有组合可重新注册证明已释放；SendInput 发送完整 Ctrl / Alt / Shift / U 按下抬起序列，真实 WM_HOTKEY 使隐藏小窗显示，Win32 WS_EX_TRANSPARENT 确认解除。其他线程占 V 后实际界面保存返回冲突，SQLite / 注册仍为 U、草稿仍为 V、revision 不变；隔离 SQLite 故障使候选 V 注册回滚，可再次占 V 而 U 仍被保留。mini 正式 getter 能读 ready / U，合法写入参数被权限拒绝。鼠标忽略是 debug probe 直接注入以验恢复，没有开放正式穿透入口。验收库 / 身份均为隔离合成；没有访问真实日志或认证。

Win32 注册与窗口线程、MOD_NOREPEAT、Windows 键与 F12 限制以 [Microsoft RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey) 和 [WM_HOTKEY](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-hotkey) 为依据。全进程冷启动配置恢复、退出后独立进程检查释放、人工按键 / 托盘恢复、Win11 仍需后续系统验收。透明度、正式穿透、其他快捷键仍待实现；穿透只有在恢复键实际注册成功后才允许。WebView 1412 退出提示仍存在；未执行性能测试。

## M11f：持久原生小窗透明度

小窗透明度沿用原型 70%–100% 范围，默认 100，作用于整个原生窗口，未引入 CSS 假透明度。main 设置页明确保存、读取确认状态与平台支持、精确 CAS、刷新保留草稿、失败保留输入 / 明确重置；mini 只读透明度。设置兼容旧 mini_window 缺少 opacity_percent，错误 / null / 越界不写默认。Writer 仅改该字段和全局修订，同值无写入；原生偏好 / 显示 / 范围等字段保留。

2026-10-02：新增 1 项纯领域预期（70→179、75→191、80→204、90→230、99→252、100→255，非法整数 / JSON 拒绝）、2 项实际 SQLite 多场景（CAS / 同值 / 旧配置 / Writer 故障 / 独立字段 / 重开）通过。完整 core 81 项、store lib 155 项普通检查通过，既有性能夹具 1 项 ignored；Vitest 29、Playwright 55、契约 / TS / Clippy warnings denied / fmt / 生产构建通过。浏览器合成 DTO 场景验证大修订、冲突 / Writer 失败后草稿和已保存值分离、不支持禁用写入；960 px 设置页截图已审查，无横向溢出。

实际 Win10 22H2 当前 150% 缩放：NATIVE_MINI_OPACITY_OK / RECOVERY_SHORTCUT / MINI / PLACEMENT / SCOPE / NAVIGATION / SMOKE 退出 0。真实设置页滑块保存 80%，GetLayeredWindowAttributes 独立检查实际 HWND alpha 255→204；置顶往返保持 204。debug 隔离库修订写入 trigger 故障时，请求 DB_WRITE_FAILED，数据库 80% / 修订保持，实际 alpha 回滚 204。mini 可以读取、不能修改。注入鼠标忽略后调用真实恢复入口，WS_EX_TRANSPARENT 解除且 alpha 保持 204；销毁并重新创建 WebView，从持久设置恢复 204。SQLite 重开与 WebView 重建分别记录，不等同完整进程冷启动验收。

首次原生检查发现 Tao 的 show / hide / pin 样式重建丢失直接设置的 WS_EX_LAYERED；已使用小窗 HWND 所属线程的 subclass 保留自有 layered 位，其他样式仍按 Tauri 管理，销毁移除。应用创建锁序列化显示与透明度更新，后台 Worker 不在窗口消息回调创建 WebView。SetLayeredWindowAttributes 失败 / Writer 失败都走受控错误和撤销；实际 API 故障尚无独立 OS 条件注入，不能将 Writer 故障替代该项。

未执行性能测试。Windows 11、完整应用冷启动、人工多屏 / DPI 组合与半透明窗口人工视觉验收仍待完成；此次实际系统证据为 HWND 属性和真实 UI 保存 / 恢复操作。正式鼠标穿透入口及其有效恢复键门禁继续实施。既有 WebView class unregister 1412 退出提示仍存在，进程退出 0。

## M11g：受控原生鼠标穿透与可靠恢复

正式显示设置加入穿透说明、实际恢复键 / 注册状态、明确确认和开启 / 关闭 / 恢复 / 刷新。确认绑定 key 与精确修订，未创建小窗、未注册恢复键、未确认或键改变时不允许开启。后端独立校验自有 OS 注册与当前保存键，在 HWND 所属线程执行鼠标忽略与 native alpha，创建锁串行化显示 / 透明度 / 穿透；不将 key 锁交给等待原生线程的 worker。main / mini 可读穿透，只有 main 可修改；不开放任意 HWND / 消息 / native key。

MiniWindowPreferences 保存 passthrough，旧配置缺少字段默认 false，null / 非 bool 拒绝。设置与全局修订同事务 CAS；只改该字段，其他偏好保留。开启提交失败撤销 native 操作。关闭 / 恢复时不会因保存失败重新启用：先恢复真实鼠标，再保存 false；真值与持久值不同的情况通过 DTO 明确展示，允许重试。已有窗口的配置读取 / 位置保存错误不阻止恢复；alpha 重设失败只记录，不妨碍鼠标可操作。快捷键 / 托盘 / 主窗口显示入口共同调用恢复路径。明确重新显示关闭穿透；尚未实现登录启动偏好，不声称自动启动恢复已完成。

2026-10-02 自动检查：新增 1 项领域多场景（显式恢复确认 / 禁止多余关闭确认 / 大修订 / 未知字段）、2 项真实 SQLite 多场景（读事务一致的 key / 偏好 / 修订、CAS / 同值 / 错误 key、独立字段、重开、旧字段缺省、坏字段 / Writer 故障保留）通过。完整 core 82、store lib 157 项普通检查通过；既有 1 项性能夹具 ignored。Vitest 29、Playwright 58、契约 / TS / Clippy warnings denied / fmt / 生产构建通过。新增浏览器 DTO 场景验证启用门禁、精确修订、确认草稿、恢复动作、native 与保存状态不同的重试和订阅卸载；960 px 设置页截图已审查，未改变主窗口导航或筛选布局。未执行性能测试。

真实 Win10 22H2 / 当前 150%：NATIVE_MINI_PASSTHROUGH_OK 与既有全部原生 marker / SMOKE 退出 0。debug 在主 HWND 线程释放恢复键后，由独立线程实际占用相同 global key，正式启用请求返回 SHORTCUT_CONFLICT，原生 / 持久穿透均 false；解除外部占用后恢复注册。实际设置页勾选确认并开启，HWND WS_EX_TRANSPARENT 生效，alpha 204 保留。独立线程创建下方 native 鼠标计数窗口；关闭穿透时真实鼠标 SendInput 没有落到该窗口，开启后同一位置点击准确落到该窗口（一次 WM_LBUTTONDOWN），不使用 DOM 点击或 SendMessage 冒充鼠标穿透。真实恢复键 SendInput 清除 native 与持久穿透，小窗可见、alpha 不变。

隔离库 trigger 注入修订写入失败：开启请求 DB_WRITE_FAILED，实际鼠标穿透撤销且偏好 / 修订保留。再次开启后，在保存失败条件下按真实恢复键，小窗仍变可操作 / 可见，DTO enabled=false / persisted_enabled=true，设置页提示保存未同步；移除 trigger 后点击关闭完成重试。mini 正式 reader 成功、setter 权限拒绝。原生故障只作用于 debug 隔离库；不访问真实 Codex 日志 / 账户。MINI_PASSTHROUGH_RECOVERY_SAVE_FAILED 是故障注入的预期受控码。

复核期间一次早期会话编辑原生 probe 在按钮尚 disabled（展开后真实数据重读未结束）时点击，候选等待超时；已将前置等待改为实际按钮可用后点击，保留原候选 / 固定起点 / 权限断言。最终全套原生通过；此修改是检查真实操作前置状态，不延长 / 忽略业务失败。实际系统 API 失败、完整进程冷启动、人工托盘菜单、物理多屏 / DPI 和 Win11 仍分别待验收。既有 WebView class unregister 1412 退出提示仍存在，进程成功退出。

## M12a：账户额度解析与独立内存协调器

新增 quota 领域模块，优先采用 rateLimitsByLimitId（空映射也具有权威性），兼容单一 rateLimits。桶按真实标识保持独立；缺少明确 codex 桶且有多个候选时不猜选择。选定桶移除后不暗中切换账户额度。周窗口只按实际 10080 分钟识别，短周期按唯一最短的已知实际时长识别，不将 primary / secondary 固定解释为五小时 / 一周。缺失、非法或越界数值保留 null，真实 0% 保留；Unix 秒按 checked 运算转换毫秒。多余账户、邮件、计划、credits 和认证字段丢弃，不进入 DTO 或日志。

QuotaCoordinator 用连接 epoch、请求 ID、十进制修订与单调时钟管理快照。账户变化 / 断开清除旧身份和额度；完成有效读取前不接受通知。已识别桶的通知只更新该桶，完整映射才替换全体；无身份通知只能更新已证明的旧版单桶，否则要求完整重查。读取开始后到达的新通知不被较旧回复覆盖。普通请求 10 秒超时，5 秒刷新下限、5 / 15 / 30 / 60 秒失败退避，显示入口可见 / 隐藏分别按 60 秒 / 5 分钟调度。失败和陈旧保留最近成功值及获取时间；重置时间到达不自行补满额度。此模块尚未连接外部进程或正式账户 IPC。

自动检查：12 项独立合成多场景覆盖实际窗口顺序、空映射、非法值 / 溢出 / 负 Unix 时间、账户类型、过期 epoch / 请求、超时、单飞、时钟倒退、退避、通知与读取竞争、多个桶与显式选择、legacy 名称碰撞和陈旧时间。完整 core 94 项、29 项 Vitest、契约生成 / 差异检查、workspace Clippy warnings denied、fmt、严格类型和前端生产构建通过。未更改 UI 和原生集成，因此不重复浏览器 / 原生小窗全套；未执行性能测试。

实际本机只验证 codex-cli 0.130.0 的版本和离线 app-server generate-json-schema，使用新建的隔离临时 CODEX_HOME。实际 Schema 与 [官方 App Server 文档](https://learn.chatgpt.com/docs/app-server) 对照后确认单桶通知和多桶读取差异；没有启动现有账户连接、读取 auth.json 或执行授权 / 网络额度查询。该检查不是实际账户握手与额度验收。受控 stdio 宿主、登录 / 取消、权限、主窗口与小窗额度显示以及真实账户条件继续实施。

## M12b：独立受控 stdio 账户通信库

新增 token-pulse-quota crate，由后台 NativeService 配置选定原生程序及可选 CODEX_HOME。要求绝对路径和实际文件 / 目录；Windows 原路径与 canonical 目标都须 .exe，不执行 .cmd / .ps1 / shell，不接受任意参数。唯一启动参数是 app-server，工作目录与服务 Home / 程序目录绑定，不把本项目作为 cwd。初始化发送明确客户端和 experimentalApi=false，校验成功后只发送一次 initialized；初始化前 / 失败后账户请求拒绝。

当前可调用方法仅 account/read（refreshToken=false）及 account/rateLimits/read，类型化枚举没有任意 RPC / 参数 / API key / 访问令牌入口。服务主动请求统一返回 -32601，不执行工具、审批或认证刷新；未知通知和旧 ID 回复丢弃。回复只发布已解析的 AccountAvailability / QuotaBook / QuotaUpdate，不缓存服务 Home、邮件、认证、原始错误文字或未知消息。account/updated 先使旧读取失效，再交给 owner 重新证明新身份；此库不自行发起登录或修改远端账户。

stdout 每帧上限 1 MiB，限长读取不使用无限 read_line；队列 16 帧、输入 4 帧、活动请求 4 条有界。后台写入也有 10 秒期限；回复 / 初始化默认 10 秒，迟到回复不能满足新请求。stderr 用固定 buffer 排空，只保留丢弃字节计数，原文不进入诊断。Windows 以 CREATE_SUSPENDED 创建自有进程，加入 kill-on-close Job 后仅恢复该自有主线程；断开先释放队列，再结束 Job / 子进程并回收读取、写入、stderr worker。不会终止用户原有 Codex 进程。Job / 恢复失败不退化为无约束运行。

自动功能检查：3 项纯协议 / framing 多场景、8 项实际合成 exe / OS pipe 场景通过。覆盖握手顺序、固定方法、敏感字段丢弃、账户通知与旧读取、单飞、主动工具请求拒绝、未知 / 旧消息、初始化不支持 / 损坏、超长 / 非 JSON / 截断帧、stderr 排空、队列填满后断开、真实 10 秒超时及迟到回复拒绝。Windows 系统 API 场景真实持有后代进程 handle：断开前 WAIT_TIMEOUT，Job 关闭后 WAIT_OBJECT_0，并确认继承管道全部释放。这个证据属于合成服务的实际原生进程生命周期，不是正式应用授权 / 网络账户 / WebView 验收。路径检查对可用的 Windows symlink 能力附加 canonical 脚本目标拒绝，缺少创建权限时该条件场景不计为实际通过。

workspace all-target Clippy、夹具特性 all-target Clippy warnings denied、fmt 与差异空白检查通过；路径 canonical 检查收紧后针对性检查再次通过。CI 增加显式 test-fixture 检查；合成 exe 仅在该特性开启时构建，普通生产构建不包含它。未改前端或采集算法，不重复旧浏览器 / 小窗原生检查；没有执行性能测试。通信库尚未接到 Tauri 生命周期和前端，后续继续串行 owner / 轮询恢复、用户授权 / 取消、配置权限和真实 UI。

## M12c：持续账户服务、生命周期与共享读取 IPC

AccountQuotaService 单一 owner 持有额度协调器和自有 stdio 连接。默认快照 disconnected，只有明确后台 connect 才建立连接；不发现或自动连接本机账户。初始化 → account/read → 有效 ChatGPT 模式 → 完整额度读取由 owner 驱动。连接控制绑定当前 epoch，额度桶选择另外比较 quota_revision；旧动作拒绝。16 条控制队列有界，过期 / 取消的排队控制拒绝；快照读取不等待网络，显示失败不会停止本地采集。

收到 account/updated 时清除旧快照 / 读取映射、旋转账户 epoch，重新验证账户；未经完整额度证明的新通知不会发布旧额度。服务退出 / 握手错误没有额度令牌，因此新增 connection_failed 受控入口：保留已知成功值并标 stale，未曾成功为 error；不支持 / 需要授权清空值。可重试连接失败按 5 / 15 / 30 / 60 秒退避，新 transport 旋转 epoch 并先发布 connecting，不能用旧身份冒充新连接。普通额度失败沿用领域单飞与退避，不重启合法服务，也不更改本地用量时间。

后台每步使用 owner 内部单调采样；可见入口合并 main / mini / taskbar 位，某一入口隐藏不会覆盖其他入口。恢复可见 / resume 请求补查，仍遵守最小间隔和在途请求；suspend 不开始新查询，但接收必要通知以撤销旧身份。实际已到的 selected 窗口 reset 每个时间值触发一次补查，不能自行补满百分比；未知 reset 不猜测。原生任务栏位目前仅为内部消费者接口，未创建任务栏宿主。

Tauri setup 建立默认断开的服务，shutdown 回收 owner / 自有子进程，Windows power 消息只设置非阻塞服务标志。原生窗口显示 / 隐藏 / close-to-hide、焦点 / 尺寸 / destroy 核对实际 IsWindowVisible / IsIconic 更新可见性。get_account_quota / refresh_account_quota 仅 main / mini；前者读取内存完整快照，后者返回 QuotaRefreshResult（started / in_flight / rate_limited / not_due，retry_after_ms 明确可空），保留旧成功值。两者经过最新 PrivateResponse，嵌套额度名称同样脱敏；account_quota_changed 仅发送 connection_epoch / quota_revision / state。尚无任意路径、任意方法或 WebView 连接 / 登录命令。

自动检查：完整 core 96 项通过，新增两项多场景覆盖传输失败保留 75% / 原获取时间、旧 epoch / 无效错误拒绝、新连接清空，以及精确限流状态、真实 0%、嵌套 DTO 的最新隐私与原值保留、溢出 / 未知字段 / 无敏感事件字段。quota 全部 3 + 8 + 7 项通过；新增持续服务实际合成 exe 场景覆盖默认不连接 / 幂等 shutdown、握手 / 身份 / 多桶与 CAS、授权缺失和 API key 不发额度查询、切换账户清空及拒绝未证明通知、进程退出后的 stale、suspend / resume 后最小间隔补查，以及握手错误后真实退避重连 / 新身份证明。最后新增重连发布前的状态同步后，7 项服务检查和夹具特性 Clippy 再通过。契约检查、严格 TS / 生产构建、29 项 Vitest、workspace / 夹具特性 all-target Clippy warnings denied、fmt / 空白检查通过。

Win10 独立新构建 --native-smoke 退出 0：main 与 mini 的真实正式 IPC 均读 disconnected / windows 空 / fetched_at_ms null，refresh 明确 QUOTA_DISCONNECTED；主响应包含正确最新隐私戳。已有真实 WebView、power 路由、托盘 / 关闭隐藏 / 单实例、小窗尺寸 / 范围 / 精确导航 / 位置、快捷键、透明度、穿透及故障恢复 marker 全部通过。这个系统检查没有连接真实 Codex 服务或账户，在线授权、额度刷新、服务可见性轮询在实际账户下仍待后续。既有 WebView class unregister 1412 提示仍存在，退出成功。没有 UI 布局变化，不重复浏览器截图 / Playwright 全套；没有执行性能测试。

后续继续受控原生程序 / Home 选择和持久连接配置、明确授权 / 取消、主窗口与小窗真实额度内容及账户桶 UI。账户快照不持久化、不混入本地统计，实际账户授权须通过明确应用流程，不能使用当前 Codex 对话工具替代。

## M12d1：连接配置持久化、选择能力与程序指纹

内部 AccountServicePreferences 保存可空目标与 auto_connect，缺少整个旧字段时默认未配置 / false；启用自动连接必须有目标。AccountServiceTarget 只含原生程序路径、可选 Home 与 SHA-256，不含认证字段。Windows 约束为本地绝对路径与 .exe，控制字符、相对路径、UNC / device / 父目录组件、非法指纹及未知字段拒绝。当前主进程尚未读取该偏好并自动连接；auto_connect 的显式用户入口和启动接线继续实现，不以存储字段当作完成证据。

SQLite account_service_preferences 在同一真实事务读取目标与 settings_revision；mutate_account_service 窄更新该字段与全局修订，保存前校验，CAS / 同值 / 原子提交 / Writer 回滚。其他窗口偏好、隐私、主题、时区、范围和 data / price revision 保留。未来版本和损坏配置不写默认值，不把凭据字段作为可忽略附加信息。

公开 DTO 为已确认配置预览、原生选择请求 / 结果、配置保存和连接管理请求。Renderer 连接请求仅提交预期配置修订、连接 epoch 和已确认程序指纹，不提交执行路径、参数、token 或任意 RPC。选择能力与来源目录能力分域，main-only、300 秒过期、16 个上限；候选携带选择时 settings_revision，后续原生选择 / 保存会复核。路径预览经过最新隐私脱敏，未知 Home 保留 null，指纹与控制 ID 可用于确认但不属于认证信息。当前这些管理命令尚未注册。

NativeService::inspect 只规范化路径 / 检查文件与目录元数据并哈希程序；from_target 检查已保存指纹，实际 launch 前再次检查以覆盖排队期间的修改。Windows 在哈希和 spawn 期间持有 FILE_SHARE_READ 文件句柄，拒绝写入 / 删除替换；指纹变化返回 STALE_CONFIRMATION，不启动新的服务。该指纹用于检测已选择程序变化，不是发布签名或恶意软件认证；首版程序版本升级后需重新确认目标，签名 / 更新仍在 M16 范围。

自动验证：新增 3 项领域多场景（路径 / 指纹 / 认证字段 / Renderer 额外路径拒绝、精确大修订、能力 owner / 分域 / 过期 / 容量、最新隐私保留未知与原 DTO）、2 项 SQLite 多场景（真实旧读事务、CAS / 同值 / 重开 / 独立字段、Writer 故障与非法 / 未来 / 损坏不覆盖）通过。新增实际合成程序检查先以保存指纹完成真实 Windows 管道握手，再改程序字节，配置验证和实际启动均拒绝且没有生成新 PID 标记。另以独立已知 abc SHA-256 常量核对哈希，真实 Windows 文件句柄持有期间写入和删除拒绝，释放后允许修改。该专项通过后仅调整测试模块位置修复 Clippy items_after_test_module，没有改运行行为。

完整 core 99、store lib 159 项普通检查（1 项性能 ignored）、quota 19 项全套及后加 1 项指纹专项、29 项 Vitest、契约生成 / 检查、严格 TS / 生产构建、workspace 与夹具特性 Clippy warnings denied、fmt / 差异空白通过。没有 UI 实现变化，未重复 Playwright / 真实 WebView 全套；上一模块的独立 Win10 检查仍为生命周期 / 默认断开基线。此处真实 OS 文件 / 合成 exe 验证不代表原生文件对话框、用户授权、自动启动或在线额度已验收。未执行性能测试。


## M12d2：原生选择、正式连接命令和设置界面

主窗口注册 get_account_service_config / choose_account_service / cancel_account_service_selection / save_account_service_config / manage_account_connection，后端再次验证 main label 与 request_id。程序和 Home 使用原生文件 / 目录对话框；current 选择重验当前程序用于单独编辑启动偏好，default_home 保留真正的 null。Renderer 只能提交原生能力、精确设置修订与已确认 SHA；没有任意路径执行接口。原生选择复核原设置修订和最新隐私，替换能力只在新选择成功后释放旧能力。失败写入保留旧配置和能力，成功写入发送 settings_changed。

连接 / 断开 / 桶选择经过串行配置操作与账户 owner；connect 必须匹配最新配置修订、程序指纹和连接 epoch；select_limit 还匹配额度修订。disconnect 不依赖 SQLite 配置可读，不删除已保存的 auto_connect。默认未配置不启动任何服务，显式保存的 auto_connect 在 setup 读取后执行；本地验证失败发布新的 error epoch / QUOTA_SERVICE_UNAVAILABLE，没有假连接或无限重试。账户服务继续不依赖本地消费 / 价格库，不读取 auth.json。

设置页沿用数据来源页签和现有布局 / 主题，分已保存配置与当前连接；草稿保存不会替换当前账户。显示实际连接状态、可选桶、实际周期、剩余百分比、已保存时区的重置时间、最近成功读取及陈旧提示。0% 与 null 不混同，已到重置显示等待额度更新，不自行补满。配置 / 额度失效事件只触发完整重读，旧序列不能覆盖新操作；隐私 epoch 立即封闭路径与草稿，迟到选择返还独立能力，卸载清理订阅。授权缺失明确显示，当前未提供假登录动作。

自动检查：新增 3 项浏览器多场景覆盖显式选择 / 保存不连接、精确大修订 / 指纹、真实零与未知、写入失败 / 冲突草稿保留、桶选择 / 断开偏好保留、隐私清除 / 能力返还 / 授权独立和订阅清理；完整 Playwright 61 项通过。新增迟到原生选择隐私拒绝 / 控制释放单元场景，Vitest 30 项通过。新增失效已保存目标发布新错误 epoch、旧 epoch 拒绝、未知值保留和不自动重试，quota 全套 21 项通过；core 99 项、协议生成检查、strict TS / 生产构建、Clippy warnings denied / fmt / 空白检查通过。1280 宽设置页截图已检查主题、细边框、路径换行、周期卡和无横向溢出。合成桥只在测试文件中，生产无演示数值。

实际 Win10 检查：独立静态构建 --native-smoke 退出 0。仅在隔离 probe 目录复制合成原生 exe / 创建测试 Home，注入受控选择能力，实际正式 IPC 保存不启动；SQLite Writer trigger 拒绝保存时旧修订 / 指纹 / 偏好保留，同能力重试成功。实际保存启动钩子读取 auto_connect 并连接（同进程，不是完整冷启动）；真实 WebView 复核 current 指纹并修改 / 保存偏好，确认不替换当前连接。错误指纹和修订拒绝不换 epoch；点击连接后等待新的 epoch / ready，实际选择另一桶、断开并观察 Windows 自有进程句柄终止，小窗允许读取但拒绝全部配置 / 管理能力。既有小窗尺寸 / 精确范围 / 位置、快捷键 / 透明度 / 穿透、power / 托盘 / 单实例 / 退出全部 marker 通过。

首轮既有 opacity probe 等待超时，独立复跑通过；新账户 probe 曾在点击重连后误把旧 ready 内容当作新连接，改为检查新 epoch / ready 和按钮结束忙碌后完整运行通过。既有 WebView2 class unregister 1412 输出仍出现且退出 0，不记作安装 / Win11 验收。原生 OS 选择对话框交互尚未自动驱动；本模块系统验证从独立能力继续，不冒称文件对话框已验收。真实 Codex 账户登录、在线额度 / 长期刷新、完整冷进程启动连接、Win11 仍需外部条件。下一模块继续受控登录 / 取消及共享额度展示。未执行性能测试。


## M12e：按用户修订复用本地已登录账户

2026-10-02 用户明确“直接抓本地登录信息”，并要求同步文档。实施口径改为复用本地已登录的 Codex 账户；撤销刚开始、尚未提交的登录 / 设备码 / 取消登录协议代码，未形成登录命令或对外功能。开发总入口 §1.1 / §6、账户专题、IPC §2.29、实施计划 M12 / A17 同步更新；这些说明优先于上面历史模块中“登录 / 取消仍待实现”的旧待办。原用户未提交文件保持原状，不修改其完整方案正文。

沿用受控本地服务：用户选择原生程序及已有 Codex Home，服务复用它管理的现有登录状态；TokenPulse 只读 account/read（refreshToken=false）和 rate limits、接收净化字段，不读写 auth.json，不复制、持久化或展示凭据。设置页将 authorization_required 显示为“本地登录态不可用”，引导改选已登录 Home / 重新连接；不再显示“授权流程尚未接入”或新增登录入口。本地消费继续独立工作，API key / 未登录 / 失败 / 旧快照保持原状态边界。

实际本机读取：用户明确要求复用本地账户后，使用当前安装的 codex-cli 0.130.0 原生程序与现有 Windows Codex Home，通过生产 NativeService 指纹 / Windows Job / 协议握手和 AccountQuotaService owner 完成只读账户证明及额度读取。返回 state=ready、error_code=null、非空 fetched_at_ms，以及实际 10080 分钟周窗口与已知剩余 / 重置时间；服务按流程断开并 shutdown。没有发起登录、设备码、登出、模型回合或额度重置，也没有使用当前对话账户工具。验收仅记录字段存在和状态，不在文档保存实际百分比、账户身份或认证。它证明本机版本的一次真实已有登录读取，不代替持续通知 / 身份变化、完整冷启动、Win11 或所有安装版本验收。

增加显式 local-account-check feature 下的本地验收 example，必须传 --read-existing-account 与已选定程序 / Home 才运行；正常生产构建、默认 tests / CI 不运行，也不自动探测真实账户。输出只含净化额度状态和周期，无身份 / 认证 / 原始服务消息。该辅助入口只用于实际验收，正式应用仍通过已实现的原生配置界面独立运行。

验证：新增的本地登录态提示 / 无登录按钮交互断言随 3 项账户设置场景通过；30 项 Vitest、strict TS / 生产构建、local-account-check Clippy warnings denied 与 fmt / 差异空白通过。实际读取 evidence 与前端合成桥分别记录。下一步完善本地服务 / Home 检测和主总览 / 小窗共享额度，再继续任务栏与部署维护；完整功能尚未完成。未运行性能测试。


## M12f / M12g：本地原生检测与主窗口 / 小窗共享额度

M12f 增加 detect_local 选择分支，按本机原生安装与现有 Home 生成草稿，不启动服务、执行 shim 或读取认证。显式 Home 保持，保存和连接继续分开。quota 增至 23 项自动检查，实际安装元数据检测与真实 WebView 草稿验收通过。该模块独立提交 3437bf6，具体证据见[检测验收](local-account-detection.md)。

M12g 移除总览 / 小窗硬编码账户占位，接入权威内存 DTO、实际周期、选定桶、剩余条、绝对重置与倒计时、旧快照、受限刷新及隐私。无本地来源或统计失败时账户显示独立可用。小窗详情支持滚动、键盘关闭与焦点循环；主窗口保持左右布局，固定会话不改变账户值。共 32 项 Vitest / 65 项 Playwright、类型 / 生产构建、桌面 Clippy / fmt 通过；设置格式共享提取后其 4 项交互再次通过。Win10 真实两 WebView 的合成服务展示、桶切换、断开清理与既有原生回归通过，退出 0；未执行性能测试。

[共享额度验证记录](account-quota-verification.md) 区分浏览器合成 DTO、实际 Windows 系统流程和外部条件。一次真实本地账户读取保留 M12e 的独立证据；不将本次合成原生 UI 检查当作真实账户长时刷新、完整冷启动或任务栏交付。M12 仍需实际持续刷新 / 通知 / 身份变化验收，第三入口随后由 M13 完成。


## 已取消数据维护的正式 UI 收敛

按用户已确认的 M14 / 迁移保护取消范围，移除正式设置页“数据与备份”占位及导航中的备份说明，避免界面继续承诺取消的功能。来源 / 账户、窗口、任务栏及价格设置保留；已有内部 SQLite / 迁移 / 检查点代码未改动。类型与生产构建、现有设置及运行壳交互检查通过，不新增数据读写或破坏性操作。


## M13a：独立任务栏宿主共享协议与失效状态

已建立 token-pulse-taskbar 库，未依赖 Tauri、SQLite、日志读取或账户通信库。长度前缀帧限制 64 KiB，发送 / 接收有界，严格消息 / 动作、宿主实例 / nonce / 精确递增序号绑定和首消息握手已实现。展示投影只携带小窗消费与独立账户所需字段，保留精确整数 / 金额、null / 零、实际周期及已计价 / 未计价；隐私投影移除名称、费用和账户数据。接收器在隐私指令、错误、shutdown 或关闭时清除旧缓存并阻止旧会话继续使用。

9 项独立预期协议测试、严格 Clippy / fmt、构建与新 schemas/taskbar-host-v1.json 生成 / 漂移检查通过，CI 已加入宿主契约检查。此模块仅验证协议与数据状态，尚无命名管道 ACL、子进程、原生渲染或 Explorer 嵌入证据；不能将 ready 回复算作任务栏就绪。下一模块完成独立 Windows 宿主、受控管道与真实任务栏探测 / 绘制，随后安全布局、交互与回退。详见[宿主协议](../design/taskbar-host-protocol.md)，未执行性能测试。

## M13b1：独立 Windows 宿主与受控管道

增加 token-pulse-taskbar-host.exe，主端先建立当前用户保护 DACL 管道，再以挂起方式启动本地原生宿主，加入关闭即终止的 Job 后恢复。命名管道拒绝远程 / 第二实例，两端检查内核进程 ID；固定参数、nonce 与精确序号绑定会话。连接 / 交换 5 秒期限，15 秒没有主端消息自动退出；回复种类与隐私值必须匹配，任何错误立即结束自有宿主。协议不变，不读取日志、SQLite 或账户，不使用 Tauri / WebView。正式应用调度、原生窗口、Explorer 嵌入与打包尚未接入，不以 ready 宣称任务栏已显示。

验证：Win10 当前实际环境完成 1 项保护 DACL / 当前用户唯一 ACE 检查、5 项真实宿主进程 / 管道检查，以及既有 9 项协议测试，共 15 项通过。覆盖正常隐私确认 / 心跳 / shutdown、Drop 清理、对象仍存活时错误立即关闭子进程、抢占同名管道、错误双方 PID / nonce、截断帧、真实心跳期限和路径参数限制。严格 Clippy、fmt 和 schema 漂移通过。首轮断连测试阻塞父 I/O reactor，调整为异步等待进程句柄后通过；没有执行性能测试。详见[宿主协议](../design/taskbar-host-protocol.md)。下一模块继续 Win32 控制窗口、实际任务栏探测、原生绘制与安全布局。

## M13b2：原生控制窗口、只读拓扑和空间计划

独立宿主正确握手后创建专属 Win32 UI 线程与隐藏顶层控制窗口，接收任务栏重建 / 显示器 / DPI / 设置 / 主题 / 电源通知。原生缓存更新使用私有有界队列，隐私确认及 shutdown 等待 UI 线程完成；退出释放自有缓存、窗口、注册类和线程。窗口不激活、不显示，也不绘制任务栏读数。

增加只读 WindowsTaskbar 拓扑探测：校验实际 Win10 build 19045、系统 explorer.exe、类名唯一性、PID / 父子关系、有界枚举、物理矩形与通知区不交叠，陌生版本 / 非法形态返回能力错误。空间计划接受测量像素宽度并保留任务按钮区域，尚未修改布局；真实嵌入仍须验证布局所有权、字体测量及按钮区重排，不能凭候选矩形称完成。

验证：新增 2 项实际隐藏 Win32 窗口 / 缓存 / 私有唤醒 / 退出和系统通知路由检查，3 项四档合成 DPI / 负坐标 / 拥挤 / 超限 / 非法结构几何检查，加既有协议 / DACL / 跨进程，共 20 项通过；跨进程再补充真实宿主窗口隐藏、快照进入原生侧后隐私确认和窗口退出，定向通过。严格 Clippy / fmt / schema 漂移通过。系统消息首次 PostMessage 因带指针消息类型被 Windows 拒绝，改为有效参数的有界同步发送后路由通过；合成广播只发送给自有窗口，不更改系统状态。

本机独立只读 probe 识别 Windows 10 19045、144 DPI（150%），主任务栏 2560×60 物理像素，任务按钮区与通知区边界 2116。360 DIP 宿主的纯计划为 540 像素，返回 disjoint 区域；没有调整 Explorer、覆盖系统控件或创建可见读数。该实际探测不代替嵌入 / Explorer 重启 / 四档实际 DPI / 多屏 / Win11 验收。继续原生绘制、安全布局与交互，再接 Tauri 管理器和失败回退；未运行性能测试。

## M13c1：精确原生读数、字体测量与自有画布

生产宿主接入 TaskbarView → 展示字段 → 系统 HFONT 测量 → 自有 Win32 读数子窗口的链路，未链接演示数据。Token 与费用不经浮点；null / 零 / 未计价、独立实际周期、周歧义、旧快照、配置时区及到期待更新分别表达。完整 / 精简 / 单项随真实字体宽度选择，高度不足使用单行，再不足返回无布局；可访问名称保留完整精度并遵循隐私。新增 GDI 绘制与有限位图辅助，资源 / 窗口代际由自有对象管理。

隐私 / shutdown 在原生线程清除布局文字与窗口名称、背景重绘并 GdiFlush 后才确认；错误进入失败流程，不忽略清除失败。当前读数窗口是隐藏控制窗口的子窗口，仍未挂入 Explorer，也没有可见任务栏功能；目前深灰画布的背景采样 / 实时系统主题跟随挂接模块接入，高对比度与明暗 palette 已有实现。

验证：新增 5 项独立文字 / 精确边界 / 周期 / 时区 / 15 种内容选择 / 隐私 / 布局预期与 2 项实际 Windows 系统字体 / GDI 位图检查，任务栏全套 27 项通过；跨进程宿主、已有自有窗口和管道回归通过。自有窗口追加真实测量宽度、完整可访问名称、隐私后名称 / 布局和退出清理。四档请求字体 DPI 的文字范围无裁切，清除后逐像素只有背景；它不是四档物理系统 DPI 验收。Clippy warnings denied、fmt 和 schema 漂移通过。

视觉：显式开发 example 用合成夹具生成 5 张原生 GDI BMP，并人工查看暗色两行、精简、浅色单行、0% 琥珀色和隐私 / 大字号。全部 DEVELOPMENT-FIXTURE 标记，价格 / 消费 / 额度为合成预期，不记为真实账户或可见任务栏截图。默认 Python 无 Pillow，直接检查 BMP，未安装额外依赖。精简测试首轮假定的宽度仍能容纳完整文字，按独立固定字宽算出完整 164 / 精简 126 后将拥挤阈值设为 150，检查通过。未执行性能测试。继续安全布局所有权、实际挂接 / 脱离与交互，然后接主应用管理器及明确回退。

## M13c2：真实任务栏空间预留、挂接与条件恢复

新增 LayoutLease，保留系统 Explorer 进程句柄 / 创建时间，校验 HWND 类名、PID、父子关系及窗口 owner 属性；线程互斥阻止另一个 UI 线程占用同一布局。同步预留真实文字宽度并保留至少 320 DIP 任务按钮空间，复核任务列表已缩小、其他根控件与通知区不交叠，再将自有 WS_CHILD 挂到 Shell_TrayWnd，并在预留区域内置于 ReBar 背景之上。没有桌面覆盖窗、DLL 注入或焦点激活。普通更新沿用槽位，禁用 / 清空 / 失效 / 正常 Drop 先隐藏脱离，仅在归属与当前客户端几何 / 容器尺寸 / DPI 匹配时恢复。外部布局变化不被旧矩形覆盖，失败状态明确保留。

原生控制队列增加明确启用 / 禁用和实际 embedded / failure / restore 结果，默认禁用；跨进程 Win32 调用的状态重入经过 busy 屏障延期。画布更新使用 SWP_NOMOVE / SWP_NOSIZE，避免 SetParent 后移到任务栏原点。任务栏 ReBar 边缘一个像素用于系统背景，配合现有明暗 / 高对比度 palette；最初 GetDC 的可见裁剪导致取色失败，改为 GetDCEx 的显式有界窗口 DC 后通过。本轮未修改用户原有未提交文件或读取认证 / 聊天 / 真额度。

自动检查：新增独立恢复几何（外部尺寸 / DPI / 位置变化）、实际命名互斥排他 / 释放，以及自有 Win32 画布隐私更新保持位置 / 大小与清除的检查；任务栏全套 30 项通过，含既有真实管道 / 子进程 / DACL / 心跳期限。Clippy warnings denied、fmt、schema 漂移和差异空白通过；没有性能测试。默认 tests / CI 不调整 Explorer。

实际系统检查：显式 check_taskbar_layout 开发程序在本机 Win10 19045、144 DPI（实际 150%）预留 601×60 物理像素的单行完整读数。约 5 秒显示后检查实际 embedded / visible、无绘制错误，更新隐私时保留槽位并移除私有数据；显式禁用返回 Restored，任务栏 / ReBar / 任务列表 / 通知区全部恢复初始矩形，前台窗口保持。随后重新挂接，再由正常 Controller Drop 恢复原布局。结果退出 0；未重启 Explorer、改变系统 DPI 或点击其他控件。

视觉检查：独立程序仅截取自身已验证的原生读数窗口像素，输出忽略目录 test-results/taskbar-native-attachment 的 DEVELOPMENT-FIXTURE-visible.bmp / privacy.bmp。人工查看任务栏实际浅灰背景、绿色 USD 估算、实际周期映射和完整周重置文字，以及隐私后的背景清除 / 隐藏文字，无裁切或旧私有字残留。初次开发截图辅助的调用线程未声明物理 DPI，输出宽度被虚拟化而截断；补齐该辅助程序线程 DPI 上下文后重新生成与查看完整 601×60 图片。该问题发生在开发截图辅助，正式 UI 线程本身已有物理 DPI 上下文。全部数值仍为明确合成夹具，不能记作真实账户或正式主端三入口验收。

下一步：补齐强制终止宿主时的跨进程布局记录及父端条件清理，再将配置 / 实际状态接入受控宿主协议和 Tauri 管理器。当前生产 wire 未启用 Explorer 挂接；正常析构不能证明 Job 强制结束、宿主崩溃、Explorer 重启的清理已完成。原生单击 / 双击 / 右键 / 悬停、系统按钮实际点击、自动隐藏、主端回退、Win11 与完整物理多屏 / DPI 矩阵继续保留待验收，不恢复已取消的数据维护或迁移保护。

## M13c3a：已结束宿主的归属证明与父端恢复

调整前先发布有界窗口记录：17 个 u32 字段（原 / 缩小客户端矩形、容器尺寸 / DPI、宿主和 Explorer 的 PID / 创建时间）、完整 128 位实例头与阶段。每个字段分成两个 16 位半字加一，已知零不与缺失混淆，不保存跨进程指针、账户数据、路径或聊天。完整记录先写、owner 最后发布，同步调整返回后确认 Reserved；每次归属验证同时核对完整实例头及全部记录。未知字段阶段、缺失半字或非法几何拒绝恢复，移除元数据的返回值也经过核对。

父端恢复 API 接受它所持的 Child 内核句柄及启动实例，先确认该句柄对应进程已结束，再匹配 PID / 创建时间，避免根据复用的 PID 猜测。当前 Explorer 进程 / 窗口代际、记录与缩小几何仍一致才恢复；外部布局变化继续保留。恢复后删除本实例记录，重复返回 NoRecord；仍在 Prepared 且几何为原值时返回 Uncertain 并保留归属。HostProcess.stop / Drop、HostConnection 错误关闭 / shutdown / Drop 接入，连接存活时可检查清理结果，不将失败称为 Restored。生产侧宿主控制窗口使用已经握手验证的 Startup.instance，协议版本 / schema 本轮未改变。

自动检查：新增 2 项实际自有窗口元数据 / 合成非法几何与 1 项真实存活进程句柄拒绝检查，覆盖完整实例、同短标记不同命名空间、未知阶段、缺失 / 非法范围、零与 u32 上限、其他所有者不可删除及正确清理；任务栏全套 33 项通过。真实管道回归补充正常 shutdown 与错误关闭后清理结果仍可读的断言。Clippy warnings denied、fmt、schema 漂移与差异空白通过。默认 tests / CI 不调整 Explorer；没有运行性能测试。

实际系统检查：显式 check_taskbar_exit 在 Win10 19045、144 DPI（实际 150%）启动本程序的开发子进程，先加入自有 Job，再允许其挂接合成读数。父端拒绝存活进程的恢复请求；关闭 Job 强制结束子进程，未发送正常 shutdown / 禁用，原归属及缩小区域仍存在。错误完整实例（即使短标记相同）和另一已结束进程句柄均被拒绝；正确句柄恢复返回 Restored，任务栏全部矩形与此前一致，第二次返回 NoRecord。新实例可再次挂接且正常 Drop 恢复。最终退出 0；没有终止其他进程、读取账户 / 日志或重启 Explorer。首轮验收错误地要求 Job 终止退出码非零，实际可为 0；改用内核进程结束及仍存留的布局证明后通过。M13c2 的正常显示 / 隐私 / 禁用 / 重挂接 / Drop 也在此次代码上回归通过。

边界与下一步：此实际检查使用共享 NativeController / 恢复实现的独立开发程序，尚非正式管道启用验收；正式 wire 默认仍不挂接。父端保持运行，不证明主进程本身被强制结束后的清理监督已完成；同步原生恢复还需要隔离 / 有界执行，Prepared 歧义需接状态 / 重试。随后接正式配置 / 实际状态与 Tauri 管理器、原生动作 / 悬停 / 菜单、失败回退及 Explorer 生命周期。Win11、自动隐藏、系统按钮实际点击与完整物理多屏 / DPI 矩阵仍在保留范围，不恢复 M14 / 迁移保护等取消项目。

## M13c3b：父进程异常退出监督与隔离清理期限

生产 HostProcess 启动改为异步：宿主挂起、加入自有 Job 后，启动同一原生程序的受限清理模式并确认 armed，再恢复宿主线程。清理进程不属于宿主 Job，通过独立当前用户管道及内核 PID / nonce 绑定，持有父端与宿主内核句柄并核对宿主创建时间和同一程序路径。已 armed 的清理进程在父端退出后继续监督；启动拒绝时只结束本次创建的清理进程，宿主尚未执行。

正常关闭及错误关闭先结束自有宿主，再收集独立清理结果，父端不做 Explorer 窗口恢复调用。一次性清理进程用独立线程限定恢复执行为 5 秒，期限到达仅终止自身；固定结果码区分真实恢复 / 外部变化 / 身份丢失 / 歧义 / 错误，普通 0 / 1 或未知退出码拒绝作为成功。父端等待宿主与清理结果也有上限，后续管理器在后台调用，避免占用主 UI。

自动检查：全套 35 项通过，另一个 ignored 测试是明确由测试创建的私有子进程 gate。新增启动参数与结果码预期、不同原生程序不能 armed 且不终止其目标的实际检查；现有跨进程关闭验证独立清理 PID 及两进程最终退出。严格 Clippy、fmt、schema 漂移检查通过，默认 tests 不调整 Explorer。

实际系统检查：显式 check_taskbar_guardian 在本机 Win10 19045 / 150% DPI 创建开发父进程、自有挂接子进程及独立清理进程。挂接完成后强制结束自有父进程，其 Job 关闭而结束宿主；未调用正常 Drop，独立清理返回 Restored，原任务栏全部矩形恢复一致。另一个一次性进程模拟阻塞恢复操作，返回 CleanupTimeout，验证 5 秒功能期限。未读取真实账户 / 日志、不重启或故意阻塞 Explorer、不运行性能测试。

边界：该开发程序共用生产监督实现，但生产 wire 尚不启用挂接，M13 整体未完成；清理进程被外部强杀或整棵进程树被终止不在成功证据内。正式配置 / 实际状态是下一模块，随后继续原生交互 / 悬停 / 菜单、Tauri 管理器、失败回退与 Explorer 生命周期。Prepared 歧义继续真实表达，Win11 / 自动隐藏 / 系统按钮点击 / 物理多屏 DPI 保留待验收。

## M13d1：正式宿主启用配置与实际状态

正式管道增加修订化配置 / 状态查询，显示偏好实际接入 UI 线程及原生字体布局；配置确认与 embedded 分开。初始未配置保持禁用 / null 修订；启用无快照为 waiting_snapshot，实际窗口可见且布局有效才报告 embedded 及密度，能力失败为 unavailable / 具体原因。新设置清除旧快照，清屏 / 脱离完成后才确认；拒绝空偏好、倒退或同修订冲突、非法回复状态。隐私修订独立跟踪，支持同设置修订的配置 / 隐私更新按任一顺序到达，仍拒绝已确认隐私冲突。

自动检查：37 项通过，另一个 ignored 项是私有子进程 gate；严格 Clippy、fmt 和更新后的 schema 漂移通过。新增配置 / 状态独立预期及真实宿主默认 / 禁用状态检查。初轮真实管道发现配置先到导致同修订隐私变更被错误拒绝，独立记录隐私修订后回归通过。

实际系统检查：check_taskbar_wire 使用正式 HostConnection / 宿主程序 / 独立监督，合成消费 / 价格 / 额度夹具明确开发专用。Win10 19045 / 150% DPI 完成等待快照、实际 embedded、新配置先脱离旧快照、单行仅 Token 的更窄自有读数、隐私清屏 / 恢复、重新显示、禁用恢复与正常关闭；原任务栏全部几何一致，前台焦点保持，清理为 NoRecord（正常禁用已移除本实例记录）。首轮开发检查错误地把只读完整拓扑探测用于已预留的任务区，得到预期 UnsafeGeometry；改为验证本次宿主 PID / 自有读数类后只读取自身窗口宽度，重新验收通过。

下一步：生产 wire 已允许显式启用，普通 Tauri 应用尚未调用，仍默认禁用。继续主端管理器、实际设置 / DTO / 状态、原生动作 / 悬停 / 菜单、失败回退及 Explorer 生命周期。此次未运行性能测试，不恢复 M14 / 迁移保护等取消项目，未验收兼容矩阵保持原范围。

## M13d2：持久任务栏偏好与一致输入快照

已增加 core 权威任务栏偏好及修改 / 快照 DTO，默认关闭、两行、四项内容和失败回退，位置显式表达。宿主显示类型复用 core，应用 TypeScript 与两个 schema 已重新生成。配置在既有设置 payload / 全局修订下原子保存，精确 CAS、无变化不递增，保留主题 / 时区 / 隐私 / 小窗配置；旧 enabled 键仅作兼容读取，新修改归一到 taskbar。非法配置及失败写入不发布默认值或部分意图。

新增 taskbar_input 在同一 SQLite 读事务取得配置、共享小窗范围的完整用量及隐私；费用 / Token / 设置修订一致，账户仍独立读取。开启而未初始化时区返回错误，关闭不查询用量。位置偏好不等同于实际能力，当前只验证托盘左侧，其他位置随后接能力拒绝及适配。该模块尚未接 Tauri 命令或启用 UI，下一步继续后台管理器及正式配置入口。

验证：新增 3 项存储检查覆盖默认 / 幂等 / CAS / 重开、同快照设置与用量、Token / 价格修订不变、旧键及非法配置、写入失败原子保留；25 项设置回归通过，16 项宿主格式 / wire 回归通过，core / store / taskbar 严格 Clippy、fmt 与生成契约检查通过。未运行性能测试，也未将这些存储检查记录为原生系统验收。

## M13d3：Tauri 后台宿主所有者、实际状态与隐私屏障

已在正式运行壳启动专属 taskbar worker，新增主窗口专用偏好读取 / 保存、实际状态读取与手动重试命令及 AppManifest / capability。启用后从同一 SQLite 快照读取小窗共享范围的真实用量 / 费用，独立读取账户快照；缺失账户保留 null，不影响本地消费。后台拥有进程 / 管道、每秒发布新快照、慢查询期间独立 5 秒心跳 / 状态查询，最多 5 次失败退避后等手动重试或配置 / 系统代际变化。默认关闭不启动宿主，应用图标右侧位置明确不可用。

隐私和任务栏偏好提交前，先暂停发布并等原生清屏 / 脱离 ACK；不在持有 PrivacyState 锁时等待线程。读取代际隔离丢弃暂停前结果，失败不提交隐私成功。主窗口隐藏继续刷新；合成电源路由接暂停 / 恢复；明确退出由后台等待线程清理后最终退出，主 UI 保持分发。状态含精确单调修订、nullable 应用设置修订、最后成功快照时间和真实清理结果。失败回退尚未接入，fallback_visible 保持 null；不据此宣称 M13 整体完成。

自动验证：3 项后台功能检查通过，覆盖阻塞旧输入不阻塞隐私屏障 / 迟到结果不发布、休眠恢复 / 幂等退出、5 次启动失败停止及显式重试重新开放。重试期限检查约 20 秒，是功能检查，未运行性能测试。11 项宿主 wire 回归通过，补充账户服务缺失时仍保留用量 / 费用投影；严格 Clippy、fmt、两个 schema 漂移及前端生产构建通过。

实际系统验证：显式 scripts/native-smoke.ps1 -Taskbar 在 Win10 19045 / 150% DPI 使用隔离无来源数据库及真实两个 WebView，经正式主窗口命令启用独立宿主，实际 embedded；mini 无管理命令权限，共享隐私切换后继续显示，主窗口隐藏后最后成功快照时间推进；合成电源消息暂停并恢复，禁用与嵌入时关闭后台服务都恢复原任务栏全部几何。输出 NATIVE_TASKBAR_MANAGER_OK，退出 0。无真实日志 / 账户读取，不重启 Explorer；电源消息不等同于机器实际休眠。WebView2 退出时输出 Chrome_WidgetWin_0 注销错误 1412，场景断言及进程退出通过，保留诊断记录。

综合原生回归单独记录：先发现已取消 M14 后旧测试仍要求 5 个设置页签，修正为 4，且脚本异常现在明确失败；重新运行已通过显示与小窗场景，但停在真实键盘恢复检查，任务栏场景未到达。因此任务栏使用独立显式场景验收，不把它当作综合回归通过；键盘恢复失败待后续复核。

下一步：任务栏正式设置 UI、原生鼠标 / 悬停 / 菜单、失败回退和 Explorer 生命周期继续实现；简单安装包须包含同目录宿主，Win11 / 系统按钮 / 自动隐藏 / 物理多屏各档 DPI 仍待验收。M14 / 迁移保护取消范围保持不变。

## M13d4：正式任务栏设置、状态诊断与 UI 验证

任务栏设置页已替换占位内容，接入真实偏好 / 运行状态命令，支持启用、四项内容选择、单行 / 两行、通知区左侧位置、精确修订保存和手动重试。偏好与运行结果分别显示：保存不等于嵌入成功，未确认密度 / 回退 / 清理 / 成功时间保持未知。应用图标右侧选择暂禁用，自动回退尚未接入，仅如实显示已保存偏好及手动小窗入口。小窗共享范围与独立账户范围说明保留，主窗口导航 / 顶部布局没有重排。

采集诊断使用相同实际状态展示，去除任务栏“正在实施”固定文本。任务栏配置 / 状态不包含消费数字、费用、路径或账户标识，前端按现有 plain 响应规则读取，但仍校验协议版本 / 请求身份。事件只使状态失效，重新调用命令取得完整快照；查询序号及大整数修订阻止旧响应倒退，卸载停止订阅和定时器。偏好草稿保留最初 CAS 修订，外部修改、刷新及写入失败不覆盖草稿；显示内容全不选时禁止保存。

自动验证：4 项新增浏览器交互检查通过，覆盖最大安全整数之外的精确 CAS、保存与嵌入分离、未知状态、冲突 / 写失败 / 读失败保留草稿、全空拒绝、实际错误 / 手动重试、事件重新查询、较旧修订拒绝和页面卸载。另有主壳与基本重建页面各 1 项回归通过；隐私 runtime 共 5 项通过，其中新增无敏感信息的任务栏响应及错误身份检查。首次页面卸载检查发现 StrictMode 重挂载时旧异步订阅泄漏，已改为每次 effect 的存活标记，重新检查通过。TypeScript、生产构建、Rust fmt / 严格 Clippy 通过。

视觉检查：查看合成 DTO 的深色 1280×960 与浅色 960×900 全页截图，深色保持原布局 / 圆角 / 蓝色强调，窄窗设置及状态分区顺序排列，无横向溢出。截图只来自浏览器开发夹具，生产不加载演示数据；浏览器检查不作为实际嵌入证据。

实际系统验证：在本机 Win10 19045 / 150% DPI 再运行显式 -Taskbar 场景。真实 WebView 设置页显示已保存启用和实际 embedded 状态，页面按钮保存关闭后，SQLite 读回 disabled 且原任务栏全部几何恢复；其余隐私屏障、隐藏主窗刷新、合成电源路由及嵌入时 shutdown 仍通过。输出 NATIVE_TASKBAR_MANAGER_OK，退出 0。此处点击是 WebView DOM 正常事件，原生任务栏鼠标 / 菜单和系统控件实际点击尚未验收；WebView2 退出错误 1412 仍保留，综合原生键盘恢复失败待复核。未运行性能测试。

下一步继续任务栏原生鼠标 / 悬停 / 菜单、自动回退、Explorer 生命周期与版本 / 物理多屏 DPI；自动回退及应用图标右侧尚不可用的 UI 状态不代表这些保留功能已取消。安装打包须包含原生宿主。M14 与迁移保护取消范围保持不变。

## M13e1：按偏好的非激活小窗回退与真实反馈

后台在有效配置启用 taskbar / fallback_to_mini 且原生状态不可用或错误恢复时，异步请求现有小窗。一段失败期间只尝试一次；成功后观察实际可见性，用户隐藏不会反复弹出。重新成功嵌入、显式重试 / 设置生命周期变更可重新开放下一次请求。关闭任务栏 / 回退以及恢复嵌入均不自动关闭已有小窗；已有可见小窗也不改位置 / 穿透 / 焦点。主窗口 UI 已提供正式回退开关及独立结果，账户与共享范围不变。

最多一个回退工作并发，不阻塞采集、原生心跳或隐私清屏 ACK。请求绑定主端代际、停止 / 暂停 / 休眠标记及实际失败状态，创建前、显示前和主线程执行时检查；旧结果不发布。主线程显示等待有 5 秒功能期限，超时取消迟到显示、上报错误并停止同段自动重试。fallback_visible 按实际小窗查询填入；失败保留 null，新增独立 fallback_error，原生不可用原因继续保留，不因回退成功变成 embedded。

小窗自动创建使用非聚焦配置，显示通过主线程的 Tauri 可见性通路；临时 Win32 子类在该操作期间保留 NOACTIVATE，再移除子类并只恢复原 NOACTIVATE 位，其他样式保留。窗口仍可正常点击、隐藏和显式恢复。初轮实际检查使用直接 ShowWindow 时发现 tao 可见性缓存未更新，正常 hide 无法隐藏；检查真实依赖源码后改为保留非激活样式的框架 show，实际复测通过，不把初轮失败计作成功。

自动验证：后台共 8 项通过，新增 5 项覆盖一次尝试 / 用户隐藏 / 恢复后新失败、代际 / 暂停 / 休眠 / 退出 / 取消门禁、阻塞回退不阻塞隐私屏障且禁用后旧结果丢弃、实际隐藏观察、超时取消仍表达未知 / 错误且不循环。前端任务栏 5 项及基本重建页面 1 项通过，新增回退偏好保存、回退失败未知与原生失败独立展示；合成状态截图已查看，无裁切。TypeScript、生产构建、契约漂移、Rust fmt 与严格 Clippy 通过。功能期限检查不属于性能测试，未运行性能测试。

实际系统验证：本机 Win10 19045 / 150% DPI 的显式 -Taskbar 场景通过。隔离库无来源 / 无账户，选择已明确不可用的 application_right 触发真实主端回退，不改系统设置或 Explorer。先销毁本测试自有 mini 覆盖从无窗口创建，再验证实际显示、前台 HWND 不变及 NOACTIVATE 位已复原；通过 mini 正式命令隐藏后不反复显示，手动重试重新显示并覆盖隐藏窗口复用。真实设置页关闭回退仍保留小窗，恢复 notification_left 实际嵌入后也保留小窗，最终嵌入时 shutdown 原任务栏几何恢复。输出 NATIVE_TASKBAR_MANAGER_OK / 退出 0；WebView2 退出 1412 诊断仍保留。

下一步继续原生单击 / 双击、悬停 / 菜单及 Explorer 生命周期。上述失败触发验证不证明 Win11、实际拥挤 / 自动隐藏、宿主真实异常期间回退或物理多屏各档 DPI 已通过；相应系统验收仍保留，综合原生键盘恢复失败另行复核。M14 / 迁移保护取消范围保持不变。

## M13e2a：原生单击 / 双击意图与有界拉取

自有读数类启用系统双击消息；单击等待 GetDoubleClickTime 的真实系统期限，双击取消待发单击并只产生 OpenStats。消费按下事件、使用 NOACTIVATE / NOPARENTNOTIFY 子窗口，避免默认处理向 Explorer 转发鼠标按下通知。最多 4 个动作、5 秒单调时钟寿命；普通同修订快照刷新保留待发动作，配置 / 隐私屏障、脱离、销毁或绘制失败清除待发与队列。

正式宿主管道增加 get_actions / actions，在私有 UI 队列内确认当前仍嵌入后才取走意图。回复携带精确配置修订，不含路径、账户或任意命令；严格校验类型、最大数量、非空队列必须有修订及身份 / 序号。继续一请求一回复，不发送未经请求的 action 帧，也不把协议心跳当原生鼠标确认。主程序执行窗口动作尚待 M13e2b 接入。

自动验证：taskbar lib 12 项、wire 12 项及真实独立宿主 native_transport 5 项通过。新增纯队列独立预期覆盖系统期限、双击无延迟小窗、容量 / 过期 / 清除 / 时间饱和；wire 覆盖严格空参数、受限动作及修订 / 数量约束；真实受控管道验证初始 / 已禁用宿主只返回空动作。schema 已同步，Rust fmt / 严格 Clippy 通过。未运行性能测试。

实际系统验证未完整通过：显式 check_taskbar_wire 在 Win10 19045 / 150% DPI 用 SendInput 前核对目标属于本次 PID 的原生读数；目标被 Windows 默认锁屏界面 CoreWindow 覆盖时拒绝发送点击。可命中读数的尝试已通过单击 / 双击动作和配置 / 隐私清除断言，但最后前台 HWND 变为 null，整场不计为通过，不能据此宣称焦点保持；后续环境可交互时重新验证。停止自有宿主后仍由既有独立清理监督处理布局。真实鼠标全场、主端窗口跳转、悬停 / 菜单、Explorer 重建、Win11 与物理多屏 DPI 继续保留。

## M13e2b：正式主端窗口执行与同范围统计

后台在已有串行受控管道每 150 毫秒拉取原生动作，只接纳与已应用配置相同的精确修订。主端待发最多 4 项、最多一个窗口工作，意图有 5 秒功能期限；每次执行重新检查持久任务栏启用 / 修订，隐私暂停、休眠、退出或代际变化取消待发和正在执行意图。原生恢复 / 不可用清除旧意图，旧取消结果不发布；真实超时独立表达失败且同一超时只首次发布，不把它当嵌入失败。窗口工作不等待在宿主管理循环内，心跳及清屏确认继续工作。

OpenFloat 使用既有 mini，恢复交互后在主线程门禁内展开为 360×380 DIP、持久保存、适配工作区并显示。创建阶段保持隐藏 / 非聚焦，最后显示再次检查意图；不在持有 interaction 锁时等待主线程。既有展开命令复用同一保存 / 回滚 / 工作区处理。OpenStats 读取正式原子 mini_usage 并按意图修订校验，保留与小窗相同的 UTC 毫秒范围 / 时区 / 会话，沿用既有主窗口统计意图和失效事件；账户范围不随之变化。菜单动作还没有生产者，保持受限明确拒绝，随后实施。

新增 TaskbarRuntimeSnapshot.action_error 为独立 nullable ErrorCode，与嵌入 / 回退错误分开；下一次成功窗口操作清除，不因动作失败将 embedded 改成 unavailable。小窗订阅既有 mini_interaction_changed 及设置事件后重新调用正式 read，不相信事件载荷；独立查询序号和生命周期拒绝迟到 / 卸载后的交互结果，解决已存在 WebView 外部展开后还显示紧凑布局的问题。Rust / TypeScript / protocol-v1 schema 同步。

自动验证：后台 11 项通过，新增 3 项覆盖精确修订 / 代际 / 容量 / 过期、单个阻塞工作与清除后不显示 / 不发布旧错误、超时只首次表达及新状态不被旧结果替换。超时取消区分后，受影响 2 项重新通过。Playwright 小窗 12 项、任务栏 6 项、基本重建 1 项，共 19 项通过；新增外部展开重新读状态 / 旧响应丢弃、动作错误与嵌入状态独立。原订阅列表断言更新以覆盖新增订阅，清理无泄漏。查看 360×380 合成 DTO 展开截图，无裁切，原布局 / 深灰与绿色费用保持。TypeScript、生产构建、契约漂移、fmt / 严格 Clippy 通过。未运行性能测试。

实际系统验证：显式 scripts/native-smoke.ps1 -TaskbarActions 使用本机 Win10 19045 / 150% DPI、隔离无来源 / 无账户 SQLite、正式主端和独立宿主及真实两个 WebView。向本次后台拥有 PID 的原生读数发送明确标记的自有窗口鼠标消息，再走正式宿主管道 / 主端工作；确认单击打开实际展开 360×380 小窗 / 持久状态 / WebView 展开，双击仅显示主统计 / 相同起点和已保存时区 / 小窗仍隐藏，最后嵌入时 shutdown 恢复原全部几何。输出 NATIVE_TASKBAR_ACTIONS_OK，退出 0；WebView2 注销 1412 诊断仍保留。首轮验收错误硬编码 UTC，与实际保存时区不符，改为对照正式 usage 范围后复测通过。

该系统场景证明真实主端通路与原生窗口，不等同 SendInput 实际鼠标 / 焦点保持验收；M13e2a 锁屏覆盖及前台 HWND 变 null 的失败仍待交互桌面复核。原生悬停详情 / 右键菜单、Explorer 重建、应用图标右侧位置、Win11 / 物理多屏 DPI、notify 和安装更新继续推进；M14 与迁移保护取消范围保持不变。
## M13e3：原生菜单动作及保留导航

接通真实 Windows 右键菜单的五项受限动作，勾选隐私来自应用快照；设置直接打开任务栏页签，隐藏持久关闭任务栏，隐私经既有清屏屏障和共享 SQLite / PrivacyState 协调器提交。两类写入检查暂停归属 / 代际及精确修订，自己的清屏不能吞掉实际写入失败。菜单使用固定 ID 白名单，模态循环不持可变画布借用，Rc 保留资源到最后一层窗口过程返回；配置 / 隐私 / 销毁结束菜单并拒绝旧选择。

统计与设置导航现在共用单一 DecimalInt 递增修订、main-only get_main_navigation 和 main_navigation_changed 失效通知。前端重新读权威意图，拒绝迟到 / 低修订响应；相同意图不会在重新显示时覆盖手动导航。统计仍用实际 mini 原子范围 / 时区 / 会话，账户范围独立。旧 get_mini_stats_request 保留读取兼容，旧统计事件不再发出。

自动验证：core 8、taskbar lib 13、后台服务 12、总览 / 任务栏 Playwright 16 项通过；包括五项菜单映射 / 原生结构 / 隐私勾选、超安全整数及溢出、自己的屏障后数据库失败仍表达、迟到统计与设置排序 / 可见性 / 低修订处理。strict TS、生产构建、契约漂移、fmt / Clippy warnings denied 通过，未运行性能测试。

实际系统验证：本机 Win10 19045 / 150% DPI，隔离无来源 / 无账户 SQLite、正式应用与独立宿主，-TaskbarActions 验证真实 Windows 弹出菜单 / 自有 PID 与菜单归属 / 文字，通过明确标记的自有 WM_CHAR 助记消息选择五项。真实 WebView / 原生窗口确认小窗、同范围统计、任务栏页签、共享隐私来回切换和持久禁用；再次启用后，菜单打开期间主窗口隐私变更结束菜单且不产生导航，再次打开菜单后 shutdown 恢复全部原几何。NATIVE_TASKBAR_MENU_OK / NATIVE_TASKBAR_ACTIONS_OK、退出 0；WebView2 退出 1412 诊断保留。

原先给标准菜单发 Home / Down / Enter 没有改变选中项；调整测试输入为原生菜单 WM_CHAR，通路通过。没有把自有消息当实际鼠标、方向键、焦点保持或屏幕阅读器验收；真实 SendInput 锁屏 / 前台异常仍单独待复核。悬停、Explorer 重建、应用图标右侧、Win11 / 物理多屏 DPI 等剩余范围继续实施，未宣称 M13 或完整功能已完成。

## M13e4a：悬停详情数据与格式化

正式主端 / 宿主快照补齐 nullable HostDetails：真实范围、范围类别、原子读取的应用主题、来源最近成功核对、受限状态、待确认 / 待核对数量、各分项完整性和计价进行状态。来源时间来自用量快照中的已知成功扫描时间，并非用量查询生成时间，也不声称所有来源均成功。HostQuota 独立保留成功 / 尝试时间及可识别 ErrorCode；原始来源 ID、路径和诊断原文不发给宿主。生产投影提供详情，旧内部开发夹具缺失时表达未提供，不伪造时间。

平台无关 DetailContent 共用显示文本 / 可访问全文，保留完整整数和定点金额、部分分项含义、时区 / 精确半开范围、价格覆盖 / 未计价、实际账户周期 / 桶 / 剩余 / 重置与失败。整数模加百分比避免 i128 溢出或浮点丢失；零用量覆盖无分母，未知用量保持未知。未知剩余条为 None，真实 0% 为 Some(0)。过期重置保留原百分比，多个周窗口逐项表达。隐私移除费用 / 原会话名 / 桶 / 账户窗口，语义与可访问全文一致。

自动验证：任务栏全套首轮 48 项通过；新增部分分项检查后，详情 8 项 / wire 12 项复测通过。设置 4 项含新增主题 / 隐私 / 用量同一修订检查通过；strict Clippy / fmt、桌面编译、宿主 schema 同步。自动用合成夹具，不读取开发日志 / 账户，不运行性能测试。既有 -TaskbarActions 在 Win10 19045 / 150% DPI 重新构建双进程后作通路回归，输出 NATIVE_TASKBAR_MENU_OK / NATIVE_TASKBAR_ACTIONS_OK，退出 0；WebView2 注销 1412 诊断保留。这是实际双进程 / 菜单 / 窗口和退出恢复回归，与悬停面板验收分开。

这是原生悬停面板的投影与格式化依赖，当前仍未创建可见面板；340 DIP 绘制 / 非激活 / 工作区 / 300 ms / 关闭与像素验收继续实施，不能据本步骤宣称悬停功能已完成。

## M13e4b：原生详情窗

正式独立宿主新增约 340 DIP、最多 600 DIP 高的只读原生详情窗，按当前工作区夹紧并拒绝无法安全显示的几何 / DPI。系统字体测量后保留所有字符，完整整数、金额、日期 / 时区、质量与实际账户周期均可换行和滚动；真实 0% 画空的真实额度条，未知条不伪造。详情采用应用主题和系统高对比度，常驻读数背景保持任务栏风格，主窗口布局未改。

TrackMouseEvent 使用 300 ms 悬停；键盘入口焦点可展示详情，方向键 / 翻页 / 首尾键浏览，Escape 关闭；鼠标可进入面板滚动，离开入口与面板后关闭。点击 / 菜单、失焦、来源隐藏 / 移动、清屏 / 禁用 / 销毁关闭详情。普通快照保持已打开状态并更新内容，不打开已隐藏面板。旧排队 hover 在配置清屏后被拒绝，新移动可以重新注册；原生非激活样式与 SetWindowPos.NOACTIVATE 用于显示。

隐私清屏先停止计时器 / 隐藏，丢弃 Rc 文本布局帧与旧窗口名称，覆盖实际客户区像素，再允许确认；旧绘制帧仍在使用时清屏失败而不能提前 ACK。无效新数据关闭并清掉旧详情。无数据库、原始日志或认证信息读取，无新增 IPC 或前端契约。

自动：任务栏 all-targets 54 项通过、1 项私有入口 ignored；新增原生详情 / 入口 5 项含 100% / 125% / 150% / 200% 字体、负坐标工作区、长文本不丢字、精确数值、未知 / 0、主题 / 隐私绘制、实际 HWND 样式 / 滚动 / 清屏全背景 / 归属销毁、旧 hover 拒绝和再进入。最终边界收尾后 lib 18 项和 taskbar / desktop all-targets Clippy warnings denied 复测通过。查看合成 GDI 深 / 浅 / 隐私 BMP，未运行性能测试。

实际：Win10 19045 / 150% DPI，隔离空来源 / 无账户 SQLite、正式 Tauri 和独立宿主；扩展 -TaskbarActions 实际显示 / 检查详情窗、监视器工作区 / 非激活样式。自有窗口焦点 / 翻页 / Escape 消息走真实 HWND，两次正式隐私切换隐藏旧面板，重新输入后内容与已提交共享策略一致。菜单五动作、小窗、同范围统计、持久禁用与 shutdown 原几何恢复回归通过；最后边界收尾后重跑通过，输出三项 NATIVE_TASKBAR_*_OK，退出 0。WebView2 注销错误 1412 保留。

自有消息和 GDI 位图不等同真实物理鼠标 / 300 ms 时序 / 键盘可达性 / 焦点保持 / 屏幕阅读器。前期锁屏 / 前台异常和综合恢复键失败仍待交互桌面验收，Explorer 生命周期、应用图标右侧位置、实际自动隐藏、Win11 / 多屏 DPI，以及价格、notify、安装更新等保留范围继续推进。

## M13e5a：原生画布丢失恢复

控制器现在能识别本代次已接收 WM_NCDESTROY 的画布，在状态 / 动作查询或重新刷新前有条件释放旧布局、丢弃旧画布 / 详情 / 点击 / 菜单，创建新类名的隐藏窗口。重新探测当前真实任务栏并测量 / 校验后，才挂接既有正式快照；新代次增加 system_revision，新建 / 绘制失败仍走真实错误及有限宿主重试。失去旧 Shell 身份时不写入旧或复用窗口、不带旧几何进入新布局。详情不自动重开，旧意图不进入新代次。

定向 controller 3 项及任务栏 all-targets 全套 55 项通过，1 项内部私有入口 ignored。新增自有隐藏窗口两次销毁，分别由 Inspect / TakeActions 恢复，验证新类、注销旧类、修订增加、最小显示缓存保留、旧动作丢弃 / 清屏；普通测试没有启用嵌入，前后真实任务栏拓扑一致。taskbar / desktop all-targets strict Clippy、fmt 与文档链接 / 差异检查通过。

实际 Win10 19045 / 150% 的 -TaskbarActions 在详情 / 待确认单击和标准菜单打开两种情况下关闭本次宿主 PID / 类的实际嵌入子窗口，再走正式管道。宿主不重启，新读数 / 详情类重新挂接，旧单击 / 导航不漏出，新详情可以打开；既有详情 / 隐私、五项菜单、小窗 / 同范围统计及最后退出原几何恢复继续通过，输出 NATIVE_TASKBAR_RECREATE_OK 及另外三项 OK、退出 0；WebView2 1412 保留。

实际 Explorer 退出 / 重启、PID / HWND 复用、系统控件和物理输入 / DPI / 多屏及 Win11 仍单独待验收，不以关闭自有子窗口代替。没有重启 Shell、读取源日志 / 账户或执行性能测试。

## M13e6：应用图标右侧位置与实际按钮变化

正式设置开放 application_right，configure 携带位置。独立宿主在核对 Explorer 映像、内核 PID / 创建时间、窗口祖先和完整几何的主任务列表上，只读 UI Automation 直接 raw-view 子元素的矩形、进程 ID 和可见性；不读取名称、应用内容或调用控件动作。专用无窗口 MTA 使用属性缓存，300 ms 连接 / 事务期限、1200 ms 请求期限、单项队列和最多 256 项；超时使探测器失效，原线程实际结束前不能继续积压工作或创建替代线程。

槽起点为实际最右按钮 + 8 DIP 与至少 320 DIP 应用区域的较大者；空列表保留最小区域。只预留原任务区域内安全槽，预留后重新读取按钮并验证，未知 / 越界 / 隐藏 / 结构改变拒绝显示。每次正式快照重新核对按钮；变化时先有条件恢复旧租约、清除旧手势 / 菜单 / 详情，再从完整当前区域测量，按实际空间精简或明确回退。新修订位置变化先清屏，同修订冲突拒绝；通知区 / 时钟 / 原生控件不能被覆盖。

自动：任务栏 all-targets 59 项通过、1 项私有 gate ignored；新增按钮边界、四档 DPI / 负坐标 / 空间不足 / 溢出、协议默认和冲突、模拟阻塞探测器不积压检查。后台服务 12 项、Playwright 任务栏 6 项、宿主 schema --check、typecheck / Vite 生产构建、strict Clippy / fmt 和任务栏 release check 通过。查看深色 1280 / 浅色 960 合成 DTO 截图，主布局未改；不运行性能测试。

实际：Win10 19045 / 150% DPI，-TaskbarActions 从真实 WebView 设置页选择 / 保存新位置，测量真实按钮边界和读数矩形。创建本测试自有、明确合成、独立 AppUserModelID 的窗口，使 Explorer 实际增加按钮，读数安全右移；关闭窗口后回到原位置，随后两位置往返切换。后续全部动作在 application_right 上检查单击小窗、双击统计、五菜单动作、隐私与退出原几何恢复，输出 APPLICATION_POSITION / DETAILS / RECREATE / MENU / ACTIONS 五项 NATIVE_TASKBAR_*_OK，退出 0。最初在主窗口隐藏后立即投递点击遇到任务按钮重排，旧手势按策略被取消；测试改为等待实际矩形和发布稳定后再投递，通过。

原 -Taskbar 回退夹具不再使用新已支持位置。仅 debug、显式 --native-smoke、隔离 native-probe 目录可替换宿主工厂为不存在的固定本地路径，不改系统形态 / 安装文件 / 用户配置；走正式启动失败及回退流程，检查重建真实小窗、前台保持、用户隐藏不重开、显式重试、关闭回退保留小窗，再恢复正式宿主及嵌入。输出 NATIVE_TASKBAR_MANAGER_OK、退出 0。两场 WebView2 注销 1412 诊断保留。

真实鼠标 / 键盘 / 焦点 / 屏幕阅读器、实际拥挤 / 自动隐藏、完整 Explorer 生命周期、Win11 和物理多屏 / DPI 继续验收。实际按钮变化不把自有输入消息变成物理点击证据；合成 DPI 检查不代表系统 DPI 切换。未读源日志或账户认证信息，整体目标仍在实施。

## M09f1：版本化模型别名写入与正式 IPC

新增 ModelAliasDraft / ModelAliasMutation（create / replace / retire）与主窗口专用 mutate_model_alias；共用精确 price_revision CAS、单写线程和事务内返回快照。新增行分配 alias-custom- 摘要 ID，替换 / 退休仅标旧行 retired_revision，旧版本保留；非该用户命名空间的导入映射只读。活动映射最多 4096，提供方和标识精确匹配，不采用相似度。目标可以尚未有价格，仍为未计价；新增时拒绝重复键、链式映射的两个方向、循环及自映射，已有不明确目录继续由 PriceCatalog 隔离。

别名、价格规则共用修订，查询 / 分页依旧固定旧 price_revision；消费事件、Token 和数据修订不改变。失败回滚旧行退休及版本，只有成功提交发出既有 price_rules_changed，响应复用最新隐私投影，开启时移除别名 / 规则。主窗口命令 / AppManifest / capability / Rust / TS / schema 已同步，mini 不获权限。

自动：计价领域 11 项、存储及查询计价 18 项通过，新增 5 项覆盖严格 DTO、独立预期 900 / 1300 原子费用、旧读事务 / 历史版本、退休恢复未计价、重复 / 链 / 循环 / provider 隔离、写入和提交前故障回滚、规则 / 别名并发 CAS 单赢家及消费 110 不变。core / store / desktop all-targets strict Clippy、fmt、前端 typecheck 和契约漂移检查通过；未运行性能测试。

实际：新增显式 -PriceAliases 场景，隔离 native-probe 库、无来源 / 无账户、真实两个 WebView与正式 SQLite 写入。主端 IPC 创建 / 替换 / 历史 / 退休、旧修订与冲突拒绝、共享隐私响应及关闭隐私后读取已提交映射、仅四次成功发布通知、mini 拒绝通过；NATIVE_PRICE_ALIAS_OK、退出 0，WebView2 1412 保留。此步骤为写入和正式 IPC，别名设置编辑界面、离线目录、费用缓存 / 后台重估继续实施，不声称全部 M09 完成。

## M09f2：正式模型别名编辑器

价格规则页内新增模型别名区，保留主导航 / 统一筛选和原布局；从同一 PriceRulesSnapshot 显示 provider / 日志模型 / 标准模型 / 发布版本。支持用户映射新增、编辑替换、退休、取消和历史版本复核；历史与非 alias-custom- 目录映射只读。编辑基于捕获的原 price_revision，当前快照刷新不覆盖草稿或改写其基线；失败 / CAS 冲突保留内容，映射冲突给明确别名说明。规则 / 别名编辑互斥，忙状态阻止重复提交，保存返回刚发布完整快照并沿用成功刷新通知。

目标模型仍可未有单价，明确说明保持未计价；自映射和空值 / 控制字符 / UTF-8 超长在前端拒绝，后端仍权威校验。不自动猜模型或更改消费。新增 runtime 方法继续经过显示策略 / epoch / 响应身份门禁；共享隐私卸载价格与别名编辑器，关闭后重新读取，不恢复已清除草稿。

自动：价格 / 别名 Playwright 4 项通过，包含两项既有规则回归及新增发布 / 替换 / 历史 / 退休、自映射 / 冲突 / 原修订草稿保持；typecheck / Vite 生产构建、desktop strict Clippy / fmt 通过。查看深色 1280 编辑器与浅色 960 列表截图；发现并修正继承价格表 min-width 造成的别名操作裁切，再查看按钮完整无裁切的最终图。合成 UI DTO 不含生产演示单价。

实际：-PriceAliases 追加真实 React DOM 输入与按钮，正式 WebView / SQLite 创建 / 替换 / 历史只读 / 退休；外部实际 IPC 发布后刷新，旧草稿仍使用捕获版本并拒绝，输入完整保留。随后真实共享隐私切换移除该未保存编辑器和旧文本，关闭隐私后新查询，不恢复草稿；确认八次成功通知，失败无通知。Win10 19045 / 150%、NATIVE_PRICE_ALIAS_OK、退出 0；WebView2 1412 仍记录。这里为实际 WebView / 原生 IPC，不代表全部物理鼠标或屏幕阅读器检查。

模型别名普通编辑流程已经贯通，实际离线价格目录、持久费用缓存和独立后台重估继续推进。未运行性能测试，整体交付尚未完成。

## M09g1a：离线事实目录与不可变发布

核对官方完整价格 Markdown 及七个历史 Codex 模型页，内嵌 51 个确切模型、172 条 Standard / Batch / Flex / Fast / Ultrafast 与上下文档位事实，保留每百万十进制价格、缓存写入维度、官方链接和核实日。开发更新脚本只读公开资料且拒绝表形状 / 标识 / 单价变化；生产没有在线抓价或演示价依赖。地区加价、其他媒介 / 微调 / 工具与尚未开始计费的条目不混入文本 Token 估算。

core 严格校验目录格式、唯一档位、All 与 Short / Long 冲突、费率精度、受控官方链接及确切模型标识。37 条 Standard、All、无独立缓存写入价格可转为规则；需额外请求证据的其余条目保留事实且不自动扁平计价。当前 API 参考价不能证明过去日期，规则从核实日 UTC 零点起，历史未计价 / 明确指定估价时点保持原有语义。

schema v4 保存不可变目录正文 / 摘要 / 发布修订，Writer 同事务提交候选规则与 price revision。重复同内容发布不推进修订，同 ID 变价和降级拒绝；新目录发布新不可变历史规则封闭旧区间，旧 SQLite 快照 / 历史价格版本仍保留原规则。自定义规则优先且不可被目录退休；离线规则不能由用户替换 / 退休；失败整批回滚。消费和 data revision 不变。既有历史 migration 未改；旧测试中 target schema 的硬编码 3 调整为当前版本常量。

自动验证：新增 core 2 项、真实 SQLite 4 项通过，独立预期验证 0.0002205 USD、指定时点评估、未知条件未计价、重复安装 / 重开、跨事务旧快照、目录更新后旧区间与新价、自定义优先、身份冲突 / 降级、发布失败保留目录 / 规则 / 修订、用户不能编辑离线规则。普通 store lib 171 通过 / 1 性能夹具 ignored；core / store strict all-target Clippy、fmt 通过。没有实际 Windows UI 验收，生产启动 / 正式目录 IPC / 浏览界面与完整条件匹配接下来实施；持久缓存和后台重估仍未完成。未运行性能测试。

## M09g1b：独立启动价格发布与正式目录浏览

正常 debug / release 启动在采集服务之前发布内嵌目录，重复启动同内容无新修订，不依赖 Node、开发服务器或在线价格服务。新 main-only `get_offline_price_catalog` 支持当前 / 指定价格修订，返回固定版本的目录或 null；超前 / 非法修订拒绝。使用同一真正只读 SQLite 事务和最新显示策略序列化，隐私时清空目录，mini 能力中没有此命令。类型 / schema 从 Rust 生成。

价格设置页保留原规则和别名布局，新增只读目录、核实日 / 发布 ID / 参考说明、确切模型搜索、处理模式选择和各上下文 / 缓存写入档位。注明全球 API 参考、其他费用另计、条件不足保持未计价及历史日期可明确选择估价时点；不冒充订阅账单。目录固定父规则的价格修订，加载时清除旧内容；独立错误 / 重试不阻止自定义编辑，返回较晚的旧请求不会覆盖新修订。隐私卸载后关闭时重新查询，不恢复旧搜索。

自动：新增目录 Playwright 2 项和原价格 / 别名 4 项全部通过，验证目录事实、模式 / 条件 / 来源、精确未知模型空态、历史 null、独立失败重试和旧请求竞争。查看深色 1280、浅色 960 最终截图；修正适用情况列的窄屏挤出后六列完整显示，页面无水平溢出，表区仍支持必要滚动。typecheck / Vite 构建、contracts:check、desktop strict Clippy / fmt 和 release cargo check 通过。

实际：Win10 19045 / 150% DPI `-OfflinePrices` 使用隔离库中的正常启动发布，main / mini 真 WebView 经正式 IPC 检查 172 条事实 / 37 条安全规则、历史 0、非法版本、实际 React 搜索 / 模式 / 历史切换、mini 拒绝、共享隐私清空 / 重开新查询、重复发布及消费修订不变；NATIVE_OFFLINE_PRICES_OK、退出 0，WebView2 1412 保留。只读官方资料、公开目录和合成使用场景分开，不读取用户日志 / 账户，不运行性能测试。完整请求计价条件、持久费用缓存与独立重估仍需继续，整体交付未完成。

最终样式构建后重复定向 -OfflinePrices 仍退出 0；随后 -PriceAliases 实际 main / mini IPC 与 React 既有编辑 / 历史 / 原修订冲突 / 隐私回归打印 NATIVE_PRICE_ALIAS_OK、退出 0。两场景均为本模块修改后的独立构建，WebView2 1412 提示仍保留。

## M09g2a：持久事件费用缓存与同快照查询

schema v5 保留既有事实表与历史 migration，追加缓存集合的账本证据 / 解析核算版本 / 算法版本 / 数量 / 发布摘要，以及每个事件计价输入的 SHA-256。复用 `valuation_sets` 和 `event_valuations` 保存精确费用原子或未计价原因；未计价金额 / 币种 / rule 为 null，真实零金额才保存零。缓存身份同时固定价格修订、估价模式、确切指定时点、事件模型 / 提供方 / 全体真实来源 / 时间 / 向量 / 核算版本；显示来源筛选不能缩小匹配证据。

内部构建从单个实际读事务流式读取活跃账本，用固定规则与别名估算，每 500 行 Writer 事务暂存候选，不保留全量事件数组。发布前重新核对当前活跃账本与证据版本，并读取候选复核精确数量 / 完整摘要，在同一事务设置 ready。进度回调与 AtomicBool 取消已接内部 API；取消、源事实变化、发布失败均使候选不可用，先前 ready 集合保留。正常启动将未完成 building 集合中断。读写不改变 Token、data / price / settings 修订、原始观察或采集检查点。

费用 visit 接 CacheReader，覆盖总览、模型 / 项目分组、会话 / 回合与小窗共用路径，明细真实租约也接同一读取。仅取 ready 且输入 / 规则 / 模式完全匹配的项；其余按当前实际快照中的同版本规则即时补算。旧真实 SQLite 快照仍保留旧费用，不能因为后台填充而混入新规则。查询复用预编译语句；没有匹配集合时一次检查后直接估算，不逐事件反复准备不存在的缓存查询。

自动验证：新增 8 项真实 SQLite / 合成用量单价测试通过，独立期望 900 / 1300 / 1500 / 1600 原子与对应定点金额；持久重开 / 同内容复用、检查点和事实修订不变、超 JS 安全整数的 `90071992547409930` 原子、指定模式 / 时点隔离、旧真实快照、新增镜像后的来源筛选歧义、601 事件跨批次取消与重试、候选过期 / 新事件即时补算、失败发布保留旧缓存都覆盖。601 事件为事务边界功能夹具，无性能计时门槛或报告。普通 store lib 179 通过 / 1 性能夹具 ignored；store / desktop strict all-target Clippy、fmt 与 release check 通过。

实际回归：最终 CacheReader 构建后 Win10 19045 / 150% 的 -OfflinePrices 原生启动 / main-mini IPC / 目录与共享隐私场景退出 0、NATIVE_OFFLINE_PRICES_OK，WebView2 1412 保留。此场景验证新增结构与既有页面 / IPC 正常运行；没有正式后台重估 UI，不能把此回归当作费用后台服务的实际验收。独立服务、自动填充 / 价格变化调度、正式重估进度与取消入口继续 M09g2b；完整请求条件计价仍待完成。未运行性能测试，整体目标继续进行。
