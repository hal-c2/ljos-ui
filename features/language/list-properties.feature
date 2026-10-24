# Sources:
#   https://doc.qt.io/qt-6/qml-list.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A list property declared in QML
  What a script reads from a `list<T>` property is the property's list, not an array of its own:
  adding to it or writing an entry changes the property and tells of the change, which is what
  an array in a var property does not do.

  Scenario: push adds to the property and tells of the change once
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property int changes: 0
        property int length: numbers.length
        property string seen
        onNumbersChanged: changes += 1
        function add() {
          numbers.push(3)
          seen = numbers.join(",")
        }
      }
      """
    Then the property "changes" of "root" is 1
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "1,2,3"
    And the property "changes" of "root" is 2
    And the property "length" of "root" is 3

  Scenario: Writing an entry changes the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names: ["a", "b"]
        property int changes: 0
        property string joined: names.join("+")
        onNamesChanged: changes += 1
        function write() { names[0] = "z" }
      }
      """
    When the method "write" of "root" is called
    Then the property "joined" of "root" is "z+b"
    And the property "changes" of "root" is 2

  Scenario: pop and a new length change the property once each
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2, 3]
        property int changes: 0
        property int length: numbers.length
        property string seen
        onNumbersChanged: changes += 1
        function pop() { seen = numbers.pop() + ":" + numbers.join(",") }
        function clear() { numbers.length = 0 }
      }
      """
    When the method "pop" of "root" is called
    Then the property "seen" of "root" is "3:1,2"
    And the property "changes" of "root" is 2
    When the method "clear" of "root" is called
    Then the property "changes" of "root" is 3
    And the property "length" of "root" is 0

  Scenario: A variable given the list is the property's list still
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
          var held = numbers
          held.push(9)
          seen = (held === numbers) + ":" + numbers.join(",")
          numbers = [5]
          seen += ":" + held.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:1,2,9:5"
    And the property "changes" of "root" is 3

  Scenario: A function given the list adds to the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1]
        property int changes: 0
        property string seen
        onNumbersChanged: changes += 1
        function add(list, value) { list.push(value); return list }
        function run() {
          var back = add(numbers, 2)
          seen = (back === numbers) + ":" + numbers.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:1,2"
    And the property "changes" of "root" is 2

  Scenario: A list of the same entries is no change
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers: [1, 2]
        property int changes: 0
        onNumbersChanged: changes += 1
        function same() { numbers = [1, 2] }
        function itself() { numbers = numbers }
        function other() { numbers = [2, 1] }
      }
      """
    When the method "same" of "root" is called
    And the method "itself" of "root" is called
    Then the property "changes" of "root" is 1
    When the method "other" of "root" is called
    Then the property "changes" of "root" is 2

  Scenario: An entry is made of the list's type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<int> numbers
        property string seen
        function run() {
          numbers.push("7")
          numbers.push(2.7)
          numbers[4] = 1
          seen = numbers.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "7,3,0,0,1"

  Scenario: A Repeater given the list follows what is added
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names: ["a"]
        property alias count: rows.count
        function add() { names.push("b") }
        Repeater {
          id: rows
          model: names
          Item {}
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "count" of "root" is 2

  Scenario: A list of objects takes an object pushed to it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<Item> things
        property int changes: 0
        property string seen
        onThingsChanged: changes += 1
        Item { id: first }
        Item { id: second }
        function run() {
          things.push(first)
          things.push(second)
          seen = things.length + ":" + (things[0] === first) + ":" + (things[1] === second)
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "2:true:true"
    And the property "changes" of "root" is 2

  Scenario: What the list's own functions give is an array of the script's
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
          var doubled = numbers.map(n => n * 2)
          doubled.push(0)
          var copy = numbers.slice()
          copy.push(0)
          seen = Array.isArray(doubled) + ":" + doubled.join(",") + ":" + numbers.length
          seen += ":" + JSON.stringify(numbers) + ":" + numbers
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:2,4,0:2:[1,2]:1,2"
    And the property "changes" of "root" is 1

  # Found by ljos-fuzz (bindings), buckets value:bool:step-set(Array.isArray) and
  # value:point:Qt.point.
  @backlog
  Scenario: The list property itself is not an array to Array.isArray
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names: ["a"]
        property list<real> sizes
        property bool namesAre: Array.isArray(names)
        property bool sizesAre: Array.isArray(sizes)
        property bool copyIs: Array.isArray(names.slice())
      }
      """
    Then the property "namesAre" of "root" is false
    And the property "sizesAre" of "root" is false
    And the property "copyIs" of "root" is true

  # Found by ljos-fuzz (bindings), bucket value:double:step-set(literal).
  @backlog
  Scenario: An empty list given an empty list is told as a change
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property list<string> names
        property int changes: 0
        onNamesChanged: changes++
        function clear() { names = [] }
      }
      """
    When the method "clear" of "root" is called
    Then the property "changes" of "root" is 1
