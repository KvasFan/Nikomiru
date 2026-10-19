// SPDX-License-Identifier: GPL-3.0-only
//
// A small label: trace steps, tags, frequencies (POP-007, POP-008). Plain
// text only; the tooltip shows the description or the tag notes.

import QtQuick
import QtQuick.Controls.Basic

Rectangle {
    id: chip
    property string text: ""
    property string tip: ""
    property string category: ""
    property real fontSize: 12
    implicitWidth: label.implicitWidth + 10
    implicitHeight: label.implicitHeight + 4
    radius: 4
    color: {
        switch (category) {
        case "popular": return "#5a3b1f";
        case "partOfSpeech": return "#2a3f5f";
        case "frequent": return "#3d5a2a";
        case "archaism": case "dialect": return "#4a3a5a";
        default: return "#3a3d44";
        }
    }

    Text {
        id: label
        anchors.centerIn: parent
        text: chip.text
        textFormat: Text.PlainText
        color: "#e8e8e8"
        font.pixelSize: chip.fontSize
    }

    HoverHandler {
        id: hover
        enabled: chip.tip !== ""
    }
    ToolTip.visible: hover.hovered
    ToolTip.text: chip.tip
    ToolTip.delay: 300
}
