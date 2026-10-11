# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#baseUrl-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
#   https://doc.qt.io/qt-6/richtext-html-subset.html
# Verified on Qt 6.11.2 (qml6, offscreen). The documentation names the img
# element and its attributes; how much room an image takes in each format
# is Qt 6.11.2's, found by comparing sizes.

Feature: Images in the text of a Text
  A document may name pictures, and a Text shows them in its lines. An
  image is as large as its picture unless the document gives it a size,
  and a name that is no whole address is looked for from baseUrl.

  Background:
    Given a 40 by 20 image file "red.png" filled with "#ff0000"
    And a 30 by 60 image file "tall.png" filled with "#0000ff"

  @qt-parity
  Rule: RichText and MarkdownText

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"; width: 300; height: 300; color: "white"
          property bool besideText: mixed.implicitWidth === plain.implicitWidth + 40
          property bool onTheBaseline: mixed.implicitHeight > 20
              && mixed.implicitHeight < 20 + plain.implicitHeight
          property bool fileIsTheBase: ("" + plain.baseUrl).endsWith("/Scenario.qml")
          function rebase() { elsewhere.baseUrl = Qt.resolvedUrl("Other.qml") }
          component R: Text { textFormat: Text.RichText }
          Text { id: plain; objectName: "plain"; y: 200; text: "abc" }
          R { objectName: "one"; text: "<img src=\"red.png\">" }
          R { objectName: "sized"; y: 100; text: "<img src=\"red.png\" width=\"10\" height=\"50\">" }
          R { objectName: "wide"; y: 100; text: "<img src=\"red.png\" width=\"80\">" }
          R { objectName: "high"; y: 100; text: "<img src=\"red.png\" height=\"80\">" }
          R { objectName: "missing"; y: 100; text: "<img src=\"nothing.png\">" }
          R { id: mixed; y: 100; text: "abc<img src=\"red.png\">" }
          R { objectName: "two"; y: 100; text: "<img src=\"red.png\"><img src=\"tall.png\">" }
          R { id: elsewhere; objectName: "elsewhere"; y: 100; baseUrl: Qt.resolvedUrl("elsewhere/A.qml"); text: "<img src=\"tall.png\">" }
          R { objectName: "back"; y: 100; baseUrl: Qt.resolvedUrl("elsewhere/A.qml"); text: "<img src=\"../tall.png\">" }
          Text { objectName: "markdown"; y: 100; textFormat: Text.MarkdownText; text: "![a picture](red.png)" }
        }
        """
      When 100 milliseconds pass

    Scenario: An image is as large as its picture
      Then the property "implicitWidth" of "one" is 40
      And the property "implicitHeight" of "one" is 20

    Scenario: The picture is painted where the image stands
      Then the pixel at 5, 5 of the window is "#ff0000"
      And the pixel at 35, 15 of the window is "#ff0000"
      And the pixel at 45, 5 of the window is "#ffffff"
      And the pixel at 5, 25 of the window is "#ffffff"

    Scenario: A document gives an image its size
      Then the property "implicitWidth" of "sized" is 10
      And the property "implicitHeight" of "sized" is 50

    Scenario: An image given a width keeps the shape of its picture
      Then the property "implicitWidth" of "wide" is 80
      And the property "implicitHeight" of "wide" is 40

    Scenario: An image given a height keeps the shape of its picture
      Then the property "implicitWidth" of "high" is 160
      And the property "implicitHeight" of "high" is 80

    Scenario: An image whose picture is not found leaves a small gap
      Then the property "implicitWidth" of "missing" is 16
      And the property "implicitHeight" of "missing" is 16

    Scenario: An image stands in the line beside the text
      Then the property "besideText" of "root" is true

    Scenario: An image stands on the baseline of its line
      Then the property "onTheBaseline" of "root" is true

    Scenario: Images stand side by side and the line is as high as the highest
      Then the property "implicitWidth" of "two" is 70
      And the property "implicitHeight" of "two" is 60

    Scenario: Markdown names an image with its text and its address
      Then the property "implicitWidth" of "markdown" is 40
      And the property "implicitHeight" of "markdown" is 20

    Scenario: Until it is given, baseUrl is the address of the QML file
      Then the property "fileIsTheBase" of "root" is true

    Scenario: A picture is looked for from baseUrl
      Then the property "implicitWidth" of "elsewhere" is 16
      And the property "implicitHeight" of "elsewhere" is 16

    Scenario: A name may step back out of the base
      Then the property "implicitWidth" of "back" is 30
      And the property "implicitHeight" of "back" is 60

    Scenario: A new baseUrl finds the pictures again
      When the method "rebase" of "root" is called
      Then the property "implicitWidth" of "elsewhere" is 30
      And the property "implicitHeight" of "elsewhere" is 60

    Scenario: A new text finds its pictures
      When the property "text" of "missing" is set to "<img src=\"tall.png\">"
      Then the property "implicitWidth" of "missing" is 30
      And the property "implicitHeight" of "missing" is 60

  Rule: StyledText

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"; width: 300; height: 300; color: "white"
          property real space: spaced.implicitWidth - letters.implicitWidth
          function room(width) { return (Math.floor(width / space) + 1) * space }
          property bool wholeSpaces: one.implicitWidth === space + room(40)
          property bool bothSidesGiven: sized.implicitWidth === space + room(10)
          property bool oneSideIsNotEnough: wide.implicitWidth === one.implicitWidth && high.implicitWidth === one.implicitWidth
          property bool together: two.implicitWidth === space + room(40) + room(30)
          property bool afterALetter: lettered.implicitWidth === letter.implicitWidth + 2 * space + room(40) + room(30)
          property bool afterASpace: after.implicitWidth === letter.implicitWidth + one.implicitWidth && spaceAfter.implicitWidth === after.implicitWidth
          property bool lineIsItsOwn: low.implicitHeight === letter.implicitHeight
          component S: Text { y: 100; textFormat: Text.StyledText }
          Text { id: spaced; y: 100; text: "a b" } Text { id: letters; y: 100; text: "ab" }
          Text { id: letter; y: 100; text: "a" }
          S { id: one; objectName: "one"; y: 0; text: "<img src=\"red.png\">" }
          S { id: sized; objectName: "sized"; text: "<img src=\"red.png\" width=\"10\" height=\"50\">" }
          S { id: wide; objectName: "wide"; text: "<img src=\"red.png\" width=\"80\">" }
          S { id: high; objectName: "high"; text: "<img src=\"red.png\" height=\"80\">" }
          S { id: two; objectName: "two"; text: "<img src=\"red.png\"><img src=\"tall.png\">" }
          S { id: lettered; text: "<img src=\"red.png\">a<img src=\"tall.png\">" }
          S { id: after; text: "a <img src=\"red.png\">" } S { id: spaceAfter; text: "a<img src=\"red.png\"> " }
          S { objectName: "tall"; text: "abc<img src=\"tall.png\">abc" }
          S { id: low; text: "a<img src=\"red.png\" width=\"10\" height=\"5\">" }
        }
        """
      When 100 milliseconds pass

    @qt-parity
    Scenario: An image is as high as its picture
      Then the property "implicitHeight" of "one" is 20

    @qt-parity
    Scenario: An image takes a whole number of spaces, after a space of its own
      Then the property "wholeSpaces" of "root" is true

    @qt-parity
    Scenario: The picture is painted in the room of its image
      Then the pixel at 10, 10 of the window is "#ff0000"
      And the pixel at 30, 10 of the window is "#ff0000"
      And the pixel at 60, 10 of the window is "#ffffff"

    @qt-parity
    Scenario: A document gives an image its size with both sides
      Then the property "implicitHeight" of "sized" is 50
      And the property "bothSidesGiven" of "root" is true

    @qt-parity
    Scenario: One side is not a size
      Then the property "implicitHeight" of "wide" is 20
      And the property "implicitHeight" of "high" is 20
      And the property "oneSideIsNotEnough" of "root" is true

    @qt-parity
    Scenario: An image after an image needs no space of its own
      Then the property "together" of "root" is true
      And the property "implicitHeight" of "two" is 60

    @qt-parity
    Scenario: An image after a letter has a space of its own
      Then the property "afterALetter" of "root" is true

    @qt-parity
    Scenario: A written space beside an image is not a second one
      Then the property "afterASpace" of "root" is true

    @qt-parity
    Scenario: A line is as high as the image in it
      Then the property "implicitHeight" of "tall" is 60

    # Qt 6.11.2 makes the line of an image that is lower than the line
    # higher by what the image lacks: 5 pixels of image in a line of 19
    # give a Text 33 pixels high. In Ljós an image that fits in its line
    # leaves the line as it is.
    @qt-diverges
    Scenario: An image lower than its line leaves the line as high as it is
      Then the property "lineIsItsOwn" of "root" is true

  # Qt 6.11.2 finds the picture the same way (it reports the same size) and
  # then never finishes painting a Text whose baseUrl is no whole address,
  # so qml6 cannot run this.
  @ljos-only
  Rule: A base that is no whole address

    Scenario: A base that is no whole address is beside the QML file
      Given the QML:
        """qml
        import QtQuick
        Text {
          objectName: "text"; textFormat: Text.RichText
          baseUrl: "elsewhere/"; text: "<img src=\"../tall.png\">"
        }
        """
      When 100 milliseconds pass
      Then the property "baseUrl" of "text" is "elsewhere/"
      And the property "implicitWidth" of "text" is 30
      And the property "implicitHeight" of "text" is 60
