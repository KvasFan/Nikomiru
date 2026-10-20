#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs a spike S5 binary in an off-screen virtual KWin (see ADR 0001 and
# ADR 0003). Usage: run-virtual.sh [--size WxH] [--shot FILE SECONDS] <binary> <arguments>
set -eu
if [ "${S5_INNER:-}" = 1 ]; then
    if [ -n "${S5_SHOT:-}" ]; then
        (sleep "$S5_SHOT_AFTER"; spectacle -b -n -f -o "$S5_SHOT" >/dev/null 2>&1) &
    fi
    exec $S5_BIN $S5_ARGS
fi
width=3840; height=2160; shot=""; after=0
while [ $# -gt 0 ]; do
    case "$1" in
        --size) width=${2%x*}; height=${2#*x}; shift 2 ;;
        --shot) shot=$2; after=$3; shift 3 ;;
        *) break ;;
    esac
done
bin=$1; shift
export S5_INNER=1 S5_BIN="$bin" S5_ARGS="$*" S5_SHOT="$shot" S5_SHOT_AFTER="$after"
exec env -u DISPLAY -u WAYLAND_DISPLAY dbus-run-session -- \
    kwin_wayland --virtual --width "$width" --height "$height" --scale 1 \
    --socket "wayland-s5-$$" --no-lockscreen --no-global-shortcuts --no-kactivities \
    --exit-with-session "$0"
