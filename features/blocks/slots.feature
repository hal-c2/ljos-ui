# Sources:
#   docs/adr/0003-blocks-and-shells.md (section 4: Ljos.Blocks, plugins)
#   docs/ARCHITECTURE.md §16 (plugins)
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html (where made items sit)

@ljos-only
Feature: A Slot shows what the application contributes to it
  A shell names the places others may add to with a `Slot`. Anything in the
  application (a page, a plugin, a file a Loader brings in) adds to a place
  with a `Contribution` of the same name. The Slot makes one item of each
  contribution's delegate, in order, beside itself in its parent, so a
  `Row` or a layout around the Slot lays them out. With nothing contributed
  it shows its own delegate.

  Rule: Contributions show in order

    Scenario: Appended contributions follow their order, then the order they were made
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Row { Slot { objectName: "slot"; name: "tools" } }
          Contribution { slot: "tools"; order: 2
            Rectangle { objectName: "c"; width: 10; height: 10 } }
          Contribution { slot: "tools"
            Rectangle { objectName: "a"; width: 10; height: 10 } }
          Contribution { slot: "tools"
            Rectangle { objectName: "b"; width: 10; height: 10 } }
        }
        """
      Then the property "x" of "a" is 0
      And the property "x" of "b" is 10
      And the property "x" of "c" is 20
      And the property "count" of "slot" is 3

    Scenario: The Slot's own delegate stays with appended contributions
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Row {
            Slot { name: "tools"; Rectangle { objectName: "own"; width: 10; height: 10 } }
          }
          Contribution { slot: "tools"
            Rectangle { objectName: "added"; width: 10; height: 10 } }
        }
        """
      Then the property "x" of "own" is 0
      And the property "x" of "added" is 10

    Scenario: Prepended contributions come before the Slot's own delegate, in their order
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Row {
            Slot { name: "tools"; Rectangle { objectName: "own"; width: 10; height: 10 } }
          }
          Contribution { slot: "tools"
            Rectangle { objectName: "added"; width: 10; height: 10 } }
          Contribution { slot: "tools"; mode: Contribution.Prepend; order: 1
            Rectangle { objectName: "second"; width: 10; height: 10 } }
          Contribution { slot: "tools"; mode: Contribution.Prepend
            Rectangle { objectName: "first"; width: 10; height: 10 } }
        }
        """
      Then the property "x" of "first" is 0
      And the property "x" of "second" is 10
      And the property "x" of "own" is 20
      And the property "x" of "added" is 30

    Scenario: A replacing contribution takes the place of the delegate and of those before it
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Row {
            Slot { name: "tools"; Rectangle { objectName: "own"; width: 10; height: 10 } }
          }
          Contribution { slot: "tools"
            Rectangle { objectName: "early"; width: 10; height: 10 } }
          Contribution { slot: "tools"; mode: Contribution.Replace; order: 1
            Rectangle { objectName: "instead"; width: 10; height: 10 } }
          Contribution { slot: "tools"; order: 2
            Rectangle { objectName: "late"; width: 10; height: 10 } }
        }
        """
      Then the object "own" does not exist
      And the object "early" does not exist
      And the property "x" of "instead" is 0
      And the property "x" of "late" is 10

  Rule: With nothing contributed the Slot shows its own delegate

    Scenario: A Slot nobody contributes to shows its delegate
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Slot { objectName: "slot"; name: "empty"
            Text { objectName: "placeholder"; text: "Nothing here" } }
          Contribution { slot: "elsewhere"; Item { objectName: "other" } }
        }
        """
      Then the property "text" of "placeholder" is "Nothing here"
      And the object "other" does not exist
      And the property "count" of "slot" is 1

    Scenario: A Slot with no delegate and no contribution shows nothing
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Slot { objectName: "slot"; name: "empty" }
        }
        """
      Then the property "count" of "slot" is 0
      And the log has no warnings

  Rule: Contributions come and go while the application runs

    Background:
      Given the file "Plugin.qml":
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          Contribution { slot: "tools"; mode: Contribution.Replace
            Rectangle { objectName: "plugged"; width: 10; height: 10 } }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          id: root
          objectName: "root"
          property bool plugin: false
          Row {
            Slot { objectName: "slot"; name: "tools"
              Rectangle { objectName: "own"; width: 10; height: 10 } }
          }
          Loader { objectName: "loader"; active: root.plugin; source: "Plugin.qml" }
        }
        """

    Scenario: A contribution made later shows
      When the property "plugin" of "root" is set to true
      Then the object "plugged" exists
      And the object "own" does not exist

    Scenario: A contribution destroyed goes, and the Slot's delegate comes back
      When the property "plugin" of "root" is set to true
      And the property "plugin" of "root" is set to false
      Then the object "plugged" does not exist
      And the object "own" exists
      And the property "count" of "slot" is 1

    Scenario: A contribution whose plugin fails goes with it
      When the property "plugin" of "root" is set to true
      And a native handler of "plugged" panics with "broken plugin"
      Then the object "plugged" does not exist
      And the object "own" exists
      And the application has no fault

  Rule: Every Slot of a name shows its contributions

    Scenario: Two Slots of one name both show a contribution
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Row { objectName: "left"; Slot { name: "status" } }
          Row { objectName: "right"; Slot { name: "status" } }
          Contribution { slot: "status"; Text { objectName: "clock"; text: "12:00" } }
        }
        """
      Then the property "children.0.text" of "left" is "12:00"
      And the property "children.0.text" of "right" is "12:00"

    Scenario: A contribution that names another slot moves there
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Row { objectName: "left"; Slot { objectName: "left-slot"; name: "left" } }
          Row { objectName: "right"; Slot { objectName: "right-slot"; name: "right" } }
          Contribution { objectName: "moving"; slot: "left"; Item { objectName: "thing" } }
        }
        """
      When the property "slot" of "moving" is set to "right"
      Then the property "children.0.objectName" of "right" is "thing"
      And the property "count" of "right-slot" is 1
      And the property "count" of "left-slot" is 0

  Rule: A block's slot is a list of contributions it hands to a Slot

    Scenario: Contributions given to a block's slot show in its Slot
      Given the file "Toolbar.qml":
        """qml
        import QtQuick
        import Ljos.Blocks
        Row {
          property list<Contribution> tools
          Slot { name: "toolbar.tools"; contributions: parent.tools }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import Ljos.Blocks
        Item {
          objectName: "root"
          Toolbar {
            objectName: "bar"
            tools: [
              Contribution { Rectangle { objectName: "save"; width: 10; height: 10 } },
              Contribution { order: -1; Rectangle { objectName: "open"; width: 10; height: 10 } }
            ]
          }
        }
        """
      Then the property "x" of "bar/open" is 0
      And the property "x" of "bar/save" is 10
