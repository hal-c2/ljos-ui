# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qtobject.html#objectName-prop
#   https://doc.qt.io/qt-6/qtquick-debugging.html
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A QML object prints as its type, its address and its name
  An object turned into text, by String(), by joining it to a string or by
  the console, gives the name of its type, its address and, when it has
  one, its objectName. A type QML declares is named after the file or the
  component that declares it; an object that declares members of its own
  is of a type made for it, named after its base.

  Background:
    Given the file "Badge.qml":
      """qml
      import QtQuick
      Rectangle { }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int pick: -1
        onPickChanged: console.warn(String([plain, box, bare, timer, own, mine, mineOwn, badge, root][pick]))
        property string run: ""
        onRunChanged: console.warn(root[run]())
        component Mine: Item { }
        Item { id: plain }
        Rectangle { id: box; objectName: "box" }
        QtObject { id: bare }
        Timer { id: timer }
        Item { id: own; property int extra }
        Mine { id: mine }
        Mine { id: mineOwn; property int extra }
        Badge { id: badge; objectName: "badge" }
        function joined() { return "" + box }
        function itself() { return box }
        function listed() { console.warn([plain, box]); return [1, "a", [2, 3], null, undefined] }
        function circular() { var a = [1]; a.push(a); return a }
      }
      """

  Rule: An object prints as its type

    Scenario Outline: <object> prints as <printed>
      When the property "pick" of "root" is set to <pick>
      Then the log has a warning matching "<printed>"

      Examples:
        | pick | object                                         | printed                                           |
        | 0    | an Item                                        | QQuickItem[(]0x[0-9a-f]+[)]$                      |
        | 1    | a Rectangle with an objectName                 | QQuickRectangle[(]0x[0-9a-f]+, .box.[)]$          |
        | 2    | a QtObject                                     | QObject[(]0x[0-9a-f]+[)]$                         |
        | 3    | a Timer                                        | QQmlTimer[(]0x[0-9a-f]+[)]$                       |
        | 4    | an Item with a property of its own             | QQuickItem_QML_[0-9]+[(]0x[0-9a-f]+[)]$           |
        | 5    | an inline component                            | Mine[(]0x[0-9a-f]+[)]$                            |
        | 6    | an inline component with a property of its own | Mine_QML_[0-9]+[(]0x[0-9a-f]+[)]$                 |
        | 7    | a file's type                                  | Badge_QMLTYPE_[0-9]+[(]0x[0-9a-f]+, .badge.[)]$   |
        | 8    | the scenario's root                            | Scenario_QMLTYPE_[0-9]+[(]0x[0-9a-f]+, .root.[)]$ |

    Scenario: An object joined to a string prints the same
      When the property "run" of "root" is set to "joined"
      Then the log has a warning matching "QQuickRectangle[(]0x[0-9a-f]+, .box.[)]$"

  Rule: The console prints what it is given as text

    Scenario: The console prints an object as String() does
      When the property "run" of "root" is set to "itself"
      Then the log has a warning matching "QQuickRectangle[(]0x[0-9a-f]+, .box.[)]$"

    Scenario: The console prints an array in brackets
      When the property "run" of "root" is set to "listed"
      Then the log has a warning matching "^.QQuickItem[(]0x[0-9a-f]+[)],QQuickRectangle[(]0x[0-9a-f]+, .box.[)].$"
      And the log has a warning matching "^.1,a,.2,3.,null,undefined.$"

    Scenario: An array that holds itself is printed once
      When the property "run" of "root" is set to "circular"
      Then the log has a warning matching "^.1,.Circular..$"
