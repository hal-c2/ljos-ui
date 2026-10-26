// A ListView of 10 000 rows, each a sender and a message wrapped on two
// to four lines (a fourth is elided into the third), scrolled from the
// first to the last and back: the rows of a chat, a mail box or a list
// with descriptions, made and dropped as they pass.
import QtQuick

Rectangle {
    id: root
    width: 400
    height: 600
    color: "white"

    readonly property var words: ["the", "quick", "brown", "fox", "jumps", "over", "a",
        "lazy", "dog", "while", "light", "falls", "on", "northern", "hills", "and",
        "every", "small", "window", "glows", "well-known", "re-read", "tomorrow"]

    ListView {
        id: list
        anchors.fill: parent
        model: 10000
        delegate: Rectangle {
            id: row
            required property int index
            width: ListView.view.width
            height: 80
            color: index % 2 ? "#eeeeee" : "white"
            Text {
                id: sender
                x: 12
                y: 4
                font.bold: true
                text: "Sender " + row.index
            }
            Text {
                x: 12
                y: sender.y + sender.height + 2
                width: row.width - 24
                wrapMode: Text.WordWrap
                maximumLineCount: 3
                elide: Text.ElideRight
                text: {
                    const w = root.words;
                    const out = ["Message", String(row.index) + ":"];
                    const n = 14 + (row.index * 7) % 22;
                    for (let i = 0; i < n; ++i)
                        out.push(w[(row.index * 5 + i * 3 + Math.floor(i / 4)) % w.length]);
                    return out.join(" ") + ".";
                }
            }
        }

        SequentialAnimation on contentY {
            loops: Animation.Infinite
            NumberAnimation { from: 0; to: 10000 * 80 - 600; duration: 20000 }
            NumberAnimation { from: 10000 * 80 - 600; to: 0; duration: 20000 }
        }
    }
}
