# v1.28.0 验证记录

适配 CPR `>=3.53.1,<3.54.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.53.1 提交 `188a6817d141a8d9a1a0bceead4f07477d35276e`。

## 合同核对

对照 CPR v3.52.2 与 v3.53.1 官方源码：

- 插件 SDK 代码、官方打包器及宿主插件合同未变（仅文档与测试调整）；仍支持 manifest v2、RPC v7、管理能力 v2 和维护能力 v1。无需插件业务代码调整。

## 本地验证

- 本次为快速发布，仅做合同核对与锁定依赖构建；完整单元测试、RPC 回归和浏览器回归未在本机重跑。发布工作流会在 x86_64 与 ARM64 原生 runner 上构建，并对两种架构的最终包运行 RPC 回归。
- 发布附件由 CI 构建，摘要以 Release 中的 `.sha256` 为准。

## 历史验证

[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
