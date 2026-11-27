# v2.1.5 验证记录

宿主范围为 CPR `>=3.54.0,<3.58.0`。发布平台为 Linux x86_64 和 ARM64，要求 glibc 2.36+。

## 兼容性核对

对照 CPR v3.57.0 和 v3.57.1 的发行源码。插件 SDK、打包器、清单 v2、RPC v7、管理能力 v2、维护能力 v1 和页面桥 v2 的实现一致。SDK 与打包工具继续固定到 CPR v3.57.0 提交 `6950c02ee460143dd63103a35a5dd679712488dc`。

## 验证

- 在隔离的 CPR 3.57.1 macOS ARM64 宿主上验证安装、启用、页面资源、账号列表、指纹库和配置读写。联调使用空账号目录和本地测试包。
- [构建工作流](https://github.com/Victor-Tau/cpr-jzjc/actions/workflows/build.yml) 在 Linux x86_64 和 ARM64 上构建发布包，运行单元测试、API 分页和打包后 RPC 回归。

## 历史验证

[2.1.4](verification-v2.1.4.md) 包含 CPR 3.54.3 至 3.57.0 的联调记录。
