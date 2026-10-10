# Sources:
#   https://doc.qt.io/qt-6/qml-string.html
#   https://doc.qt.io/qt-6/qstring.html#arg
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: String.arg fills numbered place markers
  A translated sentence puts its values in a different order in each language, so the values are
  not glued on with "+": the sentence carries markers "%1" to "%99" and arg() fills them in.

  Each call fills every marker that has the lowest number left in the string.

  Scenario Outline: The lowest number left is filled, wherever it stands
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: <expression>
      }
      """
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | expression                    | shown     |
      | "%1 of %2".arg(3).arg("five") | 3 of five |
      | "%2 then %1".arg("a").arg("b") | b then a |
      | "%1 and %1".arg("x")          | x and x   |
      | "%3 %7 %3".arg("a").arg("b")  | a b a     |
      | "%0 %1".arg("a")              | a %1      |
      | "%01 %1".arg("a")             | a a       |

  Scenario Outline: A marker has at most two digits
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: <expression>
      }
      """
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | expression        | shown |
      | "%100".arg("a")   | a0    |
      | "%123".arg("a")   | a3    |
      | "%99 %9".arg("a") | %99 a |

  Scenario: A value that looks like a marker is filled by the next call
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: "%1%2".arg("%2").arg("b")
      }
      """
    Then the property "shown" of "root" is "bb"

  Scenario: A percent sign that is not a marker stays as written
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: "100%% %1% %x".arg(5)
      }
      """
    Then the property "shown" of "root" is "100%% 5% %x"

  Scenario Outline: A value is written as a string would show it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: "%1".arg(<value>)
      }
      """
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | value      | shown       |
      | "text"     | text        |
      | 42         | 42          |
      | -7         | -7          |
      | 1234.5     | 1234.5      |
      | 0.1 + 0.2  | 0.3         |
      | true       | 1           |
      | false      | 0           |
      | null       | null        |
      | undefined  | undefined   |
      | [1, 2]     | 1,2         |

  Scenario Outline: A number keeps six significant digits
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string shown: "%1".arg(<value>)
      }
      """
    Then the property "shown" of "root" is "<shown>"

    Examples:
      | value        | shown        |
      | 2147483647   | 2147483647   |
      | 2147483648   | 2.14748e+09  |
      | 1e10         | 1e+10        |
      | -1234567.891 | -1.23457e+06 |
      | 0.00001234   | 1.234e-05    |
      | 0 / 0        | nan          |
      | 1 / 0        | inf          |

  Rule: "%L1" writes a number the way the locale does

    Scenario Outline: A number at an "L" marker gets the locale's digit grouping
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: <expression>
        }
        """
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | expression              | shown          |
        | "%L1".arg(1234567)      | 1,234,567      |
        | "%L1 %1".arg(1234.5)    | 1,234.5 1234.5 |
        | "%L1".arg(-5000)        | -5,000         |
        | "%L1".arg(999)          | 999            |
        | "%L2 of %1".arg("a").arg(1000) | 1,000 of a |

    Scenario: A string at an "L" marker is left alone
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: "%L1".arg("1234")
        }
        """
      Then the property "shown" of "root" is "1234"

  Rule: Mistakes are reported, and the string survives

    Scenario: A string with no marker is returned unchanged, with a warning
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: "plain %l1".arg("a")
        }
        """
      Then the property "shown" of "root" is "plain %l1"
      And the log has a warning matching "Argument missing"

    Scenario Outline: arg() takes exactly one value
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: "kept"
          Component.onCompleted: shown = <expression>
        }
        """
      Then the property "shown" of "root" is "kept"
      And the log has a warning matching "String.arg\(\): Invalid arguments"

      Examples:
        | expression          |
        | "%1".arg()          |
        | "%1 %2".arg(1, 2)   |
