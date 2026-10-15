// 5000 words of wrapped Text: shaping and laying out a long paragraph,
// once.
import QtQuick

Rectangle {
    width: 800
    height: 600
    color: "white"

    Text {
        anchors.fill: parent
        anchors.margins: 8
        wrapMode: Text.WordWrap
        clip: true
        font.pixelSize: 12
        text: {
            const words = ["the", "quick", "brown", "fox", "jumps", "over", "a", "lazy",
                           "dog", "while", "light", "falls", "on", "northern", "hills",
                           "and", "every", "small", "window", "glows"];
            const out = [];
            for (let i = 0; i < 5000; ++i)
                out.push(words[(i * 7 + Math.floor(i / 20)) % words.length]);
            return out.join(" ");
        }
    }
}
