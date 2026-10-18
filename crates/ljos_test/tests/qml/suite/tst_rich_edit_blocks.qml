import QtQuick
import QtTest

// A read-only rich TextEdit lays out its blocks as a rich Text does:
// paragraph margins, list indents and list markers.
Item {
    width: 400
    height: 400

    TextEdit {
        id: edit
        width: 300
        readOnly: true
        textFormat: TextEdit.RichText
        font.pixelSize: 14
        text: "<p>one</p><p>two</p><ul><li>three</li><li>four</li></ul><p>five</p>"
    }

    Text {
        id: label
        y: 200
        width: 300
        textFormat: Text.RichText
        font.pixelSize: 14
        text: edit.text
    }

    TestCase {
        name: "RichEditBlocks"
        when: windowShown

        function rectOf(word) {
            const plain = edit.getText(0, edit.length);
            return edit.positionToRectangle(plain.indexOf(word));
        }

        function test_blocks() {
            const one = rectOf("one");
            const two = rectOf("two");
            const three = rectOf("three");
            const five = rectOf("five");
            console.log("plain", JSON.stringify(edit.getText(0, edit.length)));
            console.log("one", one.x, one.y, "two", two.x, two.y, "three", three.x, three.y, "five", five.x, five.y);
            console.log("edit height", edit.implicitHeight, "text height", label.implicitHeight);
            // A paragraph's 12px margin sits between "one" and "two".
            verify(two.y - one.y >= one.height + 10, "paragraph gap");
            // A list item is indented.
            verify(three.x >= 20, "list indent");
            // Same height as a rich Text of the same source.
            fuzzyCompare(edit.implicitHeight, label.implicitHeight, 2);
        }
    }
}
