# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-patharc.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathanglearc.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Arcs in a path
  A PathArc goes to a point along part of an ellipse. A PathAngleArc is part
  of an ellipse given by its centre and two angles.

  @qt-parity
  Rule: PathArc

    Scenario: A new PathArc is a straight line going clockwise
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real radii: arc.radiusX + arc.radiusY
          property bool large: arc.useLargeArc
          property int direction: arc.direction
          property real turned: arc.xAxisRotation
          property int clockwise: PathArc.Clockwise
          property int counter: PathArc.Counterclockwise
          Path { PathArc { id: arc } }
        }
        """
      Then the property "radii" of "root" is 0
      And the property "large" of "root" is false
      And the property "direction" of "root" is 0
      And the property "turned" of "root" is 0
      And the property "clockwise" of "root" is 0
      And the property "counter" of "root" is 1

    Scenario Outline: Where an arc from 10, 50 to 90, 50 goes
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              startX: 10; startY: 50
              PathArc { <arc> }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.01 of <x>
      And the property "boundingRect.y" of "shape" is within 0.01 of <y>
      And the property "boundingRect.width" of "shape" is within 0.01 of <w>
      And the property "boundingRect.height" of "shape" is within 0.01 of <h>

      # Clockwise is over the top on a screen. Radii too small to reach are
      # grown until they do, a radius of nothing gives a straight line, and
      # the rotation is in degrees.
      Examples:
        | arc                                                                             | x  | y   | w   | h  |
        | x: 90; y: 50; radiusX: 40; radiusY: 40                                          | 10 | 10  | 80  | 40 |
        | x: 90; y: 50; radiusX: 40; radiusY: 40; direction: PathArc.Counterclockwise     | 10 | 50  | 80  | 40 |
        | x: 90; y: 50; radiusX: 50; radiusY: 50                                          | 10 | 30  | 80  | 20 |
        | x: 90; y: 50; radiusX: 50; radiusY: 50; useLargeArc: true                       | 0  | -30 | 100 | 80 |
        | x: 90; y: 50; radiusX: 50; radiusY: 50; useLargeArc: true; direction: PathArc.Counterclockwise | 0 | 50 | 100 | 80 |
        | x: 90; y: 50; radiusX: 10; radiusY: 10                                          | 10 | 10  | 80  | 40 |
        | x: 90; y: 50                                                                    | 10 | 50  | 80  | 0  |
        | x: 90; y: 50; radiusX: 40                                                       | 10 | 50  | 80  | 0  |
        | x: 90; y: 50; radiusX: 40; radiusY: 20                                          | 10 | 30  | 80  | 20 |
        | x: 90; y: 50; radiusX: 20; radiusY: 40; xAxisRotation: 90                       | 10 | 30  | 80  | 20 |
        | x: 90; y: 50; radiusX: 60; radiusY: 30; xAxisRotation: 45                       | 0  | 0   | 90  | 50 |
        | relativeX: 80; relativeY: 0; radiusX: 40; radiusY: 40                           | 10 | 10  | 80  | 40 |
        | x: 90; y: 50; radiusX: -40; radiusY: -40                                        | 10 | 10  | 80  | 40 |
        | x: 10; y: 50; radiusX: 40; radiusY: 40                                          | 10 | 50  | 0   | 0  |

    Scenario: An arc is painted round
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              startX: 10; startY: 50
              PathArc { x: 90; y: 50; radiusX: 40; radiusY: 40 }
            }
          }
        }
        """
      Then the pixel at 50, 12 of the window is "#000000"
      And the pixel at 50, 48 of the window is "#000000"
      And the pixel at 23, 23 of the window is "#000000"
      And the pixel at 19, 19 of the window is "#ffffff"
      And the pixel at 50, 52 of the window is "#ffffff"

  @qt-parity
  Rule: PathAngleArc

    Scenario: A new PathAngleArc has no size and moves to its start
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real centre: arc.centerX + arc.centerY
          property real radii: arc.radiusX + arc.radiusY
          property real angles: arc.startAngle + arc.sweepAngle
          property bool moves: arc.moveToStart
          Path { PathAngleArc { id: arc } }
        }
        """
      Then the property "centre" of "root" is 0
      And the property "radii" of "root" is 0
      And the property "angles" of "root" is 0
      And the property "moves" of "root" is true

    Scenario Outline: Where an arc round 50, 50 with a radius of 40 goes
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathAngleArc {
                centerX: 50; centerY: 50; radiusX: 40; radiusY: <ry>
                <angles>
              }
              <then>
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.03 of <x>
      And the property "boundingRect.y" of "shape" is within 0.03 of <y>
      And the property "boundingRect.width" of "shape" is within 0.03 of <w>
      And the property "boundingRect.height" of "shape" is within 0.03 of <h>

      # Angles are in degrees from three o'clock and grow clockwise on a
      # screen. Unless told otherwise the arc starts a part of its own, and
      # the path goes on from the arc's end.
      #
      # The numbers are where the circle is. Qt cuts its curves for a quarter
      # at the angle asked for, which puts the end of the arc from 45 to 75
      # degrees 0.02 off the circle (at 60.372, 88.642): hence the tolerance.
      Examples:
        | ry | angles                                         | then                                    | x      | y      | w      | h      |
        | 40 | startAngle: 0; sweepAngle: 90                  |                                         | 50     | 50     | 40     | 40     |
        | 40 | startAngle: 0; sweepAngle: -90                 |                                         | 50     | 10     | 40     | 40     |
        | 40 | startAngle: 180; sweepAngle: 90                |                                         | 10     | 10     | 40     | 40     |
        | 40 | startAngle: 45; sweepAngle: 30                 |                                         | 60.353 | 78.284 | 17.932 | 10.353 |
        | 20 | sweepAngle: 360                                |                                         | 10     | 30     | 80     | 40     |
        | 40 | sweepAngle: 720                                |                                         | 10     | 10     | 80     | 80     |
        | 40 | startAngle: 90; sweepAngle: 0                  |                                         | 50     | 90     | 0      | 0      |
        | 40 | startAngle: 0; sweepAngle: 90; moveToStart: false |                                      | 0      | 0      | 90     | 90     |
        | 40 | startAngle: 0; sweepAngle: 90                  | PathLine { relativeX: 0; relativeY: 100 } | 50   | 50     | 40     | 140    |

    Scenario: A whole turn ends where it starts
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              objectName: "path"
              fillColor: "black"; strokeWidth: -1
              startX: 90; startY: 50
              PathAngleArc {
                centerX: 50; centerY: 50; radiusX: 40; radiusY: 40
                sweepAngle: 360
              }
            }
          }
        }
        """
      Then the property "closed" of "path" is true
      And the pixel at 50, 50 of the window is "#000000"
      And the pixel at 23, 23 of the window is "#000000"
      And the pixel at 77, 77 of the window is "#000000"
      And the pixel at 19, 19 of the window is "#ffffff"
