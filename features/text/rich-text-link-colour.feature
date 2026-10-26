# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkColor-prop
#   https://doc.qt.io/qt-6/qpalette.html#ColorRole-enum (Link)
# Measured on Qt 6.11.2 (qml6, offscreen): a link of RichText or Markdown
# that its document gives no colour takes the application palette's Link
# colour, blue unless the application sets another. `palette.link` on the
# item or on what it is in leaves it as it is, and so does `linkColor`.
# QML cannot set the application palette, only its host can, so what a
# host sets is Ljós's alone to check. The default is checked on qml6 in
# rich-text-style.feature.

@ljos-only @api-QtQuick.Text.textFormat @api-QtQuick.TextEdit.textFormat
Feature: A link takes the application palette's link colour
  A link of RichText or Markdown whose document gives it no colour is drawn
  in the link colour of the application's palette, which its host sets. A
  colour the document gives the link wins over it.

  Background:
    Given the host sets the application palette's link colour to "#1d4ed8"
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 400; height: 400; color: "white"
        property string link: "<a href=\"x\" style=\"text-decoration: overline\">&nbsp;&nbsp;&nbsp;&nbsp;</a>"
        component R: Text { textFormat: Text.RichText; font.pixelSize: 60; linkColor: "#ff00ff" }
        R { text: root.link }
        R { y: 100; text: "<a href=\"x\" style=\"text-decoration: overline; color: #00ff00\">&nbsp;&nbsp;&nbsp;&nbsp;</a>" }
        TextEdit { y: 200; textFormat: TextEdit.RichText; font.pixelSize: 120; font.bold: true; text: "<a href=\"x\">IIII</a>" }
        Text { x: 200; y: 200; textFormat: Text.MarkdownText; font.pixelSize: 120; font.bold: true; linkColor: "#ff00ff"; text: "[IIII](x)" }
      }
      """
    When 100 milliseconds pass

  Scenario: A link of RichText takes the palette's colour, not linkColor
    Then the pixel at 5, 1 of the window is "#1d4ed8"

  Scenario: The style of a link wins over the palette
    Then the pixel at 5, 101 of the window is "#00ff00"

  Scenario: A link of a rich TextEdit takes the palette's colour
    Then the pixel at 15, 260 of the window is "#1d4ed8"

  Scenario: A link of Markdown takes the palette's colour, not linkColor
    Then the pixel at 215, 260 of the window is "#1d4ed8"

  Scenario: A link of Text follows the palette when the host changes it
    When the host changes the application palette's link colour to "#b91c1c"
    And 50 milliseconds pass
    Then the pixel at 5, 1 of the window is "#b91c1c"
