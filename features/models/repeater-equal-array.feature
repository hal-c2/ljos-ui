# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#model-prop
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html#javascript-arrays
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (models), bucket value:log:text.

@qt-parity
Feature: A Repeater given an array equal to the one it shows
  A new array as the model replaces every delegate, unless it holds what the shown one does:
  then the delegates stay.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var rows: [9]
        property string log: ""
        Repeater {
          model: root.rows
          onItemAdded: function (index, item) { root.log += "+" + index + ";" }
          onItemRemoved: function (index, item) { root.log += "-" + index + ";" }
          Item {}
        }
        function giveSame() { rows = [9] }
        function giveOther() { rows = [8] }
      }
      """

  @backlog
  Scenario: An equal array keeps the delegates
    When the method "giveSame" of "root" is called
    Then the property "log" of "root" is "+0;"

  Scenario: Another array replaces them
    When the method "giveOther" of "root" is called
    Then the property "log" of "root" is "+0;-0;+0;"
