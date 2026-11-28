# v2.1.6 验证记录

宿主范围为 CPR `>=3.54.0,<3.58.0`。发布平台为 Linux x86_64 和 ARM64，要求 glibc 2.36+。

## 兼容性核对

对照 CPR v3.57.1 与 v3.57.2 官方源码，插件 SDK 和打包工具没有变更。SDK 与打包工具固定到 CPR v3.57.2 提交 `533a415a9984691608799e9d2d5bf79968b3e32f`。检测沿用 OpenAI 原生 Responses，配置、计划、历史和指纹库的数据格式不变。

## 发布验证

本次按快速发布要求，不重复执行本地完整测试；通过 GitHub Actions 构建两个 Linux 架构，保留既有前端构建、Rust 检查、API 和打包后 RPC 检查。发布任务在两个架构构建成功并校验安装包及 SHA-256 后上传附件。

[发布工作流](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml)。本次未进行真实 CPR 3.57.2 宿主安装联调。

## 历史验证

[2.1.5](verification-v2.1.5.md)、[2.1.4](verification-v2.1.4.md)、[2.1.2](verification-v2.1.2.md)。
