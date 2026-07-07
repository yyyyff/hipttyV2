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
| §16 终端 &lt; 80×24 警告 | **保持现状** | 当前过小直接不可用即可；后续可能取消该限制 |
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
- 全量 prefetch 改视口懒加载（性能）  
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

- 图片滚动残留：用户已参考其他项目修复，**保持现状**，不在此文档周期内改动。  
- 详情页图片 prefetch：暂不优化。

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
| 完整登出 | 见 §5.4 现状；可选补 `adapter.logout()` + 删 `session.json`。 | 低（可不做） |
| 发帖插图 | 见 §5.5；当前 Ctrl+I 路径输入方案废弃，需重新设计交互。 | 中 |
| 黑名单管理 UI | 见 §5.6；仅显示人数 stub。 | 低 |
| 详情 `F` 收藏 | plan 非阻塞项。 | 低 |
| CommandBar `:r#N` / `:g#N` | 详情回复/跳楼层；`#` 跳楼层已在 §1 不做，`:g#N` 与之重叠。 | 低 |
| Title Bar 点 🔔/✉ 进通知/私信 | 现仅轮询显示未读标记，图标不可点。 | 低 |
| 列表页 `g`/`G` | PM/通知/搜索/我的* 等 SimpleList 无首页末页翻页。 | 低 |
| Toast 多条堆叠 | 设计 §11.6 堆叠 + 滑入滑出；现单条 2s 自动消失。 | 低 |

### 5.3 动画（仅状态反馈）

**做**：加载/处理中指示（已有 `draw_loading_indicator` 点阵循环，可扩展到列表首屏、发帖提交、登录等）。

**不做**：窗口/页面切换 slide、弹层 fade-scale、编辑器/Toast 位移动画。

目标：让用户感知「正在加载 / 正在处理」，而非装饰性转场。

### 5.4 登出 — 当前行为说明

`:logout` 与命令栏 `logout` 当前逻辑（`commands.rs`）：

1. 删除 `{profile}.credentials.json`（`clear_credentials`）
2. 内存里 `session.logged_in = false`、`username = None`
3. 跳转 Login 屏

**未做**：

- 未调用 `ForumClient::logout()`（adapter 会 `clear_cookie_store` 并写回 session 文件）
- 未删除 `{profile}.session.json`（cookie 文件仍留在磁盘）
- worker 进程内 HTTP client 的 cookie jar 未清空（直到进程退出）

因此「清本地 session」在 plan 里指删 `session.json` + 清 cookie；**现实现只清了 credentials，session 文件与内存 cookie 仍在**。多 profile 下其它 profile 文件不受影响。按当前共识可维持现状，完整登出列入 §5.2。

### 5.5 发帖插图 Ctrl+I — 当前行为

- Status Bar / 帮助仍写「Ctrl+I 插图」。
- 按下后进入 **手输本地路径** 模式（`ComposerFocus::ImagePath`），Enter 走 `UploadComposerImage` 上传并插入 `[attachimg]id[/attachimg]`。
- **无系统文件选择器**；路径方案待废弃，插图交互需单独 redesign（§5.2）。

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