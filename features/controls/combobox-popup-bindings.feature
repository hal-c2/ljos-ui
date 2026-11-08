# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html#popup-prop
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method

# The tiers and the cross-checks between them are Ljós's own.
@ljos-only
Feature: A combo box's own popup reads the combo box by its id
  A ComboBox's popup is an object of its own, given as a property, and
  its bindings name the combo box by its id. Every way Ljós runs them
  gives the same as tier 1, however deep the combo box is.

  Rule: A popup given in a file whose root is the combo box

    Background:
      Given the tier 1 cross-check is on
      And the compiled code cross-check is on
      And the file "Combo.qml":
        """qml
        pragma ComponentBehavior: Bound
        import QtQuick
        import QtQuick.Controls.Basic
        ComboBox {
          id: control
          popup: Popup {
            objectName: "popup"
            width: Math.max(control.width, 160)
            implicitHeight: Math.min(control.count, Math.floor((320 - 2 * padding) / 30)) * 30 + 2 * padding
            padding: 4
            y: control.height
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property bool shown: true
          Loader {
            active: root.shown
            sourceComponent: Combo { objectName: "combo"; width: 220; model: ["a", "b"] }
          }
        }
        """

    Scenario: The popup's bindings give what the combo box says
      Then the property "width" of "popup" is 220
      And the property "implicitHeight" of "popup" is 68
      And the log has no warning matching "tier 1 and tier 2 differ"
      And the log has no warning matching "dynamic tiers differ"

    Scenario: The popup follows the combo box's width
      When the property "width" of "combo" is set to 260
      Then the property "width" of "popup" is 260
      And the log has no warning matching "tier 1 and tier 2 differ"
      And the log has no warning matching "dynamic tiers differ"

    Scenario: The popup's bindings agree while it is open
      When the method "open" of "popup" is called
      And 300 milliseconds pass
      Then the property "visible" of "popup" is true
      And the log has no warning matching "tier 1 and tier 2 differ"
      And the log has no warning matching "dynamic tiers differ"

    Scenario: The popup's bindings agree as the combo box goes
      When the method "open" of "popup" is called
      And 300 milliseconds pass
      And the property "shown" of "root" is set to false
      And 300 milliseconds pass
      Then the object "combo" does not exist
      And the log has no warning matching "tier 1 and tier 2 differ"
      And the log has no warning matching "dynamic tiers differ"


  Rule: A combo box loaded deep inside other objects

    Scenario Outline: A combo box <depth> Loaders deep gives its popup its width
      Given the tier 1 cross-check is on
      And the compiled code cross-check is on
      And the file "Combo.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ComboBox {
          id: control
          width: 220
          popup: Popup { objectName: "popup"; width: Math.max(control.width, 160) }
        }
        """
      And the file "Level.qml":
        """qml
        import QtQuick
        Loader {
          property int depth: 0
          source: depth > 0 ? "Level.qml" : "Combo.qml"
          onLoaded: if (depth > 0) item.depth = depth - 1
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          Level { depth: <depth> }
        }
        """
      Then the property "width" of "popup" is 220
      And the log has no warning matching "tier 1 and tier 2 differ"
      And the log has no warning matching "dynamic tiers differ"
      And the log has no warning matching "call stack"

      Examples:
        | depth |
        | 16    |
        | 64    |
