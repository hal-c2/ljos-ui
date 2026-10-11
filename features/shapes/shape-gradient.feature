# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shapegradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-lineargradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-radialgradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-conicalgradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shapepath.html#fillGradient-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Gradients that fill a ShapePath
  A ShapePath's fillGradient is a LinearGradient, a RadialGradient or a
  ConicalGradient. Its coordinates are the Shape's, and it holds the same
  stops as a Rectangle's Gradient.

  @qt-parity
  Rule: The types and their properties

    Scenario: New gradients have no stops, no coordinates and a padded spread
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property int stops: line.stops.length + round.stops.length + cone.stops.length
          property real a: line.x1 + line.y1 + line.x2 + line.y2
          property real b: round.centerX + round.centerY + round.centerRadius
          property real c: round.focalX + round.focalY + round.focalRadius
          property real d: cone.centerX + cone.centerY + cone.angle
          property int spreads: line.spread + round.spread + cone.spread
          LinearGradient { id: line }
          RadialGradient { id: round }
          ConicalGradient { id: cone }
        }
        """
      Then the property "stops" of "root" is 0
      And the property "a" of "root" is 0
      And the property "b" of "root" is 0
      And the property "c" of "root" is 0
      And the property "d" of "root" is 0
      And the property "spreads" of "root" is 0

    Scenario Outline: The keys of the spread are numbers
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property int value: ShapeGradient.<key>
        }
        """
      Then the property "value" of "root" is <value>

      Examples:
        | key           | value |
        | PadSpread     | 0     |
        | ReflectSpread | 1     |
        | RepeatSpread  | 2     |

    Scenario: A ShapeGradient cannot be made in QML
      Given the file "Bad.qml":
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item { ShapeGradient { } }
        """
      Then loading "Bad.qml" is rejected

  @qt-parity
  Rule: LinearGradient

    Scenario: A linear gradient runs from its first point to its second
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1; fillColor: "green"
              fillGradient: LinearGradient {
                x1: 20; x2: 60
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 5, 50 of the window is "#ff0000"
      And the pixel at 20, 10 of the window is within 6 of "#fc0003"
      And the pixel at 30, 50 of the window is within 6 of "#bc0043"
      And the pixel at 40, 90 of the window is within 6 of "#7c0083"
      And the pixel at 50, 50 of the window is within 6 of "#3d00c2"
      And the pixel at 59, 50 of the window is within 6 of "#0300fc"
      And the pixel at 80, 50 of the window is "#0000ff"

    Scenario: A linear gradient may run in any direction
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: LinearGradient {
                x1: 0; y1: 0; x2: 100; y2: 100
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 0, 0 of the window is within 6 of "#fe0001"
      And the pixel at 25, 25 of the window is within 6 of "#be0041"
      And the pixel at 50, 50 of the window is within 6 of "#7e0081"
      And the pixel at 75, 25 of the window is within 6 of "#7e0081"
      And the pixel at 99, 99 of the window is within 6 of "#0100fe"

    Scenario Outline: The spread says what is painted beyond the ends of a linear gradient
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: LinearGradient {
                x1: 20; x2: 60; spread: ShapeGradient.<spread>
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 5, 50 of the window is within 6 of "<at5>"
      And the pixel at 40, 50 of the window is within 6 of "#7c0083"
      And the pixel at 70, 50 of the window is within 6 of "<at70>"
      And the pixel at 95, 50 of the window is within 6 of "<at95>"

      Examples:
        | spread        | at5     | at70    | at95    |
        | PadSpread     | #ff0000 | #0000ff | #0000ff |
        | RepeatSpread  | #5c00a3 | #bc0043 | #1d00e2 |
        | ReflectSpread | #a3005c | #4300bc | #e2001d |

    Scenario: A gradient's coordinates are not moved by the path's scale
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1; scale: Qt.size(2, 2)
              fillGradient: LinearGradient {
                x1: 20; x2: 60
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 50; y: 0 }
              PathLine { x: 50; y: 50 }
              PathLine { x: 0; y: 50 }
            }
          }
        }
        """
      Then the pixel at 30, 50 of the window is within 6 of "#bc0043"
      And the pixel at 50, 90 of the window is within 6 of "#3d00c2"
      And the pixel at 80, 50 of the window is "#0000ff"

    Scenario: A linear gradient whose points are one point is its first colour
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: LinearGradient {
                x1: 40; y1: 10; x2: 40; y2: 10
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 10, 50 of the window is "#ff0000"
      And the pixel at 70, 50 of the window is "#ff0000"

  @qt-parity
  Rule: The stops

    Scenario Outline: A gradient with <stops>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1; fillColor: "green"
              fillGradient: LinearGradient {
                x2: 100
                <written>
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 0, 50 of the window is within 6 of "<at0>"
      And the pixel at 25, 50 of the window is within 6 of "<at25>"
      And the pixel at 49, 50 of the window is within 6 of "<at49>"
      And the pixel at 75, 50 of the window is within 6 of "<at75>"
      And the pixel at 99, 50 of the window is within 6 of "<at99>"

      # Stops are used in the order of their positions, and of two at one
      # position the one written first. Colours are mixed with their
      # transparency already applied.
      Examples:
        | stops                         | written                                                                                    | at0     | at25    | at49    | at75    | at99    |
        | no stops is black to white    |                                                                                            | #010101 | #414141 | #7e7e7e | #c0c0c0 | #fefefe |
        | one stop is one colour        | GradientStop { position: 0.5; color: "lime" }                                              | #00ff00 | #00ff00 | #00ff00 | #00ff00 | #00ff00 |
        | stops out of order            | GradientStop { position: 1; color: "blue" } GradientStop { position: 0; color: "red" }     | #fe0001 | #be0041 | #810080 | #3f00c0 | #0100fe |
        | no stop at either end         | GradientStop { position: 0.25; color: "red" } GradientStop { position: 0.75; color: "blue" } | #ff0000 | #fc0003 | #82007d | #0300fc | #0000ff |
        | two stops at one position     | GradientStop { color: "red" } GradientStop { position: 0.5; color: "lime" } GradientStop { position: 0.5; color: "black" } GradientStop { position: 1; color: "blue" } | #fc0300 | #7d8200 | #03fc00 | #007e81 | #0003fc |
        | a stop that is partly clear   | GradientStop { position: 0; color: "transparent" } GradientStop { position: 1; color: "blue" } | #fefeff | #bebeff | #8181ff | #3f3fff | #0101ff |

    Scenario: A stop outside the gradient is refused
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: LinearGradient {
                x2: 100
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1.5; color: "lime" }
                GradientStop { position: -1; color: "black" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is within 6 of "#7e0081"
      And the pixel at 99, 50 of the window is within 6 of "#0100fe"
      And the log has a warning matching "Color position must be specified in the range 0 to 1"

    Scenario: What is painted follows the gradient and its stops
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          objectName: "root"
          width: 100; height: 100
          property alias first: stop.color
          property alias reach: fill.x2
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: LinearGradient {
                id: fill
                x2: 50
                GradientStop { id: stop; position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 60, 50 of the window is "#0000ff"
      When the property "reach" of "root" is set to 100
      Then the pixel at 60, 50 of the window is within 6 of "#650099"
      When the property "first" of "root" is set to "#00ff00"
      Then the pixel at 60, 50 of the window is within 6 of "#006599"

    Scenario: Without a gradient the fill colour is used again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              objectName: "path"
              strokeWidth: -1; fillColor: "#00ff00"
              fillGradient: LinearGradient {
                x2: 100
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 99, 50 of the window is within 6 of "#0100fe"
      When the property "fillGradient" of "path" is set to null
      Then the pixel at 99, 50 of the window is "#00ff00"

  @qt-parity
  Rule: RadialGradient

    Scenario: A radial gradient runs from its centre to its radius
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: RadialGradient {
                centerX: 50; centerY: 50; centerRadius: 40
                focalX: 50; focalY: 50
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is within 6 of "#fb0004"
      And the pixel at 60, 50 of the window is within 6 of "#bc0043"
      And the pixel at 70, 50 of the window is within 6 of "#7c0083"
      And the pixel at 50, 70 of the window is within 6 of "#7c0083"
      And the pixel at 80, 50 of the window is within 6 of "#3d00c2"
      And the pixel at 89, 50 of the window is within 6 of "#0300fc"
      And the pixel at 95, 50 of the window is "#0000ff"
      And the pixel at 5, 5 of the window is "#0000ff"

    Scenario Outline: The spread says what is painted beyond the radius
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: RadialGradient {
                centerX: 50; centerY: 50; centerRadius: 20
                focalX: 50; focalY: 50; spread: ShapeGradient.<spread>
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 60, 50 of the window is within 6 of "#790086"
      And the pixel at 71, 50 of the window is within 6 of "<at71>"
      And the pixel at 80, 50 of the window is within 6 of "<at80>"

      Examples:
        | spread        | at71    | at80    |
        | PadSpread     | #0000ff | #0000ff |
        | RepeatSpread  | #ec0013 | #790086 |
        | ReflectSpread | #1300ec | #860079 |

    Scenario: The focal radius is where a radial gradient starts
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: RadialGradient {
                centerX: 50; centerY: 50; centerRadius: 40
                focalX: 50; focalY: 50; focalRadius: 20
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is "#ff0000"
      And the pixel at 60, 50 of the window is "#ff0000"
      And the pixel at 71, 50 of the window is within 6 of "#ec0013"
      And the pixel at 80, 50 of the window is within 6 of "#790086"
      And the pixel at 95, 50 of the window is "#0000ff"

    Scenario: A focal point away from the centre pulls the first colour towards it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: RadialGradient {
                centerX: 50; centerY: 50; centerRadius: 40
                focalX: 30; focalY: 50
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 30, 50 of the window is within 6 of "#fc0003"
      And the pixel at 20, 50 of the window is within 6 of "#860079"
      And the pixel at 50, 50 of the window is within 6 of "#a80057"
      And the pixel at 70, 50 of the window is within 6 of "#5300ac"
      And the pixel at 50, 20 of the window is within 6 of "#4000bf"
      And the pixel at 92, 50 of the window is "#0000ff"

  @qt-parity
  Rule: ConicalGradient

    Scenario: A conical gradient turns anticlockwise from three o'clock
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: ConicalGradient {
                centerX: 50; centerY: 50
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 90, 49 of the window is within 6 of "#ff0000"
      And the pixel at 50, 10 of the window is within 6 of "#c0003f"
      And the pixel at 10, 50 of the window is within 6 of "#7f0080"
      And the pixel at 50, 90 of the window is within 6 of "#3f00c0"
      And the pixel at 90, 51 of the window is within 6 of "#0100fe"

    Scenario Outline: The angle turns a conical gradient's start anticlockwise
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: -1
              fillGradient: ConicalGradient {
                centerX: 50; centerY: 50; angle: <angle>
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at <x1>, <y1> of the window is within 6 of "#ff0000"
      And the pixel at <x2>, <y2> of the window is within 6 of "#0100fe"

      Examples:
        | angle | x1 | y1 | x2 | y2 |
        | 90    | 49 | 10 | 51 | 10 |
        | -45   | 80 | 79 | 79 | 80 |
        | 360   | 90 | 49 | 90 | 51 |

  @qt-parity
  Rule: A gradient and a stroke

    Scenario: The stroke keeps its colour over a gradient
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeWidth: 10; strokeColor: "#00ff00"
              fillGradient: LinearGradient {
                x1: 20; x2: 60
                GradientStop { position: 0; color: "red" }
                GradientStop { position: 1; color: "blue" }
              }
              startX: 10; startY: 10
              PathLine { x: 90; y: 10 }
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
              PathLine { x: 10; y: 10 }
            }
          }
        }
        """
      Then the pixel at 10, 50 of the window is "#00ff00"
      And the pixel at 40, 50 of the window is within 6 of "#7c0083"
      And the pixel at 80, 50 of the window is "#0000ff"
      And the pixel at 92, 50 of the window is "#00ff00"
