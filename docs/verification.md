# v2.1.2 验证记录

适配 CPR `>=3.55.3,<3.56.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.55.3 提交 `ef45165d8d30cfaee65558807b401541503d8aea`；检测内核与内置数据保持不变。

## 兼容性核对

对照 CPR v3.54.0 与 v3.55.3 官方源码，SDK 源码仅修改 `call/services/settings.rs`：运行设置字段更名、新增 `AnonymizeProject` 枚举值、定价目录新增 `supported_bands`。插件未调用这些接口，无需业务代码调整。SDK 的 Cargo 依赖、manifest v2、RPC v7、管理能力 v2 与维护能力 v1 无变化。

## 验证范围

按快速发布要求，本地仅做版本、锁文件、宿主范围、源码兼容性和差异检查。发布构建交由现有 GitHub Actions 工作流：前端类型检查与构建、Rust 格式检查、单元测试、Clippy、API smoke、最终安装包验证及 RPC 回归；x86_64 与 ARM64 均成功后自动发布。本次不执行额外浏览器或真实宿主升级回归；CI 状态以对应版本的工作流运行为准。

## 历史验证

[2.1.1](verification-v2.1.1.md)、[2.1.0](verification-v2.1.0.md)、[2.0.0](verification-v2.0.0.md)。
