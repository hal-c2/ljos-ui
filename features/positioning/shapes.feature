# Sources:
#   https://doc.qt.io/qt-6/qml-georectangle.html
#   https://doc.qt.io/qt-6/qml-geocircle.html
#   https://doc.qt.io/qt-6/qml-geopath.html
#   https://doc.qt.io/qt-6/qml-geopolygon.html
#   https://doc.qt.io/qt-6/qml-geoshape.html
#   https://doc.qt.io/qt-6/qml-qtpositioning-qtpositioning.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Areas and paths on the Earth
  `geoRectangle`, `geoCircle`, `geoPath` and `geoPolygon` are shapes made
  of coordinates; `geoShape` holds any of them. Each says its `type`,
  whether it is valid and empty, its centre, whether it contains a
  coordinate and the rectangle that bounds it. QtPositioning makes them,
  and turns a shape back into the type it holds.

  Rule: A rectangle runs from its top left corner to its bottom right

    Scenario: A rectangle and what it is made of
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var r: QtPositioning.rectangle(QtPositioning.coordinate(10, 0), QtPositioning.coordinate(0, 10))
          property var parts: [r.type, r.width, r.height, r.center.latitude, r.center.longitude,
            r.bottomLeft.latitude, r.bottomLeft.longitude, r.topRight.latitude, r.topRight.longitude]
          property bool valid: r.isValid
          property bool empty: r.isEmpty
          property string text: r.toString()
          property var inside: [r.contains(QtPositioning.coordinate(5, 5)),
            r.contains(QtPositioning.coordinate(10, 10)), r.contains(QtPositioning.coordinate(11, 5))]
          property geoRectangle none
          property var noneIs: [none.isValid, none.isEmpty]
          property var flat: QtPositioning.rectangle(QtPositioning.coordinate(5, 5), QtPositioning.coordinate(5, 5))
          property var flatIs: [flat.isValid, flat.isEmpty]
          property bool upsideDown: QtPositioning.rectangle(QtPositioning.coordinate(0, 0),
            QtPositioning.coordinate(10, 10)).isValid
        }
        """
      Then the property "parts" of "root" is [1, 10, 10, 5, 5, 0, 0, 10, 10]
      And the property "valid" of "root" is true
      And the property "empty" of "root" is false
      And the property "text" of "root" is "QGeoRectangle({10, 0}, {0, 10})"
      And the property "inside" of "root" is [true, true, false]
      And the property "noneIs" of "root" is [false, true]
      And the property "flatIs" of "root" is [true, true]
      And the property "upsideDown" of "root" is false

    Scenario: A rectangle across the date line
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var r: QtPositioning.rectangle(QtPositioning.coordinate(10, 170), QtPositioning.coordinate(0, -170))
          property real span: r.width
          property bool holdsTheLine: r.contains(QtPositioning.coordinate(5, 180))
        }
        """
      Then the property "span" of "root" is 20
      And the property "holdsTheLine" of "root" is true

    Scenario: A rectangle around a centre, and around coordinates
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property string around: QtPositioning.rectangle(QtPositioning.coordinate(5, 5), 4, 2).toString()
          property string bounding: QtPositioning.rectangle([QtPositioning.coordinate(1, 2),
            QtPositioning.coordinate(-3, 8), QtPositioning.coordinate(4, 5)]).toString()
        }
        """
      Then the property "around" of "root" is "QGeoRectangle({6, 3}, {4, 7})"
      And the property "bounding" of "root" is "QGeoRectangle({4, 2}, {-3, 8})"

    Scenario: Writing a part of a rectangle
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property geoRectangle wide: QtPositioning.rectangle(QtPositioning.coordinate(10, 0), QtPositioning.coordinate(0, 10))
          property geoRectangle corner: QtPositioning.rectangle(QtPositioning.coordinate(10, 0), QtPositioning.coordinate(0, 10))
          property geoRectangle moved: QtPositioning.rectangle(QtPositioning.coordinate(10, 0), QtPositioning.coordinate(0, 10))
          function write() {
            wide.width = 4
            corner.topLeft = QtPositioning.coordinate(20, -5)
            moved.center = QtPositioning.coordinate(50, 50)
          }
          property var texts: [wide.toString(), corner.toString(), moved.toString()]
        }
        """
      When the method "write" of "root" is called
      Then the property "texts" of "root" is ["QGeoRectangle({10, 3}, {0, 7})", "QGeoRectangle({20, -5}, {0, 10})", "QGeoRectangle({55, 45}, {45, 55})"]

  Rule: A circle is the coordinates within a radius of its centre

    Scenario: A circle
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var c: QtPositioning.circle(QtPositioning.coordinate(0, 0), 200000)
          property var parts: [c.type, c.radius, c.center.latitude, c.isValid, c.isEmpty]
          property string text: c.toString()
          property var inside: [c.contains(QtPositioning.coordinate(0, 1)), c.contains(QtPositioning.coordinate(0, 2))]
          property var box: c.boundingGeoRectangle()
          property real north: box.topLeft.latitude
          property geoCircle none
          property string noneText: none.toString()
          property var noneIs: [none.isValid, none.isEmpty, none.radius]
        }
        """
      Then the property "parts" of "root" is [2, 200000, 0, true, false]
      And the property "text" of "root" is "QGeoCircle({0, 0}, 200000)"
      And the property "inside" of "root" is [true, false]
      And the property "north" of "root" is within 0.0001 of 1.79864
      And the property "noneText" of "root" is "QGeoCircle({nan, nan}, -1)"
      And the property "noneIs" of "root" is [false, true, -1]

  Rule: A path is coordinates one after another

    Scenario: A path and its length
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var p: QtPositioning.path([QtPositioning.coordinate(0, 0), QtPositioning.coordinate(0, 1),
            QtPositioning.coordinate(1, 1)], 5)
          property var parts: [p.type, p.width, p.size(), p.isValid, p.isEmpty]
          property real round: p.length()
          property real first: p.length(0, 1)
          property real second: p.coordinateAt(1).longitude
          property bool past: p.coordinateAt(3).isValid
          property real centre: p.center.latitude
          property string text: QtPositioning.path([QtPositioning.coordinate(1, 2)]).toString()
          property geoPath none
          property var noneIs: [none.isValid, none.isEmpty, none.toString()]
        }
        """
      Then the property "parts" of "root" is [3, 5, 3, true, false]
      And the property "round" of "root" is within 0.001 of 379639.6636
      And the property "first" of "root" is within 0.001 of 111195.0523
      And the property "second" of "root" is 1
      And the property "past" of "root" is false
      And the property "centre" of "root" is 0.5
      And the property "text" of "root" is "QGeoPath([ 1° 0' 0.0\" N, 2° 0' 0.0\" E, ])"
      And the property "noneIs" of "root" is [false, true, "QGeoPath([  ])"]

  Rule: A polygon is the area a path goes round

    Scenario: A polygon
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property var p: QtPositioning.polygon([QtPositioning.coordinate(0, 0), QtPositioning.coordinate(0, 10),
            QtPositioning.coordinate(10, 10), QtPositioning.coordinate(10, 0)])
          property var parts: [p.type, p.size(), p.isValid, p.isEmpty]
          property var inside: [p.contains(QtPositioning.coordinate(5, 5)), p.contains(QtPositioning.coordinate(15, 5))]
          property real centre: p.center.longitude
          property string box: p.boundingGeoRectangle().toString()
        }
        """
      Then the property "parts" of "root" is [4, 4, true, false]
      And the property "inside" of "root" is [true, false]
      And the property "centre" of "root" is 5
      And the property "box" of "root" is "QGeoRectangle({10, 0}, {0, 10})"

  Rule: A shape holds any of them

    Scenario: A shape and the types it is turned back into
      Given the QML:
        """qml
        import QtQuick
        import QtPositioning
        Item {
          objectName: "root"
          property geoShape none
          property var noneIs: [none.type, none.isValid, none.isEmpty, none.toString()]
          property var rect: QtPositioning.rectangle(QtPositioning.coordinate(10, 0), QtPositioning.coordinate(0, 10))
          property geoShape held: rect
          property var heldIs: [held.type, held.isValid, held.toString(), held.contains(QtPositioning.coordinate(5, 5))]
          property string back: QtPositioning.shapeToRectangle(held).toString()
          property var wrong: QtPositioning.shapeToCircle(held)
          property var wrongIs: [wrong.type, wrong.isValid]
          property geoShape round: QtPositioning.circle(QtPositioning.coordinate(1, 1), 10)
          property real radius: QtPositioning.shapeToCircle(round).radius
          property var empty: QtPositioning.shape()
          property var emptyIs: [empty.type, empty.isValid]
        }
        """
      Then the property "noneIs" of "root" is [0, false, true, "QGeoShape(0)"]
      And the property "heldIs" of "root" is [1, true, "QGeoShape(1)", true]
      And the property "back" of "root" is "QGeoRectangle({10, 0}, {0, 10})"
      And the property "wrongIs" of "root" is [2, false]
      And the property "radius" of "root" is 10
      And the property "emptyIs" of "root" is [0, false]
