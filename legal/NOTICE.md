# 第三方组件

本插件使用 lm-detector 检测方法与参考数据；没有集成 Meow 检测器。

- lm-detector： https://github.com/Ikaleio/lm-detector ，MIT。评分代码来自其 Rust SDK（`sdk/rust`，lmfpd 0.2.0），仅调整模块路径；固定提交与内置数据摘要见 `vendor/lmfpd/SOURCE.json`，原许可证见 `vendor/lmfpd/LICENSE`。在线更新从 https://lm.ikale.io/data/ 或其 npm 镜像下载参考库与检测器。
- ModelTrace： https://github.com/xqy2006/ModelTrace ，MIT。lm-detector 致谢其算法思路与部分原始数据。
- Codex Proxy RS SDK： https://github.com/zyycn/codex-proxy-rs ，Apache-2.0。固定提交见 `backend/Cargo.toml`。
- @codex-proxy/ui： https://github.com/zyycn/codex-proxy-ui ，使用 0.6.0 发布包。
- Vue： https://github.com/vuejs/core ，MIT。
- Lucide： https://github.com/lucide-icons/lucide ，ISC。
- Rust 依赖（含 regex、ruzstd）及前端依赖：原始许可证与声明汇总在 `THIRD_PARTY.txt`。
- UI 构建配置参考 rs-reauth-manager，MIT，Copyright (c) 2026 Lemon-57。原许可证附于下文。

请求接口用法参考 https://github.com/R-lmmortal/codex-model-health-plugin ，未复制该项目代码或非商业许可检测器。

MIT License

Copyright (c) 2026 Lemon-57

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
