# hiptty → Rust + Ratatui 生态库调研

**文档角色**：技术栈迁移至 Rust + Ratatui + ratatui-image 时的 crate 选型参考。以 `forum-tui-prd.md` 为能力清单，按 UI 壳 / 正文渲染 / 图像 / Adapter / 平台 / 测试六层映射外部库，标明可复用与须自研边界。

**调研日期**：2026-06-26

**调研方法**：`smart-search` 对官方文档、crates.io、GitHub README 做定向抓取（`fetch` / `exa-search` / `context7-docs`），关键结论附来源链接。

---

## 1. 核心栈（已定 + 必要配套）

| 层 | Crate | 版本线索 | PRD 对应 | 事实依据 |
|----|-------|----------|----------|----------|
| TUI 核心 | **ratatui** + **crossterm** | ratatui 0.30+ | 全部屏幕、布局、主题 | 官方标准组合；内置 `Table` / `List` / `Paragraph` / `Block` |
| 终端图像 | **ratatui-image** | 11.0.6 | §10 图片/表情、终端能力探测 | 统一 Kitty / iTerm2 / Sixel；`Picker::from_query_stdio()` 自动探测协议与 cell 像素尺寸 |
| 图像解码/缩放 | **image** | 0.25+ | 上传压缩 §9.5、表情 PNG 加载 | `ImageReader` / `imageops::resize` / `save`；ratatui-image 直接依赖 |
| 异步运行时 | **tokio** | — | Adapter 网络、非阻塞 UI | [ratatui async recipe](https://ratatui.rs/recipes/apps/terminal-and-event-handler/)；`ratatui-image` 有 `examples/tokio.rs` |
| HTTP | **reqwest** | 0.13+ | §9.2 网络层 | `cookie_store` feature；`multipart` 上传；`blocking` 可选 |
| Cookie 持久化 | **reqwest_cookie_store** | 0.10+ | `session.json` §9.12 | JSON 序列化 + `cookie_provider()` |
| HTML 解析 | **scraper** | 0.27+ | §9.1 Discuz HTML | Servo `html5ever` + CSS selector，语义上最接近 hipda/Jsoup |
| 字符编码 | **encoding_rs** | 0.8+ | GBK → UTF-8 §9.2 | 提供 `GBK` 静态编码 |
| 序列化/配置 | **serde** + **serde_json** | — | `config.json` / `profiles.json` | 标准方案 |
| 配置路径 | **directories** | 6.0+ | `~/.config/hiptty/` §3.2 | 跨平台 XDG/CSIDL 路径 |
| 错误类型 | **thiserror** (+ **anyhow** 在边界) | — | §9.3 AdapterError | 与 PRD 错误模型对齐 |

---

## 2. 按 PRD 能力映射的可复用库

### 2.1 UI 壳（≈70% 工作量）

PRD 要求：8 Screen + 10 Overlay、分屏键位、三段式 shell、鼠标增强、toast/command bar。

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| 弹层/模态 | **tui-popup**（[ratatui/tui-widgets](https://github.com/ratatui/tui-widgets)） | 居中 overlay；支持 `PopupState` 拖拽 |
| 滚动容器 | **tui-scrollview** | 大内容区滚动；官方维护，有 mouse 示例 |
| 滚动条 | **tui-scrollbar** | 与 scrollview 配套 |
| 焦点 + 鼠标 + Toast | **ratatui-interact** | `FocusManager`、`Toast`/`ToastStack`、`PopupDialog`、`ListPicker`、`ScrollableContent`（含 View/Copy 模式供终端拖选） |
| 分屏键位（≈ ghui） | **ratatui-which-key** | Scope / Action / Category 模型，与 PRD 的 Global/List/Detail/Editor/Overlay 高度同构 |
| 登录/命令输入 | **tui-prompts** 或 **ratatui-interact::Input** | `:` 命令模式、登录三字段 |
| 编辑器/快捷回复 | **tui-textarea** 或 **ratatui-interact::TextArea** | 多行、`Ctrl+Enter`、滚动；tui-textarea 更成熟 |
| 帖子列表表格 | **ratatui::widgets::Table**（内置） | PRD `ThreadTable` 列布局直接用 |
| 导航状态机 | **自写**（参考现有 `navigation.ts`） | `ratatui_router` / `tui-overlay` 社区用量小；PRD 栈+overlay 状态机已定型 |
| 事件循环骨架 | **ratatui 官方 Tui recipe** 或 **crates-tui** 源码 | tokio `EventStream` + tick/render 分离 |
| 快捷键帮助 | **ratatui-which-key** 弹层 | 比自写 `HelpOverlay` 更一致 |

**备选但建议审慎：**

| Crate | 原因 |
|-------|------|
| **ratatui-kit** | React 式组件框架，stars 少，与现有 UI 设计冲突 |
| **rat-salsa / rat-scrolled / rat-widget** | 另一套 widget 体系，与 tui-widgets 功能重叠 |
| **ratatui-markdown** | PRD 不做完整 CommonMark；论坛正文是 `ContentNode`，用 ratatui 原生 `Text`/`Span`/`Line` 更合适 |

### 2.2 正文与长帖（≈20%）

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| 富文本 inline style | **ratatui::text**（内置） | PRD `ContentNode` → `Span` 带 `Style::fg()` / bold / italic |
| 引用块/楼层块 | **自写 Widget** + **tui-scrollview** | 参考现有 `FloorBlock` / `QuoteBlock` |
| 长帖可变高度滚动 | **ratatui_widget_scrolling** | 为「每行高度动态」设计；只渲染可见区；比 tui-scrollview 更贴楼层流 |
| CJK 列宽 | **unicode-width**（ratatui 已集成） | 列表列宽、截断 |
| 文本换行 | **ratatui::widgets::Paragraph** + wrap | 内置 word wrap；长帖性能需配合虚拟滚动 |
| 页内搜索 `/` | **自写**（迁移 `inPageSearch.ts`） | 纯内存搜索，无现成 crate |
| 链接点击开浏览器 | **opener** | 跨平台 `open::that(url)` |

### 2.3 图像与表情（§10）

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| 协议探测 + 渲染 | **ratatui-image** | 不要帧后手写 Kitty escape；库内处理「跳过覆盖图像区域」 |
| 启动探测持久化 | **ratatui-image::Picker** + **serde** | `from_query_stdio()` 在 alternate screen 后调用 |
| 异步加载/缩放 | **ratatui-image::ThreadProtocol** | 后台线程 `resize_encode`，UI 不阻塞 |
| 降级链 | ratatui-image 内置 | Kitty/iTerm2/Sixel → **halfblocks**；对齐 PRD 三级策略 |
| 表情 PNG 资源 | **rust-embed** 或 **include_dir** | 61 个表情本地 PNG；编译期嵌入 |
| 附件图上传压缩 | **image** + **imageops** | >8MB 缩 JPEG quality 80；对齐 hipda `UploadImgHelper` |
| 临时缓存目录 | **tempfile** | `os.tmpdir()/hiptty-images/` 退出清理 |
| 打开本机查看器 | **opener** | `[Image #N]` 点击 / `:i#N` |

#### ratatui-image 关键外部事实

来源：[ratatui/ratatui-image README](https://github.com/ratatui/ratatui-image)

| 终端 | 协议 | OK | QA | 备注 |
|------|------|----|----|------|
| Ghostty | Kitty | ✔️ | ✔️ | 实现 Kitty + unicode placeholders |
| Kitty | Kitty | ✔️ | ✔️ | 协议参考实现 |
| iTerm2 | iTerm2 | ✔️ | — | Mac only |
| Wezterm | iTerm2 | ✔️ | ❌ | Sixel/Kitty 有 bug，仅 iTerm2 稳定 |
| Xterm / Foot / mlterm | Sixel | ✔️ | ✔️ | — |
| Alacritty / Konsole / Warp | — | ❌ | — | 不支持或实现不完整 |

- **halfblocks** 降级可在所有终端工作（即使无法探测字体像素尺寸）。
- 已知问题：Sixel 在末行可能触发滚动 ([#57](https://github.com/ratatui/ratatui-image/issues/57))——楼层布局需留底边距。
- 截图测试套件：[ratatui-image-screenshots](https://benjajaja.github.io/ratatui-image-screenshots/)

#### Picker 使用要点

来源：[docs.rs/ratatui-image](https://docs.rs/ratatui-image/latest/ratatui_image/)

1. `Picker::from_query_stdio()` 必须在进入 alternate screen 之后、读取终端事件之前调用。
2. 探测失败会优雅降级到 halfblocks，多数情况不报错。
3. `ThreadProtocol` + `examples/thread.rs` 适合楼层内多图动态缩放且不阻塞 UI。

### 2.4 Adapter（≈10%，无业务轮子可抄）

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| HTTP + 超时 | **reqwest** | 对齐现有 CONNECT/READ/WRITE 超时 |
| Cookie 会话 | **reqwest** + **reqwest_cookie_store** | 持久化 `session.json` |
| HTML 解析 | **scraper** | CSS selector 对照 hipda `HiParser*` |
| GBK 解码 | **encoding_rs::GBK** | 双重判断 Content-Type + meta charset |
| GBK 表单编码 | **自写薄函数** | `encoding_rs` 编码 + 手动 `%XX`；无成熟 crate |
| 密码 MD5 | **md-5** 或 **md5** | 登录 POST |
| 表单 POST | **reqwest::multipart** + url form | 发帖/回复/上传 |
| URL 参数提取 | **regex** | 迁移 `extractParam` |
| 列表缓存 | **moka**（可选） | PRD `Map<fid, CacheEntry>` |
| 发帖间隔节流 | **自写** | 30s/15s 规则简单 |

#### 重要缺口

- **没有** Discuz / 4d4y 的 Rust adapter 或 scraper 可参考。
- hipda 选择器与业务流程仍是 **唯一 ground truth**（`refs/hipda/`）。
- `emoji-map.ts` 61 条映射表可直接移植为 Rust 常量。

#### reqwest Cookie 持久化示例

来源：[docs.rs/reqwest_cookie_store](https://docs.rs/reqwest_cookie_store/latest/reqwest_cookie_store/)

```rust
let cookie_store = reqwest_cookie_store::CookieStoreMutex::new(cookie_store);
let client = reqwest::Client::builder()
    .cookie_provider(Arc::clone(&cookie_store))
    .build()?;
// 序列化: cookie_store::serde::json::save/load → session.json
```

#### encoding_rs GBK

来源：[docs.rs/encoding_rs::GBK](https://docs.rs/encoding_rs/latest/encoding_rs/static.GBK.html)

- 提供 `GBK` 静态编码；decoder 与 gb18030 相同。
- 覆盖 4d4y 常见页面编码场景。

### 2.5 平台与工程质量

| 需求 | Crate | 说明 |
|------|-------|------|
| 跨平台打开文件/URL | **opener** | Win/macOS/Linux |
| 剪贴板（拖选复制增强） | **arboard** | PRD §7.5 鼠标拖选 |
| CLI 入口 | **clap** | `--config` / `--mock` 等 |
| 日志 | **tracing** + **tracing-subscriber** | adapter 调试 |
| 崩溃报告 | **color-eyre** | ratatui 生态惯例 |
| 终端挂起恢复 | **signal-hook** | 官方 Tui recipe 含 `suspend()` |
| 快照测试 | **insta** + **cargo-insta** | [ratatui testing recipe](https://ratatui.rs/recipes/testing/snapshots/) |
| UI 回归 | **ratatui::backend::TestBackend** | 对齐现有测试思路 |
| 发布 | **cross** / **cargo-dist** | 跨平台二进制 |

---

## 3. 推荐 Cargo 清单

### Tier 1 — 直接采用

```
ratatui, crossterm, ratatui-image, image, tokio, reqwest, reqwest_cookie_store,
scraper, encoding_rs, serde, serde_json, directories, thiserror, opener,
tui-popup, tui-scrollview, tui-scrollbar, tui-textarea, ratatui-interact,
ratatui-which-key, ratatui_widget_scrolling, rust-embed, md-5, regex, tempfile
```

### Tier 2 — 按需引入

| Crate | 场景 |
|-------|------|
| **moka** | 列表缓存 |
| **tui-prompts** | interact 的 Input 不够用 |
| **confy** | 不想手写 config load/save（directories + serde_json 也够） |
| **arboard** | 剪贴板 |
| **insta** | 快照测试 |
| **tracing** | 日志 |
| **clap** | CLI |

### Tier 3 — 不建议

| Crate | 原因 |
|-------|------|
| **ratatui-markdown** / **tui-markdown** | PRD 不做完整 Markdown |
| **ratatui-kit** | 不成熟，与现有 UI 设计冲突 |
| **kuchiki** / **lol_html** | scraper 已够 |
| **Cursive 系** | 与 Ratatui 栈不一致 |
| **自写 Kitty 协议** | ratatui-image 已解决且 Ghostty QA 通过 |

---

## 4. 架构参考项目

| 项目 | 可借鉴点 | 与 hiptty 相关度 |
|------|----------|------------------|
| [ratatui/crates-tui](https://github.com/ratatui/crates-tui) | `tui.rs` + `events.rs` 事件循环 | 高 |
| [ratatui/tui-widgets](https://github.com/ratatui/tui-widgets) | popup/scrollview/prompts 官方套件 | 高 |
| [ratatui/ratatui-image](https://github.com/ratatui/ratatui-image) | Ghostty 截图测试、`thread.rs` 异步图 | 高 |
| [JonathanBerhe/gh-tui](https://github.com/JonathanBerhe/gh-tui) | MVU 六 crate 分层、图像/表情规划 | 中高（≈ ghui） |
| [Brainwires/ratatui-interact](https://github.com/Brainwires/ratatui-interact) | 焦点/鼠标/toast/ScrollableContent | 高 |
| [jayson-lennon/ratatui-which-key](https://github.com/jayson-lennon/ratatui-which-key) | 分 scope 键位 | 高 |
| [teufelchen1/jelly](https://github.com/teufelchen1/jelly) | `ratatui_widget_scrolling` 来源 | 中（长帖） |
| [aome510/hackernews-TUI](https://github.com/aome510/hackernews-TUI) | 论坛阅读产品形态 | 低（Cursive 栈） |

### gh-tui 架构要点（MVU 参考）

来源：[JonathanBerhe/gh-tui README](https://github.com/JonathanBerhe/gh-tui)

```
gh-core    → State, Msg, Cmd（纯 reducer）
gh-input   → 输入
gh-api     → 网络
gh-render  → 渲染
gh-ui      → ratatui 组合
gh-tui     → 入口
```

- Model-View-Update：副作用作为 Cmd 派发到 async worker，经 `mpsc` 回传 Msg。
- 渲染循环不阻塞 I/O。

---

## 5. 仍须自研的部分

| # | 模块 | 说明 |
|---|------|------|
| 1 | Discuz adapter 全套 | 18 个 API 函数；只有 hipda 可参考 |
| 2 | 导航状态机 | 页面栈 + overlay；迁移 `navigation.ts` |
| 3 | ContentNode 渲染器 | 论坛 HTML 子集 → `ratatui::Text` |
| 4 | GBK urlencode | ~30 行；迁移 `parser/utils.ts` |
| 5 | emoji-map 61 条 | 数据迁移 |
| 6 | detailScroll | 楼层焦点跟随；参考 ghui |
| 7 | 命令解析 | `:i#1` `:r#35` 等；迁移 `commandParse.ts` |
| 8 | 版块配置语义 | 五角星/Defaults/更多版块 |

---

## 6. 风险矩阵

| 维度 | Rust 生态覆盖 | 残留风险 |
|------|---------------|----------|
| 图像/表情 | **强**（ratatui-image + Ghostty QA） | 楼层多图性能 → ThreadProtocol；Sixel 末行滚动 |
| 键位/overlay | **强**（which-key + interact + tui-popup） | 命令模式与 which-key 需统一 Action 枚举 |
| 长帖滚动 | **中强**（widget_scrolling） | 可变高度 + 内嵌图需实测组合 |
| 富文本 | **中**（原生 Text/Span） | `<font color>` 映射自维护 |
| Adapter | **中**（reqwest+scraper+encoding_rs） | 无参考实现；GBK 边界靠 hipda 对照 |
| 鼠标/拖选 | **中**（interact View/Copy） | 与图像区域 hit-test 要协调 |
| 测试 | **强**（insta + TestBackend） | 图像快照需真终端或协议 mock |

---

## 7. 建议 Workspace 切分

```
hiptty-core/      # 类型、导航状态机、ContentNode、emoji-map、错误模型
hiptty-adapter/   # reqwest + scraper + encoding_rs（无 ratatui 依赖）
hiptty-render/    # FloorBlock、ThreadTable、主题色
hiptty-widgets/   # 对 tui-popup/scrollview/interact 的薄封装
hiptty-app/       # App 状态、键位、MVU reducer
hiptty/           # main、配置、启动探测 Picker
```

- adapter 可单测（对齐现有 `parser.test.ts`）。
- UI 可 insta 快照。
- 图像层独立演进。

---

## 8. 可复现的调研命令

```bash
smart-search fetch "https://docs.rs/ratatui-image/latest/ratatui_image/"
smart-search fetch "https://github.com/ratatui/ratatui-image"
smart-search fetch "https://ratatui.rs/recipes/apps/terminal-and-event-handler/"
smart-search fetch "https://github.com/Brainwires/ratatui-interact"
smart-search exa-search "ratatui ecosystem widgets tui-textarea scroll table modal"
smart-search exa-search "Rust scraper HTML parsing reqwest cookie GBK encoding_rs"
smart-search context7-docs "/ratatui/ratatui-image" "backends kitty sixel"
```

证据文件目录：`/tmp/smart-search-evidence/hiptty-rust-libs/`

---

## 9. 结论

Rust 生态对 hiptty 的 **图像层**（ratatui-image）、**壳子组件**（tui-widgets + ratatui-interact + ratatui-which-key）、**Adapter 基础设施**（reqwest + scraper + encoding_rs）覆盖充分。

真正没有轮子的是 **4d4y Discuz 业务 adapter** 和 **论坛富文本渲染**——这两块应从现有 TypeScript 实现与 hipda 迁移，而非另找库。

---

## 相关文档

- [forum-tui-prd.md](./forum-tui-prd.md) — 产品权威文档