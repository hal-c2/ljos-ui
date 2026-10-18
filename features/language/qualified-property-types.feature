# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-attributes
#   https://doc.qt.io/qt-6/qtqml-syntax-imports.html#importing-into-a-qualified-local-namespace
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A property typed through the namespace of an import
  A property may name its object type through the namespace an import gave
  its module, as in `property T.Control control`. It holds an object, not
  a number, and a user of the file can bind it to one.

  Background:
    Given the file "Holder.qml":
      """qml
      import QtQuick as Q
      import QtQuick.Templates as T
      Q.Item {
        property T.Control control
        property Q.Item item
        readonly property string controlText: control ? control.objectName : "none"
      }
      """

  Scenario: The properties take the objects bound to them
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        objectName: "root"
        Button { id: control; objectName: "button" }
        Holder {
          id: holder
          control: control
          item: control
        }
        property bool same: holder.control === control && holder.item === control
        property string read: holder.controlText
      }
      """
    Then the property "same" of "root" is true
    And the property "read" of "root" is "button"
    And the log has no warnings

  Scenario: An unset one is null
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Holder { id: holder }
        property bool empty: holder.control === null && holder.item === null
      }
      """
    Then the property "empty" of "root" is true
