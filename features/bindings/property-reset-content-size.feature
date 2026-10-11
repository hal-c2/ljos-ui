# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pane.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pane.html#contentHeight-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#contentWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The content size of a pane, and taking it back with undefined
  A pane that was given no contentWidth or contentHeight has the size its
  content asks for, and its own implicit size is built on that. A given
  size is kept, and assigning undefined takes it back. A popup has the
  two properties as well and does not take them back.

  Rule: Undefined gives a pane the size of its content again

    Scenario Outline: A <type> has what its content asks for, and follows it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          <type> {
            id: p; contentWidth: 300; contentHeight: 200
            Rectangle { id: r; implicitWidth: 40; implicitHeight: 20 }
          }
          property string seen: p.contentWidth + "," + p.contentHeight
          function back() { p.contentWidth = undefined; p.contentHeight = undefined }
          function grow() { r.implicitWidth = 60; r.implicitHeight = 30 }
        }
        """
      Then the property "seen" of "root" is "300,200"
      When the method "back" of "root" is called
      Then the property "seen" of "root" is "40,20"
      When the method "grow" of "root" is called
      Then the property "seen" of "root" is "60,30"

      Examples:
        | type       |
        | Pane       |
        | Frame      |
        | GroupBox   |
        | Page       |
        | ScrollView |

    Scenario: The implicit size of the pane is built on the size taken back
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          Pane {
            id: p; padding: 10; contentWidth: 300; contentHeight: 200
            Rectangle { implicitWidth: 40; implicitHeight: 20 }
          }
          property string asks: p.implicitWidth + "," + p.implicitHeight
          function back() { p.contentWidth = undefined; p.contentHeight = undefined }
        }
        """
      Then the property "asks" of "root" is "320,220"
      When the method "back" of "root" is called
      Then the property "asks" of "root" is "60,40"

    Scenario: The change is signalled once, and taking back twice is the same as once
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          Pane {
            id: p; contentWidth: 300
            onContentWidthChanged: root.changes++
            Rectangle { implicitWidth: 40; implicitHeight: 20 }
          }
          Pane {
            id: plain
            onContentWidthChanged: root.others++
            Rectangle { implicitWidth: 40; implicitHeight: 20 }
          }
          property int changes: 0
          property int others: 0
          property string seen
          function back() {
            var n = others
            p.contentWidth = undefined; plain.contentWidth = undefined
            seen += p.contentWidth + "," + changes + "," + (others - n) + ";"
          }
        }
        """
      When the method "back" of "root" is called
      And the method "back" of "root" is called
      Then the property "seen" of "root" is "40,1,0;40,1,0;"

    Scenario: A binding that yields undefined takes it back, and a value gives it again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property bool given: true
          Pane {
            id: p; contentHeight: root.given ? 200 : undefined
            Rectangle { implicitWidth: 40; implicitHeight: 20 }
          }
          property real high: p.contentHeight
        }
        """
      Then the property "high" of "root" is 200
      When the property "given" of "root" is set to false
      Then the property "high" of "root" is 20
      When the property "given" of "root" is set to true
      Then the property "high" of "root" is 200

    Scenario: A button box that was taken back has what its buttons ask for when they change
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          DialogButtonBox { id: box; standardButtons: DialogButtonBox.Ok }
          property real one: -1
          property real wide: box.contentWidth
          property bool between: wide > one && wide < 321
          function give() { one = box.contentWidth; box.contentWidth = 321 }
          function back() { box.contentWidth = undefined }
          function more() { box.standardButtons = DialogButtonBox.Ok | DialogButtonBox.Cancel }
        }
        """
      When the method "give" of "root" is called
      Then the property "wide" of "root" is 321
      When the method "back" of "root" is called
      And the method "more" of "root" is called
      Then the property "between" of "root" is true

  Rule: Only undefined takes it back

    Scenario: null is the number zero, and the pane keeps it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          Pane {
            id: p; contentWidth: 300
            Rectangle { id: r; implicitWidth: 40; implicitHeight: 20 }
          }
          property real wide: p.contentWidth
          function clear() { p.contentWidth = null; r.implicitWidth = 60 }
        }
        """
      When the method "clear" of "root" is called
      Then the property "wide" of "root" is 0

  Rule: A popup does not take its content size back

    Scenario Outline: A <type> refuses undefined and keeps the size it was given
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          <type> { id: p; contentWidth: 123; contentHeight: 45 }
          property string said
          function clear() {
            try { p.contentWidth = undefined } catch (e) { said = String(e) }
            try { p.contentHeight = undefined } catch (e) { said += "|" + e }
            said += "|" + p.contentWidth + "," + p.contentHeight
          }
        }
        """
      When the method "clear" of "root" is called
      Then the property "said" of "root" is "<said>"

      Examples:
        | type      | said                                                                                         |
        | Popup     | Error: Cannot assign [undefined] to double\|Error: Cannot assign [undefined] to double\|123,45 |
        | Dialog    | Error: Cannot assign [undefined] to double\|Error: Cannot assign [undefined] to double\|123,45 |
        | Menu      | Error: Cannot assign [undefined] to double\|Error: Cannot assign [undefined] to double\|123,45 |
        | Flickable | Error: Cannot assign [undefined] to double\|Error: Cannot assign [undefined] to double\|123,45 |
