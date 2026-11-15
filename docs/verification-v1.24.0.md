# v1.24.0 验证记录

适配 CPR `>=3.48.0,<3.49.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.48.0 提交 `d787195f4ed335a28368c2b334b5ab569b938fbf`.

## 合同核对

对照 CPR v3.47.0 与 v3.48.0 官方源码：

- 插件 SDK 与官方打包器源码未变；宿主仍支持 manifest v2、RPC v7、管理能力 v2 和维护能力 v1。
- 插件使用的账户、分组、模型目录、受管 HTTP 与运维错误日志合同及私有状态 schema 未变，无需插件业务代码调整。

## x86_64 本地构建与验证

- Rust 1.99.1 下 `cargo fmt --check` 和 `cargo zigbuild --locked --release --target x86_64-unknown-linux-gnu.2.36` 通过。
- 前端 `pnpm build` 类型检查和生产构建通过。本机 Node.js 22.23.4 低于项目声明的 Node.js 24；发布 CI 使用 Node.js 24。
- 从已验证的 1.23.0 安装包复用未变更的 Python/NumPy 运行时，使用 CPR v3.48.0 官方打包器生成并验证安装包；插件解包、资源摘要、ELF 架构及 18 个模型离线评分验证通过。
- 完整单元测试、RPC 回归和浏览器回归按本次快速发布范围未在本机重跑；发布工作流会在 x86_64 与 ARM64 原生 runner 上构建，并对两种架构的最终包运行 RPC 回归。

本地安装包：`dist/personal.modeltrace-health-1.24.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,307 字节，SHA-256 `ecd84d06c77e28b6a0839377e6602580f4d19c3663bb32aac3c55b175669a614`。发布附件由 CI 构建，摘要以 Release 中的 `.sha256` 为准。

## 历史验证

[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)、[1.19.0](verification-v1.19.0.md)、[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
