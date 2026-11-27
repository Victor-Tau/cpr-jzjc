# v2.1.2 验证记录

适配 CPR `>=3.54.0,<3.56.0`。发布平台为 Linux x86_64 和 ARM64（aarch64），要求 glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.55.3 提交 `ef45165d8d30cfaee65558807b401541503d8aea`。

## 兼容性核对

对照 CPR v3.54.0 和 v3.55.3 的官方源码，SDK 源码变更集中在运行设置、隐私规则与定价类型。插件使用的清单 v2、RPC v7、管理能力 v2、维护能力 v1 以及网页桥接 v2 的合同一致。账户管理和运维错误日志接口继续提供插件读取的字段。

## 发布验证

- 发布工作流在两种架构的 Linux runner 上分别执行 Rust 格式检查、Clippy、单元测试、前端构建和 API 回归。
- 官方打包工具生成安装包后，工作流校验包内文件和二进制架构，并对解包后的二进制运行模拟宿主 RPC 回归。
- 发布任务核对两种架构的安装包及 SHA-256 摘要后上传附件。[发布工作流](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml)

同一 SDK、清单和程序源码已在 fork 的 Linux x86_64 和 ARM64 runner 上验证。两种架构各通过 28 项 Rust 单元测试和 84 项模拟宿主 RPC 回归。真实 CPR 宿主安装联调尚未完成。[fork 验证记录](https://github.com/monkeylxb/cpr-jzjc/actions/runs/45839934517)

## 历史验证

[2.1.1](verification-v2.1.1.md)、[2.1.0](verification-v2.1.0.md)、[2.0.0](verification-v2.0.0.md)、[1.28.0](verification-v1.28.0.md)、[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
