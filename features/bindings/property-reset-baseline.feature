# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#baselineOffset-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#control-layout
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#contentItem-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The baseline of a control, and taking it back with undefined
  A control that was given no baselineOffset has the baseline of its
  content item, moved down by the top padding, so that controls line up
  on their text. A given offset is kept, and assigning undefined takes it
  back. An item that is not a control has no baseline to take back.

  Rule: Without one a control has the baseline of its content

    Scenario: The baseline is the content's, moved down by the top padding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          Control { id: c; objectName: "c"; topPadding: 5; contentItem: Text { text: "x" } }
          Control { id: flat; topPadding: 5; contentItem: Item { implicitHeight: 10 } }
          Control { id: marked; topPadding: 5; contentItem: Item { baselineOffset: 3 } }
          Control { id: bare; topPadding: 5 }
          property bool written: c.contentItem.baselineOffset > 0
          property real over: c.baselineOffset - c.contentItem.baselineOffset
          property string others: flat.baselineOffset + "," + marked.baselineOffset + "," + bare.baselineOffset
        }
        """
      Then the property "written" of "root" is true
      And the property "over" of "root" is 5
      And the property "others" of "root" is "5,8,0"
      When the property "topPadding" of "c" is set to 8
      Then the property "over" of "root" is 8

    Scenario: Text centred in a taller control carries its baseline down
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          Text { id: plain; text: "x" }
          Control {
            id: c; padding: 4; height: 100
            contentItem: Text { text: "x"; verticalAlignment: Text.AlignVCenter }
          }
          property real inside: c.contentItem.baselineOffset - plain.baselineOffset
          property real over: c.baselineOffset - c.contentItem.baselineOffset
          property bool centred: Math.abs(inside - (92 - plain.height) / 2) < 1
        }
        """
      Then the property "centred" of "root" is true
      And the property "over" of "root" is 4

    Scenario Outline: A <type> has the baseline of its content
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          <type> { id: c; <given> }
          property bool written: c.baselineOffset > 0
          property real over: c.baselineOffset - c.topPadding - c.contentItem.baselineOffset
        }
        """
      Then the property "written" of "root" is true
      And the property "over" of "root" is 0

      Examples:
        | type     | given        |
        | Button   | text: "x"    |
        | CheckBox | text: "x"    |
        | Switch   | text: "x"    |
        | SpinBox  | value: 1     |
        | ComboBox | model: ["x"] |

  Rule: A given baseline is kept until undefined takes it back

    Scenario: A given baseline does not follow the content, and zero is one
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          Control { id: c; objectName: "c"; topPadding: 5; baselineOffset: 2; contentItem: Text { text: "x" } }
          Control { id: z; objectName: "z"; topPadding: 5; contentItem: Text { text: "x" } }
          property string seen: c.baselineOffset + "," + z.baselineOffset
          function zero() { z.baselineOffset = 0 }
        }
        """
      When the method "zero" of "root" is called
      And the property "topPadding" of "c" is set to 8
      And the property "topPadding" of "z" is set to 8
      Then the property "seen" of "root" is "2,0"

    Scenario: Undefined gives the content's baseline back, and says so once
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          Control {
            id: c; objectName: "c"; topPadding: 5; baselineOffset: 2
            contentItem: Text { text: "x" }
            onBaselineOffsetChanged: root.changes++
          }
          property int changes: 0
          property string seen
          property real over: c.baselineOffset - c.contentItem.baselineOffset
          function back() {
            c.baselineOffset = undefined
            seen += (c.baselineOffset - c.contentItem.baselineOffset) + "," + changes + ";"
          }
        }
        """
      When the method "back" of "root" is called
      And the method "back" of "root" is called
      Then the property "seen" of "root" is "5,1;5,1;"
      When the property "topPadding" of "c" is set to 8
      Then the property "over" of "root" is 8

    Scenario: A binding that yields undefined takes it back, and a value gives it again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property bool given: true
          Control {
            id: c; topPadding: 5; baselineOffset: root.given ? 2 : undefined
            contentItem: Text { text: "x" }
          }
          property real over: c.baselineOffset - c.contentItem.baselineOffset
          property real at: c.baselineOffset
        }
        """
      Then the property "at" of "root" is 2
      When the property "given" of "root" is set to false
      Then the property "over" of "root" is 5
      When the property "given" of "root" is set to true
      Then the property "at" of "root" is 2

    Scenario Outline: A <type> takes its baseline back
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          <type> { id: c; <given> }
          property real was: -1
          property real given: -1
          property bool same: false
          function keep() { was = c.baselineOffset; c.baselineOffset = 1; given = c.baselineOffset }
          function back() { c.baselineOffset = undefined; same = was > 1 && c.baselineOffset === was }
        }
        """
      When the method "keep" of "root" is called
      Then the property "given" of "root" is 1
      When the method "back" of "root" is called
      Then the property "same" of "root" is true

      Examples:
        | type     | given        |
        | Button   | text: "x"    |
        | CheckBox | text: "x"    |
        | Switch   | text: "x"    |
        | SpinBox  | value: 1     |
        | ComboBox | model: ["x"] |

  Rule: Only a control takes its baseline back

    Scenario Outline: A <type> refuses undefined and keeps its baseline
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          <type> { id: t }
          property string said
          property bool kept: false
          function clear() {
            t.baselineOffset = 4
            try { t.baselineOffset = undefined } catch (e) { said = String(e) }
            kept = t.baselineOffset === 4
          }
        }
        """
      When the method "clear" of "root" is called
      Then the property "said" of "root" is "Error: Cannot assign [undefined] to double"
      And the property "kept" of "root" is true

      Examples:
        | type      |
        | Item      |
        | Rectangle |
        | Label     |
