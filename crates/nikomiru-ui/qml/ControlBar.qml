// SPDX-License-Identifier: GPL-3.0-only
//
// Control bar of the player (M1): progress, time, play/pause, volume, speed,
// audio track, fullscreen. It never takes the keyboard focus, so shortcuts keep
// working.

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic

Rectangle {
    id: bar
    required property var controller
    property bool fullscreen: false
    signal fullscreenToggled()
    signal openRequested()

    height: 84
    color: "#d9141418"

    function clock(seconds) {
        const s = Math.max(0, Math.floor(seconds));
        const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, r = s % 60;
        const two = (n) => (n < 10 ? "0" : "") + n;
        return (h > 0 ? h + ":" + two(m) : m) + ":" + two(r);
    }

    readonly property var audioTracks: {
        try { return JSON.parse(controller.audio_tracks); } catch (e) { return []; }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 4

        Slider {
            id: progress
            Layout.fillWidth: true
            focusPolicy: Qt.NoFocus
            from: 0
            to: Math.max(controller.duration, 0.001)
            value: pressed ? value : controller.time_pos
            enabled: controller.loaded
            onMoved: controller.seek_to(value)
            Accessible.name: controller.tr("app-title")
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Button {
                focusPolicy: Qt.NoFocus
                text: controller.paused ? "▶" : "⏸"
                enabled: controller.loaded
                onClicked: controller.action("play_pause")
                ToolTip.visible: hovered
                ToolTip.text: controller.paused ? controller.tr("play") : controller.tr("pause")
                Accessible.name: controller.paused ? controller.tr("play") : controller.tr("pause")
            }

            Label {
                color: "white"
                text: bar.clock(controller.time_pos) + " / " + bar.clock(controller.duration)
                font.features: { "tnum": 1 }
            }

            Item { Layout.fillWidth: true }

            Button {
                focusPolicy: Qt.NoFocus
                text: controller.muted ? "🔇" : "🔊"
                onClicked: controller.action("mute")
                ToolTip.visible: hovered
                ToolTip.text: controller.muted ? controller.tr("unmute") : controller.tr("mute")
                Accessible.name: controller.muted ? controller.tr("unmute") : controller.tr("mute")
            }

            Slider {
                focusPolicy: Qt.NoFocus
                Layout.preferredWidth: 120
                from: 0
                to: 150
                stepSize: 5
                value: pressed ? value : controller.volume
                onMoved: controller.set_volume_level(value)
                Accessible.name: controller.tr("volume")
            }

            Button {
                focusPolicy: Qt.NoFocus
                text: "−"
                onClicked: controller.action("speed_down")
                Accessible.name: controller.tr("speed")
            }
            Label {
                color: "white"
                text: controller.speed.toFixed(2) + "×"
                font.features: { "tnum": 1 }
            }
            Button {
                focusPolicy: Qt.NoFocus
                text: "+"
                onClicked: controller.action("speed_up")
                Accessible.name: controller.tr("speed")
            }

            ComboBox {
                id: audio
                focusPolicy: Qt.NoFocus
                visible: bar.audioTracks.length > 1
                Layout.preferredWidth: 200
                model: bar.audioTracks.map(t =>
                    (t.lang || controller.tr("track-unknown-language"))
                    + (t.title ? " — " + t.title : "")
                    + (t.channels ? " (" + t.channels + " ch)" : ""))
                currentIndex: bar.audioTracks.findIndex(t => t.id === controller.aid)
                onActivated: (index) => controller.select_audio(bar.audioTracks[index].id)
                Accessible.name: controller.tr("audio-track")
            }

            Button {
                focusPolicy: Qt.NoFocus
                text: "📂"
                onClicked: bar.openRequested()
                ToolTip.visible: hovered
                ToolTip.text: controller.tr("open-file")
                Accessible.name: controller.tr("open-file")
            }

            Button {
                focusPolicy: Qt.NoFocus
                text: bar.fullscreen ? "🗗" : "⛶"
                onClicked: bar.fullscreenToggled()
                ToolTip.visible: hovered
                ToolTip.text: bar.fullscreen ? controller.tr("exit-fullscreen") : controller.tr("fullscreen")
                Accessible.name: bar.fullscreen ? controller.tr("exit-fullscreen") : controller.tr("fullscreen")
            }
        }
    }
}
