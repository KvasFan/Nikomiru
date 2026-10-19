// SPDX-License-Identifier: GPL-3.0-only
//
// Control bar of the player (M1): progress, time, play/pause, volume, speed,
// audio track, fullscreen. It never takes the keyboard focus, so shortcuts keep
// working.

import QtQuick
import QtQml
import QtQuick.Layouts
import QtQuick.Controls.Basic

Rectangle {
    id: bar
    required property var controller
    property bool fullscreen: false
    signal fullscreenToggled()
    signal openRequested()
    signal subtitleFileRequested()
    signal subtitleListRequested()
    signal dictionariesRequested()

    implicitHeight: layout.implicitHeight + 2 * layout.anchors.margins
    color: "#d9141418"

    function clock(seconds) {
        const s = Math.max(0, Math.floor(seconds));
        const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, r = s % 60;
        const two = (n) => (n < 10 ? "0" : "") + n;
        return (h > 0 ? h + ":" + two(m) : m) + ":" + two(r);
    }

    readonly property var subtitleSources: {
        try { return JSON.parse(controller.subtitle_menu); } catch (e) { return []; }
    }
    readonly property var encodings: ["", "UTF-8", "CP932", "EUC-JP", "GB18030", "BIG5", "CP1252"]

    function sourceLabel(s) {
        if (s.kind === "file")
            return s.name;
        let parts = [s.name];
        if (s.lang)
            parts.push(s.lang);
        if (s.title)
            parts.push(s.title);
        if (s.codec)
            parts.push("(" + s.codec + ")");
        return (s.kind === "bitmap" ? "🖼 " : "") + parts.join(" · ");
    }

    readonly property var audioTracks: {
        try { return JSON.parse(controller.audio_tracks); } catch (e) { return []; }
    }

    ColumnLayout {
        id: layout
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
                id: subButton
                focusPolicy: Qt.NoFocus
                text: "字"
                onClicked: subMenu.popup(subButton, 0, -subMenu.implicitHeight)
                ToolTip.visible: hovered
                ToolTip.text: controller.tr("subtitles")
                Accessible.name: controller.tr("subtitles")

                Menu {
                    id: subMenu
                    MenuItem {
                        text: controller.tr("subtitles-off")
                        checkable: true
                        checked: controller.subtitle_key === ""
                        onTriggered: controller.select_subtitles("off")
                    }
                    Instantiator {
                        model: bar.subtitleSources
                        delegate: MenuItem {
                            required property var modelData
                            text: bar.sourceLabel(modelData)
                            checkable: true
                            checked: controller.subtitle_key === modelData.key
                            onTriggered: controller.select_subtitles(modelData.key)
                        }
                        onObjectAdded: (index, object) => subMenu.insertItem(index + 1, object)
                        onObjectRemoved: (index, object) => subMenu.removeItem(object)
                    }
                    MenuSeparator {}
                    MenuItem {
                        text: controller.tr("subtitle-open-file")
                        onTriggered: bar.subtitleFileRequested()
                    }
                    MenuItem {
                        text: controller.tr("subtitles-disable") + "  (V)"
                        checkable: true
                        checked: controller.subtitles_disabled
                        onTriggered: controller.action("sub_toggle")
                    }
                    Menu {
                        id: encodingMenu
                        title: controller.tr("subtitle-encoding")
                               + (controller.subtitle_encoding_label !== "" ? ": " + controller.subtitle_encoding_label : "")
                        Instantiator {
                            model: bar.encodings
                            delegate: MenuItem {
                                required property var modelData
                                // "Automatic (detected: …)" while nothing is forced
                                text: modelData === ""
                                      ? controller.tr("encoding-auto")
                                        + (controller.subtitle_forced_encoding === "" && controller.subtitle_encoding_label !== ""
                                           ? " — " + controller.subtitle_encoding_label : "")
                                      : controller.encoding_label(modelData)
                                checkable: true
                                checked: controller.subtitle_forced_encoding === modelData
                                onTriggered: controller.force_encoding(modelData)
                            }
                            onObjectAdded: (index, object) => encodingMenu.insertItem(index, object)
                            onObjectRemoved: (index, object) => encodingMenu.removeItem(object)
                        }
                    }
                    MenuItem {
                        text: controller.tr("subtitle-list")
                        onTriggered: bar.subtitleListRequested()
                    }
                }
            }

            Button {
                focusPolicy: Qt.NoFocus
                text: "📚"
                onClicked: bar.dictionariesRequested()
                ToolTip.visible: hovered
                ToolTip.text: controller.tr("dict-page-title")
                Accessible.name: controller.tr("dict-page-title")
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
