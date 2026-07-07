# hiptty

[4d4y](https://www.4d4y.com) 论坛终端客户端（Rust + Ratatui）。提供交互式 TUI 与 headless CLI 两种模式。

## 要求

- Rust stable（2021 edition）
- 支持 Kitty / iTerm2 / Sixel 图像协议的现代终端（Windows Terminal 上帖子大图走 Sixel，头像/表情走 Kitty）
- 推荐 Nerd Font 以正确显示图标

## 构建

```bash
cargo build --release
```

产物：

- `target/release/hiptty` — TUI
- `target/release/hiptty-cli` — CLI

仅构建某一 crate：

```bash
cargo build -p hiptty
cargo build -p hiptty-cli
```

## 运行

### TUI

```bash
cargo run -p hiptty
# 或
cargo run -p hiptty --release
```

常用参数：

| 参数 | 环境变量 | 默认 | 说明 |
|------|----------|------|------|
| `--config DIR` | `HIPTTY_CONFIG` | `~/.config/hiptty` | 配置目录 |
| `--profile NAME` | `HIPTTY_PROFILE` | `default` | 会话/凭证 profile |

### CLI

默认输出 JSON（`schema_version: 1`）；加 `--human` 可读文本。

```bash
cargo run -p hiptty-cli -- auth status
cargo run -p hiptty-cli -- threads list --fid 2
cargo run -p hiptty-cli -- thread show 448060
```

完整命令与 JSON 结构见 [`docs/api.md`](docs/api.md)。

## 测试

```bash
# 全 workspace
cargo test --workspace

# 单 crate
cargo test -p hiptty-adapter
cargo test -p hiptty-cli

# 可选：录制网络 fixture（需联网，默认 ignored）
cargo test -p hiptty-adapter -- --ignored
```

## 代码质量

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI 在 `main`/`master` 分支 push 及 PR 时运行上述检查（`tui` 等分支 push 不触发，见 `tui-implementation-decisions.md` §5.9）。

## 配置

| 文件 | 说明 |
|------|------|
| `~/.config/hiptty/settings.json` | UI 设置 |
| `~/.config/hiptty/{profile}.credentials.json` | 登录凭证 |
| `~/.config/hiptty/{profile}.session.json` | HTTP cookie 会话 |

## 文档

| 文档 | 内容 |
|------|------|
| [`AGENTS.md`](AGENTS.md) | Agent 协作入口 |
| [`docs/README.md`](docs/README.md) | 文档索引与更新规则 |
| [`docs/architecture.md`](docs/architecture.md) | Crate 分层与数据流 |
| [`docs/tui-implementation-decisions.md`](docs/tui-implementation-decisions.md) | TUI 历史决策、技术备注、待补充项 |
| [`docs/api.md`](docs/api.md) | CLI API 参考 |
| [`docs/archive/`](docs/archive/) | 历史设计稿与调研（只读） |

## License

MIT