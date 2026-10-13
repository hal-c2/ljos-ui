# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatechooser.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatechoice.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @backlog
Feature: DelegateChooser
  A DelegateChooser is a delegate that picks one for each row: the delegate of the first DelegateChoice that matches the row.

  Rule: In a Repeater, the first choice that matches a row gives its delegate

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property int made: 0
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          function reset() { rep.model = 0; rep.model = lm }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { kind: "a"; n: 1 }
            ListElement { kind: "b"; n: 2 }
            ListElement { kind: "c"; n: 3 }
          }
          DelegateChooser {
            id: ch; objectName: "ch"; role: "kind"
            DelegateChoice { roleValue: "a"; Item { property string tag: "A" + index; Component.onCompleted: root.made++ } }
            DelegateChoice { index: 2; Item { property string tag: "I" + index + kind; Component.onCompleted: root.made++ } }
            DelegateChoice { Item { property string tag: "D" + index; Component.onCompleted: root.made++ } }
          }
          Repeater { id: rep; objectName: "rep"; model: lm; delegate: ch }
        }
        """

    Scenario: Each row has the delegate of the first choice it matches
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,D1,I2c"
      And the property "count" of "rep" is 3
      And the property "made" of "root" is 3

    Scenario: A change of the chooser's role makes the row's delegate again
      When the method "setProperty" of "lm" is called with 1, "kind", "a"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,A1,I2c"
      And the property "made" of "root" is 4

    Scenario: Writing the same value to the chooser's role still makes the delegate again
      When the method "setProperty" of "lm" is called with 0, "kind", "a"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,D1,I2c"
      And the property "made" of "root" is 4

    Scenario: A change of another role keeps the delegate
      When the method "setProperty" of "lm" is called with 0, "n", 7
      Then the property "made" of "root" is 3

    Scenario: A choice by index is made when the delegate is, and kept when the row moves
      When the method "insert" of "lm" is called with 0, {"kind": "c", "n": 0}
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "D0,A1,D2,I3c"
      And the property "made" of "root" is 4

    Scenario: A new role takes effect when a row's data changes
      When the property "role" of "ch" is set to "n"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,D1,I2c"
      And the property "made" of "root" is 3

    Scenario: A new role takes effect for every row when the model is reset
      When the property "role" of "ch" is set to "n"
      And the method "reset" of "root" is called
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "D0,D1,I2c"

  Rule: A choice matches when every constraint it has matches

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < rep.count; ++i) { var o = rep.itemAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          function useNumber() { rep.delegate = byNumber }
          ListModel { id: lm
            ListElement { kind: "a"; n: 1 }
            ListElement { kind: "b"; n: 2 }
            ListElement { kind: "c"; n: 3 }
          }
          DelegateChooser {
            id: byKind; role: "kind"
            DelegateChoice { roleValue: "b"; row: 5; Item { property string tag: "B5" } }
            DelegateChoice { roleValue: "b"; Item { property string tag: "B" + index } }
            DelegateChoice { roleValue: "a"; Item { property string tag: "A" + index } }
          }
          DelegateChooser {
            id: byNumber; role: "n"
            DelegateChoice { roleValue: 2; Item { property string tag: "two" } }
            DelegateChoice { Item { property string tag: "other" } }
          }
          Repeater { id: rep; objectName: "rep"; model: lm; delegate: byKind }
        }
        """

    Scenario: A choice whose row does not match is passed over
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,B1,none"

    Scenario: A row no choice matches has no delegate and still counts
      When the method "show" of "root" is called
      Then the property "count" of "rep" is 3

    Scenario: A number matches the role's value
      When the method "useNumber" of "root" is called
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "other,two,other"

  Rule: Every view that takes a delegate takes a chooser

    Scenario: An Instantiator makes the chosen object for each row
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < inst.count; ++i) { var o = inst.objectAt(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { kind: "a" }
            ListElement { kind: "b" }
          }
          Instantiator {
            id: inst
            model: lm
            delegate: DelegateChooser {
              role: "kind"
              DelegateChoice { roleValue: "a"; QtObject { property string tag: "A" + index } }
              DelegateChoice { QtObject { property string tag: "D" + index } }
            }
          }
        }
        """
      When the method "setProperty" of "lm" is called with 1, "kind", "a"
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,A1"

    Scenario Outline: A <view> shows the chosen delegate for each row
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          width: 200; height: 300
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < view.count; ++i) { var o = view.itemAtIndex(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { kind: "a" }
            ListElement { kind: "b" }
            ListElement { kind: "c" }
          }
          <view> {
            id: view; anchors.fill: parent; model: lm
            delegate: DelegateChooser {
              role: "kind"
              DelegateChoice { roleValue: "a"; Item { width: 50; height: 20; property string tag: "A" + index } }
              DelegateChoice { roleValue: "b"; Item { width: 50; height: 20; property string tag: "B" + index } }
              DelegateChoice { Item { width: 50; height: 20; property string tag: "D" + index } }
            }
          }
        }
        """
      When the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,B1,D2"

      Examples:
        | view     |
        | ListView |
        | GridView |

    Scenario Outline: A <view> makes a row's delegate again when the chooser's role changes
      Given the QML:
        """qml
        import QtQuick
        import QtQml.Models
        Item {
          id: root; objectName: "root"
          width: 200; height: 300
          property string shown: ""
          function show() { var s = []; for (var i = 0; i < view.count; ++i) { var o = view.itemAtIndex(i); s.push(o ? o.tag : "none") } shown = s.join(",") }
          ListModel {
            id: lm; objectName: "lm"
            ListElement { kind: "a" }
            ListElement { kind: "b" }
          }
          <view> {
            id: view; anchors.fill: parent; model: lm
            delegate: DelegateChooser {
              role: "kind"
              DelegateChoice { roleValue: "a"; Item { width: 50; height: 20; property string tag: "A" + index } }
              DelegateChoice { Item { width: 50; height: 20; property string tag: "D" + index } }
            }
          }
        }
        """
      When the method "setProperty" of "lm" is called with 1, "kind", "a"
      And 50 milliseconds pass
      And the method "show" of "root" is called
      Then the property "shown" of "root" is "A0,A1"

      Examples:
        | view     |
        | ListView |
        | GridView |
