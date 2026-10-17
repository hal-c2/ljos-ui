# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-treeviewdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a TreeView gives a TreeViewDelegate is required
  treeView, depth, hasChildren, expanded, isTreeNode, current, selected
  and editing are required properties of a TreeViewDelegate, which its
  TreeView gives each delegate it makes. A TreeViewDelegate made outside
  a view, without them, is not made. (The style's delegate may require
  more of its own, as qml6's model and row.)

  Scenario: A TreeViewDelegate made without a view is not made
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        objectName: "root"
        property string out: ""
        Component { id: c; TreeViewDelegate {} }
        function go() { out = c.createObject(this) === null ? "null" : "made" }
      }
      """
    When the method "go" of "root" is called
    Then the log has a warning matching "Required property treeView was not initialized"
    And the property "out" of "root" is "null"

  Scenario: A TreeViewDelegate template given what is required is made
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Templates as T
      Item {
        objectName: "root"
        property string out: ""
        Component { id: c; T.TreeViewDelegate {} }
        function go() {
          var d = c.createObject(this, { treeView: null, depth: 2, hasChildren: true, expanded: false,
                                         isTreeNode: true, current: false, selected: true, editing: false })
          out = [d.depth, d.hasChildren, d.selected].join(" ")
        }
      }
      """
    When the method "go" of "root" is called
    Then the property "out" of "root" is "2 true true"
    And the log has no warnings
