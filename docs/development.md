# 开发与构建

[返回项目首页](../README.md) · [使用指南](usage.md) · [当前发行包验证记录](verification.md)

## 源码结构

| 路径 | 职责 |
| --- | --- |
| `backend/` | Rust 插件进程、宿主交互、检测调度与状态管理 |
| `frontend/` | Vue / TypeScript 插件管理页面 |
| `backend/src/lmfpd/` | lm-detector Rust SDK 评分代码（上游文件，仅调整模块路径） |
| `backend/src/engine.rs` | 进程内评分：挑战生成、样本校验、评分结果转换 |
| `vendor/lmfpd/` | 内置 lm-detector 参考库（`bundle.json`）、检测器（`shared_detector.bin`）、来源与原始许可 |
| `scripts/` | 打包、许可证收集及回归验证 |
| `plugin.json` | 插件身份、宿主兼容范围、资源及私有状态声明 |
| `docs/releases/` | 各发行版本的详细说明 |

## 环境准备

支持 Linux x86_64 和 ARM64（aarch64），两种架构共用业务代码与构建脚本。需要以下工具：

| 工具 | 要求或用途 |
| --- | --- |
| Rust | `rust-toolchain.toml` 固定为 1.99.1，含 rustfmt 和 Clippy |
| Node.js / pnpm | Node.js >=24、pnpm 9.12.2，使用前端锁文件安装依赖 |
| Python | >=3.11.4，仅用于验证脚本（标准库）；CI 使用 3.11.17 |
| Zig / cargo-zigbuild | 构建 glibc 2.36 目标的发布二进制；也可在匹配架构、glibc <=2.36 的 Linux 环境启用原生构建 |
| cpr-plugin | `scripts/install_packager.py` 自动使用 `backend/Cargo.toml` 中 SDK 的固定提交安装 |

```bash
git clone https://github.com/Victor-Tau/cpr-jzjc.git
cd cpr-jzjc
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements-dev.txt
pnpm --dir frontend install --frozen-lockfile
```

Rust 与前端依赖分别由 `backend/Cargo.lock` 和 `frontend/pnpm-lock.yaml` 固定。SDK 的确切提交见 `backend/Cargo.toml`，lm-detector 来源与内置数据摘要见 `vendor/lmfpd/SOURCE.json`。包使用 `trustedProcess`，管理页面应从 CPR 中打开，以获得宿主提供的桥接接口。

## 自动构建与发布

[GitHub Actions 工作流](../.github/workflows/build.yml) 在每次 PR、主分支更新和手动触发时，以独立原生 runner 验证 x86_64、ARM64。两个任务都在 Python 3.11.17 / Debian 12 容器内运行，避免原生链接引入高于 glibc 2.36 的依赖。

双架构实现的本地构建、架构防错与发布流程验证见 [ARM64 验证记录](verification-arm64.md)。

每个任务调用同一个 `scripts/package.sh`，检查 Rust、前端、评分及构建脚本，验证最终包的 ELF 架构、资源摘要和进程内启动评分，再执行最终包二进制的完整 RPC 回归。缓存键包含目标架构和依赖输入，两个任务使用各自工作区，不共享运行时和许可证输出。

日常功能修改无需增加 ARM64 专用操作。维护者一次性在仓库规则中将 `Build x86_64-unknown-linux-gnu` 和 `Build aarch64-unknown-linux-gnu` 设为必需检查。

发布时同步 `plugin.json`、`backend/Cargo.toml` 和 `frontend/package.json` 中的同一个版本，推送 `v<版本>` 标签。工作流检查版本一致性，等待两个构建任务通过，核对两份安装包及摘要后上传 GitHub Release。安装包名称和 manifest 的 `package.target` 区分架构，插件版本不加架构后缀。

升级 CPR SDK 时，打包工具自动跟随 SDK 提交；验证脚本从源码 manifest 读取预期兼容范围。升级 lm-detector 时，用上游 `sdk/rust/src` 中的 `analysis`、`challenge`、`codec`、`error`、`features`、`ranker` 覆盖 `backend/src/lmfpd/`（把 `crate::` 改为 `super::`），同步 `data.rs` 的解码部分，并用 `https://lm.ikale.io/data/` 的 `bundle.json` 与解压后的 `shared_detector.bin` 替换 `vendor/lmfpd/data/`，更新 `SOURCE.json`。

## 检查与测试

在仓库根目录执行：

```bash
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml --locked -- -D warnings
cargo test --manifest-path backend/Cargo.toml --locked
pnpm --dir frontend build
node scripts/api_smoke.mjs
cargo build --manifest-path backend/Cargo.toml --locked
python scripts/protocol_smoke.py -v
```

RPC 集成测试启动实际插件进程，模拟账户、计量 Key、宿主存储、模型事件和指纹库下载。覆盖请求绑定、复测、身份变化、定时范围、停用/移组重试、更新回退、取消、持久化恢复、并发与预算边界，不访问真实账户。可通过 `--binary /path/to/modeltrace-health` 验证从安装包解出的二进制。

## 单核发布构建

所有编译进程可通过 CPU 亲和性限制在单个核心，线程数同时设为 1。以下示例使用 CPU 0；若 CPU 0 不在容器或主机允许的集合中，请将 `task_cpu` 改为可用核心。

```bash
export CARGO_BUILD_JOBS=1
export RAYON_NUM_THREADS=1
export UV_THREADPOOL_SIZE=1
export OPENBLAS_NUM_THREADS=1
export OMP_NUM_THREADS=1
task_cpu=0
```

从 SDK 声明的固定提交安装官方打包工具，沿用其 workspace 锁文件，无需另行维护 CLI 的标签或提交：

```bash
taskset -c "$task_cpu" python scripts/install_packager.py
export PATH="$PWD/.cache/tools/bin:$PATH"
```

安装所需工具并激活虚拟环境后，可使用完整打包脚本：

```bash
taskset -c "$task_cpu" bash scripts/package.sh
```

默认目标为 `x86_64-unknown-linux-gnu`。在原生 Debian 12 ARM64 环境中构建完整包：

```bash
CPR_TARGET=aarch64-unknown-linux-gnu CPR_NATIVE_BUILD=1 \
  taskset -c "$task_cpu" bash scripts/package.sh
```

`CPR_NATIVE_BUILD=1` 要求宿主 Rust 目标与发布目标一致，且系统使用 glibc <=2.36。其他构建环境请安装 Zig / cargo-zigbuild，并在对应 Linux 架构运行最终包验证。

该脚本安装前端依赖、构建页面、检查 Rust、运行单元测试、收集许可、生成发布二进制，并通过官方 CLI 打包、验证安装包。最终产物在 `dist/`，包括 `.tar.gz` 与对应 `.sha256`。完整 RPC 回归由 CI 对最终安装包执行；本地也可通过 `protocol_smoke.py --binary ...` 运行。浏览器及跨版本升级验证需另外运行。

需要分别执行步骤时：

```bash
taskset -c "$task_cpu" pnpm --dir frontend build
task_target="${CPR_TARGET:-x86_64-unknown-linux-gnu}"
python scripts/collect_licenses.py
taskset -c "$task_cpu" cargo zigbuild --manifest-path backend/Cargo.toml \
  --locked --release --target "${task_target}.2.36"
cpr-plugin package --manifest plugin.json \
  --binary "backend/target/$task_target/release/modeltrace-health" \
  --target "$task_target" --resource-map web=frontend/dist --output-dir dist
task_version="$(python -c 'import json; print(json.load(open("plugin.json"))["version"])')"
python scripts/verify_artifact.py \
  "dist/personal.modeltrace-health-${task_version}-${task_target}.tar.gz" "$task_target"
```

`verify_artifact.py` 验证包和资源摘要、静态页面引用、包内不再含 Python 运行时，并运行实际插件进程验证进程内评分和完整检测流程。跨版本保留验证使用：

```bash
python scripts/upgrade_smoke.py /path/to/previous-release.tar.gz /path/to/current-release.tar.gz
```

## 浏览器验证

浏览器测试使用实际插件生成的模拟数据和生产前端，覆盖筛选、批量提交、配置、报告、指纹库、实时进度、取消、深色主题及移动布局：

```bash
python scripts/ui_fixture.py --binary /path/to/package/bin/modeltrace-health
npm install --prefix .cache/browser --no-save playwright@1.58.2
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin \
  node .cache/browser/node_modules/playwright/cli.js install chromium
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

系统需具备 Chromium 运行依赖和中文字体。截图默认输出到 `.cache/`，包括 `accounts.png`、`settings.png`、`report.png`、`history.png` 与 `mobile.png`；公开截图使用模拟数据。

`ui_smoke.mjs` 支持两个可选环境变量，便于并行构建时互不覆盖：`UI_DIST` 指定已构建页面目录（默认 `frontend/dist`），`UI_SHOTS` 指定截图目录（默认 `.cache`），相对路径以仓库根目录为基准：

```bash
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin UI_DIST=/path/to/dist UI_SHOTS=/path/to/shots \
  node scripts/ui_smoke.mjs
```

## 管理接口

插件 ID 为 `personal.modeltrace-health`，所有管理接口使用宿主管理员身份，路径相对插件实例命名空间。

| 方法 | 相对路径 | 用途 |
| --- | --- | --- |
| GET | `api/bootstrap` / `api/accounts` / `api/models?keyId=...` | 配置、账户与可见模型目录 |
| POST | `api/config/preview` | `{config}` 校验及补齐默认值，只预览 |
| POST | `api/config` | `{config, revision}` 保存配置，拒绝覆盖冲突版本 |
| POST | `api/submit` | `{accountIds, requestId}` 幂等提交 |
| GET | `api/jobs` / `api/report?id=...` | 任务列表及报告 |
| GET | `api/progress?since=...` | 进度快照；未变化时返回 `changed=false,jobs=null` |
| POST | `api/cancel` | 可选 `{id, source}`；`source` 为 `manual`、`schedule` 或 `error_code`，均省略时取消全部队列 |
| POST | `api/delete` | `{id}` 或 `{ids}` 删除历史；批量返回 `deleted`、`skipped` |
| POST | `api/retry-disable` / `api/retry-group` | `{id}` 重试关闭调度或移组 |
| POST | `api/banks/check` / `api/banks/apply` / `api/banks/rollback` / `api/banks/builtin` | 检查、应用、回退或恢复内置库 |

1.14.0 起，`api/bootstrap`、`api/jobs` 与 `api/progress` 附带 `errorTrigger` 请求日志触发状态（最近读取、最近命中、累计触发次数与读取错误）。

1.10.0 起，大列表响应附带 `pagination: {field, nextOffset, snapshot}`，单页预算为 768 KiB。后续请求保留原查询参数，并附加 `pageOffset` 与 `pageSnapshot`；`nextOffset=null` 表示结束。快照变化返回 HTTP 409 和 `error.code=pagination_changed`，客户端应放弃部分数据并从头读取。`frontend/src/api.ts` 已实现完整合并和有限重读，写操作不会因分页自动重试。

## 提交贡献

修复或功能变更请说明触发场景、预期行为与验证结果。涉及检测、状态或协议时，补充对应 RPC 场景；涉及页面时，提供模拟数据下的交互证据或截图。文档中的功能、兼容范围和默认值应与实际实现一致。

发布时同步插件、Rust 和前端版本，更新宿主范围、SDK 提交、CHANGELOG 与发行说明，并完成最终安装包验证。构建结果和验证边界记录在 `docs/verification.md`，不将缓存、凭据或真实账户数据提交到仓库。
