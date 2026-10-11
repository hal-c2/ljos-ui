# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#selectedText-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionAt-method
#   docs/ARCHITECTURE.md (section 7.5)
# Qt Quick's Text cannot be selected in, and nothing in Qt Quick selects
# across items. SelectionScope is a type of module Ljos, so these scenarios
# run on Ljós only. Its words (selectedText, select, positionAt) are those
# of TextEdit.

@ljos-only
Feature: Selecting across Text items
  The Text items inside a SelectionScope are read as one text, a line apart:
  a drag selects from one item into the next, and what is selected can be
  read and copied.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Rectangle {
        id: root; objectName: "root"; width: 300; height: 200; color: "white"
        property int found: -1
        property string link: "nothing yet"
        function probe(x, y) { found = scope.positionAt(x, y) }
        function probeLink(x, y) { link = scope.linkAt(x, y) }
        SelectionScope {
          id: scope; objectName: "scope"; anchors.fill: parent
          selectionColor: "#ff0000"
          Column {
            Text { font.pixelSize: 20; text: "abc def" }
            Text {
              objectName: "answer"; font.pixelSize: 20
              textFormat: Text.MarkdownText; text: "**ghi**\n\n[jkl](there)"
            }
          }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: Nothing is selected at first
    Then the property "selectedText" of "scope" is ""
    And the property "selectionStart" of "scope" is 0
    And the property "selectionEnd" of "scope" is 0

  Scenario: A selection reaches from one Text into the next
    When the method "select" of "scope" is called with 4, 11
    Then the property "selectedText" of "scope" is "def\nghi"
    And the property "selectionStart" of "scope" is 4
    And the property "selectionEnd" of "scope" is 11

  Scenario: A selection given backwards is the same selection
    When the method "select" of "scope" is called with 11, 4
    Then the property "selectedText" of "scope" is "def\nghi"
    And the property "selectionStart" of "scope" is 4

  Scenario: Everything is selected at once
    When the method "selectAll" of "scope" is called
    Then the property "selectedText" of "scope" is "abc def\nghi\njkl"
    And the property "selectionEnd" of "scope" is 15

  Scenario: The selection is dropped
    When the method "selectAll" of "scope" is called
    And the method "deselect" of "scope" is called
    Then the property "selectedText" of "scope" is ""

  Scenario: A drag selects from where it starts to where it is
    When the user presses the pointer at 0, 5
    And the user moves the pointer to 299, 199
    Then the property "selectedText" of "scope" is "abc def\nghi\njkl"
    When the user moves the pointer to 299, 5
    Then the property "selectedText" of "scope" is "abc def"

  Scenario: A press that does not move selects nothing
    When the method "selectAll" of "scope" is called
    And the user presses the pointer at 0, 5
    And the user releases the pointer at 0, 5
    Then the property "selectedText" of "scope" is ""

  Scenario: The platform's key for copying copies the selection
    When the user drags from 0, 5 to 299, 5
    And the user presses "Ctrl+C"
    Then the clipboard holds "abc def"

  Scenario: The platform's key for selecting everything selects every item
    When the user drags from 0, 5 to 299, 5
    And the user presses "Ctrl+A"
    Then the property "selectedText" of "scope" is "abc def\nghi\njkl"

  Scenario: copy puts the selection on the clipboard
    When the method "select" of "scope" is called with 8, 11
    And the method "copy" of "scope" is called
    Then the clipboard holds "ghi"

  Scenario: positionAt counts through the items
    When the method "probe" of "root" is called with 0, 5
    Then the property "found" of "root" is 0
    When the method "probe" of "root" is called with 299, 5
    Then the property "found" of "root" is 7
    When the method "probe" of "root" is called with 0, 35
    Then the property "found" of "root" is 8

  Scenario: linkAt finds a link in whichever item it is
    When the method "probeLink" of "root" is called with 2, 70
    Then the property "link" of "root" is "there"
    When the method "probeLink" of "root" is called with 2, 5
    Then the property "link" of "root" is ""

  Scenario: What is selected is painted over the selection colour
    Then the pixel at 1, 1 of the window is "#ffffff"
    When the method "select" of "scope" is called with 0, 3
    Then the pixel at 1, 1 of the window is "#ff0000"
    When the method "deselect" of "scope" is called
    Then the pixel at 1, 1 of the window is "#ffffff"

  Scenario: A scope that does not select by mouse leaves a drag alone
    When the property "selectByMouse" of "scope" is set to false
    And the user drags from 0, 5 to 299, 5
    Then the property "selectedText" of "scope" is ""
