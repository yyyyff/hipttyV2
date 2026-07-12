#!/usr/bin/env bash
# hiptty one-line installer / uninstaller
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.sh | bash
#   curl -fsSL ... | bash -s -- --uninstall
#   HIPTTY_INSTALL_DIR=/opt/hiptty bash install.sh
#
# Env:
#   HIPTTY_INSTALL_DIR  install prefix (default: ~/.local/bin)
#   HIPTTY_VERSION      release tag, e.g. v0.1.0 (default: latest)
#   HIPTTY_REPO         owner/repo (default: yyyyff/hipttyV2)
#   HIPTTY_FORCE        set to 1 to reinstall without interactive menu

set -euo pipefail

REPO="${HIPTTY_REPO:-yyyyff/hipttyV2}"
INSTALL_DIR="${HIPTTY_INSTALL_DIR:-${HOME}/.local/bin}"
VERSION="${HIPTTY_VERSION:-latest}"
FORCE="${HIPTTY_FORCE:-0}"
GITHUB_API="https://api.github.com"
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
die()   { err "error: $*"; exit 1; }

usage() {
  cat <<'EOF'
hiptty installer

Usage:
  install.sh              Install or upgrade hiptty (default)
  install.sh --uninstall  Uninstall (with confirmations)
  install.sh --help       Show this help

Environment:
  HIPTTY_INSTALL_DIR  Install directory (default: ~/.local/bin)
  HIPTTY_VERSION      Tag like v0.1.0, or "latest" (default)
  HIPTTY_REPO         GitHub owner/repo (default: yyyyff/hipttyV2)
  HIPTTY_FORCE        1 = reinstall without asking when already installed
EOF
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

# True when the user can answer prompts (tty, or answers piped into a file-backed script).
can_prompt() {
  [[ -t 0 ]] || [[ -f "${BASH_SOURCE[0]:-}" ]] || [[ -r /dev/tty ]]
}

# Read a line for confirmations.
# - Interactive terminal: stdin
# - `bash install.sh` with piped answers: stdin
# - `curl | bash -s`: controlling TTY (/dev/tty), because stdin is the script body
read_prompt() {
  local prompt="$1"
  local reply=""
  if [[ -t 0 ]]; then
    read -r -p "$prompt" reply || true
  elif [[ -f "${BASH_SOURCE[0]:-}" ]]; then
    # File-backed invocation; stdin may carry scripted answers (tests / automation).
    printf '%s' "$prompt" >&2
    read -r reply || true
    printf '\n' >&2
  elif [[ -r /dev/tty ]]; then
    # shellcheck disable=SC2162
    read -r -p "$prompt" reply </dev/tty || true
  else
    die "need an interactive terminal for confirmation (or set HIPTTY_FORCE=1 for install)"
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
        *) die "unsupported macOS architecture: $arch" ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64|amd64)  echo "x86_64-unknown-linux-gnu" ;;
        aarch64|arm64) die "Linux aarch64 builds are not published yet; download source or open an issue" ;;
        *) die "unsupported Linux architecture: $arch" ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*)
      die "Windows: please download the .zip from GitHub Releases and extract manually"
      ;;
    *)
      die "unsupported OS: $os"
      ;;
  esac
}

bin_hiptty()     { printf '%s/hiptty' "$INSTALL_DIR"; }
bin_hiptty_cli() { printf '%s/hiptty-cli' "$INSTALL_DIR"; }

is_installed() {
  [[ -x "$(bin_hiptty)" ]] || [[ -x "$(bin_hiptty_cli)" ]]
}

installed_paths() {
  local paths=()
  [[ -e "$(bin_hiptty)" ]]     && paths+=("$(bin_hiptty)")
  [[ -e "$(bin_hiptty_cli)" ]] && paths+=("$(bin_hiptty_cli)")
  printf '%s\n' "${paths[@]}"
}

installed_version() {
  local bin
  bin="$(bin_hiptty)"
  if [[ -x "$bin" ]]; then
    # Prefer --version if the binary supports it; fall back to path note.
    if out="$("$bin" --version 2>/dev/null)"; then
      printf '%s' "$out"
      return
    fi
  fi
  printf 'unknown (at %s)' "$INSTALL_DIR"
}

ensure_install_dir() {
  mkdir -p "$INSTALL_DIR"
}

path_hint() {
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) return 0 ;;
  esac
  warn ""
  warn "${INSTALL_DIR} is not on your PATH."
  warn "Add this to your shell config (~/.zshrc or ~/.bashrc):"
  info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
}

download() {
  local url="$1" dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --retry-delay 1 -o "$dest" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$dest" "$url"
  else
    die "need curl or wget to download releases"
  fi
}

resolve_asset_url() {
  local target="$1"
  local asset="hiptty-${target}.tar.gz"
  local url

  if [[ "$VERSION" == "latest" ]]; then
    url="${GITHUB_DL}/${REPO}/releases/latest/download/${asset}"
  else
    # Accept both v0.1.0 and 0.1.0
    local tag="$VERSION"
    [[ "$tag" == v* ]] || tag="v${tag}"
    url="${GITHUB_DL}/${REPO}/releases/download/${tag}/${asset}"
  fi
  printf '%s' "$url"
}

do_install() {
  local target archive url tmpdir staging
  need_cmd uname
  need_cmd tar
  need_cmd mkdir
  need_cmd mktemp
  need_cmd chmod
  need_cmd cp
  need_cmd rm

  target="$(detect_target)"
  archive="hiptty-${target}.tar.gz"
  url="$(resolve_asset_url "$target")"

  info "Installing hiptty"
  info "  repo:    ${REPO}"
  info "  version: ${VERSION}"
  info "  target:  ${target}"
  info "  dir:     ${INSTALL_DIR}"
  info "  url:     ${url}"
  info ""

  ensure_install_dir
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/hiptty-install.XXXXXX")"
  # shellcheck disable=SC2064
  trap "rm -rf '${tmpdir}'" EXIT

  info "Downloading..."
  if ! download "$url" "${tmpdir}/${archive}"; then
    die "download failed.
  Check that a release exists and publishes ${archive}
  Releases: ${GITHUB_DL}/${REPO}/releases"
  fi

  info "Extracting..."
  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

  # Archive layout: hiptty-<target>/{hiptty,hiptty-cli,...}
  staging_bin="$(find "$tmpdir" -type f -name hiptty 2>/dev/null | head -n1 || true)"
  [[ -n "$staging_bin" ]] || die "archive did not contain hiptty binary"
  staging="$(dirname "$staging_bin")"

  [[ -f "${staging}/hiptty" ]]     || die "missing hiptty in archive"
  [[ -f "${staging}/hiptty-cli" ]] || die "missing hiptty-cli in archive"

  info "Installing binaries to ${INSTALL_DIR}..."
  cp "${staging}/hiptty"     "$(bin_hiptty)"
  cp "${staging}/hiptty-cli" "$(bin_hiptty_cli)"
  chmod 755 "$(bin_hiptty)" "$(bin_hiptty_cli)"

  ok "Installed:"
  info "  $(bin_hiptty)"
  info "  $(bin_hiptty_cli)"
  path_hint
  info ""
  ok "Done. Run: hiptty"
}

confirm_yes() {
  local prompt="$1"
  local reply
  reply="$(read_prompt "$prompt")"
  case "$reply" in
    y|Y|yes|YES|Yes) return 0 ;;
    *) return 1 ;;
  esac
}

do_uninstall() {
  local paths=()
  local p config_dir

  while IFS= read -r p; do
    [[ -n "$p" ]] && paths+=("$p")
  done < <(installed_paths)

  if [[ ${#paths[@]} -eq 0 ]]; then
    warn "Nothing to uninstall under ${INSTALL_DIR}"
    info "  looked for: $(bin_hiptty)"
    info "              $(bin_hiptty_cli)"
    exit 0
  fi

  config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/hiptty"

  info "${BOLD}Uninstall hiptty${RESET}"
  info ""
  info "The following files will be removed:"
  for p in "${paths[@]}"; do
    info "  - ${p}"
  done
  info ""
  info "Not removed by default (user data):"
  info "  - ${config_dir}/   (settings, credentials, session)"
  info ""

  if ! confirm_yes "Continue with uninstall? [y/N] "; then
    warn "Aborted."
    exit 0
  fi

  info ""
  warn "Second confirmation required."
  info "Type ${BOLD}yes${RESET} to permanently delete the files listed above."
  local reply
  reply="$(read_prompt "Type 'yes' to confirm: ")"
  if [[ "$reply" != "yes" ]]; then
    warn "Aborted (expected exactly: yes)."
    exit 0
  fi

  for p in "${paths[@]}"; do
    rm -f "$p"
    ok "removed ${p}"
  done

  if [[ -d "$config_dir" ]]; then
    info ""
    if confirm_yes "Also delete config/data at ${config_dir}? [y/N] "; then
      info "Type ${BOLD}yes${RESET} to delete config/data (credentials & session included)."
      reply="$(read_prompt "Type 'yes' to confirm config deletion: ")"
      if [[ "$reply" == "yes" ]]; then
        rm -rf "$config_dir"
        ok "removed ${config_dir}"
      else
        warn "Kept config directory."
      fi
    else
      info "Kept config directory."
    fi
  fi

  info ""
  ok "Uninstall complete."
}

interactive_when_installed() {
  info "hiptty is already installed at ${INSTALL_DIR}"
  if [[ -x "$(bin_hiptty)" ]]; then
    info "  version: $(installed_version)"
  fi
  info ""
  info "What do you want to do?"
  info "  1) Reinstall / upgrade  (default)"
  info "  2) Uninstall"
  info "  3) Cancel"
  local choice
  choice="$(read_prompt "Choose [1/2/3]: ")"
  case "${choice:-1}" in
    2) do_uninstall ;;
    3|q|Q|n|N) warn "Cancelled."; exit 0 ;;
    1|"") do_install ;;
    *) die "invalid choice: ${choice}" ;;
  esac
}

main() {
  local mode="install"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      -h|--help) usage; exit 0 ;;
      -u|--uninstall) mode="uninstall"; shift ;;
      -y|--yes) FORCE=1; shift ;;
      *) die "unknown argument: $1 (try --help)" ;;
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
          warn "Already installed at ${INSTALL_DIR}; reinstalling (non-interactive)."
          do_install
        fi
      else
        do_install
      fi
      ;;
  esac
}

main "$@"
