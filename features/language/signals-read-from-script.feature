# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#connecting-signals-to-methods-and-signals
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A signal read from a script
  A script that reads an object's signal is given a function that emits it on that object, wherever
  the function is called from. Each read gives a function of its own, with no properties of its
  own: `connect` and `disconnect` are on `Function.prototype`, so every read shares them, and so
  does every other function.

  Scenario: Each read is a function of its own, and they share connect and disconnect
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        QtObject { id: o; signal pong() }
        function run() {
          seen = [o.pong === o.pong, o.pong.connect === o.pong.connect,
                  o.pong.disconnect === o.pong.disconnect, typeof o.pong,
                  typeof (function () {}).connect,
                  Object.getPrototypeOf(o.pong) === Function.prototype].join(":")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "false:true:true:function:function:true"

  Scenario: A signal read has no properties of its own
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        QtObject { id: o; signal pong(int n) }
        function run() {
          seen = [o.pong.name === "", o.pong.length, Object.getOwnPropertyNames(o.pong).length,
                  Object.keys(o.pong).length, o.pong.hasOwnProperty("connect")].join(":")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true:0:0:0:false"

  Scenario: A signal kept in a variable emits on its object and connects there
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        QtObject { id: o; signal pong(int n); property int got: 0; onPong: (n) => got += n }
        function run() {
          const f = o.pong
          const heard = []
          const h = (n) => heard.push(n)
          f.connect(h)
          f(1); o.pong(2); f.call(null, 3); f.apply(o, [4]); f.bind(null, 5)()
          f.disconnect(h)
          f(6)
          seen = heard.join(",") + ":" + o.got
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "1,2,3,4,5:21"

  Scenario: A signal read is not a constructor
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen
        QtObject { id: o; signal pong() }
        function run() {
          try { new o.pong(); seen = "made" } catch (e) { seen = e instanceof TypeError }
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "true"
