# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textDocument-prop
#   https://doc.qt.io/qt-6/qsyntaxhighlighter.html#details
#   docs/ARCHITECTURE.md (section 7.5)
# In Qt a program colours the text of a TextEdit from C++, with a
# QSyntaxHighlighter on the document behind textDocument. In Ljós the
# document takes the coloured stretches themselves (highlight and
# clearHighlights are not Qt's), so these scenarios run on Ljós only. What
# Qt's document has is in text-document.feature.

@ljos-only
Feature: Highlighting the text of a TextEdit
  A program says which stretches of a TextEdit's text are shown in which
  colours, through the document of the TextEdit. That changes how the text
  is painted and nothing else: not the text, not where its lines break,
  not what can be undone.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 200; height: 120; color: "white"
        property real widthBefore: -1
        property bool sameWidth: edit.contentWidth === widthBefore
        function remember() { widthBefore = edit.contentWidth }
        function mark(from, to) { edit.textDocument.highlight(from, to, "black", "#00ff00") }
        function clear() { edit.textDocument.clearHighlights() }
        TextEdit {
          id: edit; objectName: "edit"; anchors.fill: parent; focus: true
          font.pixelSize: 40; selectionColor: "#0000ff"; text: "abcd\nefgh"
        }
      }
      """
    When 100 milliseconds pass

  Scenario: A highlighted stretch is painted on its background
    When the method "mark" of "root" is called with 0, 2
    Then the pixel at 1, 1 of the window is "#00ff00"
    And the pixel at 1, 58 of the window is "#ffffff"
    And the pixel at 190, 1 of the window is "#ffffff"

  Scenario: A stretch is counted in the positions of the text
    When the method "mark" of "root" is called with 5, 7
    Then the pixel at 1, 58 of the window is "#00ff00"
    And the pixel at 1, 1 of the window is "#ffffff"

  Scenario: A stretch reaches over a line break
    When the method "mark" of "root" is called with 0, 9
    Then the pixel at 1, 1 of the window is "#00ff00"
    And the pixel at 1, 58 of the window is "#00ff00"

  Scenario: Highlighting changes neither the text nor its layout
    When the method "remember" of "root" is called
    And the method "mark" of "root" is called with 0, 2
    Then the property "text" of "edit" is "abcd\nefgh"
    And the property "lineCount" of "edit" is 2
    And the property "canUndo" of "edit" is false
    And the property "sameWidth" of "root" is true

  Scenario: Taking the highlighting away shows the text as it was
    When the method "mark" of "root" is called with 0, 2
    And the method "clear" of "root" is called
    Then the pixel at 1, 1 of the window is "#ffffff"

  Scenario: The selection is painted over a highlighted stretch
    When the method "mark" of "root" is called with 0, 2
    And the method "select" of "edit" is called with 0, 2
    Then the pixel at 1, 1 of the window is "#0000ff"

  Scenario: A stretch past the end of the text is cut to the text
    When the method "mark" of "root" is called with 7, 400
    Then the pixel at 1, 58 of the window is "#ffffff"
    And the property "text" of "edit" is "abcd\nefgh"
