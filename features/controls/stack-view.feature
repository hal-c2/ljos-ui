# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: StackView
  A stack view shows the top page of a stack. Pages are pushed, popped
  and replaced, as items, as components or as files; the page that
  leaves and the one that comes are told, and a page the view made
  from a component is destroyed when it leaves the stack.

  Background:
    Given the file "Note.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        property string tag: "note"
        color: "green"
        StackView.onActivating: root.log += tag + ":activating "
        StackView.onActivated: root.log += tag + ":activated "
        StackView.onDeactivating: root.log += tag + ":deactivating "
        StackView.onDeactivated: root.log += tag + ":deactivated "
        StackView.onRemoved: root.log += tag + ":removed "
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200; property string log: ""; property string seen: ""; property var held: null
        Component { id: note; Note { } }
        Note { id: loose; objectName: "loose"; tag: "loose" }
        StackView { id: stack; objectName: "stack"; anchors.fill: parent; initialItem: Note { objectName: "first"; tag: "first" } }
        function tags() { seen = ""; for (var i = 0; i < stack.depth; ++i) { var it = stack.get(i); seen += (it ? it.tag : "null") + " " } }
        function pushNote(animated) { var m = { tag: "made", objectName: "made" }; log = ""; held = animated ? stack.push(note, m) : stack.push(note, m, StackView.Immediate) }
        function pushLoose() { log = ""; stack.push(loose, {}, StackView.Immediate) }
        function popTop() { var p = stack.pop(StackView.Immediate); seen = p ? p.tag : "null" }
        function popLoose() { pushLoose(); log = ""; stack.pop(StackView.Immediate) }
        function looseAfter() { seen = loose.visible + " " + (loose.parent === root) }
        function replaceByFile() { log = ""; stack.replace("Note.qml", { tag: "file" }, StackView.Immediate) }
        function pushMany() { stack.push([note, { tag: "a" }, note, { tag: "b" }], StackView.Immediate) }
        function loadSecond() { seen = stack.get(1, StackView.ForceLoad).tag }
        function findB() { seen = stack.find(function(item) { return item.tag === "b" }).tag }
        function popToFirst() { pushMany(); log = ""; seen = stack.pop(stack.get(0), StackView.Immediate).tag }
        function clearAll() { log = ""; stack.clear() }
        function newApi() { stack.pushItem(note, { tag: "p" }, StackView.Immediate); stack.pushItems([note, { tag: "q" }, note, { tag: "r" }], StackView.Immediate); tags() }
        function toIndex() { newApi(); seen = stack.popToIndex(1, StackView.Immediate).tag; tags() }
        function replaceTop() { seen = stack.replaceCurrentItem(note, { tag: "s" }, StackView.Immediate).tag }
        function popLast() { var p = stack.popCurrentItem(StackView.Immediate); seen = p.tag + " " + stack.depth }
        function attached() { var f = stack.get(0).StackView; seen = [f.index, f.status, f.view === stack, f.visible].join(" ") }
      }
      """

  @api-QtQuick.Controls.StackView.initialItem @api-QtQuick.Controls.StackView.depth
  Scenario: The initial item is the first page
    Then the property "depth" of "stack" is 1
    And the property "currentItem.tag" of "stack" is "first"
    And the property "busy" of "stack" is false
    And the property "empty" of "stack" is false

  @api-QtQuick.Controls.StackView.view @api-QtQuick.Controls.StackView.status
  Scenario: A page knows its place in the stack
    When the method "attached" of "root" is called
    Then the property "seen" of "root" is "0 3 true true"

  @api-QtQuick.Controls.StackView.push
  Scenario: A page pushed from a component is made with its properties
    When the method "pushNote" of "root" is called
    Then the property "depth" of "stack" is 2
    And the property "currentItem.tag" of "stack" is "made"
    And the property "held.tag" of "root" is "made"
    And the property "currentItem.width" of "stack" is 300

  @api-QtQuick.Controls.StackView.activated @api-QtQuick.Controls.StackView.deactivated
  Scenario: The coming page is activated before the leaving one is deactivated
    When the method "pushLoose" of "root" is called
    Then the property "log" of "root" is "loose:activating loose:activated first:deactivating first:deactivated "

  @api-QtQuick.Controls.StackView.visible
  Scenario: A page under the top one is hidden
    When the method "pushLoose" of "root" is called
    Then the property "visible" of "first" is false
    And the property "visible" of "loose" is true

  @api-QtQuick.Controls.StackView.pop
  Scenario: Popping tells the leaving page before the coming one
    When the method "popLoose" of "root" is called
    Then the property "log" of "root" is "loose:deactivating loose:deactivated loose:removed first:activating first:activated "
    And the property "currentItem.tag" of "stack" is "first"

  Scenario: An item pushed as it is goes back to its parent hidden
    When the method "popLoose" of "root" is called
    And the method "looseAfter" of "root" is called
    Then the property "seen" of "root" is "false true"

  Scenario: A page made from a component is destroyed when popped
    When the method "pushNote" of "root" is called
    And the method "popTop" of "root" is called
    And 10 milliseconds pass
    Then the property "seen" of "root" is "made"
    And the object "made" does not exist

  Scenario: The last page is not popped
    When the method "popTop" of "root" is called
    Then the property "seen" of "root" is "null"
    And the property "depth" of "stack" is 1

  @api-QtQuick.Controls.StackView.replace
  Scenario: A page is replaced by one made from a file
    When the method "replaceByFile" of "root" is called
    Then the property "depth" of "stack" is 1
    And the property "currentItem.tag" of "stack" is "file"
    And the property "log" of "root" is "first:deactivating first:deactivated first:removed file:activating file:activated "

  @api-QtQuick.Controls.StackView.get
  Scenario: Pages pushed together are made when they are needed
    When the method "pushMany" of "root" is called
    And the method "tags" of "root" is called
    Then the property "depth" of "stack" is 3
    And the property "seen" of "root" is "first null b "

  Scenario: A page that was not made yet is made on request
    When the method "pushMany" of "root" is called
    And the method "loadSecond" of "root" is called
    Then the property "seen" of "root" is "a"

  @api-QtQuick.Controls.StackView.find
  Scenario: A page is found by a function
    When the method "pushMany" of "root" is called
    And the method "findB" of "root" is called
    Then the property "seen" of "root" is "b"

  Scenario: Popping down to a page removes every page above it
    When the method "popToFirst" of "root" is called
    Then the property "seen" of "root" is "b"
    And the property "depth" of "stack" is 1
    And the property "log" of "root" is "b:deactivating b:deactivated b:removed first:activating first:activated "

  @api-QtQuick.Controls.StackView.clear @api-QtQuick.Controls.StackView.empty
  Scenario: Clearing removes every page
    When the method "clearAll" of "root" is called
    Then the property "depth" of "stack" is 0
    And the property "empty" of "stack" is true
    And the property "currentItem" of "stack" is null
    And the property "log" of "root" is "first:removed "

  @api-QtQuick.Controls.StackView.pushItems
  Scenario: The newer methods push one page and several
    When the method "newApi" of "root" is called
    Then the property "seen" of "root" is "first p null r "

  @api-QtQuick.Controls.StackView.popToIndex
  Scenario: Popping down to an index
    When the method "toIndex" of "root" is called
    Then the property "seen" of "root" is "first p "
    And the property "currentItem.tag" of "stack" is "p"

  @api-QtQuick.Controls.StackView.replaceCurrentItem
  Scenario: The current page is replaced
    When the method "replaceTop" of "root" is called
    Then the property "seen" of "root" is "s"
    And the property "depth" of "stack" is 1

  @api-QtQuick.Controls.StackView.popCurrentItem
  Scenario: The newer pop takes the last page too
    When the method "popLast" of "root" is called
    Then the property "seen" of "root" is "first 0"

  @api-QtQuick.Controls.StackView.busy
  Scenario: The view is busy while a page comes in
    When the method "pushNote" of "root" is called with true
    Then the property "busy" of "stack" is true
    And the property "log" of "root" is "made:activating first:deactivating "
    When 1 second passes
    Then the property "busy" of "stack" is false
    And the property "log" of "root" is "made:activating first:deactivating made:activated first:deactivated "
    And the property "visible" of "first" is false

  @api-QtQuick.Controls.StackView.pushEnter
  Scenario: The pushed page comes in from the right
    When the method "pushNote" of "root" is called with true
    And 1 second passes
    Then the property "currentItem.x" of "stack" is 0
    And the property "x" of "first" is -300
