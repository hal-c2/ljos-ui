# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#padding-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#horizontalPadding-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#topInset-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#hoverEnabled-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Resetting a property of a control with undefined
  A control's paddings stand in a row: a side follows its axis and an axis
  follows the padding, each until it is given a value. Assigning undefined
  takes that value back. Insets, spacing and hovering go back to what a
  bare control has.

  Rule: A padding follows the wider one again

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Control {
          objectName: "c"
          padding: 10
          horizontalPadding: 7; verticalPadding: 8
          leftPadding: 4; topPadding: 3
          function side() { leftPadding = undefined; topPadding = undefined }
          function axis() {
            horizontalPadding = undefined; verticalPadding = undefined
          }
          function all() { padding = undefined }
        }
        """

    Scenario: A side follows its axis
      When the method "side" of "c" is called
      Then the property "leftPadding" of "c" is 7
      And the property "topPadding" of "c" is 8
      When the property "horizontalPadding" of "c" is set to 9
      Then the property "leftPadding" of "c" is 9

    Scenario: An axis follows the padding, and a side that has a value keeps it
      When the method "axis" of "c" is called
      Then the property "horizontalPadding" of "c" is 10
      And the property "verticalPadding" of "c" is 10
      And the property "rightPadding" of "c" is 10
      And the property "bottomPadding" of "c" is 10
      And the property "leftPadding" of "c" is 4
      When the property "padding" of "c" is set to 12
      Then the property "rightPadding" of "c" is 12

    Scenario: The padding goes back to none
      When the method "side" of "c" is called
      And the method "axis" of "c" is called
      And the method "all" of "c" is called
      Then the property "padding" of "c" is 0
      And the property "leftPadding" of "c" is 0
      And the property "bottomPadding" of "c" is 0

  Rule: A binding that yields undefined takes the padding back for as long

    Scenario: A side bound to undefined follows the padding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          property bool own: true
          Control {
            objectName: "c"
            padding: 10
            rightPadding: root.own ? 2 : undefined
          }
        }
        """
      Then the property "rightPadding" of "c" is 2
      When the property "own" of "root" is set to false
      Then the property "rightPadding" of "c" is 10
      And the log has no warnings
      When the property "padding" of "c" is set to 11
      Then the property "rightPadding" of "c" is 11
      When the property "own" of "root" is set to true
      Then the property "rightPadding" of "c" is 2

  Rule: What a style gave is taken back too

    Scenario: A button without its style's padding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button {
          objectName: "go"
          text: "Go"
          function bare() { padding = undefined }
        }
        """
      Then the property "padding" of "go" is 6
      When the method "bare" of "go" is called
      Then the property "padding" of "go" is 0
      And the property "topPadding" of "go" is 0

    Scenario: A button is as wide as it wants to be again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button {
          objectName: "go"
          text: "Go"
          width: 200
          property bool natural: width === implicitWidth
          function loosen() { width = undefined }
        }
        """
      Then the property "natural" of "go" is false
      When the method "loosen" of "go" is called
      Then the property "natural" of "go" is true

  Rule: Other properties of a control

    Scenario Outline: <property> after undefined is <after>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Control {
          objectName: "c"
          <property>: <given>
          function clear() { <property> = undefined }
        }
        """
      Then the property "<property>" of "c" is <given>
      When the method "clear" of "c" is called
      Then the property "<property>" of "c" is <after>

      Examples:
        | property     | given | after |
        | topInset     | 5     | 0     |
        | leftInset    | 5     | 0     |
        | rightInset   | 5     | 0     |
        | bottomInset  | 5     | 0     |
        | spacing      | 9     | 0     |
        | hoverEnabled | false | true  |

    Scenario Outline: <property> has no reset and refuses undefined
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Control {
          objectName: "c"
          property string said
          <property>: <given>
          function clear() {
            try { <property> = undefined } catch (e) { said = String(e) }
          }
        }
        """
      When the method "clear" of "c" is called
      Then the property "said" of "c" is "Error: Cannot assign [undefined] to <class>"
      And the property "<property>" of "c" is <kept>

      Examples:
        | property     | given         | class           | kept |
        | wheelEnabled | true          | bool            | true |
        | focusPolicy  | Qt.ClickFocus | Qt::FocusPolicy | 2    |

    Scenario Outline: <property> of a <type> after undefined is <after>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        <type> {
          objectName: "c"
          <property>: <given>
          function clear() { <property> = undefined }
        }
        """
      Then the property "<property>" of "c" is <given>
      When the method "clear" of "c" is called
      Then the property "<property>" of "c" is <after>

      Examples:
        | type      | property     | given | after |
        | Label     | topInset     | 5     | 0     |
        | Label     | rightInset   | 5     | 0     |
        | TextField | leftInset    | 5     | 0     |
        | TextField | bottomInset  | 5     | 0     |
        | TextField | hoverEnabled | false | true  |
        | TextArea  | topInset     | 5     | 0     |
        | TextArea  | rightInset   | 5     | 0     |
        | TextArea  | hoverEnabled | false | true  |

  Rule: A popup takes back what a control does, and its place

    Scenario Outline: <property> of a popup after undefined is <after>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 200; height: 200
          Popup {
            objectName: "p"
            <given>
            function clear() { <property> = undefined }
          }
        }
        """
      Then the property "<property>" of "p" is <before>
      When the method "clear" of "p" is called
      Then the property "<property>" of "p" is <after>

      Examples:
        | given                                             | property          | before | after |
        | padding: 10; horizontalPadding: 7; leftPadding: 4 | leftPadding       | 4      | 7     |
        | padding: 10; horizontalPadding: 7                 | horizontalPadding | 7      | 10    |
        | padding: 10; verticalPadding: 7; topPadding: 3    | topPadding        | 3      | 7     |
        | padding: 10; verticalPadding: 7                   | verticalPadding   | 7      | 10    |
        | padding: 10                                       | padding           | 10     | 0     |
        | topInset: 5                                       | topInset          | 5      | 0     |
        | rightInset: 5                                     | rightInset        | 5      | 0     |
        | spacing: 9                                        | spacing           | 9      | 0     |
        | margins: 6; leftMargin: 2                         | leftMargin        | 2      | 6     |
        | margins: 6; bottomMargin: 2                       | bottomMargin      | 2      | 6     |
        | margins: 6                                        | margins           | 6      | -1    |
        | z: 3                                              | z                 | 3      | 0     |
        | closePolicy: Popup.NoAutoClose                    | closePolicy       | 0      | 17    |
        | dim: true                                         | dim               | true   | false |
        | modal: true; dim: false                           | dim               | false  | true  |
        | implicitWidth: 40; width: 90                      | width             | 90     | 40    |
        | implicitHeight: 30; height: 80                    | height            | 80     | 30    |

    Scenario: A popup goes back to the item it was written in
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 200; height: 200
          property bool home: popup.parent === root
          Item { id: other }
          Popup {
            id: popup
            parent: other
          }
          function back() { popup.parent = undefined }
        }
        """
      Then the property "home" of "root" is false
      When the method "back" of "root" is called
      Then the property "home" of "root" is true

    Scenario: A popup is no longer centred
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 200; height: 200
          property bool centred: popup.anchors.centerIn === root
          Popup {
            id: popup
            anchors.centerIn: parent
          }
          function loosen() { popup.anchors.centerIn = undefined }
        }
        """
      Then the property "centred" of "root" is true
      When the method "loosen" of "root" is called
      Then the property "centred" of "root" is false
