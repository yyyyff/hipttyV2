#!/usr/bin/env bash
# hiptty 一键安装 / 卸载脚本（macOS / Linux / Windows Git Bash）
#
# 用法：
#   curl -fsSL https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.sh | bash
#   curl -fsSL ... | bash -s -- --uninstall
#   HIPTTY_INSTALL_DIR=/opt/hiptty bash install.sh
#
# 环境变量：
#   HIPTTY_INSTALL_DIR  安装目录
#                       默认：Unix 为 ~/.local/bin；Windows 为 %LOCALAPPDATA%/hiptty
#   HIPTTY_VERSION      版本 tag，如 v0.1.0（默认 latest）
#   HIPTTY_REPO         仓库 owner/repo（默认 yyyyff/hipttyV2）
#   HIPTTY_FORCE        设为 1 时强制重装、跳过已安装菜单
#
# Windows 原生 PowerShell 用户请用：
#   irm https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.ps1 | iex

set -euo pipefail

REPO="${HIPTTY_REPO:-yyyyff/hipttyV2}"
VERSION="${HIPTTY_VERSION:-latest}"
FORCE="${HIPTTY_FORCE:-0}"
GITHUB_DL="https://github.com"

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
YELLOW=$'\033[0;33m'
BOLD=$'\033[1m'
RESET=$'\033[0m'

info()  { printf '%s\n' "$*"; }
ok()    { printf '%b%s%b\n' "$GREEN" "$*" "$RESET"; }
warn()  { printf '%b%s%b\n' "$YELLOW" "$*" "$RESET"; }
err()   { printf '%b%s%b\n' "$RED" "$*" "$RESET" >&2; }
die()   { err "错误：$*"; exit 1; }

is_windows() {
  case "$(uname -s 2>/dev/null || true)" in
    MINGW*|MSYS*|CYGWIN*) return 0 ;;
    *) return 1 ;;
  esac
}

# 默认安装目录（可被 HIPTTY_INSTALL_DIR 覆盖）
default_install_dir() {
  if [[ -n "${HIPTTY_INSTALL_DIR:-}" ]]; then
    printf '%s' "$HIPTTY_INSTALL_DIR"
    return
  fi
  if is_windows; then
    if [[ -n "${LOCALAPPDATA:-}" ]]; then
      # Git Bash 下 LOCALAPPDATA 形如 C:\Users\...\AppData\Local
      printf '%s' "$(cygpath -u "$LOCALAPPDATA" 2>/dev/null || echo "$LOCALAPPDATA")/hiptty"
    else
      printf '%s' "${HOME}/AppData/Local/hiptty"
    fi
  else
    printf '%s' "${HOME}/.local/bin"
  fi
}

INSTALL_DIR="$(default_install_dir)"

exe_suffix() {
  if is_windows; then
    printf '.exe'
  else
    printf ''
  fi
}

usage() {
  cat <<EOF
hiptty 安装脚本

用法：
  install.sh              安装或升级（默认）
  install.sh --uninstall  卸载（确认后删除）
  install.sh --help       显示本帮助

环境变量：
  HIPTTY_INSTALL_DIR  安装目录
                      默认：Unix ~/.local/bin；Windows %LOCALAPPDATA%/hiptty
  HIPTTY_VERSION      版本 tag，如 v0.1.0，或 latest（默认）
  HIPTTY_REPO         GitHub 仓库（默认 yyyyff/hipttyV2）
  HIPTTY_FORCE        设为 1 时已安装则直接重装

Windows 原生 PowerShell：
  irm https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.ps1 | iex
EOF
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "缺少命令：$1"
}

can_prompt() {
  [[ -t 0 ]] || [[ -f "${BASH_SOURCE[0]:-}" ]] || [[ -r /dev/tty ]]
}

# 读取确认输入：
# - 交互终端：stdin
# - bash install.sh + 管道喂答案：stdin
# - curl | bash -s：从 /dev/tty 读（stdin 是脚本本身）
read_prompt() {
  local prompt="$1"
  local reply=""
  if [[ -t 0 ]]; then
    read -r -p "$prompt" reply || true
  elif [[ -f "${BASH_SOURCE[0]:-}" ]]; then
    printf '%s' "$prompt" >&2
    read -r reply || true
    printf '\n' >&2
  elif [[ -r /dev/tty ]]; then
    # shellcheck disable=SC2162
    read -r -p "$prompt" reply </dev/tty || true
  else
    die "需要交互式终端才能确认（安装可设 HIPTTY_FORCE=1 跳过）"
  fi
  printf '%s' "$reply"
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin)
      case "$arch" in
        arm64|aarch64) echo "aarch64-apple-darwin" ;;
        x86_64)        echo "x86_64-apple-darwin" ;;
        *) die "不支持的 macOS 架构：$arch" ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64|amd64)  echo "x86_64-unknown-linux-gnu" ;;
        aarch64|arm64) die "暂未发布 Linux aarch64 预编译包，请从源码构建或提 issue" ;;
        *) die "不支持的 Linux 架构：$arch" ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*)
      case "$arch" in
        x86_64|amd64|i686|i386)
          # 目前只发布 x86_64-msvc；32 位与 arm64 Windows 未提供
          if [[ "$arch" == i686 || "$arch" == i386 ]]; then
            die "暂未发布 32 位 Windows 预编译包"
          fi
          echo "x86_64-pc-windows-msvc"
          ;;
        aarch64|arm64) die "暂未发布 Windows ARM64 预编译包" ;;
        *) die "不支持的 Windows 架构：$arch" ;;
      esac
      ;;
    *)
      die "不支持的操作系统：$os"
      ;;
  esac
}

archive_ext() {
  if is_windows; then
    printf 'zip'
  else
    printf 'tar.gz'
  fi
}

bin_hiptty()     { printf '%s/hiptty%s' "$INSTALL_DIR" "$(exe_suffix)"; }
bin_hiptty_cli() { printf '%s/hiptty-cli%s' "$INSTALL_DIR" "$(exe_suffix)"; }

is_installed() {
  [[ -f "$(bin_hiptty)" ]] || [[ -f "$(bin_hiptty_cli)" ]] || \
    [[ -x "$(bin_hiptty)" ]] || [[ -x "$(bin_hiptty_cli)" ]]
}

installed_paths() {
  [[ -e "$(bin_hiptty)" ]]     && printf '%s\n' "$(bin_hiptty)"
  [[ -e "$(bin_hiptty_cli)" ]] && printf '%s\n' "$(bin_hiptty_cli)"
}

installed_version() {
  local bin
  bin="$(bin_hiptty)"
  if [[ -f "$bin" ]]; then
    if out="$("$bin" --version 2>/dev/null)"; then
      printf '%s' "$out"
      return
    fi
  fi
  printf '未知（位于 %s）' "$INSTALL_DIR"
}

ensure_install_dir() {
  mkdir -p "$INSTALL_DIR"
}

path_hint() {
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) return 0 ;;
  esac
  # Windows Git Bash：路径可能以不同形式出现在 PATH
  if is_windows; then
    local win_dir
    win_dir="$(cygpath -w "$INSTALL_DIR" 2>/dev/null || echo "$INSTALL_DIR")"
    case ";${PATH};" in
      *";${INSTALL_DIR};"*|*"${win_dir}"*) return 0 ;;
    esac
  fi
  warn ""
  warn "注意：${INSTALL_DIR} 不在 PATH 中。"
  if is_windows; then
    warn "可在「系统属性 → 环境变量」中把该目录加入用户 PATH，或在 Git Bash 的 ~/.bashrc 中加入："
    info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    warn "PowerShell 用户也可用 install.ps1，它可自动写入用户 PATH。"
  else
    warn "请把下面一行加入 shell 配置（~/.zshrc 或 ~/.bashrc）："
    info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  fi
}

download() {
  local url="$1" dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --retry-delay 1 -o "$dest" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$dest" "$url"
  else
    die "需要 curl 或 wget 才能下载发布包"
  fi
}

resolve_asset_url() {
  local target="$1"
  local ext archive url
  ext="$(archive_ext)"
  archive="hiptty-${target}.${ext}"

  if [[ "$VERSION" == "latest" ]]; then
    url="${GITHUB_DL}/${REPO}/releases/latest/download/${archive}"
  else
    local tag="$VERSION"
    [[ "$tag" == v* ]] || tag="v${tag}"
    url="${GITHUB_DL}/${REPO}/releases/download/${tag}/${archive}"
  fi
  printf '%s' "$url"
}

extract_archive() {
  local archive_path="$1" dest_dir="$2"
  case "$archive_path" in
    *.tar.gz)
      need_cmd tar
      tar -xzf "$archive_path" -C "$dest_dir"
      ;;
    *.zip)
      if command -v unzip >/dev/null 2>&1; then
        unzip -qo "$archive_path" -d "$dest_dir"
      elif command -v powershell.exe >/dev/null 2>&1; then
        # Git Bash：用 PowerShell 解压
        local win_zip win_dest
        win_zip="$(cygpath -w "$archive_path" 2>/dev/null || echo "$archive_path")"
        win_dest="$(cygpath -w "$dest_dir" 2>/dev/null || echo "$dest_dir")"
        powershell.exe -NoProfile -Command \
          "Expand-Archive -LiteralPath '$win_zip' -DestinationPath '$win_dest' -Force" \
          >/dev/null
      else
        die "解压 zip 需要 unzip 或 powershell.exe"
      fi
      ;;
    *)
      die "未知压缩格式：$archive_path"
      ;;
  esac
}

find_staging_dir() {
  local tmpdir="$1"
  local name="hiptty$(exe_suffix)"
  local staging_bin
  staging_bin="$(find "$tmpdir" -type f -name "$name" 2>/dev/null | head -n1 || true)"
  [[ -n "$staging_bin" ]] || die "压缩包中未找到 ${name}"
  dirname "$staging_bin"
}

do_install() {
  local target archive url tmpdir staging ext
  need_cmd uname
  need_cmd mkdir
  need_cmd mktemp
  need_cmd cp
  need_cmd rm
  need_cmd find
  need_cmd head

  target="$(detect_target)"
  ext="$(archive_ext)"
  archive="hiptty-${target}.${ext}"
  url="$(resolve_asset_url "$target")"

  info "正在安装 hiptty"
  info "  仓库：  ${REPO}"
  info "  版本：  ${VERSION}"
  info "  目标：  ${target}"
  info "  目录：  ${INSTALL_DIR}"
  info "  地址：  ${url}"
  info ""

  ensure_install_dir
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/hiptty-install.XXXXXX")"
  # shellcheck disable=SC2064
  trap "rm -rf '${tmpdir}'" EXIT

  info "正在下载…"
  if ! download "$url" "${tmpdir}/${archive}"; then
    die "下载失败。
  请确认已发布且包含 ${archive}
  发布页：${GITHUB_DL}/${REPO}/releases"
  fi

  info "正在解压…"
  extract_archive "${tmpdir}/${archive}" "$tmpdir"

  staging="$(find_staging_dir "$tmpdir")"
  local bin_name="hiptty$(exe_suffix)"
  local cli_name="hiptty-cli$(exe_suffix)"
  [[ -f "${staging}/${bin_name}" ]] || die "压缩包缺少 ${bin_name}"
  [[ -f "${staging}/${cli_name}" ]] || die "压缩包缺少 ${cli_name}"

  info "正在安装到 ${INSTALL_DIR}…"
  cp "${staging}/${bin_name}" "$(bin_hiptty)"
  cp "${staging}/${cli_name}" "$(bin_hiptty_cli)"
  if ! is_windows; then
    chmod 755 "$(bin_hiptty)" "$(bin_hiptty_cli)"
  fi

  ok "已安装："
  info "  $(bin_hiptty)"
  info "  $(bin_hiptty_cli)"
  path_hint
  info ""
  ok "完成。运行：hiptty"
}

# 接受 y/yes/是
confirm_yes() {
  local prompt="$1"
  local reply
  reply="$(read_prompt "$prompt")"
  case "$reply" in
    y|Y|yes|YES|Yes|是) return 0 ;;
    *) return 1 ;;
  esac
}

config_dir_path() {
  if [[ -n "${XDG_CONFIG_HOME:-}" ]]; then
    printf '%s/hiptty' "$XDG_CONFIG_HOME"
  else
    printf '%s/.config/hiptty' "$HOME"
  fi
}

do_uninstall() {
  local paths=()
  local p config_dir

  while IFS= read -r p; do
    [[ -n "$p" ]] && paths+=("$p")
  done < <(installed_paths)

  if [[ ${#paths[@]} -eq 0 ]]; then
    warn "在 ${INSTALL_DIR} 下没有可卸载的文件"
    info "  查找路径：$(bin_hiptty)"
    info "            $(bin_hiptty_cli)"
    exit 0
  fi

  config_dir="$(config_dir_path)"

  info "${BOLD}卸载 hiptty${RESET}"
  info ""
  info "将删除以下文件："
  for p in "${paths[@]}"; do
    info "  - ${p}"
  done
  info ""
  info "默认不会删除用户数据："
  info "  - ${config_dir}/   （设置、登录凭证、会话）"
  info ""

  if ! confirm_yes "确认继续卸载？[y/N] "; then
    warn "已取消。"
    exit 0
  fi

  for p in "${paths[@]}"; do
    rm -f "$p"
    ok "已删除 ${p}"
  done

  if [[ -d "$config_dir" ]]; then
    info ""
    if confirm_yes "是否同时删除配置目录 ${config_dir}？[y/N] "; then
      rm -rf "$config_dir"
      ok "已删除 ${config_dir}"
    else
      info "已保留配置目录。"
    fi
  fi

  info ""
  ok "卸载完成。"
}

interactive_when_installed() {
  info "检测到 hiptty 已安装于 ${INSTALL_DIR}"
  if [[ -f "$(bin_hiptty)" ]]; then
    info "  当前版本：$(installed_version)"
  fi
  info ""
  info "请选择操作："
  info "  1) 重新安装 / 升级  （默认）"
  info "  2) 卸载"
  info "  3) 取消"
  local choice
  choice="$(read_prompt "请输入 [1/2/3]：")"
  case "${choice:-1}" in
    2) do_uninstall ;;
    3|q|Q|n|N) warn "已取消。"; exit 0 ;;
    1|"") do_install ;;
    *) die "无效选项：${choice}" ;;
  esac
}

main() {
  local mode="install"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      -h|--help|help) usage; exit 0 ;;
      -u|--uninstall|uninstall) mode="uninstall"; shift ;;
      -y|--yes) FORCE=1; shift ;;
      *) die "未知参数：$1（可用 --help）" ;;
    esac
  done

  case "$mode" in
    uninstall)
      do_uninstall
      ;;
    install)
      if is_installed && [[ "$FORCE" != "1" ]]; then
        if can_prompt; then
          interactive_when_installed
        else
          warn "已安装于 ${INSTALL_DIR}；非交互模式，直接重新安装。"
          do_install
        fi
      else
        do_install
      fi
      ;;
  esac
}

main "$@"
