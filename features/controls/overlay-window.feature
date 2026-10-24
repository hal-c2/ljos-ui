# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html#overlay-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Qt makes the items of a Window in the window, so `Overlay.overlay` is the
# window's overlay when their bindings first run. The oracle's driver makes
# every file under a window of its own that is already shown, where the
# overlay is always there, so these scenarios run on Ljós only.
#
# Ljós's driver has one view, 400 × 400. A window opened on its own is
# put in the tree that view shows, as a window made without a host is, so
# its popups end in that overlay: a 200 × 100 dialog is centred at 100, 150.

@ljos-only
Feature: The overlay of a window, while the window is made
  A dialog centred in `Overlay.overlay` reads its parent's size in the
  bindings of its `x` and `y`. A `Window` or an `ApplicationWindow` has
  its overlay from the start, before anything shows it, so those bindings
  never read the size of no parent and the dialog opens in the middle of
  the window. So it is for a popup at any depth, for one in a popup that
  is not open, and for a window made later.

  Background:
    Given the file "Dialogs.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Window {
        width: 640; height: 480
        visible: true
        Item {
          anchors.fill: parent
          Column {
            Item {
              Popup {
                objectName: "dialog"
                parent: Overlay.overlay
                x: Math.round((parent.width - width) / 2)
                y: Math.round((parent.height - height) / 2)
                width: 200; height: 100
                modal: true
              }
            }
          }
        }
      }
      """
    And the file "Nested.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Window {
        width: 640; height: 480
        visible: true
        Popup {
          objectName: "outer"
          width: 300; height: 300
          Item {
            Popup {
              objectName: "inner"
              parent: Overlay.overlay
              x: Math.round((parent.width - width) / 2)
              y: Math.round((parent.height - height) / 2)
              width: 200; height: 100
            }
          }
        }
      }
      """
    And the file "Later.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Window {
        id: root
        objectName: "later"
        width: 640; height: 480
        Component {
          id: later
          Window {
            width: 300; height: 200
            Popup {
              parent: Overlay.overlay
              x: Math.round((parent.width - width) / 2)
              width: 100; height: 100
            }
          }
        }
        function make() { later.createObject(root) }
      }
      """
    And the file "Application.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 640; height: 480
        visible: true
        Item {
          Popup {
            objectName: "appDialog"
            parent: Overlay.overlay
            x: Math.round((parent.width - width) / 2)
            y: Math.round((parent.height - height) / 2)
            width: 200; height: 100
            modal: true
          }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {}
      """

  @api-QtQuick.Controls.Overlay.overlay
  Scenario: A dialog centred in the overlay reads no size of no parent
    When the file "Dialogs.qml" is opened in a window of its own
    Then the log has no warnings

  Scenario: The dialog opens in the middle of the window
    When the file "Dialogs.qml" is opened in a window of its own
    And the method "open" of "dialog" is called
    Then the property "x" of "dialog" is 100
    And the property "y" of "dialog" is 150
    And the log has no warnings

  Scenario: A popup in a popup that is not open has the window's overlay
    When the file "Nested.qml" is opened in a window of its own
    And the method "open" of "outer" is called
    And the method "open" of "inner" is called
    Then the property "x" of "inner" is 100
    And the property "y" of "inner" is 150
    And the log has no warnings

  Scenario: A window made later has its overlay while it is made
    When the file "Later.qml" is opened in a window of its own
    And the method "make" of "later" is called
    Then the log has no warnings

  Scenario: A dialog in an application window opens in its middle
    When the file "Application.qml" is opened in a window of its own
    And the method "open" of "appDialog" is called
    Then the property "x" of "appDialog" is 100
    And the property "y" of "appDialog" is 150
    And the log has no warnings
