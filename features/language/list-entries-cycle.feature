# Sources:
#   https://doc.qt.io/qt-6/qml-list.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-sequencetypes.html
#
# Ljós behaviour: qml6 6.11.2 does not finish an assignment of an object that holds itself to a
# list<var>, so there is nothing of Qt's to match.

Feature: A list given an object that holds itself takes it
  A list property copies the arrays and objects it is given (list-entries-copied.feature). An
  object that holds itself, or holds an object that holds it, is copied down to where it comes
  round again, and from there the list shares it. The list takes it with no warning.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<var> held
        property string seen
        function own() {
          var c = { n: 1 }
          c.self = c
          held = [c]
          seen = held.length + " " + held[0].n + " " + (held[0] === c) + " " + held[0].self.self.n
        }
        function round() {
          var a = { n: 1 }, b = { n: 2, a: a }
          a.b = b
          held = [a]
          seen = held[0].b.n + " " + held[0].b.a.b.n
        }
      }
      """

  Scenario: An object that holds itself
    When the method "own" of "root" is called
    Then the property "seen" of "root" is "1 1 false 1"
    And the log has no warnings

  Scenario: Two objects that hold each other
    When the method "round" of "root" is called
    Then the property "seen" of "root" is "2 2"
    And the log has no warnings
