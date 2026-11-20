# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Items an Instantiator adds to a menu
  The items declared in a menu are its own from the start, so the index
  an `onObjectAdded` handler gives counts them, even when it runs before
  the menu is complete.

  Rule: Items inserted after a declared item

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string seen: ""
          function look() {
            var s = []; for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text); seen = s.join(",")
          }
          function grow() { lm.append({ name: "P3" }); look() }
          function shrink() { lm.remove(0); look() }
          ListModel { id: lm; ListElement { name: "P1" } ListElement { name: "P2" } }
          Menu {
            id: m
            MenuItem { text: "All" }
            Instantiator {
              model: lm
              delegate: MenuItem { text: name }
              onObjectAdded: (index, object) => m.insertItem(index + 1, object)
              onObjectRemoved: (index, object) => m.removeItem(object)
            }
            MenuItem { text: "End" }
          }
          Component.onCompleted: look()
        }
        """

    @api-QtQuick.Controls.Menu.insertItem
    Scenario: Items inserted while the menu is made count the declared items
      Then the property "seen" of "root" is "All,P1,P2,End"

    @api-QtQuick.Controls.Menu.insertItem
    Scenario: A row added to the model later is inserted at its place
      When the method "grow" of "root" is called
      Then the property "seen" of "root" is "All,P1,P2,P3,End"

    @api-QtQuick.Controls.Menu.removeItem
    Scenario: A row removed from the model takes its item out
      When the method "shrink" of "root" is called
      Then the property "seen" of "root" is "All,P2,End"

  Rule: Items added after declared actions and menus

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string seen: ""
          Menu {
            id: m
            Action { text: "Act" }
            Instantiator {
              model: ["P1", "P2"]
              delegate: MenuItem { required property var modelData; text: modelData }
              onObjectAdded: (index, object) => m.addItem(object)
            }
            Menu { title: "Sub" }
          }
          Component.onCompleted: {
            var s = []
            for (var i = 0; i < m.count; i++) s.push(m.itemAt(i).text)
            seen = s.join(",")
          }
        }
        """

    @api-QtQuick.Controls.Menu.addItem
    Scenario: Items added while the menu is made come after every declared item
      Then the property "seen" of "root" is "Act,Sub,P1,P2"

  Rule: Methods called while the menu is made

    @api-QtQuick.Controls.Menu.takeAction
    Scenario: A declared action taken out while the menu is made stays out
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string took: ""
          property string seen: ""
          Menu {
            id: a
            Action { text: "Act" }
            MenuItem { text: "B" }
            Instantiator {
              model: 1; delegate: QtObject {}
              onObjectAdded: { var t = a.takeAction(0); root.took = t.text + " " + a.count }
            }
          }
          Component.onCompleted: Qt.callLater(function() { seen = a.count + " " + a.itemAt(0).text })
        }
        """
      Then the property "took" of "root" is "Act 1"
      And the property "seen" of "root" is "1 B"

    @api-QtQuick.Controls.Menu.takeItem
    Scenario: Taking nothing out while the menu is made counts the declared items
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property string rest: ""
          property string seen: ""
          Menu {
            id: b
            MenuItem { text: "A" }
            Instantiator {
              model: 1; delegate: QtObject {}
              onObjectAdded: { b.takeItem(-1); root.rest = "" + b.count }
            }
          }
          Component.onCompleted: Qt.callLater(function() { seen = b.count + " " + b.itemAt(0).text })
        }
        """
      Then the property "rest" of "root" is "1"
      And the property "seen" of "root" is "1 A"

  Rule: A delegate that adds an item while it is made

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property bool done: false
          property string seen: ""
          MenuItem { id: extra; text: "Extra" }
          Menu {
            id: c
            delegate: MenuItem { Component.onCompleted: if (!root.done) { root.done = true; c.addItem(extra) } }
            Action { text: "Act" }
            Instantiator {
              model: 1; delegate: QtObject {}
              onObjectAdded: { c.addItem(gen) }
            }
            MenuItem { id: gen; text: "Gen" }
          }
          Component.onCompleted: Qt.callLater(function() {
            var s = []; for (var i = 0; i < c.count; i++) s.push(c.itemAt(i).text); seen = s.join(",")
          })
        }
        """

    @api-QtQuick.Controls.Menu.addItem @api-QtQuick.Controls.Menu.delegate
    Scenario: The item the delegate adds comes before the action's item
      Then the property "seen" of "root" is "Extra,Act,Gen"
