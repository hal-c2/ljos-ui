# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pathsvg.html
#   https://www.w3.org/TR/SVG11/paths.html#PathData
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: PathSvg
  A PathSvg is a part of a path written as the path data of SVG.

  Rule: The path data of SVG

    @qt-parity
    Scenario: A new PathSvg has no path data
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string written: svg.path
          Path { PathSvg { id: svg } }
        }
        """
      Then the property "written" of "root" is ""

    @qt-parity
    Scenario Outline: Where path data goes: <what>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathSvg { path: "<data>" }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.01 of <x>
      And the property "boundingRect.y" of "shape" is within 0.01 of <y>
      And the property "boundingRect.width" of "shape" is within 0.01 of <w>
      And the property "boundingRect.height" of "shape" is within 0.01 of <h>

      Examples: Lines
        | what                                 | data                                    | x   | y   | w  | h  |
        | move and lines                       | M 10 20 L 60 20 L 60 80                 | 10  | 20  | 50 | 60 |
        | commas and no spaces                 | M10,20L60,20,60,80                      | 10  | 20  | 50 | 60 |
        | points after a move are lines        | M 10 20 60 20 60 80                     | 10  | 20  | 50 | 60 |
        | and relative after a relative move   | m 10 20 50 0 0 60                       | 10  | 20  | 50 | 60 |
        | relative lines                       | M 10 20 l 50 0 l 0 60                   | 10  | 20  | 50 | 60 |
        | horizontal and vertical              | M 10 20 H 60 V 80                       | 10  | 20  | 50 | 60 |
        | relative horizontal and vertical     | M 10 20 h 50 v 60                       | 10  | 20  | 50 | 60 |
        | repeated horizontal and vertical     | M 10 20 h 20 30 v 30 30                 | 10  | 20  | 50 | 60 |
        | a relative move after a move         | M 10 20 m 5 5 l 10 10                   | 15  | 25  | 10 | 10 |
        | two parts                            | M 10 10 L 20 20 M 50 50 L 80 90         | 10  | 10  | 70 | 80 |
        | a move at the end                    | M 10 10 L 20 20 M 100 100               | 10  | 10  | 90 | 90 |
        | only a move                          | M 10 20                                 | 10  | 20  | 0  | 0  |

      Examples: Numbers
        | what                                 | data                                    | x   | y   | w  | h    |
        | a sign starts a number               | M10-20L-30-40                           | -30 | -40 | 40 | 20   |
        | a second point starts a number       | M.5.5L10.5.25                           | 0.5 | 0.25 | 10 | 0.25 |
        | a number may end in a point          | M 5. 5. L 10. 20.                       | 5   | 5   | 5  | 15   |
        | exponents                            | M 1e1 2E1 L 6e+1 800e-1                 | 10  | 20  | 50 | 60   |
        | plus signs                           | M +10 +20 L +60 +80                     | 10  | 20  | 50 | 60   |
        | a comma at the end                   | M 10 20 L 60 80,                        | 10  | 20  | 50 | 60   |

      Examples: Curves
        | what                                 | data                                    | x   | y     | w   | h     |
        | a cubic curve                        | M 10 50 C 10 10 90 10 90 50             | 10  | 20    | 80  | 30    |
        | a relative cubic curve               | M 10 50 c 0 -40 80 -40 80 0             | 10  | 20    | 80  | 30    |
        | a smooth cubic curve                 | M 10 50 C 10 10 50 10 50 50 S 90 90 90 50 | 10 | 20   | 80  | 60    |
        | a relative smooth cubic curve        | M 10 50 c 0 -40 40 -40 40 0 s 40 40 40 0 | 10 | 20    | 80  | 60    |
        | a smooth curve after no curve        | M 10 50 S 90 10 90 50                   | 10  | 32.22 | 80  | 17.78 |
        | a quadratic curve                    | M 10 10 Q 60 110 110 10                 | 10  | 10    | 100 | 50    |
        | a relative quadratic curve           | M 10 10 q 50 100 100 0                  | 10  | 10    | 100 | 50    |
        | a smooth quadratic curve             | M 10 50 Q 30 10 50 50 T 90 50           | 10  | 30    | 80  | 40    |
        | a relative smooth quadratic curve    | M 10 50 q 20 -40 40 0 t 40 0            | 10  | 30    | 80  | 40    |
        | a chain of smooth quadratic curves   | M 10 50 Q 30 10 50 50 T 90 50 130 50    | 10  | 30    | 120 | 40    |
        | a smooth quadratic after no curve    | M 10 50 T 90 80                         | 10  | 50    | 80  | 30    |

      Examples: Arcs
        | what                                 | data                                    | x   | y   | w   | h  |
        | an arc with the sweep flag           | M 10 50 A 40 40 0 0 1 90 50             | 10  | 10  | 80  | 40 |
        | an arc without it                    | M 10 50 A 40 40 0 0 0 90 50             | 10  | 50  | 80  | 40 |
        | the large arc                        | M 10 50 A 50 50 0 1 1 90 50             | 0   | -30 | 100 | 80 |
        | a relative arc                       | M 10 50 a 40 20 0 0 1 80 0              | 10  | 30  | 80  | 20 |
        | a turned arc                         | M 10 50 A 20 40 90 0 1 90 50            | 10  | 30  | 80  | 20 |
        | two arcs after one letter            | M 10 50 A 20 20 0 0 1 50 50 20 20 0 0 0 90 50 | 10 | 30 | 80 | 40 |

      Examples: Closing
        | what                                 | data                                    | x   | y   | w  | h  |
        | a closed part                        | M 10 10 L 50 10 L 50 50 Z               | 10  | 10  | 40 | 40 |
        | a line after closing starts again    | M 10 10 L 50 10 L 50 50 Z L 0 100       | 0   | 10  | 50 | 90 |
        | and so does a relative one           | M 10 10 L 50 10 L 50 50 z l -5 80       | 5   | 10  | 45 | 80 |
        | a relative move after closing        | M 10 10 h 10 v 10 z m 30 30 h 10 v 10 z | 10  | 10  | 40 | 40 |
        | closing twice                        | M 10 10 L 50 10 L 50 50 Z Z             | 10  | 10  | 40 | 40 |

    @qt-parity
    Scenario: Path data may be written on several lines
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathSvg { path: "M 10 20\n\tL 60 20\r\n L 60 80" }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is 10
      And the property "boundingRect.width" of "shape" is 50
      And the property "boundingRect.height" of "shape" is 60

    @qt-parity
    Scenario: Path data is painted
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 100; height: 100
          Shape {
            ShapePath {
              fillColor: "black"; strokeWidth: -1
              PathSvg { path: "M 10 50 A 40 40 0 0 1 90 50 Z M 20 60 h 60 v 30 h -60 z" }
            }
          }
        }
        """
      Then the pixel at 50, 30 of the window is "#000000"
      And the pixel at 50, 55 of the window is "#ffffff"
      And the pixel at 50, 75 of the window is "#000000"
      And the pixel at 15, 20 of the window is "#ffffff"

    # Qt reads each of an arc's two flags as a number, so it needs something
    # between them. SVG lets a flag be one digit with nothing after it, and
    # files made by tools that shorten path data are written that way.
    @qt-diverges
    Scenario Outline: The flags of an arc need nothing between them
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathSvg { path: "<data>" }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is within 0.01 of 10
      And the property "boundingRect.y" of "shape" is within 0.01 of <y>
      And the property "boundingRect.width" of "shape" is within 0.01 of 80
      And the property "boundingRect.height" of "shape" is within 0.01 of 40

      Examples:
        | data                       | y  |
        | M 10 50 A 40 40 0 01 90 50 | 10 |
        | M10 50a40 40 0 0180 0      | 10 |
        | M10 50a40 40 0 0080 0      | 50 |

  Rule: A PathSvg among other elements

    @qt-parity
    Scenario Outline: Path data and the elements around it
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
              startX: 5; startY: 5
              <elements>
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is <x>
      And the property "boundingRect.y" of "shape" is <y>
      And the property "boundingRect.width" of "shape" is <w>
      And the property "boundingRect.height" of "shape" is <h>

      # The path goes on from where the path data ended. Path data with no
      # move goes on from where the path was, but counts its own coordinates
      # from the origin.
      Examples:
        | elements                                                                | x  | y  | w  | h  |
        | PathSvg { path: "M 10 20 L 60 20" } PathLine { relativeX: 0; relativeY: 50 } | 10 | 20 | 50 | 50 |
        | PathLine { x: 40; y: 30 } PathSvg { path: "L 60 20" }                   | 5  | 5  | 55 | 25 |
        | PathLine { x: 40; y: 30 } PathSvg { path: "l 10 10" }                   | 5  | 5  | 35 | 25 |
        | PathSvg { path: "" }                                                    | 5  | 5  | 0  | 0  |

  Rule: Path data that cannot be read

    @qt-parity
    Scenario Outline: What can be read before a mistake is kept: <what>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            objectName: "shape"
            ShapePath {
              strokeWidth: 0
              PathSvg { path: "<data>" }
            }
          }
        }
        """
      Then the property "boundingRect.x" of "shape" is <x>
      And the property "boundingRect.y" of "shape" is <y>
      And the property "boundingRect.width" of "shape" is <w>
      And the property "boundingRect.height" of "shape" is <h>

      Examples:
        | what                          | data                          | x  | y  | w  | h  |
        | a letter that is no command   | M 10 20 L 60 20 X 5 5 L 90 90 | 10 | 20 | 50 | 0  |
        | a point with one coordinate   | M 10 20 L 60 20 L 90          | 10 | 20 | 50 | 0  |
        | a curve short of numbers      | M 10 50 C 10 10 90 10         | 10 | 50 | 0  | 0  |
        | a letter inside a number      | M 10 20 L 6o 20               | 10 | 20 | 0  | 0  |
        | two commas                    | M 10 20 L 60,,80              | 10 | 20 | 0  | 0  |
        | numbers before any command    | 10 20 L 60 20                 | 0  | 0  | 0  | 0  |
        | later commands are not read   | M 10 20 L 60 20 L 70 30 Q 1   | 10 | 20 | 60 | 10 |

    # Qt says nothing about path data it cannot read. A warning that names
    # the place saves a search through a long string.
    @qt-diverges
    Scenario Outline: A mistake in path data is reported with its place
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            ShapePath {
              strokeWidth: 0
              PathSvg { path: "<data>" }
            }
          }
        }
        """
      Then the log has a warning matching "<warning>"

      Examples:
        | data                          | warning                                                     |
        | M 10 20 L 60 20 X 5 5 L 90 90 | PathSvg: unknown command 'X' at character 17                |
        | M 10 20 L 60 20 L 90          | PathSvg: 'L' needs 2 numbers, found 1, at character 17      |
        | M 10 50 C 10 10 90 10         | PathSvg: 'C' needs 6 numbers, found 4, at character 9       |
        | M 10 20 60 20 60              | PathSvg: 'L' needs 2 numbers, found 1, at character 15      |
        | M 10 20 L 6o 20               | PathSvg: unexpected 'o' at character 12                     |
        | M 10 20 L 60,,80              | PathSvg: unexpected ',' at character 14                     |
        | 10 20 L 60 20                 | PathSvg: a number where a command should be, at character 1 |
        | M 10 50 A 40 40 0 2 1 90 50   | PathSvg: an arc flag must be 0 or 1, at character 19        |

    @qt-diverges
    Scenario: Path data that can be read is not warned about
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          Shape {
            ShapePath {
              strokeWidth: 0
              PathSvg { path: "M10-20L-30-40 1e1.5z" }
            }
          }
        }
        """
      Then the log has no warnings
