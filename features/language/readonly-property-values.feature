# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#read-only-properties
# Verified on Qt 6.12.0 (qml6, offscreen).
# Found in review of a corpus stand-in that declared `readonly property date timestamp`.

@qt-parity
Feature: A read-only property is given its value where it is declared
  A read-only property must be given a value or a binding when it is
  declared: nothing can give it one later. One declared without either
  stops the file loading, and what is said is at the declaration. A binding
  of the property written apart from its declaration is no initializer.

  Rule: A read-only property declared with no value stops the file loading

    Scenario Outline: <written> is refused
      Given the file "Bad.qml":
        """qml
        import QtQuick
        Item {
          <written>
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Bad.qml" }
        }
        """
      Then the property "status" of "loader" is 3
      And the log has a warning matching "Bad\.qml:3:11: Read-only properties require an initializer\.$"

      Examples:
        | written                          |
        | readonly property int count      |
        | readonly property var value      |
        | readonly property date when      |
        | readonly property Item item      |
        | readonly property int count; count: 3 |

  Rule: A value or an object at the declaration is its initializer

    Scenario: Read-only properties given values where they are declared load
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property int count: 3
          readonly property var nothing: undefined
          readonly property Item item: Item { objectName: "inner" }
          readonly property bool made: item !== null
        }
        """
      Then the property "count" of "root" is 3
      And the property "made" of "root" is true
      And the log has no warnings
