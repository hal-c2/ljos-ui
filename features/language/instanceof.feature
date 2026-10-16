# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-objecttypes.html
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html#inline-components
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A QML type on the right of instanceof
  A type's name is a value in a script. `object instanceof Type` says whether the object is of
  that type or of one derived from it, for the types of a module, a file and an inline component.

  Background:
    Given the file "Swatch.qml":
      """qml
      import QtQuick
      Rectangle { }
      """
    And the file "Plain.qml":
      """qml
      import QtQuick
      Item { }
      """

  Scenario Outline: <object> instanceof <type> is <is>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root
        objectName: "root"
        component Tile: Rectangle { }
        Rectangle { id: rect }
        Swatch { id: swatch }
        Tile { id: tile }
        Button { id: button }
        Timer { id: timer }
        Menu { id: menu; MenuItem { text: "a" } MenuSeparator { } }
        property bool is
        Component.onCompleted: is = <object> instanceof <type>
      }
      """
    Then the property "is" of "root" is <is>

    Examples: a type of a module, and its bases
      | object         | type          | is    |
      | rect           | Rectangle     | true  |
      | rect           | Item          | true  |
      | rect           | QtObject      | true  |
      | rect           | Text          | false |
      | timer          | Timer         | true  |
      | timer          | QtObject      | true  |
      | timer          | Item          | false |
      | root           | Item          | true  |

    Examples: a type of Qt Quick Controls
      | object         | type          | is    |
      | button         | Button        | true  |
      | button         | Item          | true  |
      | button         | Rectangle     | false |
      | menu.itemAt(0) | MenuItem      | true  |
      | menu.itemAt(1) | MenuSeparator | true  |
      | menu.itemAt(0) | MenuSeparator | false |

    Examples: a type a file is, and an inline component
      | object         | type          | is    |
      | swatch         | Swatch        | true  |
      | swatch         | Rectangle     | true  |
      | swatch         | Plain         | false |
      | swatch         | Tile          | false |
      | tile           | Tile          | true  |
      | tile           | Rectangle     | true  |
      | tile           | Swatch        | false |

  Scenario Outline: <value> is no instance of a QML type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool is: true
        Component.onCompleted: is = <value> instanceof Item
      }
      """
    Then the property "is" of "root" is false

    Examples:
      | value      |
      | 5          |
      | null       |
      | "text"     |
      | ({})       |
      | Item       |
      | new Date() |

  Scenario: A type a script imports from a module is a type to instanceof
    Given the file "kinds.js":
      """
      .import QtQuick as Q
      function isItem(o) { return o instanceof Q.Item }
      """
    And the QML:
      """qml
      import QtQuick
      import "kinds.js" as Kinds
      Item {
        objectName: "root"
        Rectangle { id: rect }
        Timer { id: timer }
        property bool rectIsItem: Kinds.isItem(rect)
        property bool timerIsItem: Kinds.isItem(timer)
      }
      """
    Then the property "rectIsItem" of "root" is true
    And the property "timerIsItem" of "root" is false
