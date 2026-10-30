# Sources:
#   https://doc.qt.io/qt-6/qtgraphs-index.html
#   https://doc.qt.io/qt-6/qml-qtgraphs-graphsview.html
#   https://doc.qt.io/qt-6/qtlocation-index.html
#   docs/ARCHITECTURE.md §1 Goals and non-goals
# Qt 3D, WebEngine, QtLocation and QtGraphs are non-goals; QtQuick.Timeline
# is not built yet. Qt Quick 3D is built (features/quick3d), and of its
# Helpers ExtendedSceneEnvironment, but not its Effects, Particles3D and
# other add-on modules.
# Under Qt a GraphsView is an Item that shows a chart; here the
# application around it is what matters.

@ljos-only
Feature: The types of a Qt module Ljós does not have stand in as empty items
  An application that shows a chart in one corner (Qt Graphs) is still
  mostly an application of buttons, lists and text. Ljós does not draw
  charts, so a type of such a module stands in as an empty Item: where
  it is placed, the size it is given, its ids, its children and the
  properties, signals and functions the file declares on it are kept,
  and what only the missing type has (an axis, a series, a map plugin) is
  left out. The rest of the application loads and runs around it. The
  log says once, for each such module, that it is not drawn. A timeline
  (QtQuick.Timeline), which Ljós has not built yet, is left out the same
  way: the application shows where it starts.

  Background:
    Given the file "Scene.qml":
      """qml
      import QtQuick
      import QtGraphs
      GraphsView {
        id: view
        property int turns: 3
        axisX: ValueAxis { id: axis; max: 10 }
        LineSeries { XYPoint { x: 0; y: 0 } XYPoint { x: 10; y: 5 } }
      }
      """

  Scenario: The rest of the window loads around a chart
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        width: 200; height: 100
        RowLayout {
          anchors.fill: parent
          spacing: 0
          Scene { objectName: "scene"; Layout.preferredWidth: 120; Layout.fillHeight: true }
          Rectangle { objectName: "side"; color: "#2060a0"; Layout.fillWidth: true; Layout.fillHeight: true }
        }
      }
      """
    Then the property "width" of "scene" is 120
    And the property "turns" of "scene" is 3
    And the property "x" of "side" is 120
    And the pixel at 160, 50 of the window is "#2060a0"
    And the log has 1 warnings matching "QtGraphs"

  Scenario: One warning for a module however many files import it
    Given the file "Other.qml":
      """qml
      import QtQuick
      import QtGraphs
      BarSet { property string label: "other" }
      """
    And the QML:
      """qml
      import QtQuick
      Column {
        Scene { objectName: "first"; width: 10; height: 10 }
        Scene { objectName: "second"; width: 10; height: 10 }
        Other { objectName: "other" }
      }
      """
    Then the property "label" of "other" is "other"
    And the log has 1 warnings matching "QtGraphs"

  Scenario: An application animated by a timeline shows where it starts
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Timeline
      Rectangle {
        id: box
        width: 100; height: 100; color: "#2060a0"
        Timeline {
          startFrame: 0; endFrame: 100; enabled: true
          KeyframeGroup {
            target: box; property: "opacity"
            Keyframe { frame: 0; value: 0 }
          }
        }
      }
      """
    Then the pixel at 50, 50 of the window is "#2060a0"
    And the log has 1 warnings matching "QtQuick.Timeline"

  # Qt makes no object that leaves a required property unset: one written
  # so where a value goes can only be a template, given to the type to
  # make (a delegate), and the type is not there to make it.
  Scenario: A delegate written on a type that stands in is not made
    Given the QML:
      """qml
      import QtQuick
      import QtLocation
      Item {
        id: root
        objectName: "root"; width: 100; height: 100
        property int made: 0
        MapItemView {
          model: 3
          delegate: MapCircle {
            required property int index
            Component.onCompleted: root.made++
          }
        }
      }
      """
    Then the property "made" of "root" is 0
    And the log has 1 warnings matching "QtLocation"
    And the log has no warning matching "Required property"

  # Qt names a property that takes a Component `delegate`, or ends it in
  # `Delegate` or `Component`; what is written there is made by the type
  # that is missing, so it is not made at all, as no model drives it.
  Scenario: What is written for the delegate of a missing type is not made
    Given the QML:
      """qml
      import QtQuick
      import QtLocation
      Item {
        id: root
        objectName: "root"
        property int made: 0
        MapCircle {
          MapItemView {
            model: 2
            delegate: MapCircle { Component.onCompleted: root.made += 1 }
          }
        }
      }
      """
    Then the property "made" of "root" is 0
    And the log has 1 warnings matching "QtLocation"

  Scenario: A property declared with a missing type holds what stands in for it
    Given the QML:
      """qml
      import QtQuick
      import QtLocation
      Item {
        objectName: "root"
        Map { id: view; width: 30; height: 20 }
        property Map shown: view
        property list<MapCircle> nodes: [ MapCircle { }, MapCircle { } ]
        property real shownWidth: shown ? shown.width : -1
        property int nodeCount: nodes.length
      }
      """
    Then the property "shownWidth" of "root" is 30
    And the property "nodeCount" of "root" is 2
    And the log has no warning matching "Unable to assign"
