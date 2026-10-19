// SPDX-License-Identifier: GPL-3.0-only
//
// One headword of the popup (POP-007, POP-008): headword with furigana, trace
// steps, tags, frequencies, pitch accents, then one collapsible block per
// dictionary with its glossaries.

import QtQuick

Column {
    id: view
    required property var entry
    property var controller: null
    property int level: 0
    property real base: 15
    spacing: 6

    // headword with furigana (LKP-050)
    Row {
        spacing: 0
        Repeater {
            model: view.entry.furigana
            Column {
                required property var modelData
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: parent.modelData.r === "" ? " " : parent.modelData.r
                    textFormat: Text.PlainText
                    color: "#c5cff5"
                    font.pixelSize: 12
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: parent.modelData.t
                    textFormat: Text.PlainText
                    color: "white"
                    font.pixelSize: 26
                }
            }
        }
    }

    // trace steps, innermost first (LKP-024), and term tags
    Flow {
        width: view.width
        spacing: 4
        visible: view.entry.trace.length > 0 || view.entry.termTags.length > 0
        Repeater {
            model: view.entry.trace
            Chip {
                required property var modelData
                text: modelData.name
                tip: modelData.description
                color: "#24465a"
            }
        }
        Repeater {
            model: view.entry.termTags
            Chip {
                required property var modelData
                text: modelData.name
                tip: modelData.notes
                category: modelData.category
            }
        }
    }

    // frequencies (LKP-040)
    Flow {
        width: view.width
        spacing: 4
        visible: view.entry.frequencies.length > 0
        Repeater {
            model: view.entry.frequencies
            Chip {
                required property var modelData
                text: modelData.dictionary + ": " + modelData.values.join(", ")
                color: "#2f3a2a"
            }
        }
    }

    // pitch accents (LKP-041…043)
    Repeater {
        model: view.entry.pitches
        Flow {
            id: pitchRow
            required property var modelData
            width: view.width
            spacing: 8
            Chip {
                text: pitchRow.modelData.dictionaries.length > 1
                      ? view.controller.tr("popup-pitch-accent") + " · " + pitchRow.modelData.dictionaries.length
                      : pitchRow.modelData.dictionaries[0]
                tip: pitchRow.modelData.dictionaries.join(", ")
            }
            Repeater {
                model: pitchRow.modelData.accents
                Row {
                    id: accent
                    required property var modelData
                    spacing: 0
                    Repeater {
                        model: view.entry.morae
                        Item {
                            required property var modelData
                            required property int index
                            readonly property bool high: accent.modelData.highLow[index] === "H"
                            readonly property bool drop: high && accent.modelData.highLow[index + 1] === "L"
                            width: mora.implicitWidth + 2
                            height: mora.implicitHeight + 6
                            Rectangle {
                                // a line over high morae, a tick where the pitch drops
                                visible: parent.high
                                width: parent.width
                                height: 1.5
                                color: "#f0b35a"
                            }
                            Rectangle {
                                visible: parent.drop
                                x: parent.width - 1.5
                                width: 1.5
                                height: 6
                                color: "#f0b35a"
                            }
                            Text {
                                id: mora
                                y: 5
                                text: parent.modelData
                                textFormat: Text.PlainText
                                color: "#e8e8e8"
                                font.pixelSize: 15
                            }
                        }
                    }
                    Text {
                        y: 5
                        text: " [" + accent.modelData.position + "] " + accent.modelData.category
                        textFormat: Text.PlainText
                        color: "#9aa0aa"
                        font.pixelSize: 12
                    }
                }
            }
            Repeater {
                model: pitchRow.modelData.patterns.concat(pitchRow.modelData.transcriptions)
                Text {
                    required property var modelData
                    text: modelData
                    textFormat: Text.PlainText
                    color: "#e8e8e8"
                    font.pixelSize: 14
                }
            }
        }
    }

    // one block per dictionary (POP-008, DIC-034)
    Repeater {
        model: view.entry.blocks
        Column {
            id: block
            required property var modelData
            property bool expanded: modelData.expanded
            width: view.width
            spacing: 4
            Rectangle {
                width: block.width
                height: title.implicitHeight + 6
                color: "#262a31"
                radius: 4
                Text {
                    id: title
                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: (block.expanded ? "▾ " : "▸ ") + block.modelData.dictionary
                    textFormat: Text.PlainText
                    color: "#c9d1dd"
                    font.pixelSize: 13
                    font.bold: true
                }
                TapHandler {
                    onTapped: block.expanded = !block.expanded
                }
            }
            Repeater {
                model: block.expanded ? block.modelData.glossaries : []
                Column {
                    id: glossary
                    required property var modelData
                    width: block.width
                    spacing: 2
                    Flow {
                        width: glossary.width
                        spacing: 4
                        visible: glossary.modelData.tags.length > 0
                        Repeater {
                            model: glossary.modelData.tags
                            Chip {
                                required property var modelData
                                text: modelData.name
                                tip: modelData.notes
                                category: modelData.category
                                fontSize: 11
                            }
                        }
                    }
                    GlossaryBlocks {
                        width: glossary.width
                        blocks: glossary.modelData.blocks
                        controller: view.controller
                        level: view.level
                        base: view.base
                    }
                }
            }
        }
    }
}
