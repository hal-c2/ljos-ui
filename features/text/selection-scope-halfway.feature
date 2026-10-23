# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionAt-method
#   features/text-input/pointer-halfway.feature
# Qt Quick's Text has no positionAt (qml6: "is not a function"), so there
# is nothing of Qt's to measure for a Text alone. SelectionScope is a type
# of module Ljos and keeps the rule measured for TextEdit, plain and rich.

@ljos-only
Feature: A point halfway across a character of a SelectionScope
  positionAt gives the nearer edge of the character under the point, and
  exactly halfway the edge before it, as a TextEdit does. In Liberation
  Sans at 20 pixels an "x" is 10 pixels wide.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Ljos
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property int found: -1
        function probe(x) { found = scope.positionAt(x, 10) }
        SelectionScope {
          id: scope; anchors.fill: parent
          Text { font.family: "Liberation Sans"; font.pixelSize: 20; text: "xx" }
        }
      }
      """

  Scenario Outline: positionAt(<x>, 10) is <position>
    When the method "probe" of "root" is called with <x>
    Then the property "found" of "root" is <position>

    Examples:
      | x  | position |
      | 4  | 0        |
      | 5  | 0        |
      | 6  | 1        |
      | 15 | 1        |
      | 16 | 2        |
