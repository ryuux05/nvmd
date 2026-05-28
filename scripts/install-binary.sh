#!/usr/bin/env sh
set -eu

repo="${NVMD_REPO:-ryuux05/nvmd}"
version="${NVMD_VERSION:-latest}"
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  MINGW*|MSYS*|CYGWIN*) os="windows" ;;
  *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64|AMD64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "unsupported CPU architecture: $(uname -m)" >&2; exit 1 ;;
esac

asset="nvmd-${os}-${arch}.tar.gz"
if [ "$version" = "latest" ]; then
  url="https://github.com/${repo}/releases/latest/download/${asset}"
else
  url="https://github.com/${repo}/releases/download/${version}/${asset}"
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

mkdir -p "$root/bin"
curl -fL "$url" -o "$tmpdir/$asset"
tar -xzf "$tmpdir/$asset" -C "$root/bin"
chmod +x "$root/bin/nvmd" "$root/bin/nvmd.exe" 2>/dev/null || true

echo "installed $asset to $root/bin"
