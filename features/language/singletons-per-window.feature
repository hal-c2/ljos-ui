# Sources:
#   https://doc.qt.io/qt-6/qml-singleton.html
#   https://doc.qt.io/qt-6/qqmlengine.html#qmlRegisterSingletonType
#   docs/ARCHITECTURE.md §6.4 (singletons) and D2 (one engine per application)

# Qt has one engine per set of singletons, so an application that wants per-window state makes
# an engine for each window. Ljós has one engine and lets a native singleton type be one for
# each window instead.
@ljos-only
Feature: A singleton for each window
  A native singleton type the application registers as one for each window has one object for
  every window. Everything under a window reads that window's object by the type's name: the
  objects of the window's own file, the components it uses, and the objects its scripts make
  later. The object is made the first time something under the window reads it, and it goes
  when the window goes. An application that made a window's object before the window, as a Qt
  application hands its engine an instance, can give it to the window instead: the window
  reads that object and leaves it standing when it goes. A singleton for the engine is still
  one object, shared by every window. A QML `pragma Singleton` file that reads a window's
  singleton, itself or through another such file, is one for each window as well, as it is in
  a Qt application with an engine for each window; any other is shared.

  The application gives these scenarios the module `Fixtures`. Its `Document` is one for each
  window and its `Session` is one for the engine. Each has a `title`. The application logs
  "Document made" and "Document dropped" when a window's `Document` is made or goes.

  Background:
    Given the file "Second.qml":
      """qml
      import QtQuick
      import Fixtures
      Item {
        objectName: "second"
        property string title: Document.title
        property string session: Session.title
        function name(t) { Document.title = t }
      }
      """

  Rule: Each window reads its own object

    Scenario: A write under one window is not seen under another
      Given the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property string title: Document.title
          Component.onCompleted: Document.title = "first"
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the method "name" of "second" is called with "second"
      Then the property "title" of "root" is "first"
      And the property "title" of "second" is "second"

    Scenario: A component a window uses reads the window's object
      Given the file "Label.qml":
        """qml
        import QtQuick
        import Fixtures
        Item { property string shown: Document.title }
        """
      And the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          Label { objectName: "label" }
          Component.onCompleted: Document.title = "mine"
        }
        """
      When the file "Second.qml" is opened in a window of its own
      Then the property "shown" of "label" is "mine"
      And the property "title" of "second" is ""

    Scenario: An object a window's script makes with no parent reads the window's object
      Given the file "Probe.qml":
        """qml
        import QtQuick
        import Fixtures
        QtObject { property string shown: Document.title }
        """
      And the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property QtObject probe
          Component.onCompleted: Document.title = "first"
          function make() { probe = Qt.createComponent("Probe.qml").createObject(null) }
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the method "make" of "root" is called
      Then the property "probe.shown" of "root" is "first"

    Scenario: A singleton for the engine is shared by every window
      Given the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          Component.onCompleted: Session.title = "shared"
        }
        """
      When the file "Second.qml" is opened in a window of its own
      Then the property "session" of "second" is "shared"

  Rule: The object is made when first read and goes with its window

    Scenario: Each window that reads the type makes one object of its own
      Given the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property string title: Document.title
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the method "name" of "second" is called with "again"
      Then the log has 2 infos matching "^Document made$"

    Scenario: A window that never reads the type makes no object of it
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      Then the log has no info matching "Document made"

    Scenario: Closing a window drops its object and leaves the others
      Given the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property string title: Document.title
          Component.onCompleted: Document.title = "first"
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the object "second" is destroyed
      Then the log has an info matching "^Document dropped$"
      And the property "title" of "root" is "first"

  Rule: A QML singleton that reads a window's singleton is one for each window

    Scenario: Each window reads its own object of a QML singleton built on a window's singleton
      Given the file "qmldir":
        """
        singleton Theme Theme.qml
        """
      And the file "Theme.qml":
        """qml
        pragma Singleton
        import QtQuick
        import Fixtures
        QtObject {
          property string title: Document ? Document.title : "none"
          property string note: ""
        }
        """
      And the file "Second.qml":
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "second"
          property string title: Theme.title
          property string note: Theme.note
          function name(t) { Document.title = t; Theme.note = "noted " + t }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property string title: Theme.title
          property string note: Theme.note
          Component.onCompleted: { Document.title = "first"; Theme.note = "noted first" }
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the method "name" of "second" is called with "second"
      Then the property "title" of "root" is "first"
      And the property "note" of "root" is "noted first"
      And the property "title" of "second" is "second"
      And the property "note" of "second" is "noted second"
      And the log has no warnings

    Scenario: A QML singleton that reads one through another QML singleton is one for each window too
      Given the file "qmldir":
        """
        singleton Theme Theme.qml
        singleton Header Header.qml
        """
      And the file "Theme.qml":
        """qml
        pragma Singleton
        import QtQuick
        import Fixtures
        QtObject { property string title: Document.title }
        """
      And the file "Header.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject { property string shown: "[" + Theme.title + "]" }
        """
      And the file "Second.qml":
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "second"
          property string shown: Header.shown
          function name(t) { Document.title = t }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property string shown: Header.shown
          Component.onCompleted: Document.title = "first"
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the method "name" of "second" is called with "second"
      Then the property "shown" of "root" is "[first]"
      And the property "shown" of "second" is "[second]"

    Scenario: A QML singleton that reads no window's singleton is shared by every window
      Given the file "qmldir":
        """
        singleton Counter Counter.qml
        """
      And the file "Counter.qml":
        """qml
        pragma Singleton
        import QtQuick
        import Fixtures
        QtObject { property string note: Session.title }
        """
      And the file "Second.qml":
        """qml
        import QtQuick
        Item {
          objectName: "second"
          property string note: Counter.note
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          Component.onCompleted: { Session.title = "shared"; Counter.note = "set once" }
        }
        """
      When the file "Second.qml" is opened in a window of its own
      Then the property "note" of "second" is "set once"

    Scenario: Closing a window drops its object of a QML singleton with its window's singleton
      Given the file "qmldir":
        """
        singleton Theme Theme.qml
        """
      And the file "Theme.qml":
        """qml
        pragma Singleton
        import QtQuick
        import Fixtures
        QtObject {
          property string title: Document.title
          Component.onDestruction: console.info("Theme gone")
        }
        """
      And the file "Second.qml":
        """qml
        import QtQuick
        Item { objectName: "second"; property string title: Theme.title }
        """
      And the QML:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "root"
          property string title: Theme.title
          Component.onCompleted: Document.title = "first"
        }
        """
      When the file "Second.qml" is opened in a window of its own
      And the object "second" is destroyed
      Then the log has an info matching "^Theme gone$"
      And the log has an info matching "^Document dropped$"
      And the property "title" of "root" is "first"

  Rule: A reload keeps each window's object

    Scenario: A window's object outlives a reload of the window's file
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the file "Second.qml" is opened in a window of its own
      And the method "name" of "second" is called with "kept"
      And the file "Second.qml" is changed to:
        """qml
        import QtQuick
        import Fixtures
        Item {
          objectName: "second"
          property string title: "now " + Document.title
          function name(t) { Document.title = t }
        }
        """
      And a reload happens
      Then the property "title" of "second" is "now kept"
      And the log has no info matching "Document dropped"

  Rule: The application can hand a window an object it made

    Scenario: A window reads the object its application made before it
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the file "Second.qml" is opened in a window of its own, given a Document titled "made first"
      Then the property "title" of "second" is "made first"
      And the log has no info matching "Document made"

    Scenario: A write under the window reaches the application's object
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the file "Second.qml" is opened in a window of its own, given a Document titled "made first"
      And the method "name" of "second" is called with "renamed"
      Then the Document the application gave is titled "renamed"

    Scenario: The object the application gave outlives its window
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """
      When the file "Second.qml" is opened in a window of its own, given a Document titled "made first"
      And the object "second" is destroyed
      Then the log has an info matching "^Document dropped$"
      And the Document the application gave is titled "made first"
