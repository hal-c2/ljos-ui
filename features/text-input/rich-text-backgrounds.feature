# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#css-properties
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen), by looking at its pictures. The
# text is spaces that do not break, so no letter is in the way of a pixel.

@qt-parity
Feature: Background colours of a rich TextEdit
  A part of the text with a background colour has it filled behind it in
  its line, from the line's top to its bottom, as in a rich Text, and text
  typed into that part has it too. A paragraph with a background colour
  has it filled behind its lines across the whole width of the document,
  in a TextEdit as in a Text; the room between paragraphs stays clear.
  The text a TextEdit gives back keeps the paragraph's colour.

  Rule: The background of a part of the text

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 400; height: 400; color: "white"
          property string half: "&nbsp;&nbsp;&nbsp;"
          component E: TextEdit { textFormat: TextEdit.RichText; font.pixelSize: 60 }
          E { text: "<span style='background-color: #ff0000'>" + half + half + "</span>" }
          E { x: 200; text: "<span style='background-color: #ff0000'>" + half
              + "<span style='background-color: transparent'>" + half + "</span></span>" }
          E { objectName: "typed"; y: 100; text: "<span style='background-color: #00ff00'>" + half + "</span>" }
        }
        """
      When 100 milliseconds pass

    Scenario: A background colour fills the line behind its text
      Then the pixel at 5, 2 of the window is "#ff0000"
      And the pixel at 60, 40 of the window is "#ff0000"

    Scenario: The background ends where its text ends
      Then the pixel at 180, 20 of the window is "#ffffff"
      And the pixel at 5, 95 of the window is "#ffffff"

    Scenario: A transparent background takes the background of the text inside away
      Then the pixel at 205, 2 of the window is "#ff0000"
      And the pixel at 270, 2 of the window is "#ffffff"

    Scenario: Text typed into the part has its background
      Then the pixel at 80, 102 of the window is "#ffffff"
      When the method "forceActiveFocus" of "typed" is called
      And the property "cursorPosition" of "typed" is set to 1
      And the user types "   "
      And 100 milliseconds pass
      Then the pixel at 80, 102 of the window is "#00ff00"

  Rule: The background of a paragraph

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 400; height: 400; color: "white"
          property string doc: "<p>&nbsp;</p><p style='background-color: #0000ff'>&nbsp;&nbsp;</p><p>&nbsp;</p>"
          TextEdit { width: 180; textFormat: TextEdit.RichText; font.pixelSize: 30; text: doc }
          Text { x: 200; width: 180; textFormat: Text.RichText; font.pixelSize: 30; text: doc }
          TextEdit { id: narrow; y: 200; textFormat: TextEdit.RichText; font.pixelSize: 30; text: "<p style='background-color: #0000ff'>&nbsp;&nbsp;</p>" }
          TextEdit { y: 300; textFormat: TextEdit.RichText; font.pixelSize: 30; text: narrow.text }
        }
        """
      When 100 milliseconds pass

    Scenario Outline: A paragraph's background fills its lines across the document
      Then the pixel at <x>, 50 of the window is "#0000ff"
      And the pixel at <x>, 78 of the window is "#0000ff"

      Examples:
        | x   |
        | 1   |
        | 100 |
        | 179 |
        | 201 |
        | 379 |

    Scenario Outline: The room around the paragraph stays clear
      Then the pixel at <x>, 44 of the window is "#ffffff"
      And the pixel at <x>, 82 of the window is "#ffffff"
      And the pixel at <x>, 20 of the window is "#ffffff"

      Examples:
        | x   |
        | 10  |
        | 210 |

    Scenario: The background ends at the document's edge
      Then the pixel at 181, 50 of the window is "#ffffff"
      And the pixel at 381, 50 of the window is "#ffffff"

    Scenario: A document as wide as its text is filled as wide as its text
      Then the pixel at 1, 205 of the window is "#0000ff"
      And the pixel at 50, 205 of the window is "#ffffff"

    Scenario: The text a TextEdit gives back keeps the paragraph's background
      Then the pixel at 1, 305 of the window is "#0000ff"
      And the pixel at 50, 305 of the window is "#ffffff"
