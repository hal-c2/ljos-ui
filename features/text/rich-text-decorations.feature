# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Decorations and backgrounds in rich text
  RichText draws the lines named by its elements and styles, and paints an
  inline element's background behind its glyphs.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 400; height: 400; color: "white"
        property string gap: "&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;"
        component R: Text { textFormat: Text.RichText; font.pixelSize: 60 }
        R { text: "<u style=\"color: #00ff00\">" + gap + "</u>" }
        R { y: 100; text: "<s>" + gap + "</s>" }
        R { y: 200; text: "<span style=\"text-decoration: underline; text-decoration-color: #00ff00\">" + gap + "</span>" }
        R { y: 300; text: "<span style=\"text-decoration: line-through; text-decoration-color: #ff0000\">" + gap + "</span>" }
        R { x: 200; text: "<span style=\"text-decoration: overline; text-decoration-color: #ff0000\">" + gap + "</span>" }
        R { x: 200; y: 100; text: "<span style=\"background-color: #ff0000\">" + gap + "</span>" }
        R { x: 200; y: 200; text: "<s style=\"background-color: lightgrey\">" + gap + "</s>" }
      }
      """
    When 100 milliseconds pass

  Scenario: An underlined element paints its line in the element's text colour
    Then the pixel at 20, 59 of the window is within 5 of "#00ff00"

  @qt-parity
  Scenario: A struck-through element paints a line through its text
    Then the pixel at 20, 140 of the window is within 5 of "#000000"

  Scenario: A span paints its underline in the decoration colour
    Then the pixel at 20, 259 of the window is within 5 of "#00ff00"

  @qt-parity
  Scenario: A span paints its strike-through in the decoration colour
    Then the pixel at 20, 340 of the window is within 5 of "#ff0000"

  @qt-parity
  Scenario: A span paints its overline in the decoration colour
    Then the pixel at 220, 1 of the window is within 5 of "#ff0000"

  @qt-parity
  Scenario: An inline element paints its background behind its text
    Then the pixel at 220, 120 of the window is within 5 of "#ff0000"
    And the pixel at 220, 220 of the window is within 5 of "#d3d3d3"

  Rule: Decorated text with glyphs

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 400; height: 400; color: "white"
          property string gap: "&nbsp;&nbsp;&nbsp;&nbsp;"
          component R: Text { textFormat: Text.RichText; font.pixelSize: 60 }
          R { text: "<p><u style=\"color: #00ff00\">x" + gap + "</u></p>" }
          R { y: 100; text: "<p><s>x" + gap + "</s></p>" }
          R { y: 200; text: "<p><span style=\"text-decoration: underline; text-decoration-color: #00ff00\">x" + gap + "</span></p>" }
          R { y: 300; text: "<p><span style=\"text-decoration: line-through; text-decoration-color: #ff0000\">x" + gap + "</span></p>" }
          R { x: 200; text: "<p><span style=\"text-decoration: overline; text-decoration-color: #ff0000\">x" + gap + "</span></p>" }
          R { x: 200; y: 100; text: "<p><span style=\"background-color: #ff0000\">x" + gap + "</span></p>" }
          R { x: 200; y: 200; text: "<p><s style=\"background-color: lightgrey\">x" + gap + "</s></p>" }
        }
        """
      When 100 milliseconds pass

    Scenario: Glyphs do not take away an element's underline
      Then the pixel at 80, 59 of the window is within 5 of "#00ff00"

    @qt-parity
    Scenario: Glyphs do not take away an element's strike-through
      Then the pixel at 80, 140 of the window is within 5 of "#000000"

    Scenario: Glyphs do not take away a span's underline or decoration colour
      Then the pixel at 80, 259 of the window is within 5 of "#00ff00"

    @qt-parity
    Scenario: Glyphs do not take away a span's strike-through or decoration colour
      Then the pixel at 80, 340 of the window is within 5 of "#ff0000"

    @qt-parity
    Scenario: Glyphs do not take away a span's overline or decoration colour
      Then the pixel at 280, 1 of the window is within 5 of "#ff0000"

    @qt-parity
    Scenario: Glyphs paint over an inline element's background
      Then the pixel at 205, 102 of the window is within 5 of "#ff0000"
      And the pixel at 205, 202 of the window is within 5 of "#d3d3d3"

  Rule: Several small paragraphs

    Background:
      Given the QML:
        """qml
        import QtQuick
        Text {
          textFormat: Text.RichText
          wrapMode: Text.WordWrap
          width: 440
          font.pointSize: 12
          text: `<p><u style="color: green;">green with underline</u>
                    <span style="text-decoration: underline; text-decoration-color: green;">
                      green underline</span></p>
                 <p><s style="background-color: lightgrey;">plain strikethrough</s>
                    <span style="text-decoration: line-through; text-decoration-color: orange;">
                      orange strikethrough</span></p>
                 <p><span style="text-decoration: overline;">plain overline</span>
                    <span style="text-decoration: overline; text-decoration-color: red;">
                      red overline</span></p>`
        }
        """
      When 100 milliseconds pass

    Scenario: Small paragraphs keep their underlines and decoration colours
      Then the pixel at 5, 15 of the window is within 5 of "#008000"
      And the pixel at 160, 15 of the window is within 5 of "#008000"

    @qt-parity
    Scenario: Small paragraphs keep their inline backgrounds
      Then the pixel at 5, 34 of the window is within 5 of "#d3d3d3"

    Scenario: Small paragraphs keep their strike-throughs and decoration colours
      Then the pixel at 5, 40 of the window is within 5 of "#000000"
      And the pixel at 160, 40 of the window is within 5 of "#ffa500"

    Scenario: Small paragraphs keep their overlines and decoration colours
      Then the pixel at 5, 60 of the window is within 5 of "#000000"
      And the pixel at 110, 60 of the window is within 5 of "#ff0000"
