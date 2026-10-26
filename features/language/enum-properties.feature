# Sources:
#   https://doc.qt.io/qt-6/qml-enumeration.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#transformOrigin-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html#buttonRole-attached-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Properties that hold an enumeration
  A property such as Item.transformOrigin takes the keys of one enumeration.
  Written as a plain value in the source it has to name a key; a number
  there is refused, though the same number given by a script is taken.

  Rule: A plain value in the source names a key or the file does not load

    Scenario: A string that is a key of the enumeration
      Given the QML:
        """qml
        import QtQuick
        Item {
          Item { objectName: "corner"; transformOrigin: "TopRight" }
          Text { objectName: "label"; horizontalAlignment: "AlignRight" }
          Flow { objectName: "flow"; flow: "TopToBottom" }
        }
        """
      Then the property "transformOrigin" of "corner" is 2
      And the property "horizontalAlignment" of "label" is 2
      And the property "flow" of "flow" is 1

    Scenario: The value reads as a number
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: item; objectName: "item"
          transformOrigin: "Left"
          property string kind: typeof item.transformOrigin
          property bool same: item.transformOrigin === Item.Left
        }
        """
      Then the property "kind" of "item" is "number"
      And the property "same" of "item" is true

    Scenario Outline: <value> is not a key
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Item {
          Item { transformOrigin: <value> }
        }
        """
      Then loading "Wrong.qml" is rejected

      Examples:
        | value          |
        | "Nowhere"      |
        | "topright"     |
        | "Item.Top"     |
        | "AlignRight"   |
        | ""             |
        | 2              |
        | -1             |
        | 2.5            |
        | true           |
        | null           |

    Scenario Outline: A number for <property> of <type> is refused too
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          <type> { <property>: 1 }
        }
        """
      Then loading "Wrong.qml" is rejected

      Examples:
        | type      | property            |
        | Text      | horizontalAlignment |
        | Text      | elide               |
        | Image     | fillMode            |
        | Flow      | flow                |
        | Row       | layoutDirection     |
        | MouseArea | acceptedButtons     |
        | Item      | Layout.alignment    |

    Scenario: A property that holds flags takes several keys in one string
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Layouts
        Item {
          MouseArea { objectName: "both"; acceptedButtons: "LeftButton|RightButton" }
          MouseArea { objectName: "spaced"; acceptedButtons: "LeftButton | MiddleButton" }
          RowLayout {
            Item {
              objectName: "cell"
              Layout.alignment: "AlignRight|AlignTop"
              property int at: Layout.alignment
            }
          }
        }
        """
      Then the property "acceptedButtons" of "both" is 3
      And the property "acceptedButtons" of "spaced" is 5
      And the property "at" of "cell" is 34

    Scenario Outline: The flags <value> do not all name a key
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Item {
          MouseArea { acceptedButtons: <value> }
        }
        """
      Then loading "Wrong.qml" is rejected

      Examples:
        | value                       |
        | "LeftButton\|Nowhere"       |
        | "LeftButton\|"              |
        | "LeftButton\|\|RightButton" |
        | "LeftButton,RightButton"    |
        | "LeftButton RightButton"    |

    Scenario: A key of another type and a calculation are scripts, and are taken
      Given the QML:
        """qml
        import QtQuick
        Item {
          Item { objectName: "other"; transformOrigin: Text.AlignRight }
          Item { objectName: "sum"; transformOrigin: Item.Top + 2 }
          Item { objectName: "signed"; transformOrigin: +5 }
          Item { objectName: "nothing"; transformOrigin: undefined }
        }
        """
      Then the property "transformOrigin" of "other" is 2
      And the property "transformOrigin" of "sum" is 3
      And the property "transformOrigin" of "signed" is 5
      And the property "transformOrigin" of "nothing" is 4

  Rule: A binding gives a number or the name of a key

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property var given: "Bottom"
          property int changes: 0
          Item {
            objectName: "item"
            transformOrigin: root.given
            onTransformOriginChanged: root.changes++
          }
        }
        """

    Scenario: The name of a key is its number
      Then the property "transformOrigin" of "item" is 7
      And the log has no warnings

    Scenario Outline: The binding follows <given>
      When the property "given" of "root" is set to <given>
      Then the property "transformOrigin" of "item" is <origin>

      Examples:
        | given      | origin |
        | "TopLeft"  | 0      |
        | 5          | 5      |
        | 2.7        | 3      |
        | 2.5        | 3      |
        | true       | 1      |
        | false      | 0      |

    Scenario Outline: A binding that gives <given> leaves the value as it was
      When the property "given" of "root" is set to <given>
      Then the property "transformOrigin" of "item" is 7
      And the property "changes" of "root" is 1
      And the log has a warning matching "Scenario.qml:8:13: Unable to assign <what> to \S*TransformOrigin$"

      Examples:
        | given     | what    |
        | "Nowhere" | QString |
        | "bottom"  | QString |
        | ""        | QString |
        | "3"       | QString |
        | null      | null    |

  Rule: An assignment takes the same values and refuses the rest

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int changes: 0
          Item { objectName: "item"; onTransformOriginChanged: root.changes++ }
        }
        """

    Scenario Outline: Assigning <given>
      When the property "transformOrigin" of "item" is set to <given>
      Then the property "transformOrigin" of "item" is <origin>
      And the property "changes" of "root" is 1

      Examples:
        | given         | origin |
        | "BottomRight" | 8      |
        | 6             | 6      |
        | 2.7           | 3      |
        | true          | 1      |
        | false         | 0      |

    Scenario Outline: Assigning <given> is refused
      Then setting the property "transformOrigin" of "item" to <given> is rejected
      And the property "transformOrigin" of "item" is 4
      And the property "changes" of "root" is 0

      Examples:
        | given       |
        | "Nowhere"   |
        | "bottom"    |
        | ""          |
        | "3"         |
        | null        |
        | [1, 2]      |

  Rule: A number out of an enumeration's range is read as unsigned, unless a key is negative

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string given: "0"
          Text { objectName: "text"; wrapMode: Number(root.given) }
          Text {
            objectName: "plain"
            function bind() { wrapMode = Qt.binding(function () { return Number("x") }) }
          }
        }
        """

    # Found by ljos-fuzz (bindings), bucket value:step:int-range.
    Scenario Outline: A binding that gives <given> reads <read>
      When the property "given" of "root" is set to <given>
      Then the property "wrapMode" of "text" is <read>

      Examples:
        | given      | read       |
        | "-1"       | 4294967295 |
        | "-2"       | 4294967294 |
        | "1e21"     | 4294967295 |
        | "Infinity" | 4294967295 |

    Scenario: A binding that gives NaN leaves the value as it was
      When the property "given" of "root" is set to "NaN"
      Then the property "wrapMode" of "text" is 0
      And the log has a warning matching "Scenario.qml:5:\d+: Unable to assign double to \S*WrapMode$"

    # Found by ljos-fuzz (bindings), bucket
    # warning:assign(qt-only):Unable_to_assign_double_to_QQuickText::WrapMode.
    @backlog
    Scenario: A binding Qt.binding made that gives NaN at once leaves the value as it was
      When the method "bind" of "plain" is called
      Then the property "wrapMode" of "plain" is 0
      And the log has a warning matching "Unable to assign double to \S*WrapMode$"

  Rule: An enumeration with a negative key reads signed

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"
          property string given: "0"
          property int key: DialogButtonBox.InvalidRole
          Button { objectName: "spare"; property var role: DialogButtonBox.buttonRole }
          Button {
            objectName: "given"; property var role: DialogButtonBox.buttonRole
            DialogButtonBox.buttonRole: Number(root.given)
          }
        }
        """

    Scenario: A role nobody gave is the negative key's number
      Then the property "role" of "spare" is -1
      And the property "key" of "root" is -1

    Scenario Outline: A binding that gives <given> reads <read>
      When the property "given" of "root" is set to <given>
      Then the property "role" of "given" is <read>

      Examples:
        | given        | read |
        | "-1"         | -1   |
        | "-2"         | -2   |
        | "1e21"       | -1   |
        | "4294967295" | -1   |
