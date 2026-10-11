# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathcurve.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathpolyline.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathmultiline.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathrectangle.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Curves through points, polylines and rectangles in a path
  A PathCurve goes smoothly through its points, a PathPolyline and a
  PathMultiline are lines through lists of points, and a PathRectangle is a
  rectangle whose corners may be round or cut off.

  @qt-parity
  Rule: PathCurve

    Scenario Outline: Where curves through points go
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              objectName: "path"
              strokeWidth: 0
              startX: <x0>; startY: <y0>
              <curves>
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.001 of <x>
      And the property "boundingRect.y" of "shape" is within 0.001 of <y>
      And the property "boundingRect.width" of "shape" is within 0.001 of <w>
      And the property "boundingRect.height" of "shape" is within 0.001 of <h>
      And the property "closed" of "path" is <closed>

      # One curve is a straight line. A curve bends by the points of the
      # curves beside it, so it swings past its own points; a line beside it
      # does not bend it. Curves that end where the path starts go round
      # smoothly.
      Examples:
        | x0 | y0 | curves                                                                                                     | x | y       | w   | h        | closed |
        | 0  | 0  | PathCurve { x: 100; y: 0 }                                                                                 | 0 | 0       | 100 | 0        | false  |
        | 0  | 50 | PathCurve { x: 50; y: 0 } PathCurve { x: 100; y: 50 }                                                      | 0 | 0       | 100 | 50       | false  |
        | 0  | 50 | PathCurve { x: 40; y: 0 } PathCurve { x: 80; y: 100 } PathCurve { x: 120; y: 50 }                          | 0 | -0.9367 | 120 | 101.8733 | false  |
        | 0  | 50 | PathCurve { relativeX: 40; relativeY: -50 } PathCurve { relativeX: 40; relativeY: 100 } PathCurve { relativeX: 40; relativeY: -50 } | 0 | -0.9367 | 120 | 101.8733 | false |
        | 0  | 50 | PathLine { x: 40; y: 0 } PathCurve { x: 80; y: 100 } PathCurve { x: 120; y: 50 }                           | 0 | 0       | 120 | 100.9367 | false  |
        | 50 | 0  | PathCurve { x: 100; y: 50 } PathCurve { x: 50; y: 100 } PathCurve { x: 0; y: 50 } PathCurve { x: 50; y: 0 } | 0 | 0       | 100 | 100      | true   |

    Scenario: Curves through four points round a centre fill what is nearly a circle
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              startX: 50; startY: 10
              PathCurve { x: 90; y: 50 }
              PathCurve { x: 50; y: 90 }
              PathCurve { x: 10; y: 50 }
              PathCurve { x: 50; y: 10 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is "#000000"
      And the pixel at 74, 26 of the window is "#000000"
      And the pixel at 26, 74 of the window is "#000000"
      And the pixel at 82, 18 of the window is "#ffffff"

  @qt-parity
  Rule: PathPolyline and PathMultiline

    Scenario: A polyline starts at its first point
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property point first: line.start
          property int points: line.path.length
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0; startX: 3; startY: 4
              PathPolyline {
                id: line
                path: [Qt.point(10, 10), Qt.point(40, 20), Qt.point(20, 60)]
              }
            }
          }
        }
        """
      Then the property "first.x" of "root" is 10
      And the property "first.y" of "root" is 10
      And the property "points" of "root" is 3
      And the property "boundingRect.x" of "shape" is 10
      And the property "boundingRect.y" of "shape" is 10
      And the property "boundingRect.width" of "shape" is 30
      And the property "boundingRect.height" of "shape" is 50

    Scenario: A path goes on from a polyline's last point
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathPolyline {
                path: [Qt.point(10, 10), Qt.point(40, 20), Qt.point(20, 60)]
              }
              PathLine { relativeX: 100; relativeY: 0 }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is 10
      And the property "boundingRect.width" of "shape" is 110
      And the property "boundingRect.height" of "shape" is 50

    Scenario Outline: A polyline of fewer than two points draws nothing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0; startX: 3; startY: 4
              PathPolyline { path: <points> }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is 3
      And the property "boundingRect.y" of "shape" is 4
      And the property "boundingRect.width" of "shape" is 0
      And the property "boundingRect.height" of "shape" is 0

      Examples:
        | points              |
        | []                  |
        | [Qt.point(10, 10)]  |

    Scenario: A polyline is filled like any other part of a path
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathPolyline {
                path: [Qt.point(50, 10), Qt.point(90, 90), Qt.point(10, 90)]
              }
            }
          }
        }
        """
      Then the pixel at 50, 60 of the window is "#000000"
      And the pixel at 20, 30 of the window is "#ffffff"
      And the pixel at 5, 5 of the window is "#ffffff"

    Scenario: A multiline is a list of polylines, each a part of its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          objectName: "root"
          width: 100; height: 100
          property point first: lines.start
          Shape {
            objectName: "shape"
            ShapePath {
              fillColor: "black"; strokeWidth: 0; startX: 3; startY: 4
              PathMultiline {
                id: lines
                paths: [
                  [Qt.point(10, 10), Qt.point(40, 10), Qt.point(40, 40)],
                  [Qt.point(60, 60), Qt.point(90, 60), Qt.point(90, 95)]
                ]
              }
            }
          }
        }
        """
      Then the property "first.x" of "root" is 10
      And the property "boundingRect.x" of "shape" is 10
      And the property "boundingRect.y" of "shape" is 10
      And the property "boundingRect.width" of "shape" is 80
      And the property "boundingRect.height" of "shape" is 85
      And the pixel at 35, 20 of the window is "#000000"
      And the pixel at 85, 70 of the window is "#000000"
      And the pixel at 50, 50 of the window is "#ffffff"

    Scenario: A path goes on from a multiline's last point
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathMultiline {
                paths: [
                  [Qt.point(10, 10), Qt.point(40, 20)],
                  [Qt.point(50, 60), Qt.point(70, 95)]
                ]
              }
              PathLine { relativeX: 100; relativeY: 0 }
            }
          }
        }
        """
      Then the property "boundingRect.width" of "shape" is 160
      And the property "boundingRect.height" of "shape" is 85

  Rule: PathRectangle

    @qt-parity
    Scenario: A new PathRectangle has no size and square corners
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real size: box.width + box.height + box.strokeAdjustment
          property real radii: box.radius + box.topLeftRadius + box.bottomRightRadius
          property bool cut: box.bevel || box.topLeftBevel || box.bottomRightBevel
          Path { PathRectangle { id: box } }
        }
        """
      Then the property "size" of "root" is 0
      And the property "radii" of "root" is 0
      And the property "cut" of "root" is false

    @qt-parity
    Scenario Outline: Where a rectangle in a path is
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0; startX: 5; startY: 5
              <elements>
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is <x>
      And the property "boundingRect.y" of "shape" is <y>
      And the property "boundingRect.width" of "shape" is <w>
      And the property "boundingRect.height" of "shape" is <h>

      # The stroke adjustment takes half of itself off every side, so that a
      # stroke of that width stays inside the rectangle. The path goes on
      # from the start of the rectangle's top edge.
      Examples:
        | elements                                                                              | x  | y  | w   | h   |
        | PathRectangle { x: 10; y: 20; width: 50; height: 30 }                                 | 10 | 20 | 50  | 30  |
        | PathRectangle { relativeX: 10; relativeY: 20; width: 50; height: 30 }                 | 15 | 25 | 50  | 30  |
        | PathRectangle { x: 10; y: 20; width: 50; height: 30; strokeAdjustment: 4 }            | 12 | 22 | 46  | 26  |
        | PathRectangle { x: 10; y: 20; width: 50; height: 30; radius: 100 }                    | 10 | 20 | 50  | 30  |
        | PathRectangle { x: 10; y: 20; width: 50; height: 30 } PathLine { relativeX: 100; relativeY: 100 } | 10 | 20 | 100 | 100 |
        | PathRectangle { x: 10; y: 20; width: 50; height: 30; radius: 10 } PathLine { relativeX: 100; relativeY: 100 } | 10 | 20 | 110 | 100 |
        | PathRectangle { x: 10; y: 20; width: 10; height: 10 } PathRectangle { relativeX: 50; relativeY: 50; width: 10; height: 10 } | 10 | 20 | 60 | 60 |

    @qt-parity
    Scenario: A corner follows the radius unless it has one of its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          objectName: "root"
          width: 100; height: 100
          property real followed: box.topLeftRadius
          property real own: box.topRightRadius
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathRectangle {
                id: box
                x: 10; y: 10; width: 80; height: 60
                radius: 20; topRightRadius: 0
              }
            }
          }
        }
        """
      Then the property "followed" of "root" is 20
      And the property "own" of "root" is 0
      And the pixel at 13, 13 of the window is "#ffffff"
      And the pixel at 17, 17 of the window is "#000000"
      And the pixel at 88, 11 of the window is "#000000"
      And the pixel at 86, 66 of the window is "#ffffff"
      And the pixel at 83, 63 of the window is "#000000"
      And the pixel at 50, 40 of the window is "#000000"

    @qt-parity
    Scenario: A radius is no more than half the shorter side
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathRectangle { x: 10; y: 10; width: 80; height: 40; radius: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 11 of the window is "#000000"
      And the pixel at 50, 48 of the window is "#000000"
      And the pixel at 11, 30 of the window is "#000000"
      And the pixel at 88, 30 of the window is "#000000"
      And the pixel at 14, 14 of the window is "#ffffff"
      And the pixel at 18, 18 of the window is "#000000"

    @qt-parity
    Scenario: A bevelled corner is cut off straight by its radius
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathRectangle {
                x: 10; y: 10; width: 80; height: 60
                radius: 20; bevel: true; bottomLeftRadius: 30
              }
            }
          }
        }
        """
      Then the pixel at 18, 18 of the window is "#ffffff"
      And the pixel at 20, 21 of the window is "#000000"
      And the pixel at 19, 20 of the window is within 4 of "#808080"
      And the pixel at 23, 55 of the window is "#ffffff"
      And the pixel at 26, 55 of the window is "#000000"
      And the pixel at 81, 61 of the window is "#ffffff"
      And the pixel at 78, 59 of the window is "#000000"

    @qt-parity
    Scenario: One corner may be bevelled alone
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathRectangle {
                x: 10; y: 10; width: 80; height: 60
                radius: 20; topLeftBevel: true
              }
            }
          }
        }
        """
      Then the pixel at 19, 20 of the window is within 4 of "#808080"
      And the pixel at 16, 16 of the window is "#ffffff"
      And the pixel at 83, 17 of the window is "#000000"
      And the pixel at 86, 13 of the window is "#ffffff"

    # Qt 6.11.2 leaves a corner bevelled when its own bevel is written as
    # false under a rectangle that is bevelled; its documentation says such a
    # corner is rounded, which is what happens here.
    @qt-diverges
    Scenario: A corner may be left round in a bevelled rectangle
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathRectangle {
                x: 10; y: 10; width: 80; height: 60
                radius: 20; bevel: true; topRightBevel: false
              }
            }
          }
        }
        """
      Then the pixel at 83, 17 of the window is "#000000"
      And the pixel at 86, 13 of the window is "#ffffff"
      And the pixel at 18, 18 of the window is "#ffffff"

    @qt-parity
    Scenario: A rectangle with no width draws nothing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathRectangle { x: 10; y: 20; width: 0; height: 30 }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is 0
      And the property "boundingRect.width" of "shape" is 0
      And the property "boundingRect.height" of "shape" is 0
