# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Dialog
  A dialog is a popup with a title above its content and a row of buttons
  below it. It closes with a result: accepted or rejected.

  Rule: What a dialog is made of

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          Dialog { objectName: "plain" }
          Dialog {
            objectName: "own"; spacing: 5
            header: Rectangle { implicitWidth: 80; implicitHeight: 20 }
            footer: Rectangle { implicitWidth: 90; implicitHeight: 30 }
            Item { implicitWidth: 10; implicitHeight: 10 }
          }
        }
        """

    @api-QtQuick.Controls.Dialog
    Scenario Outline: A dialog starts with <property> <value>
      Then the property "<property>" of "plain" is <value>

      Examples:
        | property             | value |
        | title                | ""    |
        | standardButtons      | 0     |
        | result               | 0     |
        | focus                | true  |
        | modal                | false |
        | visible              | false |
        | padding              | 12    |
        | spacing              | 0     |
        | closePolicy          | 17    |
        | implicitWidth        | 24    |
        | implicitHeight       | 24    |
        | implicitHeaderHeight | 0     |
        | implicitFooterHeight | 0     |
        | footer.count         | 0     |
        | footer.position      | 1     |

    @api-QtQuick.Controls.Dialog.header
    Scenario: A dialog without a title or buttons shows neither bar
      When the method "open" of "plain" is called
      Then the property "header.visible" of "plain" is false
      And the property "footer.visible" of "plain" is false
      And the property "implicitHeight" of "plain" is 24
      And the property "contentItem.y" of "plain" is 12

    @api-QtQuick.Controls.Dialog.title
    Scenario: The title is shown in the header, in bold
      When the property "title" of "plain" is set to "Hello"
      And the method "open" of "plain" is called
      Then the property "header.visible" of "plain" is true
      And the property "header.text" of "plain" is "Hello"
      And the property "header.font.bold" of "plain" is true
      And the property "header.y" of "plain" is 0
      And the property "implicitHeaderHeight" of "plain" is at least 30

    @api-QtQuick.Controls.Dialog.implicitHeaderHeight
    Scenario: A closed dialog counts neither its header nor its footer
      Then the property "implicitWidth" of "own" is 34
      And the property "implicitHeight" of "own" is 34
      And the property "implicitHeaderHeight" of "own" is 0
      And the property "implicitFooterWidth" of "own" is 0

    @api-QtQuick.Controls.Dialog.footer
    Scenario: An open dialog is as large as its header, content and footer ask, with the spacing between them
      When the method "open" of "own" is called
      Then the property "implicitWidth" of "own" is 90
      And the property "implicitHeight" of "own" is 94
      And the property "implicitHeaderWidth" of "own" is 80
      And the property "implicitHeaderHeight" of "own" is 20
      And the property "implicitFooterWidth" of "own" is 90
      And the property "implicitFooterHeight" of "own" is 30

    Scenario Outline: In an open dialog the <part> has <property> <value>
      When the method "open" of "own" is called
      Then the property "<part>.<property>" of "own" is <value>

      Examples:
        | part        | property | value |
        | header      | x        | 0     |
        | header      | y        | 0     |
        | header      | width    | 90    |
        | header      | height   | 20    |
        | contentItem | x        | 12    |
        | contentItem | y        | 37    |
        | contentItem | width    | 66    |
        | contentItem | height   | 10    |
        | footer      | x        | 0     |
        | footer      | y        | 64    |
        | footer      | width    | 90    |
        | footer      | height   | 30    |

    @api-QtQuick.Controls.Dialog.availableHeight
    Scenario: The available size is what is inside the padding, bars included
      When the method "open" of "own" is called
      Then the property "availableWidth" of "own" is 66
      And the property "availableHeight" of "own" is 70

    Scenario: A dialog given a size puts the footer at its bottom and the content between the bars
      When the property "width" of "own" is set to 200
      And the property "height" of "own" is set to 150
      And the method "open" of "own" is called
      Then the property "header.width" of "own" is 200
      And the property "footer.y" of "own" is 120
      And the property "contentItem.y" of "own" is 37
      And the property "contentItem.width" of "own" is 176
      And the property "contentItem.height" of "own" is 66
      And the property "implicitHeight" of "own" is 94

  Rule: A dialog closes with a result

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
          Dialog {
            objectName: "d"
            standardButtons: Dialog.Ok | Dialog.Cancel
            onAccepted: root.log += "accepted "
            onRejected: root.log += "rejected "
            onAboutToHide: root.log += "aboutToHide "
            onClosed: root.log += "closed "
            onResultChanged: root.log += "result=" + result + " "
          }
        }
        """

    @api-QtQuick.Controls.Dialog.accept
    Scenario: Accepting sets the result, tells and closes
      When the method "open" of "d" is called
      And the method "accept" of "d" is called
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 1
      And the property "log" of "root" is "result=1 accepted aboutToHide closed "

    @api-QtQuick.Controls.Dialog.reject
    Scenario: Rejecting tells and closes
      When the method "open" of "d" is called
      And the method "reject" of "d" is called
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 0
      And the property "log" of "root" is "rejected aboutToHide closed "

    @api-QtQuick.Controls.Dialog.done
    Scenario Outline: Closing a dialog with the result <result>
      When the method "open" of "d" is called
      And the method "done" of "d" is called with <result>
      Then the property "visible" of "d" is false
      And the property "result" of "d" is <result>
      And the property "log" of "root" is "<log> aboutToHide closed "

      Examples:
        | result | log               |
        | 1      | result=1 accepted |
        | 0      | rejected          |
        | 5      | result=5          |

    @api-QtQuick.Controls.Dialog.result
    Scenario: Closing a dialog otherwise leaves the result of the last time
      When the method "open" of "d" is called
      And the method "accept" of "d" is called
      And the method "open" of "d" is called
      Then the property "result" of "d" is 1
      When the method "close" of "d" is called
      Then the property "result" of "d" is 1
      And the property "log" of "root" is "result=1 accepted aboutToHide closed aboutToHide closed "

    Scenario: Accepting a closed dialog sets the result and tells
      When the method "accept" of "d" is called
      Then the property "result" of "d" is 1
      And the property "log" of "root" is "result=1 accepted "

    @api-QtQuick.Controls.Dialog.standardButtons
    Scenario: The standard buttons are the buttons of the footer
      Then the property "footer.count" of "d" is 2
      And the property "footer.standardButtons" of "d" is 4195328
      And the property "implicitWidth" of "d" is 24
      When the method "open" of "d" is called
      Then the property "footer.visible" of "d" is true
      And the property "header.visible" of "d" is false
      And the property "width" of "d" is 225
      And the property "height" of "d" is 88
      And the property "footer.y" of "d" is 24
      And the property "footer.height" of "d" is 64

    Scenario: Other standard buttons replace the ones there
      When the property "standardButtons" of "d" is set to 2097152
      Then the property "footer.count" of "d" is 1
      And the property "footer.standardButtons" of "d" is 2097152

    Scenario: The button that accepts closes the dialog accepted
      When the method "open" of "d" is called
      And the user touches 160, 56
      Then the property "visible" of "d" is false
      And the property "result" of "d" is 1
      And the property "log" of "root" is "result=1 accepted aboutToHide closed "

    Scenario: The button that rejects closes the dialog rejected
      When the method "open" of "d" is called
      And the user touches 60, 56
      Then the property "visible" of "d" is false
      And the property "log" of "root" is "rejected aboutToHide closed "

    @api-QtQuick.Controls.Dialog.rejected
    Scenario: Escape rejects a dialog
      When the method "open" of "d" is called
      And the user presses "Escape"
      Then the property "visible" of "d" is false
      And the property "log" of "root" is "rejected aboutToHide closed "

    Scenario: A press beside a dialog rejects it
      When the method "open" of "d" is called
      And the user touches 300, 300
      Then the property "visible" of "d" is false
      And the property "log" of "root" is "rejected aboutToHide closed "

    Scenario: A dialog that does not close on Escape is not rejected by it
      When the property "closePolicy" of "d" is set to 0
      And the method "open" of "d" is called
      And the user presses "Escape"
      Then the property "visible" of "d" is true
      And the property "log" of "root" is ""

  Rule: The buttons tell the dialog what was asked

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
          property string found: ""
          function press(i) { d.footer.itemAt(i).clicked() }
          function look() {
            found = d.standardButton(Dialog.Apply).text + " " + d.standardButton(Dialog.Ok)
          }
          Dialog {
            id: d
            objectName: "d"
            standardButtons: Dialog.Apply | Dialog.Reset | Dialog.Help | Dialog.Discard
            onApplied: root.log += "applied "
            onReset: root.log += "reset "
            onHelpRequested: root.log += "help "
            onDiscarded: root.log += "discarded "
            onClosed: root.log += "closed "
          }
        }
        """

    @api-QtQuick.Controls.Dialog.applied
    Scenario Outline: The button "<text>" tells the dialog "<log>" and leaves it open
      When the method "open" of "d" is called
      Then the property "footer.count" of "d" is 4
      When the method "press" of "root" is called with <index>
      Then the property "log" of "root" is "<log> "
      And the property "visible" of "d" is true

      Examples:
        | index | text  | log     |
        | 0     | Help  | help    |
        | 1     | Reset | reset   |
        | 2     | Apply | applied |

    @api-QtQuick.Controls.Dialog.discarded
    Scenario: The button that discards tells the dialog and closes it, neither accepted nor rejected
      When the method "open" of "d" is called
      And the method "press" of "root" is called with 3
      Then the property "log" of "root" is "discarded closed "
      And the property "visible" of "d" is false
      And the property "result" of "d" is 0

    @api-QtQuick.Controls.Dialog.standardButton
    Scenario: A dialog hands out its standard buttons
      When the method "look" of "root" is called
      Then the property "found" of "root" is "Apply null"

  Rule: A dialog and the keyboard

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          Item { objectName: "outside"; focus: true }
          Dialog {
            objectName: "d"
            Item { objectName: "field"; focus: true }
          }
        }
        """

    @api-QtQuick.Controls.Dialog.focus
    Scenario: An open dialog has the keyboard and gives it back
      When the method "open" of "d" is called
      Then the property "activeFocus" of "d" is true
      And the property "activeFocus" of "field" is true
      And the property "activeFocus" of "outside" is false
      When the method "reject" of "d" is called
      Then the property "activeFocus" of "outside" is true
