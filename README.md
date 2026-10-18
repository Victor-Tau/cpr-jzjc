# 账号降智检测 · Codex Proxy RS 插件

使用 [ModelTrace](https://github.com/xqy2006/ModelTrace) 的三轮数字指纹挑战检测账户实际输出更接近哪个候选模型，支持自动复测、定时检测、确认后关闭账户调度，以及在线更新指纹库。

## 安装

适配 **Codex Proxy RS >=3.29.1、<3.30.0**，发布包适用于 Linux x86_64、glibc 2.36+。运行模式为 `trustedProcess`。安装包包含私有 Python/NumPy 评分运行时，无需安装系统 Python 或单独运行 ModelTrace 服务。

在宿主「插件管理 → 安装插件 → 上传包」上传：

`dist/personal.modeltrace-health-1.3.0-x86_64-unknown-linux-gnu.tar.gz`

启用后打开「账号降智检测」，进入「参数配置」选择已有计量 Key，保存后即可单号或批量检测。检测请求由宿主按指定账户 ID 执行，使用该账户绑定的代理并计入所选 Key。Key 的账户分组和模型权限仍由宿主校验。页面仅接收 Key ID 和名称。

运行时首次检测会解出约91MB文件，目标目录需可写、可执行并有足够空间。自动跳过 `noexec` 临时目录，解包失败时清理残留并尝试 `/var/tmp`，兼容 Docker 中仅64MB且禁止执行的 `/tmp`。可用宿主进程的 `MODELTRACE_RUNTIME_DIR` 指定首选目录，或用 `TMPDIR` 修改系统临时目录。解包不恢复不必要的文件时间戳；异常会显示具体条目和系统错误。

### 1.3 更新

- 一次后台维护连续完成准备、请求、校验与评分，预算充足时一轮只需一次回调，减少原先步骤间约90秒的调度等待。
- 按本次宿主调用的剩余预算限制请求，预留校验和状态保存时间。慢请求记录为超时并按规则重试，避免常见的「上一次探针执行被中断」；真正的重启或不确定请求仍保留中断保护。
- Python/NumPy 评分进程常驻复用，缓存内置库元数据和已解析的指纹库；运行时解包移到阻塞工作线程，仅在首次使用时执行。
- 进度与报告读取使用独立快照，请求进行中仍可刷新及取消；显示准备、等待响应、接收字节数、校验、评分和复测倒计时。
- 活跃任务每秒刷新，空闲每10秒刷新；隐藏页面暂停，恢复时立即刷新。使用版本号省略未变化的任务列表，并自动更新打开的报告。
- 等待复测的账户不再阻塞其他已就绪账户。

### 1.2 更新

- 修复账户前复选框被单元格截断的问题，选择列在横向滚动时固定显示。
- 账户状态增加颜色和圆点：正常为绿色、异常为红色、额度耗尽和限流为橙色、已停用为灰色。
- 参数配置页只保留标题区的一个「保存全部参数」按钮。
- 检测、重试和复测始终绑定任务对应的账户 ID，沿用宿主中该账户配置的代理；手动和定时共用执行路径。代理由宿主管理，无需在插件中重复配置。

## 使用

- **账户检测**：列出全部账户；按订阅、状态和分组组合筛选，搜索名称、邮箱或账户ID。支持10/20/50/100条分页、勾选当前页、跨页批量检测。首版检测OpenAI账户；其他提供方会显示不支持。
- **参数配置**：一套全局配置，手动和定时共用。请求模型支持目录选择与手动输入；支持计量Key、temperature、推理强度、输出上限、超时、降级名单、别名和阈值配置。宿主不接受的请求参数会终止本次任务并显示错误类别。
- **检测历史**：查看每轮全部候选概率、家族概率、触发原因、请求ID、配置快照和指纹库版本。删除历史不会恢复已停用账户。

默认配置：

| 参数 | 默认值 |
| --- | --- |
| 请求模型 | `gpt-6.1-sol` |
| 降级模型 | `gpt-5.6-luna` |
| 低置信度阈值 | 60% |
| 降智阈值 | 80% |
| 连续确认轮数 | 3 |
| 低置信度额外复测 | 最多2轮 |
| 重试与复测间隔 | 30秒 |
| 推理强度 / 输出上限 | low / 4096 tokens |
| 请求超时 | 默认120秒，无固定上限，允许非负整数秒；0表示不设置插件超时 |
| 定时检测 / 自动停用 / 自动检查更新 | 默认关闭 |
| 历史保留 | 30天，可配置1至365天，最多500条并受存储预算限制 |

### 判定与请求预算

每轮只发送一次检测请求，获得一个有效回答后评分；失败时最多重试到五次采样尝试。无效回答、限流与超时消耗尝试预算；认证、权限或参数错误立即终止。失败或证据不足均不会关闭调度。

1. **低置信度**：最高候选概率严格小于60%，即所有候选均低于阈值，自动重新检测。
2. **疑似降智**：请求模型（应用别名后）不在降级名单内，最高候选在降级名单内且概率大于等于80%。默认连续三轮才确认；任何非疑似结果中断连续计数。
3. **其他结果**：高置信度且不命中降级名单为「未命中降智规则」；其余为「证据不足」。

例如请求 `gpt-6.1-sol`，连续三轮均以至少80%概率识别为 `gpt-5.6-luna`，才确认降智。直接请求luna，或别名指向luna时，命中luna不会触发降智。

每个任务总轮数上限为「连续确认轮数 + 低置信度额外复测预算」，默认最多5轮、25次请求（含失败重试）；多数任务一轮结束。低置信度复测还单独受其预算约束。确认降智后仅在开启自动停用时将宿主账户 `enabled=false`。停用失败可在报告中重试，不重复采样；管理员负责恢复。

后台按账户任务串行执行。宿主每30秒调用一次维护回调，1.3 在一次回调内连续完成一轮；剩余预算不足时保留已完成的安全步骤，下次继续。首次开始、重试和复测仍受宿主调度间隔影响。关闭页面不影响执行；取消会通知执行器结束等待并关闭进行中的模型流，不再等完整请求超时。

请求超时默认120秒，0表示不设置请求超时。执行器始终取配置超时与宿主剩余预算中的较短值：维护调用预留1.5秒，发起请求前另留6秒用于关闭流、样本校验及保存；评分预算不足时延至下次回调。RS 3.29.1 的后台维护回调总预算仍为30秒，较长模型请求需要宿主提供更长的调用预算，修改插件配置不能突破该限制。升级保留原配置；已提交任务继续使用原配置快照。真正的进程重启或存储失败导致请求结果不确定时，仍标记中断，需新建检测以避免重复请求。

### 自动定时

支持固定间隔（分钟）和指定IANA时区的每日时刻。每日时刻遇到夏令时缺失分钟时，在之后首个有效分钟执行；重复时刻当日只运行一次。

范围可保存订阅、状态、分组和关键词筛选条件，运行时重新匹配以纳入新账户；也可保存固定账户ID。定时只检测启用的OpenAI账户，跳过已排队账户。错过的时间点合并一次，不补跑全部历史时间点。每次最多提交500个账户，超出会在配置页显示错误，需缩小范围。

### 指纹库在线更新

在「参数配置 → ModelTrace指纹库」点击「检查更新」，随后「下载并应用」。也可开启后台定时检查，默认间隔24小时；应用始终由管理员点击。

来源固定为 `xqy2006/ModelTrace` 默认分支的 `data/unified_bank.json`。先解析提交SHA，再按该提交下载，显示当前提交、SHA-256、建库时间、完整模型列表，以及新库新增和移除的模型。可设置HTTPS镜像前缀，地址拼接为 `镜像前缀/完整GitHub地址`。

下载限20MiB，压缩后存储限3MiB；指纹库检查仍受宿主维护调用预算约束。下载后校验schema、指纹方法、模型唯一性与顺序、矩阵维度、有限数值、校准参数，并离线跑通1/2/3样本评分。只有全部通过才切换版本，库不兼容时提示升级插件。在线更新仅加载JSON数据；算法与Python运行时随插件更新。

提供「回退上一版本」「恢复内置库」。本地保留内置库、当前库、上一库及活动任务引用的旧库。更新、回退不改变已开始任务的库；整个任务及复测均固定同一库。旧报告保留当时的概率，不重新计算。新库移除正在配置使用的降级模型或别名目标时，阻止应用并提示冲突。下载失败或保存中断保留当前库，下次载入清理暂存数据。所有持久库分块存入宿主私有状态，重启及同实例升级保留。

ModelTrace概率是当前候选库内、均匀先验下的相对概率；库外模型仍会归入现有候选。「降智」是用户配置规则的检测标签，不是智商测量或模型身份认证。

## 开发与验证

需要 Rust 1.97、Node >=24、pnpm、Python >=3.11、crane（下载固定运行时基础镜像）、cargo-zigbuild 和与RS同版的 `cpr-plugin`。Python开发依赖见 `requirements-dev.txt`。Rust与前端依赖分别通过锁文件固定。

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
cargo test --manifest-path backend/Cargo.toml --locked
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
.venv/bin/python -m unittest discover -s engine/tests -v
cargo build --manifest-path backend/Cargo.toml --locked
.venv/bin/python scripts/protocol_smoke.py -v
.venv/bin/python scripts/build_runtime.py
cargo zigbuild --manifest-path backend/Cargo.toml --locked --release --target x86_64-unknown-linux-gnu.2.36
cpr-plugin package --manifest plugin.json --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.3.0-x86_64-unknown-linux-gnu.tar.gz
```

基础镜像已缓存时可向 `build_runtime.py` 传 `--base-tar /path/python-base.tar`。脚本验证固定镜像配置、各镜像层和NumPy wheel哈希，不依赖Docker守护进程。`scripts/package.sh` 可串联构建和校验，需在已激活的开发环境运行。

未修改 Python/NumPy 依赖时，也可用 `.venv/bin/python scripts/refresh_runtime.py /path/to/previous-release.tar.gz` 复用已验证发布包中的固定运行时，并刷新评分与上游源码。必须同时提供原包的 `.sha256` 文件；脚本校验包与全部资源摘要。依赖变更时使用完整构建脚本。

RPC集成测试启动实际插件二进制，模拟账户、计量Key、宿主状态、模型事件与GitHub下载，覆盖绑定、复测预算、身份变化、重启、定时范围、连续确认与停用重试、在线更新及中断恢复，并验证执行期间读取进度、取消、流进度、调度公平性和宿主预算超时，不访问真实账户。

安装了 `strace` 时可用以下命令模拟禁止时间戳系统调用，验证发布二进制在该限制下仍可解包并完成一轮评分（路径替换为解出的安装包）：

```bash
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/package/bin/modeltrace-health --deny-timestamps Integration.test_binding_idempotency_and_completed_scoring -v
```

Linux 上可加 `--runtime-tmpdir /dev/shm` 验证小容量、`noexec` 临时挂载下的自动回退（需确认本机该挂载选项）。

具备私有挂载命名空间权限的 Linux 环境可运行 `bash scripts/restricted_runtime_smoke.sh /path/to/package/bin/modeltrace-health`。该脚本在独立命名空间创建64MB临时挂载，依次验证 `noexec` 回退、时间戳调用被拒绝、空间耗尽后的清理与回退；不会修改宿主挂载。

`verify_artifact.py` 除离线运行 Python 评分外，也通过实际发布二进制验证 Rust 解包和完整检测流程。

浏览器交互测试使用同一模拟宿主生成数据，覆盖搜索、订阅筛选、批量提交、配置保存、模型目录、指纹库更新回退、报告和移动端深色主题，同时验证进度与报告自动刷新、隐藏暂停及恢复：

```bash
.venv/bin/python scripts/ui_fixture.py
npm install --prefix .cache/browser --no-save playwright@1.58.2
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node .cache/browser/node_modules/playwright/cli.js install chromium
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

系统需具备 Chromium 运行依赖和中文字体。截图输出到 `.cache/accounts.png`、`.cache/settings.png`、`.cache/report.png` 与 `.cache/mobile.png`。`ui_fixture.py` 和 `protocol_smoke.py` 均可使用 `--binary` 指定从安装包解出的发布二进制。

本版交付包的测试结果与界面截图见 [验证记录](docs/verification.md)。

## 接口

插件ID：`personal.modeltrace-health`。所有管理接口使用宿主管理员身份。

| 方法 | 插件相对路径 | 用途 |
| --- | --- | --- |
| GET | `api/bootstrap` / `api/accounts` / `api/models?keyId=...` | 配置、账户与可见模型目录 |
| POST | `api/config` | `{config, revision}` 保存配置，版本冲突拒绝覆盖 |
| POST | `api/submit` | `{accountIds, requestId}` 幂等提交 |
| GET | `api/jobs` / `api/report?id=...` | 任务列表与报告 |
| GET | `api/progress?since=...` | 轻量进度快照；版本未变化时返回 `changed=false,jobs=null` |
| POST | `api/cancel` / `api/delete` / `api/retry-disable` | `{id}` 取消、删除历史、重试关闭调度；取消省略id时停止全部队列 |
| POST | `api/banks/check` / `apply` / `rollback` / `builtin` | 检查、应用、回退与恢复内置库 |

许可证为MIT。第三方原始声明见 `legal/NOTICE.md`、`legal/THIRD_PARTY.txt` 与 `vendor/modeltrace/LICENSE`。
