# v2.1.8 验证记录

宿主范围为 CPR `>=3.54.0,<3.59.0`。发布平台为 Linux x86_64、Linux ARM64（glibc 2.36+）及 macOS ARM64。

[构建工作流](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml)在两个 Linux 原生目标运行 Rust 检查、前端构建、API 分页检查和打包后 RPC 回归。macOS ARM64 使用同一打包及验证脚本原生构建，检查 Mach-O 架构、资源摘要和进程内评分。

[CPR 3.58.0 的兼容性核对](verification-v2.1.7.md)
