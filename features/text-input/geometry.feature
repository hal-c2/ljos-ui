# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#positionAt-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#positionToRectangle-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#cursorRectangle-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#horizontalAlignment-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#autoScroll-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where the text and the cursor of a TextInput are
  Positions in the text map to places in the item and back. The numbers are
  those of the bundled font, Liberation Sans, at 14 pixels.

  Rule: Positions map to rectangles and back

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"
            font.family: "Liberation Sans"; font.pixelSize: 14
            text: "hello brave world"
            property real probe: 0
            function gapAt(x) { return positionAt(x, 5, TextInput.CursorBetweenCharacters) }
            function characterAt(x) { return positionAt(x, 5, TextInput.CursorOnCharacter) }
          }
        }
        """

    Scenario Outline: The size of the text
      Then the property "<property>" of "input" is <value>

      Examples:
        | property               | value  |
        | contentWidth           | 105.75 |
        | contentHeight          | 16     |
        | implicitWidth          | 106    |
        | implicitHeight         | 16     |
        | cursorRectangle.x      | 105.75 |
        | cursorRectangle.y      | 0      |
        | cursorRectangle.width  | 1      |
        | cursorRectangle.height | 16     |

    Scenario: The cursor rectangle follows the cursor
      When the property "cursorPosition" of "input" is set to 1
      Then the property "cursorRectangle.x" of "input" is 7.78125

    Scenario Outline: positionToRectangle says where the cursor would be
      When the property "probe" of "input" is bound to "positionToRectangle(<position>).<side>"
      Then the property "probe" of "input" is <value>

      Examples:
        | position | side   | value   |
        | 5        | x      | 29.5625 |
        | 5        | height | 16      |
        | 17       | x      | 105.75  |
        | 40       | height | 0       |

    Scenario Outline: positionAt finds the nearest gap between characters
      When the property "cursorPosition" of "input" is bound to "gapAt(<x>)"
      Then the property "cursorPosition" of "input" is <position>

      Examples:
        | x   | position |
        | 3   | 0        |
        | 4   | 1        |
        | 5   | 1        |
        | 500 | 17       |

    Scenario: positionAt can find the character under a point instead
      When the property "cursorPosition" of "input" is bound to "characterAt(7)"
      Then the property "cursorPosition" of "input" is 0

  Rule: The text is aligned inside the item

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"
            font.family: "Liberation Sans"; font.pixelSize: 14
            width: 200; height: 60
            text: "hello brave world"
          }
        }
        """

    Scenario Outline: Horizontal alignment moves the cursor with the text
      When the property "horizontalAlignment" of "input" is set to <alignment>
      Then the property "cursorRectangle.x" of "input" is <x>

      Examples:
        | alignment | x       |
        | 1         | 105.75  |
        | 2         | 200     |
        | 4         | 152.875 |

    Scenario Outline: Vertical alignment moves the cursor with the text
      When the property "verticalAlignment" of "input" is set to <alignment>
      Then the property "cursorRectangle.y" of "input" is <y>

      Examples:
        | alignment | y  |
        | 32        | 0  |
        | 64        | 44 |
        | 128       | 22 |

    Scenario: Padding moves the text and adds to the implicit size
      When the property "padding" of "input" is set to 8
      And the property "cursorPosition" of "input" is set to 0
      Then the property "cursorRectangle.x" of "input" is 8
      And the property "cursorRectangle.y" of "input" is 8
      And the property "implicitWidth" of "input" is 122
      And the property "implicitHeight" of "input" is 32

  Rule: Right-to-left text starts at the right

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"
            font.family: "Liberation Sans"; font.pixelSize: 14
            width: 200
            text: "שלום"
          }
        }
        """

    Scenario: The text aligns right by itself
      Then the property "horizontalAlignment" of "input" is 2
      And the property "effectiveHorizontalAlignment" of "input" is 2

    Scenario: Position 0 is at the right edge
      When the property "cursorPosition" of "input" is set to 0
      Then the property "cursorRectangle.x" of "input" is 200

  Rule: A text wider than the item scrolls to keep the cursor in view

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"
            font.family: "Liberation Sans"; font.pixelSize: 14
            width: 60
            text: "hello brave world"
          }
        }
        """

    Scenario: The cursor at the end sits at the right edge
      Then the property "cursorRectangle.x" of "input" is 60

    Scenario: Moving the cursor to the start scrolls back
      When the property "cursorPosition" of "input" is set to 0
      Then the property "cursorRectangle.x" of "input" is 0

    Scenario: A cursor that is in view does not scroll the text
      When the property "cursorPosition" of "input" is set to 12
      Then the property "cursorRectangle.x" of "input" is 26.5625

    Scenario: Without autoScroll the text stays where it is
      When the property "autoScroll" of "input" is set to false
      And the property "cursorPosition" of "input" is set to 0
      And the property "cursorPosition" of "input" is set to 17
      Then the property "cursorRectangle.x" of "input" is 105.75

  Rule: positionToRectangle leaves out the padding, unlike cursorRectangle

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; width: 100; leftPadding: 10; topPadding: 7
            font.family: "Liberation Sans"; font.pixelSize: 20
            text: "xx"
            property real probe: 0
          }
          TextInput {
            objectName: "centred"; y: 100; width: 100; padding: 6
            horizontalAlignment: TextInput.AlignHCenter
            font.family: "Liberation Sans"; font.pixelSize: 20
            text: "x"
            property real probe: 0
          }
        }
        """

    Scenario Outline: positionToRectangle(<position>).<side> of <name> is <value>
      When the property "probe" of "<name>" is bound to "positionToRectangle(<position>).<side>"
      Then the property "probe" of "<name>" is <value>

      Examples:
        | name    | position | side | value |
        | input   | 0        | x    | 0     |
        | input   | 2        | x    | 20    |
        | input   | 0        | y    | 0     |
        | centred | 0        | x    | 39    |

    Scenario: cursorRectangle counts the padding
      Then the property "cursorRectangle.x" of "input" is 30
      And the property "cursorRectangle.y" of "input" is 7
      And the property "cursorRectangle.x" of "centred" is 55

  Rule: Centred text starts a whole 64th of a pixel left of the middle

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; x: 20; y: 20; width: 60; padding: 6
            horizontalAlignment: TextInput.AlignHCenter
            font.family: "Liberation Sans"; font.pixelSize: 12
            text: "0"
            property real probe: 0
          }
        }
        """

    Scenario: The free room is cut to a 64th before it is halved
      When the property "probe" of "input" is bound to "positionToRectangle(0).x"
      Then the property "probe" of "input" is 20.65625
      And the property "cursorRectangle.x" of "input" is 33.328125

    Scenario Outline: A press at <x> puts the cursor at <position>
      When the user presses the pointer at <x>, 30
      And the user releases the pointer at <x>, 30
      Then the property "cursorPosition" of "input" is <position>

      Examples:
        | x  | position |
        | 49 | 0        |
        | 50 | 1        |
