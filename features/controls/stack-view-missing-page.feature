# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
# Verified on Qt 6.11.2 (qml6, offscreen); the scenario marked @qt-diverges
# says where qml6 loses the shown page.

@qt-parity
Feature: A StackView page that cannot be made
  Pushing or replacing with a file that cannot be loaded warns and leaves
  the stack as it was: the shown page stays current and is not told it
  leaves, and the next page that does replace it removes it.

  Background:
    Given the file "Note.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        property string tag
        StackView.onDeactivated: root.log += tag + ":deactivated "
        StackView.onRemoved: root.log += tag + ":removed "
        Component.onDestruction: root.log += tag + ":destroyed "
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string log: ""; property string seen: ""
        StackView { id: stack; objectName: "stack"; anchors.fill: parent }
        Component.onCompleted: {
          stack.push("Note.qml", { tag: "a" }, StackView.Immediate)
          stack.push("Note.qml", { tag: "b" }, StackView.Immediate)
          log = ""
        }
        function tags() { seen = ""; for (var i = 0; i < stack.depth; ++i) seen += stack.get(i).tag + " " }
        function missing() { var r = stack.replace("Missing.qml", { tag: "m" }, StackView.Immediate); seen = r.tag + " " + stack.depth + " " + stack.currentItem.tag + " " + stack.currentItem.visible }
        function pushMissing() { stack.push("Missing.qml", {}, StackView.Immediate); seen = stack.currentItem.tag + " " + stack.currentItem.visible }
        function next() { stack.replace("Note.qml", { tag: "c" }, StackView.Immediate) }
      }
      """

  @api-QtQuick.Controls.StackView.replace
  Scenario: A missing file does not replace the shown page
    When the method "missing" of "root" is called
    Then the property "seen" of "root" is "b 2 b true"
    And the property "log" of "root" is ""
    And the log has a warning matching "QML StackView: replace: .*Missing\.qml"

  @api-QtQuick.Controls.StackView.push
  Scenario: A missing file pushed leaves the shown page current
    When the method "pushMissing" of "root" is called
    Then the property "seen" of "root" is "b true"
    And the property "log" of "root" is ""
    And the log has a warning matching "QML StackView: push: .*Missing\.qml"

  # qml6 takes the shown page out of its stack (get(1) reads null) yet
  # keeps it, shown, outside it until the application quits: a menu that
  # names a missing page leaks a page each time it is chosen. Ljós keeps
  # the page in the stack, so the next replace removes it.
  @qt-diverges @api-QtQuick.Controls.StackView.replace
  Scenario: The page a missing file left in place goes with the next replace
    When the method "missing" of "root" is called
    And the method "tags" of "root" is called
    Then the property "seen" of "root" is "a b "
    When the method "next" of "root" is called
    And 50 milliseconds pass
    Then the property "log" of "root" is "b:deactivated b:removed b:destroyed "
    And the property "currentItem.tag" of "stack" is "c"
    And the property "depth" of "stack" is 2
