# v2.1.4 验证记录

宿主范围为 CPR `>=3.54.0,<3.58.0`。发布平台为 Linux x86_64 和 ARM64，要求 glibc 2.36+。SDK 与打包工具固定到 CPR v3.57.0 提交 `6950c02ee460143dd63103a35a5dd679712488dc`。

## 兼容性核对

CPR 3.57 将 OpenAI 请求和 WebSocket 连接池配置纳入运行设置。插件通过账户管理、私有状态和原生 Responses 请求完成检测，继续使用宿主当前配置。宿主支持的清单 v2、RPC v7、管理能力 v2、维护能力 v1 及页面桥 v2 保持兼容。

## 验证

- Rust 格式检查、Clippy、28 项单元测试、前端构建及 API 分页回归通过。
- 模拟宿主验证了账号绑定与评分、配置导入预览、分组分页及失效令牌处理。
- 在隔离的 macOS ARM64 宿主 CPR 3.54.3、3.55.3、3.56.0 和 3.57.0 上，验证安装、启用、页面资源、账号列表、指纹库和配置读写。联调使用空账号目录。
- Linux x86_64 和 ARM64 的发布包构建与完整 RPC 回归由 [GitHub Actions](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml) 执行。

## 历史验证

[2.1.2](verification-v2.1.2.md)、[2.1.1](verification-v2.1.1.md)、[2.1.0](verification-v2.1.0.md)、[2.0.0](verification-v2.0.0.md)、[1.28.0](verification-v1.28.0.md)、[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
