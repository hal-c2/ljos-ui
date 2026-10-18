# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html#scrollable-textarea
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#wrapMode-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A TextArea that is the one item of a ScrollView
  The area fills the view: it is as wide as the view while it wraps, and
  no narrower than the view while it does not, and no lower than the view.
  The view scrolls what the area asks for, so a long line wraps and every
  line makes the content taller, and a line that does not wrap scrolls
  sideways. Where the cursor goes, the view follows, keeping the area's
  padding around it in view. The numbers are those of the bundled font,
  Liberation Sans, at 14 pixels, in the Basic style: a line is 16 pixels,
  and the area's padding 10 at the sides and 6 above and below.

  Rule: A wrapping area

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          function addLine() { area.text = area.text + "\nand another" }
          ScrollView {
            id: view
            objectName: "view"
            width: 200; height: 150
            TextArea {
              id: area
              objectName: "area"
              wrapMode: TextEdit.Wrap
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "a line of words that is a great deal wider than the two hundred pixels of the view it is in"
            }
          }
        }
        """

    Scenario: The area is as wide as the view and wraps
      Then the property "width" of "area" is 200
      And the property "lineCount" of "area" is 4
      And the property "contentWidth" of "area" is at most 180
      And the property "implicitHeight" of "area" is 76

    Scenario: The area is as tall as the view while its lines are fewer
      Then the property "height" of "area" is 150

    Scenario: The view scrolls the wrapped height and no width
      Then the property "contentHeight" of "view" is 76
      And the property "contentWidth" of "view" is at most 200
      And the property "contentItem.contentHeight" of "view" is 76

    Scenario: A new line makes the area and its content taller
      When the method "addLine" of "root" is called
      Then the property "lineCount" of "area" is 5
      And the property "implicitHeight" of "area" is 92
      And the property "contentHeight" of "view" is 92

    Scenario: A wider view wraps the text onto fewer lines
      When the property "width" of "view" is set to 300
      Then the property "width" of "area" is 300
      And the property "lineCount" of "area" is 2
      And the property "contentHeight" of "view" is 44

  Rule: An area that does not wrap

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            TextArea {
              objectName: "area"
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "a line of words that is a great deal wider than the two hundred pixels"
            }
          }
          ScrollView {
            objectName: "small"
            y: 200; width: 180; height: 100
            TextArea {
              objectName: "short"
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "one"
            }
          }
        }
        """

    Scenario: A long line keeps its width and the view scrolls sideways
      Then the property "lineCount" of "area" is 1
      And the property "width" of "area" is at least 430
      And the property "height" of "area" is 100
      And the property "contentWidth" of "view" is at least 430
      And the property "contentHeight" of "view" is 28

    Scenario: A short line is as wide as the view
      Then the property "width" of "short" is 180
      And the property "height" of "short" is 100
      And the property "contentWidth" of "small" is at most 40

    Scenario: The view follows the cursor to the end of the line and back
      When the property "cursorPosition" of "area" is set to 70
      Then the property "contentItem.contentX" of "view" is at least 237
      And the property "contentItem.contentX" of "view" is at most 238
      When the property "cursorPosition" of "area" is set to 10
      Then the property "contentItem.contentX" of "view" is 53

  Rule: Lines below the view

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          ScrollView {
            objectName: "view"
            width: 200; height: 100
            TextArea {
              objectName: "area"
              wrapMode: TextEdit.Wrap
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve"
            }
          }
        }
        """

    Scenario: The area is as tall as its lines
      Then the property "height" of "area" is 204
      And the property "contentHeight" of "view" is 204
      And the property "contentItem.contentY" of "view" is 0

    Scenario: The view follows the cursor down to the last line and up again
      When the property "cursorPosition" of "area" is set to 62
      Then the property "contentItem.contentY" of "view" is 104
      When the property "cursorPosition" of "area" is set to 24
      Then the property "contentItem.contentY" of "view" is 80
      When the property "cursorPosition" of "area" is set to 0
      Then the property "contentItem.contentY" of "view" is 0

    Scenario: Typing at the end keeps the cursor in view
      When the property "cursorPosition" of "area" is set to 62
      And the method "forceActiveFocus" of "area" is called
      And the user presses "Return"
      And the user types "x"
      Then the property "contentHeight" of "view" is 220
      And the property "contentItem.contentY" of "view" is 120

  Rule: A view as tall as the area asks

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          function setLines(n) {
            let lines = []
            for (let i = 0; i < n; ++i) lines.push("line " + i)
            area.text = lines.join("\n")
          }
          function twoLines() { setLines(2) }
          function twelveLines() { setLines(12) }
          ScrollView {
            objectName: "view"
            width: 180; height: Math.min(implicitHeight, 200)
            TextArea {
              id: area
              objectName: "area"
              wrapMode: TextEdit.Wrap
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "one"
            }
          }
        }
        """

    Scenario: The view grows a line at a time up to its limit
      Then the property "height" of "view" is 28
      And the property "width" of "area" is 180
      When the method "twoLines" of "root" is called
      Then the property "height" of "view" is 44
      And the property "height" of "area" is 44
      When the method "twelveLines" of "root" is called
      Then the property "height" of "view" is 200
      And the property "height" of "area" is 204
