// SPDX-License-Identifier: GPL-3.0-only
//
// Inline content of a glossary (POP-009): text runs with their style, ruby,
// links, images and line breaks, laid out in a flow. Every text is plain text:
// nothing from a dictionary is read as markup (NFR-SEC-01). Images are data
// URLs made from the dictionary's media table (POP-010).

import QtQuick

Flow {
    id: flow
    property var runs: []
    property var controller: null
    property int level: 0
    property real base: 15
    property color textColor: "#e8e8e8"
    // the popup's budget of items created at once (see GlossaryBlocks)
    property var budget: null

    // text next to ruby starts lower, level with the base of the ruby
    readonly property real rubyLift: {
        for (let i = 0; i < runs.length; i++)
            if (runs[i].k === "ruby")
                return base * 0.55 * 1.35;
        return 0;
    }

    function px(length, size) {
        if (!length)
            return 0;
        return length.u === "em" ? length.v * size : length.v;
    }

    Repeater {
        model: flow.runs
        Loader {
            id: run
            required property var modelData
            asynchronous: flow.budget ? flow.budget.spend() : false
            sourceComponent: {
                switch (modelData.k) {
                case "text": return textRun;
                case "ruby": return rubyRun;
                case "link": return linkRun;
                case "image": return imageRun;
                case "br": return breakRun;
                }
                return null;
            }
        }
    }

    Component {
        id: textRun
        Text {
            readonly property var r: parent ? parent.modelData : ({})
            readonly property var s: r.s || ({})
            readonly property real size: flow.base * (s.font_scale || 1)
            text: r.t || ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            width: Math.min(implicitWidth, flow.width)
            color: s.color || flow.textColor
            font.pixelSize: s.vertical_align ? size * 0.75 : size
            font.bold: s.bold === true
            font.italic: s.italic === true
            font.underline: s.underline === true
            font.strikeout: s.strike === true
            topPadding: flow.rubyLift + flow.px(s.padding ? s.padding.top : null, size)
            bottomPadding: flow.px(s.padding ? s.padding.bottom : null, size)
            leftPadding: flow.px(s.padding ? s.padding.left : null, size)
            rightPadding: flow.px(s.padding ? s.padding.right : null, size)
            Rectangle {
                z: -1
                anchors.fill: parent
                anchors.topMargin: flow.rubyLift
                visible: !!parent.s.background || !!parent.s.border_color
                color: parent.s.background || "transparent"
                radius: flow.px(parent.s.border_radius, parent.size)
                border.color: parent.s.border_color || "transparent"
                border.width: parent.s.border_color ? Math.max(1, flow.px(parent.s.border_width, parent.size)) : 0
            }
        }
    }

    Component {
        id: rubyRun
        Column {
            readonly property var r: parent ? parent.modelData : ({})
            readonly property var s: r.s || ({})
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: parent.r.rt || ""
                textFormat: Text.PlainText
                color: parent.s.color || flow.textColor
                font.pixelSize: flow.base * 0.55
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: parent.r.t || ""
                textFormat: Text.PlainText
                color: parent.s.color || flow.textColor
                font.pixelSize: flow.base * (parent.s.font_scale || 1)
                font.bold: parent.s.bold === true
            }
        }
    }

    Component {
        id: linkRun
        Text {
            id: link
            readonly property var r: parent ? parent.modelData : ({})
            text: r.t || ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            width: Math.min(implicitWidth, flow.width)
            color: "#8ab4ff"
            font.pixelSize: flow.base * ((r.s && r.s.font_scale) || 1)
            font.underline: true
            topPadding: flow.rubyLift
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    const target = link.r.target;
                    if (target.kind === "external") {
                        // only web pages, in the system browser (POP-011)
                        if (/^https?:\/\//i.test(target.to))
                            Qt.openUrlExternally(target.to);
                    } else if (flow.controller) {
                        const p = link.mapToItem(null, 0, 0);
                        const query = target.to;
                        const level = flow.level;
                        const ctl = flow.controller;
                        Qt.callLater(() => ctl.popup_lookup(level, query, p.x, p.y, link.width, link.height));
                    }
                }
            }
        }
    }

    Component {
        id: imageRun
        Item {
            readonly property var im: parent && parent.modelData ? parent.modelData.image : ({})
            readonly property real unit: im.units === "em" ? flow.base : 1
            implicitWidth: pic.visible ? pic.width : alt.implicitWidth
            implicitHeight: pic.visible ? pic.height : alt.implicitHeight
            Image {
                id: pic
                visible: !!parent.im.src && !parent.im.collapsed
                source: visible ? parent.im.src : ""
                asynchronous: true
                fillMode: Image.PreserveAspectFit
                smooth: !parent.im.pixelated
                width: Math.min(flow.width, parent.im.width ? parent.im.width * parent.unit
                                            : (parent.im.height && sourceSize.height > 0
                                               ? parent.im.height * parent.unit * sourceSize.width / sourceSize.height
                                               : sourceSize.width))
                height: parent.im.height ? parent.im.height * parent.unit
                        : (sourceSize.width > 0 ? width * sourceSize.height / sourceSize.width : 0)
            }
            Text {
                id: alt
                visible: !pic.visible
                text: "[" + (parent.im.alt || parent.im.title || "image") + "]"
                textFormat: Text.PlainText
                color: "#9aa0aa"
                font.pixelSize: flow.base * 0.85
            }
        }
    }

    Component {
        id: breakRun
        Item {
            width: flow.width
            height: 0
        }
    }
}
