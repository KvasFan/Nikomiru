// SPDX-License-Identifier: GPL-3.0-only
//
// Text subtitles drawn by Nikomiru over the video (SUB-009, SUB-010, SUB-014).
// Each grapheme is a Text item that knows its byte offset in the cue's
// plain_text, so a click selects exactly that character. Sizes follow the
// height of the visible video: 36 px at 1080 by default, ASS sizes in
// PlayResY units when ASS styles are respected.

import QtQuick
import QtQuick.Effects

Item {
    id: overlay
    required property var controller
    // height of the control bar when it is visible: subtitles move above it
    property real barHeight: 0

    readonly property real srcW: controller.video_width > 0 ? controller.video_width : 16
    readonly property real srcH: controller.video_height > 0 ? controller.video_height : 9
    readonly property real fit: Math.min(width / srcW, height / srcH)
    // the visible video inside the window (letterbox)
    readonly property rect video: Qt.rect((width - srcW * fit) / 2, (height - srcH * fit) / 2,
                                          srcW * fit, srcH * fit)
    readonly property real unit: video.height / 1080
    readonly property real assUnit: controller.subtitle_play_res_y > 0
                                    ? video.height / controller.subtitle_play_res_y : unit
    readonly property real fontPx: Math.max(8, controller.subtitle_font_size * unit)
    readonly property string obscure: controller.subtitle_obscure
    readonly property real sigma: Math.max(12, 0.45 * fontPx)
    readonly property var cues: {
        try { return JSON.parse(controller.subtitle_cues); } catch (e) { return []; }
    }

    // numpad anchor: \an, or the ASS style's when styles are respected (resolved
    // in Rust); bottom centre otherwise
    function anchorOf(cue) {
        return cue.anchor ? cue.anchor : 2;
    }
    function group(cue) {
        if (cue.pos || (cue.vtt && cue.vtt.line))
            return "free";
        const a = anchorOf(cue);
        return a >= 7 ? "top" : (a >= 4 ? "middle" : "bottom");
    }
    function hAlign(cue) {
        if (cue.vtt && cue.vtt.align) {
            const al = cue.vtt.align;
            return (al === "start" || al === "left") ? 0 : (al === "end" || al === "right") ? 2 : 1;
        }
        return (anchorOf(cue) - 1) % 3;
    }
    function marginV(cue) {
        return cue.style && cue.style.margin_v > 0 ? cue.style.margin_v * assUnit : 75 * unit;
    }

    component Cue: Item {
        id: cueItem
        required property var cue
        readonly property int align: overlay.hAlign(cue)
        readonly property var st: cue.style
        implicitWidth: lines.implicitWidth
        implicitHeight: lines.implicitHeight
        width: implicitWidth
        height: implicitHeight
        opacity: overlay.obscure === "hidden" ? 0 : 1

        HoverHandler {
            onHoveredChanged: overlay.controller.subtitle_hover(hovered)
        }

        Column {
            id: lines
            Repeater {
                model: cueItem.cue.lines
                Row {
                    id: row
                    required property var modelData
                    x: cueItem.align === 1 ? (lines.width - width) / 2
                       : cueItem.align === 2 ? lines.width - width : 0
                    Repeater {
                        model: row.modelData
                        Text {
                            id: glyph
                            required property var modelData
                            // the clicked grapheme, then the text the popup matched (POP-002)
                            readonly property bool selected: !!cueItem.cue.hl && modelData.o >= cueItem.cue.hl[0]
                                                             && modelData.o < cueItem.cue.hl[1]
                            text: modelData.t
                            textFormat: Text.PlainText
                            font.pixelSize: modelData.fs ? modelData.fs * overlay.assUnit
                                            : (cueItem.st ? cueItem.st.size * overlay.assUnit : overlay.fontPx)
                            font.family: modelData.fn ? modelData.fn
                                         : (cueItem.st && cueItem.st.font ? cueItem.st.font : Qt.application.font.family)
                            font.weight: (modelData.b === true || (modelData.b === undefined && cueItem.st && cueItem.st.bold))
                                         ? Font.Bold : Font.Normal
                            font.italic: modelData.i === true || (modelData.i === undefined && cueItem.st && cueItem.st.italic)
                            font.underline: modelData.u === true
                            font.strikeout: modelData.s === true
                            color: modelData.c ? modelData.c : (cueItem.st ? cueItem.st.color : "white")
                            style: Text.Outline
                            styleColor: cueItem.st ? cueItem.st.outline_color : "#99000000"

                            Rectangle {
                                anchors.fill: parent
                                z: -1
                                visible: glyph.selected
                                radius: 3
                                color: "#664a90e2"
                            }
                            Text {
                                // ruby reading over the graphemes it covers (WebVTT <rt>)
                                visible: glyph.modelData.ruby !== undefined
                                text: glyph.modelData.ruby || ""
                                color: glyph.color
                                style: Text.Outline
                                styleColor: glyph.styleColor
                                font.pixelSize: glyph.font.pixelSize * 0.5
                                x: (glyph.width * (glyph.modelData.rubyN || 1) - width) / 2
                                anchors.bottom: glyph.top
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    // the popup must not cover the cue (POP-006)
                                    const p = cueItem.mapToItem(null, 0, 0);
                                    overlay.controller.grapheme_clicked(cueItem.cue.cue, glyph.modelData.o,
                                                                        p.x, p.y, cueItem.width, cueItem.height);
                                }
                            }
                        }
                    }
                }
            }
        }

        layer.enabled: true
        layer.effect: MultiEffect {
            shadowEnabled: true
            shadowBlur: 0.5
            shadowColor: "#cc000000"
            shadowVerticalOffset: 1
            blurEnabled: overlay.obscure === "blur"
            blur: 1.0
            blurMax: Math.min(64, Math.round(overlay.sigma * 2))
        }
    }

    // A row of a stack: as wide as the video, with the cue aligned inside it
    // (a Column sets the x of its children itself).
    component Slot: Item {
        id: slot
        required property var modelData
        width: parent ? parent.width : 0
        height: inner.height
        Cue {
            id: inner
            cue: slot.modelData
            x: align === 1 ? (slot.width - width) / 2
               : align === 2 ? slot.width - width - 40 * overlay.unit : 40 * overlay.unit
        }
    }

    // cues at the bottom stack upwards above the margin and the control bar
    Column {
        id: bottom
        width: overlay.video.width
        x: overlay.video.x
        y: Math.min(overlay.video.y + overlay.video.height - overlay.marginV({}),
                    overlay.height - overlay.barHeight - 12 * overlay.unit) - height
        spacing: 4 * overlay.unit
        Repeater {
            model: overlay.cues.filter(c => overlay.group(c) === "bottom")
            Slot {}
        }
    }

    Column {
        id: middle
        width: overlay.video.width
        x: overlay.video.x
        y: overlay.video.y + (overlay.video.height - height) / 2
        Repeater {
            model: overlay.cues.filter(c => overlay.group(c) === "middle")
            Slot {}
        }
    }

    Column {
        id: top
        width: overlay.video.width
        x: overlay.video.x
        y: overlay.video.y + 40 * overlay.unit
        Repeater {
            model: overlay.cues.filter(c => overlay.group(c) === "top")
            Slot {}
        }
    }

    // \pos and WebVTT line/position: placed at their point, aligned by the anchor
    Repeater {
        model: overlay.cues.filter(c => overlay.group(c) === "free")
        Cue {
            required property var modelData
            cue: modelData
            readonly property real px: modelData.pos ? modelData.pos[0]
                                       : (modelData.vtt.position !== null ? modelData.vtt.position / 100 : 0.5)
            readonly property real py: modelData.pos ? modelData.pos[1]
                                       : (modelData.vtt.line.percent !== undefined ? modelData.vtt.line.percent / 100 : 0.9)
            readonly property int an: modelData.pos ? overlay.anchorOf(modelData) : 8
            x: overlay.video.x + px * overlay.video.width - (align === 1 ? width / 2 : align === 2 ? width : 0)
            y: overlay.video.y + py * overlay.video.height - (an >= 7 ? 0 : an >= 4 ? height / 2 : height)
        }
    }
}
