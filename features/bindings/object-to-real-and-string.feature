# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html (real, string)
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An object given to a real or a string property
  A real takes what JavaScript makes a number of: an object asks its valueOf, so a Date is its
  time, an empty array is 0 and an array of one value is that value; an object with no number
  in it, a QML object among them, is NaN. A string refuses an object, though it has a toString,
  and a QML object: a binding says so and leaves the string as it was, and a script that assigns
  one is thrown an error.

  Rule: A real is the number JavaScript makes of the object

    Scenario Outline: A real bound to <value> is <number>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property QtObject kid: QtObject {}
          property real r: <value>
          property string shown: "" + r
        }
        """
      Then the property "shown" of "root" is "<number>"

      Examples:
        | value                                     | number |
        | ({})                                      | NaN    |
        | ({ valueOf: function() { return 4 } })    | 4      |
        | new Date(5)                               | 5      |
        | []                                        | 0      |
        | ["7"]                                     | 7      |
        | [1, 2]                                    | NaN    |
        | kid                                       | NaN    |
        | Qt.point(1, 2)                            | NaN    |

    Scenario Outline: A real a script gives <value> is <number>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property QtObject kid: QtObject {}
          property real r: 2
          property string shown: "" + r
          function give() { r = <value> }
        }
        """
      When the method "give" of "root" is called
      Then the property "shown" of "root" is "<number>"

      Examples:
        | value                                     | number |
        | ({})                                      | NaN    |
        | ({ valueOf: function() { return 9 } })    | 9      |
        | new Date(6)                               | 6      |
        | []                                        | 0      |
        | kid                                       | NaN    |
        | Qt.point(1, 2)                            | NaN    |

  Rule: A string refuses an object

    Scenario Outline: A string bound to <value> says it cannot take it
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property QtObject kid: QtObject {}
          property string s: <value>
        }
        """
      Then the log has a warning matching "Scenario.qml:5:\d+: Unable to assign <said> to QString$"
      And the property "s" of "root" is ""

      Examples:
        | value                                        | said     |
        | ({})                                         | QJSValue |
        | ({ a: 1 })                                   | QJSValue |
        | ({ toString: function() { return "t" } })    | QJSValue |
        | kid                                          | QObject  |

    Scenario Outline: A script that gives a string <value> is thrown an error
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property QtObject kid: QtObject {}
          property string s: "x"
          property string caught
          function give() { try { s = <value> } catch (e) { caught = "" + e } }
        }
        """
      When the method "give" of "root" is called
      Then the property "caught" of "root" is "Error: Cannot assign <said> to QString"
      And the property "s" of "root" is "x"

      Examples:
        | value                                        | said     |
        | ({})                                         | QJSValue |
        | ({ toString: function() { return "t" } })    | QJSValue |
        | kid                                          | QObject* |

    Scenario Outline: A string still takes <value>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var given: <value>
          property string s: given
        }
        """
      Then the property "s" of "root" is "<text>"

      Examples:
        | value | text |
        | null  |      |
        | 12    | 12   |
        | true  | true |
