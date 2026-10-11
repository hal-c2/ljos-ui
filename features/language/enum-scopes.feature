# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-enumerations.html
#   https://doc.qt.io/qt-6/qtqml-syntax-imports.html#importing-into-a-qualified-local-namespace
#   https://doc.qt.io/qt-6/qml-font.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html#policy-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where a script reads an enumeration from
  A key is read from the type that has it, wherever the script is: the
  type need not be the one the script is in, and it may be named through
  the namespace an import gave its module.

  Rule: A key is read from any type the file imports

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Rectangle {
          objectName: "root"
          property int centre: Text.AlignHCenter
          property int bold: Font.Bold
          property int capitals: Font.AllUppercase
          property int shown: ScrollBar.AlwaysOn
          property int scoped: ScrollBar.Policy.AlwaysOff
          property int closes: Popup.CloseOnEscape | Popup.CloseOnPressOutside
          property string nowhere: typeof Text.Nowhere + " " + typeof Font.Nowhere
          Text { objectName: "label"; font.weight: Font.DemiBold }
        }
        """

    Scenario Outline: The key read into <property> is <number>
      Then the property "<property>" of "root" is <number>

      Examples:
        | property | number |
        | centre   | 4      |
        | bold     | 700    |
        | capitals | 1      |
        | shown    | 2      |
        | scoped   | 1      |
        | closes   | 17     |

    Scenario: A key of the font's weights is a weight
      Then the property "font.weight" of "label" is 600

    Scenario: A name that is no key is undefined
      Then the property "nowhere" of "root" is "undefined undefined"
      And the log has no warnings

  Rule: A key is read through the namespace of an import

    Background:
      Given the QML:
        """qml
        import QtQuick as Q
        import QtQuick.Controls as C
        import QtQuick.Templates as T
        Q.Item {
          objectName: "root"
          property int hidden: T.ScrollBar.AlwaysOff
          property int scoped: T.ScrollBar.Policy.AlwaysOn
          property int closes: T.Popup.CloseOnEscape
          property int centre: Q.Text.AlignHCenter
          property int corner: Q.Item.BottomRight
          property int bold: Q.Font.Bold
          property int middle: Q.Qt.AlignVCenter
          property string nowhere: typeof T.ScrollBar.Nowhere + " " + typeof T.Nowhere
          Q.Text { objectName: "label"; horizontalAlignment: Q.Text.AlignRight }
          C.ScrollBar { objectName: "bar"; policy: T.ScrollBar.AlwaysOn }
        }
        """

    Scenario Outline: The key read into <property> is <number>
      Then the property "<property>" of "root" is <number>

      Examples:
        | property | number |
        | hidden   | 1      |
        | scoped   | 2      |
        | closes   | 16     |
        | centre   | 4      |
        | corner   | 8      |
        | bold     | 700    |
        | middle   | 128    |

    Scenario: An enumeration property is bound to a key of a namespace
      Then the property "horizontalAlignment" of "label" is 2
      And the property "policy" of "bar" is 2

    Scenario: A name the namespace does not have is undefined
      Then the property "nowhere" of "root" is "undefined undefined"
      And the log has no warnings

  Rule: A function and a handler read keys as a binding does

    Scenario: Keys read where a script runs later
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtQuick.Templates as T
        Item {
          id: root; objectName: "root"
          property var asked
          property var handled
          function look() {
            asked = [T.ScrollBar.AlwaysOff, Font.Bold, Text.AlignHCenter, ScrollBar.AsNeeded]
          }
          onAskedChanged: handled = [T.ScrollBar.AlwaysOn, Font.Light, Text.AlignRight]
        }
        """
      When the method "look" of "root" is called
      Then the property "asked" of "root" is [1, 700, 4, 0]
      And the property "handled" of "root" is [2, 300, 2]
