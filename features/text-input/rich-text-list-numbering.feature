# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getFormattedText-method
#   https://doc.qt.io/qt-6/richtext-html-subset.html
# Verified on Qt 6.11.2 (qml6, offscreen and vnc).

@qt-parity
Feature: The items of a numbered list are numbered again when one leaves it
  Backspace at the start of an item takes it out of its list, and the
  items after it are numbered one less: they are still of the list the
  item left, however far apart. The paragraph that was the item stays in
  the list's HTML while items of the list follow it. The list has the
  items "one" to "four", at 20 pixels in Liberation Sans, and a paragraph
  "end" follows it. Each tag of the HTML is shown without its style.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 300; height: 200; color: "white"
        property string out
        function at(i) { edit.forceActiveFocus(); edit.cursorPosition = i }
        function html() {
          const lt = String.fromCharCode(60)
          const t = edit.getFormattedText(0, edit.length)
          const a = t.indexOf(lt + "body")
          out = t.slice(t.indexOf(">", a) + 1, t.indexOf(lt + "/body"))
            .replace(/ style="[^"]*"/g, "").replace(/\n/g, "").split(lt + "!--StartFragment-->").join("")
            .split(lt + "!--EndFragment-->").join("").split(lt).join("[").split(">").join("]")
        }
        TextEdit {
          id: edit; objectName: "edit"; width: 300
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: "[ol][li]one[/li][li]two[/li][li]three[/li][li]four[/li][/ol][p]end[/p]"
            .replace(/\[/g, String.fromCharCode(60)).replace(/\]/g, ">")
        }
      }
      """

  Rule: An item out of the list leaves the items after it a number less

    Scenario: The third item is numbered 3
      When the method "at" of "root" is called with 0
      Then the pixel at 27, 60 of the window is within 60 of "#000000"

    Scenario: With the second item out of the list the third is numbered 2
      When the method "at" of "root" is called with 4
      And the user presses "Backspace"
      And the method "at" of "root" is called with 0
      Then the pixel at 27, 60 of the window is within 8 of "#ffffff"
      And the pixel at 27, 83 of the window is within 60 of "#000000"

  Rule: A paragraph out of a list stays in its HTML while the list goes on

    Scenario Outline: Backspace at <at> writes <html>
      When the method "at" of "root" is called with <at>
      And the user presses "Backspace"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "<html>"

      Examples:
        | at | html |
        | 4  | [ol][li]one[/li][p]two[/p][li]three[/li][li]four[/li][/ol][p]end[/p] |
        | 0  | [p]one[/p][ol][li]two[/li][li]three[/li][li]four[/li][/ol][p]end[/p] |
        | 14 | [ol][li]one[/li][li]two[/li][li]three[/li][/ol][p]four[/p][p]end[/p] |

    Scenario: Backspace twice at the second item keeps it in the HTML of the list
      When the method "at" of "root" is called with 4
      And the user presses "Backspace"
      And the user presses "Backspace"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "[ol][li]one[/li][p]two[/p][li]three[/li][li]four[/li][/ol][p]end[/p]"

    Scenario: The first item out after the second leaves both before the list
      When the method "at" of "root" is called with 4
      And the user presses "Backspace"
      And the user presses "Backspace"
      And the method "at" of "root" is called with 0
      And the user presses "Backspace"
      And the method "html" of "root" is called
      Then the property "out" of "root" is "[p]one[/p][p]two[/p][ol][li]three[/li][li]four[/li][/ol][p]end[/p]"
