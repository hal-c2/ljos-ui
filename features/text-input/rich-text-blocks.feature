# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionToRectangle-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A rich TextEdit lays out its paragraphs as a rich Text does
  A paragraph's margins stand between it and the next, a list item stands
  in with its bullet in the indent, and the editor is as tall as a rich
  Text of the same document. The cursor, a press and a selection find the
  characters where they are drawn, and an edit keeps the paragraphs as
  they were. The numbers are those of the bundled font, Liberation Sans,
  at 14 pixels: a line is 16 pixels and a paragraph's margin 12.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property real twoY
        property real threeX
        property real threeY
        property real fiveY
        function measure() {
          const plain = edit.getText(0, edit.length)
          twoY = edit.positionToRectangle(plain.indexOf("two")).y
          threeX = edit.positionToRectangle(plain.indexOf("three")).x
          threeY = edit.positionToRectangle(plain.indexOf("three")).y
          fiveY = edit.positionToRectangle(plain.indexOf("five")).y
        }
        TextEdit {
          id: edit
          objectName: "edit"
          width: 300
          focus: true
          selectByMouse: true
          textFormat: TextEdit.RichText
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: "<p>one</p><p>two</p><ul><li>three</li><li>four</li></ul><p>five</p>"
        }
        Text {
          objectName: "label"
          y: 200
          width: 300
          textFormat: Text.RichText
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: edit.text
        }
      }
      """

  Scenario: Margins and a list's indent place the paragraphs
    When the method "measure" of "root" is called
    Then the property "twoY" of "root" is 28
    And the property "threeX" of "root" is 40
    And the property "threeY" of "root" is 56
    And the property "fiveY" of "root" is 100
    And the property "implicitHeight" of "edit" is 116
    And the property "implicitHeight" of "label" is 116

  Scenario: The cursor stands where a list item's text is drawn
    When the property "cursorPosition" of "edit" is set to 8
    Then the property "cursorRectangle.x" of "edit" is 40
    And the property "cursorRectangle.y" of "edit" is 56

  Scenario: A press on a list item puts the cursor in its text
    When the user presses the pointer at 41, 60
    And the user releases the pointer at 41, 60
    Then the property "cursorPosition" of "edit" is 8

  Scenario: A press in a list item's indent puts the cursor at its start
    When the user presses the pointer at 10, 60
    And the user releases the pointer at 10, 60
    Then the property "cursorPosition" of "edit" is 8

  Scenario: A press just under a paragraph puts the cursor at its end
    When the user presses the pointer at 10, 44.5
    And the user releases the pointer at 10, 44.5
    Then the property "cursorPosition" of "edit" is 7

  Scenario: A press lower in the margin puts the cursor at the next paragraph's start
    When the user presses the pointer at 100, 20
    And the user releases the pointer at 100, 20
    Then the property "cursorPosition" of "edit" is 4

  Scenario: A press in the margin under the list puts the cursor in the next paragraph
    When the user presses the pointer at 10, 90
    And the user releases the pointer at 10, 90
    Then the property "cursorPosition" of "edit" is 19

  Scenario: A drag selects across the list's items
    When the user presses the pointer at 41, 60
    And the user moves the pointer to 100, 70
    And the user moves the pointer to 200, 76
    And the user releases the pointer at 200, 76
    Then the property "selectionStart" of "edit" is 8
    And the property "selectionEnd" of "edit" is 18

  Scenario: Typing in a list item keeps the paragraphs
    When the property "cursorPosition" of "edit" is set to 13
    And the user types "s"
    And the method "measure" of "root" is called
    Then the property "threeX" of "root" is 40
    And the property "fiveY" of "root" is 100
    And the property "implicitHeight" of "edit" is 116
    And the property "implicitHeight" of "label" is 116
