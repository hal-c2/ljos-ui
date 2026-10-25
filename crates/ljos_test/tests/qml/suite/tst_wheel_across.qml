import QtQuick
import QtTest

// The wheel over a list that runs across, alone and inside a Flickable that
// runs down. A list moves only along its own way: a lengthways turn leaves
// it where it is and goes on to what it is in, and a turn both ways is
// taken by the list for the part it can use. Measured on Qt 6.11.2.
Item {
    id: window
    width: 400
    height: 200

    ListView {
        id: alone
        width: 200; height: 50
        orientation: ListView.Horizontal
        model: 50
        delegate: Rectangle { width: 40; height: 50 }
    }

    Flickable {
        id: outer
        x: 200; width: 200; height: 200
        contentWidth: 200; contentHeight: 1000
        ListView {
            id: inner
            width: 200; height: 50
            orientation: ListView.Horizontal
            model: 50
            delegate: Rectangle { width: 40; height: 50 }
        }
    }

    TestCase {
        name: "WheelAcross"
        when: windowShown

        function init() {
            alone.contentX = 0;
            inner.contentX = 0;
            outer.contentY = 0;
            wait(400);
        }

        function test_lengthways_leaves_a_list_alone() {
            mouseWheel(window, 100, 25, 0, -120);
            wait(400);
            compare(alone.contentX, 0);
            compare(alone.contentY, 0);
        }

        function test_sideways_moves_it() {
            mouseWheel(window, 100, 25, -120, 0);
            tryCompare(alone, "contentX", 72);
        }

        function test_lengthways_goes_to_what_it_is_in() {
            mouseWheel(window, 300, 25, 0, -120);
            tryCompare(outer, "contentY", 72);
            compare(inner.contentX, 0);
        }

        function test_both_ways_is_kept_by_the_list() {
            mouseWheel(window, 300, 25, -120, -120);
            tryCompare(inner, "contentX", 72);
            wait(400);
            compare(outer.contentY, 0);
        }
    }
}
