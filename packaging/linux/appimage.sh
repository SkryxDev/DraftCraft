#!/usr/bin/env bash
# Build DraftCraft's AppImage from release binaries.
#
#   packaging/linux/appimage.sh <directory with draftcraft and draftcraft-cli> <out.AppImage>
#
# The AppImage starts the app; with `cli` as its first argument it runs draftcraft-cli instead
# (`./draftcraft-linux-x86_64.AppImage cli info drawing.dwg`). APPIMAGETOOL names an appimagetool to
# use; without it the current one is downloaded. CRAFT_FONTS_DIR adds the embedded fonts' licences.
# ARCH defaults to x86_64.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$(cd "$1" && pwd)"
OUT="$2"
ARCH="${ARCH:-x86_64}"
ID="dev.skryx.draftcraft"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APP="$WORK/DraftCraft.AppDir"

mkdir -p "$APP/usr/bin" "$APP/usr/share/applications" "$APP/usr/share/icons" "$APP/usr/share/doc/draftcraft"
install -m 755 "$BIN/draftcraft" "$BIN/draftcraft-cli" "$APP/usr/bin/"
cp "$ROOT/packaging/linux/$ID.desktop" "$APP/usr/share/applications/"
cp "$ROOT/packaging/linux/$ID.desktop" "$APP/"
cp -R "$ROOT/assets/app-icon/hicolor" "$APP/usr/share/icons/"
cp "$ROOT/assets/app-icon/hicolor/256x256/apps/$ID.png" "$APP/$ID.png"
cp "$APP/$ID.png" "$APP/.DirIcon"
cp "$ROOT/README.md" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/NOTICE" "$ROOT/ATTRIBUTION.md" "$APP/usr/share/doc/draftcraft/"
if [[ -n "${CRAFT_FONTS_DIR:-}" ]]; then
  for f in "$CRAFT_FONTS_DIR"/fonts/*/OFL.txt; do
    [[ -e "$f" ]] && cp "$f" "$APP/usr/share/doc/draftcraft/OFL-$(basename "$(dirname "$f")").txt"
  done
fi

cat >"$APP/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
if [ "${1:-}" = "cli" ]; then
  shift
  exec "$HERE/usr/bin/draftcraft-cli" "$@"
fi
exec "$HERE/usr/bin/draftcraft" "$@"
RUN
chmod 755 "$APP/AppRun"

TOOL="${APPIMAGETOOL:-}"
if [[ -z "$TOOL" ]]; then
  TOOL="$WORK/appimagetool"
  curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
  chmod 755 "$TOOL"
fi
# CI runners have no FUSE: the tool runs from its extracted files.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" --no-appstream "$APP" "$OUT"
ls -l "$OUT"
