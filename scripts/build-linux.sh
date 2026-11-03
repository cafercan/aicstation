#!/usr/bin/env bash
# Builds the Linux packages (.deb, .rpm, AppImage), then writes SHA256SUMS.txt.
# Output, like the Windows installer script (build-installer.ps1):
#   dist/SessionStation-v<version>-linux-amd64.deb
#   dist/SessionStation-v<version>-linux-x86_64.rpm
#   dist/SessionStation-v<version>-linux-x86_64.AppImage
#   dist/SHA256SUMS.txt
# Packages built on a newer distro need its glibc; release builds come from
# the oldest supported Ubuntu (see .github/workflows/release-linux.yml).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(node -p 'require("./src-tauri/tauri.conf.json").version')"
case "$(uname -m)" in
  x86_64) ;;
  *) echo "only x86_64 is packaged" >&2; exit 1 ;;
esac

npx tauri build --bundles deb,rpm,appimage

bundle="target/release/bundle"
one() { # the single file matching a glob, or fail
  local f=( $1 )
  [ "${#f[@]}" -eq 1 ] && [ -f "${f[0]}" ] || { echo "expected one file for $1, got: ${f[*]}" >&2; exit 1; }
  printf '%s' "${f[0]}"
}
deb="$(one "$bundle/deb/SessionStation_${version}_amd64.deb")"
rpm="$(one "$bundle/rpm/SessionStation-${version}-1.x86_64.rpm")"
appimage="$(one "$bundle/appimage/SessionStation_${version}_amd64.AppImage")"

rm -rf dist
mkdir -p dist
cp "$deb" "dist/SessionStation-v${version}-linux-amd64.deb"
cp "$rpm" "dist/SessionStation-v${version}-linux-x86_64.rpm"
cp "$appimage" "dist/SessionStation-v${version}-linux-x86_64.AppImage"
chmod +x "dist/SessionStation-v${version}-linux-x86_64.AppImage"

(cd dist && sha256sum SessionStation-v"${version}"-linux-* > SHA256SUMS.txt)
cat dist/SHA256SUMS.txt
