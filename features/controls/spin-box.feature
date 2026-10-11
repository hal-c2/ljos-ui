# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-spinbox
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#locale-method
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: SpinBox
  A spin box holds a whole number between two bounds. It is stepped up and
  down, and shows the number as text. The numbers are those of the bundled
  font, Liberation Sans, at 14 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 300
        SpinBox {
          objectName: "s"
          x: 20; y: 20
          font.family: "Liberation Sans"; font.pixelSize: 14
          property string log: ""
          onValueChanged: log += "v" + value + " "
          onValueModified: log += "m" + value + " "
        }
        SpinBox {
          objectName: "kg"
          y: 100; value: 5
          textFromValue: function(value, locale) { return value + " kg" }
        }
        SpinBox {
          objectName: "de"
          y: 200; to: 9999999; value: 1234567
          locale: Qt.locale("de_DE")
        }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A spin box starts at nought, between 0 and 99
      Then the property "<property>" of "s" is <value>

      Examples:
        | property             | value |
        | from                 | 0     |
        | to                   | 99    |
        | value                | 0     |
        | stepSize             | 1     |
        | editable             | false |
        | wrap                 | false |
        | live                 | false |
        | displayText          | "0"   |
        | inputMethodHints     | 65536 |
        | inputMethodComposing | false |
        | wheelEnabled         | false |
        | hoverEnabled         | true  |
        | up.pressed           | false |
        | up.hovered           | false |
        | down.pressed         | false |
        | down.hovered         | false |

    Scenario Outline: It is as large as its background, with room for a button at either side
      Then the property "<property>" of "s" is <value>

      Examples:
        | property                     | value |
        | width                        | 140   |
        | height                       | 40    |
        | implicitWidth                | 140   |
        | implicitHeight               | 40    |
        | implicitBackgroundWidth      | 140   |
        | implicitBackgroundHeight     | 0     |
        | implicitContentHeight        | 28    |
        | padding                      | 0     |
        | leftPadding                  | 40    |
        | rightPadding                 | 40    |
        | topPadding                   | 0     |
        | spacing                      | 0     |
        | availableWidth               | 60    |
        | availableHeight              | 40    |
        | up.implicitIndicatorWidth    | 40    |
        | up.implicitIndicatorHeight   | 40    |
        | down.implicitIndicatorWidth  | 40    |
        | down.implicitIndicatorHeight | 40    |

    @api-QtQuick.Controls.SpinBox.contentItem
    Scenario Outline: The number is a TextInput between the buttons
      Then the property "<property>" of "s" is <value>

      Examples:
        | property                        | value     |
        | contentItem.x                   | 40        |
        | contentItem.y                   | 0         |
        | contentItem.width               | 60        |
        | contentItem.height              | 40        |
        | contentItem.z                   | 2         |
        | contentItem.text                | "0"       |
        | contentItem.readOnly            | true      |
        | contentItem.horizontalAlignment | 4         |
        | contentItem.verticalAlignment   | 128       |
        | contentItem.padding             | 6         |
        | contentItem.inputMethodHints    | 65536     |
        | contentItem.color               | "#353637" |
        | contentItem.selectionColor      | "#0066ff" |
        | contentItem.font.family         | "Liberation Sans" |

    Scenario: The number stands in the middle of the room between the buttons
      Then the property "contentItem.cursorRectangle.y" of "s" is 12
      And the property "contentItem.cursorRectangle.x" of "s" is at least 20

    @api-QtQuick.Controls.SpinBox.up.indicator @api-QtQuick.Controls.SpinBox.down.indicator
    Scenario Outline: The button that steps up is at the right, the one that steps down at the left
      Then the property "<property>" of "s" is <value>

      Examples:
        | property              | value     |
        | up.indicator.x        | 100       |
        | up.indicator.y        | 0         |
        | up.indicator.width    | 40        |
        | up.indicator.height   | 40        |
        | up.indicator.color    | "#e0e0e0" |
        | up.indicator.enabled  | true      |
        | down.indicator.x      | 0         |
        | down.indicator.y      | 0         |
        | down.indicator.width  | 40        |
        | down.indicator.height | 40        |
        | down.indicator.color  | "#e0e0e0" |

    @api-QtQuick.Controls.SpinBox.background
    Scenario Outline: The background fills the box and lies behind the rest
      Then the property "<property>" of "s" is <value>

      Examples:
        | property                | value     |
        | background.x            | 0         |
        | background.width        | 140       |
        | background.height       | 40        |
        | background.z            | -1        |
        | background.color        | "#ffffff" |
        | background.border.color | "#e0e0e0" |

    Scenario: A larger font makes a taller box
      When the property "font.pixelSize" of "s" is set to 40
      Then the property "implicitHeight" of "s" is 57
      And the property "contentItem.font.pixelSize" of "s" is 40

    Scenario: A box that is not enabled is drawn in the colours for that
      When the property "enabled" of "s" is set to false
      Then the property "contentItem.color" of "s" is "#7f353637"
      And the property "background.color" of "s" is "#e0e0e0"
      And the property "up.indicator.enabled" of "s" is false

    Scenario: The buttons follow the height of the box
      When the property "height" of "s" is set to 60
      And the property "width" of "s" is set to 200
      Then the property "up.indicator.height" of "s" is 60
      And the property "up.indicator.x" of "s" is 160
      And the property "contentItem.width" of "s" is 120

  @qt-parity
  Rule: The value stays between from and to

    @api-QtQuick.Controls.SpinBox.value
    Scenario Outline: A value of <given> becomes <kept>
      When the property "value" of "s" is set to <given>
      Then the property "value" of "s" is <kept>

      Examples:
        | given | kept |
        | 42    | 42   |
        | 150   | 99   |
        | -5    | 0    |

    @api-QtQuick.Controls.SpinBox.to
    Scenario: A lower end takes the value with it
      When the property "value" of "s" is set to 80
      And the property "to" of "s" is set to 50
      Then the property "value" of "s" is 50

    @api-QtQuick.Controls.SpinBox.from
    Scenario: A higher beginning takes the value with it
      When the property "value" of "s" is set to 20
      And the property "from" of "s" is set to 30
      Then the property "value" of "s" is 30
      And the property "displayText" of "s" is "30"

    Scenario: A range may be given the wrong way round
      When the property "from" of "s" is set to 10
      And the property "to" of "s" is set to 5
      Then the property "value" of "s" is 10
      When the property "value" of "s" is set to 7
      Then the property "value" of "s" is 7
      When the property "value" of "s" is set to 20
      Then the property "value" of "s" is 10
      When the property "value" of "s" is set to 2
      Then the property "value" of "s" is 5

    Scenario: A button is not enabled where there is nowhere to go
      Then the property "down.indicator.enabled" of "s" is false
      And the property "up.indicator.enabled" of "s" is true
      When the property "value" of "s" is set to 99
      Then the property "down.indicator.enabled" of "s" is true
      And the property "up.indicator.enabled" of "s" is false

    Scenario: With wrap both buttons are always enabled
      When the property "wrap" of "s" is set to true
      Then the property "down.indicator.enabled" of "s" is true
      And the property "up.indicator.enabled" of "s" is true

  @qt-parity
  Rule: increase and decrease step the value

    @api-QtQuick.Controls.SpinBox.increase
    Scenario: increase adds one step and says the value changed, not that the user changed it
      When the method "increase" of "s" is called
      Then the property "value" of "s" is 1
      And the property "displayText" of "s" is "1"
      And the property "log" of "s" is "v1 "

    @api-QtQuick.Controls.SpinBox.decrease
    Scenario: decrease takes one step away
      When the property "value" of "s" is set to 5
      And the method "decrease" of "s" is called
      Then the property "value" of "s" is 4

    @api-QtQuick.Controls.SpinBox.stepSize
    Scenario: A step that would pass the end stops at it
      When the property "stepSize" of "s" is set to 10
      And the property "value" of "s" is set to 95
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 99

    Scenario: A step below the beginning stops at it
      When the property "stepSize" of "s" is set to 10
      And the property "value" of "s" is set to 5
      And the method "decrease" of "s" is called
      Then the property "value" of "s" is 0

    Scenario: A negative step turns increase around
      When the property "value" of "s" is set to 20
      And the property "stepSize" of "s" is set to -3
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 17

    Scenario: A step of nought goes nowhere
      When the property "value" of "s" is set to 20
      And the property "stepSize" of "s" is set to 0
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 20

    @api-QtQuick.Controls.SpinBox.wrap
    Scenario: With wrap a step past the end begins again
      When the property "to" of "s" is set to 20
      And the property "stepSize" of "s" is set to 5
      And the property "wrap" of "s" is set to true
      And the property "value" of "s" is set to 18
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 0
      When the method "decrease" of "s" is called
      Then the property "value" of "s" is 20

    Scenario: In a range given the wrong way round increase goes towards to
      When the property "from" of "s" is set to 10
      And the property "to" of "s" is set to 5
      And the property "value" of "s" is set to 7
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 6
      When the method "increase" of "s" is called
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 5
      When the method "decrease" of "s" is called
      Then the property "value" of "s" is 6

    Scenario: A range given the wrong way round wraps from to to from
      When the property "from" of "s" is set to 10
      And the property "to" of "s" is set to 5
      And the property "wrap" of "s" is set to true
      And the property "value" of "s" is set to 5
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 10
      When the method "decrease" of "s" is called
      Then the property "value" of "s" is 5

  @qt-parity
  Rule: The number is shown as the locale writes it

    @api-QtQuick.Controls.SpinBox.displayText
    Scenario: Thousands are grouped
      When the property "to" of "s" is set to 9999999
      And the property "value" of "s" is set to 1234567
      Then the property "displayText" of "s" is "1,234,567"
      And the property "contentItem.text" of "s" is "1,234,567"

    Scenario: A negative number has its sign in front
      When the property "from" of "s" is set to -5000
      And the property "value" of "s" is set to -1234
      Then the property "displayText" of "s" is "-1,234"

    Scenario: The locale of the box decides how
      Then the property "displayText" of "de" is "1.234.567"
      And the property "validator.locale" of "de" is "de_DE"

    @api-QtQuick.Controls.SpinBox.textFromValue
    Scenario: textFromValue writes the text
      Then the property "displayText" of "kg" is "5 kg"
      When the method "increase" of "kg" is called
      Then the property "displayText" of "kg" is "6 kg"
      And the property "contentItem.text" of "kg" is "6 kg"

    @api-QtQuick.Controls.SpinBox.validator
    Scenario: The validator takes whole numbers of the range
      Then the property "validator.bottom" of "s" is 0
      And the property "validator.top" of "s" is 99
      When the property "from" of "s" is set to -10
      Then the property "validator.bottom" of "s" is -10

    Scenario: The validator of a range given the wrong way round is the right way round
      When the property "from" of "s" is set to 10
      And the property "to" of "s" is set to 5
      Then the property "validator.bottom" of "s" is 5
      And the property "validator.top" of "s" is 10

  @ljos-only @a11y
  Rule: A screen reader is told the number and may change it

    Background:
      When the property "Accessible.name" of "s" is set to "Count"

    Scenario: A spin box is a spin button
      Then the object "s" is exposed as a "spinbutton" named "Count"

    Scenario: An increase is a step by the user
      When an assistive technology asks the "spinbutton" named "Count" to increase
      Then the property "value" of "s" is 1
      And the property "log" of "s" is "v1 m1 "

    Scenario: A decrease at the beginning is refused
      When an assistive technology asks the "spinbutton" named "Count" to decrease
      Then the property "value" of "s" is 0
      And the property "log" of "s" is ""

    Scenario Outline: It takes a value and keeps it in its range
      When an assistive technology asks the "spinbutton" named "Count" to take the value <asked>
      Then the property "value" of "s" is <kept>

      Examples:
        | asked | kept |
        | 42    | 42   |
        | 500   | 99   |
        | -3    | 0    |

    Scenario: A box that is not enabled does not change
      When the property "enabled" of "s" is set to false
      And an assistive technology asks the "spinbutton" named "Count" to increase
      Then the property "value" of "s" is 0
