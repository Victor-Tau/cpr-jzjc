# v1.23.0 验证记录

适配 CPR `>=3.47.0,<3.48.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 v3.47.0 提交 `0e02a056b5a4fcb9fe204e741cbc20575041d7ba`。

## 合同核对

对照 CPR v3.45.1 与 v3.47.0 官方源码：

- 插件宿主兼容声明继续支持 manifest v2、RPC v7、管理能力 v2 和维护能力 v1；官方 CLI 与插件打包合同未变。
- SDK 新增智能调度的额度重置窗口权重字段，使用默认值兼容旧宿主设置。插件不读取智能调度设置，无需业务代码调整。
- CPR 3.46.0 起，宿主管理的出站网络在未指定可用代理时使用部署默认出口。插件指纹库更新继续通过宿主 HTTP 出站，相关注意事项见 [v1.23.0 发行说明](releases/v1.23.0.md)。
- 插件使用的账户列表、账户更新、分组、模型目录、受管 HTTP 与运维错误日志接口在本次范围内保持兼容；插件 RPC、manifest 与私有状态 schema 不变。

## x86_64 本地构建与验证

使用 Rust 1.99.1、CPR 固定提交的官方 `cpr-plugin`、pnpm 9.12.2 和固定 Python 评分运行时执行 `scripts/package.sh`。完整打包脚本及最终产物校验通过。

- `cargo fmt --check`、Clippy `--all-targets -D warnings` 与 26 项 Rust 单元测试通过。
- Python 评分 6 项单元测试、构建脚本 5 项防错测试和分页 API 回归通过。
- 前端类型检查与 Vite 生产构建通过；本地 Node.js 为 22.23.4，pnpm 提示项目声明 Node.js >=24，发布 CI 使用 Node.js 24 验证。
- 最终安装包摘要、资源摘要、manifest、x86_64 ELF、评分运行时、18 个模型离线校验及插件运行时解包验证通过。
- 最终安装包二进制的 82 项 RPC 集成回归通过。
- 1.22.0 → 1.23.0 同实例升级验证通过，保留配置、计划、历史及指纹库，无探针重放。

本地安装包：`dist/personal.modeltrace-health-1.23.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,691 字节，SHA-256 `26d1edf9ecfb4eadf41c307bd360050aaacbdee59ec351a57215236a214b7b32`。Release 附件由 CI 构建，摘要以 Release 中的 `.sha256` 为准。

## 双架构 CI

标签发布由 GitHub Actions 在 x86_64 与 ARM64 原生 runner 上分别构建完整安装包并执行最终包 RPC 回归；两个架构均通过后才上传 Release。CI 使用 Node.js 24，验证前端声明的运行版本。

## 复现命令

```bash
python scripts/install_packager.py
bash scripts/package.sh
python scripts/protocol_smoke.py --binary /path/to/unpacked/bin/modeltrace-health -v
python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.22.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.23.0-x86_64-unknown-linux-gnu.tar.gz
```

构建命令与工具准备见[开发与构建](development.md)。

## 验证边界

使用最终 x86_64 安装包、生产前端及模拟宿主验证，未在生产 CPR 宿主安装，未访问真实账户。ARM64 安装包由发布 CI 构建和验证。

## 历史验证

[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-v1.21.0.md)、[1.20.0](verification-v1.20.0.md)、[1.19.0](verification-v1.19.0.md)、[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
