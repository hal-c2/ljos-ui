# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Binding loops
  Two bindings that read each other are reported instead of looping forever.

  Scenario: A binding loop is reported in the log
    Given the file "Loop.qml":
      """qml
      import QtQuick
      Item {
        property int p: q + 1
        property int q: p + 1
      }
      """
    When "Loop.qml" is loaded
    Then the log has a warning matching "Binding loop detected"

  Scenario: Only the property declared first is reported
    Given the file "Loop.qml":
      """qml
      import QtQuick
      Item {
        property int p: q + 1
        property int q: p + 1
      }
      """
    When "Loop.qml" is loaded
    Then the log has a warning matching "Binding loop detected for property .p."
    And the log has no warning matching "Binding loop detected for property .q."

  Scenario: A binding loop stops after one round
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int p: q + 1
        property int q: p + 1
      }
      """
    Then the property "p" of "root" is 2
    And the property "q" of "root" is 3

  Scenario: A loop through three properties stops after one round
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int a: b + 1
        property int b: c + 1
        property int c: a + 1
      }
      """
    Then the property "a" of "root" is 3
    And the property "b" of "root" is 5
    And the property "c" of "root" is 4
    And the log has a warning matching "Binding loop detected for property .a."
    And the log has no warning matching "Binding loop detected for property .[bc]."

  Scenario: A property bound to itself is reported
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int count: count + 1
      }
      """
    Then the property "count" of "root" is 1
    And the log has a warning matching "Binding loop detected for property .count."

  Scenario: Bindings that read each other without changing are not reported
    Given the file "Still.qml":
      """qml
      import QtQuick
      Item {
        property int p: q * 0
        property int q: p * 0
        property int same: same * 1
      }
      """
    When "Still.qml" is loaded
    Then the log has no warnings

  Scenario: A later change goes round the loop once more
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int step: 0
        property int p: q + step
        property int q: p + 1
      }
      """
    When the property "step" of "root" is set to 5
    Then the property "p" of "root" is 7
    And the property "q" of "root" is 8

  Scenario: A loop on width and height is reported for the one written first
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: height + 1
        height: width + 1
      }
      """
    Then the property "width" of "root" is 2
    And the property "height" of "root" is 3
    And the log has a warning matching "Binding loop detected for property .width."
    And the log has no warning matching "Binding loop detected for property .height."

  Scenario: Qt.binding that closes a loop is reported for the new binding
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int p: 0
        property int q: 0
      }
      """
    When the property "p" of "root" is bound to "q + 1"
    And the property "q" of "root" is bound to "p + 1"
    Then the property "p" of "root" is 3
    And the property "q" of "root" is 2
    And the log has a warning matching "Binding loop detected for property .q."

  Scenario: An acyclic binding chain produces no warning
    Given the file "Chain.qml":
      """qml
      import QtQuick
      Item { property int p: 1; property int q: p + 1; property int r: q + 1 }
      """
    When "Chain.qml" is loaded
    Then the log has no warnings

  Scenario Outline: The order of declarations in a chain does not matter
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        <first>
        <second>
        <third>
      }
      """
    Then the property "r" of "root" is 3
    And the log has no warnings

    Examples:
      | first                 | second                | third                 |
      | property int p: 1     | property int q: p + 1 | property int r: q + 1 |
      | property int p: 1     | property int r: q + 1 | property int q: p + 1 |
      | property int r: q + 1 | property int q: p + 1 | property int p: 1     |
      | property int q: p + 1 | property int r: q + 1 | property int p: 1     |

  Scenario: A file with a syntax error is rejected
    Given the file "Broken.qml":
      """qml
      import QtQuick
      Item { property int p: }
      """
    Then loading "Broken.qml" is rejected

  # Found by ljos-fuzz (bindings), bucket warning:binding-loop(qt-only):Math.hypot+color.r.
  @backlog
  Scenario: An implicit height that reads the height is a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        implicitHeight: height + 1
      }
      """
    Then the log has a warning matching "Binding loop detected for property .implicitHeight."

  # Found by ljos-fuzz (bindings), bucket warning:binding-loop(ljos-only):isNaN.
  # A font member that reads another is a loop (giving it a value changes the font it read),
  # unless the member it reads was given a size the font ignores.
  Scenario: One member of a font that reads another is a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        Text {
          id: label
          font.pixelSize: 12
          font.italic: label.font.pixelSize > 10
        }
      }
      """
    Then the log has a warning matching "Binding loop detected for property .font.italic."

  @backlog
  Scenario: One member of a font that reads a size the font ignored is not a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        Text {
          id: label
          objectName: "label"
          font.pixelSize: 0
          font.italic: isNaN(label.font.pixelSize)
        }
      }
      """
    Then the property "font.italic" of "label" is false
    And the log has no warning matching "Binding loop"

  # Found by ljos-fuzz (bindings), buckets warning:binding-loop(qt-only):Qt.rect+alias+object-1
  # and warning:binding-loop(qt-only):Qt.colorEqual+Qt.rgba-computed+alias.
  # Qt 6.11 reports a loop for a binding of a child that reads both an alias and the bound
  # property it names, and keeps the value the binding had before the alias was read (1 here,
  # not 2). Nothing loops: Ljós computes the binding once and says nothing.
  @qt-diverges
  Scenario: An alias and the property it names read together are not a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        property real p: 1 + 0
        property alias a: root.p
        Item {
          objectName: "child"
          property real both: root.a + root.p
        }
      }
      """
    Then the property "both" of "child" is 2
    And the log has no warning matching "Binding loop"

  # Found by ljos-fuzz (bindings), bucket
  # warning:binding-loop(qt-only):Qt.lighter+array-includes+color.r.
  # While the object is made, a handler of the first binding writes what a later binding
  # reads; Qt reports the later binding.
  @backlog
  Scenario: A handler run while the object is made that writes what a binding reads is a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        property real a: Qt.lighter("steelblue").r
        onAChanged: { other.l = [] }
        Item {
          id: other
          property list<real> l
          property bool has: l.includes(root.a)
        }
      }
      """
    Then the log has a warning matching "Binding loop detected for property .has."
