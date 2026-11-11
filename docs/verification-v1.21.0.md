# v1.21.0 验证记录

适配 CPR `>=3.44.0,<3.45.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具仍固定至 v3.44.0 提交 `c622f085748f812936a7371fcc85d1621fa72ca8`。

## 变更范围

合并 [#5](https://github.com/Victor-Tau/cpr-jzjc/pull/5)：后端按编译目标选择私有 Python 加载器与库目录，构建脚本通过 `CPR_TARGET` 选择目标，运行时构建、复用和产物校验按目标核对固定镜像、NumPy wheel 摘要与 ELF 架构，新增双架构 GitHub Actions 构建与发布。业务逻辑、评分算法、RPC v7、管理能力 v2、180 秒维护预算、UI 0.8.0 和私有状态 schema 与 1.20.0 一致。

合并前核对：工作流使用的 6 个官方 Action 提交与对应标签一致；ARM64 NumPy 2.4.3 wheel 摘要与 PyPI 一致；ARM64 基础镜像为 `python:3.11.17-slim-bookworm` 的 arm64 清单，与 x86_64 同为 Python 3.11.17；CI 构建容器固定至摘要。工作流仅在 `pull_request` 下以只读权限运行，发布任务仅在推送 `v*` 标签时获得写权限。

## x86_64 本地构建与验证

使用 Rust 1.99.1、Node.js 24、pnpm 9.12.2、Python 3.11 和 Zig；通过 `scripts/package.sh` 完整构建，私有运行时由固定基础镜像与 wheel 全新构建。

- Rust 格式检查、Clippy `--all-targets -- -D warnings` 和 26 项单元测试通过。
- 前端锁文件安装与生产构建通过；Python 评分 6 项单元测试、构建脚本 5 项防错测试和分页 API 回归通过。
- 最终安装包摘要与资源摘要、manifest 与源码一致性、目标架构、RPC v7、18 个模型离线校验、Rust 运行时解包及完整评分通过。
- 最终安装包二进制的 82 项 RPC 场景全部通过。
- 1.20.0 → 1.21.0 升级验证通过，保留配置、计划、历史和指纹库，无探针重放。

本地安装包：`dist/personal.modeltrace-health-1.21.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,437 字节，SHA-256 `fbd5855cb030cf8e0b92d094ee29e2363c2a8cc4d99f3280232bbe48df554cb7`。Release 附件由 CI 构建，摘要以 Release 中的 `.sha256` 为准。

## 双架构 CI

GitHub Actions 在 `ubuntu-24.04`（x86_64）与 `ubuntu-24.04-arm`（ARM64）原生 runner 的 Debian 12 / Python 3.11.17 容器内，分别执行完整 `scripts/package.sh`、最终包校验和 82 项 RPC 回归。标签发布等待两个任务成功、核对四个文件及摘要后上传 Release。

贡献者此前在 ARM64 Linux 容器内原生完成同样的验证，记录见 [ARM64 验证记录](verification-arm64.md)。

## 复现命令

```bash
bash scripts/package.sh                                        # x86_64
CPR_TARGET=aarch64-unknown-linux-gnu CPR_NATIVE_BUILD=1 \
  bash scripts/package.sh                                      # ARM64 原生环境
python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.20.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.21.0-x86_64-unknown-linux-gnu.tar.gz
python scripts/protocol_smoke.py --binary .cache/pkg-v1.21.0/bin/modeltrace-health -v
```

构建命令与工具准备见[开发与构建](development.md)。

## 验证边界

使用最终安装包二进制、生产前端与模拟宿主验证，未在生产 CPR 宿主安装，未访问真实账户。ARM64 安装包未在真实 ARM64 CPR 宿主上安装验证。前端与 1.20.0 相同，本次未重跑浏览器回归。

## 历史验证

[1.20.0](verification-v1.20.0.md)、[1.19.0](verification-v1.19.0.md)、[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
