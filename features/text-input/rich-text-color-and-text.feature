# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#color-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#font-prop
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A rich TextEdit whose color and text change together
  A TextEdit's color and its rich text may follow the same property, as
  they do when both follow a light or dark appearance. When that property
  changes, the editor shows the new text in the new color: the text that
  was there before is not kept.

  Rule: The text and the color change as one property changes

    Scenario Outline: The new text comes in <markup> as the color changes
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool lit: true
          property string chars
          property bool blue
          function look() {
            chars = edit.getText(0, edit.length)
            blue = edit.text.indexOf("#60a5fa") >= 0
          }
          function shade() { return root.lit ? "#1d4ed8" : "#60a5fa" }
          TextEdit {
            id: edit
            textFormat: TextEdit.RichText
            color: root.lit ? "black" : "white"
            text: <text>
          }
        }
        """
      When the property "lit" of "root" is set to false
      And the method "look" of "root" is called
      Then the property "chars" of "root" is "c"
      And the property "blue" of "root" is true

      Examples:
        | markup             | text                                                                                                               |
        | a link's own style | "<a style='color:" + root.shade() + "' href='x'>" + (root.lit ? "b" : "c") + "</a>"                                 |
        | a span's style     | "<span style='color:" + root.shade() + "'>" + (root.lit ? "b" : "c") + "</span>"                                    |
        | a style sheet      | "<html><head><style>a { color: " + root.shade() + " }</style></head><body><a href='x'>" + (root.lit ? "b" : "c") + "</a></body></html>" |

    Scenario: A rich text with no color change takes its new text too
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool lit: true
          property string chars
          function look() { chars = edit.getText(0, edit.length) }
          TextEdit {
            id: edit
            textFormat: TextEdit.RichText
            color: "black"
            text: "<span style='color:#1d4ed8'>" + (root.lit ? "b" : "c") + "</span>"
          }
        }
        """
      When the property "lit" of "root" is set to false
      And the method "look" of "root" is called
      Then the property "chars" of "root" is "c"

  Rule: The change comes from deep inside other handlers
    A handler that changes a property runs what that change reaches before
    it goes on, so a change made ten handlers deep comes at the end of a
    long chain. The new text comes in all the same.

    Scenario Outline: The new text comes in <markup> as the <what> changes, ten handlers deep
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool lit: true
          property string chars
          property bool blue
          property int n0; property int n1; property int n2; property int n3; property int n4
          property int n5; property int n6; property int n7; property int n8; property int n9
          onN0Changed: n1++; onN1Changed: n2++; onN2Changed: n3++; onN3Changed: n4++; onN4Changed: n5++
          onN5Changed: n6++; onN6Changed: n7++; onN7Changed: n8++; onN8Changed: n9++; onN9Changed: lit = false
          function look() {
            chars = edit.getText(0, edit.length)
            blue = edit.text.indexOf("#60a5fa") >= 0
          }
          function shade() { return root.lit ? "#1d4ed8" : "#60a5fa" }
          TextEdit {
            id: edit
            textFormat: TextEdit.RichText
            <binding>
            text: <text>
          }
        }
        """
      When the property "n0" of "root" is set to 1
      And the method "look" of "root" is called
      Then the property "chars" of "root" is "c"
      And the property "blue" of "root" is true

      Examples:
        | markup             | what       | binding                                  | text                                                                                                               |
        | a link's own style | color      | color: root.lit ? "black" : "white"      | "<a style='color:" + root.shade() + "' href='x'>" + (root.lit ? "b" : "c") + "</a>"                                 |
        | a span's style     | color      | color: root.lit ? "black" : "white"      | "<span style='color:" + root.shade() + "'>" + (root.lit ? "b" : "c") + "</span>"                                    |
        | a style sheet      | color      | color: root.lit ? "black" : "white"      | "<html><head><style>a { color: " + root.shade() + " }</style></head><body><a href='x'>" + (root.lit ? "b" : "c") + "</a></body></html>" |
        | a link's own style | font size  | font.pixelSize: root.lit ? 14 : 15       | "<a style='color:" + root.shade() + "' href='x'>" + (root.lit ? "b" : "c") + "</a>"                                 |

    Scenario: A plain text comes in as its color changes, ten handlers deep
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool lit: true
          property int n0; property int n1; property int n2; property int n3; property int n4
          property int n5; property int n6; property int n7; property int n8; property int n9
          onN0Changed: n1++; onN1Changed: n2++; onN2Changed: n3++; onN3Changed: n4++; onN4Changed: n5++
          onN5Changed: n6++; onN6Changed: n7++; onN7Changed: n8++; onN8Changed: n9++; onN9Changed: lit = false
          TextEdit {
            id: edit
            objectName: "edit"
            textFormat: TextEdit.PlainText
            color: root.lit ? "black" : "white"
            text: root.lit ? "b" : "c"
          }
        }
        """
      When the property "n0" of "root" is set to 1
      Then the property "text" of "edit" is "c"
      And the property "length" of "edit" is 1
