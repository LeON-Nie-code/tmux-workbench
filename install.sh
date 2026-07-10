#!/usr/bin/env sh
set -eu

REPO="LeON-Nie-code/tmux-workbench"
VERSION="${TMUX_WORKBENCH_VERSION:-v0.1.2}"
BIN_NAME="ws"

say() {
  printf '%s\n' "$*"
}

fail() {
  say "error: $*" >&2
  exit 1
}

has() {
  command -v "$1" >/dev/null 2>&1
}

path_contains() {
  case ":$PATH:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

choose_install_dir() {
  if [ -n "${TMUX_WORKBENCH_INSTALL_DIR:-}" ]; then
    say "$TMUX_WORKBENCH_INSTALL_DIR"
    return
  fi

  if has "$BIN_NAME"; then
    existing="$(command -v "$BIN_NAME")"
    existing_dir="$(dirname "$existing")"
    if [ ! -L "$existing" ] && ! is_homebrew_prefix "$existing_dir" && can_install_to "$existing_dir"; then
      say "$existing_dir"
      return
    fi
  fi

  for dir in "$HOME/.local/bin" "$HOME/bin"; do
    if path_contains "$dir" && can_install_to "$dir"; then
      say "$dir"
      return
    fi
  done

  say "$HOME/.local/bin"
}

is_homebrew_prefix() {
  case "$1" in
    /opt/homebrew/bin | /usr/local/bin) return 0 ;;
    *) return 1 ;;
  esac
}

can_install_to() {
  dir="$1"
  if [ -d "$dir" ]; then
    [ -w "$dir" ]
  else
    [ -w "$(dirname "$dir")" ]
  fi
}

shell_hint() {
  dir="$1"
  shell_name="$(basename "${SHELL:-sh}")"
  case "$shell_name" in
    zsh) rc="$HOME/.zshrc" ;;
    bash) rc="$HOME/.bashrc" ;;
    fish)
      say "Add this to your fish config:"
      say "  fish_add_path $dir"
      return
      ;;
    *) rc="$HOME/.profile" ;;
  esac
  say "Add ws to your PATH:"
  say "  echo 'export PATH=\"$dir:\$PATH\"' >> $rc"
  say "  export PATH=\"$dir:\$PATH\""
}

os="$(uname -s)"
arch="$(uname -m)"

case "$os:$arch" in
  Darwin:arm64 | Darwin:aarch64) asset="ws-macos-aarch64" ;;
  Darwin:x86_64) asset="ws-macos-x86_64" ;;
  Linux:x86_64 | Linux:amd64) asset="ws-linux-x86_64" ;;
  *)
    fail "unsupported platform: $os $arch. Build from source with: cargo install --git https://github.com/$REPO ws"
    ;;
esac

INSTALL_DIR="$(choose_install_dir)"
url="https://github.com/$REPO/releases/download/$VERSION/$asset"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Installing Tmux Workbench $VERSION"
say "  platform: $os $arch"
say "  asset:    $asset"
say "  target:   $INSTALL_DIR/$BIN_NAME"

mkdir -p "$INSTALL_DIR" || fail "could not create $INSTALL_DIR"
[ -w "$INSTALL_DIR" ] || fail "$INSTALL_DIR is not writable. Set TMUX_WORKBENCH_INSTALL_DIR to a writable directory."

if has curl; then
  curl -fL --progress-bar "$url" -o "$tmp/$BIN_NAME" || fail "download failed: $url"
elif has wget; then
  wget -q --show-progress "$url" -O "$tmp/$BIN_NAME" || fail "download failed: $url"
else
  fail "curl or wget is required"
fi

chmod +x "$tmp/$BIN_NAME"
mv "$tmp/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"

say ""
say "Installed ws to $INSTALL_DIR/$BIN_NAME"

if path_contains "$INSTALL_DIR"; then
  "$INSTALL_DIR/$BIN_NAME" --version >/dev/null 2>&1 || "$INSTALL_DIR/$BIN_NAME" --help >/dev/null
  say "Run: ws"
else
  say ""
  shell_hint "$INSTALL_DIR"
  say ""
  say "Then run: ws"
fi
