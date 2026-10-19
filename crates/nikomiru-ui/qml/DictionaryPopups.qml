// SPDX-License-Identifier: GPL-3.0-only
//
// The dictionary popups (POP-001…POP-013): one panel per open popup, nested
// popups above their parent. The layer itself takes no clicks; a click
// outside every panel reaches the window, which closes them all.

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

Item {
    id: popupLayer
    required property var controller
    readonly property var popups: {
        try { return JSON.parse(controller.popups); } catch (e) { return []; }
    }
    readonly property bool open: popups.length > 0

    Repeater {
        model: popupLayer.popups
        Rectangle {
            id: panel
            required property var modelData
            readonly property var popup: modelData
            readonly property real base: 15
            readonly property real prefWidth: Math.min(560, popupLayer.width - 16)
            readonly property real prefHeight: Math.min(body.implicitHeight + 20, popupLayer.height * 0.7)
            // without a rectangle (a click from the test harness), the subtitle area
            readonly property var anchor: popup.anchor.w > 0 || popup.anchor.h > 0 ? popup.anchor
                : ({ x: popupLayer.width * 0.25, y: popupLayer.height * 0.72, w: popupLayer.width * 0.5, h: popupLayer.height * 0.12 })
            readonly property var place: JSON.parse(popupLayer.controller.place_popup(
                anchor.x, anchor.y, anchor.w, anchor.h,
                prefWidth, prefHeight, popupLayer.width, popupLayer.height))
            x: place.x
            y: place.y
            width: place.w
            height: Math.max(place.h, 48)
            z: popup.level
            color: "#f21b1d22"
            radius: 8
            border.color: "#40ffffff"
            border.width: 1

            // blocks and inline runs created at once, about the first sense of
            // a rich entry (Jitendex's 行く: ~800 items in all); the rest is
            // created in the next frames (NFR-PERF-06). A plain JS counter:
            // spending it must not re-evaluate bindings.
            property var budget: ({
                left: 30,
                spend: function () { this.left -= 1; return this.left < 0; }
            })

            // the first frame with the content on screen (T-PERF-06)
            property bool reported: false
            Connections {
                target: panel.Window.window
                enabled: !panel.reported && !panel.popup.loading
                function onFrameSwapped() {
                    panel.reported = true;
                    const level = panel.popup.level;
                    Qt.callLater(() => popupLayer.controller.popup_rendered(level));
                }
            }

            // the pointer over a popup keeps a hover popup open (POP-003)
            HoverHandler {
                onHoveredChanged: popupLayer.controller.popup_hover(hovered)
            }
            // Alt+wheel: next or previous entry (POP-014)
            WheelHandler {
                acceptedModifiers: Qt.AltModifier
                onWheel: (event) => {
                    const d = event.angleDelta.y !== 0 ? event.angleDelta.y : event.angleDelta.x;
                    popupLayer.controller.popup_entry(d < 0 ? 1 : -1);
                }
            }

            function scrollToEntry() {
                const item = entryList.itemAt(popup.entry);
                if (item && popup.entry > 0)
                    flick.contentY = Math.max(0, Math.min(item.y, flick.contentHeight - flick.height));
            }
            onPopupChanged: Qt.callLater(scrollToEntry)
            Component.onCompleted: Qt.callLater(scrollToEntry)

            // a click in the panel closes the popups above it (POP-012) and
            // never reaches the video
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onPressed: (mouse) => {
                    const level = panel.popup.level;
                    Qt.callLater(() => popupLayer.controller.popup_focus(level));
                }
                onWheel: (wheel) => wheel.accepted = false
            }

            Flickable {
                id: flick
                anchors.fill: parent
                anchors.margins: 10
                contentWidth: width
                contentHeight: body.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}

                Column {
                    id: body
                    width: panel.prefWidth - 20
                    spacing: 14

                    Text {
                        visible: panel.popup.loading
                        text: popupLayer.controller.tr("popup-loading")
                        color: "#9aa0aa"
                        font.pixelSize: panel.base
                    }
                    Text {
                        visible: panel.popup.empty
                        text: popupLayer.controller.tr("popup-no-results")
                        textFormat: Text.PlainText
                        color: "#9aa0aa"
                        font.pixelSize: panel.base
                    }
                    Repeater {
                        model: panel.popup.content ? panel.popup.content.kanji : []
                        KanjiCard {
                            required property var modelData
                            kanji: modelData
                            width: body.width
                        }
                    }
                    // replay the cue, or go to it (POP-015)
                    Row {
                        visible: panel.popup.fromSubtitle && panel.popup.level === 0 && !panel.popup.loading
                        spacing: 6
                        Chip {
                            text: "↻ " + popupLayer.controller.tr("popup-replay")
                            fontSize: 13
                            TapHandler { onTapped: popupLayer.controller.popup_replay() }
                        }
                        Chip {
                            text: "⇥ " + popupLayer.controller.tr("popup-goto")
                            fontSize: 13
                            TapHandler { onTapped: Qt.callLater(() => popupLayer.controller.popup_goto()) }
                        }
                    }
                    Repeater {
                        id: entryList
                        model: panel.popup.content ? panel.popup.content.entries : []
                        Item {
                            id: entryItem
                            required property var modelData
                            required property int index
                            width: body.width
                            height: entryView.height
                            // the entry the keyboard is on (POP-014)
                            Rectangle {
                                visible: entryItem.index === panel.popup.entry && panel.popup.content.entries.length > 1
                                width: 3
                                height: parent.height
                                radius: 1.5
                                color: "#6aa0ff"
                            }
                            // the first entry at once, the others in the next frames
                            Loader {
                                id: entryView
                                x: 8
                                width: parent.width - 8
                                asynchronous: entryItem.index > 0
                                sourceComponent: PopupEntry {
                                    width: entryView.width
                                    entry: entryItem.modelData
                                    controller: popupLayer.controller
                                    level: panel.popup.level
                                    base: panel.base
                                    budget: panel.budget
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
