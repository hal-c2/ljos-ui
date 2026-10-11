# Sources:
#   https://doc.qt.io/qt-6/qml-var.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An object or a function in a var property
  A var property holds the script's own object. The property changes when it is given another
  object, not when the one it holds is changed, and not when it is given the one it holds.

  Scenario: A member written in place is not a change of the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var box: ({ n: 1 })
        property int changes: 0
        property string seen
        onBoxChanged: changes += 1
        function run() {
          box.n = 2
          box.more = [3]
          seen = (box === box) + ":" + box.n + ":" + box.more.length
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:2:1"
    And the property "changes" of "root" is 1

  Scenario: Assigning the object the property holds changes nothing
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var box: ({ n: 1 })
        property int changes: 0
        onBoxChanged: changes += 1
        function same() { box = box }
        function other() { box = { n: 1 } }
      }
      """
    When the method "same" of "root" is called
    Then the property "changes" of "root" is 1
    When the method "other" of "root" is called
    Then the property "changes" of "root" is 2

  Scenario: Assigning the function the property holds changes nothing
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var action: function() { return 1 }
        property int changes: 0
        onActionChanged: changes += 1
        function same() { action = action }
        function other() { action = function() { return 2 } }
      }
      """
    When the method "same" of "root" is called
    Then the property "changes" of "root" is 1
    When the method "other" of "root" is called
    Then the property "changes" of "root" is 2

  Scenario: A property given an array or an object where it is declared tells of it once
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var empty: []
        property var list: [1, "two"]
        property var box: ({})
        property var number: 5
        property var nothing
        property string told
        onEmptyChanged: told += "empty "
        onListChanged: told += "list "
        onBoxChanged: told += "box "
        onNumberChanged: told += "number "
        onNothingChanged: told += "nothing "
      }
      """
    Then the property "told" of "root" is "empty list box "

  Scenario: An object literal with a property in it is a new object when the property changes
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 1
        property var box: ({ n: n })
        property string seen
        function run() {
          var before = box
          box.extra = true
          n = 5
          seen = (before === box) + ":" + box.n + ":" + box.extra
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "false:5:undefined"

  Scenario: Another object's property holds the same object
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var box: ({ n: 1 })
        property var mirror: box
        property string seen
        Item {
          id: other
          property var held
        }
        function run() {
          other.held = box
          other.held.n = 4
          seen = (other.held === box) + ":" + (mirror === box) + ":" + mirror.n
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:true:4"
