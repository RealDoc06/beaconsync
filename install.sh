#!/usr/bin/env bash
# Installs the `beacon` CLI (Linux and macOS) from GitHub releases:
#
#   curl -fsSL https://github.com/RealDoc06/beaconsync/releases/latest/download/install.sh | bash
#
# A version can be pinned: `... | bash -s -- 0.1.0-alpha.4`.
# BEACON_INSTALL_DIR overrides where it goes (default: /usr/local/bin as root, else ~/.local/bin).
set -euo pipefail

REPO="RealDoc06/beaconsync"
version="${1:-}"

die() { echo "error: $*" >&2; exit 1; }
command -v curl >/dev/null || die "curl is required"

case "$(uname -s)" in
  Linux) os="unknown-linux-musl" ;;
  Darwin) os="apple-darwin" ;;
  *) die "unsupported OS $(uname -s). On Windows download beacon-x86_64-pc-windows-msvc.exe from https://github.com/$REPO/releases" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  aarch64 | arm64) arch="aarch64" ;;
  *) die "unsupported architecture $(uname -m)" ;;
esac
# Rosetta reports x86_64; prefer the native build on Apple silicon.
[[ "$os" == apple-darwin && "$arch" == x86_64 && "$(sysctl -n sysctl.proc_translated 2>/dev/null)" == 1 ]] && arch="aarch64"
asset="beacon-$arch-$os"

if [[ -z "$version" ]]; then
  api="https://api.github.com/repos/$REPO/releases/latest"
else
  api="https://api.github.com/repos/$REPO/releases/tags/v${version#v}"
fi
release="$(curl -fsSL -H "Accept: application/vnd.github+json" "$api")" || die "no published release at $api"
tag="$(grep -m1 '"tag_name"' <<<"$release" | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
# GitHub's sha256 for the asset: the first "digest" after its "name" line.
digest="$(awk -v n="\"name\": \"$asset\"" 'index($0, n) { found = 1 } found && /"digest"/ { print; exit }' <<<"$release" |
  sed -E 's/.*"sha256:([0-9a-f]+)".*/\1/')"
[[ -n "$digest" ]] || die "$tag has no $asset"

if [[ -n "${BEACON_INSTALL_DIR:-}" ]]; then
  dir="$BEACON_INSTALL_DIR"
elif [[ "$(id -u)" == 0 ]]; then
  dir="/usr/local/bin"
else
  dir="$HOME/.local/bin"
fi
mkdir -p "$dir"

tmp="$(mktemp "$dir/.beacon.XXXXXX")"
trap 'rm -f "$tmp"' EXIT
echo "Downloading beacon $tag ($asset)"
curl -fL --progress-bar -o "$tmp" "https://github.com/$REPO/releases/download/$tag/$asset"

if command -v sha256sum >/dev/null; then
  got="$(sha256sum "$tmp" | cut -d' ' -f1)"
else
  got="$(shasum -a 256 "$tmp" | cut -d' ' -f1)"
fi
[[ "$got" == "$digest" ]] || die "checksum mismatch for $asset"
chmod 755 "$tmp"
"$tmp" --version >/dev/null 2>&1 || die "the downloaded beacon does not run on this machine"
mv -f "$tmp" "$dir/beacon"
trap - EXIT

echo "Installed $("$dir/beacon" --version) to $dir/beacon"
case ":$PATH:" in
  *":$dir:"*) echo "Next: beacon login, then beacon server install" ;;
  *) echo "$dir is not on your PATH. Add it with:"
     echo "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.$(basename "${SHELL:-bash}")rc && export PATH=\"$dir:\$PATH\"" ;;
esac
