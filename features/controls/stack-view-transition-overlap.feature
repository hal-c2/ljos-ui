# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html#transitions
# Qt 6.11.2 (qml6, offscreen) runs one Transition on two pages at once: a
# page still going out when the next push comes goes on to the end of
# pushExit and is told then, while the page it covered runs pushExit too.
# A Transition in Ljós moves one page at a time, so the page that was
# going out ends at once and is hidden. stack-view-transitions.feature
# holds what is the same on both.

@qt-diverges
Feature: A StackView transition needed again while it runs
  A push that needs the transition a page is still going out with ends
  that page at once: it is told it was deactivated and hidden, and the
  pages of the push move as usual.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200; property string log: ""
        Component {
          id: page
          Rectangle {
            property string tag; objectName: tag
            StackView.onActivating: root.log += tag + ":activating "; StackView.onActivated: root.log += tag + ":activated "
            StackView.onDeactivating: root.log += tag + ":deactivating "; StackView.onDeactivated: root.log += tag + ":deactivated "
            StackView.onRemoved: root.log += tag + ":removed "
          }
        }
        StackView {
          id: stack; objectName: "stack"; anchors.fill: parent; onBusyChanged: root.log += "busy:" + busy + " "
          pushEnter: Transition { NumberAnimation { property: "x"; from: 300; to: 0; duration: 400 } }
          pushExit: Transition { NumberAnimation { property: "x"; from: 0; to: -300; duration: 100 } }
          popEnter: Transition { NumberAnimation { property: "x"; from: -300; to: 0; duration: 300 } }
          popExit: Transition { NumberAnimation { property: "x"; from: 0; to: 300; duration: 300 } }
        }
        function start() { stack.push(page, { tag: "a" }, StackView.Immediate); log = ""; stack.push(page, { tag: "b" }) }
        function pushC() { log = ""; stack.push(page, { tag: "c" }) }
        function pushNow() { log = ""; stack.push(page, { tag: "c" }, StackView.Immediate) }
        function popB() { log = ""; stack.pop() }
      }
      """

  @api-QtQuick.Controls.StackView.pushExit
  Scenario: A page still going out ends at once when its transition is needed again
    When the method "start" of "root" is called
    And 50 milliseconds pass
    And the method "pushC" of "root" is called
    Then the property "log" of "root" is "a:deactivated c:activating b:deactivating "
    And the property "visible" of "a" is false
    When 1 second passes
    Then the property "log" of "root" is "a:deactivated c:activating b:deactivating b:deactivated c:activated busy:false "
