# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#pressAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#toggleAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#increaseAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#scrollDownAction-signal
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#nextPageAction-signal
# Verified on Qt 6.11.2 (qml6, offscreen).
# The oracle cannot play an assistive technology, so here the item emits the
# signals itself: what is verified is that each signal and its handler exist.

@qt-parity @a11y
Feature: The action signals of Accessible
  Accessible has a signal for each thing an assistive technology can ask of an
  item, and a handler for each signal.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        function emit(name) { it.Accessible[name]() }
        Rectangle {
          id: it
          Accessible.role: Accessible.Slider
          Accessible.onPressAction: root.log += "press "
          Accessible.onToggleAction: root.log += "toggle "
          Accessible.onIncreaseAction: root.log += "increase "
          Accessible.onDecreaseAction: root.log += "decrease "
          Accessible.onScrollUpAction: root.log += "up "
          Accessible.onScrollDownAction: root.log += "down "
          Accessible.onScrollLeftAction: root.log += "left "
          Accessible.onScrollRightAction: root.log += "right "
          Accessible.onPreviousPageAction: root.log += "previous "
          Accessible.onNextPageAction: root.log += "next "
        }
      }
      """

  Scenario Outline: A signal reaches its handler
    When the method "emit" of "root" is called with "<signal>"
    Then the property "log" of "root" is "<heard> "

    Examples:
      | signal             | heard    |
      | pressAction        | press    |
      | toggleAction       | toggle   |
      | increaseAction     | increase |
      | decreaseAction     | decrease |
      | scrollUpAction     | up       |
      | scrollDownAction   | down     |
      | scrollLeftAction   | left     |
      | scrollRightAction  | right    |
      | previousPageAction | previous |
      | nextPageAction     | next     |

  Scenario: Nothing is heard until something is asked
    Then the property "log" of "root" is ""
