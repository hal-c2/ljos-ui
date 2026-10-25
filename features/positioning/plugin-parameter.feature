# Sources:
#   https://doc.qt.io/qt-6/qml-qtpositioning-pluginparameter.html
#   https://doc.qt.io/qt-6/qml-qtpositioning-positionsource.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Parameters for a backend
  A PluginParameter is a name and a value handed to a backend. QtLocation's
  Plugin takes them as children, and so does a PositionSource, whose
  `parameters` lists them. Each of the two is kept as it is first given:
  a later write is ignored, and once both are there the parameter says
  it is initialized.

  @api-QtPositioning.PluginParameter @api-QtPositioning.PositionSource.parameters
  Scenario: The parameters declared in a source are its parameters
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        PositionSource {
          id: source
          PluginParameter { name: "a"; value: "b" }
          PluginParameter { name: "c"; value: 42 }
        }
        property var names: [source.parameters.length, source.parameters[0].name,
          source.parameters[1].name]
        property var values: [source.parameters[0].value, source.parameters[1].value]
      }
      """
    Then the property "names" of "root" is [2, "a", "c"]
    And the property "values" of "root" is ["b", 42]

  @api-QtPositioning.PluginParameter.name @api-QtPositioning.PluginParameter.value
  Scenario: A name and a value are kept as they are first given
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property string log: ""
        property var seen: []
        PluginParameter {
          id: given; name: "n"; value: 1
          onNameChanged: log += "N"; onValueChanged: log += "V"; onInitialized: log += "I"
        }
        PluginParameter {
          id: later
          onNameChanged: log += "n"; onValueChanged: log += "v"; onInitialized: log += "i"
        }
        Component.onCompleted: {
          given.name = "m"; given.value = 2
          seen.push(typeof later.value, later.name)
          later.value = 3; later.name = ""; later.value = 4
          seen.push(log)
          later.name = "a"; later.name = "b"
          seen = seen.concat([given.name, given.value, later.name, later.value])
        }
      }
      """
    Then the property "seen" of "root" is ["undefined", "", "v", "n", 1, "a", 3]
    And the property "log" of "root" is "vni"

  @api-QtPositioning.Position
  Scenario: A position may be declared on its own
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        Position { id: p }
        property var known: [p.coordinate.isValid, p.latitudeValid, p.speedValid]
      }
      """
    Then the property "known" of "root" is [false, false, false]
