# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#css-properties
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkColor-prop
# Verified on Qt 6.11.2 (qml6, offscreen). The documentation's table has
# color, background-color, background and text-decoration. It does not have
# text-decoration-color, which Qt 6.11.2 reads all the same. Where a
# background and a line are painted is Qt 6.11.2's, found by looking at its
# pictures, and so is what a declaration that cannot be read costs. Only
# what no font changes is asked here: a background fills its line from the
# top, and an overline runs along the top of the line. The text is spaces
# that do not break, so no letter is in the way of a pixel.
# The documentation of linkColor says it is for StyledText and not for
# RichText; Qt 6.11.2 paints a link of RichText in the application
# palette's link colour, which is blue unless the host sets another
# (rich-text-link-colour.feature).

@qt-parity
Feature: Styles written in rich text
  An element of RichText may carry a style attribute. The colour of its
  text, the colour behind it and the lines drawn over, under and through
  it are read from there. What else a style says is left out.

  Rule: Colours and lines

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: root; objectName: "root"; width: 400; height: 400; color: "white"
          property string half: "&nbsp;&nbsp;&nbsp;"
          property string gap: half + half
          property bool sameSize: filled.implicitWidth === bare.implicitWidth
              && filled.implicitHeight === bare.implicitHeight
              && over.implicitHeight === bare.implicitHeight
          function span(style, inside) {
            return "<span style=\"" + style + "\">" + (inside || gap) + "</span>"
          }
          component R: Text { textFormat: Text.RichText; font.pixelSize: 60 }
          R { id: bare; visible: false; text: root.gap }
          R { id: filled; text: root.span("background-color: #ff0000") }
          R { id: over; y: 100; text: root.span("text-decoration: overline; color: #00ff00") }
          R { y: 200; text: root.span("text-decoration: overline; text-decoration-color: #0000ff; color: #00ff00") }
          R { y: 300; text: root.span("text-decoration: overline", root.half + root.span("text-decoration: none", root.half)) }
          R { x: 200; text: root.span("background-color: #ff0000", root.half + root.span("background-color: transparent", root.half)) }
          R { x: 200; y: 100; text: root.span("background-color: #ff0000;; bogus: 1; background-color: #00ff00", root.half)
              + root.span("background-color: #0000ff; color #ff0000; background-color: #ff0000", root.half) }
          R { x: 200; y: 200; text: "<span STYLE='BACKGROUND-COLOR: Blue'>" + root.half + "</span>"
              + root.span("background-color: rgb(0, 255, 0)", root.half) }
          R { x: 200; y: 300; text: root.span("color #ff0000; background-color: #ff0000") }
        }
        """
      When 100 milliseconds pass

    Scenario: A background colour fills the line behind its text
      Then the pixel at 5, 2 of the window is "#ff0000"
      And the pixel at 60, 40 of the window is "#ff0000"

    Scenario: The background ends where its text ends
      Then the pixel at 180, 20 of the window is "#ffffff"
      And the pixel at 5, 95 of the window is "#ffffff"

    Scenario: A style changes the size of the text in nothing
      Then the property "sameSize" of "root" is true

    Scenario: An overline runs along the top of the line in the colour of the text
      Then the pixel at 5, 101 of the window is "#00ff00"
      And the pixel at 5, 140 of the window is "#ffffff"

    Scenario: A decoration colour is the colour of the line
      Then the pixel at 5, 201 of the window is "#0000ff"

    Scenario: A decoration of none takes the lines of the text inside away
      Then the pixel at 5, 301 of the window is "#000000"
      And the pixel at 70, 301 of the window is "#ffffff"

    Scenario: A transparent background takes the background of the text inside away
      Then the pixel at 205, 2 of the window is "#ff0000"
      And the pixel at 270, 2 of the window is "#ffffff"

    Scenario: The last of two declarations counts and a property nobody knows is passed over
      Then the pixel at 205, 102 of the window is "#00ff00"

    Scenario: A declaration that is no name and value is passed over
      Then the pixel at 270, 102 of the window is "#ff0000"

    Scenario: A style whose first declaration is no name and value says nothing
      Then the pixel at 205, 302 of the window is "#ffffff"

    Scenario: Names are read in capitals or small letters, in either kind of quotes
      Then the pixel at 205, 202 of the window is "#0000ff"

    Scenario: A colour may be written as rgb()
      Then the pixel at 270, 202 of the window is "#00ff00"

  Rule: Links

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"; width: 400; height: 400; color: "white"
          property string gap: "&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;"
          component R: Text { textFormat: Text.RichText; font.pixelSize: 60; linkColor: "#ff00ff" }
          R { text: "<a href=\"x\" style=\"text-decoration: overline\">" + gap + "</a>" }
          R { y: 100; text: "<a href=\"x\" style=\"text-decoration: overline; color: #00ff00\">" + gap + "</a>" }
          R { y: 200; text: "<span style=\"color: #00ff00\"><a href=\"x\" style=\"text-decoration: overline\">" + gap + "</a></span>" }
        }
        """
      When 100 milliseconds pass

    Scenario: A link of RichText is blue whatever linkColor says
      Then the pixel at 5, 1 of the window is "#0000ff"

    Scenario: The style of a link gives it another colour
      Then the pixel at 5, 101 of the window is "#00ff00"

    Scenario: The colour of the text around a link is not the link's
      Then the pixel at 5, 201 of the window is "#0000ff"
