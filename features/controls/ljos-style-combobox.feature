# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   features/controls/combobox-editable.feature (the same behaviour in Basic)
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`.

@ljos-only
Feature: Ljós's own style: an editable combo box
  A combo box of Ljos.Controls that is `editable` has a field to type in,
  as a combo box of every other style has: what is typed is its
  `editText`, the Return key accepts it, its validator says which texts
  are acceptable, and the field has the edit menu of a text field, in
  the same style. One that is not editable shows the text of its current
  row, cut short where it does not fit.

  # Ljos.Controls has no ContextMenu of its own to read the menu by, so
  # its rows are read from the overlay: a row of the style's Menu has
  # `iconSpace`, Basic's has not.
  Background:
    Given the file "MenuReader.qml":
      """qml
      import QtQuick
      import QtQuick.Controls as QC
      Item {
        property var ov: QC.Overlay.overlay
        property bool ownMenu: false
        function entries(n, out) {
          for (var i = 0; i < n.children.length; i++) {
            const k = n.children[i]
            if (!k.visible) continue
            if ("triggered" in k && "highlighted" in k) out.push(k); else entries(k, out)
          }
          return out
        }
        function look() {
          const rows = entries(ov, [])
          ownMenu = rows.some(k => k.text === "Paste") && rows.every(k => "iconSpace" in k)
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Controls
      Item {
        id: root; objectName: "root"
        width: 400; height: 400
        MenuReader { objectName: "reader" }
        ComboBox {
          id: c; objectName: "c"; width: 160; editable: true
          model: ["One", "Two", "Three"]
          property int times: 0
          property bool keep: false
          onAccepted: {
            times++
            if (keep && find(editText) === -1) model = model.concat([editText])
          }
        }
        ComboBox {
          objectName: "numbers"; y: 50; width: 160; editable: true; model: ["1", "2"]
          validator: IntValidator { top: 9 }
          property int times: 0
          onAccepted: times++
        }
        ComboBox { objectName: "plain"; y: 100; width: 160; model: ["One", "Two"] }
        readonly property bool inText: "" + c.contentItem.color === "" + Theme.text
      }
      """

  Rule: The content of an editable combo box is a field

    Scenario: The field shows the edit text and can be typed in
      Then the property "contentItem.text" of "c" is "One"
      And the property "contentItem.enabled" of "c" is true
      When the user touches 60, 14
      And the user types "x"
      Then the property "editText" of "c" is "Onex"
      And the property "contentItem.text" of "c" is "Onex"
      And the property "currentIndex" of "c" is 0

    Scenario: A combo box that is not editable shows its row and takes no text
      Then the property "displayText" of "plain" is "One"
      And the property "contentItem.enabled" of "plain" is false
      When the property "editable" of "plain" is set to true
      Then the property "contentItem.enabled" of "plain" is true

    Scenario: The field is drawn in the theme's text colour
      Then the property "inText" of "root" is true

  Rule: The Return key accepts the text, and a validator says which

    Scenario: The Return key says the text was accepted
      When the user touches 60, 14
      And the user presses "Return"
      Then the property "times" of "c" is 1
      And the property "currentIndex" of "c" is 0

    Scenario: A text the handler adds to the model becomes the current row
      When the property "keep" of "c" is set to true
      And the user touches 60, 14
      And the user types "x"
      And the user presses "Return"
      Then the property "count" of "c" is 4
      And the property "currentIndex" of "c" is 3
      And the property "currentText" of "c" is "Onex"

    Scenario: A character the validator refuses is not typed
      When the user touches 60, 64
      And the user types "x"
      Then the property "editText" of "numbers" is "1"
      And the property "acceptableInput" of "numbers" is true

    Scenario: The Return key accepts what the validator allows
      When the user touches 60, 64
      And the user presses "Return"
      Then the property "times" of "numbers" is 1

  Rule: The field has the edit menu, in the style

    Scenario: A right-click on the field opens the style's edit menu
      When the user clicks the right button at 60, 14
      And 200 milliseconds pass
      And the method "look" of "reader" is called
      Then the property "ownMenu" of "reader" is true

  Rule: The indicator opens the popup

    Scenario: A touch on the indicator opens the popup and leaves the field
      When the user touches 146, 14
      Then the property "popup.visible" of "c" is true
      And the property "contentItem.activeFocus" of "c" is false

    Scenario: A row of the popup is picked with the keys and shown in the field
      When the user touches 146, 14
      And the user presses "Down"
      And the user presses "Return"
      Then the property "currentIndex" of "c" is 1
      And the property "editText" of "c" is "Two"
      And the property "contentItem.text" of "c" is "Two"
