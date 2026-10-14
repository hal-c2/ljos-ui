# Sources:
#   https://doc.qt.io/qt-6/qml-singleton.html
#   https://doc.qt.io/qt-6/qtqml-modules-qmldir.html
#   https://doc.qt.io/qt-6/qtqml-documents-structure.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Singleton types written in QML
  A file that starts with `pragma Singleton`, named by a `singleton` line of
  the qmldir file of its directory, is one object and not a type to make
  objects of. Every file that sees the directory reads that one object by the
  type's name, and it is made the first time something reads it.

  Background:
    Given the file "qmldir":
      """
      singleton Theme Theme.qml
      """
    And the file "Theme.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject {
        property int base: 4
        readonly property int twice: base * 2
        property color accent: "#336699"
        function scaled(n) { return n * base }
        enum Mode { Light, Dark }
        Component.onCompleted: console.warn("theme made")
      }
      """

  Rule: The type's name is the object

    Scenario: Its properties, functions and enums are read by the name
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int base: Theme.base
          property int twice: Theme.twice
          property color accent: Theme.accent
          property int scaled: Theme.scaled(3)
          property int dark: Theme.Dark
          property int scopedDark: Theme.Mode.Dark
          property string kind: typeof Theme
        }
        """
      Then the property "base" of "root" is 4
      And the property "twice" of "root" is 8
      And the property "accent" of "root" is "#336699"
      And the property "scaled" of "root" is 12
      And the property "dark" of "root" is 1
      And the property "scopedDark" of "root" is 1
      And the property "kind" of "root" is "object"

    Scenario: A name the singleton does not have is undefined
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool missing: Theme.nothing === undefined
        }
        """
      Then the property "missing" of "root" is true

    Scenario: It has no parent to read
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool noParent: Theme.parent === undefined
        }
        """
      Then the property "noParent" of "root" is true

    Scenario: The name is a value that can be kept and passed on
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var held: Theme
          property int through: held.base
          property bool same: held === Theme
          function baseOf(theme) { return theme.base }
          property int passed: baseOf(Theme)
        }
        """
      Then the property "through" of "root" is 4
      And the property "same" of "root" is true
      And the property "passed" of "root" is 4

    Scenario: A script assigns to its property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int base: Theme.base
          property int twice: Theme.twice
          property int scaled: Theme.scaled(3)
          function grow() { Theme.base = 10 }
        }
        """
      When the method "grow" of "root" is called
      Then the property "base" of "root" is 10
      And the property "twice" of "root" is 20
      And the property "scaled" of "root" is 30

    Scenario: Assigning to a read-only property of it is an error
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string error: "none"
          property int twice: Theme.twice
          function spoil() {
            try { Theme.twice = 1 } catch (e) { error = e.name }
          }
        }
        """
      When the method "spoil" of "root" is called
      Then the property "error" of "root" is "TypeError"
      And the property "twice" of "root" is 8

  Rule: Every file reads the same object

    Background:
      Given the file "Reader.qml":
        """qml
        import QtQuick
        Item {
          property var seen: Theme
          property int base: Theme.base
          property int changes: 0
          Connections {
            target: Theme
            function onBaseChanged() { changes += 1 }
          }
        }
        """

    Scenario: Two files name one object
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Reader { id: reader }
          property bool same: reader.seen === Theme
        }
        """
      Then the property "same" of "root" is true

    Scenario: What one file writes, the bindings and handlers of another see
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Reader { objectName: "reader" }
          function grow() { Theme.base = 10 }
        }
        """
      When the method "grow" of "root" is called
      Then the property "base" of "reader" is 10
      And the property "changes" of "reader" is 1

  Rule: The object is made when it is first read

    Scenario: A binding that does not reach the name leaves it unmade
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool on: false
          property int size: on ? Theme.base : -1
        }
        """
      Then the property "size" of "root" is -1
      And the log has no warning matching "theme made"

    Scenario: The first read makes it
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool on: false
          property int size: on ? Theme.base : -1
        }
        """
      When the property "on" of "root" is set to true
      Then the property "size" of "root" is 4
      And the log has a warning matching "theme made"

    Scenario: A function that returns before the name leaves it unmade
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool on: false
          function size() {
            if (!on)
              return -1
            return Theme.base
          }
          property int now: size()
        }
        """
      Then the property "now" of "root" is -1
      And the log has no warning matching "theme made"

    Scenario: While it is made, its own bindings read it by the name
      Given the file "own/qmldir":
        """
        singleton Gauge Gauge.qml
        """
      And the file "own/Gauge.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          id: self
          property int base: 2
          property bool same: Gauge === self
          property int twice: Gauge.base * 2
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "own"
        Item {
          objectName: "root"
          property bool same: Gauge.same
          property int twice: Gauge.twice
        }
        """
      Then the property "same" of "root" is true
      And the property "twice" of "root" is 4

  Rule: It is not a type to make objects of

    Scenario: An object of the type does not load
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Item {
          Theme { }
        }
        """
      Then loading "Wrong.qml" is rejected
