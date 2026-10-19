// SPDX-License-Identifier: GPL-3.0-only
//
// Short message (volume, speed, mute…) that fades out after 1.5 s. It shows
// again whenever `serial` changes, even with the same text.

import QtQuick

Rectangle {
    id: toast
    property string text: ""
    property int serial: 0

    radius: 6
    color: "#cc000000"
    width: label.implicitWidth + 28
    height: label.implicitHeight + 16
    opacity: 0

    onSerialChanged: {
        opacity = 1;
        hide.restart();
    }

    Text {
        id: label
        anchors.centerIn: parent
        text: toast.text
        color: "white"
        font.pixelSize: 18
    }

    Timer {
        id: hide
        interval: 1500
        onTriggered: toast.opacity = 0
    }

    Behavior on opacity { NumberAnimation { duration: 200 } }
}
