# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#method-attributes
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A function declared in QML, called from a script
  A script that calls a function another object declares passes it the script's own values and is
  given back what the function returns, as a call of any JavaScript function is: a zero keeps its
  sign, an object or an array is the same one, what it throws is the caller's to catch, and it reads
  its object's properties as they are when it is called.

  Scenario: A zero keeps its sign through the call and back
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        Item {
          id: other
          function neg(x) { return -x }
          function same(x) { return x }
        }
        function run() { seen = (1 / other.neg(0)) + ":" + (1 / other.same(-0)) }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "-Infinity:-Infinity"

  Scenario: An object or an array is the same one on both sides of the call
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        Item {
          id: other
          property var held: ({ k: 1 })
          function box() { return held }
          function push(a) { a.push(1); return a }
        }
        function run() {
          var a = []
          seen = (other.box() === other.box()) + ":" + (other.push(a) === a) + ":" + a.length
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:true:1"

  Scenario: What the function throws the caller catches
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        Item {
          id: other
          property int n: 1
          function fail() { throw new Error("no " + n) }
        }
        function run() {
          try { other.fail() } catch (e) { seen = (e instanceof Error) + ":" + e.message }
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:no 1"

  Scenario: The function reads its object as it is when called, and every argument it is given
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        Item {
          id: other
          property int n: 1
          function get() { return n }
          function count() { return arguments.length }
        }
        function run() {
          other.n = 5
          seen = other.get() + ":" + other.count(1, 2, 3)
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "5:3"
