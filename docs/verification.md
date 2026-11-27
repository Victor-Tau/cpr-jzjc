# v2.1.3 验证记录

宿主范围为 CPR `>=3.54.0,<3.57.0`。发布平台为 Linux x86_64 和 ARM64，要求 glibc 2.36+。SDK 与打包工具固定到 CPR v3.56.0 提交 `a46d0f4c08bcf0f89aa004eb1d089e9e0039898c`。

## 兼容性核对

宿主保持清单 v2、RPC v7、管理能力 v2、维护能力 v1 和页面宿主桥 v2。此版 SDK 的新增声明用于中间件，插件使用管理和维护能力。

CPR 3.56 移除了 Responses、Messages 和 GenAI 之间的内置转换。检测请求的协议与目标账号提供方均为 OpenAI，走宿主原生 Responses 路径。模型列表继续以所选 Key 可通过该协议访问的模型为准。

## 验证

- Rust 格式检查、Clippy、28 项单元测试、前端构建和 API 回归通过。
- 使用官方打包工具生成 macOS ARM64 验证包。在隔离的 CPR 3.54.3、3.55.3 和 3.56.0 上通过安装、启用、管理页面资源、账号列表、指纹库加载及配置读写验证。联调使用空账号目录。
- Linux x86_64 和 ARM64 的发布包构建和模拟宿主 RPC 回归由 [GitHub Actions](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml) 执行。

## 历史验证

[2.1.2](verification-v2.1.2.md)、[2.1.1](verification-v2.1.1.md)、[2.1.0](verification-v2.1.0.md)、[2.0.0](verification-v2.0.0.md)、[1.28.0](verification-v1.28.0.md)、[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
