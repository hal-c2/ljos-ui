# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html#transitions
#   https://doc.qt.io/qt-6/qaccessibilityhints.html#motionPreference-prop
#   https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html
#   https://web.dev/articles/prefers-reduced-motion
#   docs/design/motion.md (reduced motion)
# Qt tells an application that the user wants less motion and leaves the rest
# to it. Ours: a StackView's pages do not slide. The page that comes fades in
# where it ends, the page that goes fades out where it is, in at most 150
# milliseconds, and the pages are told the same things in the same order as
# with motion. Runs on Ljós only.

@ljos-only @a11y
Feature: StackView transitions with reduced motion
  When the system asks for reduced motion a push, a pop or a replace
  cross-fades the two pages in place instead of sliding them.

  Background:
    Given the system setting "reduced motion" is true
    And the QML:
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
          }
        }
        StackView { id: stack; objectName: "stack"; anchors.fill: parent }
        function start() { stack.push(page, { tag: "a" }, StackView.Immediate); log = "" }
        function pushB() { stack.push(page, { tag: "b" }) }
        function popB() { log = ""; stack.pop() }
      }
      """

  @api-QtQuick.Controls.StackView.pushEnter @api-QtQuick.Controls.StackView.pushExit
  Scenario: A push fades the new page in where it ends and the old one out where it is
    When the method "start" of "root" is called
    And the method "pushB" of "root" is called
    And 75 milliseconds pass
    Then the property "x" of "b" is 0
    And the property "opacity" of "b" is within 0.15 of 0.5
    And the property "x" of "a" is 0
    And the property "opacity" of "a" is within 0.15 of 0.5
    And the property "busy" of "stack" is true

  Scenario: The pages of a push are told the same things in the same order
    When the method "start" of "root" is called
    And the method "pushB" of "root" is called
    And 400 milliseconds pass
    Then the property "log" of "root" is "b:activating a:deactivating b:activated a:deactivated "
    And the property "busy" of "stack" is false
    And the property "visible" of "a" is false
    And the property "opacity" of "b" is 1

  @api-QtQuick.Controls.StackView.popEnter @api-QtQuick.Controls.StackView.popExit
  Scenario: A pop fades the page below back in where it is
    When the method "start" of "root" is called
    And the method "pushB" of "root" is called
    And 400 milliseconds pass
    And the method "popB" of "root" is called
    And 75 milliseconds pass
    Then the property "x" of "a" is 0
    And the property "opacity" of "a" is within 0.15 of 0.5
    When 400 milliseconds pass
    Then the property "opacity" of "a" is 1
    And the property "busy" of "stack" is false
