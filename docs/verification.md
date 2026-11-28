# v2.1.7 验证记录

宿主范围为 CPR `>=3.54.0,<3.59.0`。发布平台为 Linux x86_64 和 ARM64，要求 glibc 2.36+。

## 兼容性核对

CPR 3.58.0 与 3.57.2 的插件 SDK 和打包工具源码一致。清单 v2、RPC v7、管理能力 v2 和维护能力 v1 继续适用。检测沿用 OpenAI 原生 Responses，配置、计划、检测历史和指纹库沿用原有数据格式。

## 验证

[构建工作流](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml)在两种 Linux 架构上运行 Rust 检查、前端构建、API 分页检查和打包后 RPC 回归。

## 历史验证

[2.1.6](verification-v2.1.6.md)。
