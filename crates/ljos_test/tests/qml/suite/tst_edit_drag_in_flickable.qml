import QtQuick
import QtTest

// A mouse drag down a selectable TextEdit inside a ListView that can scroll
// selects text; the list does not take the drag. The presses stay clear of
// the text's top left corner: Qt 6.11 takes a press within ten pixels of
// it for a triple click, which selects whole lines.
Item {
    width: 400
    height: 200

    ListView {
        id: list
        anchors.fill: parent
        model: 6
        delegate: TextEdit {
            required property int index
            width: ListView.view.width
            readOnly: true
            selectByMouse: true
            font.pixelSize: 14
            text: "row " + index + " first line\nrow " + index + " second line\nrow " + index + " third line"
        }
    }

    TestCase {
        name: "EditDragInFlickable"
        when: windowShown

        function test_vertical_drag_selects() {
            const edit = list.itemAtIndex(0);
            verify(list.contentHeight > list.height, "the list can scroll");
            mousePress(edit, 20, 5);
            for (let y = 10; y <= 45; y += 5)
                mouseMove(edit, 60, y, -1, Qt.LeftButton);
            mouseRelease(edit, 60, 45);
            console.log("selected", JSON.stringify(edit.selectedText), "contentY", list.contentY);
            compare(list.contentY, 0, "the list did not move");
            verify(edit.selectedText.indexOf("second") >= 0, "the drag selected across lines");
        }

        function test_horizontal_drag_selects() {
            const edit = list.itemAtIndex(1);
            edit.deselect();
            mousePress(edit, 20, 5);
            for (let x = 30; x <= 80; x += 10)
                mouseMove(edit, x, 6, -1, Qt.LeftButton);
            mouseRelease(edit, 80, 6);
            console.log("selected", JSON.stringify(edit.selectedText));
            verify(edit.selectedText.length > 0);
        }
    }
}
