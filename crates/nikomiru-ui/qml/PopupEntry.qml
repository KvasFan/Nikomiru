// SPDX-License-Identifier: GPL-3.0-only
//
// One headword of the popup (POP-007, POP-008): headword with furigana, trace
// steps, tags, frequencies, pitch accents, then one collapsible block per
// dictionary with its glossaries. Repeaters get counts, not arrays of objects
// (see GlossaryBlocks).

import QtQuick

Column {
    id: view
    required property var entry
    property var controller: null
    property int level: 0
    property real base: 15
    property var budget: null
    // the shared GlossaryBlocks components (see GlossaryBlocks)
    property var blocksComponents: null
    spacing: 6

    // headword with furigana (LKP-050)
    Row {
        spacing: 0
        Repeater {
            model: view.entry.furigana.length
            Column {
                required property int index
                readonly property var part: view.entry.furigana[index]
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: parent.part.r === "" ? " " : parent.part.r
                    textFormat: Text.PlainText
                    color: "#c5cff5"
                    font.pixelSize: 12
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: parent.part.t
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
            model: view.entry.trace.length
            Chip {
                required property int index
                readonly property var step: view.entry.trace[index]
                text: step.name
                tip: step.description
                color: "#24465a"
            }
        }
        Repeater {
            model: view.entry.termTags.length
            Chip {
                required property int index
                readonly property var tag: view.entry.termTags[index]
                text: tag.name
                tip: tag.notes
                category: tag.category
            }
        }
    }

    // frequencies (LKP-040)
    Flow {
        width: view.width
        spacing: 4
        visible: view.entry.frequencies.length > 0
        Repeater {
            model: view.entry.frequencies.length
            Chip {
                required property int index
                readonly property var frequency: view.entry.frequencies[index]
                text: frequency.dictionary + ": " + frequency.values.join(", ")
                color: "#2f3a2a"
            }
        }
    }

    // pitch accents (LKP-041…043)
    Repeater {
        model: view.entry.pitches.length
        Flow {
            id: pitchRow
            required property int index
            readonly property var pitch: view.entry.pitches[index]
            width: view.width
            spacing: 8
            Chip {
                text: pitchRow.pitch.dictionaries.length > 1
                      ? view.controller.tr("popup-pitch-accent") + " · " + pitchRow.pitch.dictionaries.length
                      : pitchRow.pitch.dictionaries[0]
                tip: pitchRow.pitch.dictionaries.join(", ")
            }
            Repeater {
                model: pitchRow.pitch.accents.length
                Row {
                    id: accent
                    required property int index
                    readonly property var pattern: pitchRow.pitch.accents[index]
                    spacing: 0
                    Repeater {
                        model: view.entry.morae
                        Item {
                            required property var modelData
                            required property int index
                            readonly property bool high: accent.pattern.highLow[index] === "H"
                            readonly property bool drop: high && accent.pattern.highLow[index + 1] === "L"
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
                        text: " [" + accent.pattern.position + "] " + accent.pattern.category
                        textFormat: Text.PlainText
                        color: "#9aa0aa"
                        font.pixelSize: 12
                    }
                }
            }
            Repeater {
                model: pitchRow.pitch.patterns.concat(pitchRow.pitch.transcriptions)
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
        model: view.entry.blocks.length
        Column {
            id: block
            required property int index
            readonly property var section: view.entry.blocks[index]
            property bool expanded: section.expanded
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
                    text: (block.expanded ? "▾ " : "▸ ") + block.section.dictionary
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
                model: block.expanded ? block.section.glossaries.length : 0
                Column {
                    id: glossary
                    required property int index
                    readonly property var gloss: block.section.glossaries[index]
                    width: block.width
                    spacing: 2
                    Flow {
                        width: glossary.width
                        spacing: 4
                        visible: glossary.gloss.tags.length > 0
                        Repeater {
                            model: glossary.gloss.tags.length
                            Chip {
                                required property int index
                                readonly property var tag: glossary.gloss.tags[index]
                                text: tag.name
                                tip: tag.notes
                                category: tag.category
                                fontSize: 11
                            }
                        }
                    }
                    GlossaryBlocks {
                        width: glossary.width
                        blocks: glossary.gloss.blocks
                        controller: view.controller
                        level: view.level
                        base: view.base
                        budget: view.budget
                        components: view.blocksComponents
                    }
                }
            }
        }
    }
}
