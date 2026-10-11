# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-enumerations.html
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#acceptedButtons-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#closePolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#alignment-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#horizontalAlignment-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Properties that hold flags
  Some properties hold several keys of an enumeration at once: the buttons
  a MouseArea accepts, how a Layout aligns a cell, when a Popup closes.
  Such a property takes a combination of keys and reports it as their
  sum; a property that holds one key does not.

  Rule: A combination of keys is taken and read back

    Scenario: Keys combined in a binding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtQuick.Layouts
        Item {
          objectName: "root"
          property int policy: popup.closePolicy
          property int cell: inner.Layout.alignment
          MouseArea { objectName: "area"; acceptedButtons: Qt.LeftButton | Qt.RightButton }
          Popup { id: popup; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent }
          RowLayout {
            Item { id: inner; Layout.alignment: Qt.AlignLeft | Qt.AlignVCenter }
          }
        }
        """
      Then the property "acceptedButtons" of "area" is 3
      And the property "policy" of "root" is 18
      And the property "cell" of "root" is 129

    Scenario: A script gives each kind of flags a string of keys
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtQuick.Layouts
        Item {
          id: root; objectName: "root"
          property int policy: popup.closePolicy
          property int cell: inner.Layout.alignment
          function give() {
            popup.closePolicy = "CloseOnEscape|CloseOnReleaseOutside"
            inner.Layout.alignment = "AlignRight | AlignTop"
          }
          Popup { id: popup }
          RowLayout { Item { id: inner } }
        }
        """
      When the method "give" of "root" is called
      Then the property "policy" of "root" is 20
      And the property "cell" of "root" is 34

  Rule: An assignment takes numbers and the names of keys

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int changes: 0
          MouseArea { objectName: "area"; onAcceptedButtonsChanged: root.changes++ }
        }
        """

    Scenario Outline: Assigning <given>
      When the property "acceptedButtons" of "area" is set to <given>
      Then the property "acceptedButtons" of "area" is <buttons>
      And the property "changes" of "root" is 1

      Examples:
        | given                       | buttons |
        | 3                           | 3       |
        | "MiddleButton"              | 4       |
        | "LeftButton\|RightButton"   | 3       |
        | "LeftButton \| MiddleButton" | 5       |
        | "NoButton"                  | 0       |
        | 1000                        | 1000    |
        | -1                          | -1      |
        | 2.7                         | 3       |
        | false                       | 0       |

    Scenario Outline: Assigning <given> is refused
      Then setting the property "acceptedButtons" of "area" to <given> is rejected
      And the property "acceptedButtons" of "area" is 1
      And the property "changes" of "root" is 0

      Examples:
        | given                  |
        | "Nowhere"              |
        | "LeftButton\|Nowhere"  |
        | "leftbutton"           |
        | "LeftButton,RightButton" |
        | "AlignLeft"            |
        | "3"                    |
        | null                   |
        | [1, 2]                 |

  Rule: A binding gives a number or the names of keys

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var given: "LeftButton|RightButton"
          MouseArea { objectName: "area"; acceptedButtons: root.given }
        }
        """

    Scenario: The names of keys are their sum
      Then the property "acceptedButtons" of "area" is 3
      And the log has no warnings

    Scenario Outline: The binding follows <given>
      When the property "given" of "root" is set to <given>
      Then the property "acceptedButtons" of "area" is <buttons>

      Examples:
        | given                        | buttons |
        | "MiddleButton \| LeftButton" | 5       |
        | 6                            | 6       |
        | "NoButton"                   | 0       |

    Scenario Outline: A binding that gives <given> leaves the value as it was
      When the property "given" of "root" is set to <given>
      Then the property "acceptedButtons" of "area" is 3
      And the log has a warning matching "Scenario.qml:5:43: Unable to assign <what> to \S*MouseButton\S*$"

      Examples:
        | given                 | what    |
        | "Nowhere"             | QString |
        | "LeftButton\|Nowhere" | QString |
        | null                  | null    |

  Rule: A property that holds one key takes one key

    Scenario: Two keys in a string are no key of an enumeration
      Given the QML:
        """qml
        import QtQuick
        Item {
          Text { objectName: "label" }
        }
        """
      Then setting the property "horizontalAlignment" of "label" to "AlignRight|AlignHCenter" is rejected
      And the property "horizontalAlignment" of "label" is 1

    Scenario Outline: <property> of <type> written with two keys does not load
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Item {
          <type> { <property>: "<keys>" }
        }
        """
      Then loading "Wrong.qml" is rejected

      Examples:
        | type | property            | keys                      |
        | Text | horizontalAlignment | AlignRight\|AlignHCenter  |
        | Text | elide               | ElideLeft\|ElideRight     |
        | Item | transformOrigin     | Top\|Left                 |
        | Flow | flow                | LeftToRight\|TopToBottom  |
