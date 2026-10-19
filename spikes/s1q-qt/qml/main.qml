// SPDX-License-Identifier: GPL-3.0-only
//
// Spike S1q test UI: the same overlay as S1 (control bar, fake subtitle whose
// graphemes are clickable and change 30 times per second), on top of mpv.

import QtQuick
import QtQuick.Window
import Nikomiru.Video
import Nikomiru.Spike

Window {
    id: win
    width: 1280
    height: 720
    visible: true
    color: "black"
    title: "Nikomiru — spike S1q"
    visibility: probe.fullscreen ? Window.FullScreen : Window.Windowed

    Probe { id: probe }

    property int clicks: 0
    property int keys: 0
    property string last: "—"
    property var graphemes: []
    property int cycle: 1
    property var rss: ({})
    property var snapA: null
    property var timeline: []
    property int lastDrops: 0
    readonly property var mpv: videoLoader.item

    Loader {
        id: videoLoader
        anchors.fill: parent
        sourceComponent: MpvItem {
            Component.onCompleted: load(probe.video)
        }
    }

    Connections {
        target: win.mpv
        function onTimePosChanged() {
            if (probe.static_sub && win.graphemes.length > 0)
                return;
            const t = win.mpv.timePos.toFixed(2).padStart(7, " ");
            win.graphemes = JSON.parse(probe.graphemes("字幕のテストです・" + t + "秒・クリックしてね"));
        }
    }

    Item {
        id: overlay
        anchors.fill: parent
        visible: !probe.no_overlay
        focus: true

        Keys.onPressed: (e) => {
            win.keys++;
            win.last = "key " + e.key + " (scancode " + e.nativeScanCode + ")";
            if (e.key === Qt.Key_Space)
                win.mpv.command(["cycle", "pause"]);
            else if (e.key === Qt.Key_Left)
                win.mpv.command(["seek", "-3"]);
            else if (e.key === Qt.Key_Right)
                win.mpv.command(["seek", "3"]);
            else if (e.key === Qt.Key_F)
                win.visibility = win.visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen;
            else if (e.key === Qt.Key_Q || e.key === Qt.Key_Escape)
                Qt.quit();
        }

        Rectangle {
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: 44
            color: "#bf121216"
            Row {
                anchors.verticalCenter: parent.verticalCenter
                x: 14
                spacing: 12
                Rectangle {
                    width: 120; height: 30; radius: 6
                    color: pauseArea.containsMouse ? "#3a3a44" : "#2a2a30"
                    border.color: "#666"
                    Text {
                        anchors.centerIn: parent
                        color: "white"
                        text: win.mpv && win.mpv.paused ? "▶ Play" : "⏸ Pause"
                    }
                    MouseArea {
                        id: pauseArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: { win.clicks++; win.last = "pause button"; win.mpv.command(["cycle", "pause"]); }
                    }
                }
                Rectangle {
                    width: 140; height: 30; radius: 6
                    color: fsArea.containsMouse ? "#3a3a44" : "#2a2a30"
                    border.color: "#666"
                    Text { anchors.centerIn: parent; color: "white"; text: "⛶ Fullscreen" }
                    MouseArea {
                        id: fsArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: {
                            win.clicks++;
                            win.last = "fullscreen button";
                            win.visibility = win.visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen;
                        }
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    color: "#eee"
                    text: "clicks " + win.clicks + " · keys " + win.keys + " · last: " + win.last
                }
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 75 * win.height / 1080
            Repeater {
                model: win.graphemes
                Text {
                    required property string modelData
                    required property int index
                    text: modelData
                    color: "white"
                    font.pixelSize: Math.max(18, 36 * win.height / 1080)
                    style: Text.Outline
                    styleColor: "black"
                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: { win.clicks++; win.last = "grapheme " + parent.index; }
                    }
                }
            }
        }
    }

    // ---- measurements -------------------------------------------------------

    function snap() {
        const s = win.mpv.stats();
        return {
            t: Date.now(),
            drops: Number(win.mpv.getProperty("frame-drop-count") || 0),
            delayed: Number(win.mpv.getProperty("vo-delayed-frame-count") || 0),
            renders: Number(s.renders),
            newFrames: Number(s.new_frames)
        };
    }

    Timer {
        interval: 200
        repeat: true
        running: probe.mode !== "interactive"
        onTriggered: {
            if (!win.mpv || Number(win.mpv.timePos) <= 0.5)
                return;
            stop();
            if (probe.mode === "measure")
                warmup.start();
            else
                stressTimer.start();
        }
    }

    Timer {
        id: warmup
        interval: 3000
        onTriggered: {
            win.snapA = win.snap();
            probe.cpu_snapshot();
            win.mpv.resetRenderTimes();
            win.lastDrops = win.snapA.drops;
            dropTimer.start();
            endTimer.start();
        }
    }

    Timer {
        id: dropTimer
        interval: 1000
        repeat: true
        onTriggered: {
            const d = Number(win.mpv.getProperty("frame-drop-count") || 0);
            win.timeline.push(d - win.lastDrops);
            win.lastDrops = d;
        }
    }

    Timer {
        id: endTimer
        interval: probe.seconds * 1000
        onTriggered: {
            dropTimer.stop();
            const b = win.snap();
            const a = win.snapA;
            const secs = (b.t - a.t) / 1000;
            const s = win.mpv.stats();
            probe.finish(JSON.stringify({
                mode: "measure",
                video: probe.video,
                seconds: secs,
                frame_drop_count: b.drops - a.drops,
                vo_delayed_frame_count: b.delayed - a.delayed,
                renders_per_second: (b.renders - a.renders) / secs,
                new_frames_per_second: (b.newFrames - a.newFrames) / secs,
                render_us: { p50: s.render_us_p50, p95: s.render_us_p95, max: s.render_us_max },
                render_size: [s.fbo_width, s.fbo_height],
                device_pixel_ratio: s.device_pixel_ratio,
                window_logical: [win.width, win.height],
                hwdec_current: win.mpv.getProperty("hwdec-current"),
                estimated_vf_fps: win.mpv.getProperty("estimated-vf-fps"),
                drops_per_second_timeline: win.timeline
            }));
        }
    }

    Timer {
        id: stressTimer
        interval: 100
        repeat: true
        onTriggered: {
            if (!win.mpv || Number(win.mpv.stats().renders) < 20)
                return;
            if (win.cycle === 5 || win.cycle === probe.seconds)
                win.rss[win.cycle] = probe.rss_mib();
            if (win.cycle === probe.seconds) {
                stop();
                const first = win.rss[5], lastRss = win.rss[probe.seconds];
                probe.finish(JSON.stringify({
                    mode: "stress",
                    cycles: probe.seconds,
                    rss_mib_after_cycle_5: first,
                    rss_mib_at_end: lastRss,
                    growth_pct: (lastRss - first) / first * 100
                }));
                return;
            }
            win.cycle++;
            // A new MpvItem means a new mpv core and a new render context.
            videoLoader.active = false;
            videoLoader.active = true;
        }
    }
}
