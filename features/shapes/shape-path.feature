# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shapepath.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shape.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: ShapePath
  A ShapePath is one path of a Shape together with how it is filled and
  stroked.

  @qt-parity
  Rule: The type and its properties

    Scenario: A new ShapePath is white, with a stroke one unit wide
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property int dashes: path.dashPattern.length
          Shape { ShapePath { id: path; objectName: "path" } }
        }
        """
      Then the property "strokeColor" of "path" is "#ffffff"
      And the property "strokeWidth" of "path" is 1
      And the property "fillColor" of "path" is "#ffffff"
      And the property "fillRule" of "path" is 0
      And the property "joinStyle" of "path" is 64
      And the property "capStyle" of "path" is 16
      And the property "miterLimit" of "path" is 2
      And the property "strokeStyle" of "path" is 1
      And the property "dashOffset" of "path" is 0
      And the property "dashes" of "root" is 2
      And the property "scale.width" of "path" is 1
      And the property "scale.height" of "path" is 1
      And the property "startX" of "path" is 0
      And the property "closed" of "path" is false

    Scenario: A new ShapePath has no fill gradient and dashes of four and two
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property bool plain: path.fillGradient === null
          property real dash: path.dashPattern[0]
          property real gap: path.dashPattern[1]
          Shape { ShapePath { id: path } }
        }
        """
      Then the property "plain" of "root" is true
      And the property "dash" of "root" is 4
      And the property "gap" of "root" is 2

    Scenario Outline: The keys of ShapePath's enumerations are numbers
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property int value: ShapePath.<key>
        }
        """
      Then the property "value" of "root" is <value>

      Examples:
        | key         | value |
        | OddEvenFill | 0     |
        | WindingFill | 1     |
        | MiterJoin   | 0     |
        | BevelJoin   | 64    |
        | RoundJoin   | 128   |
        | FlatCap     | 0     |
        | SquareCap   | 16    |
        | RoundCap    | 32    |
        | SolidLine   | 1     |
        | DashLine    | 2     |

  @qt-parity
  Rule: Fill and stroke

    Scenario: A path is filled, and stroked along its line
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "red"; strokeColor: "blue"; strokeWidth: 4
              startX: 20; startY: 20
              PathLine { x: 80; y: 20 }
              PathLine { x: 80; y: 80 }
              PathLine { x: 20; y: 80 }
              PathLine { x: 20; y: 20 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is "#ff0000"
      And the pixel at 17, 50 of the window is "#ffffff"
      And the pixel at 18, 50 of the window is "#0000ff"
      And the pixel at 21, 50 of the window is "#0000ff"
      And the pixel at 22, 50 of the window is "#ff0000"
      And the pixel at 50, 81 of the window is "#0000ff"
      And the pixel at 50, 82 of the window is "#ffffff"

    Scenario: A path that says nothing of its colours is white
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100; color: "black"
          Shape {
            ShapePath {
              startX: 20; startY: 20
              PathLine { x: 80; y: 20 }
              PathLine { x: 80; y: 80 }
              PathLine { x: 20; y: 80 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is "#ffffff"
      And the pixel at 10, 50 of the window is "#000000"

    Scenario: A path left open is filled as if a line closed it, and not stroked there
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "red"; strokeColor: "blue"; strokeWidth: 4
              capStyle: ShapePath.FlatCap
              startX: 20; startY: 20
              PathLine { x: 80; y: 20 }
              PathLine { x: 80; y: 80 }
            }
          }
        }
        """
      Then the pixel at 70, 40 of the window is "#ff0000"
      And the pixel at 50, 20 of the window is "#0000ff"
      And the pixel at 50, 50 of the window is within 4 of "#ff8080"
      And the pixel at 40, 60 of the window is "#ffffff"

    Scenario Outline: A stroke no wider than nothing is not painted
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"
              strokeWidth: <width>
              startX: 20; startY: 20
              PathLine { x: 20; y: 80 }
            }
          }
        }
        """
      Then the pixel at 20, 50 of the window is "<pixel>"

      Examples:
        | width | pixel   |
        | 4     | #000000 |
        | 0     | #ffffff |
        | -1    | #ffffff |
        | -4    | #ffffff |

    Scenario: A stroke one unit wide on a pixel boundary shades the pixels on both sides
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              strokeColor: "black"; capStyle: ShapePath.FlatCap
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 50, 49 of the window is within 4 of "#808080"
      And the pixel at 50, 50 of the window is within 4 of "#808080"
      And the pixel at 50, 48 of the window is "#ffffff"
      And the pixel at 50, 51 of the window is "#ffffff"

    Scenario: A transparent stroke or fill leaves the other alone
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "blue"; strokeWidth: 4
              startX: 10; startY: 10
              PathLine { x: 40; y: 10 }
              PathLine { x: 40; y: 40 }
              PathLine { x: 10; y: 40 }
              PathLine { x: 10; y: 10 }
            }
            ShapePath {
              fillColor: "red"; strokeColor: "transparent"; strokeWidth: 4
              startX: 60; startY: 60
              PathLine { x: 90; y: 60 }
              PathLine { x: 90; y: 90 }
              PathLine { x: 60; y: 90 }
              PathLine { x: 60; y: 60 }
            }
          }
        }
        """
      Then the pixel at 25, 25 of the window is "#ffffff"
      And the pixel at 10, 25 of the window is "#0000ff"
      And the pixel at 75, 75 of the window is "#ff0000"
      And the pixel at 59, 75 of the window is "#ffffff"
      And the pixel at 60, 75 of the window is "#ff0000"

    Scenario: A colour that is partly transparent lets what is behind show through
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "#80ff0000"; strokeWidth: -1
              PathLine { x: 100; y: 0 }
              PathLine { x: 100; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is within 2 of "#ff7f7f"

    Scenario Outline: The fill rule decides whether a part the path goes round twice is filled
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              fillRule: ShapePath.<rule>
              startX: 50; startY: 10
              PathLine { x: 74; y: 82 }
              PathLine { x: 12; y: 38 }
              PathLine { x: 88; y: 38 }
              PathLine { x: 26; y: 82 }
              PathLine { x: 50; y: 10 }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is "<middle>"
      And the pixel at 50, 22 of the window is "#000000"
      And the pixel at 20, 40 of the window is "#000000"
      And the pixel at 30, 20 of the window is "#ffffff"

      Examples:
        | rule        | middle  |
        | OddEvenFill | #ffffff |
        | WindingFill | #000000 |

    Scenario: A path's scale moves its points, not the width of its stroke
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            objectName: "shape"
            ShapePath {
              fillColor: "red"; strokeColor: "blue"; strokeWidth: 4
              capStyle: ShapePath.FlatCap
              scale: Qt.size(2, 3)
              startX: 10; startY: 10
              PathLine { x: 40; y: 10 }
              PathLine { x: 40; y: 30 }
              PathLine { x: 10; y: 30 }
              PathLine { x: 10; y: 10 }
            }
          }
        }
        """
      Then the pixel at 50, 60 of the window is "#ff0000"
      And the pixel at 81, 60 of the window is "#0000ff"
      And the pixel at 82, 60 of the window is "#ffffff"
      And the pixel at 50, 91 of the window is "#0000ff"
      And the pixel at 50, 92 of the window is "#ffffff"
      And the property "boundingRect.x" of "shape" is 18
      And the property "boundingRect.width" of "shape" is 64
      And the property "boundingRect.height" of "shape" is 64

    Scenario: What is painted follows the path and its colours
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          objectName: "root"
          width: 100; height: 100
          property alias reach: far.x
          Shape {
            ShapePath {
              objectName: "path"
              fillColor: "red"; strokeWidth: -1
              PathLine { id: far; x: 40; y: 0 }
              PathLine { x: far.x; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      Then the pixel at 60, 50 of the window is "#ffffff"
      When the property "reach" of "root" is set to 80
      Then the pixel at 60, 50 of the window is "#ff0000"
      When the property "fillColor" of "path" is set to "#00ff00"
      Then the pixel at 60, 50 of the window is "#00ff00"
      When the property "strokeWidth" of "path" is set to 10
      And the property "strokeColor" of "path" is set to "#0000ff"
      Then the pixel at 82, 50 of the window is "#0000ff"

  Rule: Joins and caps

    @qt-parity
    Scenario Outline: The join style shapes the outside of a corner
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"
              strokeWidth: 20; joinStyle: ShapePath.<join>
              startX: 30; startY: 80
              PathLine { x: 30; y: 30 }
              PathLine { x: 80; y: 30 }
            }
          }
        }
        """
      Then the pixel at 21, 21 of the window is "<tip>"
      And the pixel at 24, 24 of the window is "<shoulder>"
      And the pixel at 28, 28 of the window is "#000000"

      Examples:
        | join      | tip     | shoulder |
        | MiterJoin | #000000 | #000000  |
        | RoundJoin | #ffffff | #000000  |
        | BevelJoin | #ffffff | #ffffff  |

    @qt-parity
    Scenario Outline: The cap style shapes the ends of a stroke
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"
              strokeWidth: 20; capStyle: ShapePath.<cap>
              startX: 30; startY: 50
              PathLine { x: 70; y: 50 }
            }
          }
        }
        """
      Then the pixel at 24, 50 of the window is "<beyond>"
      And the pixel at 21, 41 of the window is "<corner>"
      And the pixel at 31, 50 of the window is "#000000"
      And the pixel at 19, 50 of the window is "#ffffff"

      Examples:
        | cap       | beyond  | corner  |
        | FlatCap   | #ffffff | #ffffff |
        | RoundCap  | #000000 | #ffffff |
        | SquareCap | #000000 | #000000 |

    @qt-parity
    Scenario: A miter limit of nothing cuts every corner off
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"
              strokeWidth: 20; joinStyle: ShapePath.MiterJoin
              miterLimit: 0
              startX: 30; startY: 80
              PathLine { x: 30; y: 30 }
              PathLine { x: 80; y: 30 }
            }
          }
        }
        """
      Then the pixel at 21, 21 of the window is "#ffffff"
      And the pixel at 24, 24 of the window is "#ffffff"
      And the pixel at 28, 28 of the window is "#000000"

    # Qt's software renderer cuts a miter off one stroke width from the corner
    # whatever the limit is, which leaves a blunt point; here the limit is the
    # one of SVG: the point is whole up to the limit and a bevel beyond it.
    @qt-diverges
    Scenario Outline: A corner sharper than the miter limit allows is cut off
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"
              strokeWidth: 10; joinStyle: ShapePath.MiterJoin
              miterLimit: <limit>; capStyle: ShapePath.FlatCap
              startX: 90; startY: 40
              PathLine { x: 30; y: 50 }
              PathLine { x: 90; y: 60 }
            }
          }
        }
        """
      Then the pixel at 10, 50 of the window is "<point>"
      And the pixel at 32, 50 of the window is "#000000"

      # The corner turns by about 19 degrees: its point is 6.08 stroke
      # widths long and ends near 0, 50. Cut off, the corner ends at 29.2.
      Examples:
        | limit | point   |
        | 2     | #ffffff |
        | 6     | #ffffff |
        | 7     | #000000 |

  @qt-parity
  Rule: Dashes

    Scenario: A dash pattern counts in stroke widths
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"; strokeWidth: 4
              capStyle: ShapePath.FlatCap
              strokeStyle: ShapePath.DashLine; dashPattern: [4, 2]
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 10, 50 of the window is "#000000"
      And the pixel at 25, 50 of the window is "#000000"
      And the pixel at 26, 50 of the window is "#ffffff"
      And the pixel at 33, 50 of the window is "#ffffff"
      And the pixel at 34, 50 of the window is "#000000"
      And the pixel at 49, 50 of the window is "#000000"
      And the pixel at 50, 50 of the window is "#ffffff"

    Scenario: A dash pattern is not used by a solid line
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"; strokeWidth: 4
              dashPattern: [1, 4]
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 30, 50 of the window is "#000000"
      And the pixel at 52, 50 of the window is "#000000"

    Scenario Outline: The dash offset starts the pattern further along
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"; strokeWidth: 4
              capStyle: ShapePath.FlatCap
              strokeStyle: ShapePath.DashLine; dashOffset: <offset>
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 12, 50 of the window is "<at12>"
      And the pixel at 24, 50 of the window is "<at24>"
      And the pixel at 30, 50 of the window is "<at30>"

      # The default pattern is a dash of 16 and a gap of 8 at this width.
      Examples:
        | offset | at12    | at24    | at30    |
        | 0      | #000000 | #000000 | #ffffff |
        | 1      | #000000 | #ffffff | #000000 |
        | 4.5    | #ffffff | #000000 | #000000 |
        | -1     | #ffffff | #000000 | #ffffff |

    Scenario: Each dash has the caps of the stroke
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"; strokeWidth: 4
              capStyle: ShapePath.SquareCap
              strokeStyle: ShapePath.DashLine
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 8, 50 of the window is "#000000"
      And the pixel at 27, 50 of the window is "#000000"
      And the pixel at 28, 50 of the window is "#ffffff"
      And the pixel at 31, 50 of the window is "#ffffff"
      And the pixel at 32, 50 of the window is "#000000"

    Scenario Outline: A dash or a gap of no length
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"; strokeWidth: 4
              capStyle: ShapePath.FlatCap
              strokeStyle: ShapePath.DashLine; dashPattern: <pattern>
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 12, 50 of the window is "<at12>"
      And the pixel at 30, 50 of the window is "<at30>"

      Examples:
        | pattern | at12    | at30    |
        | []      | #000000 | #000000 |
        | [2, 0]  | #000000 | #000000 |
        | [0, 2]  | #ffffff | #ffffff |
        | [4, 2]  | #000000 | #ffffff |

    Scenario: A pattern with an odd number of lengths gets a gap of one added
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "transparent"; strokeColor: "black"; strokeWidth: 4
              capStyle: ShapePath.FlatCap
              strokeStyle: ShapePath.DashLine; dashPattern: [3]
              startX: 10; startY: 50
              PathLine { x: 90; y: 50 }
            }
          }
        }
        """
      Then the pixel at 21, 50 of the window is "#000000"
      And the pixel at 22, 50 of the window is "#ffffff"
      And the pixel at 25, 50 of the window is "#ffffff"
      And the pixel at 26, 50 of the window is "#000000"
      And the log has a warning matching "Pattern not of even length"
