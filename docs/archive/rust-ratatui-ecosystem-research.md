# hiptty → Rust + Ratatui 生态库调研

> **归档**：2026-06-26 调研快照，crate 选型已大部分落地。生态库相关备忘见 [`../tui-implementation-decisions.md`](../tui-implementation-decisions.md) §5.8。

**文档角色**：技术栈迁移至 Rust + Ratatui + ratatui-image 时的 crate 选型参考。以初版 `tui-design.md` 为设计依据，按 UI 壳 / 正文渲染 / 图像 / Adapter / 平台 / 测试六层映射外部库，标明可复用与须自研边界。

**调研日期**：2026-06-26

**调研方法**：对官方文档、crates.io、GitHub README 做定向调研，关键结论附来源链接。

---

## 1. 核心栈（已定 + 必要配套）

| 层 | Crate | 说明 |
|----|-------|------|
| TUI 核心 | **ratatui** + **crossterm** | 官方标准组合；内置 `Table` / `List` / `Paragraph` / `Block` |
| 终端图像 | **ratatui-image** | 统一 Kitty / iTerm2 / Sixel；`Picker::from_query_stdio()` 自动探测协议与 cell 像素尺寸 |
| 图像解码/缩放 | **image** | `ImageReader` / `imageops::resize` / `save`；ratatui-image 直接依赖 |
| 异步运行时 | **tokio** | Adapter 网络、非阻塞 UI |
| HTTP | **reqwest** | `cookie_store` feature；`multipart` 上传 |
| Cookie 持久化 | **reqwest_cookie_store** | JSON 序列化 + cookie 持久化 |
| HTML 解析 | **scraper** | Servo `html5ever` + CSS selector，语义上最接近 hipda/Jsoup |
| 字符编码 | **encoding_rs** | 提供 `GBK` 静态编码 |
| 序列化/配置 | **serde** + **serde_json** | 标准方案 |
| 配置路径 | **directories** | 跨平台 XDG/CSIDL 路径 |
| 错误类型 | **thiserror** | AdapterError 错误模型 |

---

## 2. 按功能域映射的可复用库

### 2.1 UI 壳

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| 弹层/模态 | **tui-popup**（[ratatui/tui-widgets](https://github.com/ratatui/tui-widgets)） | 居中 overlay；支持 `PopupState` 拖拽 |
| 滚动容器 | **tui-scrollview** | 大内容区滚动；官方维护，有 mouse 示例 |
| 滚动条 | **tui-scrollbar** | 与 scrollview 配套 |
| 焦点 + 鼠标 + Toast | **ratatui-interact** | `FocusManager`、`Toast`/`ToastStack`、`PopupDialog`、`ListPicker`、`ScrollableContent` |
| 分屏键位 | **ratatui-which-key** | Scope / Action / Category 模型，与 tui-design 的快捷键 Scope 体系高度同构 |
| 命令输入 | **ratatui-interact::Input** | `:` 命令模式 |
| 编辑器 | **tui-textarea** | 多行、`Ctrl+Enter`、滚动；成熟稳定 |
| 帖子列表 | **ratatui::widgets::Table**（内置） | 列布局直接用 |
| 导航状态机 | **自写** | 见 `tui-design.md` §14 |
| 事件循环骨架 | **ratatui 官方 async recipe** 或 **crates-tui** 源码 | tokio `EventStream` + tick/render 分离 |
| 快捷键帮助 | **ratatui-which-key** 弹层 | 比自写 `HelpOverlay` 更一致 |

**备选但建议审慎：**

| Crate | 原因 |
|-------|------|
| **ratatui-kit** | React 式组件框架，stars 少，与现有 UI 设计冲突 |
| **rat-salsa / rat-scrolled / rat-widget** | 另一套 widget 体系，与 tui-widgets 功能重叠 |
| **ratatui-markdown** | 论坛正文是 `ContentNode` 而非 CommonMark，用 ratatui 原生 `Text`/`Span`/`Line` 更合适 |

### 2.2 正文与长帖

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| 富文本 inline style | **ratatui::text**（内置） | `ContentNode` → `Span` 带 `Style::fg()` / bold / italic |
| 引用块/楼层块 | **自写 Widget** | 见 `tui-design.md` §15 `FloorBlock` / `QuoteBlock` |
| 长帖可变高度滚动 | **ratatui_widget_scrolling** | 为「每行高度动态」设计；只渲染可见区 |
| CJK 列宽 | **unicode-width**（ratatui 已集成） | 列表列宽、截断 |
| 文本换行 | **ratatui::widgets::Paragraph** + wrap | 内置 word wrap |
| 链接点击开浏览器 | **opener** | 跨平台 `open::that(url)` |

### 2.3 图像与表情

| 需求 | 推荐 Crate | 说明 |
|------|------------|------|
| 协议探测 + 渲染 | **ratatui-image** | Kitty / iTerm2 / Sixel 协议统一渲染 |
| 启动探测持久化 | **ratatui-image::Picker** + **serde** | `from_query_stdio()` 在 alternate screen 后调用 |
| 异步加载/缩放 | **ratatui-image::ThreadProtocol** | 后台线程 `resize_encode`，UI 不阻塞 |
| 表情 PNG 资源 | **rust-embed** 或 **include_dir** | 60+ 表情本地 PNG；编译期嵌入 |
| 附件图上传压缩 | **image** + **imageops** | >2MB 缩 JPEG |
| 临时缓存目录 | **tempfile** | 退出清理 |
| 打开本机查看器 | **opener** | 图片 / 附件外部打开 |

**ratatui-image 终端兼容性：**

| 终端 | 协议 | 状态 |
|------|------|------|
| Ghostty | Kitty | ✔️ QA 通过 |
| Kitty | Kitty | ✔️ 参考实现 |
| iTerm2 | iTerm2 | ✔️ Mac only |
| Wezterm | iTerm2 | ✔️ Sixel/Kitty 有 bug，仅 iTerm2 稳定 |
| Xterm / Foot / mlterm | Sixel | ✔️ |
| Alacritty / Konsole / Warp | — | ❌ 不支持 |

- 已知问题：Sixel 在末行可能触发滚动 ([#57](https://github.com/ratatui/ratatui-image/issues/57))——楼层布局需留底边距。
- 截图测试套件：[ratatui-image-screenshots](https://benjajaja.github.io/ratatui-image-screenshots/)

### 2.4 Adapter — 已实现

Adapter 层（`hiptty-adapter` crate）已完整实现，核心依赖：

| 模块 | Crate | 说明 |
|------|-------|------|
| HTTP + 超时 + Cookie | **reqwest** + **reqwest_cookie_store** | GET/POST/Multipart；Cookie 持久化至 `session.json` |
| HTML 解析 | **scraper** | CSS selector 对照 hipda 原始解析器 |
| GBK 编解码 | **encoding_rs::GBK** | 响应解码 + 表单 GBK 百分号编码 |
| 密码 MD5 | **md-5** | 登录 POST |
| 内联图片上传 | **image** + **reqwest::multipart** | 本地图片自动检测上传 |
| 发帖节流 | **自写** | 30s 间隔 |

`ForumClient` trait 共 30 个方法，已覆盖全部读写操作。

### 2.5 平台与工程质量

| 需求 | Crate | 说明 |
|------|-------|------|
| 跨平台打开文件/URL | **opener** | Win/macOS/Linux |
| 剪贴板 | **arboard** | 鼠标拖选复制增强 |
| CLI 入口 | **clap** | 已在 `hiptty-cli` 中集成 |
| 日志 | **tracing** + **tracing-subscriber** | adapter 调试 |
| 崩溃报告 | **color-eyre** | ratatui 生态惯例 |
| 终端挂起恢复 | **signal-hook** | 参考官方 recipe `suspend()` |
| 快照测试 | **insta** + **cargo-insta** | UI 快照回归 |
| UI 回归 | **ratatui::backend::TestBackend** | 对齐现有测试思路 |
| 发布 | **cross** / **cargo-dist** | 跨平台二进制 |

---

## 3. 推荐 Cargo 清单

### Tier 1 — 直接采用

```
ratatui, crossterm, ratatui-image, image, tokio, reqwest, reqwest_cookie_store,
scraper, encoding_rs, serde, serde_json, directories, thiserror, opener,
tui-popup, tui-scrollview, tui-scrollbar, tui-textarea, ratatui-interact,
ratatui-which-key, ratatui_widget_scrolling, rust-embed, tempfile
```

### Tier 2 — 按需引入

| Crate | 场景 |
|-------|------|
| **moka** | 列表缓存 |
| **arboard** | 剪贴板 |
| **insta** | 快照测试 |
| **tracing** | 日志 |
| **color-eyre** | 错误报告 |

### Tier 3 — 不建议

| Crate | 原因 |
|-------|------|
| **ratatui-markdown** / **tui-markdown** | 论坛内容不是 CommonMark |
| **ratatui-kit** | 不成熟，与现有 UI 设计冲突 |
| **kuchiki** / **lol_html** | scraper 已够 |
| **Cursive 系** | 与 Ratatui 栈不一致 |
| **自写 Kitty 协议** | ratatui-image 已解决 |

---

## 4. 架构参考项目

| 项目 | 可借鉴点 | 相关度 |
|------|----------|--------|
| [ratatui/crates-tui](https://github.com/ratatui/crates-tui) | `tui.rs` + `events.rs` 事件循环 | 高 |
| [ratatui/tui-widgets](https://github.com/ratatui/tui-widgets) | popup/scrollview/prompts 官方套件 | 高 |
| [ratatui/ratatui-image](https://github.com/ratatui/ratatui-image) | 终端图像渲染参考 | 高 |
| [JonathanBerhe/gh-tui](https://github.com/JonathanBerhe/gh-tui) | MVU 六 crate 分层架构 | 中高 |
| [Brainwires/ratatui-interact](https://github.com/Brainwires/ratatui-interact) | 焦点/鼠标/toast/ScrollableContent | 高 |
| [jayson-lennon/ratatui-which-key](https://github.com/jayson-lennon/ratatui-which-key) | 分 scope 键位 | 高 |
| [teufelchen1/jelly](https://github.com/teufelchen1/jelly) | `ratatui_widget_scrolling` 来源 | 中 |

### gh-tui 架构要点（MVU 参考）

```
gh-core    → State, Msg, Cmd（纯 reducer）
gh-input   → 输入
gh-api     → 网络
gh-render  → 渲染
gh-ui      → ratatui 组合
gh-tui     → 入口
```

---

## 5. 仍须自研的部分

| # | 模块 | 说明 |
|---|------|------|
| 1 | ContentNode 渲染器 | `ContentNode` 树 → `ratatui::Text`，含引用块、图片锚点、楼层引用 |
| 2 | FloorBlock 组件 | 楼层流布局：竖线、avatar、正文、签名 |
| 3 | ThreadFeedList 组件 | 帖子列表虚拟滚动 + 瀑布流自动加载 |
| 4 | 导航状态机 | 页面栈 + overlay；见 `tui-design.md` §14 |
| 5 | 命令解析 | `:r#N` `:g#N` `:i#N` 等 |
| 6 | Logo 动画 | 1 行文本紫色↔灰色呼吸动画 |
| 7 | 内联 Smiley 渲染 | 60+ 表情图片异步加载与排版 |
| 8 | 动画系统 | 页面切换、弹层、Toast 动画（见 `tui-design.md` §12） |

---

## 6. 风险矩阵

| 维度 | Rust 生态覆盖 | 残留风险 |
|------|---------------|----------|
| 图像 | **强**（ratatui-image） | 楼层多图性能 → ThreadProtocol；Sixel 末行滚动 |
| 键位/overlay | **强**（which-key + interact + tui-popup） | 命令模式与 which-key 需统一 Action 枚举 |
| 长帖滚动 | **中强**（widget_scrolling） | 可变高度 + 内嵌图需实测组合 |
| 富文本 | **中**（原生 Text/Span） | `<font color>` 映射自维护 |
| Adapter | **已实现**（reqwest+scraper+encoding_rs） | 已通过 CLI 集成测试验证 |
| 鼠标 | **中**（interact View/Copy） | 与图像区域 hit-test 要协调 |
| 测试 | **强**（insta + TestBackend） | 图像快照需真终端或协议 mock |

---

## 7. 建议 Workspace 结构

```
hiptty-core/      # ✅ 已实现 — 类型、ContentNode、错误模型
hiptty-adapter/   # ✅ 已实现 — reqwest + scraper + encoding_rs（无 ratatui 依赖）
hiptty-cli/       # ✅ 已实现 — clap CLI 入口
hiptty-render/    # 待建 — FloorBlock、ContentRenderer、主题色
hiptty-widgets/   # 待建 — 对 tui-popup/scrollview/interact 的薄封装
hiptty-app/       # 待建 — App 状态、导航、快捷键、动画、MVU reducer
hiptty/           # 待建 — main、配置、启动探测 Picker、图片缓存
```

---

## 8. 结论

Rust 生态对 hiptty 的 **图像层**（ratatui-image）、**壳子组件**（tui-widgets + ratatui-interact + ratatui-which-key）、**Adapter 基础设施**（reqwest + scraper + encoding_rs）覆盖充分。Adapter 已完整实现并经过 CLI 验证。

TUI 层需要自研的是 **论坛富文本渲染**（ContentNode → ratatui::Text）、**楼层布局组件**、**动画系统** 和 **导航状态机**——这些已在 `tui-design.md` 中做了详细设计。

---

## 相关文档

- [tui-design.md](./tui-design.md) — 初版 TUI 设计稿（归档）
- [tui-implementation-decisions.md](../tui-implementation-decisions.md) — TUI 决策与备忘
- [api.md](../api.md) — CLI API 参考
