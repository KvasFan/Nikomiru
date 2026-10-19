// SPDX-License-Identifier: GPL-3.0-only
//
// A kanji entry (POP-013, LKP-037): the character, its readings, meanings,
// strokes, radical, statistics and frequencies.

import QtQuick

Row {
    id: card
    required property var kanji
    spacing: 12

    Text {
        text: card.kanji.character
        textFormat: Text.PlainText
        color: "white"
        font.pixelSize: 56
    }

    Column {
        width: card.width - x
        spacing: 4
        Text {
            text: card.kanji.dictionary
            textFormat: Text.PlainText
            color: "#9aa0aa"
            font.pixelSize: 12
        }
        Flow {
            width: parent.width
            spacing: 4
            Repeater {
                model: card.kanji.onyomi
                Chip {
                    required property var modelData
                    text: modelData
                    color: "#3a2a44"
                }
            }
            Repeater {
                model: card.kanji.kunyomi
                Chip {
                    required property var modelData
                    text: modelData
                    color: "#2a3a44"
                }
            }
        }
        Text {
            width: parent.width
            text: card.kanji.meanings.join(", ")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: "#e8e8e8"
            font.pixelSize: 15
        }
        Flow {
            width: parent.width
            spacing: 4
            Chip {
                visible: !!card.kanji.strokes
                text: "strokes: " + card.kanji.strokes
            }
            Chip {
                visible: !!card.kanji.radical
                text: "radical: " + card.kanji.radical
            }
            Repeater {
                model: card.kanji.stats
                Chip {
                    required property var modelData
                    text: modelData[0] + ": " + modelData[1]
                }
            }
            Repeater {
                model: card.kanji.frequencies
                Chip {
                    required property var modelData
                    text: modelData.dictionary + ": " + modelData.values.join(", ")
                    color: "#2f3a2a"
                }
            }
        }
    }
}
