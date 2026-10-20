#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
#
# Spike S5's measurements: S1q (Qt Quick), egui and iced on the same videos,
# in the same off-screen virtual KWin, one after the other for each case so
# that they share the machine's conditions. One JSON report per run in OUT,
# named <toolkit>-<case>-<repetition>.json.
#
# Usage: measure-all.sh OUT [POPUPS_FILE] [REPETITIONS]
# A real 2160p60 HEVC 10-bit source is taken from $S5_2160P (default
# target/fixtures/av_2160p60_hevc10.mkv); without it those cases are skipped.
# It can be made with:
#   ffmpeg -f lavfi -i testsrc2=size=3840x2160:rate=60 -f lavfi -i sine=frequency=440:sample_rate=48000 \
#     -t 30 -c:v libx265 -preset ultrafast -pix_fmt yuv420p10le -c:a aac -ac 1 av_2160p60_hevc10.mkv
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=$1
popups=${2:-}
reps=${3:-1}
mkdir -p "$out"
fx="$root/target/fixtures"
uhd=${S5_2160P:-$fx/av_2160p60_hevc10.mkv}
qt="$root/spikes/s1q-qt/target/release/spike-s1q-qt"
egui="$here/target/release/spike-s5-egui"
iced="$here/target/release/spike-s5-iced"
pids=""
trap 'kill $pids 2>/dev/null || true' EXIT

# busy loops at normal priority: FACTOR per hardware thread
load_start() {
    i=0
    while [ "$i" -lt $(( $(nproc) * $1 )) ]; do
        sh -c 'while :; do :; done' &
        pids="$pids $!"
        i=$((i + 1))
    done
    sleep 5
}
load_stop() {
    kill $pids 2>/dev/null || true
    pids=""
    sleep 5
}

# run TOOLKIT CASE OUTPUT_SIZE VIDEO [ENV...]
run() {
    tk=$1 name=$2 size=$3 video=$4
    shift 4
    eval bin=\$$tk
    [ -f "$video" ] || { echo "skipped $tk-$name: no $video" >&2; return; }
    env "$@" QT_FORCE_STDERR_LOGGING=1 timeout 180 "$here/run-virtual.sh" --size "$size" "$bin" "$video" --fullscreen --measure 20 \
        > "$out/$tk-$name.json" 2> "$out/$tk-$name.err" || echo "failed: $tk-$name" >&2
}

# each case for the three toolkits in turn
each() {
    for tk in qt egui iced; do
        run "$tk" "$@"
    done
}

r=1
while [ "$r" -le "$reps" ]; do
    each "1080-h264-$r" 1920x1080 "$fx/av_hd60_h264.mkv"
    each "1080to2160-h264-$r" 3840x2160 "$fx/av_hd60_h264.mkv"
    each "1080to2160-hevc10-$r" 3840x2160 "$fx/av_hd60_hevc10.mkv"
    each "2160-hevc10-$r" 3840x2160 "$uhd"
    # control: Qt's CPU without its overlay
    run qt "2160-hevc10-nooverlay-$r" 3840x2160 "$uhd" S1Q_NO_OVERLAY=1
    load_start 1
    each "2160-hevc10-load1-$r" 3840x2160 "$uhd"
    load_stop
    load_start 3
    each "1080to2160-hevc10-load3-$r" 3840x2160 "$fx/av_hd60_hevc10.mkv"
    each "2160-hevc10-load3-$r" 3840x2160 "$uhd"
    # control: egui with iced's copy-back decoding
    run egui "2160-hevc10-load3-copy-$r" 3840x2160 "$uhd" S5_MPV_OPTIONS=hwdec=auto-copy-safe
    load_stop
    r=$((r + 1))
done

if [ -n "$popups" ]; then
    for tk in egui iced; do
        eval bin=\$$tk
        timeout 300 "$here/run-virtual.sh" --size 1600x900 "$bin" "$fx/av_basic.mkv" --popups "$popups" \
            > "$out/$tk-popups.json" 2> "$out/$tk-popups.err" || echo "failed: $tk-popups" >&2
    done
fi
