# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#containsMouse-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# A build without a table of Qt's types reads QML against Ljós's own types, which may lack a member
# Qt has: there a name its table lacks is a compile warning, and the file loads (`@qt-table`).

@qt-parity
Feature: A name an object's type does not have
  A binding, a handler or a group member names something of the object's type. A name the type
  does not have, a misspelt property most often, stops the file: making an object of it fails,
  and says where the name is.

  Rule: A name the type does not have stops the file

    @qt-table
    Scenario Outline: <what> stops the file
      Given the file "Typo.qml":
        """qml
        import QtQuick
        Item {
          <binding>
        }
        """
      Then loading "Typo.qml" is rejected

      Examples:
        | what                            | binding                       |
        | A misspelt property             | widht: 100                    |
        | A handler of no signal          | onWidhtChanged: {}            |
        | A misspelt member of a group    | anchors.lefft: parent.left    |
        | A misspelt property of a Rectangle's group | Rectangle { border.widht: 2 } |
        | A misspelt property of a child  | Rectangle { colr: "red" }     |

    @qt-table
    Scenario: A Loader of the file says where the name is
      Given the file "Typo.qml":
        """qml
        import QtQuick
        Item {
          widht: 100
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Typo.qml" }
        }
        """
      Then the property "status" of "loader" is 3
      And the log has a warning matching "Typo\.qml:3:\d+: .*widht"

  Rule: What the file and its types declare is a member

    Scenario: A property and a signal the object declares take a binding and a handler
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int size
          signal poked
          size: 3
          onPoked: size = 4
          onSizeChanged: {}
          Component.onCompleted: poked()
        }
        """
      Then the property "size" of "root" is 4

    Scenario: A property a file declares is a member of the objects made of it
      Given the file "Box.qml":
        """qml
        import QtQuick
        Rectangle {
          property int size
          property alias label: text.text
          Text { id: text }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Box { objectName: "box"; size: 2; label: "two"; color: "red"; onSizeChanged: {} }
        }
        """
      Then the property "size" of "box" is 2
      And the property "label" of "box" is "two"

    Scenario: The change of a property has a handler whatever its signal is called
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int heard: 0
          MouseArea {
            objectName: "area"
            onContainsMouseChanged: root.heard++
            onHoveredChanged: root.heard++
          }
        }
        """
      Then the property "heard" of "root" is 0
      And the property "containsMouse" of "area" is false
