# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textDocument-prop
#   https://doc.qt.io/qt-6/qsyntaxhighlighter.html#details
#   docs/ARCHITECTURE.md (section 7.5)
# In Qt a program colours the text of a TextEdit from C++, with a
# QSyntaxHighlighter on the document behind textDocument. In Ljós the
# document takes the coloured stretches themselves and reads them back
# (highlight, clearHighlights and highlights are not Qt's), so these
# scenarios run on Ljós only. What Qt's document has is in
# text-document.feature.

@ljos-only
Feature: Highlighting the text of a TextEdit
  A program says which stretches of a TextEdit's text are shown in which
  colours, through the document of the TextEdit. That changes how the text
  is painted and nothing else: not the text, not where its lines break,
  not what can be undone. The document reads back the stretches it paints,
  as one list in the order of the text, none overlapping.

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
        function paint(from, to, color) { edit.textDocument.highlight(from, to, color) }
        property string spans: "unread"
        function read() {
          spans = edit.textDocument.highlights().map(function (s) {
            return s.start + "-" + s.end + " " + s.color + " on " + s.background
          }).join(", ")
        }
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

  Scenario: The document reads back the stretches it paints
    When the method "mark" of "root" is called with 0, 4
    And the method "paint" of "root" is called with 2, 6, "#ff0000"
    And the method "read" of "root" is called
    Then the property "spans" of "root" is "0-2 #000000 on #00ff00, 2-4 #ff0000 on #00ff00, 4-6 #ff0000 on undefined"

  Scenario: Nothing is read back once the highlighting is taken away
    When the method "mark" of "root" is called with 0, 2
    And the method "clear" of "root" is called
    And the method "read" of "root" is called
    Then the property "spans" of "root" is ""

  Scenario: What is read back is counted in the positions of the text
    When the property "text" of "edit" is set to "ééab"
    And the method "mark" of "root" is called with 2, 4
    And the method "read" of "root" is called
    Then the property "spans" of "root" is "2-4 #000000 on #00ff00"

  Scenario: A stretch cut to the text is read back as it is painted
    When the method "mark" of "root" is called with 7, 400
    And the method "read" of "root" is called
    Then the property "spans" of "root" is "7-9 #000000 on #00ff00"
