# Sources:
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#focus-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
# Verified on Qt 6.12.0 (qmltestrunner and qml6, offscreen).
# What a script makes with `focus: true` has active focus by the time
# createObject gives it back, not a frame later, also when it is made
# while another creation is still running.

@qt-parity
Feature: An item made with focus has active focus when it is made
  A script that makes an item with focus: true in a window can read its
  activeFocus at once, the way it can read any other property it set.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string seen: ""
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        Component {
          id: plain
          Item { objectName: "made"; property bool on: false; focus: on }
        }
        Component {
          id: mouse
          MouseArea { objectName: "made"; property bool on: false; focus: on }
        }
        Component {
          id: maker
          Item {
            objectName: "maker"
            property Item inner: null
            property bool innerHadIt: true
            Component.onCompleted: {
              inner = plain.createObject(root, { on: true })
              innerHadIt = inner.activeFocus
            }
          }
        }
        function make(c, props) {
          const t = c.createObject(root, props)
          seen = (t.focus ? "focus" : "no focus") + ", "
            + (t.activeFocus ? "active" : "not active")
        }
        function makePlain() { make(plain, { on: true }) }
        function makeMouse() { make(mouse, { on: true }) }
        function makeWithout() { make(plain, {}) }
        function makeNested() {
          const m = maker.createObject(root)
          seen = (m.innerHadIt ? "inside: active" : "inside: not active") + ", "
            + (m.inner.activeFocus ? "after: active" : "after: not active")
        }
      }
      """

  Scenario Outline: <what> made with focus: true has active focus when createObject returns
    When the method "<method>" of "root" is called
    Then the property "seen" of "root" is "focus, active"
    And the property "active" of "root" is "made"

    Examples:
      | what        | method    |
      | An Item     | makePlain |
      | A MouseArea | makeMouse |

  Scenario: An item made without focus has no active focus
    When the method "makeWithout" of "root" is called
    Then the property "seen" of "root" is "no focus, not active"
    And the property "active" of "root" is ""

  Scenario: An item made while another creation runs has it before that one is complete
    When the method "makeNested" of "root" is called
    Then the property "seen" of "root" is "inside: active, after: active"
    And the property "active" of "root" is "made"
