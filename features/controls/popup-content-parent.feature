# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#contentItem-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#contentChildren-prop
#   https://doc.qt.io/qt-6/qml-qtquick-window.html (attached properties)
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen), but for the scenario
# tagged @ljos-only: qml6 does not parse `async` functions.

@qt-parity
Feature: What is written in a popup sees where it is from the start
  An item written in a popup goes into the popup's content item before its
  bindings first run, so `parent` is that content item and never null. An
  attached object can be read through any expression that names an item,
  not only through an id: `parent.Window.window` in a popup is the window
  of the item the popup was written in.

  Rule: An item written in a popup has the content item as its parent from the start
    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property real seen: -1
          property bool inContent: false
          Popup {
            id: details
            objectName: "details"
            width: 240
            padding: 10
            ColumnLayout {
              objectName: "column"
              width: parent.width
              Component.onCompleted: {
                root.seen = width
                root.inContent = parent === details.contentItem
              }
            }
          }
        }
        """

    @api-QtQuick.Controls.Popup.contentItem
    Scenario: A size bound to the parent is the content item's when the item is completed
      Then the property "seen" of "root" is 220
      And the property "inContent" of "root" is true
      And the log has no warnings

    Scenario: A size bound to the parent follows the popup
      When the property "width" of "details" is set to 300
      Then the property "width" of "column" is 280

  Rule: An attached object is read through any expression that names an item
    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Window {
          id: win
          objectName: "win"
          width: 400; height: 400
          Item {
            objectName: "host"
            ToolTip {
              objectName: "tip"
              property bool found: parent?.Window.window === win
              property bool plain: parent.Window.window === win
              property int ownWidth: parent.Window.width
            }
          }
        }
        """

    @api-QtQuick.Window.window
    Scenario: A popup reads the window attached to its parent
      Then the property "found" of "tip" is true
      And the property "plain" of "tip" is true
      And the property "ownWidth" of "tip" is 400
      And the log has no warnings

  Rule: A chain that reads an attached object stops where `?.` meets nothing
    Scenario: A member named like a type is a plain object's own, and nothing gives undefined
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var plain: ({ Window: 7 })
          property var none: null
          property int fromPlain: plain?.Window
          property bool nothing: none?.Window.window === undefined
        }
        """
      Then the property "fromPlain" of "root" is 7
      And the property "nothing" of "root" is true
      And the log has no warnings

    Scenario: Past where `?.` met something, the chain reads as written
      Given the QML:
        """qml
        import QtQuick
        Window {
          id: win
          objectName: "win"
          width: 400; height: 400
          Item { id: content }
          property var hollow: ({ inner: null })
          property var finder: ({ item: content, find() { return this.item } })
          property string thrown: {
            try { return String(hollow?.inner.Window) } catch (e) { return e.constructor.name }
          }
          property bool kept: finder.find?.().Window.window === win
        }
        """
      Then the property "thrown" of "win" is "TypeError"
      And the property "kept" of "win" is true

    # Ljós only: Qt's QML takes no `async` function.
    @ljos-only
    Scenario: A key an optional call waits for is waited for by the chain
      Given the QML:
        """qml
        import QtQuick
        Window {
          id: win
          objectName: "win"
          Item { id: host; width: 10; height: 10; Item { width: 5; height: 5 } }
          property var look: async key => host[await key]?.(1, 1).Window.window === win
          property string seen: "-"
          Component.onCompleted: look("childAt").then(found => seen = String(found))
        }
        """
      When 10 milliseconds pass
      Then the property "seen" of "win" is "true"
      And the log has no warnings
