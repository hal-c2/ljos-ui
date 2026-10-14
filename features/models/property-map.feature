# Sources:
#   https://doc.qt.io/qt-6/qqmlpropertymap.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# A property map is made by the application and given to the QML, as the
# documentation's example gives one the name "owner". The expectations
# below were measured on Qt 6.11.2 that way. No step can hand a scenario
# a map yet, on either backend, so the scenarios that need one are
# @backlog; the Rust tests of ljos_engine pin the same behaviour.

Feature: PropertyMap
  A property map holds key and value pairs the application gives the QML. Each key is a property of the map: a binding that reads it follows it, and writing it in QML tells the application.

  Rule: A property map is given to QML, not made in it

    @qt-parity
    Scenario Outline: PropertyMap is not a type an <import> gives
      Given the file "Wrong.qml":
        """qml
        import <import>
        import QtQuick
        Item { PropertyMap { } }
        """
      Then loading "Wrong.qml" is rejected

      Examples:
        | import        |
        | QtQml         |
        | QtQml.Models  |
        | QtQuick       |

  Rule: Each key is a property, followed on its own

    Background:
      Given a property map "owner" holding "name"="John Smith", "phone"="555-5555"
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string shown: owner.name + " " + owner.phone
          property string names: ""
          property int nameRuns: 0
          property string nameOnly: { nameRuns++; return owner.name }
          property string changed: ""
          function listKeys() { names = owner.keys().join(",") }
          function rename() { owner.name = "Zed" }
          function addEmail() { owner.email = "z@x.is" }
          Connections {
            target: owner
            function onValueChanged(key, value) { root.changed = key + "=" + value }
          }
        }
        """

    @backlog
    Scenario: A binding reads the keys as properties
      Then the property "shown" of "root" is "John Smith 555-5555"

    @backlog
    Scenario: keys() lists the keys in the order they were added
      When the method "listKeys" of "root" is called
      Then the property "names" of "root" is "name,phone"

    @backlog
    Scenario: A new value from the application reaches what is bound to that key
      When the property map "owner" is given "phone"="111"
      Then the property "shown" of "root" is "John Smith 111"
      And the property "changed" of "root" is ""

    @backlog
    Scenario: A key's new value does not re-run a binding on another key
      When the property map "owner" is given "phone"="111"
      Then the property "nameRuns" of "root" is 1

    @backlog
    Scenario: The same value again from the application re-runs nothing
      When the property map "owner" is given "name"="John Smith"
      Then the property "nameRuns" of "root" is 1

    @backlog
    Scenario: Writing a key in QML tells the application and updates bindings
      When the method "rename" of "root" is called
      Then the property "changed" of "root" is "name=Zed"
      And the property "shown" of "root" is "Zed 555-5555"

    # Also waiting on the engine seeing writes to properties an object
    # has not got: today only the application adds keys.
    @backlog
    Scenario: Writing a key the map has not got adds it
      When the method "addEmail" of "root" is called
      And the method "listKeys" of "root" is called
      Then the property "changed" of "root" is "email=z@x.is"
      And the property "names" of "root" is "name,phone,email"

  Rule: Keys are cleared, never taken away

    @backlog
    Scenario: A cleared key reads as nothing and is still listed
      Given a property map "owner" holding "name"="John Smith"
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string kind: typeof owner.name
          property string names: ""
          function listKeys() { names = owner.keys().join(",") }
        }
        """
      When the key "name" of the property map "owner" is cleared
      And the method "listKeys" of "root" is called
      Then the property "kind" of "root" is "undefined"
      And the property "names" of "root" is "name"

    # In Qt, reading a key the map has not got adds it (with no value), so
    # a binding that read it follows the value the application later
    # gives it. That needs reads of missing properties to be seen by the
    # map, which the engine has no hook for yet.
    @backlog
    Scenario: A key given later reaches a binding that read it before
      Given a property map "owner" holding "name"="John Smith"
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var late: owner.late
          property string kind: typeof late
        }
        """
      When the property map "owner" is given "late"=42
      Then the property "late" of "root" is 42
      And the property "kind" of "root" is "number"

  Rule: A frozen map takes no new keys

    @backlog
    Scenario: Existing keys still change and new ones do not appear
      Given a property map "owner" holding "name"="John Smith"
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string shown: owner.name
          property string names: ""
          function listKeys() { names = owner.keys().join(",") }
        }
        """
      When the property map "owner" is frozen
      And the property map "owner" is given "name"="Zed"
      And the property map "owner" is given "email"="z@x.is"
      And the method "listKeys" of "root" is called
      Then the property "shown" of "root" is "Zed"
      And the property "names" of "root" is "name"
