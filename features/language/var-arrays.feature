# Sources:
#   https://doc.qt.io/qt-6/qml-var.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An array in a var property
  A var property holds the script's own array. Reading it gives that array, not a copy, so a
  script that changes it in place changes what the property holds; nothing is told of such a
  change, because the property still holds the same array.

  Scenario: Reading the property twice gives the same array
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1, 2]
        property bool same: false
        property bool isArray: false
        function check() {
          same = xs === xs
          isArray = Array.isArray(xs) && xs instanceof Array
        }
      }
      """
    When the method "check" of "root" is called
    Then the property "same" of "root" is true
    And the property "isArray" of "root" is true

  Scenario: push changes the array the property holds
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: []
        property string seen
        function add() {
          xs.push(4)
          xs.push(5)
          xs[0] = 7
          seen = xs.length + ":" + xs.join(",")
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "seen" of "root" is "2:7,5"

  Scenario: A change in place is not a change of the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property int changes: 0
        property int length: xs.length
        onXsChanged: changes += 1
        function add() { xs.push(2) }
      }
      """
    Then the property "changes" of "root" is 1
    When the method "add" of "root" is called
    Then the property "changes" of "root" is 1
    And the property "length" of "root" is 1

  Scenario: Emitting the change signal by hand runs what reads the array again
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property int changes: 0
        property int length: xs.length
        onXsChanged: changes += 1
        function add() {
          xs.push(2)
          xsChanged()
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "length" of "root" is 2
    And the property "changes" of "root" is 2

  Scenario: Assigning the array the property holds changes nothing
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property int changes: 0
        onXsChanged: changes += 1
        function same() { xs = xs }
        function other() { xs = [1] }
      }
      """
    When the method "same" of "root" is called
    Then the property "changes" of "root" is 1
    When the method "other" of "root" is called
    Then the property "changes" of "root" is 2

  Scenario: The property holds the array it was given, not a copy of it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs
        property string seen
        function run() {
          var mine = [1, 2]
          xs = mine
          mine.push(3)
          seen = (xs === mine) + ":" + xs.length
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:3"

  Scenario: A function is given the caller's array and gives it back
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property string seen
        function add(list, value) {
          list.push(value)
          return list
        }
        function run() {
          var back = root.add(xs, 2)
          seen = (back === xs) + ":" + xs.join(",")
        }
        id: root
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:1,2"

  Scenario: Another object's function and property get the same array
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var xs: [1]
        property string seen
        Item {
          id: other
          property var held
          function keep(list) { held = list; return list }
        }
        function run() {
          var back = other.keep(xs)
          seen = (back === xs) + ":" + (other.held === xs)
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:true"

  Scenario: A var bound to another var holds the same array
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property var mirror: xs
        property alias also: root.xs
        property string seen
        id: root
        function run() {
          also.push(2)
          seen = (mirror === xs) + ":" + (also === xs) + ":" + mirror.length
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:true:2"

  Scenario: The same array again is no change for what is bound to it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property var mirror: xs
        property int changes: 0
        onMirrorChanged: changes += 1
        function add() {
          xs.push(2)
          xsChanged()
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "changes" of "root" is 1

  Scenario: A signal carries the array itself
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: [1]
        property string seen
        signal sent(var list)
        onSent: list => {
          list.push(2)
          seen = (list === xs) + ":" + xs.length
        }
        function run() { sent(xs) }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:2"

  Scenario: An array literal with a property in it is a new array when the property changes
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 1
        property var xs: [n, n + 1]
        property int changes: 0
        property string seen
        onXsChanged: changes += 1
        function run() {
          var before = xs
          xs.push(9)
          n = 5
          seen = (before === xs) + ":" + xs.join(",")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "false:5,6"
    And the property "changes" of "root" is 2

  Scenario: A Repeater given the array does not see a change in place
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: ["a"]
        property alias count: rows.count
        function add() { xs.push("b") }
        function tell() { xsChanged() }
        Repeater {
          id: rows
          model: xs
          Item {}
        }
      }
      """
    When the method "add" of "root" is called
    Then the property "count" of "root" is 1
    When the method "tell" of "root" is called
    Then the property "count" of "root" is 2

  Scenario: What a Repeater is given reads back as another array
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var xs: ["a"]
        property string seen
        function run() { seen = (rows.model === xs) + ":" + rows.model.length }
        Repeater {
          id: rows
          model: xs
          Item {}
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "false:1"
