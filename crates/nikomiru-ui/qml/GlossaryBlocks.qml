// SPDX-License-Identifier: GPL-3.0-only
//
// Block content of a glossary (POP-009): flows of inline runs, boxes, lists,
// tables (scrollable) and details, nested as the dictionary wrote them. Built
// from the view model of nikomiru_dict::structured, never from markup. Nested
// blocks create this file again, so a level is only created when it exists.
//
// Repeaters get counts, never the arrays: a Repeater turns an array of
// objects into variants, deeply, and each delegate's modelData back into
// JavaScript, so nested content would be copied at every level.

import QtQuick
import QtQuick.Layouts

Column {
    id: root
    property var blocks: []
    property var controller: null
    property int level: 0
    property real base: 15
    // the popup's budget of items created at once (DictionaryPopups); the
    // others are created in the next frames, so a long entry shows its
    // start at once (NFR-PERF-06)
    property var budget: null
    // this file as a component, one per depth (components.get(depth)), made
    // once and shared by the popups: QML cannot create a file inside itself,
    // loading it by URL at every level costs more, and a component cannot
    // create an object while it is still completing an ancestor
    property var components: null
    property int depth: 0
    spacing: 3

    function px(length, size) {
        if (!length)
            return 0;
        return length.u === "em" ? length.v * (size || root.base) : length.v;
    }
    function side(sides, name) {
        return sides && sides[name] ? root.px(sides[name]) : 0;
    }
    // the item's own marker (Jitendex's ①, ②…) or the list's
    function marker(list, index) {
        const own = list.markers ? list.markers[index] : null;
        const m = own || (list.s && list.s.list_marker ? list.s.list_marker : "");
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
        return list.ordered ? (index + 1) + "." : "•";
    }

    Repeater {
        model: root.blocks.length
        Loader {
            required property int index
            readonly property var block: root.blocks[index]
            width: root.width
            asynchronous: root.budget ? root.budget.spend() : index >= 3
            sourceComponent: {
                switch (block.k) {
                case "text": return textBlock;
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
    component Nested: Item {
        id: nest
        property var nested: []
        property Item content: null
        implicitHeight: content ? content.implicitHeight : 0
        Component.onCompleted: {
            const c = root.components ? root.components.get(root.depth)
                                      : Qt.createComponent("GlossaryBlocks.qml");
            content = c.createObject(nest, {
                blocks: nest.nested,
                controller: root.controller,
                level: root.level,
                base: root.base,
                budget: root.budget,
                components: root.components,
                depth: root.depth + 1,
                width: Qt.binding(() => nest.width)
            });
        }
    }

    // a paragraph without ruby, images or chips: one Text. The markup comes
    // from Rust, generated from escaped text with a fixed set of tags
    // (NFR-SEC-01); links are indices into the block's targets.
    Component {
        id: textBlock
        Text {
            id: paragraph
            readonly property var b: parent ? parent.block : ({})
            width: parent ? parent.width : 0
            text: b.html || ""
            textFormat: Text.StyledText
            wrapMode: Text.Wrap
            color: "#e8e8e8"
            linkColor: "#8ab4ff"
            font.pixelSize: root.base
            onLinkActivated: (link) => {
                const target = (paragraph.b.links || [])[parseInt(link)];
                if (!target)
                    return;
                if (target.kind === "external") {
                    // only web pages, in the system browser (POP-011)
                    if (/^https?:\/\//i.test(target.to))
                        Qt.openUrlExternally(target.to);
                } else if (root.controller) {
                    const p = paragraph.mapToItem(null, 0, 0);
                    const query = target.to, level = root.level, ctl = root.controller;
                    Qt.callLater(() => ctl.popup_lookup(level, query, p.x, p.y, paragraph.width, paragraph.height));
                }
            }
            HoverHandler {
                enabled: (paragraph.b.links || []).length > 0
                cursorShape: paragraph.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
            }
        }
    }

    Component {
        id: flowBlock
        GlossaryRuns {
            width: parent ? parent.width : 0
            runs: parent ? parent.block.runs : []
            controller: root.controller
            level: root.level
            base: root.base
            budget: root.budget
        }
    }

    Component {
        id: boxBlock
        Item {
            readonly property var b: parent ? parent.block : ({})
            readonly property var s: b.s || ({})
            width: parent ? parent.width : 0
            implicitHeight: frame.height + root.side(s.margin, "top") + root.side(s.margin, "bottom")
            Rectangle {
                id: frame
                x: root.side(parent.s.margin, "left")
                y: root.side(parent.s.margin, "top")
                width: parent.width - x - root.side(parent.s.margin, "right")
                height: inner.implicitHeight + root.side(parent.s.padding, "top") + root.side(parent.s.padding, "bottom")
                // a border on some sides only (Jitendex's note boxes: left)
                readonly property var sides: parent.s.border_sides || null
                readonly property bool someSides: !!sides && !(sides[0] && sides[1] && sides[2] && sides[3])
                readonly property real line: parent.s.border_width ? Math.max(1, root.px(parent.s.border_width)) : (someSides ? 3 : 1)
                readonly property color lineColor: parent.s.border_color || (someSides ? "#80909aa8" : "transparent")
                color: parent.s.background || "transparent"
                radius: someSides ? 0 : root.px(parent.s.border_radius)
                border.color: someSides ? "transparent" : (parent.s.border_color || "transparent")
                border.width: !someSides && parent.s.border_color ? line : 0
                Repeater {
                    model: frame.someSides ? 4 : 0
                    Rectangle {
                        required property int index
                        visible: frame.sides[index]
                        color: frame.lineColor
                        x: index === 1 ? frame.width - frame.line : 0
                        y: index === 2 ? frame.height - frame.line : 0
                        width: index === 0 || index === 2 ? frame.width : frame.line
                        height: index === 0 || index === 2 ? frame.line : frame.height
                    }
                }
                Nested {
                    id: inner
                    nested: parent.parent.b.blocks || []
                    x: root.side(parent.parent.s.padding, "left") + (frame.someSides && frame.sides[3] ? frame.line + 4 : 0)
                    y: root.side(parent.parent.s.padding, "top")
                    width: parent.width - x - root.side(parent.parent.s.padding, "right")
                }
            }
        }
    }

    Component {
        id: listBlock
        Column {
            readonly property var b: parent ? parent.block : ({})
            width: parent ? parent.width : 0
            leftPadding: root.side(b.s ? b.s.margin : null, "left")
            spacing: 2
            Repeater {
                model: parent.b.items ? parent.b.items.length : 0
                Row {
                    id: item
                    required property int index
                    readonly property var list: parent.b
                    spacing: 4
                    Text {
                        id: bullet
                        text: root.marker(item.list, item.index)
                        textFormat: Text.PlainText
                        color: "#aab2bf"
                        font.pixelSize: root.base
                        width: text === "" ? 0 : Math.max(implicitWidth, root.base)
                    }
                    Nested {
                        nested: item.list.items[item.index]
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
            readonly property var b: parent ? parent.block : ({})
            // the cells of every row (arrays from Rust have no flat())
            readonly property var cells: {
                const cells = [];
                const rows = b.rows || [];
                for (let r = 0; r < rows.length; r++)
                    for (let c = 0; c < rows[r].length; c++)
                        cells.push(rows[r][c]);
                return cells;
            }
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
                    model: scroll.cells.length
                    Rectangle {
                        id: cell
                        required property int index
                        readonly property var cellData: scroll.cells[index]
                        Layout.row: cellData.row
                        Layout.column: cellData.col
                        Layout.rowSpan: cellData.rowspan
                        Layout.columnSpan: cellData.colspan
                        Layout.fillHeight: true
                        Layout.fillWidth: true
                        // the width its text needs, measured in Rust (a wrapping
                        // text has no natural width of its own)
                        Layout.preferredWidth: Math.min(280, Math.max(32, cellData.naturalEm * root.base + 14))
                        Layout.preferredHeight: cellContent.implicitHeight + 8
                        color: cellData.s && cellData.s.background ? cellData.s.background
                               : (cellData.header ? "#2c3038" : "transparent")
                        border.color: "#555b66"
                        border.width: 1
                        Nested {
                            id: cellContent
                            x: 6
                            y: 4
                            nested: cell.cellData.blocks
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
            readonly property var b: parent ? parent.block : ({})
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
            readonly property var b: parent ? parent.block : ({})
            width: parent ? parent.width : 0
            spacing: 2
            Repeater {
                model: parent.b.items ? parent.b.items.length : 0
                Flow {
                    id: pair
                    required property int index
                    readonly property var pairData: parent.b.items[index]
                    width: parent.width
                    spacing: 4
                    Text {
                        text: pair.pairData[0]
                        textFormat: Text.PlainText
                        color: "#e8e8e8"
                        font.pixelSize: root.base
                    }
                    Repeater {
                        model: pair.pairData[1]
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
