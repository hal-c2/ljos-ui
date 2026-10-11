# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialog.html#standardButtons-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: DialogButtonBox
  The row of buttons at the foot of a dialog. Each button has a role; the
  role says where in the row the button goes on this platform and what
  the box tells when it is clicked.

  Rule: An empty box

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          property string roles: [DialogButtonBox.InvalidRole, DialogButtonBox.AcceptRole,
            DialogButtonBox.RejectRole, DialogButtonBox.DestructiveRole,
            DialogButtonBox.ActionRole, DialogButtonBox.HelpRole, DialogButtonBox.YesRole,
            DialogButtonBox.NoRole, DialogButtonBox.ResetRole, DialogButtonBox.ApplyRole].join(" ")
          property string layouts: [DialogButtonBox.WinLayout, DialogButtonBox.MacLayout,
            DialogButtonBox.KdeLayout, DialogButtonBox.GnomeLayout,
            DialogButtonBox.AndroidLayout].join(" ")
          property string places: DialogButtonBox.Header + " " + DialogButtonBox.Footer
          DialogButtonBox { objectName: "box" }
        }
        """

    @api-QtQuick.Controls.DialogButtonBox
    Scenario Outline: A button box starts with <property> <value>
      Then the property "<property>" of "box" is <value>

      Examples:
        | property          | value     |
        | count             | 0         |
        | standardButtons   | 0         |
        | position          | 1         |
        | alignment         | 0         |
        | padding           | 12        |
        | spacing           | 1         |
        | implicitWidth     | 24        |
        | implicitHeight    | 40        |
        | background.x      | 1         |
        | background.y      | 1         |
        | background.width  | 22        |
        | background.height | 38        |
        | background.color  | "#ffffff" |

    # The Linux desktops the oracle runs on order buttons as GNOME does.
    @api-QtQuick.Controls.DialogButtonBox.buttonLayout
    Scenario: On a Linux desktop the buttons are ordered as GNOME orders them
      Then the property "buttonLayout" of "box" is 3

    Scenario: The roles, the layouts and the two places have these values
      Then the property "roles" of "root" is "-1 0 1 2 3 4 5 6 7 8"
      And the property "layouts" of "root" is "0 1 2 3 4"
      And the property "places" of "root" is "0 1"

  Rule: The role of a button says where it goes

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string order: ""
          function look() {
            var s = []; for (var i = 0; i < box.count; i++) s.push(box.itemAt(i).text)
            order = s.join(" ")
          }
          DialogButtonBox {
            id: box; objectName: "box"
            Button { text: "plain" }
            Button { text: "action"; DialogButtonBox.buttonRole: DialogButtonBox.ActionRole }
            Button { text: "accept"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
            Button { text: "reject"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            Button { text: "destroy"; DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole }
            Button { text: "help"; DialogButtonBox.buttonRole: DialogButtonBox.HelpRole }
            Button { text: "yes"; DialogButtonBox.buttonRole: DialogButtonBox.YesRole }
            Button { text: "no"; DialogButtonBox.buttonRole: DialogButtonBox.NoRole }
            Button { text: "reset"; DialogButtonBox.buttonRole: DialogButtonBox.ResetRole }
            Button { text: "apply"; DialogButtonBox.buttonRole: DialogButtonBox.ApplyRole }
            Button { text: "also"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
          }
        }
        """

    @api-QtQuick.Controls.DialogButtonBox.buttonLayout
    Scenario Outline: The order of the buttons as <platform> has it
      When the property "buttonLayout" of "box" is set to <layout>
      And the method "look" of "root" is called
      Then the property "order" of "root" is "<order>"

      Examples:
        | platform | layout | order                                                             |
        | Windows  | 0      | reset yes accept also destroy no action reject apply help plain   |
        | macOS    | 1      | help reset apply action destroy reject accept also no yes plain   |
        | KDE      | 2      | help reset yes no action accept also apply destroy reject plain   |
        | GNOME    | 3      | help reset action apply destroy reject accept also no yes plain   |
        | Android  | 4      | help reset apply action reject no destroy accept also yes plain   |

    @api-QtQuick.Controls.DialogButtonBox.count
    Scenario: Every button counts
      Then the property "count" of "box" is 11

  Rule: The buttons share the row

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          component B: Button { background: Rectangle { implicitWidth: 60; implicitHeight: 40 } }
          Column {
            DialogButtonBox {
              objectName: "two"; width: 400; height: 100
              B { objectName: "ok"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
              B { objectName: "no"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            }
            DialogButtonBox {
              objectName: "one"; width: 400; height: 100
              B { objectName: "only" }
            }
            DialogButtonBox {
              objectName: "fit"
              B { objectName: "first" }
              B { background: Rectangle { implicitWidth: 90; implicitHeight: 50 } }
              B { objectName: "last" }
            }
          }
        }
        """

    @api-QtQuick.Controls.DialogButtonBox.implicitContentWidth
    Scenario: A box asks for room for its buttons, each as wide as the widest and as tall as the tallest
      Then the property "implicitWidth" of "fit" is 296
      And the property "implicitHeight" of "fit" is 74
      And the property "width" of "first" is 90
      And the property "height" of "first" is 50
      And the property "x" of "first" is 0
      And the property "contentItem.x" of "fit" is 12
      And the property "contentItem.width" of "fit" is 272

    Scenario: A box with one button asks for twice the room and puts the button at its right
      Then the property "implicitWidth" of "one" is 144
      And the property "alignment" of "one" is 2
      And the property "contentItem.x" of "one" is 328
      And the property "contentItem.y" of "one" is 30
      And the property "contentItem.width" of "one" is 60
      And the property "width" of "only" is 60
      And the property "height" of "only" is 40

    Scenario: The buttons of a wider box share its width
      Then the property "implicitWidth" of "two" is 145
      And the property "implicitHeight" of "two" is 64
      And the property "contentItem.x" of "two" is 12
      And the property "contentItem.y" of "two" is 12
      And the property "contentItem.width" of "two" is 376
      And the property "contentItem.height" of "two" is 76
      And the property "x" of "no" is 0
      And the property "width" of "no" is 187.5
      And the property "height" of "no" is 40
      And the property "x" of "ok" is 188.5
      And the property "width" of "ok" is 187.5

  # Qt places the buttons when the box is made and when its size changes;
  # the boxes here are declared as they are meant to be.
  Rule: Where the buttons of a box go

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          component B: Button { background: Rectangle { implicitWidth: 60; implicitHeight: 40 } }
          component Two: DialogButtonBox { width: 400; height: 100; B {} B {} }
          Column {
            Two { objectName: "left"; alignment: Qt.AlignLeft }
            Two { objectName: "right"; alignment: Qt.AlignRight }
            Two { objectName: "middle"; alignment: Qt.AlignHCenter }
            Two { objectName: "top"; alignment: Qt.AlignTop }
            Two { objectName: "bottom"; alignment: Qt.AlignBottom }
            Two { objectName: "half"; alignment: Qt.AlignVCenter }
            Two { objectName: "corner"; alignment: Qt.AlignBottom | Qt.AlignRight }
            DialogButtonBox {
              objectName: "gap"; width: 400; spacing: 10
              B { objectName: "g1" } B { objectName: "g2" }
            }
          }
        }
        """

    @api-QtQuick.Controls.DialogButtonBox.alignment
    Scenario Outline: Buttons aligned <where> are at <x>, <y> and <width> wide together
      When 50 milliseconds pass
      Then the property "contentItem.x" of "<box>" is <x>
      And the property "contentItem.y" of "<box>" is <y>
      And the property "contentItem.width" of "<box>" is <width>
      And the property "contentItem.height" of "<box>" is 40

      Examples:
        | where                      | box    | x     | y  | width |
        | left                       | left   | 12    | 30 | 121   |
        | right                      | right  | 267   | 30 | 121   |
        | in the middle              | middle | 139.5 | 30 | 121   |
        | at the top                 | top    | 12    | 12 | 376   |
        | at the bottom              | bottom | 12    | 48 | 376   |
        | half way down              | half   | 12    | 30 | 376   |
        | at the bottom on the right | corner | 267   | 48 | 121   |

    @api-QtQuick.Controls.DialogButtonBox.spacing
    Scenario: The spacing is the gap between the buttons
      When 50 milliseconds pass
      Then the property "width" of "g1" is 183
      And the property "x" of "g2" is 193
      And the property "width" of "g2" is 183

  Rule: Standard buttons

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string log: ""
          property string texts: ""
          function look() {
            var s = []
            for (var i = 0; i < box.count; i++)
              s.push(box.itemAt(i).text + ":" + box.itemAt(i).DialogButtonBox.buttonRole)
            texts = s.join(", ")
          }
          function press(i) { box.itemAt(i).clicked() }
          DialogButtonBox {
            id: box
            objectName: "box"
            standardButtons: DialogButtonBox.Ok | DialogButtonBox.Cancel
            onClicked: (b) => root.log += "clicked:" + b.text + " "
            onAccepted: root.log += "accepted "
            onRejected: root.log += "rejected "
          }
        }
        """

    @api-QtQuick.Controls.DialogButtonBox.standardButtons
    Scenario: A box makes its standard buttons and puts them in order
      When the method "look" of "root" is called
      Then the property "texts" of "root" is "Cancel:1, OK:0"
      And the property "count" of "box" is 2
      And the property "implicitWidth" of "box" is 225
      And the property "implicitHeight" of "box" is 64

    Scenario Outline: The standard button <name> reads "<text>" and has the role <role>
      When the property "standardButtons" of "box" is set to <flag>
      And the method "look" of "root" is called
      Then the property "texts" of "root" is "<text>:<role>"

      Examples:
        | name            | flag      | text                 | role |
        | Ok              | 1024      | OK                   | 0    |
        | Save            | 2048      | Save                 | 0    |
        | SaveAll         | 4096      | Save All             | 0    |
        | Open            | 8192      | Open                 | 0    |
        | Yes             | 16384     | Yes                  | 5    |
        | YesToAll        | 32768     | Yes to All           | 5    |
        | No              | 65536     | No                   | 6    |
        | NoToAll         | 131072    | No to All            | 6    |
        | Abort           | 262144    | Abort                | 1    |
        | Retry           | 524288    | Retry                | 0    |
        | Ignore          | 1048576   | Ignore               | 0    |
        | Close           | 2097152   | Close                | 1    |
        | Cancel          | 4194304   | Cancel               | 1    |
        | Discard         | 8388608   | Close without Saving | 2    |
        | Help            | 16777216  | Help                 | 4    |
        | Apply           | 33554432  | Apply                | 8    |
        | Reset           | 67108864  | Reset                | 7    |
        | RestoreDefaults | 134217728 | Restore Defaults     | 7    |

    Scenario: No standard buttons leaves the box empty
      When the property "standardButtons" of "box" is set to 0
      Then the property "count" of "box" is 0

    @api-QtQuick.Controls.DialogButtonBox.clicked
    Scenario: A click tells which button it was, and then what the role means
      When the method "press" of "root" is called with 1
      Then the property "log" of "root" is "clicked:OK accepted "

    @api-QtQuick.Controls.DialogButtonBox.rejected
    Scenario: The button that rejects
      When the method "press" of "root" is called with 0
      Then the property "log" of "root" is "clicked:Cancel rejected "

    @api-QtQuick.Controls.DialogButtonBox.accepted
    Scenario Outline: A click on <name> tells "<log>"
      When the property "standardButtons" of "box" is set to <flag>
      And the method "press" of "root" is called with 0
      Then the property "log" of "root" is "clicked:<name> <log> "

      Examples:
        | name | flag  | log      |
        | Yes  | 16384 | accepted |
        | No   | 65536 | rejected |

    Scenario: The user clicks a standard button
      When the user touches 160, 32
      Then the property "log" of "root" is "clicked:OK accepted "

  Rule: Buttons come and go

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string seen: ""
          function look() {
            var s = []; for (var i = 0; i < box.count; i++) s.push(box.itemAt(i).text)
            seen = s.join(" ") + " | " + (a.DialogButtonBox.buttonBox === box) + " "
              + (spare.DialogButtonBox.buttonBox === box) + " " + spare.DialogButtonBox.buttonRole
          }
          function add() { box.addItem(spare) }
          function take() { box.takeItem(0) }
          function remove() { box.removeItem(a) }
          function swap() { h.DialogButtonBox.buttonRole = DialogButtonBox.AcceptRole }
          Button { id: spare; text: "spare" }
          DialogButtonBox {
            id: box
            objectName: "box"
            Button { id: a; objectName: "a"; text: "a"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
            Button { id: h; text: "h"; DialogButtonBox.buttonRole: DialogButtonBox.HelpRole }
          }
        }
        """

    @api-QtQuick.Controls.DialogButtonBox.buttonBox
    Scenario: A button knows the box it is in
      When the method "look" of "root" is called
      Then the property "seen" of "root" is "h a | true false -1"

    @api-QtQuick.Controls.DialogButtonBox.addItem
    Scenario: A button added to the box goes where its role puts it
      When the method "add" of "root" is called
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "h a spare | true true -1"
      And the property "count" of "box" is 3

    @api-QtQuick.Controls.DialogButtonBox.takeItem
    Scenario: A button taken out is no longer in the box
      When the method "take" of "root" is called
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "a | true false -1"

    @api-QtQuick.Controls.DialogButtonBox.removeItem
    Scenario: A button removed is no longer in the box
      When the method "remove" of "root" is called
      Then the property "count" of "box" is 1

    @api-QtQuick.Controls.DialogButtonBox.buttonRole
    Scenario: A button given another role stays where it was put
      When the method "swap" of "root" is called
      And 50 milliseconds pass
      And the method "look" of "root" is called
      Then the property "seen" of "root" is "h a | true false -1"
