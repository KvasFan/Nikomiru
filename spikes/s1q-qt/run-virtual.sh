#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs spike S1q in an off-screen virtual KWin (see ADR 0001 and ADR 0003).
# Usage: run-virtual.sh [--size WxH] [--xwayland] <spike arguments>
#   QT_QPA_PLATFORM=xcb with --xwayland tests X11; QT_XCB_GL_INTEGRATION=xcb_egl enables EGL there.
set -eu
if [ "${S1_INNER:-}" = 1 ]; then
    exec "$S1_BIN" $S1_ARGS
fi
here=$(cd "$(dirname "$0")" && pwd)
width=3840; height=2160; xwayland=""
while [ $# -gt 0 ]; do
    case "$1" in
        --size) width=${2%x*}; height=${2#*x}; shift 2 ;;
        --xwayland) xwayland=--xwayland; shift ;;
        *) break ;;
    esac
done
export S1_INNER=1 S1_BIN="$here/target/release/spike-s1q-qt" S1_ARGS="$*" QT_FORCE_STDERR_LOGGING=1
exec env -u DISPLAY -u WAYLAND_DISPLAY dbus-run-session -- \
    kwin_wayland --virtual --width "$width" --height "$height" --scale 1 \
    --socket "wayland-s1q-$$" $xwayland --no-lockscreen --no-global-shortcuts --no-kactivities \
    --exit-with-session "$here/run-virtual.sh"
