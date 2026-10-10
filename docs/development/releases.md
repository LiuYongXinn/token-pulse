# 发布与更新包

正式版本发布到 [GitHub Releases](https://github.com/LiuYongXinn/token-pulse/releases)。Windows x64 采用 NSIS 安装包，应用更新使用项目既有密钥生成的版本绑定签名。

## 版本与源码

发布前统一 `package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock` 和 `src-tauri/tauri.conf.json` 的应用版本。每次发布使用新的 `v<版本>` 标签，标签指向实际发布源码提交；已发布版本保留。

1.0.1 包含 1.0.0 之后的本地盘符兼容、来源与消费关联扩展、同快照分组分页、候选继续浏览、大标题索引增量读取、来源轮次时间、完整透明度范围、PATH 与账户额度能力检测修复，以及统计表布局与查询交互调整。详细依据见[项目限制审查](restriction-audit.md)。

## 构建、签名与准备

在仓库根目录执行：

```powershell
npm run tauri:build -- -- --locked --offline
npm run release:sign
npm run release:prepare -- --desktop target/release/token-pulse-desktop.exe --installer target/release/bundle/nsis/TokenPulse_1.0.1_x64-setup.exe --signature target/release/bundle/nsis/TokenPulse_1.0.1_x64-setup.exe.sig --output .local/artifacts/release-v1.0.1 --notes .local/tmp/release-v1.0.1-notes.md --published-at <UTC时间>
```

离线构建要求锁定依赖已在本地缓存。构建入口同时准备前端、对应版本的任务栏宿主和第三方声明。签名入口校验本地密钥登记、权限和项目公钥，密码经当前用户 DPAPI 解密，仅传给签名进程。

准备脚本使用同次 release 主程序的无窗口维护入口校验签名、版本和目标平台，再生成安装包副本、签名、`latest.json` 和本地 `release-verification.json`。输出目录及已有签名禁止覆盖，重复操作使用新的验证目录。

## 上传与公开验证

上传 `TokenPulse_<版本>_x64-setup.exe`、对应 `.sig`、`latest.json` 和 `SHA256SUMS.txt`。先在草稿 Release 中检查全部附件大小与 SHA-256，再公开为正式最新版本。校验文件记录安装包、签名和更新元数据的 SHA-256；本地验签回执保存在忽略的 `.local/artifacts/`，不作为用户下载附件。

公开后通过不带认证信息的下载地址获取附件，核对 SHA-256、更新元数据的版本与安装包 URL，并用同次 release 主程序再次验签。源码标签、公开安装包和更新元数据必须对应同一版本。

## 验证边界

发布检查包括前端类型与生产构建、单元测试、浏览器回归、发布脚本回归、Rust 工作区测试、格式与生成契约一致性，以及实际安装包的版本绑定签名验证。自动构建、验签和匿名下载核验不代表已完成本机重新安装、真实多屏 / DPI / 物理输入或所有 Windows 任务栏结构验收；这些结果单独记录在[交付记录](delivery-status.md)。

## 1.0.1 构建验证记录

2026-10-10，正式 Windows x64 构建、现有项目密钥签名和同次 release 主程序原生验签均成功。安装包为 `TokenPulse_1.0.1_x64-setup.exe`，7,071,548 字节，SHA-256 为 `a64af8afea9668335cccd3b44e6304070c7d6d805847144c09518d211238c68b`。

本轮 TypeScript、前端生产构建、57 项 Vitest、13 项发布脚本回归、128 项 Playwright 和 Rust 工作区全功能串行测试均通过；Rust 799 passed / 0 failed / 5 ignored。Rust 格式、应用生成契约和任务栏宿主生成契约检查通过。首次宿主契约检查发现遗漏两个标题索引诊断代码，重新生成仅补齐对应枚举值并单独提交，重跑通过。

本地日志为 `.local/tmp/release-v1.0.1-*.log`，准备的安装包、签名、更新元数据、校验文件和原生回执位于 `.local/artifacts/release-v1.0.1/`。本轮未重新安装或启动用户的正式应用，真实系统交互验收范围沿用上节说明。
