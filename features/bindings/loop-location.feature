# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where a binding loop is reported
  The warning names the file, line and column where the looping binding is written: the place of
  the property name, not of the expression after it.

  Columns count from the start of the line as it stands in this file, so they include the six
  spaces every QML line is indented by here.

  Scenario: A loop on a declared property names the object and then the declaration
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
          property int p:    q + 1
        property int q: p + 1
      }
      """
    Then the log has a warning matching "(?s)Scenario.qml:2:7: QML Scenario: Binding loop detected for property .p.:.*Scenario.qml:4:11$"

  Scenario: A loop on width is reported in one line at the property name
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
           width:    height + 1
        height: width + 1
      }
      """
    Then the log has a warning matching "Scenario.qml:4:12: QML Scenario: Binding loop detected for property .width.$"

  Scenario: A loop on a dotted name is placed at its first name
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        radius: border.width + 1
            border.width:   radius + 1
      }
      """
    Then the log has a warning matching "(?s)Scenario.qml:2:7: QML Scenario .*: Binding loop detected for property .width.:.*Scenario.qml:4:13$"

  Scenario: A loop on a name inside a group is placed at that name
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        radius: border.width + 1
        border {
              width: radius + 1
        }
      }
      """
    Then the log has a warning matching "(?s)Binding loop detected for property .width.:.*Scenario.qml:5:15$"

  Scenario: A loop written in a component file is placed in that file
    Given the file "Base.qml":
      """qml
      import QtQuick
      Item {
          property int m:   k + 1
        property int k: m + 1
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        Base {}
      }
      """
    Then the log has a warning matching "(?s)Scenario.qml:3:9: QML Base: Binding loop detected for property .m.:.*Base.qml:3:11$"

  Scenario: A loop written where a component is used is placed there
    Given the file "Plain.qml":
      """qml
      import QtQuick
      Item {
        property int n: 1
        property int o: n
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        Plain {
                n: o + 1
          o: n + 1
        }
      }
      """
    Then the log has a warning matching "(?s)Scenario.qml:3:9: QML Plain: Binding loop detected for property .n.:.*Scenario.qml:4:17$"

  Scenario: A binding that replaces the component's own is the one that is placed
    Given the file "Plain.qml":
      """qml
      import QtQuick
      Item {
        property int n: 1
        property int o: n
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        Plain {
             n: o + 1
        }
      }
      """
    Then the log has a warning matching "(?s)Binding loop detected for property .n.:.*Scenario.qml:4:14$"
    And the log has no warning matching "Plain.qml:\d+:\d+$"

  Scenario: A loop across two objects is reported for the later object
    Given the QML:
      """qml
      import QtQuick
      Item {
        Item { id: a; objectName: "a"; property int n: b.n + 1 }
        Item { id: b; objectName: "b"; property int n: a.n + 1 }
      }
      """
    Then the log has a warning matching "(?s)Scenario.qml:4:9: QML Item: Binding loop detected for property .n.:.*Scenario.qml:4:40$"
    And the property "n" of "a" is 3
    And the property "n" of "b" is 2

  Scenario: A loop over the widths of three objects is reported for the last object
    Given the QML:
      """qml
      import QtQuick
      Item {
        Item { id: a; objectName: "a"; width: c.width + 1 }
        Item { id: b; objectName: "b"; width: a.width + 1 }
        Item { id: c; objectName: "c"; width: b.width + 1 }
      }
      """
    Then the log has a warning matching "Scenario.qml:5:40: QML Item: Binding loop detected for property .width.$"
    And the property "width" of "a" is 4
    And the property "width" of "b" is 5
    And the property "width" of "c" is 3

  Scenario: A loop through a height and a declared property is reported for the height
    Given the QML:
      """qml
      import QtQuick
      Item {
        Item { id: a; objectName: "a"; property int n: b.height + 1 }
        Item { id: b; objectName: "b"; height: a.n + 1 }
      }
      """
    Then the log has a warning matching "Scenario.qml:4:40: QML Item: Binding loop detected for property .height.$"
    And the log has no warning matching "Binding loop detected for property .n."
    And the property "n" of "a" is 3
    And the property "height" of "b" is 2
