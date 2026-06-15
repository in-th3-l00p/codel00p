#!/bin/sh
# codel00p installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.sh | sh
#
# Environment overrides:
#   CODEL00P_INSTALL_DIR   install location (default: $HOME/.local/bin)
#   CODEL00P_VERSION       release tag to install (default: latest)
set -eu

REPO="in-th3-l00p/codel00p"
BIN="codel00p"
INSTALL_DIR="${CODEL00P_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${CODEL00P_VERSION:-latest}"

err() {
  echo "error: $*" >&2
  exit 1
}

# Resolve the Rust target triple for this host.
os="$(uname -s)"
arch="$(uname -m)"

case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) err "unsupported architecture: $arch" ;;
esac

case "$os" in
  Darwin) target="${arch}-apple-darwin" ;;
  Linux) target="${arch}-unknown-linux-gnu" ;;
  *) err "unsupported OS: $os (Windows users: see install.ps1)" ;;
esac

asset="${BIN}-${target}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${asset}"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
fi

# Pick a downloader.
if command -v curl >/dev/null 2>&1; then
  download() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  download() { wget -qO "$2" "$1"; }
else
  err "need curl or wget to download codel00p"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading codel00p ($target)..."
download "$url" "$tmp/$asset" || err "download failed: $url"

tar -xzf "$tmp/$asset" -C "$tmp" || err "failed to extract $asset"
[ -f "$tmp/$BIN" ] || err "archive did not contain $BIN"

mkdir -p "$INSTALL_DIR"
install -m 0755 "$tmp/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null ||
  { mv "$tmp/$BIN" "$INSTALL_DIR/$BIN" && chmod 0755 "$INSTALL_DIR/$BIN"; }

echo "Installed codel00p to $INSTALL_DIR/$BIN"

# Nudge the user if the install dir is not on PATH.
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo
    echo "Add it to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
