# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#signal-parameters
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#itemAdded-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A handler's undeclared parameters come before the names of the document
  A handler that does not declare its parameters is given those of the signal
  that have names, and such a name comes first: before an id of the document,
  a property of the object the handler is on and one of the root. A handler
  that declares its parameters gets the arguments in order under its own
  names; one that declares none is given none, and the names are the
  document's again. A parameter the signal does not name is never given by
  name (see tst_tap_handler_arguments.qml for TapHandler's `button`).

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int depth: 7; property int outer: 8
        property string seen: ""; property int declared: 0; property int none: 0
        signal fired(int button, int depth, int width)
        Item { id: button; objectName: "theButton" }
        Item { id: item; objectName: "theItem" }
        onFired: seen = [typeof button === "object" ? button.objectName : button, depth, width].join(" ")
        Item {
          id: kid
          signal shot(int outer)
          Connections { target: kid; function onShot(outer) { root.declared = outer } }
          Connections { target: kid; function onShot() { root.none = outer } }
        }
        Repeater { model: 1; Item { objectName: "made" }
          onItemAdded: root.seen = item.objectName + " " + index }
        Repeater { id: second; objectName: "second"; model: 0; Item { objectName: "other" }
          onItemAdded: (at, made) => root.seen = made.objectName + " " + at }
        function fire() { fired(11, 12, 13) }
        function shoot() { kid.shot(21) }
      }
      """

  Scenario: An undeclared parameter comes before an id and the object's properties
    When the method "fire" of "root" is called
    Then the property "seen" of "root" is "11 12 13"

  Scenario: A native signal's parameter comes before an id
    Then the property "seen" of "root" is "made 0"

  Scenario: A handler that declares its parameters gets them by its own names
    When the property "model" of "second" is set to 1
    Then the property "seen" of "root" is "other 0"

  Scenario: A handler that declares no parameters sees the document's names
    When the method "shoot" of "root" is called
    Then the property "declared" of "root" is 21
    And the property "none" of "root" is 8
