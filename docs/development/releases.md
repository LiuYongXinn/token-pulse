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
