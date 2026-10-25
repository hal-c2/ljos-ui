# Sources:
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), bucket value:font:step-member-write(literal).

@qt-parity
Feature: A font's pixel size is a whole number more than zero
  A font value keeps its pixel size when it is given zero or less: there is no text of that
  size, so the write is ignored. A fraction is rounded to the nearest whole size. A weight
  is held between 1 and 1000. A script that gives either what is no whole number, such as
  NaN, null or a text of a fraction, is thrown an error and the font is left as it was.

  Scenario Outline: A pixel size of <size> is ignored
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({pixelSize: 10})
        property int size: f.pixelSize
        function shrink() { f.pixelSize = <size> }
      }
      """
    When the method "shrink" of "root" is called
    Then the property "size" of "root" is 10

    Examples:
      | size      |
      | 0         |
      | -3        |
      | 0.4       |
      | Infinity  |
      | undefined |

  # Found by ljos-fuzz (bindings), buckets throws:Error/none:step-member-write(mod) and (parseInt).
  Scenario Outline: A script that gives the <member> of a font NaN is thrown an error
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({pixelSize: 10, weight: 400})
        property string seen: ""
        function give() {
          try { f.<member> = NaN } catch (e) { seen = e.message }
        }
      }
      """
    When the method "give" of "root" is called
    Then the property "seen" of "root" is "Cannot assign double to int"

    Examples:
      | member    |
      | pixelSize |
      | weight    |

  Scenario: An int property a script gives NaN is zero
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 5
        function give() { n = NaN }
      }
      """
    When the method "give" of "root" is called
    Then the property "n" of "root" is 0

  Scenario: A font Qt.font makes has the pixel size it was given
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real size: Qt.font({pixelSize: 10}).pixelSize
      }
      """
    Then the property "size" of "root" is 10

  # Found by ljos-fuzz (bindings), bucket value:step:rounding.
  Scenario Outline: A pixel size of <size> is rounded to <read>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({pixelSize: 10})
        property int size: f.pixelSize
        function grow() { f.pixelSize = <size> }
      }
      """
    When the method "grow" of "root" is called
    Then the property "size" of "root" is <read>

    Examples:
      | size                  | read       |
      | 10.5                  | 11         |
      | 0.5                   | 1          |
      | Math.sqrt(2147483648) | 46341      |
      | 1e10                  | 1410065408 |
      | " 12"                 | 12         |
      | true                  | 1          |

  Scenario Outline: A script that gives the pixel size of a font <given> is thrown an error
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({pixelSize: 10})
        property string seen: ""
        function give() {
          try { f.pixelSize = <given> } catch (e) { seen = e.message + " " + f.pixelSize }
        }
      }
      """
    When the method "give" of "root" is called
    Then the property "seen" of "root" is "<seen>"

    Examples:
      | given  | seen                                   |
      | "12.6" | Cannot assign QString to int 10        |
      | "0x10" | Cannot assign QString to int 10        |
      | null   | Cannot assign std::nullptr_t to int 10 |
      | [13]   | Cannot assign QJSValue to int 10       |

  Scenario Outline: A weight of <given> is <read>
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({pixelSize: 10, weight: 400})
        property int weight: f.weight
        function give() { f.weight = <given> }
      }
      """
    When the method "give" of "root" is called
    Then the property "weight" of "root" is <read>

    Examples:
      | given | read |
      | 450.5 | 451  |
      | 0     | 1    |
      | 1001  | 1000 |
      | "500" | 500  |
