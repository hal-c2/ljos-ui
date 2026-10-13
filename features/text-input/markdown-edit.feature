# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#getFormattedText-method
#   https://doc.qt.io/qt-6/qml-qtquick-textselection.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Editing Markdown in a TextEdit
  A TextEdit whose textFormat is MarkdownText shows the Markdown formatted,
  keeps the formats while it is edited, and its text reads back as
  Markdown, each paragraph ended by an empty line. Line breaks read as bars,
  so that the text fits in a step.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function markdown() { out = edit.text.split("\n").join("|") }
        function plain() { out = edit.getText(0, edit.length).split("\u2029").join("|") }
        function fragment() { out = edit.getFormattedText(0, 6).split("\n").join("|") }
        function bolden() {
          let f = edit.cursorSelection.font
          f.bold = true
          edit.cursorSelection.font = f
        }
        TextEdit {
          id: edit; objectName: "edit"; width: 280; focus: true
          textFormat: TextEdit.MarkdownText
          text: "a **bold** c\n\nsecond *it*"
        }
      }
      """

  Scenario: The text reads back as Markdown
    When the method "markdown" of "root" is called
    Then the property "out" of "root" is "a **bold** c||second *it*||"

  Scenario: The characters are those of the formatted text
    When the method "plain" of "root" is called
    Then the property "out" of "root" is "a bold c|second it"

  Scenario: Typing inside a bold word types bold text
    When the property "cursorPosition" of "edit" is set to 4
    And the user types "x"
    And the method "markdown" of "root" is called
    Then the property "out" of "root" is "a **boxld** c||second *it*||"

  Scenario: getFormattedText gives Markdown
    When the method "fragment" of "root" is called
    Then the property "out" of "root" is "a **bold**||"

  Scenario: A toolbar bolds the selection
    When the method "select" of "edit" is called with 0, 1
    And the method "bolden" of "root" is called
    And the method "markdown" of "root" is called
    Then the property "out" of "root" is "**a** **bold** c||second *it*||"

  Scenario: Return in a bold word makes a new paragraph, bold on both sides
    When the property "cursorPosition" of "edit" is set to 4
    And the user presses "Return"
    And the method "markdown" of "root" is called
    Then the property "out" of "root" is "a **bo**||**ld** c||second *it*||"

  Scenario: Undoing the typing gives the Markdown back as it was
    When the property "cursorPosition" of "edit" is set to 4
    And the user types "x"
    And the method "undo" of "edit" is called
    And the method "markdown" of "root" is called
    Then the property "out" of "root" is "a **bold** c||second *it*||"
