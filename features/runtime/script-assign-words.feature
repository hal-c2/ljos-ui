# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-sequencetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a script is told when a property refuses its value
  An assignment a property cannot take throws an Error that names both
  sides as Qt's engine holds them: the C++ class of the property's type,
  and what the value is to it. A whole number is an int, an object is a
  QObject, and a function is a JavaScript function, which no property but
  a var takes.

  Rule: An object property is told the C++ class of what it was given

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said
          property Item it
          property Rectangle rect
          property QtObject qo
          property int i: 1; property url u
          property var code: ({
            itInt: function() { it = 5 },
            itReal: function() { it = 2.5 },
            itText: function() { it = "x" },
            itHuge: function() { it = 3000000000 },
            rectInt: function() { rect = -4 },
            rectRoot: function() { rect = root },
            qoInt: function() { qo = 5 },
            intObject: function() { i = ({}) },
            urlInt: function() { u = 7 }
          })
          function run(name) { try { code[name](); said = "ok" } catch (e) { said = String(e) } }
        }
        """

    Scenario Outline: <name> is refused in Qt's words
      When the method "run" of "root" is called with "<name>"
      Then the property "said" of "root" is "Error: Cannot assign <from> to <to>"

      Examples:
        | name      | from     | to               |
        | itInt     | int      | QQuickItem*      |
        | itReal    | double   | QQuickItem*      |
        | itText    | QString  | QQuickItem*      |
        | itHuge    | double   | QQuickItem*      |
        | rectInt   | int      | QQuickRectangle* |
        | rectRoot  | QObject* | QQuickRectangle* |
        | qoInt     | int      | QObject*         |
        | intObject | QJSValue | int              |
        | urlInt    | int      | QUrl             |

  Rule: A function goes only to a var, and undefined to no list

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said
          property Item it; property int i: 1; property bool b: true; property string s: "s"
          property list<int> li; property list<real> lr; property list<string> ls
          property list<var> lv; property list<color> lc; property list<Item> lit
          property var v
          property var code: ({
            itFunction: function() { it = function() {} },
            intFunction: function() { i = function() {} },
            boolFunction: function() { b = function() {} },
            stringFunction: function() { s = function() {} },
            intsFunction: function() { li = function() {} },
            intsUndefined: function() { li = undefined },
            realsUndefined: function() { lr = undefined },
            stringsUndefined: function() { ls = undefined },
            varsUndefined: function() { lv = undefined },
            colorsUndefined: function() { lc = undefined },
            itemsUndefined: function() { lit = undefined }
          })
          function run(name) { try { code[name](); said = "ok" } catch (e) { said = String(e) } }
          function keep() { v = function() { return 2 }; said = typeof v }
        }
        """

    Scenario Outline: <name> is refused in Qt's words
      When the method "run" of "root" is called with "<name>"
      Then the property "said" of "root" is "Error: Cannot assign <from> to <to>"

      Examples:
        | name             | from                | to                           |
        | itFunction       | JavaScript function | QQuickItem*                  |
        | intFunction      | JavaScript function | int                          |
        | boolFunction     | JavaScript function | bool                         |
        | stringFunction   | JavaScript function | QString                      |
        | intsFunction     | JavaScript function | QList<int>                   |
        | intsUndefined    | [undefined]         | QList<int>                   |
        | realsUndefined   | [undefined]         | QList<double>                |
        | stringsUndefined | [undefined]         | QStringList                  |
        | varsUndefined    | [undefined]         | QVariantList                 |
        | colorsUndefined  | [undefined]         | QList<QColor>                |
        | itemsUndefined   | [undefined]         | QQmlListProperty<QQuickItem> |

    Scenario: A refused function leaves the property as it was
      When the method "run" of "root" is called with "boolFunction"
      And the method "run" of "root" is called with "stringFunction"
      Then the property "b" of "root" is true
      And the property "s" of "root" is "s"

    Scenario: A var takes a function
      When the method "keep" of "root" is called
      Then the property "said" of "root" is "function"
