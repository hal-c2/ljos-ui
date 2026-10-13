# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A rich TextEdit keeps the unit of a font size
  A size given in points reads back in points and one given in pixels in
  pixels. The scenario compares the body of the HTML as the rich text
  scenarios do: double quotes read as single quotes.

  Scenario: Sizes in points and pixels read back as they were given
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function spans() {
          out = edit.text.replace(/^[\s\S]*<body[^>]*>\n?/, "").replace(/<\/body>[\s\S]*$/, "")
            .replace(/<p [^>]*>/, "<p>").split("\"").join("'")
        }
        TextEdit {
          id: edit; textFormat: TextEdit.RichText
          text: "<span style='font-size:20pt'>a</span><span style='font-size:10.5pt'>b</span><span style='font-size:15px'>c</span>"
        }
      }
      """
    When the method "spans" of "root" is called
    Then the property "out" of "root" is "<p><span style=' font-size:20pt;'>a</span><span style=' font-size:10.5pt;'>b</span><span style=' font-size:15px;'>c</span></p>"
