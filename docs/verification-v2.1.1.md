# v2.1.1 验证记录

适配 CPR `>=3.54.0,<3.55.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.54.0 提交 `a770419818117098c43919a5c6553cc7023ecf71`；检测内核与内置数据与 2.1.0 一致。

## 兼容性核对

对照 CPR v3.53.1 与 v3.54.0 官方源码：SDK 变化限于运行设置（新增必填字段）与 Fast 档位 `ultrafast`，插件未调用运行设置读写接口，也不读取 `fast_mode`；manifest、RPC、管理与维护合同未变，无需业务代码调整。

## 本地验证（x86_64）

- 按发布要求精简验证：`cargo fmt --check`、`cargo clippy --all-targets -D warnings`、`cargo test`（28 项）通过。
- 发布附件由 CI 在 x86_64 与 ARM64 原生 runner 上构建并运行最终包 RPC 回归，摘要以 Release 中的 `.sha256` 为准。

## 历史验证

[2.1.0](verification-v2.1.0.md)、[2.0.0](verification-v2.0.0.md)、[1.28.0](verification-v1.28.0.md)、[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
