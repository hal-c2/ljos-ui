# Sources:
#   https://doc.qt.io/qt-6/qml-list.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A list property a binding gives and a script hands on
  A script that adds to a list property changes what the property holds and leaves its binding
  where it is. The list is the property's own only while a script passes it around: kept in a
  var property, sent with a signal or assigned to another list, it is the entries it had.

  Scenario: push leaves the binding in place
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int first: 1
        property list<int> numbers: [first, 2]
        property string seen
        function run() {
          numbers.push(3)
          seen = numbers.join(",")
          first = 5
          seen += ":" + numbers.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,2,3:5,2"

  Scenario: A binding that gives the same entries again is no change
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int factor: 1
        property list<int> numbers: [factor * 0, 2]
        property int changes: 0
        onNumbersChanged: changes += 1
      }
      """
    When the property "factor" of "root" is set to 7
    Then the property "changes" of "root" is 1

  Scenario: Writing the entry an entry has is no change
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property int changes: 0
        onNumbersChanged: changes += 1
        function same() { numbers[0] = 1; numbers.length = 2; numbers.push() }
        function other() { numbers[0] = 4 }
      }
      """
    When the method "same" of "root" is called
    Then the property "changes" of "root" is 1
    When the method "other" of "root" is called
    Then the property "changes" of "root" is 2

  Scenario: A list of var is a new list whenever it is assigned
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<var> things: [1, "a"]
        property int changes: 0
        onThingsChanged: changes += 1
        function same() { things = [1, "a"] }
      }
      """
    When the method "same" of "root" is called
    Then the property "changes" of "root" is 2

  Scenario: A function that returns the list returns the property's list
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property string seen
        function list() { return numbers }
        function run() {
          var held = list()
          held.push(3)
          seen = (held === numbers) + ":" + (list() === list()) + ":" + numbers.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:true:1,2,3"

  Scenario: A var property given the list keeps the entries it had
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property var kept
        property var bound: numbers
        property string seen
        function run() {
          kept = numbers
          numbers.push(3)
          kept.push(8, 9)
          seen = kept.join(",") + ":" + numbers.join(",") + ":" + bound.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,2,8,9:1,2,3:1,2,3"

  Scenario: A signal sent the list carries the entries it had
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property string seen
        signal sent(var list)
        onSent: list => {
          list.push(9)
          seen = list.join(",") + ":" + numbers.join(",")
        }
        function run() { sent(numbers) }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,2,9:1,2"

  Scenario: Another list given the list has entries of its own
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property list<int> others
        property int changes: 0
        property string seen
        onOthersChanged: changes += 1
        function run() {
          others = numbers
          others.push(3)
          seen = others.join(",") + ":" + numbers.join(",")
          others = [1, 2, 3]
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,2,3:1,2"
    And the property "changes" of "root" is 2

  Scenario: An alias of the list adds to the list
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property alias also: root.numbers
        id: root
        property int changes: 0
        property string seen
        onNumbersChanged: changes += 1
        function run() {
          also.push(4)
          seen = numbers.join(",") + ":" + also.length
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,2,4:3"
    And the property "changes" of "root" is 2

  Scenario: One value assigned is a list of it, and a string a list of its characters
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property list<int> codes
        property list<string> names
        property string seen
        function run() {
          numbers = 2.7
          codes = "ab"
          names = "one"
          seen = numbers.join(",") + ":" + codes.join(",") + ":" + names.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "3:97,98:o,n,e"

  Scenario: An entry of an array assigned is converted as a script converts it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> ints: [true, -2.5, 2.7, " 12 ", "0x10", "x", null]
        property list<real> reals: [true, "2.5", null]
        property list<string> strings: [7, 2.5, true, null]
        property list<bool> bools: ["false", "", 0, 3]
        property string seen
        function run() {
          seen = ints.join(",") + ":" + reals.join(",") + ":" + strings.join(",")
          seen += ":" + bools.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,-2,2,12,16,0,0:1,2.5,0:7,2.5,true,null:true,false,false,true"

  Scenario: An entry taken away is the zero of the type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property int changes: 0
        property string seen
        onNumbersChanged: changes += 1
        function run() {
          delete numbers[0]
          seen = numbers.length + ":" + numbers.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "2:0,2"
    And the property "changes" of "root" is 2
