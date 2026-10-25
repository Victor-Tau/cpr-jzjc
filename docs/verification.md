# v1.7.3 验证记录

使用 Codex Proxy RS 3.31.1 官方标签对应提交 `06e1d8f0ac33d8633198d837c4f9017ad466f358` 的 SDK 和打包工具重新编译。

适配 Codex Proxy RS `>=3.31.1,<3.32.0`，Linux x86_64、glibc 2.36+。RPC v5、管理能力 v2、维护能力 v1 和私有状态 schema 保持兼容。SDK 差异包括新增 outbound 中间件合同，以及移除模型响应观测中的旧模型名称字段；本插件使用的账户、状态、模型流、管理与维护接口无需源码迁移。

## 单核构建

前端、Rust 插件和 CPR 打包工具全部通过 `taskset -c 0` 绑定到 CPU 0，设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`。插件使用仓库固定的 Rust 1.99.1，前端使用 Node 24.22.0。

```bash
taskset -c 0 env CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 \
  cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline \
  --release --target x86_64-unknown-linux-gnu.2.36
```

CPR CLI 从 `v3.31.1` 官方源码的 `backend/apps/plugin-cli` 编译，使用官方 workspace 锁文件；插件 SDK 依赖固定到同一提交，前后端依赖锁文件保留。

私有 Python/NumPy 运行时从验证过原包摘要及全部资源摘要的 1.7.2 安装包复用，并通过 `refresh_runtime.py` 刷新随附源码。运行时依赖和评分算法未修改。

## 验证结果

交付文件：`dist/personal.modeltrace-health-1.7.3-x86_64-unknown-linux-gnu.tar.gz`，30,755,419 字节，SHA-256 `7846d99ec8c61446011a321a500e87a5088e92e82fd0a921cc26ea618245f25c`。

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 17 项通过 |
| Python 评分单元测试 | 6 项通过 |
| 静态检查 | `cargo fmt --check`、Clippy `-D warnings`、`vue-tsc --noEmit` 通过 |
| 前端生产构建 | Vite 构建通过，2,158 个模块 |
| 安装包校验 | 包摘要、全部资源摘要、manifest、协议 v5、私有 Python/NumPy 离线评分和实际插件解包检测通过；18 个指纹模型 |
| RPC 回归（安装包二进制） | 68 项全部通过，347.493 秒 |
| 升级回归 | 1.7.2 → 1.7.3 通过：配置、计划、历史和指纹库保留，无探针重发 |
| 受限运行时解包 | 通过 strace 拒绝时间戳系统调用后，实际发布二进制仍完成检测与评分 |

运行时归档包含 1,303 个文件，展开 90,710,161 字节，SHA-256 `f57f34194ad4dd313bb0942d23d213970b9bba96c1cd4145a94b07db8d4ec37e`。第三方声明重新收集后核对，许可条目及正文与原版完全一致，保留原排列。

Zig 链接保留既有优化参数弃用提示，发布构建成功。受限解包测试首次在沙箱中因禁止 `ptrace` 无法启动，随后在允许跟踪自有子进程的环境重跑通过。

测试使用实际发布二进制与模拟宿主，不发送真实账户模型请求。本次未在生产 CPR 宿主安装或切换版本；前端仅更新宿主版本文案，类型检查和生产构建通过，未重跑浏览器交互测试。

## 历史验证

[1.7.2](verification-v1.7.2.md)、[1.7.1](verification-v1.7.1.md)、[1.7.0](verification-v1.7.0.md)、[1.6.1](verification-v1.6.1.md)。
