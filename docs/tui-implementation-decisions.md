# TUI 实施决策记录

**日期**: 2026-07-02（持续更新）
**作用**: 记录 TUI 相关决策、技术备注与待补充项，供跨 session 衔接。本文档可能滞后；与代码或其他说法冲突时，停下来列清差异，等人决定后再继续。
**§1「偏离」**: 相对已归档的初版设计稿（[`archive/tui-design.md`](./archive/tui-design.md)）而言；未读过设计稿时，直接看代码即可。

---

## 1. 相对设计稿的偏离（刻意不做）

| 设计稿条目 | 决策 | 理由 |
|------------|------|------|
| §6.2 楼层竖线：非 active 为 dim、切换 150ms 过渡 | **不做** | 当前仅 active 楼层画 accent 竖线，效果已足够弱；保持现状 |
| §6.2 楼层头部：时间/签名两行布局、签名 25 字符 | **不做** | 当前排版满意，不调整 |
| §6.2 楼层间距：分隔线 + 1 空行 | **不做** | 保持现状 |
| §6.3 `Enter` 点引用 / `FloorRef` 跳转 | **不做** | 交互延后到更晚阶段 |
| §6.3 `o` 打开附件 / AppMark / 失败图片 | **不做** | 附件仅展示；图片失败只显示 `[图片加载失败]` |
| §6.3 `#` 跳楼层 | **不做** | |
| §6.4 投票块「当前选中项」高亮 | **延后** | 与楼层/投票排版大修时一起做 |
| §12 Feed↔Detail 切换动画 | **不做** | 滑动导致 Kitty 头像残留；用加载动画代替 |
| §16 终端 &lt; 80×24 警告 | **已取消（C3-a）** | 不硬拦截；窄窗做渐进降级（藏头像 / 藏计数 / active tab 优先），仅 width/height == 0 时跳过绘制 |
| §16 引用嵌套展平 | **不关心** | 论坛内容不会出现 Quote 嵌套 Quote |
| §16 头像失败：首字符 / dim 方块 | **保持现状** | 使用 `noavatar` 占位 + `…` |
| §3 图片「不做降级」 | **实际有 WT 策略** | Windows Terminal 帖子大图走 Sixel；头像/表情走 Kitty；以运行时稳定为准 |
| §16 图片失败保留 `o` | **不做** | 仅文本提示失败 |

---

## 2. 已实现（2026-07-02）

| 项 | 说明 |
|----|------|
| **详情多页懒加载** | 视口最后可见楼层距末尾 ≤2 时请求 `page+1` 并追加 `posts`；`g`/`G` 仍为整页替换 |
| **加载动画** | `draw_loading_indicator` 点阵循环（`tick` 驱动）；首屏加载详情时不绘制楼层 |
| **详情懒加载触发** | 按视口内**最后可见楼层**距末尾 ≤2 触发（兼容半屏 `j`/`k` 滚动） |
| **Sixel 末行边距** | `graphics_bottom_margin` 按协议保留底行 |

---

## 3. 明确不做（本阶段）

- Feed ↔ Detail 横向滑动切换
- 详情 `j`/`k` 逐行滚动（半屏滚动为避免大图残留，保持现状）
- 详情页 `#` 跳楼层、`Enter` 引用跳转、`o` 打开附件/链接
- 投票块排版与高亮（延后）
- 楼层头部/间距/竖线动画等视觉微调
- ~~全量 prefetch 改视口懒加载~~（2026-07-09：详情图已做视口 ±1 + 并发 3）
- 恢复 `erase_graphics_guard_band` 每帧擦洗（见 §4）

---

## 4. 图形 / 终端技术备注

### 4.1 `erase_graphics_guard_band`（已废弃）

- **目的**: Windows Terminal + Kitty 图层渗入 Status Bar 区域时，每帧擦洗内容区底部 N 行。
- **现状**: 调用已移除；函数保留但**不得**重新接入 draw 循环。
- **与残留修复的关系**: 与当前 placement 同步清理方案冲突，会加剧鬼影。
- **与 Sixel 末行滚动无关**。

### 4.2 Sixel 末行滚动（ratatui-image [#57](https://github.com/ratatui/ratatui-image/issues/57)）

- **问题**: Sixel 画到视口/终端最末一行时，部分终端会误触发向上滚动。
- **修复**: `hiptty_image::graphics_bottom_margin` — 按**实际绘制协议**在视口底部保留 1 行：
  - `ProtocolType::Sixel` → 1 行
  - `ProtocolType::Kitty` → 1 行（mdfried 模型，避免图层压住底栏）
  - Windows Terminal 上 `ImageKind::Content` 强制按 Sixel 计边距（与 `cache.rs` 解码协议一致）
- **实现位置**: `draw_graphic_in_viewport`、`floor_list::render_image_block` 裁剪高度。

### 4.3 残留 / 性能

- **WT 详情大图滚动溢出（2026-07-09）**: 帖子 Content 图在 WT 上走 Sixel + `SlicedImage`。当图同时被上裁（`skip>0`）与下裁（`drop>0`）时，上游 `ratatui-image` 11.0.6 的 `SlicedSixelData::bands` 按 `(height - drop)` 取 band，**未扣 skip**，Sixel 序列比 `image_area` 更高，像素溢出进 Status Bar，再滚形成残图。仅上裁或仅下裁、或图完整入屏时不复现。
  - **修复**: `[patch.crates-io]` → `vendor/ratatui-image`（基于 11.0.6），见 `vendor/ratatui-image/PATCHES.md`。
  - 与 §4.1 guard band、Kitty placement 清理是不同路径；勿用每帧 guard band 顶替。
- **图片缓存**: Content/Avatar/Smiley 均为进程内 `ImageCache` 内存缓存（decode + 协议数据）。同会话再进帖不会再占位；**无** Content 磁盘缓存。Avatar 另有 `AvatarDiskCache`。
  - 内存：最多 256 条（硬）+ Ready 渲染像素估值软预算 128 MiB；视口 ±pad **pin** 不可被软预算淘汰；刚插入的 Ready 自我保护。解码队列另限 16 job / 16 MiB。
  - 磁盘头像：启动时 `purge` + 重计；运行期维护 `file_count`/`total_bytes`，高水位（1800 文件或 64 MiB）触发清理到低水位（1600 / 48 MiB），每 32 次写入也 opportunistically 检查。
  - 下载：`get_bytes` 用无 cookie_provider 客户端，仅附加 Cookie **快照**（登录附件可读，Set-Cookie 不回写 jar）；硬限 8 MiB。
- **详情图片性能（2026-07-09，2026-07-10 修正）**:
  - 视口懒加载：只 prefetch 可见楼层 ±1，滚动时再补；HTTP 并发上限 3。
  - **真并发**：`FetchImage` 在 worker 内 `tokio::spawn`（与页面加载/发帖串行队列解耦）；此前「并发 3」只是入队深度，实际仍串行。
  - 解码线程池：2–4 worker（非单线程）。
  - Loading 占位高度按 `max_cols/2` 估（8–20），宽度用满内容列，减轻撑开。
  - 滚动保持：decode 前后用 `(floor, offset_in_floor)` 锚点，禁止吸附楼顶（修「图未出完就滚再弹回」）。
- **详情布局缓存（2026-07-10）**:
  - 此前每 50ms 帧对**全部**楼层重复 `layout_post_blocks`/换行（`floor_list_total_height` + scroll anchor + `draw_floor_list` 各扫一遍）；1000 楼仅计算即可 ~48ms，逼近帧预算。
  - 现：`FloorLayout` 单次构建 heights/offsets/total，缓存在 `DetailState::layout`；宽变/帖变/图片 decode 完成时失效。
  - 绘制用缓存高度 O(log n) 二分跳到首可见楼，仅对可见楼再 `layout_post_blocks` 出画。
- **详情加载意图 / g·G / layout_revision（2026-07-10）**:
  - `DetailLoadIntent::{ReplaceTop, ReplaceBottom, AppendPreserve, PrependPreserve}` + `request_id` 丢弃过期响应。
  - `layout_revision`：同宽同帖数换页也会重建布局（修复 G 复用第一页高度）。
  - `G` → 最后一页后 `selected=last` + `max_scroll`；`g` → 首页顶。
  - 仅加载末页后上滚：`loaded_page_lo` + `PrependPreserve` 向前拼页并平移 scroll，避免卡在末页第一楼。
  - 图片 `get_bytes` 使用无 cookie 客户端；JPEG 解码不再绕过 Limits；解码队列有界。

- **Kitty placement 清理（2026-07-09）**:
  - `clear_content_viewport` / `clear_graphics_in_area` **仍每帧** `clear_rect` + `fill_area_spaces`（Sixel/格子覆盖）。
  - Kitty `d=y`（`clear_terminal_placements_in_area`）**仅在 geometry dirty 时**发送：scroll / 翻页 / resize / 图 decode / 首帧。由 `begin_frame_graphics` + `App::graphics_dirty` 控制。
  - WT 上 Content 走 Sixel，每帧 `d=y` 主要服务头像/表情 Kitty 层；空闲 tick 不再刷屏。
  - `d=y` 序列改为单次 write 批处理。
  - **`erase_graphics_guard_band` 仍废弃**，不重新接入。

---

## 5. Backlog / TODO（2026-07-03 共识）

相对初版设计稿（[`archive/tui-design.md`](./archive/tui-design.md)）与 plan Phase 0–8，**尚未实现或刻意延后**的条目如下。已列入 §1 偏离项的不重复。

### 5.1 明确不做（本阶段关闭）

| 项 | 说明 |
|----|------|
| PM 列表 `n` 新私信 | 设计稿为 PMList 快捷键；含「给列表外 UID 发起对话」等可能。Adapter 另有 `pm_new_list`（`filter=newpm`，筛有新消息的对话），TUI 均未接。**结论：不做。** |
| 关注页 `attention` | API 已有，无入口、无需求。**不做。** |
| Feed↔Detail / 页面切换动画 | 终端里体验差（头像残留等）。**只做状态动画**（见 §5.3）。 |
| `ratatui-which-key` | 调研结论：Emacs 式分 scope 动态键位提示；我们已有 `?` 静态帮助 + Status Bar 文案，自研 overlay 够用。**非必须，不引入。** |

### 5.2 延后 TODO（记录想法，暂不实现）

| 项 | 说明 | 优先级 |
|----|------|--------|
| 详情看图 `v` / `:i#N` | 完整看图流程（外部查看器或终端内预览）。 | 低 |
| 多 Profile 凭证管理 | CLI/TUI 仅有 `--profile` 参数（默认 `default`），文件为 `{profile}.credentials.json` + `{profile}.session.json`。**无 UI 切换/增删 profile**；应在设置或登录页管理多账号凭证。 | 中 |
| ~~完整登出~~ | **已做（2026-07-10）**：见 §5.4。 | — |
| 发帖插图 | 见 §5.5；当前 Ctrl+I 路径输入方案废弃，需重新设计交互。 | 中 |
| 黑名单管理 UI | 见 §5.6；仅显示人数 stub。 | 低 |
| 详情 `F` 收藏 | plan 非阻塞项。 | 低 |
| CommandBar `:r#N` / `:g#N` | 详情回复/跳楼层；`#` 跳楼层已在 §1 不做，`:g#N` 与之重叠。 | 低 |
| Title Bar 点 🔔/✉ 进通知/私信 | 现仅轮询显示未读标记，图标不可点。 | 低 |
| 列表页 `g`/`G` | PM/通知/搜索/我的* 等 SimpleList 无首页末页翻页。 | 低 |
| Toast 多条堆叠 | 设计 §11.6 堆叠 + 滑入滑出；现单条 2s 自动消失。 | 低 |

### 5.3 动画（仅状态反馈）

**做**：加载/处理中指示——**有旧数据时**主反馈在 Status Bar 右侧（`加载中…`）；**首载无数据**时内容区居中 `正在加载` 占位。composer/confirm 等仍用自身 loading 态。Logo 呼吸约 3s/周期。

**不做**：窗口/页面切换 slide、弹层 fade-scale、编辑器/Toast 位移动画。

目标：让用户感知「正在加载 / 正在处理」，而非装饰性转场。

### 5.4 登出 — 当前行为说明

`:logout` / 命令栏 `logout`（`commands.rs` + `worker`，2026-07-10 起完整）：

1. 删除 `{profile}.credentials.json`（`clear_credentials`，阻止 AutoLogin）
2. 发 `WorkerRequest::Logout` → `ForumClient::logout()`：清空内存 cookie jar，persist 空 jar，并删除 `{profile}.session.json`
3. UI：`session.logged_in = false`、清 username/uid/未读标记，跳转 Login

多 profile 下其它 profile 文件不受影响。

### 5.5 发帖插图 — 当前行为

- **主路径**：正文写本地图片路径；`post()` 内 `resolve_inline_images` 负责压缩/上传并替换为 `[attachimg]`（远程 URL 包 `[img]`）。无 Ctrl+I、无发送前本地预检、无 composer 插图说明文案。
- 发送中 status bar 右侧显示「发送中…」（含图片处理）；composer 标题同步「发送中…」。
- Worker `UploadComposerImage` 仍保留供 CLI/调试；TUI 不走该路径。

### 5.5b Status Bar（2026-07-09，2026-07-10 修订）

- **布局**：左 `KeyHint` 快捷键（accent 键 + secondary 说明，窄屏按 priority 丢次要键）+ 右状态（`加载中…` / `页码 page/max`，`secondary` 色）。
- **Loading**：首载无数据 → 内容区居中占位；有旧数据刷新 → 仅 status 右侧，列表不闪空。
- **弹层期间**：MainMenu / Settings / SearchPrompt / ForumPicker **不**再在底栏重复快捷键（footer / 弹层内 hints 为准）。
- **Command bar**：`:` 内联替换 status 行（vim cmdline）：左 `:` + 可移动光标输入，右为前缀过滤的命令建议；Tab 补全；Ctrl+U 清空；空 Enter 取消。不再单独 overlay；内容不 dim。
- **命令目录**（`commands::COMMANDS`）：`q` `pm` `notif` `search` `my` `replies` `fav` `refresh` `login` `logout` `exit`（及别名）。
- **详情页回复命令**：`r` → 主题回帖（`ReplyThread`）；`r#N` → 回复 N 楼（`ReplyPost`）；`rr#N` → 引用 N 楼（`QuotePost`）；楼层命令仅在已加载楼层上生效并选中该楼。
- **详情页建议条**：隐藏 nav / 账号命令（`pm` `notif` `search` `my` `replies` `fav` `login` `logout`）；保留 `r` / `r#N` / `rr#N` / `q` / `refresh` / `exit`。
- **快捷键调整**：Feed / 列表 `r` = 强制刷新（不再回复）；详情去掉全局 `q`/`e`/`d`（楼层上下文动作延后）。

### 5.5c UI 精修（2026-07-10）

| 项 | 决策 |
|----|------|
| 内容态 | 统一 `list_placeholder` + `draw_content_placeholder`：Loading / Empty / ErrorEmpty；有数据时错误仅 toast |
| 选中态 | 左侧粉色 `│` = 键盘焦点；标题 accent+bold 辅助；去掉帖子列表 `accent_bg` 碎块 |
| PM「我发的」 | 作者 `我 ·` 前缀 + accent 色；**不**占用焦点竖线；本轮不做右对齐气泡 |
| 窄窗（C3-a） | 不硬拦截；&lt;55 列藏头像；标题列过窄藏回复/浏览量；forum tabs **active 优先可见** |
| 色彩语义 | `muted` 分隔/装饰；`secondary` 帮助/快捷键/时间/状态；弹层 footer 用 secondary |
| Toast | 暗色完整边框 + 亮色剩余时间弧，避免边框「坏掉」观感 |
| Logo | 呼吸周期 ~3s（60×50ms），字符相位差减小 |
| 回归 | `hiptty-widgets` `visual_snapshot`：40×12 / 80×24 / 120×40 关键字形断言 |

### 5.6 黑名单 — 当前行为（空壳）

- 打开设置时 worker 拉 `LoadBlacklist`，仅更新 `blacklist_count`。
- 设置第 5 行显示「黑名单 [N 人]」；Enter  toast「黑名单管理将在后续版本提供」。
- Adapter 的 `blacklist_add` / `blacklist_remove` 已实现，TUI 无管理界面。

### 5.7 鼠标与滚动

**已实现（2026-07-03）**：

- 左键点列表行改选中（Feed / PM / 通知 / 搜索 / 我的* / PM 对话）。
- Title Bar 点 🔔/✉ 进通知/私信（需已登录且有未读标记）。
- 右侧 `tui-scrollbar` 垂直滚动条：轨道点击跳转、拇指拖拽。
- 滚轮在内容区 **按行平滑滚动**（`WHEEL_LINES=3`），列表用 `scroll_lines` 子行偏移，**不等于 j/k**。
- 可变高度详情：`scroll_top` 行偏移 + 滚动条；参考 `tui-scrollview` 思路，自研不接库。

**待做**：

- 详情楼层左键选中。
- **j/k 体验**：半屏/逐条滚动偏硬，后续单独优化（可参考其它 terminal 客户端）。

**明确不做**：滚轮等同 j/k 的简易映射。

### 5.8 生态库调研备忘

| 库 | 结论 |
|----|------|
| [tui-popup](https://github.com/ratatui/tui-widgets/tree/main/tui-popup) | 已并入 [tui-widgets](https://github.com/ratatui/tui-widgets)；弹层现自绘（菜单/帮助/设置/命令栏）。迁移可选，非阻塞。 |
| [tui-scrollview](https://github.com/ratatui/tui-widgets/tree/main/tui-scrollview) | 固定内容区平滑滚动；**详情楼层为可变高度**，参考其行偏移思路，用 `scroll_top` + 自绘列表裁剪，**不直接依赖**。 |
| [tui-scrollbar](https://github.com/ratatui/tui-widgets/tree/main/tui-scrollbar) | **已引入**（`hiptty-widgets/scroll.rs`）：垂直条 + 拖拽/滚轮；`refs/tui-widgets` 供对照。 |
| `ratatui-interact` | plan Phase 8；FocusManager / ToastStack 等，与自研 overlay 重叠，按需引入。 |

### 5.9 工程待办（自行排期）

| 项 | 说明 |
|----|------|
| `insta` 快照测试 | plan Phase 8；UI 回归。 |
| §16 边界 | 极小终端、长帖性能等。 |
| CI | `.github/workflows/ci.yml` 仅 `main`/`master` 触发，`tui` 分支 push 不跑 CI。 |
| tick 间隔 | plan 写 16ms，实现为 50ms；非功能缺口，可文档对齐。 |

---

## 6. 修订历史

| 日期 | 变更 |
|------|------|
| 2026-07-02 | 初版：汇总 P2/P3 偏离决策、待办、图形技术备注 |
| 2026-07-03 | §5 Backlog：PM n/关注/动画/登出/插图/黑名单/鼠标/生态库/工程待办 |
| 2026-07-03 | 鼠标：滚轮平滑滚动 + tui-scrollbar 拖拽条 + Title Bar 图标点击 |
| 2026-07-07 | 文档整理：`tui-design.md` 与生态调研移至 `archive/`；新增 `AGENTS.md`、`architecture.md`、根目录 `README.md` |
| 2026-07-07 | 协作口径调整：代码/文档/口头说明均可能过时；冲突或方向跑偏时停下等人决定；Backlog 为备忘 |
| 2026-07-09 | §5.5b Status Bar：左 hint 分档 + 右 loading/页码；`:` 内联；Feed `r` 刷新；详情去 qed |
| 2026-07-09 | 命令模式：目录/Tab 补全/建议条/光标编辑；移除独立 CommandBar overlay |
| 2026-07-09 | 详情命令：`r#N` 回复楼、`rr#N` 引用楼 |
| 2026-07-09 | IME：输入态 `frame.set_cursor_position`，光标跟随登录/命令栏/搜索/composer |
| 2026-07-09 | Composer：Ctrl+Enter 发送；去插图；引用块只读；Tab 仅新帖/编辑 |
| 2026-07-09 | 新帖分类：PrePost `#typeid` 驱动 UI；B&S/占位 0 强制选择；←→ 切换 |
| 2026-07-09 | 取消终端 &lt; 80×24 硬拦截：小窗口继续渲染，仅 0 尺寸跳过绘制 |
| 2026-07-09 | 详情正文：引用头去重（`@author in time`）；空白折叠；表情行内；underline/strike |
| 2026-07-09 | 楼层头：去重 `发表于`；`本帖最后由…编辑` 并入 chrome（`编辑于` / `由X编辑于`） |
| 2026-07-09 | §4.3：vendor patch `ratatui-image` 修复 Sixel skip+drop 溢出残图 |
| 2026-07-09 | §4.3：详情图视口懒加载、解码池、占位高度、scroll 楼内锚点 |
| 2026-07-09 | §4.3：Kitty `d=y` 仅 geometry dirty 时发送；guard band 仍废弃 |
| 2026-07-10 | §4.3：FetchImage 真并发；内存/磁盘缓存上限；下载 8 MiB + 解码像素限制 |
| 2026-07-10 | §4.3：详情 `FloorLayout` 缓存，消除每帧全帖 re-wrap |
| 2026-07-10 | 详情文档坐标 `DocY=u32`；完整 logout 清 cookie/session |
| 2026-07-10 | 网络：HTTP 按类型拆分 timeout（上传 90s）；worker 四 lane（读 latest-wins / 写串行 / 未读独立 / 图并发）；全局 request_id + 登出 barrier |
| 2026-07-10 | 修：未读新 epoch abort 旧任务；Login 会话 barrier；ImageCache generation/reset_session；SimpleList append 分页 |
| 2026-07-10 | Auth barrier 全程暂存读请求；reset_session 清空解码等待队列；auth_in_flight 防重复 AutoLogin |
| 2026-07-10 | Auth barrier 结束后丢弃暂存读；Logout/登录成功清理 session UI 与登录明文 |
| 2026-07-10 | auth_op_id 门闩：忽略过期 Session/LoginResult/LoggedOut，防登录登出互相覆盖 |
| 2026-07-10 | §5.5c UI 精修：内容态占位、选中竖线统一、PM 我方前缀、窄窗 C3-a、对比度、Toast/Logo/hints、visual_snapshot |
| 2026-07-10 | 修：Kitty `REPORT_ALL_KEYS_AS_ESCAPE_CODES` 导致 IME 中文在登录/输入框“消失”（crossterm 无 associated text） |