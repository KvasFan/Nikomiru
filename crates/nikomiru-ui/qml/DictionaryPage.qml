// SPDX-License-Identifier: GPL-3.0-only
//
// The dictionaries page (DIC-030…DIC-034, DIC-016): import a Yomitan zip with
// its progress, the update offer for a new revision, and the list in priority
// order with enable, move, rename, popup block state and delete.

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import QtQuick.Dialogs

Rectangle {
    id: page
    required property var controller
    signal closed()

    readonly property var pageData: {
        try { return JSON.parse(controller.dict_page); } catch (e) { return { rows: [], import: { state: "idle" } }; }
    }
    readonly property var rows: pageData.rows || []
    readonly property var importState: pageData.import || { state: "idle" }
    property string renaming: ""
    property string deleting: ""

    function open() {
        controller.dict_open();
        visible = true;
    }
    function close() {
        renaming = "";
        visible = false;
        closed();
    }
    function size(bytes) {
        if (bytes >= 1048576)
            return (bytes / 1048576).toFixed(1) + " MB";
        return Math.max(1, Math.round(bytes / 1024)) + " kB";
    }
    function counts(r) {
        const parts = [];
        if (r.terms > 0)
            parts.push(r.terms + " " + controller.tr("dict-type-terms"));
        if (r.kanji > 0)
            parts.push(r.kanji + " " + controller.tr("dict-type-kanji"));
        if (r.frequencies > 0)
            parts.push(r.frequencies + " " + controller.tr("dict-type-frequency"));
        if (r.pitches > 0)
            parts.push(r.pitches + " " + controller.tr("dict-type-pitch"));
        return parts.join(" · ");
    }

    visible: false
    color: "#a6000000"

    component RowButton: Button {
        flat: true
        focusPolicy: Qt.NoFocus
        implicitWidth: 34
        implicitHeight: 32
        font.pixelSize: 15
    }

    // the page takes every click and the wheel: nothing reaches the video
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onWheel: (wheel) => wheel.accepted = true
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.min(780, parent.width - 32)
        height: Math.min(parent.height - 32, 640)
        radius: 10
        color: "#f0202227"
        border.color: "#40ffffff"

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                Text {
                    Layout.fillWidth: true
                    text: page.controller.tr("dict-page-title")
                    color: "white"
                    font.pixelSize: 22
                    font.bold: true
                }
                Button {
                    text: page.controller.tr("dict-import")
                    enabled: page.importState.state === "idle"
                    onClicked: zipDialog.open()
                }
                Button {
                    text: "✕"
                    onClicked: page.close()
                    ToolTip.visible: hovered
                    ToolTip.text: page.controller.tr("dict-close")
                    Accessible.name: page.controller.tr("dict-close")
                }
            }

            // an import running
            Rectangle {
                Layout.fillWidth: true
                visible: page.importState.state === "running"
                implicitHeight: running.implicitHeight + 16
                radius: 6
                color: "#2a2f3a"
                ColumnLayout {
                    id: running
                    anchors.fill: parent
                    anchors.margins: 8
                    RowLayout {
                        Text {
                            Layout.fillWidth: true
                            text: page.controller.tr("dict-importing") + ": " + (page.importState.file || "")
                            textFormat: Text.PlainText
                            elide: Text.ElideMiddle
                            color: "#e8e8e8"
                        }
                        Button {
                            text: page.controller.tr("dict-cancel")
                            onClicked: page.controller.dict_cancel()
                        }
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        from: 0
                        to: Math.max(1, page.importState.total || 0)
                        value: page.importState.done || 0
                        indeterminate: !page.importState.total
                    }
                    Text {
                        text: (page.importState.entries || 0) + " " + page.controller.tr("dict-entries")
                        color: "#9aa0aa"
                        font.pixelSize: 12
                    }
                }
            }

            // a new revision of an installed dictionary (DIC-016)
            Rectangle {
                Layout.fillWidth: true
                visible: page.importState.state === "confirm"
                implicitHeight: confirm.implicitHeight + 16
                radius: 6
                color: "#3a3424"
                ColumnLayout {
                    id: confirm
                    anchors.fill: parent
                    anchors.margins: 8
                    Text {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: page.controller.tr("dict-update-offer")
                        color: "#e8e8e8"
                    }
                    Text {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: (page.importState.title || "") + " — " + page.controller.tr("dict-revision") + " "
                              + (page.importState.installed || "") + " → " + (page.importState.file || "")
                        textFormat: Text.PlainText
                        color: "#c9d1dd"
                        font.pixelSize: 13
                    }
                    RowLayout {
                        Button {
                            text: page.controller.tr("dict-update")
                            onClicked: page.controller.dict_confirm(true)
                        }
                        Button {
                            text: page.controller.tr("dict-keep")
                            onClicked: page.controller.dict_confirm(false)
                        }
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: page.rows.length === 0
                wrapMode: Text.Wrap
                text: page.controller.tr("dict-empty")
                color: "#9aa0aa"
                font.pixelSize: 15
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignHCenter
            }

            ListView {
                id: list
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: page.rows.length > 0
                clip: true
                spacing: 6
                model: page.rows
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    required property int index
                    width: list.width - 8
                    implicitHeight: rowLayout.implicitHeight + 12
                    radius: 6
                    color: modelData.enabled ? "#2a2d34" : "#1f2126"

                    RowLayout {
                        id: rowLayout
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 8

                        CheckBox {
                            checked: row.modelData.enabled
                            onToggled: page.controller.dict_set_enabled(row.modelData.id, checked)
                            ToolTip.visible: hovered
                            ToolTip.text: page.controller.tr("dict-enabled")
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            RowLayout {
                                visible: page.renaming !== row.modelData.id
                                Text {
                                    text: row.modelData.name
                                    textFormat: Text.PlainText
                                    color: row.modelData.enabled ? "white" : "#8a8f99"
                                    font.pixelSize: 16
                                    font.bold: true
                                    elide: Text.ElideRight
                                    Layout.maximumWidth: list.width * 0.45
                                }
                                Text {
                                    text: page.controller.tr("dict-revision") + " " + row.modelData.revision
                                    textFormat: Text.PlainText
                                    color: "#9aa0aa"
                                    font.pixelSize: 12
                                }
                            }
                            TextField {
                                id: nameField
                                visible: page.renaming === row.modelData.id
                                Layout.fillWidth: true
                                placeholderText: row.modelData.title
                                onVisibleChanged: if (visible) { text = row.modelData.renamed ? row.modelData.name : ""; forceActiveFocus(); }
                                onAccepted: {
                                    page.controller.dict_rename(row.modelData.id, text);
                                    page.renaming = "";
                                }
                                Keys.onEscapePressed: page.renaming = ""
                            }
                            Text {
                                text: page.counts(row.modelData) + " · " + page.size(row.modelData.size)
                                      + (row.modelData.importedAt > 0 ? " · " + new Date(row.modelData.importedAt).toLocaleDateString() : "")
                                color: "#9aa0aa"
                                font.pixelSize: 12
                            }
                        }

                        ComboBox {
                            id: collapse
                            Layout.preferredWidth: 130
                            // only term dictionaries have popup blocks; the space stays, so columns line up
                            opacity: row.modelData.types.indexOf("terms") >= 0 ? 1 : 0
                            enabled: opacity > 0
                            model: [
                                { value: "automatic", text: page.controller.tr("dict-collapse-automatic") },
                                { value: "expanded", text: page.controller.tr("dict-collapse-expanded") },
                                { value: "collapsed", text: page.controller.tr("dict-collapse-collapsed") }
                            ]
                            textRole: "text"
                            valueRole: "value"
                            Component.onCompleted: currentIndex = indexOfValue(row.modelData.collapse)
                            onActivated: page.controller.dict_set_collapse(row.modelData.id, currentValue)
                            ToolTip.visible: hovered
                            ToolTip.text: page.controller.tr("dict-collapse")
                        }
                        RowButton {
                            text: "▲"
                            enabled: row.index > 0
                            onClicked: page.controller.dict_move(row.modelData.id, -1)
                            ToolTip.visible: hovered
                            ToolTip.text: page.controller.tr("dict-move-up")
                        }
                        RowButton {
                            text: "▼"
                            enabled: row.index < page.rows.length - 1
                            onClicked: page.controller.dict_move(row.modelData.id, 1)
                            ToolTip.visible: hovered
                            ToolTip.text: page.controller.tr("dict-move-down")
                        }
                        RowButton {
                            text: "✎"
                            onClicked: page.renaming = page.renaming === row.modelData.id ? "" : row.modelData.id
                            ToolTip.visible: hovered
                            ToolTip.text: page.controller.tr("dict-rename")
                        }
                        RowButton {
                            text: "🗑"
                            onClicked: {
                                page.deleting = row.modelData.id;
                                deleteDialog.text = row.modelData.name;
                                deleteDialog.open();
                            }
                            ToolTip.visible: hovered
                            ToolTip.text: page.controller.tr("dict-delete")
                        }
                    }
                }
            }
        }
    }

    FileDialog {
        id: zipDialog
        title: page.controller.tr("dict-import")
        nameFilters: [page.controller.tr("dict-import-filter") + " (*.zip)"]
        onAccepted: page.controller.dict_choose(selectedFile.toString())
    }

    MessageDialog {
        id: deleteDialog
        title: page.controller.tr("dict-delete-confirm")
        buttons: MessageDialog.Yes | MessageDialog.No
        onButtonClicked: (button, role) => {
            if (button === MessageDialog.Yes && page.deleting !== "")
                page.controller.dict_delete(page.deleting);
            page.deleting = "";
        }
    }
}
