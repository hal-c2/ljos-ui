# Sources:
#   https://doc.qt.io/qt-6/qml-qtpositioning-positionsource.html
#   https://doc.qt.io/qt-6/qml-qtpositioning-position.html
# The qml6 these were written against finds the system's geoclue2 service,
# and no step can give Qt no source or a position a scenario says, so the
# scenarios run on Ljós only. Without a source a PositionSource is what Qt
# documents: not valid, and start(), stop() and update() do nothing.

@ljos-only
Feature: A position source, fed by the system
  A PositionSource reads the system's position service. Without one, as
  on a desktop until its host installs a backend, the source is not
  valid, has no name and supports no positioning methods, and starting it
  does nothing. With one, a started source is given each fix the system
  has, at the next frame, as its `position`; a stopped one is given none.
  `update()` asks for one fix, and stays active only until it comes.

  Scenario: Without a position source on the system, nothing starts
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        PositionSource {
          id: source; objectName: "source"
          property int fixes: 0
          property bool placed: position.coordinate.isValid
          property var asked: [backendProperty("anything") === undefined, setBackendProperty("anything", 1)]
          onPositionChanged: fixes++
          function startAll() { start(); update() }
        }
      }
      """
    When the method "startAll" of "source" is called
    And 100 milliseconds pass
    Then the property "valid" of "source" is false
    And the property "name" of "source" is ""
    And the property "supportedPositioningMethods" of "source" is 0
    And the property "sourceError" of "source" is 3
    And the property "active" of "source" is false
    And the property "fixes" of "source" is 0
    And the property "placed" of "source" is false
    And the property "asked" of "source" is [true, false]

  Scenario: A source that is not started is given no fix
    Given the system's position is latitude 64.1466, longitude -21.9426
    And the QML:
      """qml
      import QtQuick
      import QtPositioning
      PositionSource {
        objectName: "source"
        property int fixes: 0
        property bool placed: position.coordinate.isValid
        onPositionChanged: fixes++
      }
      """
    When 100 milliseconds pass
    Then the property "valid" of "source" is true
    And the property "name" of "source" is "fixed"
    And the property "supportedPositioningMethods" of "source" is -1
    And the property "active" of "source" is false
    And the property "fixes" of "source" is 0
    And the property "placed" of "source" is false

  Scenario: A started source is given the system's position, and where it moves
    Given the system's position is latitude 64.1466, longitude -21.9426
    And the QML:
      """qml
      import QtQuick
      import QtPositioning
      PositionSource {
        objectName: "source"
        property int fixes: 0
        property geoCoordinate here: position.coordinate
        property real latitude: here.latitude
        property real longitude: here.longitude
        property var valid: [position.latitudeValid, position.longitudeValid, position.altitudeValid]
        property bool dated: !isNaN(position.timestamp.getTime())
        onPositionChanged: fixes++
      }
      """
    When the method "start" of "source" is called
    And 100 milliseconds pass
    Then the property "active" of "source" is true
    And the property "fixes" of "source" is 1
    And the property "latitude" of "source" is 64.1466
    And the property "longitude" of "source" is -21.9426
    And the property "valid" of "source" is [true, true, false]
    And the property "dated" of "source" is true
    When the system's position moves to latitude 51.5074, longitude -0.1278, altitude 11
    Then the property "fixes" of "source" is 2
    And the property "latitude" of "source" is 51.5074
    And the property "valid" of "source" is [true, true, true]

  Scenario: A stopped source keeps its last fix and is given no more
    Given the system's position is latitude 64.1466, longitude -21.9426
    And the QML:
      """qml
      import QtQuick
      import QtPositioning
      PositionSource {
        objectName: "source"
        active: true
        property int fixes: 0
        property real latitude: position.coordinate.latitude
        onPositionChanged: fixes++
      }
      """
    When 100 milliseconds pass
    And the method "stop" of "source" is called
    And the system's position moves to latitude 51.5074, longitude -0.1278
    Then the property "active" of "source" is false
    And the property "fixes" of "source" is 1
    And the property "latitude" of "source" is 64.1466

  Scenario: update() asks for one fix
    Given the system's position is latitude 64.1466, longitude -21.9426
    And the QML:
      """qml
      import QtQuick
      import QtPositioning
      PositionSource {
        objectName: "source"
        property int fixes: 0
        property bool activeWhileAsking
        property real latitude: position.coordinate.latitude
        onPositionChanged: fixes++
        function ask() { update(); activeWhileAsking = active }
      }
      """
    When the method "ask" of "source" is called
    And 100 milliseconds pass
    Then the property "activeWhileAsking" of "source" is true
    And the property "active" of "source" is false
    And the property "fixes" of "source" is 1
    And the property "latitude" of "source" is 64.1466
    When the system's position moves to latitude 51.5074, longitude -0.1278
    Then the property "fixes" of "source" is 1
