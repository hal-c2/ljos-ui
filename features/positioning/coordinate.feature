# Sources:
#   https://doc.qt.io/qt-6/qml-geocoordinate.html
#   https://doc.qt.io/qt-6/qml-qtpositioning-qtpositioning.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A coordinate on the Earth
  A `geoCoordinate` is a latitude and a longitude in degrees and an
  altitude in metres. It is valid when the latitude is within ±90 and the
  longitude within ±180; the altitude may be left out, and is then NaN.
  Distances are along a great circle of a sphere the Earth's size, and the
  altitude plays no part in them. As text it is degrees, minutes and
  seconds with the hemisphere.

  Scenario: A coordinate made by QtPositioning.coordinate
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property var c: QtPositioning.coordinate(64.1466, -21.9426, 10)
        property real latitude: c.latitude
        property real longitude: c.longitude
        property real altitude: c.altitude
        property bool valid: c.isValid
        property string text: c.toString()
        property string json: JSON.stringify(c)
        property bool flat: isNaN(QtPositioning.coordinate(1, 2).altitude)
      }
      """
    Then the property "latitude" of "root" is 64.1466
    And the property "longitude" of "root" is -21.9426
    And the property "altitude" of "root" is 10
    And the property "valid" of "root" is true
    And the property "text" of "root" is "64° 8' 47.8\" N, 21° 56' 33.4\" W, 10m"
    And the property "json" of "root" is "{\"latitude\":64.1466,\"longitude\":-21.9426,\"altitude\":10,\"isValid\":true}"
    And the property "flat" of "root" is true

  Scenario: A coordinate property starts invalid
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property geoCoordinate none
        property bool valid: none.isValid
        property bool unknown: isNaN(none.latitude) && isNaN(none.longitude) && isNaN(none.altitude)
        property string text: none.toString()
        property bool tooFar: QtPositioning.coordinate(91, 0).isValid
          || QtPositioning.coordinate(0, 181).isValid
        property bool edge: QtPositioning.coordinate(-90, 180).isValid
      }
      """
    Then the property "valid" of "root" is false
    And the property "unknown" of "root" is true
    And the property "text" of "root" is ""
    And the property "tooFar" of "root" is false
    And the property "edge" of "root" is true

  Scenario: Coordinates with the same parts are the same
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property bool same: QtPositioning.coordinate(1, 2) === QtPositioning.coordinate(1, 2)
        property bool higher: QtPositioning.coordinate(1, 2) === QtPositioning.coordinate(1, 2, 3)
        property bool unset: QtPositioning.coordinate() === QtPositioning.coordinate()
      }
      """
    Then the property "same" of "root" is true
    And the property "higher" of "root" is false
    And the property "unset" of "root" is true

  Scenario: A coordinate's parts are written one at a time
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property geoCoordinate c: QtPositioning.coordinate(10, 20)
        property real latitude: c.latitude
        function move() { c.latitude = 30; c.altitude = 5 }
      }
      """
    When the method "move" of "root" is called
    Then the property "latitude" of "root" is 30
    And the property "c" of "root" is {"latitude": 30, "longitude": 20, "altitude": 5, "isValid": true}

  Scenario: Text rounds the seconds and shows the altitude as it is
    Given the QML:
      """qml
      import QtQuick
      import QtPositioning
      Item {
        objectName: "root"
        property string pole: QtPositioning.coordinate(89.99999, 0).toString()
        property string equator: QtPositioning.coordinate(0, 0, 1.5).toString()
        property string below: QtPositioning.coordinate(-33.5, 151.25, -3).toString()
      }
      """
    Then the property "pole" of "root" is "90° 0' 0.0\" N, 0° 0' 0.0\""
    And the property "equator" of "root" is "0° 0' 0.0\", 0° 0' 0.0\", 1.5m"
    And the property "below" of "root" is "33° 30' 0.0\" S, 151° 15' 0.0\" E, -3m"

  Rule: Distances and bearings are along a great circle

    Scenario: The distance and the azimuth to another coordinate
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var origin: QtPositioning.coordinate(0, 0)
          property real degree: origin.distanceTo(QtPositioning.coordinate(0, 1))
          property real across: QtPositioning.coordinate(64.1466, -21.9426)
            .distanceTo(QtPositioning.coordinate(51.5074, -0.1278))
          property real half: origin.distanceTo(QtPositioning.coordinate(0, 180))
          property real high: QtPositioning.coordinate(0, 0, 1000).distanceTo(QtPositioning.coordinate(0, 1))
          property var bearings: [origin.azimuthTo(QtPositioning.coordinate(0, 1)),
            origin.azimuthTo(QtPositioning.coordinate(1, 0)),
            origin.azimuthTo(QtPositioning.coordinate(0, -1)),
            origin.azimuthTo(origin)]
          property real toNowhere: origin.distanceTo(QtPositioning.coordinate())
        }
        """
      Then the property "degree" of "root" is within 0.001 of 111195.0523
      And the property "across" of "root" is within 0.001 of 1889740.4543
      And the property "half" of "root" is within 0.001 of 20015109.4155
      And the property "high" of "root" is within 0.001 of 111195.0523
      And the property "bearings" of "root" is [90, 0, 270, 0]
      And the property "toNowhere" of "root" is 0

    Scenario: The coordinate a distance away in a direction
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var there: QtPositioning.coordinate(0, 0, 5).atDistanceAndAzimuth(1000, 45)
          property string text: there.toString()
          property real back: there.distanceTo(QtPositioning.coordinate(0, 0))
          property bool fromNowhere: QtPositioning.coordinate().atDistanceAndAzimuth(1000, 45).isValid
        }
        """
      Then the property "text" of "root" is "0° 0' 22.9\" N, 0° 0' 22.9\" E, 5m"
      And the property "back" of "root" is within 0.001 of 1000
      And the property "fromNowhere" of "root" is false

  Rule: QtPositioning turns coordinates into Web Mercator and back

    Scenario: To Mercator and back
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var m: QtPositioning.coordToMercator(QtPositioning.coordinate(45, 90))
          property real mx: m.x
          property real my: m.y
          property var c: QtPositioning.mercatorToCoord(Qt.point(0.25, 0.4))
          property real latitude: c.latitude
          property real longitude: c.longitude
          property real altitude: c.altitude
        }
        """
      Then the property "mx" of "root" is 0.75
      And the property "my" of "root" is within 0.0000001 of 0.3597250
      And the property "latitude" of "root" is within 0.0000001 of 33.8412203
      And the property "longitude" of "root" is -90
      And the property "altitude" of "root" is 0
