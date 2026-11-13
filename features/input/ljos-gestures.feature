# Sources:
#   docs/DIALECT.md §4.7 (input: one pointer stream, gestures as children), §4.14
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Ui`'s gestures are Ljós's own names for Qt Quick's input handlers: the scenarios in them
# are `@ljos-only`, and each is held to the numbers of the Qt file before it, which runs on both.
# A handler's `accepted*` filters say what the gesture takes, so they are named for that:
# `buttons`, `modifiers`, `devices` and `pointerTypes`, as a pointer event names what it carries.

Feature: Ljós's gestures: Tap and Hover
  A gesture is a child of the item it acts on. `Tap` is Qt Quick's
  TapHandler and `Hover` its HoverHandler, under the names Ljós's API
  gives them: what a handler accepts is what the gesture takes, so
  `acceptedButtons` is `buttons`, `acceptedModifiers` is `modifiers`,
  `acceptedDevices` is `devices` and `acceptedPointerTypes` is
  `pointerTypes`. A file written with the handlers is lowered to the
  gestures member by member.

  Rule: A Tap takes the taps a TapHandler takes

    @qt-parity
    Scenario: A TapHandler that accepts the right button
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int taps: 0
          TapHandler { acceptedButtons: Qt.RightButton; onTapped: root.taps += 1 }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      And the user clicks the right button at 150, 60
      And the user clicks the right button at 20, 20
      Then the property "taps" of "root" is 2

    @ljos-only
    Scenario: A Tap that takes the right button
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int taps: 0
          Tap { buttons: MouseButton.Right; onTapped: root.taps += 1 }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      And the user clicks the right button at 150, 60
      And the user clicks the right button at 20, 20
      Then the property "taps" of "root" is 2

    # qml6 parents a handler declared in a Window to the window's content item. Its scene
    # driver holds what it loads in a window of its own, so the window is a test's.
    @qt-parity
    Scenario: A TapHandler declared in a Window takes the taps on the window
      Given the file "tst_window_tap.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          Window {
            id: w
            width: 200; height: 100; visible: true
            property int taps: 0
            TapHandler { onTapped: w.taps += 1 }
          }
          TestCase {
            name: "WindowTap"
            when: windowShown
            function test_taps() {
              mouseClick(w.contentItem, 50, 50)
              mouseClick(w.contentItem, 150, 60)
              compare(w.taps, 2)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : WindowTap::test_taps()
        """

    @ljos-only
    Scenario: A Tap declared in a Window takes the taps on the window
      Given the QML:
        """qml
        import Ljos.Ui
        Window {
          id: root; objectName: "root"
          width: 200; height: 100; visible: true
          property int taps: 0
          Tap { onTapped: root.taps += 1 }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      And the user presses the pointer at 150, 60
      And the user releases the pointer at 150, 60
      Then the property "taps" of "root" is 2

    @ljos-only
    Scenario: A Tap's devices and its enums are the handler's
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int taps: 0
          Tap {
            objectName: "tap"
            devices: PointerDevice.Mouse | PointerDevice.TouchPad
            gesturePolicy: Tap.WithinBounds
            onTapped: root.taps += 1
          }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "taps" of "root" is 1
      And the property "devices" of "tap" is 5
      And the property "gesturePolicy" of "tap" is 1

  Rule: A Hover follows the pointer as a HoverHandler does

    @qt-parity
    Scenario: A HoverHandler colours its parent while the pointer is over it
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "card"
          width: 100; height: 60
          color: hover.hovered ? "#c03020" : "#2060c0"
          property int changes: 0
          HoverHandler { id: hover; onHoveredChanged: parent.changes += 1 }
        }
        """
      When the user moves the pointer to 50, 30
      Then the pixel at 50, 30 of the window is "#c03020"
      When the user moves the pointer to 150, 30
      Then the pixel at 50, 30 of the window is "#2060c0"
      And the property "changes" of "card" is 2

    @ljos-only
    Scenario: A Hover colours its parent while the pointer is over it
      Given the QML:
        """qml
        import Ljos.Ui
        Rectangle {
          objectName: "card"
          width: 100; height: 60
          fill: hover.hovered ? "#c03020" : "#2060c0"
          property int changes: 0
          Hover { id: hover; devices: PointerDevice.Mouse; onHoveredChanged: parent.changes += 1 }
        }
        """
      When the user moves the pointer to 50, 30
      Then the pixel at 50, 30 of the window is "#c03020"
      When the user moves the pointer to 150, 30
      Then the pixel at 50, 30 of the window is "#2060c0"
      And the property "changes" of "card" is 2

  Rule: A file with handlers is lowered to the gestures

    # A TapHandler takes the left button until it is told otherwise, so setting its buttons is a
    # change, on Qt as here: the 100 is the lowered `onButtonsChanged` running once.
    @ljos-only
    Scenario: The handlers lowered take the same pointer
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          id: card; objectName: "card"
          width: 100; height: 60
          color: hover.hovered ? "#c03020" : "#2060c0"
          property int taps: 0
          HoverHandler { id: hover; acceptedDevices: PointerDevice.Mouse }
          TapHandler {
            id: tap; objectName: "tap"
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onTapped: card.taps += 1
            onAcceptedButtonsChanged: card.taps += 100
          }
        }
        """
      Then the property "fill" of "card" is "#2060c0"
      And the property "buttons" of "tap" is 3
      And the property "taps" of "card" is 100
      When the user moves the pointer to 50, 30
      Then the pixel at 50, 30 of the window is "#c03020"
      When the user clicks the right button at 50, 30
      Then the property "taps" of "card" is 101

    @ljos-only @headless
    Scenario: ljos migrate writes a file's handlers as gestures
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 60
          color: hover.hovered ? "#c03020" : "#2060c0"
          HoverHandler { id: hover; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad }
          TapHandler { acceptedButtons: Qt.RightButton; gesturePolicy: TapHandler.WithinBounds }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (8 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Rectangle {
          width: 100; height: 60
          fill: hover.hovered ? "#c03020" : "#2060c0"
          Hover { id: hover; devices: PointerDevice.Mouse | PointerDevice.TouchPad }
          Tap { buttons: MouseButton.Right; gesturePolicy: Tap.WithinBounds }
        }
        """

    # Qt's grab permissions are a gesture's `takeOver` in Ljós's API (docs/DIALECT.md §4.7), which
    # is not one flag for one flag; until it is built a file that sets or reads them, or names
    # their keys, runs as written.
    @ljos-only @headless
    Scenario: A handler's grab permissions keep its file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          TapHandler { grabPermissions: PointerHandler.CanTakeOverFromAnything }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:16: `grabPermissions` is a gesture's `takeOver` in Ljós's API, which is not built yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A key of the grab permissions named through the handler keeps its file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property int permissions: TapHandler.CanTakeOverFromAnything
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:40: `grabPermissions` is a gesture's `takeOver` in Ljós's API, which is not built yet
        1 files, 0 lowered, 1 kept
        """

    # Once lowered, a gesture's new name is found on the gesture before the object outside it,
    # where in Qt the gesture has no member of that name.
    @ljos-only @headless
    Scenario: A name a gesture now has, read inside it, keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property int buttons: 0
          TapHandler { onTapped: buttons += 1 }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:26: `buttons` would be the `Tap`'s own once lowered (Qt's `acceptedButtons`), not what it names in Qt
        1 files, 0 lowered, 1 kept
        """
