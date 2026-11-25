# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
# Verified on Qt 6.12.0 (qml6, offscreen).
# What one object declares lives in two sets of names: its properties (aliases among them), and its
# methods (its signals, its functions, and the change signal `<name>Changed` of each of its
# properties). A name may be in both sets once, but not twice in one.

@qt-parity
Feature: An object declares each of its members once
  An object that declares two properties, two signals or two functions of
  one name, or a signal or function that has the name of one of its
  properties' change signals, does not load, and whatever makes an object of
  its file is told which declaration is the second.

  Rule: A name declared twice in one set stops the file from loading

    Scenario Outline: <first> and then <second> does not load
      Given the file "Twice.qml":
        """qml
        import QtQuick
        Item {
          Item { id: other; property int sample }
          <first>
          <second>
        }
        """
      Then loading "Twice.qml" is rejected

      Examples:
        | first                          | second                         |
        | function f() { return 1 }      | function f() { return 2 }      |
        | function f(a) { }              | function f() { }               |
        | property int p: 1              | property int p: 2              |
        | readonly property int p        | property string p              |
        | property int a                 | property alias a: other.sample |
        | property alias a: other.sample | property int a                 |
        | property alias a: other.sample | property alias a: other.sample |
        | signal s()                     | signal s(int a)                |
        | signal f()                     | function f() { }               |
        | function f() { }               | signal f()                     |
        | property int p                 | signal pChanged()              |
        | signal pChanged()              | property int p                 |
        | property int p                 | function pChanged() { }        |
        | function pChanged() { }        | property int p                 |
        | property alias a: other.sample | signal aChanged()              |
        | property alias a: other.sample | function aChanged() { }        |

  Rule: A property and a method may share a name

    Scenario Outline: <first> and <second> load
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          <first>
          <second>
          property bool loaded: true
        }
        """
      Then the property "loaded" of "root" is true

      Examples:
        | first            | second                |
        | property int q   | function q() { }      |
        | function q() { } | property int q        |
        | property int q   | signal q()            |
        | signal s()       | signal sChanged()     |
        | property int p   | property int pChanged |
        | property int p   | onPChanged: { }       |

    Scenario: Two objects each declare a function of one name
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          function f() { return 1 }
          Item { objectName: "inner"; function f() { return 2 } }
          property int got: f()
        }
        """
      Then the property "got" of "root" is 1

  Rule: What makes an object of the file is told where the second declaration is

    Scenario Outline: A Loader of a file with <what> says where it is
      Given the file "Twice.qml":
        """qml
        import QtQuick
        Item {
          <first>
          <second>
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Twice.qml" }
        }
        """
      Then the property "status" of "loader" is 3
      And the log has a warning matching "Twice\.qml:<at>: <message>"

      Examples:
        | what                  | first             | second           | at   | message                 |
        | two functions         | function f() { }  | function f() { } | 4:20 | Duplicate method name   |
        | two properties        | property int p    | property real p  | 4:25 | Duplicate property name |
        | two signals           | signal s()        | signal s()       | 4:18 | Duplicate signal name   |
        | a signal and function | function f() { }  | signal f()       | 3:20 | Duplicate method name   |
        | a change signal       | signal pChanged() | property int p   | 3:18 | Duplicate signal name   |
