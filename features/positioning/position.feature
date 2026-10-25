# Sources:
#   https://doc.qt.io/qt-6/qml-qtpositioning-positionsource.html
#   https://doc.qt.io/qt-6/qml-qtpositioning-position.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a position source says before it has a fix
  A PositionSource's `position` is an object of its own. Until a fix
  comes it knows nothing: its coordinate is invalid, every number is NaN,
  every part is marked not valid and the timestamp is an invalid date.

  Scenario: The names of the positioning methods and of the errors
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property var methods: [PositionSource.NoPositioningMethods, PositionSource.SatellitePositioningMethods,
          PositionSource.NonSatellitePositioningMethods, PositionSource.AllPositioningMethods]
        property var errors: [PositionSource.AccessError, PositionSource.ClosedError,
          PositionSource.UnknownSourceError, PositionSource.NoError, PositionSource.UpdateTimeoutError]
      }
      """
    Then the property "methods" of "root" is [0, 255, -256, -1]
    And the property "errors" of "root" is [0, 1, 2, 3, 4]

  Scenario: Before a fix the position knows nothing
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        PositionSource { id: source }
        property var p: source.position
        property bool unplaced: !p.coordinate.isValid && !p.latitudeValid && !p.longitudeValid
          && !p.altitudeValid
        property var flags: [p.speedValid, p.directionValid, p.verticalSpeedValid,
          p.horizontalAccuracyValid, p.verticalAccuracyValid, p.magneticVariationValid,
          p.directionAccuracyValid]
        property bool unmeasured: [p.speed, p.direction, p.verticalSpeed, p.horizontalAccuracy,
          p.verticalAccuracy, p.magneticVariation, p.directionAccuracy].every(isNaN)
        property bool undated: p.timestamp instanceof Date && isNaN(p.timestamp.getTime())
        property int interval: source.updateInterval
        property int preferred: source.preferredPositioningMethods
      }
      """
    Then the property "unplaced" of "root" is true
    And the property "flags" of "root" is [false, false, false, false, false, false, false]
    And the property "unmeasured" of "root" is true
    And the property "undated" of "root" is true
    And the property "interval" of "root" is 0
    And the property "preferred" of "root" is -1
