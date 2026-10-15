# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-synchronizer-synchronizer.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Synchronizer keeps properties in step without breaking bindings
  A `Synchronizer` (`Qt.labs.synchronizer`) ties two or more properties
  together: a change of any of them is written to the others, and a
  binding on any of them stays in place. When it is created the source
  gives its value to the others; the source is an alias named `source`,
  else the `sourceObject`/`sourceProperty` pair, else the property it is
  declared `on`. Values are converted as QML converts them. A property
  that answers a write with another value has bounced it, and one that
  gives no change signal has ignored it; either is reported by a signal and
  not passed on.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.synchronizer
      Item {
        id: root
        objectName: "root"
        property int base: 3
        property int a: base * 2
        property int b: 1
        property string s: "7"
        property string bouncedName: ""
        property int bounced: 0
        QtObject {
          id: clamp
          objectName: "clamp"
          property int v: 0
          onVChanged: if (v > 10) v = 10
        }
        Synchronizer {
          sourceObject: root; sourceProperty: "a"
          targetObject: root; targetProperty: "b"
        }
        Synchronizer {
          sourceObject: root; sourceProperty: "s"
          targetObject: clamp; targetProperty: "v"
          onValueBounced: (o, p) => { root.bounced++; root.bouncedName = p }
        }
      }
      """

  Scenario: The target starts with the source's value, converted
    Then the property "b" of "root" is 6
    And the property "v" of "clamp" is 7
    And the property "bounced" of "root" is 0

  Scenario: A change of the target is written to the source
    When the property "b" of "root" is set to 20
    Then the property "a" of "root" is 20
    And the property "base" of "root" is 3

  Scenario: The source's binding stays in place
    When the property "b" of "root" is set to 20
    And the property "base" of "root" is set to 10
    Then the property "a" of "root" is 20
    And the property "b" of "root" is 20

  Scenario: A value the target refuses is not passed back
    When the property "s" of "root" is set to "42"
    Then the property "v" of "clamp" is 10
    And the property "s" of "root" is "42"
    And the property "bounced" of "root" is 1
    And the property "bouncedName" of "root" is "v"

  Scenario: A change of the target is converted for the source
    When the property "v" of "clamp" is set to 4
    Then the property "s" of "root" is "4"

  Scenario: Nothing is logged
    Then the log has no warnings

  Rule: The source is an alias named source, then the pair, then the property it is on

    Scenario: An alias named source wins over the property it is on
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.synchronizer
        Item {
          id: root
          objectName: "aliased"
          property int q1: 11
          property int q2: 12
          Synchronizer on q1 { property alias source: root.q2 }
        }
        """
      Then the property "q1" of "aliased" is 12
      And the property "q2" of "aliased" is 12

    Scenario: The pair wins over the property it is on, and every alias follows
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.synchronizer
        Item {
          id: root
          objectName: "paired"
          property int p1: 1
          property int p2: 2
          property int p3: 3
          property int p4: 4
          Synchronizer on p1 {
            sourceObject: root; sourceProperty: "p2"
            targetObject: root; targetProperty: "p3"
            property alias other: root.p4
          }
        }
        """
      Then the property "p1" of "paired" is 2
      And the property "p3" of "paired" is 2
      And the property "p4" of "paired" is 2
      When the property "p4" of "paired" is set to 40
      Then the property "p1" of "paired" is 40
      And the property "p2" of "paired" is 40
      And the property "p3" of "paired" is 40

    Scenario: The property it is on is the source when nothing else is
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.synchronizer
        Item {
          id: root
          objectName: "onlyOn"
          property int r1: 21
          property int r2: 22
          Synchronizer on r1 { property alias x: root.r2 }
        }
        """
      Then the property "r2" of "onlyOn" is 21
      When the property "r2" of "onlyOn" is set to 5
      Then the property "r1" of "onlyOn" is 5

  Rule: A property that gives no change signal has ignored the value

    Scenario: A string that is not a number gives an int 0, and then nothing
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.synchronizer
        Item {
          id: root
          objectName: "ignoring"
          property string t: "1"
          property int i: 5
          property string ignoredName: ""
          Synchronizer {
            sourceObject: root; sourceProperty: "t"
            targetObject: root; targetProperty: "i"
            onValueIgnored: (o, p) => root.ignoredName = p
          }
        }
        """
      When the property "t" of "ignoring" is set to "y"
      Then the property "i" of "ignoring" is 0
      And the property "ignoredName" of "ignoring" is ""
      When the property "t" of "ignoring" is set to "z"
      Then the property "i" of "ignoring" is 0
      And the property "ignoredName" of "ignoring" is "i"
