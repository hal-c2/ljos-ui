# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getText-method
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#role-prop
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot see the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: What a rich editor tells an assistive technology
  A rich TextEdit's `text` is HTML. A screen reader is told the characters
  the user sees, never the tags, and its places are the editor's own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextEdit {
          objectName: "notes"; focus: true
          width: 200; height: 80
          textFormat: TextEdit.RichText
          text: "a<b>bold</b>c"
          property string probe
          Accessible.name: "Notes"
        }
      }
      """

  Rule: The value is the characters

    Scenario: The value of a rich TextEdit has no tags
      Then the "multilinetextinput" named "Notes" has the value "aboldc"

    Scenario: The value follows what is typed
      Given an assistive technology is listening
      When the user presses "End"
      And the user types "!"
      Then the "multilinetextinput" named "Notes" has the value "aboldc!"

    Scenario: The value follows new text
      When the property "text" of "notes" is set to "<i>x</i>y"
      Then the "multilinetextinput" named "Notes" has the value "xy"

    Scenario: What is selected is said in characters
      Given an assistive technology is listening
      When the method "select" of "notes" is called with 1, 5
      Then the "multilinetextinput" named "Notes" has "bold" selected

  Rule: A screen reader's places are characters

    Scenario: A selection is taken in characters
      When an assistive technology selects characters 1 to 5 of the "multilinetextinput" named "Notes"
      Then the property "selectedText" of "notes" is "bold"

    Scenario: What a screen reader types takes the format where it lands
      When an assistive technology selects characters 2 to 4 of the "multilinetextinput" named "Notes"
      And an assistive technology replaces the selection of the "multilinetextinput" named "Notes" with "OL"
      Then the "multilinetextinput" named "Notes" has the value "abOLdc"
      When the property "probe" of "notes" is bound to "getFormattedText(2, 4).indexOf('font-weight:') >= 0 ? 'bold' : 'plain'"
      Then the property "probe" of "notes" is "bold"
