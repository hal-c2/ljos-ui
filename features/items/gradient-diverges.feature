# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-gradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-gradientstop.html
#   https://doc.qt.io/qt-6/qml-qtquick-gradient.html#updated-signal
#
# A binding that reads a stop: Qt 6.11.2 re-evaluates a binding only when a
# property with a change signal changes, and a stop has none, so on Qt
# `seen` keeps the first colour and the log says the expression "depends on
# non-bindable properties". Every property of Ljós can be followed, and
# nothing here is worth giving that up.
#
# `updated`: Qt tells it for every assignment to a stop, whether or not the
# value is new. Ljós tells it when what the gradient paints has changed.

@qt-diverges
Feature: Where a gradient's stops differ from Qt
  A stop of a gradient can be followed by a binding like any other
  property, and a gradient says that it was updated only when what it
  paints has changed.

  Scenario: A binding that reads a stop follows it
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "box"
        property alias topColor: top.color
        property color seen: top.color
        gradient: Gradient { GradientStop { id: top; color: "#ff0000" } }
      }
      """
    When the property "topColor" of "box" is set to "#00ff00"
    Then the property "seen" of "box" is "#00ff00"

  Scenario: Assigning the colour a stop already has is not an update
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: box
        objectName: "box"
        property int updates: 0
        property bool ready: false
        property alias topColor: top.color
        gradient: Gradient {
          onUpdated: if (box.ready) box.updates++
          GradientStop { id: top; color: "#ff0000" }
        }
        Component.onCompleted: ready = true
      }
      """
    When the property "topColor" of "box" is set to "#ff0000"
    Then the property "updates" of "box" is 0
