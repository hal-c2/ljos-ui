# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#selectByMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#selectByMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#keepMouseGrab-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Selecting by mouse inside a Flickable
  An editor that selects by mouse keeps the press it holds: a drag over it
  selects, in any direction, and the Flickable or view around it does not
  take the drag. An editor that does not select by mouse lets the drag go
  to the Flickable, which scrolls. The numbers are those of the bundled
  font, Liberation Sans, at 14 pixels.

  Rule: A TextEdit in a list

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 200
          property bool selects: true
          ListView {
            objectName: "list"
            anchors.fill: parent
            model: 6
            delegate: TextEdit {
              required property int index
              objectName: "edit" + index
              width: ListView.view.width
              readOnly: true
              selectByMouse: root.selects
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "row " + index + " first line\nrow " + index + " second line\nrow " + index + " third line"
            }
          }
        }
        """

    Scenario: A drag down a selecting TextEdit selects its lines
      When the user presses the pointer at 5, 5
      And the user moves the pointer to 60, 10
      And the user moves the pointer to 60, 20
      And the user moves the pointer to 60, 30
      And the user moves the pointer to 60, 45
      And the user releases the pointer at 60, 45
      Then the property "selectedText" of "edit0" is "ow 0 first line\nrow 0 second line\nrow 0 thir"
      And the property "contentY" of "list" is 0

    Scenario: A drag up a selecting TextEdit selects and the list stays
      When the user presses the pointer at 60, 140
      And the user moves the pointer to 60, 120
      And the user moves the pointer to 60, 110
      And the user moves the pointer to 60, 100
      And the user releases the pointer at 60, 100
      Then the property "contentY" of "list" is 0
      And the property "selectedText" of "edit2" is " line\nrow 2 second line\nrow 2 thir"

    Scenario: A drag along one line of a selecting TextEdit selects
      When the user presses the pointer at 20, 5
      And the user moves the pointer to 30, 6
      And the user moves the pointer to 40, 6
      And the user moves the pointer to 60, 6
      And the user releases the pointer at 60, 6
      Then the property "contentY" of "list" is 0
      And the property "selectedText" of "edit0" is " 0 first"

    Scenario: A TextEdit that does not select by mouse lets the list scroll
      When the property "selects" of "root" is set to false
      And the user presses the pointer at 60, 140
      And the user moves the pointer to 60, 120
      And the user moves the pointer to 60, 110
      And the user moves the pointer to 60, 100
      And the user releases the pointer at 60, 100
      Then the property "selectedText" of "edit2" is ""
      And the property "contentY" of "list" is 10

  Rule: A TextInput in a Flickable

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 400; height: 200
          Flickable {
            objectName: "f"
            anchors.fill: parent
            contentWidth: 400; contentHeight: 1000
            TextInput {
              objectName: "input"
              y: 100
              width: 300
              selectByMouse: true
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "hello brave world"
            }
          }
        }
        """

    Scenario: A sideways drag on a selecting TextInput selects and the Flickable stays
      When the user presses the pointer at 2, 105
      And the user moves the pointer to 30, 104
      And the user moves the pointer to 50, 100
      And the user moves the pointer to 70, 90
      And the user releases the pointer at 70, 90
      Then the property "contentY" of "f" is 0
      And the property "selectedText" of "input" is "hello brave"

    Scenario: A drag up a selecting TextInput, which has one line, scrolls the Flickable
      When the user presses the pointer at 70, 105
      And the user moves the pointer to 70, 85
      And the user moves the pointer to 70, 75
      And the user moves the pointer to 70, 60
      And the user releases the pointer at 70, 60
      Then the property "selectedText" of "input" is ""
      And the property "contentY" of "f" is 15

    Scenario: A TextInput that does not select by mouse lets the Flickable scroll
      When the property "selectByMouse" of "input" is set to false
      And the user presses the pointer at 70, 105
      And the user moves the pointer to 70, 85
      And the user moves the pointer to 70, 75
      And the user moves the pointer to 70, 60
      And the user releases the pointer at 70, 60
      Then the property "selectedText" of "input" is ""
      And the property "contentY" of "f" is 15

  Rule: A TextArea in a ScrollView

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 200
          ScrollView {
            objectName: "view"
            anchors.fill: parent
            TextArea {
              objectName: "area"
              readOnly: true
              selectByMouse: true
              font.family: "Liberation Sans"; font.pixelSize: 14
              text: "line one\nline two\nline three\nline four\nline five\nline six\nline seven\nline eight\nline nine\nline ten\nline eleven\nline twelve"
            }
          }
        }
        """

    Scenario: A drag down a selecting TextArea selects and the view stays
      When the user presses the pointer at 10, 10
      And the user moves the pointer to 40, 30
      And the user moves the pointer to 30, 40
      And the user moves the pointer to 30, 45
      And the user releases the pointer at 30, 45
      Then the property "contentItem.contentY" of "view" is 0
      And the property "selectionStart" of "area" is 0
      And the property "selectionEnd" of "area" is at least 20
