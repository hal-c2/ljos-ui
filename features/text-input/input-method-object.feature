# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#inputMethod-prop
#   https://doc.qt.io/qt-6/qml-qtquick-inputmethod.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: The input method object
  `Qt.inputMethod` describes the platform's input method. It is there on a
  machine without an on-screen keyboard too, so an application can use it
  without asking first.

  Rule: It is one object, there whatever the platform has

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property var im: Qt.inputMethod
          readonly property bool same: Qt.inputMethod === Qt.inputMethod
          readonly property string kind: typeof Qt.inputMethod
          readonly property bool shown: im.visible
          readonly property bool animating: im.animating
          readonly property real keyboardHeight: im.keyboardRectangle.height
          readonly property real keyboardWidth: im.keyboardRectangle.width
          readonly property int direction: im.inputDirection
          readonly property string localeKind: typeof im.locale
          readonly property string localeName: typeof im.locale.name
          property string refused: ""
          function write() { try { Qt.inputMethod.visible = true } catch (e) { refused = e.name } }
        }
        """

    Scenario: The same object every time
      Then the property "kind" of "root" is "object"
      And the property "same" of "root" is true

    Scenario: With no keyboard on the screen it says so
      Then the property "shown" of "root" is false
      And the property "animating" of "root" is false
      And the property "keyboardHeight" of "root" is 0
      And the property "keyboardWidth" of "root" is 0

    Scenario: It says which way input goes and in what locale
      Then the property "direction" of "root" is 0
      And the property "localeKind" of "root" is "object"
      And the property "localeName" of "root" is "string"

    Scenario: What it says cannot be written
      When the method "write" of "root" is called
      Then the property "refused" of "root" is "TypeError"
      And the property "shown" of "root" is false

  Rule: Asking it to act is safe with nothing to act on

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          readonly property bool shown: Qt.inputMethod.visible
          property int visibleChanges: 0
          Connections { target: Qt.inputMethod; function onVisibleChanged() { root.visibleChanges++ } }
          function focusInput() { input.forceActiveFocus() }
          function callAll() {
            Qt.inputMethod.show()
            Qt.inputMethod.hide()
            Qt.inputMethod.update(Qt.ImQueryAll)
            Qt.inputMethod.reset()
            Qt.inputMethod.commit()
            Qt.inputMethod.invokeAction(Qt.Click, 0)
          }
          TextInput { id: input; objectName: "input"; width: 100; height: 30 }
        }
        """

    Scenario: Every method can be called with no editor focused
      When the method "callAll" of "root" is called
      Then the property "shown" of "root" is false
      And the property "visibleChanges" of "root" is 0

    Scenario: Every method can be called with an editor focused
      When the method "focusInput" of "root" is called
      And the method "callAll" of "root" is called
      Then the property "text" of "input" is ""
      And the property "activeFocus" of "input" is true

  Rule: The rectangles follow the editor with focus, in the window's coordinates

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Window
        Item {
          objectName: "root"
          width: 200; height: 100
          readonly property var im: Qt.inputMethod
          readonly property real cursorX: im.cursorRectangle.x
          readonly property real cursorY: im.cursorRectangle.y
          readonly property bool hasCursor: im.cursorRectangle.height > 0
          readonly property bool anchorIsCursor: im.anchorRectangle.y === im.cursorRectangle.y
          readonly property real clipX: im.inputItemClipRectangle.x
          readonly property real clipY: im.inputItemClipRectangle.y
          readonly property real clipWidth: im.inputItemClipRectangle.width
          readonly property real clipHeight: im.inputItemClipRectangle.height
          readonly property bool clipIsWindow: clipWidth === Window.width && clipHeight === Window.height
          function focusInput() { input.forceActiveFocus() }
          property bool anchorAtStart: false; property bool cursorAtEnd: false
          function selectPart() {
            input.forceActiveFocus(); input.text = "hello world"; input.select(2, 6)
            anchorAtStart = im.anchorRectangle.x === input.x + input.positionToRectangle(2).x
            cursorAtEnd = im.cursorRectangle.x === input.x + input.positionToRectangle(6).x
          }
          TextInput { id: input; x: 20; y: 30; width: 100; height: 30; font.pixelSize: 12 }
        }
        """

    Scenario: With nothing focused it has the window and no cursor
      Then the property "hasCursor" of "root" is false
      And the property "cursorX" of "root" is 0
      And the property "clipX" of "root" is 0
      And the property "clipY" of "root" is 0
      And the property "clipIsWindow" of "root" is true

    Scenario: With an editor focused it has the editor and its cursor
      When the method "focusInput" of "root" is called
      Then the property "hasCursor" of "root" is true
      And the property "anchorIsCursor" of "root" is true
      And the property "cursorX" of "root" is 20
      And the property "cursorY" of "root" is 30
      And the property "clipX" of "root" is 20
      And the property "clipY" of "root" is 30
      And the property "clipWidth" of "root" is 100
      And the property "clipHeight" of "root" is 30

    Scenario: With a selection the anchor is where it starts
      When the method "selectPart" of "root" is called
      Then the property "anchorAtStart" of "root" is true
      And the property "cursorAtEnd" of "root" is true
