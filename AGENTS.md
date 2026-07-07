# Agent 指南

hiptty 是 4d4y 论坛的 Terminal 客户端。本文档是 Agent 的**项目入口**：有哪些文档、crate 怎么定位、怎么协作。

行为准则（称呼、E2E 测试标准、工程质量）见 [`CLAUDE.md`](CLAUDE.md)。

---

## 1. 文档地图

| 文档 | 用途 |
|------|------|
| [`README.md`](README.md) | 构建、运行、测试命令 |
| [`docs/architecture.md`](docs/architecture.md) | Crate 分层、TUI 数据流、配置路径 |
| [`docs/tui-implementation-decisions.md`](docs/tui-implementation-decisions.md) | 历史决策、技术备注、待补充项（可能滞后于代码） |
| [`docs/api.md`](docs/api.md) | CLI 命令与 JSON schema |
| [`docs/README.md`](docs/README.md) | 文档索引 |
| [`docs/archive/`](docs/archive/) | 初版设计稿 / 生态调研，只读参考 |

**信息从哪来**

各方都可能漏看、记错或过时——代码、文档、我当时的口头说明、你自己的回忆，也包括你读到的上下文。

- **代码**：反映当前实际做了什么，查实现时从这里入手。
- **文档**：跨 session 的背景与备忘，可能滞后。`tui-implementation-decisions.md` 里的「偏离」是相对已归档设计稿而言的；没读过 `archive/tui-design.md` 时，不必套这个 framing。
- **session 里的描述**：当下意图的一种表达，同样可能不完整或有误，不能默认永远正确。

按需查阅：定位模块看 `architecture.md`；了解 CLI 看 `api.md`；需要历史背景时再翻 `tui-implementation-decisions.md`。

有实质性变更时，酌情更新相关文档，并在该文档修订历史中记一笔。

---

## 2. Crate 速查

| Crate | 改什么时来这里 |
|-------|----------------|
| `hiptty-core` | 领域类型、错误码、数据结构 |
| `hiptty-adapter` | HTTP、HTML 解析、登录、发帖 API |
| `hiptty-render` | 主题色、正文渲染、换行 |
| `hiptty-image` | 终端图像协议、缓存、Sixel 边距 |
| `hiptty-widgets` | 列表、楼层、滚动条、弹层等 UI 组件 |
| `hiptty-app` | 状态机、事件循环、worker、鼠标、命令栏 |
| `hiptty` | TUI 二进制入口 |
| `hiptty-cli` | Headless CLI 入口 |

依赖方向：`hiptty` / `hiptty-cli` → `hiptty-app`（仅 TUI）→ `widgets` / `image` / `render` → `adapter` → `core`。

---

## 3. 常用命令

```bash
cargo build --release
cargo run -p hiptty
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

详见 [`README.md`](README.md)。

---

## 4. 测试要求

- Bug 修复：先按 [`CLAUDE.md`](CLAUDE.md) 要求，在接近真实用户场景的 E2E 条件下复现。
- 解析逻辑：`hiptty-adapter` 有 HTML fixture 测试（`tests/fixtures/`）。
- UI 变更：关注像素级表现。

---

## 5. 遇到分歧时

发现以下情况，**停下来说明，等我决定**，不要自行择一后继续：

- 文档与代码对不上
- 我的描述与代码/文档明显矛盾
- 按我的描述做，会走向与现有实现完全不同的方向

简要列出各方差异（代码怎么说、文档怎么说、我（或你）刚才说了什么），加上你的理解，等我拍板。

长期大方向是细节精修、bug 修复、性能优化；具体做什么、先做什么，需要在 session 里对齐。`tui-implementation-decisions.md` §5 等只是备忘，不能代替当下的确认。