// SPDX-License-Identifier: GPL-3.0-only
//
// The main window: player page (M1). Video below, QML overlay above, composited
// by Qt Quick on the render thread (SPECIFICATION §5.2, ADR 0003).

import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import Nikomiru.Video
import Nikomiru.UI

ApplicationWindow {
    id: win
    width: 1280
    height: 720
    visible: true
    color: "black"
    title: ctl.loaded ? ctl.title + " — " + ctl.tr("app-title") : ctl.tr("app-title")

    PlayerController {
        id: ctl
        window_fullscreen: win.visibility === Window.FullScreen
        onActivateRequested: {
            win.raise();
            win.requestActivate();
        }
        // closing the last window quits, through onClosing like the user
        onQuitRequested: win.close()
    }

    // GUI test harness (TEST §2.1): these signals exist only in builds with the
    // `test-harness` feature, so the release build must load without them.
    Connections {
        target: ctl
        ignoreUnknownSignals: true
        function onDropRequested(url) {
            win.openDropped([url]);
        }
        function onKeyRequested(scanCode, modifiers) {
            win.handleKey(scanCode, modifiers, true, false);
            win.handleKey(scanCode, modifiers, false, false);
        }
    }

    // Drag & drop: the first URL is opened (LIB-001).
    function openDropped(urls) {
        if (urls.length === 0)
            return;
        const url = urls[0].toString();
        const ext = url.split(".").pop().toLowerCase();
        if (ctl.loaded && subtitleExtensions.indexOf(ext) >= 0)
            ctl.open_subtitle_file(url);
        else
            ctl.open_file(url);
    }

    // Keyboard: every key goes to Rust with its scan code (KEY-002).
    function handleKey(scanCode, modifiers, pressed, autoRepeat) {
        if (pressed)
            win.apply(ctl.key_event(scanCode, modifiers, true, autoRepeat, false));
        else if (!autoRepeat)
            ctl.key_event(scanCode, modifiers, false, false, false);
    }

    property bool subListVisible: false
    readonly property var subtitleExtensions: ["srt", "ass", "ssa", "vtt"]

    function toggleSubList(searching) {
        if (searching) {
            subListVisible = true;
            subList.focusSearch();
        } else {
            subListVisible = !subListVisible;
            if (!subListVisible)
                keys.forceActiveFocus();
        }
    }

    function apply(effect) {
        if (effect === "sub_list")
            toggleSubList(false);
        else if (effect === "sub_search")
            toggleSubList(true);
        else if (effect === "fullscreen")
            win.visibility = win.visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen;
        else if (effect === "back" && win.visibility === Window.FullScreen)
            win.visibility = Window.Windowed;
        else if (effect === "open_file")
            fileDialog.open();
    }

    onClosing: ctl.shutdown()

    VideoItem {
        id: video
        anchors.fill: parent
        onRendererReadyChanged: if (rendererReady) ctl.renderer_ready()
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: (e) => {
            win.handleKey(e.nativeScanCode, e.modifiers, true, e.isAutoRepeat);
            e.accepted = true;
        }
        Keys.onReleased: (e) => {
            win.handleKey(e.nativeScanCode, e.modifiers, false, e.isAutoRepeat);
            e.accepted = true;
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: ctl.controls_visible ? Qt.ArrowCursor : Qt.BlankCursor
        onPositionChanged: ctl.mouse_moved()
        onDoubleClicked: win.apply("fullscreen")
        onClicked: {
            keys.forceActiveFocus();
            if (ctl.loaded)
                ctl.action("play_pause");
        }
    }

    Text {
        anchors.centerIn: parent
        visible: !ctl.loaded && ctl.error_text === ""
        text: ctl.tr("drop-hint")
        color: "#bbbbbb"
        font.pixelSize: 20
    }

    Rectangle {
        anchors.centerIn: parent
        visible: ctl.error_text !== ""
        width: Math.min(parent.width - 64, errorLabel.implicitWidth + 32)
        height: errorLabel.implicitHeight + 24
        radius: 8
        color: "#cc3a1a1a"
        Text {
            id: errorLabel
            anchors.centerIn: parent
            width: Math.min(implicitWidth, win.width - 96)
            wrapMode: Text.Wrap
            text: ctl.error_text
            color: "white"
            font.pixelSize: 16
        }
    }

    SubtitleOverlay {
        controller: ctl
        anchors.fill: parent
        barHeight: ctl.controls_visible ? controlBar.height : 0
    }

    SubtitleList {
        id: subList
        controller: ctl
        visible: win.subListVisible
        width: Math.min(420, win.width * 0.4)
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: controlBar.visible ? controlBar.top : parent.bottom
        onCloseRequested: win.toggleSubList(false)
    }

    ControlBar {
        id: controlBar
        controller: ctl
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        opacity: ctl.controls_visible ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 150 } }
        fullscreen: win.visibility === Window.FullScreen
        onFullscreenToggled: win.apply("fullscreen")
        onOpenRequested: fileDialog.open()
        onSubtitleFileRequested: subtitleDialog.open()
        onSubtitleListRequested: win.toggleSubList(false)
    }

    Toast {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: 32
        text: ctl.toast
        serial: ctl.toast_serial
    }

    DropArea {
        anchors.fill: parent
        onDropped: (drop) => {
            if (drop.hasUrls)
                win.openDropped(drop.urls);
        }
    }

    FileDialog {
        id: fileDialog
        title: ctl.tr("open-file")
        nameFilters: [
            ctl.tr("file-filter-video") + " (*.mp4 *.mkv *.avi *.mov *.webm *.m4v *.ts *.m2ts *.mts *.flv *.wmv *.mpg *.mpeg *.ogv)",
            ctl.tr("file-filter-all") + " (*)"
        ]
        onAccepted: {
            ctl.open_file(selectedFile.toString());
            keys.forceActiveFocus();
        }
    }

    FileDialog {
        id: subtitleDialog
        title: ctl.tr("subtitle-open-file")
        nameFilters: [
            ctl.tr("file-filter-subtitles") + " (*.srt *.ass *.ssa *.vtt)",
            ctl.tr("file-filter-all") + " (*)"
        ]
        onAccepted: {
            ctl.open_subtitle_file(selectedFile.toString());
            keys.forceActiveFocus();
        }
    }

    Timer {
        interval: 250
        running: true
        repeat: true
        onTriggered: ctl.tick()
    }
}
