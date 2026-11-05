#!/usr/bin/env bash
# Builds the Arch Linux package from the .deb that build-linux.sh made:
#   dist/AICStation-v<version>-linux-x86_64.pkg.tar.zst
#   dist/PKGBUILD             (the same recipe, for the AUR: aicstation-bin)
#   dist/SHA256SUMS-arch.txt
# Needs makepkg (Arch or an archlinux container) and must not run as root.
# The dependencies are not checked here (--nodeps): they are only needed to
# run the app, not to repackage it.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -n1)"
[ -n "$version" ] || { echo "no version in tauri.conf.json" >&2; exit 1; }
deb="dist/AICStation-v${version}-linux-amd64.deb"
[ -f "$deb" ] || { echo "$deb not found: run scripts/build-linux.sh first" >&2; exit 1; }
[ "$(id -u)" -ne 0 ] || { echo "makepkg refuses to run as root" >&2; exit 1; }

sum="$(sha256sum "$deb" | cut -d' ' -f1)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
sed -e "s/@VERSION@/${version}/" -e "s/@SHA256@/${sum}/" packaging/arch/PKGBUILD.in > "$work/PKGBUILD"
# makepkg uses a source file it finds next to the PKGBUILD instead of downloading it.
cp "$deb" "$work/"
(cd "$work" && PKGEXT=.pkg.tar.zst PKGDEST="$work/out" makepkg --nodeps --force --cleanbuild --noconfirm)

pkg=( "$work"/out/aicstation-bin-"${version}"-1-x86_64.pkg.tar.zst )
[ -f "${pkg[0]}" ] || { echo "makepkg made no package" >&2; exit 1; }
out="dist/AICStation-v${version}-linux-x86_64.pkg.tar.zst"
cp "${pkg[0]}" "$out"
cp "$work/PKGBUILD" dist/PKGBUILD

# The package must hold the app, butaglyph and the license.
list="$(bsdtar -tf "$out")"
for f in usr/bin/aicstation usr/bin/buta usr/share/applications/AICStation.desktop \
  "usr/share/licenses/aicstation-bin/LICENSE"; do
  grep -qx "$f" <<<"$list" || { echo "missing from the package: $f" >&2; exit 1; }
done

(cd dist && sha256sum "AICStation-v${version}-linux-x86_64.pkg.tar.zst" PKGBUILD > SHA256SUMS-arch.txt)
cat dist/SHA256SUMS-arch.txt
