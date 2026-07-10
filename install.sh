#!/usr/bin/env sh
set -eu

REPO="LeON-Nie-code/tmux-workbench"
VERSION="${TMUX_WORKBENCH_VERSION:-v0.1.1}"
INSTALL_DIR="${TMUX_WORKBENCH_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os:$arch" in
  Darwin:arm64) asset="ws-macos-aarch64" ;;
  Darwin:x86_64) asset="ws-macos-x86_64" ;;
  Linux:x86_64) asset="ws-linux-x86_64" ;;
  *)
    echo "unsupported platform: $os $arch" >&2
    exit 1
    ;;
esac

url="https://github.com/$REPO/releases/download/$VERSION/$asset"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$INSTALL_DIR"

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$tmp/ws"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$url" -O "$tmp/ws"
else
  echo "curl or wget is required" >&2
  exit 1
fi

chmod +x "$tmp/ws"
mv "$tmp/ws" "$INSTALL_DIR/ws"

echo "Installed ws to $INSTALL_DIR/ws"
echo "Make sure $INSTALL_DIR is in your PATH."
