# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-path.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathline.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathmove.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathquad.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathcubic.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Path and its straight and curved segments
  A Path starts at a point and is drawn by its elements, each from where the
  one before it ended. A Shape's bounding rectangle shows where a path with
  no stroke goes.

  @qt-parity
  Rule: The path

    Scenario: A new Path starts at the origin, holds nothing and is not closed
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int held: path.pathElements.length
          property bool closed: path.closed
          property real x0: path.startX
          property real y0: path.startY
          Path { id: path }
        }
        """
      Then the property "held" of "root" is 0
      And the property "closed" of "root" is false
      And the property "x0" of "root" is 0
      And the property "y0" of "root" is 0

    Scenario: The elements of a path are its default property
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int held: path.pathElements.length
          Path {
            id: path
            PathLine { x: 10; y: 10 }
            PathMove { x: 20; y: 20 }
            PathQuad { x: 30; y: 30 }
          }
        }
        """
      Then the property "held" of "root" is 3

    Scenario Outline: A path is closed when it ends where it started
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            ShapePath {
              objectName: "path"
              startX: 10; startY: 10
              <elements>
            }
          }
        }
        """
      Then the property "closed" of "path" is <closed>

      Examples:
        | elements                                                                    | closed |
        | PathLine { x: 50; y: 10 } PathLine { x: 50; y: 50 } PathLine { x: 10; y: 10 } | true   |
        | PathLine { x: 50; y: 10 } PathLine { x: 50; y: 50 }                           | false  |
        | PathLine { x: 50; y: 10 } PathLine { relativeX: -40; relativeY: 0 }           | true   |
        | PathMove { x: 50; y: 50 }                                                     | false  |
        | PathSvg { path: "M 10 10 L 50 10 L 50 50 Z" }                                 | true   |
        | PathSvg { path: "M 20 10 L 50 10 L 50 50 Z" }                                 | false  |

    Scenario Outline: A point part of the way along a path
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real part: 0
          property point at: path.pointAtPercent(part)
          Path {
            id: path
            PathLine { x: 100; y: 0 }
            PathLine { x: 100; y: 100 }
          }
        }
        """
      When the property "part" of "root" is set to <part>
      Then the property "at.x" of "root" is within 0.01 of <x>
      And the property "at.y" of "root" is within 0.01 of <y>

      # The part is of the path's length, and is held between 0 and 1.
      Examples:
        | part | x   | y   |
        | 0.25 | 50  | 0   |
        | 0.5  | 100 | 0   |
        | 0.75 | 100 | 50  |
        | 1    | 100 | 100 |
        | 2    | 100 | 100 |
        | -1   | 0   | 0   |

  @qt-parity
  Rule: Lines and moves

    Scenario Outline: Where lines and moves take a path
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              startX: 10; startY: 10
              <elements>
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is <x>
      And the property "boundingRect.y" of "shape" is <y>
      And the property "boundingRect.width" of "shape" is <w>
      And the property "boundingRect.height" of "shape" is <h>

      # A coordinate that is relative counts from where the element starts,
      # and wins over the absolute one of the same axis. A move that nothing
      # follows still counts, and a move straight after the start replaces it.
      Examples:
        | elements                                                                    | x  | y  | w  | h  |
        | PathLine { x: 40; y: 30 }                                                   | 10 | 10 | 30 | 20 |
        | PathLine { relativeX: 30; relativeY: 20 }                                   | 10 | 10 | 30 | 20 |
        | PathLine { x: 500; relativeX: 30; y: 40 }                                   | 10 | 10 | 30 | 30 |
        | PathLine { relativeX: 30 } PathLine { relativeY: 20 }                       | 0  | 0  | 40 | 20 |
        | PathLine { relativeX: 30; relativeY: 0 } PathLine { relativeX: 0; relativeY: 20 } | 10 | 10 | 30 | 20 |
        | PathLine { x: 20; y: 20 } PathMove { x: 50; y: 50 } PathLine { x: 60; y: 70 } | 10 | 10 | 50 | 60 |
        | PathMove { relativeX: 10; relativeY: 10 } PathLine { relativeX: 10; relativeY: 0 } | 20 | 20 | 10 | 0  |
        | PathLine { x: 40; y: 30 } PathMove { x: 100; y: 100 }                       | 10 | 10 | 90 | 90 |
        | PathMove { x: 50; y: 50 }                                                   | 50 | 50 | 0  | 0  |

    Scenario: A move starts a part of the path that is filled on its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              startX: 10; startY: 10
              PathLine { x: 40; y: 10 }
              PathLine { x: 40; y: 40 }
              PathLine { x: 10; y: 40 }
              PathMove { x: 60; y: 60 }
              PathLine { x: 90; y: 60 }
              PathLine { x: 90; y: 90 }
              PathLine { x: 60; y: 90 }
            }
          }
        }
        """
      Then the pixel at 25, 25 of the window is "#000000"
      And the pixel at 75, 75 of the window is "#000000"
      And the pixel at 50, 50 of the window is "#ffffff"
      And the pixel at 45, 30 of the window is "#ffffff"

  @qt-parity
  Rule: Curves with control points

    Scenario: New curve elements have no coordinates
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real a: quad.controlX + quad.controlY + quad.x + quad.y
          property real b: quad.relativeControlX + quad.relativeX
          property real c: cubic.control1X + cubic.control1Y + cubic.control2X
          property real d: cubic.control2Y + cubic.relativeControl1X
          Path {
            PathQuad { id: quad }
            PathCubic { id: cubic }
          }
        }
        """
      Then the property "a" of "root" is 0
      And the property "b" of "root" is 0
      And the property "c" of "root" is 0
      And the property "d" of "root" is 0

    Scenario Outline: Where a curve goes
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
              <curve>
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.001 of <x>
      And the property "boundingRect.y" of "shape" is within 0.001 of <y>
      And the property "boundingRect.width" of "shape" is within 0.001 of <w>
      And the property "boundingRect.height" of "shape" is within 0.001 of <h>

      # A relative control point counts from where the curve starts.
      Examples:
        | curve                                                                                              | x  | y  | w  | h  |
        | PathQuad { x: 90; y: 50; controlX: 50; controlY: 110 }                                             | 10 | 50 | 80 | 30 |
        | PathQuad { relativeX: 80; relativeY: 0; relativeControlX: 40; relativeControlY: 60 }               | 10 | 50 | 80 | 30 |
        | PathQuad { x: 90; y: 50; controlX: 500; relativeControlX: 40; controlY: 110 }                      | 10 | 50 | 80 | 30 |
        | PathCubic { x: 90; y: 50; control1X: 10; control1Y: 10; control2X: 90; control2Y: 10 }             | 10 | 20 | 80 | 30 |
        | PathCubic { relativeX: 80; relativeY: 0; relativeControl1X: 0; relativeControl1Y: -40; relativeControl2X: 80; relativeControl2Y: -40 } | 10 | 20 | 80 | 30 |
        | PathCubic { x: 90; y: 50; control1X: 30; control1Y: 10; control2X: 70; control2Y: 90 }             | 10 | 38.453 | 80 | 23.094 |

    Scenario: A curve is painted where it goes
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              startX: 10; startY: 10
              PathQuad { x: 90; y: 10; controlX: 50; controlY: 150 }
            }
          }
        }
        """
      Then the pixel at 50, 40 of the window is "#000000"
      And the pixel at 50, 75 of the window is "#000000"
      And the pixel at 50, 85 of the window is "#ffffff"
      And the pixel at 15, 60 of the window is "#ffffff"
