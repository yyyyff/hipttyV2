# hiptty 架构

**作用**：crate 分层、数据流与关键模块速查。改 bug 或加功能时先定位 crate，再查对应模块。

---

## 1. 总览

hiptty 是 [4d4y](https://www.4d4y.com)（Discuz）论坛客户端，提供两种入口：

| 二进制 | Crate | 用途 |
|--------|-------|------|
| `hiptty` | `crates/hiptty` | 终端 UI（Ratatui） |
| `hiptty-cli` | `crates/hiptty-cli` | Headless CLI，JSON 输出，供脚本/Agent 调用 |

共享层：`hiptty-core`（领域类型）+ `hiptty-adapter`（HTTP + HTML 解析）。TUI 在之上叠加渲染、图像、组件与应用逻辑。

```
                    ┌─────────────┐     ┌──────────────┐
                    │   hiptty    │     │  hiptty-cli  │
                    └──────┬──────┘     └──────┬───────┘
                           │                    │
                    ┌──────▼──────┐             │
                    │  hiptty-app │             │
                    └──────┬──────┘             │
         ┌─────────────────┼─────────────────┐ │
         │                 │                 │ │
  ┌──────▼──────┐   ┌──────▼──────┐   ┌──────▼──────┐
  │hiptty-widgets│   │ hiptty-image │   │hiptty-render│
  └──────┬──────┘   └──────┬──────┘   └──────┬──────┘
         │                 │                 │
         └─────────────────┼─────────────────┘ │
                           │                   │
                    ┌──────▼───────────────────▼──┐
                    │       hiptty-adapter        │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │        hiptty-core          │
                    └─────────────────────────────┘
```

---

## 2. Crate 职责

### hiptty-core

纯领域层，无 I/O。定义论坛数据结构、错误码、设置与凭证类型。

| 模块 | 内容 |
|------|------|
| `content` | `ContentNode` / `ContentSpan`（正文、图片、引用、表情） |
| `thread`, `post`, `list`, `search` | 帖子、列表、搜索 |
| `forum` | 静态版块列表 |
| `session`, `settings` | 会话与 UI 设置 |
| `error` | `AdapterError` 及错误码 |
| `poll`, `user`, `security` | 投票、用户、密保问题 |

### hiptty-adapter

Discuz HTML ↔ 领域类型的桥梁。

| 模块 | 内容 |
|------|------|
| `client` | `ForumClient` trait — CLI 与 TUI 的统一 API |
| `discuz` | `DiscuzClient` — 真实 HTTP 实现 |
| `parser` | 各页面的 HTML 解析（scraper + selector） |
| `auth`, `session` | 登录、cookie 持久化（`{profile}.session.json`） |
| `stub` | 测试用 stub client |
| `fixture` | 录制 HTML fixture（`--ignored` 网络测试） |

测试 fixture 位于 `crates/hiptty-adapter/tests/fixtures/`。

### hiptty-render

TUI 渲染辅助，不依赖 adapter。

| 模块 | 内容 |
|------|------|
| `theme` | Dark/Light 色板（`Palette`） |
| `content` | `ContentNode` → `ratatui::Text` |
| `wrap`, `text`, `fill` | 换行、计数格式化、填充 |
| `terminal` | 终端图形清理等 |

### hiptty-image

终端图像协议（Kitty / Sixel / halfblocks）与缓存。

| 模块 | 内容 |
|------|------|
| `cache` | 内存解码缓存 + 协议选择 |
| `draw` | 在视口内绘制图形，含 `graphics_bottom_margin` |
| `layout`, `content_layout` | 头像/表情/帖子大图布局 |
| `prefetch`, `avatar_disk` | 预取与磁盘头像缓存 |
| `avatar_placeholder`, `smiley` | 占位图与表情资源 |

### hiptty-widgets

可复用 Ratatui 组件，无应用状态。

| 模块 | 内容 |
|------|------|
| `thread_list`, `simple_list` | Feed / PM / 通知等列表 |
| `floor_list` | 详情页楼层（可变高度 + 图片块） |
| `scroll` | 垂直滚动条（基于 `tui-scrollbar`） |
| `title_bar`, `status_bar` | 顶栏 / 底栏（左快捷键分档 + 右状态；`:` 命令行内联于 status bar） |
| `overlays` | 菜单、设置、搜索提示（命令栏已并入 status bar） |
| `composer`, `login`, `forum_picker` | 发帖编辑器、登录、版块选择 |
| `poll_block`, `pm_thread`, `logo` | 投票块、私信对话、Logo |

### hiptty-app

TUI 应用核心：状态机、事件循环、绘制编排。

| 模块 | 内容 |
|------|------|
| `app` | `App` 状态：`Page`、`Overlay`、列表/详情/登录等 |
| `run` | 主循环：tick（50ms）、键盘/鼠标、draw |
| `worker` | 后台 tokio task，通过 channel 处理 `ForumClient` 请求 |
| `event`, `handlers` | 按键分发与 worker 响应处理 |
| `mouse` | 点击、滚轮、滚动条拖拽 |
| `nav` | `NavStack` 页面栈 |
| `draw` | 按 `Page`/`Overlay` 调度 widgets 绘制 |
| `commands` | `:` 命令栏逻辑 |
| `composer` | 发帖/回复/引用状态 |
| `config` | 配置目录、`settings.json`、凭证读写 |
| `list_page` | 各 SimpleList 页面状态 |

### hiptty / hiptty-cli

薄入口：`main.rs` 解析 CLI 参数，构造 client，调用 `hiptty_app::run` 或 `hiptty-cli::run::execute`。

---

## 3. TUI 数据流

```
用户输入 (键盘/鼠标)
    → event.rs / mouse.rs / handlers.rs
    → WorkerRequest (mpsc)
    → worker.rs (tokio task)
    → ForumClient (hiptty-adapter)
    → WorkerResponse (mpsc)
    → handlers.rs 更新 App 状态
    → draw.rs 调度 hiptty-widgets + hiptty-image
    → ratatui Terminal::draw
```

**图像路径**：`run.rs` 启动时 `Picker::from_query_stdio()` 探测协议一次；`worker` 收到 `FetchImage` 后下载解码，结果写入 `ImageCache`；绘制时 `hiptty-image::draw` 按协议渲染。

**会话**：启动时 `CheckSession` → 可选 `AutoLogin`（读 `{profile}.credentials.json`）→ 进入 `ThreadFeed` 或 `Login`。

---

## 4. 配置与持久化

| 路径 | 内容 |
|------|------|
| `~/.config/hiptty/` | 默认配置目录（可用 `--config` / `HIPTTY_CONFIG` 覆盖） |
| `settings.json` | UI 设置（主题等） |
| `{profile}.credentials.json` | 登录凭证（MD5 密码 + 密保）；Unix 写入后权限 `0600` |
| `{profile}.session.json` | HTTP cookie 会话；Unix 写入后权限 `0600` |

Profile 默认 `default`，可通过 `--profile` / `HIPTTY_PROFILE` 切换。

macOS 会自动迁移旧路径 `~/Library/Application Support/hiptty/`。

登录后 TUI 约每 30 秒检查一次未读私信/通知；若上一轮仍在 worker 中则跳过，避免断网时请求积压。

---

## 5. 外部参考

`refs/` 目录存放对照用的第三方项目源码（`tui-widgets`、`mdfried` 等），**不是** hiptty 构建依赖，仅供开发时参考。