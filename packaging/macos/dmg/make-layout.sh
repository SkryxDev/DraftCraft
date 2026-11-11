#!/usr/bin/env bash
# Remakes the DMG window's look, which packaging/macos/package.sh copies into every disk image:
#
#   packaging/macos/dmg/background.png    the window background (from background.swift), and
#   packaging/macos/dmg/background@2x.png  its Retina version; package.sh joins them into
#                                          /.background/background.tiff on the volume
#   packaging/macos/dmg/DS_Store          Finder's view settings for the volume: window size, no
#                                         toolbar or sidebar, 128 pt icons at the cards' centres, and
#                                         the background (an alias to /.background/background.tiff
#                                         on a volume named DraftCraft)
#
# Run it on a Mac after changing the art or the layout, then commit both files. Releases don't run
# it: they only copy the files, so building a DMG needs no Python and mounts nothing.
#
#   packaging/macos/dmg/make-layout.sh <dir with Inter-Medium.ttf and Inter-Regular.ttf>
#
# Needs Xcode's swift and Python 3 with ds_store 1.3.1 and mac_alias 2.2.2 (dmgbuild's libraries;
# `pip install ds_store==1.3.1 mac_alias==2.2.2`, or PYTHON=<a venv's python>).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FONTS="${1:?usage: make-layout.sh <Inter font dir>}"
PYTHON="${PYTHON:-python3}"

# Keep in step with background.swift: window size and icon centres, in points.
WIDTH=660
HEIGHT=420
# Finder counts the title bar in the window height.
TITLE_BAR=28
APP_X=170
APPS_X=490
ICON_Y=236

WORK="$(mktemp -d)"
cleanup() {
  if [[ -n "${DEVICE:-}" ]]; then hdiutil detach -quiet "$DEVICE" || hdiutil detach -quiet -force "$DEVICE" || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

swift "$HERE/background.swift" "$FONTS" "$HERE"

# A scratch volume with the same name and layout as the release DMG, so the alias inside the view
# settings points where the background will be.
hdiutil create -quiet -size 8m -fs HFS+ -volname DraftCraft -type UDIF -layout NONE "$WORK/layout.dmg"
MOUNT="$WORK/mnt"
mkdir -p "$MOUNT"
DEVICE="$(hdiutil attach "$WORK/layout.dmg" -nobrowse -noautoopen -noverify -mountpoint "$MOUNT" | awk 'NR==1{print $1}')"
mkdir -p "$MOUNT/.background" "$MOUNT/DraftCraft.app"
# One TIFF with both resolutions, as package.sh makes it: Finder picks the 2x image on Retina displays.
tiffutil -cathidpicheck "$HERE/background.png" "$HERE/background@2x.png" -out "$MOUNT/.background/background.tiff"
ln -s /Applications "$MOUNT/Applications"

"$PYTHON" - "$MOUNT" "$WIDTH" "$((HEIGHT + TITLE_BAR))" "$APP_X" "$APPS_X" "$ICON_Y" <<'PY'
import sys
from ds_store import DSStore
from mac_alias import Alias

mount, width, height, app_x, apps_x, icon_y = sys.argv[1], *map(int, sys.argv[2:])
alias = Alias.for_file(f"{mount}/.background/background.tiff")
# for_file records where the scratch volume is mounted here (a temporary folder). Point the alias at
# the release volume instead: Finder finds the background by these paths, since the release image is a
# new volume with its own creation date. The file ids are the ones `hdiutil makehybrid` gives
# /.background and its TIFF on the release volume (package.sh stages the same top-level entries).
alias.volume.posix_path = "/Volumes/DraftCraft"
alias.target.posix_path = "/.background/background.tiff"
alias.target.carbon_path = b"DraftCraft:.background:\x00background.tiff"
alias.target.folder_cnid = 20
alias.target.cnid = 22
alias.target.cnid_path = (20,)
with DSStore.open(f"{mount}/.DS_Store", "w+") as store:
    store["."]["bwsp"] = {
        "WindowBounds": f"{{{{200, 160}}, {{{width}, {height}}}}}",
        "ShowStatusBar": False,
        "ShowToolbar": False,
        "ShowTabView": False,
        "ShowPathbar": False,
        "ShowSidebar": False,
        "ContainerShowSidebar": False,
        "PreviewPaneVisibility": False,
        "SidebarWidth": 0,
    }
    store["."]["icvp"] = {
        "viewOptionsVersion": 1,
        "backgroundType": 2,
        "backgroundImageAlias": alias.to_bytes(),
        "backgroundColorRed": 1.0,
        "backgroundColorGreen": 1.0,
        "backgroundColorBlue": 1.0,
        "arrangeBy": "none",
        "gridOffsetX": 0.0,
        "gridOffsetY": 0.0,
        "gridSpacing": 100.0,
        "iconSize": 128.0,
        "textSize": 13.0,
        "labelOnBottom": True,
        "showIconPreview": False,
        "showItemInfo": False,
        "scrollPositionX": 0.0,
        "scrollPositionY": 0.0,
    }
    store["."]["vSrn"] = ("long", 1)
    store["."]["icvl"] = ("type", b"icnv")
    store["DraftCraft.app"]["Iloc"] = (app_x, icon_y)
    store["Applications"]["Iloc"] = (apps_x, icon_y)
PY

cp "$MOUNT/.DS_Store" "$HERE/DS_Store"
echo "wrote $HERE/background.png, background@2x.png and DS_Store"
