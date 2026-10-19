#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs spike S1 inside an off-screen virtual KWin with a separate D-Bus bus,
# without touching the running graphical session (docs/adr/0001-video-composition.md).
#
# Usage: [GDK_BACKEND=wayland|x11] run-virtual.sh [--scale 2] [--xwayland] <spike arguments>
# With GDK_BACKEND=x11, --xwayland is added automatically.
set -eu
if [ "${S1_INNER:-}" = 1 ]; then
    # inside the KWin session: WAYLAND_DISPLAY (and DISPLAY with --xwayland) are already set
    exec "$S1_BIN" $S1_ARGS
fi
here=$(cd "$(dirname "$0")" && pwd)
scale=1
xwayland=""
[ "${GDK_BACKEND:-}" = x11 ] && xwayland=--xwayland
while [ $# -gt 0 ]; do
    case "$1" in
        --scale) scale=$2; shift 2 ;;
        --xwayland) xwayland=--xwayland; shift ;;
        *) break ;;
    esac
done
width=$((1920 * scale)); height=$((1080 * scale))
export S1_INNER=1 S1_BIN="$here/target/release/spike-s1-composition" S1_ARGS="$*"
exec env -u DISPLAY -u WAYLAND_DISPLAY dbus-run-session -- \
    kwin_wayland --virtual --width "$width" --height "$height" --scale "$scale" \
    --socket "wayland-s1-$$" $xwayland --no-lockscreen --no-global-shortcuts --no-kactivities \
    --exit-with-session "$here/run-virtual.sh"
