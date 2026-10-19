// SPDX-License-Identifier: GPL-3.0-only
//
// Block content of a glossary (POP-009): flows of inline runs, boxes, lists,
// tables (scrollable) and details, nested as the dictionary wrote them. Built
// from the view model of nikomiru_dict::structured, never from markup. Nested
// blocks load this file again through a Loader, so a level is only created
// when it exists.

import QtQuick
import QtQuick.Layouts

Column {
    id: root
    property var blocks: []
    property var controller: null
    property int level: 0
    property real base: 15
    spacing: 3

    function px(length, size) {
        if (!length)
            return 0;
        return length.u === "em" ? length.v * (size || root.base) : length.v;
    }
    function side(sides, name) {
        return sides && sides[name] ? root.px(sides[name]) : 0;
    }
    function marker(style, ordered, index) {
        const m = style && style.list_marker ? style.list_marker : "";
        if (m.startsWith("\""))
            return m.slice(1, -1);
        switch (m) {
        case "none": return "";
        case "circle": return "◦";
        case "square": return "▪";
        case "disc": return "•";
        case "decimal": return (index + 1) + ".";
        case "lower-alpha": return String.fromCharCode(97 + index % 26) + ".";
        case "upper-alpha": return String.fromCharCode(65 + index % 26) + ".";
        }
        return ordered ? (index + 1) + "." : "•";
    }

    Repeater {
        model: root.blocks
        Loader {
            required property var modelData
            width: root.width
            sourceComponent: {
                switch (modelData.k) {
                case "flow": return flowBlock;
                case "box": return boxBlock;
                case "list": return listBlock;
                case "table": return tableBlock;
                case "details": return detailsBlock;
                case "pairs": return pairsBlock;
                }
                return null;
            }
        }
    }

    // a nested GlossaryBlocks, created when needed
    component Nested: Loader {
        property var nested: []
        Component.onCompleted: setSource("GlossaryBlocks.qml", {
            blocks: nested,
            controller: root.controller,
            level: root.level,
            base: root.base,
            width: Qt.binding(() => width)
        })
    }

    Component {
        id: flowBlock
        GlossaryRuns {
            width: parent ? parent.width : 0
            runs: parent ? parent.modelData.runs : []
            controller: root.controller
            level: root.level
            base: root.base
        }
    }

    Component {
        id: boxBlock
        Item {
            readonly property var b: parent ? parent.modelData : ({})
            readonly property var s: b.s || ({})
            width: parent ? parent.width : 0
            implicitHeight: frame.height + root.side(s.margin, "top") + root.side(s.margin, "bottom")
            Rectangle {
                id: frame
                x: root.side(parent.s.margin, "left")
                y: root.side(parent.s.margin, "top")
                width: parent.width - x - root.side(parent.s.margin, "right")
                height: inner.implicitHeight + root.side(parent.s.padding, "top") + root.side(parent.s.padding, "bottom")
                color: parent.s.background || "transparent"
                radius: root.px(parent.s.border_radius)
                border.color: parent.s.border_color || "transparent"
                border.width: parent.s.border_color ? Math.max(1, root.px(parent.s.border_width)) : 0
                Nested {
                    id: inner
                    nested: parent.parent.b.blocks || []
                    x: root.side(parent.parent.s.padding, "left")
                    y: root.side(parent.parent.s.padding, "top")
                    width: parent.width - x - root.side(parent.parent.s.padding, "right")
                }
            }
        }
    }

    Component {
        id: listBlock
        Column {
            readonly property var b: parent ? parent.modelData : ({})
            width: parent ? parent.width : 0
            leftPadding: root.side(b.s ? b.s.margin : null, "left")
            spacing: 2
            Repeater {
                model: parent.b.items || []
                Row {
                    id: item
                    required property var modelData
                    required property int index
                    readonly property var list: parent.b
                    spacing: 4
                    Text {
                        id: bullet
                        text: root.marker(item.list.s, item.list.ordered, item.index)
                        textFormat: Text.PlainText
                        color: "#aab2bf"
                        font.pixelSize: root.base
                        width: text === "" ? 0 : Math.max(implicitWidth, root.base)
                    }
                    Nested {
                        nested: item.modelData
                        width: item.parent.width - item.parent.leftPadding - bullet.width - 4
                    }
                }
            }
        }
    }

    Component {
        id: tableBlock
        Flickable {
            id: scroll
            readonly property var b: parent ? parent.modelData : ({})
            width: parent ? parent.width : 0
            height: grid.implicitHeight
            contentWidth: grid.implicitWidth
            contentHeight: grid.implicitHeight
            clip: true
            flickableDirection: Flickable.HorizontalFlick
            boundsBehavior: Flickable.StopAtBounds
            GridLayout {
                id: grid
                columns: Math.max(1, scroll.b.columns || 1)
                rowSpacing: 0
                columnSpacing: 0
                Repeater {
                    model: {
                        // the cells of every row (arrays from Rust have no flat())
                        const cells = [];
                        const rows = scroll.b.rows || [];
                        for (let r = 0; r < rows.length; r++)
                            for (let c = 0; c < rows[r].length; c++)
                                cells.push(rows[r][c]);
                        return cells;
                    }
                    Rectangle {
                        id: cell
                        required property var modelData
                        Layout.row: modelData.row
                        Layout.column: modelData.col
                        Layout.rowSpan: modelData.rowspan
                        Layout.columnSpan: modelData.colspan
                        Layout.fillHeight: true
                        Layout.fillWidth: true
                        // the width its text needs, measured in Rust (a wrapping
                        // text has no natural width of its own)
                        Layout.preferredWidth: Math.min(280, Math.max(32, modelData.naturalEm * root.base + 14))
                        Layout.preferredHeight: cellContent.implicitHeight + 8
                        color: modelData.s && modelData.s.background ? modelData.s.background
                               : (modelData.header ? "#2c3038" : "transparent")
                        border.color: "#555b66"
                        border.width: 1
                        Nested {
                            id: cellContent
                            x: 6
                            y: 4
                            nested: cell.modelData.blocks
                            width: cell.width - 12
                        }
                    }
                }
            }
        }
    }

    Component {
        id: detailsBlock
        Column {
            id: details
            readonly property var b: parent ? parent.modelData : ({})
            property bool open: b.open === true
            width: parent ? parent.width : 0
            spacing: 2
            Row {
                spacing: 4
                Text {
                    text: details.open ? "▾" : "▸"
                    color: "#aab2bf"
                    font.pixelSize: root.base
                }
                GlossaryRuns {
                    width: details.width - 20
                    runs: details.b.summary || []
                    controller: root.controller
                    level: root.level
                    base: root.base
                }
                TapHandler {
                    onTapped: details.open = !details.open
                }
            }
            Loader {
                active: details.open
                width: details.width
                sourceComponent: Nested {
                    nested: details.b.blocks || []
                    width: details.width - 16
                    x: 16
                }
            }
        }
    }

    Component {
        id: pairsBlock
        Column {
            readonly property var b: parent ? parent.modelData : ({})
            width: parent ? parent.width : 0
            spacing: 2
            Repeater {
                model: parent.b.items || []
                Flow {
                    id: pair
                    required property var modelData
                    width: parent.width
                    spacing: 4
                    Text {
                        text: pair.modelData[0]
                        textFormat: Text.PlainText
                        color: "#e8e8e8"
                        font.pixelSize: root.base
                    }
                    Repeater {
                        model: pair.modelData[1]
                        Chip {
                            required property var modelData
                            text: modelData
                        }
                    }
                }
            }
        }
    }
}
