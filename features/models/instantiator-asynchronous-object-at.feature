# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html#asynchronous-prop
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html#objectAt-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding that reads an asynchronous Instantiator's objects
  An asynchronous Instantiator makes its objects after a moment. A binding that walks them with
  objectAt runs again as each one arrives, so it ends up seeing all of them.

  Scenario: The binding sees the objects once they are made
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root
        objectName: "root"
        property string later: {
          let s = ""
          for (let c = 0; c < slow.count; ++c) {
            const it = slow.objectAt(c)
            s += it ? it.objectName : "-"
          }
          return s
        }
        Instantiator {
          id: slow
          asynchronous: true
          model: 3
          QtObject { objectName: "o" + index; property string seen: root.later }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "later" of "root" is "o0o1o2"
