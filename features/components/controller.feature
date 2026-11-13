# Sources:
#   docs/DIALECT.md §4.20 (Controllers), §15 (the lifecycle's names)
#   https://lit.dev/docs/composition/controllers/
#   https://www.typescriptlang.org/docs/handbook/2/objects.html
# Qt has no Controller: a QtObject does not know the item it is declared in, and an
# attached object is made by the type it attaches to. Every scenario is `@ljos-only`.

@ljos-only
Feature: Controllers: behaviour an item is given by declaring it
  A `Controller` (`Ljos.Core`) is a non-visual object that acts on the
  item it is declared in, its `host`. It is told when it is `attached`
  to the host, once the host is complete, when it is `detached`, as it
  or its host goes, and, if it asks, when the host was `updated`: once
  after it is attached and once after each turn in which a property of
  the host changed. One is written as a QML file whose root is a
  `Controller`, or as a TypeScript module that exports
  `controller({ … })`; both make the same object. The gestures of
  `Ljos.Ui` are Controllers.

  Rule: A Controller acts on the item it is declared in

    Scenario: A Controller's host is its item, and it is attached once the item is complete
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int ready: 0
          Component.onCompleted: ready = 1
          Controller {
            objectName: "c"
            property string seen: ""
            property int attaches: 0
            property int readyThen: -1
            onAttached: (host) => {
              seen = host.objectName
              attaches = attaches + 1
              readyThen = root.ready
            }
          }
        }
        """
      Then the property "seen" of "c" is "root"
      And the property "attaches" of "c" is 1
      And the property "readyThen" of "c" is 1
      And the application has nothing scheduled

    Scenario: A Controller in a Controller shares its host
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          objectName: "root"
          Controller {
            objectName: "outer"
            Controller {
              objectName: "inner"
              property string seen: ""
              onAttached: (host) => { seen = host.objectName }
            }
          }
        }
        """
      Then the property "seen" of "inner" is "root"

    Scenario: A QML file whose root is a Controller is a type
      Given the file "Breakpoint.qml":
        """qml
        import Ljos.Core
        Controller {
          property real width: 600
          readonly property bool compact: host !== undefined && host.width < width
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 500
          Breakpoint { id: size; objectName: "size"; width: 720 }
          property bool narrow: size.compact
        }
        """
      Then the property "narrow" of "root" is true
      When the property "width" of "root" is set to 800
      Then the property "narrow" of "root" is false

    Scenario: A Controller made into a host that is complete is attached at once
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property string seen: ""
          property int updates: 0
          function make() { made.createObject(root) }
          Component {
            id: made
            Controller {
              onAttached: (host) => { root.seen = host.objectName }
              onUpdated: root.updates = root.updates + 1
            }
          }
        }
        """
      When the method "make" of "root" is called
      Then the property "seen" of "root" is "root"
      And the property "updates" of "root" is 1
      When the property "width" of "root" is set to 300
      Then the property "updates" of "root" is 2
      And the application has nothing scheduled

  Rule: A Controller is detached once, as it or its host goes

    Scenario: A Controller is detached when its host is unloaded
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          objectName: "root"
          Loader {
            objectName: "loader"
            component: Item {
              objectName: "page"
              Controller {
                onAttached: (host) => console.warn("attached to " + host.objectName)
                onDetached: console.warn("detached")
              }
            }
          }
        }
        """
      Then the log has a warning matching "^attached to page$"
      When the property "active" of "loader" is set to false
      Then the log has a warning matching "^detached$"

    Scenario: A Controller destroyed by itself is detached, and its host stays
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property int detaches: 0
          function drop() { c.destroy() }
          Controller { id: c; objectName: "c"; onDetached: root.detaches = root.detaches + 1 }
        }
        """
      When the method "drop" of "root" is called
      And 10 milliseconds pass
      Then the object "c" does not exist
      And the object "root" exists
      And the property "detaches" of "root" is 1

  Rule: A Controller that asks is told the host was updated, once a turn

    Scenario: updated runs once after attached and once after a turn that changed the host
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int updates: 0
          function grow() { width = 300; height = 150; width = 320 }
          Controller {
            onUpdated: {
              root.updates = root.updates + 1
              root.implicitHeight = root.updates
            }
          }
        }
        """
      Then the property "updates" of "root" is 1
      And the application has nothing scheduled
      When the method "grow" of "root" is called
      Then the property "updates" of "root" is 2
      And the property "implicitHeight" of "root" is 2
      And the application has nothing scheduled

  Rule: The gestures are Controllers

    Scenario: A Tap's host is its item, and it is attached to it
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          Tap {
            objectName: "tap"
            property string seen: ""
            onAttached: (host) => { seen = host.objectName }
          }
        }
        """
      Then the property "seen" of "tap" is "root"

    Scenario: A Tap in a Controller acts on the Controller's host
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property int taps: 0
          Controller {
            Tap { onTapped: root.taps = root.taps + 1 }
          }
        }
        """
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "taps" of "root" is 1

    Scenario: A Controller and a gesture in a popup act on the popup's content
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          width: 400; height: 300
          property int taps: 0
          property int nested: 0
          Popup {
            id: popup
            x: 0; y: 0; width: 200; height: 200
            padding: 0
            Component.onCompleted: open()
            TapHandler { onTapped: root.taps = root.taps + 1 }
            Controller {
              objectName: "c"
              property bool onContent: false
              onAttached: (host) => { onContent = host === popup.contentItem }
              TapHandler { onTapped: root.nested = root.nested + 1 }
            }
          }
        }
        """
      Then the property "onContent" of "c" is true
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "taps" of "root" is 1
      And the property "nested" of "root" is 1

    Scenario: A popup's Controllers and gestures move to the content item that replaces its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        import Ljos.Core
        Item {
          id: root; objectName: "root"; width: 400; height: 300
          property int taps: 0
          property int nested: 0
          property int detaches: 0
          Popup {
            id: popup; objectName: "popup"
            x: 0; y: 0; width: 200; height: 200; padding: 0
            Component.onCompleted: open()
            TapHandler { onTapped: root.taps = root.taps + 1 }
            Controller {
              objectName: "c"
              property bool onNew: false
              property int attaches: 0
              onAttached: (host) => { attaches = attaches + 1; onNew = host === replacement }
              onDetached: root.detaches = root.detaches + 1
              TapHandler { onTapped: root.nested = root.nested + 1 }
            }
          }
          Item { id: replacement; objectName: "replacement" }
          function replace() { popup.contentItem = replacement }
        }
        """
      When the method "replace" of "root" is called
      Then the property "onNew" of "c" is true
      And the property "attaches" of "c" is 2
      And the property "detaches" of "root" is 1
      When the user presses the pointer at 50, 50
      And the user releases the pointer at 50, 50
      Then the property "taps" of "root" is 1
      And the property "nested" of "root" is 1

  Rule: A TypeScript module exports a Controller with controller({ … })

    Background:
      Given the file "breakpoint.ts":
        """ts
        export const Breakpoint = controller({
          properties: { width: 600 },
          bindings: {
            compact: (host, self) => host.width < self.width,
          },
          attached(host, self) {
            console.warn("watching " + host.objectName + " from " + self.width)
            return () => console.warn("stopped watching " + host.objectName)
          },
        })
        """

    Scenario: A Controller from a module is used as its import names it
      Given the QML:
        """qml
        import Ljos.Ui
        import "breakpoint.ts" as Size
        Item {
          id: root; objectName: "root"
          width: 500
          Size.Breakpoint { id: size; objectName: "size"; width: 720 }
          property bool narrow: size.compact
        }
        """
      Then the property "narrow" of "root" is true
      And the log has a warning matching "^watching root from 720$"
      When the property "width" of "root" is set to 800
      Then the property "narrow" of "root" is false

    Scenario: What attached returns runs when the Controller is detached
      Given the QML:
        """qml
        import Ljos.Ui
        import "breakpoint.ts" as Size
        Item {
          objectName: "root"
          Loader {
            objectName: "loader"
            component: Item {
              objectName: "page"
              Size.Breakpoint {}
            }
          }
        }
        """
      When the property "active" of "loader" is set to false
      Then the log has a warning matching "^stopped watching page$"

    Scenario: The checker types a Controller from a module
      Given the file "Pick.qml":
        """qml
        import Ljos.Ui
        import "breakpoint.ts" as Size
        Item {
          Size.Breakpoint { id: size; width: 720 }
          property bool narrow: size.compact
        }
        """
      When the files are checked
      Then the check reports:
        """
        2 files, 0 errors, 0 warnings
        """

    Scenario: The checker reports a Controller from a module given what it does not hold
      Given the file "Pick.qml":
        """qml
        import Ljos.Ui
        import "breakpoint.ts" as Size
        Item {
          Size.Breakpoint { id: size; width: "wide" }
          property string narrow: size.compact
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pick.qml:4:46: error: Invalid property assignment: number expected [invalid-assignment]
        Pick.qml:5:35: error: `narrow` is a string; this gives it a boolean [ts-assign-type]
        2 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: The checker reads what a module's controller declares
      Given the file "bad.ts":
        """ts
        export const Wide = controller({
          properties: { width: 600 },
          bindings: {
            wide: (host, self) => host.width > self.widht,
          },
          started(host) {},
        })
        """
      And the file "Pick.qml":
        """qml
        import Ljos.Ui
        import "bad.ts" as Bad
        Item {
          Bad.Wide {}
        }
        """
      When the files are checked
      Then the check reports:
        """
        bad.ts:4:53: error: `widht` has no type: an object has no member of that name [ts-no-any]
        bad.ts:6:11: error: a controller has `properties`, `bindings`, `attached`, `detached` and `updated`; `started` is none of them [ts-forbidden]
        2 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: The checker reports a controller's members not written in it
      Given the file "made.ts":
        """ts
        const defaults = { width: 600 }
        const extra = { height: 400 }
        export const Elsewhere = controller({
          properties: defaults,
        })
        export const Spread = controller({
          properties: { ...extra, width: 600 },
        })
        """
      And the file "Pick.qml":
        """qml
        import Ljos.Ui
        import "made.ts" as Made
        Item {
          Made.Elsewhere {}
          Made.Spread {}
        }
        """
      When the files are checked
      Then the check reports:
        """
        made.ts:4:23: error: a controller's `properties` is an object written in it, as `properties: { … }`: the Controller is made from the members written there [ts-forbidden]
        made.ts:7:25: error: each member of a controller's `properties` is written `name: value`: the Controller is made from the names written there [ts-forbidden]
        2 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

  Rule: A Controller needs an item to act on

    Scenario: The checker reports a Controller declared outside an item
      Given the file "Pick.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          property Object held: Object {
            property Controller inner: Controller {}
          }
          Controller {}
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pick.qml:5:40: error: a `Controller` acts on the item it is declared in, and an `Object` is not an item: declare it in one [controller-host]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    # Ljós's own table describes a `Window` as a `Rectangle`; Qt's as no item, whose children
    # go into its content item, where qml6 hosts a handler declared in it.
    Scenario: The checker takes a gesture in a Window, with or without Qt's type table
      Given the file "Pick.qml":
        """qml
        import Ljos.Ui
        Window {
          width: 200
          height: 100
          Tap {}
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: The checker takes a gesture in an ApplicationWindow, which hosts it in its content
      Given the file "Pick.qml":
        """qml
        import Ljos.Ui
        import Ljos.Controls
        ApplicationWindow {
          Tap {}
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: The checker takes a Controller and a gesture in a popup, which hosts them in its content
      Given the file "Pick.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        import Ljos.Controls
        Item {
          Popup {
            Tap {}
            Controller {}
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
