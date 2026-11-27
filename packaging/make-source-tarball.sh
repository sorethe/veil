#!/usr/bin/env bash
# Create packaging/veil-<pkgver>.tar.gz from the working tree for makepkg.
# Includes tracked and untracked non-ignored files (so uncommitted changes are
# packaged). Excludes ignored build/vendor output, packaging/ itself, and the
# local (git-ignored) src-tauri/resources/sing-box binary; the PKGBUILD fetches
# the pinned official upstream core instead.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKGDIR="$ROOT/packaging"
pkgver="$(sed -n 's/^pkgver=//p' "$PKGDIR/PKGBUILD" | head -n1)"
name="veil-$pkgver"
out="$PKGDIR/$name.tar.gz"
mtime="${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct)}"

cd "$ROOT"
git ls-files -z -co --exclude-standard \
  | grep -z -v -e '^packaging/' -e '^src-tauri/resources/sing-box$' \
  > "$PKGDIR/.filelist.$$"

tar --null --no-recursion -T "$PKGDIR/.filelist.$$" \
  --owner=0 --group=0 --numeric-owner --mtime="@$mtime" \
  --transform "s,^,$name/," -cf - \
  | gzip -n > "$out"
rm -f "$PKGDIR/.filelist.$$"

echo "wrote $out"
sha256sum "$out"
