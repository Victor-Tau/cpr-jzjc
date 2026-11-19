# v1.26.0 验证记录

适配 CPR `>=3.51.0,<3.52.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.51.0 提交 `ba2afe67d04755a247a9d534115ac168d4db7478`。

## 合同核对

对照 CPR v3.49.0 与 v3.51.0 官方源码：

- 官方打包器、manifest v2、RPC v7、管理能力 v2 和维护能力 v1 合同未变。
- 插件 SDK 变化：调度能力升至 v4（候选新增 `session_bound` / `role`），新增 `quota_history` 额度历史查询；插件不声明调度能力，也不调用该接口。账户、分组、模型目录、受管 HTTP 与运维错误日志合同及私有状态 schema 未变，无需插件业务代码调整。

## 本地验证

- 本次为快速发布，仅做合同核对与锁定依赖构建检查（`cargo check --locked`）；完整单元测试、RPC 回归和浏览器回归未在本机重跑。发布工作流会在 x86_64 与 ARM64 原生 runner 上构建，并对两种架构的最终包运行 RPC 回归。
- 发布附件由 CI 构建，摘要以 Release 中的 `.sha256` 为准。

## 历史验证

[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)、[1.19.0](verification-v1.19.0.md)、[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
