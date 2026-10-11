# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shape.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shapepath.html
#   https://doc.qt.io/qt-6/qtquick-shapes-qmlmodule.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Shape
  A Shape is an item that paints the paths its ShapePath children describe.

  @qt-parity
  Rule: The type and its properties

    Scenario: A new Shape has no paths and no size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property int paths: shape.data.length
          Shape { id: shape; objectName: "shape" }
        }
        """
      Then the property "paths" of "root" is 0
      And the property "width" of "shape" is 0
      And the property "height" of "shape" is 0
      And the property "boundingRect.width" of "shape" is 0
      And the property "asynchronous" of "shape" is false
      And the property "containsMode" of "shape" is 0
      And the property "preferredRendererType" of "shape" is 0
      And the property "antialiasing" of "shape" is false

    Scenario Outline: The keys of Shape's enumerations are numbers
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property int value: Shape.<key>
        }
        """
      Then the property "value" of "root" is <value>

      Examples:
        | key                  | value |
        | Null                 | 0     |
        | Ready                | 1     |
        | Processing           | 2     |
        | BoundingRectContains | 0     |
        | FillContains         | 1     |
        | UnknownRenderer      | 0     |
        | GeometryRenderer     | 1     |
        | NvprRenderer         | 2     |
        | SoftwareRenderer     | 3     |
        | CurveRenderer        | 4     |

    Scenario: A Shape is ready once it has been painted
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          width: 100; height: 100
          Shape {
            objectName: "shape"
            ShapePath { PathLine { x: 50; y: 50 } }
          }
        }
        """
      When 100 milliseconds pass
      Then the property "status" of "shape" is 1

    Scenario: A renderer may be asked for and paths may be asked to load in the background
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            objectName: "shape"
            preferredRendererType: Shape.CurveRenderer
            asynchronous: true
            ShapePath {
              strokeWidth: -1; fillColor: "red"
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the property "preferredRendererType" of "shape" is 4
      And the property "asynchronous" of "shape" is true
      And the pixel at 50, 50 of the window is "#ff0000"

  @qt-parity
  Rule: The bounding rectangle and the implicit size

    Scenario Outline: The bounding rectangle is the path's, grown by half the stroke
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: <width>; strokeColor: "<color>"
              capStyle: ShapePath.<cap>
              startX: 10; startY: 20
              PathLine { x: 110; y: 70 }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.001 of <x>
      And the property "boundingRect.y" of "shape" is within 0.001 of <y>
      And the property "boundingRect.width" of "shape" is within 0.001 of <w>
      And the property "boundingRect.height" of "shape" is within 0.001 of <h>

      # A square cap reaches the stroke's width times the square root of a
      # half beyond the path. A stroke that paints nothing adds nothing, and
      # a negative width takes the same margin off.
      Examples:
        | width | color       | cap       | x      | y      | w       | h      |
        | 4     | black       | FlatCap   | 8      | 18     | 104     | 54     |
        | 4     | black       | RoundCap  | 8      | 18     | 104     | 54     |
        | 4     | black       | SquareCap | 7.172  | 17.172 | 105.657 | 55.657 |
        | 4     | transparent | SquareCap | 10     | 20     | 100     | 50     |
        | 0     | black       | SquareCap | 10     | 20     | 100     | 50     |
        | -1    | black       | FlatCap   | 10.5   | 20.5   | 99      | 49     |

    Scenario: A curve is bounded by where it goes, not by its control point
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
              PathQuad { x: 110; y: 10; controlX: 60; controlY: 110 }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is 10
      And the property "boundingRect.y" of "shape" is 10
      And the property "boundingRect.width" of "shape" is 100
      And the property "boundingRect.height" of "shape" is 50

    Scenario: The bounding rectangle holds every path
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0; startX: 10; startY: 10
              PathLine { x: 20; y: 20 }
            }
            ShapePath {
              strokeWidth: 0; startX: 50; startY: 60
              PathLine { x: 70; y: 80 }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is 10
      And the property "boundingRect.y" of "shape" is 10
      And the property "boundingRect.width" of "shape" is 60
      And the property "boundingRect.height" of "shape" is 70

    Scenario: A Shape is as large as the far corner of its bounding rectangle
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0; startX: 10; startY: 10
              PathLine { x: 120; y: 40 }
            }
          }
        }
        """
      Then the property "implicitWidth" of "shape" is 120
      And the property "implicitHeight" of "shape" is 40
      And the property "width" of "shape" is 120
      And the property "height" of "shape" is 40

    Scenario: A Shape with a size of its own keeps it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            width: 30; height: 40
            ShapePath {
              strokeWidth: 0; startX: 10; startY: 10
              PathLine { x: 120; y: 20 }
            }
          }
        }
        """
      Then the property "width" of "shape" is 30
      And the property "height" of "shape" is 40
      And the property "implicitWidth" of "shape" is 120
      And the property "boundingRect.width" of "shape" is 110

    Scenario: The bounding rectangle follows the path
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property alias endX: line.x
          property alias stroke: path.strokeWidth
          Shape {
            objectName: "shape"
            ShapePath {
              id: path
              strokeWidth: 0; capStyle: ShapePath.FlatCap
              PathLine { id: line; x: 50; y: 40 }
            }
          }
        }
        """
      When the property "endX" of "root" is set to 80
      Then the property "boundingRect.width" of "shape" is 80
      And the property "width" of "shape" is 80
      When the property "stroke" of "root" is set to 6
      Then the property "boundingRect.x" of "shape" is -3
      And the property "boundingRect.width" of "shape" is 86
      And the property "width" of "shape" is 83

  @qt-parity
  Rule: What a Shape holds and how it is painted

    Scenario: Paths and items may both be children of a Shape
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          objectName: "root"
          width: 100; height: 100
          property int held: shape.data.length
          property int items: shape.children.length
          Shape {
            id: shape
            ShapePath {
              strokeWidth: -1; fillColor: "red"
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
            Rectangle { x: 60; width: 20; height: 20; color: "blue" }
          }
        }
        """
      Then the property "held" of "root" is 2
      And the property "items" of "root" is 1
      And the pixel at 30, 10 of the window is "#ff0000"
      And the pixel at 70, 10 of the window is "#0000ff"

    Scenario: A later path is painted over an earlier one
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1; fillColor: "red"
              PathLine { x: 60; y: 0 }
              PathLine { x: 60; y: 100 }
              PathLine { x: 0; y: 100 }
            }
            ShapePath {
              strokeWidth: -1; fillColor: "blue"; startX: 40
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 40; y: 100 }
            }
          }
        }
        """
      Then the pixel at 20, 50 of the window is "#ff0000"
      And the pixel at 50, 50 of the window is "#0000ff"
      And the pixel at 80, 50 of the window is "#0000ff"

    Scenario: A path is painted beyond the Shape's own size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            x: 20; y: 20; width: 10; height: 10
            ShapePath {
              strokeWidth: -1; fillColor: "red"; startX: -10; startY: -10
              PathLine { x: 60; y: -10 }
              PathLine { x: 60; y: 60 }
              PathLine { x: -10; y: 60 }
            }
          }
        }
        """
      Then the pixel at 12, 12 of the window is "#ff0000"
      And the pixel at 75, 75 of the window is "#ff0000"
      And the pixel at 85, 85 of the window is "#ffffff"
      And the pixel at 5, 5 of the window is "#ffffff"

    Scenario: The edge of a path is smoothed whether or not antialiasing is asked for
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1; fillColor: "black"; startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
              PathLine { x: 50; y: 10 }
            }
          }
        }
        """
      Then the pixel at 32, 44 of the window is within 4 of "#bfbfbf"
      And the pixel at 32, 45 of the window is within 4 of "#3f3f3f"
      And the pixel at 33, 44 of the window is "#000000"
      And the pixel at 31, 44 of the window is "#ffffff"

    Scenario: A Shape's opacity is the opacity of what it paints
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            opacity: 0.5
            ShapePath {
              strokeWidth: -1; fillColor: "black"
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is within 2 of "#808080"

    Scenario: A Shape that is not visible paints nothing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            visible: false
            ShapePath {
              strokeWidth: -1; fillColor: "black"
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is "#ffffff"
