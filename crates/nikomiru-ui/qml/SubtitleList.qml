// SPDX-License-Identifier: GPL-3.0-only
//
// The subtitle list (SUB-015): every cue with its time, a search field, and the
// row of the current cue followed during playback (in the gaps it stays on the
// previous cue). A click on the time seeks there; a click in the text selects
// that character, as in the overlay.

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic

Rectangle {
    id: panel
    required property var controller
    signal closeRequested()

    color: "#e6141418"
    property string query: ""
    readonly property var all: {
        try { return JSON.parse(controller.subtitle_list); } catch (e) { return []; }
    }
    readonly property var rows: {
        if (query === "")
            return all;
        let hits = [];
        try { hits = JSON.parse(controller.search_subtitles(query)); } catch (e) {}
        const keep = new Set(hits);
        return all.filter(r => keep.has(r.i));
    }
    // the row of the current cue among the rows shown
    readonly property int currentRow: {
        const cur = controller.subtitle_list_row;
        let row = -1;
        for (let k = 0; k < rows.length && rows[k].i <= cur; k++)
            row = k;
        return row;
    }

    function focusSearch() {
        search.forceActiveFocus();
        search.selectAll();
    }

    function clock(ms) {
        const s = Math.max(0, Math.floor(ms / 1000));
        const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, r = s % 60;
        const two = (n) => (n < 10 ? "0" : "") + n;
        return (h > 0 ? h + ":" + two(m) : m) + ":" + two(r);
    }

    onCurrentRowChanged: {
        if (currentRow >= 0 && !list.moving)
            list.positionViewAtIndex(currentRow, ListView.Contain);
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: panel.controller.tr("subtitle-list")
                color: "white"
                font.bold: true
                Layout.fillWidth: true
            }
            Button {
                text: "✕"
                focusPolicy: Qt.NoFocus
                onClicked: panel.closeRequested()
                Accessible.name: panel.controller.tr("subtitle-list")
            }
        }

        TextField {
            id: search
            Layout.fillWidth: true
            placeholderText: panel.controller.tr("subtitle-search")
            onTextChanged: panel.query = text
            Keys.onEscapePressed: {
                if (text !== "")
                    text = "";
                else
                    panel.closeRequested();
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: panel.rows
            spacing: 2
            ScrollBar.vertical: ScrollBar {}

            delegate: Rectangle {
                id: rowItem
                required property var modelData
                required property int index
                width: list.width
                height: Math.max(time.implicitHeight, body.implicitHeight) + 8
                radius: 4
                color: index === panel.currentRow ? "#3a4a90e2" : "transparent"

                Text {
                    id: time
                    x: 4
                    y: 4
                    width: 52
                    text: panel.clock(rowItem.modelData.start)
                    color: "#9ab8ff"
                    font.features: { "tnum": 1 }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: panel.controller.list_time_clicked(rowItem.modelData.i)
                    }
                }
                TextEdit {
                    id: body
                    x: time.width + 10
                    y: 4
                    width: rowItem.width - x - 6
                    text: rowItem.modelData.text
                    readOnly: true
                    selectByMouse: false
                    wrapMode: TextEdit.Wrap
                    color: "white"
                    font.pixelSize: 16
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: (mouse) => panel.controller.list_text_clicked(
                            rowItem.modelData.i, body.positionAt(mouse.x, mouse.y))
                    }
                }
            }
        }
    }
}
