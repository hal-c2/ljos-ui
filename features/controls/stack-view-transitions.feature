# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html#transitions
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: StackView operations while pages move
  An operation that comes while a transition runs neither waits for it
  nor ends it first. A page it moves again goes on from where it is and
  is not told how the earlier operation would have ended; a page is told
  its end when its own transition ends. The view is busy until every
  page is still, and pages that left are removed after that.

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

  @api-QtQuick.Controls.StackView.pushExit @api-QtQuick.Controls.StackView.activated
  Scenario: A page is told its end when its own transition ends
    When the method "start" of "root" is called
    Then the property "log" of "root" is "b:activating a:deactivating busy:true "
    When 200 milliseconds pass
    Then the property "log" of "root" is "b:activating a:deactivating busy:true a:deactivated "
    When 300 milliseconds pass
    Then the property "log" of "root" is "b:activating a:deactivating busy:true a:deactivated b:activated busy:false "

  @api-QtQuick.Controls.StackView.push @api-QtQuick.Controls.StackView.busy
  Scenario: A page covered while it comes in is never activated
    When the method "start" of "root" is called
    And 200 milliseconds pass
    And the method "pushC" of "root" is called
    Then the property "log" of "root" is "c:activating b:deactivating "
    And the property "x" of "b" is 0
    And the property "busy" of "stack" is true
    When 1 second passes
    Then the property "log" of "root" is "c:activating b:deactivating b:deactivated c:activated busy:false "
    And the property "visible" of "b" is false
    And the property "depth" of "stack" is 3

  @api-QtQuick.Controls.StackView.pop @api-QtQuick.Controls.StackView.removed
  Scenario: A page popped while it comes in is removed once the view is still
    When the method "start" of "root" is called
    And 200 milliseconds pass
    And the method "popB" of "root" is called
    Then the property "log" of "root" is "b:deactivating a:activating "
    When 1 second passes
    Then the property "log" of "root" is "b:deactivating a:activating b:deactivated a:activated busy:false b:removed "
    And the property "depth" of "stack" is 1

  @api-QtQuick.Controls.StackView.Immediate
  Scenario: A page pushed at once while another comes in
    When the method "start" of "root" is called
    And 200 milliseconds pass
    And the method "pushNow" of "root" is called
    Then the property "log" of "root" is "c:activating c:activated b:deactivating b:deactivated busy:false "
    And the property "visible" of "b" is false
