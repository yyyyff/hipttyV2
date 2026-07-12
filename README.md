# hiptty

[4d4y](https://www.4d4y.com) 论坛终端客户端（Rust + Ratatui）。提供交互式 TUI 与 headless CLI 两种模式。

## 安装（普通用户）

### 一键安装

**macOS / Linux：**

```bash
curl -fsSL https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.sh | bash
```

**Windows（PowerShell，推荐）：**

```powershell
irm https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.ps1 | iex
```

**Windows（Git Bash）：** 同上可用 `install.sh`（会下载 `.zip` 并装到 `%LOCALAPPDATA%\hiptty`）。

| 平台 | 默认安装目录 |
|------|----------------|
| macOS / Linux | `~/.local/bin` |
| Windows | `%LOCALAPPDATA%\hiptty`（PowerShell 会尝试写入用户 PATH） |

**已安装时**：脚本会询问：

1. 重新安装 / 升级（默认）
2. 卸载（列出将删除的文件，并二次确认；配置目录默认保留）
3. 取消

**直接卸载：**

```bash
# macOS / Linux / Git Bash
curl -fsSL https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.sh | bash -s -- --uninstall
```

```powershell
# Windows PowerShell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.ps1))) -Uninstall
```

**可选环境变量：**

| 变量 | 默认 | 说明 |
|------|------|------|
| `HIPTTY_INSTALL_DIR` | 见上表 | 安装目录 |
| `HIPTTY_VERSION` | `latest` | 指定 tag，如 `v0.1.0` |
| `HIPTTY_FORCE` | （空） | 设为 `1` 时强制重装、跳过菜单 |

未做代码签名时，macOS Gatekeeper / Windows SmartScreen 可能提示未知发布者，属预期现象。

### 手动下载

所有平台预编译包见：[Releases](https://github.com/yyyyff/hipttyV2/releases)

| 资源 | 平台 |
|------|------|
| `hiptty-aarch64-apple-darwin.tar.gz` | macOS Apple Silicon |
| `hiptty-x86_64-apple-darwin.tar.gz` | macOS Intel |
| `hiptty-x86_64-unknown-linux-gnu.tar.gz` | Linux x86_64 |
| `hiptty-x86_64-pc-windows-msvc.zip` | Windows x86_64 |

### 从源码安装（需要 Rust）

```bash
cargo install --git https://github.com/yyyyff/hipttyV2 --locked --bin hiptty
cargo install --git https://github.com/yyyyff/hipttyV2 --locked --bin hiptty-cli
```

## 要求

- **运行预编译包**：无需安装 Rust
- **从源码构建**：Rust stable
- 支持 Kitty / iTerm2 / Sixel 图像协议的现代终端（Windows Terminal 上帖子大图走 Sixel，头像/表情走 Kitty）
- 推荐 Nerd Font 以正确显示图标

## 运行

### TUI

```bash
hiptty
# 开发时：
# cargo run -p hiptty
# cargo run -p hiptty --release
```

常用参数：

| 参数 | 环境变量 | 默认 | 说明 |
|------|----------|------|------|
| `--config DIR` | `HIPTTY_CONFIG` | `~/.config/hiptty` | 配置目录 |
| `--profile NAME` | `HIPTTY_PROFILE` | `default` | 会话/凭证 profile |

### CLI

默认输出 JSON（`schema_version: 1`）；加 `--human` 可读文本。

```bash
hiptty-cli auth status
hiptty-cli threads list --fid 2
hiptty-cli thread show 448060
# 开发时：cargo run -p hiptty-cli -- <args>
```

完整命令与 JSON 结构见 [`docs/api.md`](docs/api.md)。

## 配置

| 文件 | 说明 |
|------|------|
| `~/.config/hiptty/settings.json` | UI 设置 |
| `~/.config/hiptty/{profile}.credentials.json` | 登录凭证 |
| `~/.config/hiptty/{profile}.session.json` | HTTP cookie 会话 |

## 构建（开发者）

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

### 发布新版本

```bash
# 确认 main 干净且已 push
git tag v0.1.0
git push origin v0.1.0
```

推送 `v*` tag 后，[Release 工作流](.github/workflows/release.yml) 会自动构建多平台包并创建 GitHub Release。

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
