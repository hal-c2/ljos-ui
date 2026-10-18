// A wrapping TextArea that is the one item of a ScrollView takes the
// view's width and grows with its lines (QQuickTextArea attaches to the
// Flickable the view makes for it). Passes on qmltestrunner (Qt 6).
import QtQuick
import QtQuick.Controls
import QtTest

Item {
    width: 400
    height: 300

    ScrollView {
        id: view
        width: 200
        height: 150

        TextArea {
            id: area
            wrapMode: TextEdit.Wrap
            text: "a line of words that is a great deal wider than the two hundred pixels of the view it is in"
        }
    }

    TestCase {
        name: "TextAreaInScrollView"
        when: windowShown

        function test_the_area_is_as_wide_as_the_view() {
            compare(area.width, view.availableWidth);
        }

        function test_the_text_wraps_onto_several_lines() {
            verify(area.lineCount > 1, "lineCount " + area.lineCount);
            verify(area.contentWidth <= view.availableWidth, "contentWidth " + area.contentWidth);
        }

        function test_the_view_scrolls_the_wrapped_height_and_no_width() {
            compare(view.contentHeight, area.implicitHeight);
            verify(view.contentWidth <= view.availableWidth, "contentWidth " + view.contentWidth);
        }

        function test_a_new_line_makes_the_area_taller() {
            const before = area.implicitHeight;
            area.text = area.text + "\nand another";
            verify(area.implicitHeight > before);
            compare(view.contentHeight, area.implicitHeight);
        }
    }
}
