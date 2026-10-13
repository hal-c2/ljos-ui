# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-doublespinbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#locale-method
# Verified on Qt 6.11.2 (qml6, offscreen; key and pointer input over its vnc
# platform, the wheel with QtTest's mouseWheel).

Feature: DoubleSpinBox
  A double spin box holds a number with a set count of decimals between two
  bounds. It is stepped and typed as a SpinBox is, and shows the number as
  the locale writes one with that many decimals.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 340
        DoubleSpinBox {
          objectName: "s"; x: 20; y: 20
          font.family: "Liberation Sans"; font.pixelSize: 14
          property string log: ""; property string said: ""
          onValueChanged: log += "v" + value + " "
          onValueModified: log += "m" + value + " "
          function say(v, places, name) { said = textFromValue(v, places, Qt.locale(name)) }
          function read(text, name) { said = String(valueFromText(text, Qt.locale(name))) }
        }
        DoubleSpinBox {
          objectName: "kg"; x: 20; y: 100; value: 2.5; editable: true
          textFromValue: function(value, decimals, locale) { return value.toFixed(decimals) + " kg" }
          valueFromText: function(text, locale) { return parseFloat(text) * 2 }
        }
        DoubleSpinBox {
          objectName: "de"; x: 20; y: 180; to: 100000; value: 1234.5; editable: true
          locale: Qt.locale("de_DE")
        }
        TextInput { objectName: "t"; y: 300; width: 100; height: 20 }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A double spin box starts at nought, between 0 and 99.99, with two decimals
      Then the property "<property>" of "s" is <value>

      Examples:
        | property             | value  |
        | from                 | 0      |
        | to                   | 99.99  |
        | value                | 0      |
        | stepSize             | 1      |
        | decimals             | 2      |
        | editable             | false  |
        | wrap                 | false  |
        | displayText          | "0.00" |
        | inputMethodHints     | 65536  |
        | inputMethodComposing | false  |
        | wheelEnabled         | false  |
        | up.pressed           | false  |
        | down.hovered         | false  |

    Scenario Outline: It is laid out as a spin box is
      Then the property "<property>" of "s" is <value>

      Examples:
        | property                   | value     |
        | width                      | 140       |
        | height                     | 40        |
        | leftPadding                | 40        |
        | rightPadding               | 40        |
        | implicitContentHeight      | 28        |
        | contentItem.x              | 40        |
        | contentItem.width          | 60        |
        | contentItem.text           | "0.00"    |
        | contentItem.readOnly       | true      |
        | contentItem.inputMethodHints | 65536   |
        | contentItem.color          | "#353637" |
        | up.indicator.x             | 100       |
        | up.indicator.width         | 40        |
        | up.indicator.enabled       | true      |
        | down.indicator.x           | 0         |
        | down.indicator.enabled     | false     |
        | background.width           | 140       |
        | background.color           | "#ffffff" |

  @qt-parity
  Rule: decimals is how many places the value has

    @api-QtQuick.Controls.DoubleSpinBox.value
    Scenario Outline: A value of <given> is kept as <kept>
      When the property "value" of "s" is set to <given>
      Then the property "value" of "s" is <kept>
      And the property "displayText" of "s" is "<shown>"

      Examples:
        | given   | kept  | shown |
        | 42.5    | 42.5  | 42.50 |
        | 1.234   | 1.23  | 1.23  |
        | 3.14159 | 3.14  | 3.14  |
        | 0.125   | 0.13  | 0.13  |
        | 2.675   | 2.67  | 2.67  |

    @api-QtQuick.Controls.DoubleSpinBox.decimals
    Scenario: Fewer decimals round the value and the range
      When the property "value" of "s" is set to 2.67
      And the property "decimals" of "s" is set to 0
      Then the property "value" of "s" is 3
      And the property "displayText" of "s" is "3"
      And the property "to" of "s" is 100
      And the property "log" of "s" is "v2.67 v3 "

    Scenario: A half is rounded away from nought
      When the property "decimals" of "s" is set to 0
      And the property "value" of "s" is set to 2.5
      Then the property "value" of "s" is 3
      When the property "value" of "s" is set to 3.5
      Then the property "value" of "s" is 4

    Scenario: More decimals show more places
      When the property "decimals" of "s" is set to 4
      And the property "value" of "s" is set to 1.23456789
      Then the property "value" of "s" is 1.2346
      And the property "displayText" of "s" is "1.2346"

    Scenario: The ends of the range have the decimals too
      When the property "to" of "s" is set to 5.555
      Then the property "to" of "s" is 5.55
      When the property "from" of "s" is set to 0.004
      Then the property "from" of "s" is 0
      When the property "from" of "s" is set to 0.006
      Then the property "from" of "s" is 0.01

  @qt-parity
  Rule: The value stays between from and to

    Scenario Outline: A value of <given> becomes <kept>
      When the property "value" of "s" is set to <given>
      Then the property "value" of "s" is <kept>

      Examples:
        | given | kept  |
        | 150   | 99.99 |
        | -5    | 0     |

    @api-QtQuick.Controls.DoubleSpinBox.to
    Scenario: A lower end takes the value with it
      When the property "value" of "s" is set to 80
      And the property "to" of "s" is set to 50.5
      Then the property "value" of "s" is 50.5
      And the property "displayText" of "s" is "50.50"

    @api-QtQuick.Controls.DoubleSpinBox.from
    Scenario: A range may be given the wrong way round
      When the property "from" of "s" is set to 10
      And the property "to" of "s" is set to 5
      Then the property "value" of "s" is 10
      When the property "value" of "s" is set to 2
      Then the property "value" of "s" is 5

    Scenario: A button is not enabled where there is nowhere to go
      When the property "value" of "s" is set to 99.99
      Then the property "down.indicator.enabled" of "s" is true
      And the property "up.indicator.enabled" of "s" is false

  @qt-parity
  Rule: increase and decrease step the value

    @api-QtQuick.Controls.DoubleSpinBox.increase
    Scenario: increase adds one step and says the value changed, not that the user changed it
      When the method "increase" of "s" is called
      Then the property "value" of "s" is 1
      And the property "displayText" of "s" is "1.00"
      And the property "log" of "s" is "v1 "

    @api-QtQuick.Controls.DoubleSpinBox.stepSize
    Scenario: A step may be a part of one
      When the property "stepSize" of "s" is set to 0.25
      And the property "value" of "s" is set to 1.5
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 1.75
      And the property "displayText" of "s" is "1.75"

    Scenario: A step is added as it is, and only the text is rounded
      When the property "stepSize" of "s" is set to 0.1
      And the method "increase" of "s" is called
      And the method "increase" of "s" is called
      And the method "increase" of "s" is called
      Then the property "log" of "s" is "v0.1 v0.2 v0.30000000000000004 "
      And the property "displayText" of "s" is "0.30"

    @api-QtQuick.Controls.DoubleSpinBox.decrease
    Scenario: A step that would pass the end stops at it
      When the property "value" of "s" is set to 99.5
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 99.99
      When the property "value" of "s" is set to 0.5
      And the method "decrease" of "s" is called
      Then the property "value" of "s" is 0

    Scenario: A negative step turns increase around
      When the property "value" of "s" is set to 1.5
      And the property "stepSize" of "s" is set to -0.25
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 1.25

    @api-QtQuick.Controls.DoubleSpinBox.wrap
    Scenario: With wrap a step past the end begins again
      When the property "to" of "s" is set to 1
      And the property "stepSize" of "s" is set to 0.3
      And the property "wrap" of "s" is set to true
      And the property "value" of "s" is set to 0.9
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 0
      When the method "decrease" of "s" is called
      Then the property "value" of "s" is 1

    Scenario: In a range given the wrong way round increase goes towards to
      When the property "from" of "s" is set to 10
      And the property "to" of "s" is set to 5
      And the property "value" of "s" is set to 7
      And the method "increase" of "s" is called
      Then the property "value" of "s" is 6

  @qt-parity
  Rule: The number is shown as the locale writes it

    @api-QtQuick.Controls.DoubleSpinBox.displayText
    Scenario: Thousands are grouped
      When the property "to" of "s" is set to 100000
      And the property "value" of "s" is set to 1234.5
      Then the property "displayText" of "s" is "1,234.50"
      And the property "contentItem.text" of "s" is "1,234.50"

    Scenario: A negative number has its sign in front
      When the property "from" of "s" is set to -5000
      And the property "value" of "s" is set to -1234.5
      Then the property "displayText" of "s" is "-1,234.50"

    Scenario: The locale of the box decides how
      Then the property "displayText" of "de" is "1.234,50"
      And the property "validator.locale" of "de" is "de_DE"

    @api-QtQuick.Controls.DoubleSpinBox.textFromValue
    Scenario: textFromValue is given the decimals
      Then the property "displayText" of "kg" is "2.50 kg"
      When the method "increase" of "kg" is called
      Then the property "displayText" of "kg" is "3.50 kg"

    # valueFromText doubles what it reads, so the new value shows that the
    # text was read.
    Scenario: Other decimals read the number that was shown again
      When the property "decimals" of "kg" is set to 1
      Then the property "value" of "kg" is 5
      And the property "displayText" of "kg" is "5.0 kg"

    Scenario Outline: A script may call textFromValue: <number> with <places> places in <locale> is "<text>"
      When the method "say" of "s" is called with <number>, <places>, "<locale>"
      Then the property "said" of "s" is "<text>"

      Examples:
        | number   | places | locale | text     |
        | 1234.567 | 2      | en_US  | 1,234.57 |
        | 1234.567 | 2      | de_DE  | 1.234,57 |
        | 1.5      | 0      | en_US  | 2        |
        | -0.5     | 3      | en_US  | -0.500   |

    @api-QtQuick.Controls.DoubleSpinBox.valueFromText
    Scenario Outline: A script may call valueFromText: "<text>" in <locale> is <number>
      When the method "read" of "s" is called with "<text>", "<locale>"
      Then the property "said" of "s" is "<number>"

      Examples:
        | text     | locale | number |
        | 1,234.5  | en_US  | 1234.5 |
        | 1.234,5  | de_DE  | 1234.5 |
        | -2.25    | en_US  | -2.25  |

    @api-QtQuick.Controls.DoubleSpinBox.validator
    Scenario: The validator takes numbers of the range with the decimals
      Then the property "validator.bottom" of "s" is 0
      And the property "validator.top" of "s" is 99.99
      And the property "validator.decimals" of "s" is 2
      When the property "decimals" of "s" is set to 1
      And the property "from" of "s" is set to -10
      Then the property "validator.decimals" of "s" is 1
      And the property "validator.bottom" of "s" is -10
      And the property "validator.top" of "s" is 100

  @qt-parity
  Rule: The arrow keys step the value

    Scenario: Up is a step up by the user
      When the user presses "Tab"
      And the user presses "Up"
      Then the property "value" of "s" is 1
      And the property "log" of "s" is "v1 m1 "

    Scenario: Down at the beginning does nothing
      When the user presses "Tab"
      And the user presses "Down"
      Then the property "value" of "s" is 0
      And the property "log" of "s" is ""

    Scenario: With wrap Down at the beginning goes to the end
      When the property "wrap" of "s" is set to true
      And the user presses "Tab"
      And the user presses "Down"
      Then the property "value" of "s" is 99.99

    Scenario Outline: <key> is not a key of a double spin box
      When the property "value" of "s" is set to 5
      And the user presses "Tab"
      And the user presses "<key>"
      Then the property "value" of "s" is 5

      Examples:
        | key    |
        | PgUp   |
        | Home   |

  @qt-parity
  Rule: A click on a button is one step

    Scenario: The right button steps up
      When the user touches 140, 40
      Then the property "value" of "s" is 1
      And the property "log" of "s" is "v1 m1 "

    Scenario: The left button steps down
      When the property "stepSize" of "s" is set to 0.5
      And the property "value" of "s" is set to 2
      And the user touches 40, 40
      Then the property "value" of "s" is 1.5

  @qt-parity
  Rule: In an editable box the number is typed

    Background:
      When the property "editable" of "s" is set to true

    @api-QtQuick.Controls.DoubleSpinBox.editable
    Scenario: What is typed is shown and is not the value until Return
      When the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user types "4.56"
      Then the property "displayText" of "s" is "4.56"
      And the property "value" of "s" is 0
      And the property "log" of "s" is ""
      When the user presses "Return"
      Then the property "value" of "s" is 4.56
      And the property "displayText" of "s" is "4.56"
      And the property "log" of "s" is "v4.56 m4.56 "

    Scenario: Leaving the box with the Tab key makes what was typed the value
      When the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user types "7.5"
      And the user presses "Tab"
      Then the property "value" of "s" is 7.5
      And the property "displayText" of "s" is "7.50"

    Scenario: A place past the decimals is refused
      When the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user types "1.234"
      Then the property "displayText" of "s" is "1.23"

    Scenario: A sign that no number of the range has is refused
      When the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user types "-1"
      Then the property "displayText" of "s" is "1"

    Scenario: A number above the range becomes its end
      When the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user types "150"
      And the user presses "Return"
      Then the property "value" of "s" is 99.99
      And the property "displayText" of "s" is "99.99"
      And the property "log" of "s" is "v99.99 m99.99 "

    Scenario: An empty number is nought
      When the property "value" of "s" is set to 7
      And the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user presses "Backspace"
      And the user presses "Return"
      Then the property "value" of "s" is 0
      And the property "displayText" of "s" is "0.00"

    Scenario: A step by a key throws away what was typed
      When the user touches 90, 40
      And the user presses "Ctrl+A"
      And the user types "42"
      And the user presses "Up"
      Then the property "value" of "s" is 1
      And the property "displayText" of "s" is "1.00"

    Scenario: The number is typed as the locale of the box writes it
      When the user touches 90, 200
      And the user presses "Ctrl+A"
      And the user types "3,75"
      And the user presses "Return"
      Then the property "value" of "de" is 3.75
      And the property "displayText" of "de" is "3,75"

    Scenario: The author's valueFromText makes the value
      When the user touches 90, 120
      And the user presses "Ctrl+A"
      And the user types "3.5"
      And the user presses "Return"
      Then the property "value" of "kg" is 7
      And the property "displayText" of "kg" is "7.00 kg"

  # The oracle's input channel has no wheel; QtTest's mouseWheel on qml6
  # shows the same.
  @ljos-only
  Rule: The wheel steps a box that asks for it

    Background:
      When the property "value" of "s" is set to 5
      And the property "log" of "s" is set to ""

    @api-QtQuick.Controls.Control.wheelEnabled
    Scenario: A box does not listen to the wheel unless told to
      When the user turns the wheel 1 notch up at 90, 40
      Then the property "value" of "s" is 5

    Scenario: A notch up is a step up by the user
      When the property "wheelEnabled" of "s" is set to true
      And the user turns the wheel 1 notch up at 90, 40
      Then the property "value" of "s" is 6
      And the property "log" of "s" is "v6 m6 "

    Scenario: Notches down are steps down, taken at once
      When the property "wheelEnabled" of "s" is set to true
      And the user turns the wheel 2 notches down at 90, 40
      Then the property "value" of "s" is 3
      And the property "log" of "s" is "v3 m3 "

    Scenario: A part of a notch is that part of a step
      When the property "wheelEnabled" of "s" is set to true
      And the user turns the wheel 0.25 notches up at 90, 40
      Then the property "value" of "s" is 5.25
      And the property "log" of "s" is "v5.25 m5.25 "

    Scenario: The wheel stops at the end of the range
      When the property "wheelEnabled" of "s" is set to true
      And the property "value" of "s" is set to 99
      And the user turns the wheel 3 notches up at 90, 40
      Then the property "value" of "s" is 99.99

  # Qt 6.11 gives a double spin box no accessible role (0); Ljós makes it
  # the spin button a SpinBox is, so that a screen reader can step it.
  @ljos-only @a11y
  Rule: A screen reader is told the number and may change it

    Background:
      When the property "Accessible.name" of "s" is set to "Price"

    Scenario: A double spin box is a spin button
      Then the object "s" is exposed as a "spinbutton" named "Price"

    Scenario: An increase is a step by the user
      When an assistive technology asks the "spinbutton" named "Price" to increase
      Then the property "value" of "s" is 1
      And the property "log" of "s" is "v1 m1 "

    Scenario Outline: It takes a value and keeps it to its range and decimals
      When an assistive technology asks the "spinbutton" named "Price" to take the value <asked>
      Then the property "value" of "s" is <kept>

      Examples:
        | asked | kept  |
        | 2.345 | 2.35  |
        | 500   | 99.99 |
        | -3    | 0     |
