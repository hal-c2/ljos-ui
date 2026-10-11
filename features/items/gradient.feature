# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-gradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-gradientstop.html
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#gradient-prop
#   https://doc.qt.io/qt-6/qgradient.html#Preset-enum
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Gradient and GradientStop
  A Rectangle with a gradient is filled with a blend of the colours of the
  gradient's stops, from its top to its bottom or from its left to its right.

  @qt-parity
  Rule: The types and their properties

    Scenario: A Rectangle has no gradient until one is assigned
      Given the QML:
        """qml
        import QtQuick
        Rectangle { objectName: "box"; width: 100; height: 100 }
        """
      Then the property "gradient" of "box" is null

    Scenario: A stop is black at position 0 until it says otherwise
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          property alias position: stop.position
          property alias stopColor: stop.color
          gradient: Gradient { GradientStop { id: stop } }
        }
        """
      Then the property "position" of "box" is 0
      And the property "stopColor" of "box" is "#000000"

    Scenario: A gradient is vertical and has no stops until it is given some
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          property alias orientation: grad.orientation
          property int count: grad.stops.length
          gradient: Gradient { id: grad }
        }
        """
      Then the property "orientation" of "box" is 2
      And the property "count" of "box" is 0

    Scenario Outline: The orientations are numbers
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int value: Gradient.<key>
        }
        """
      Then the property "value" of "root" is <value>

      Examples:
        | key        | value |
        | Vertical   | 2     |
        | Horizontal | 1     |

    Scenario: The orientation is given by its name
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          property alias orientation: grad.orientation
          gradient: Gradient { id: grad; orientation: Gradient.Horizontal }
        }
        """
      Then the property "orientation" of "box" is 1

    Scenario: The stops written inside a gradient are its stops, in the order written
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          property int count: grad.stops.length
          property real first: grad.stops[0].position
          property real last: grad.stops[2].position
          gradient: Gradient {
            id: grad
            GradientStop { position: 1; color: "blue" }
            GradientStop { position: 0; color: "red" }
            GradientStop { position: 0.5; color: "green" }
          }
        }
        """
      Then the property "count" of "box" is 3
      And the property "first" of "box" is 1
      And the property "last" of "box" is 0.5

    Scenario: The stops may be given as a list
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          property int count: grad.stops.length
          property color second: grad.stops[1].color
          gradient: Gradient {
            id: grad
            stops: [
              GradientStop { position: 0.2; color: "red" },
              GradientStop { position: 0.8; color: "#0000ff" }
            ]
          }
        }
        """
      Then the property "count" of "box" is 2
      And the property "second" of "box" is "#0000ff"

    Scenario: One gradient may fill several rectangles
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool same: a.gradient === b.gradient && a.gradient === shared
          Gradient {
            id: shared
            GradientStop { position: 0; color: "red" }
            GradientStop { position: 1; color: "blue" }
          }
          Rectangle { id: a; width: 50; height: 50; gradient: shared }
          Rectangle { id: b; x: 50; width: 50; height: 50; gradient: shared }
        }
        """
      Then the property "same" of "root" is true
      And the pixel at 25, 49 of the window is within 4 of "#0200fd"
      And the pixel at 75, 49 of the window is within 4 of "#0200fd"

    # The three refusals below wait for the engine: when a file is loaded,
    # an object of the wrong type in a list and a handler for a signal that
    # does not exist are warnings in the log and the file loads all the same.
    # Ljós has no stop signals and takes only stops; it does not yet say no.
    @backlog
    Scenario: Something that is not a stop cannot be put in a gradient
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Rectangle { gradient: Gradient { Item {} } }
        """
      Then loading "Wrong.qml" is rejected

    @backlog
    Scenario: A stop has no signal for its position
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Rectangle {
          gradient: Gradient { GradientStop { onPositionChanged: console.log("moved") } }
        }
        """
      Then loading "Wrong.qml" is rejected

    @backlog
    Scenario: A Rectangle has no signal for its gradient
      Given the file "Wrong.qml":
        """qml
        import QtQuick
        Rectangle { onGradientChanged: console.log("changed") }
        """
      Then loading "Wrong.qml" is rejected

  @qt-parity
  Rule: What a gradient paints

    Scenario: A gradient is painted instead of the colour, from the top to the bottom
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "green"
          gradient: Gradient {
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        """
      Then the pixel at 50, 0 of the window is within 4 of "#fe0001"
      And the pixel at 50, 50 of the window is within 4 of "#7e0081"
      And the pixel at 50, 99 of the window is within 4 of "#0100fe"
      And the pixel at 5, 50 of the window is within 4 of "#7e0081"
      And the pixel at 95, 50 of the window is within 4 of "#7e0081"

    Scenario: A horizontal gradient is painted from the left to the right
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        """
      Then the pixel at 0, 50 of the window is within 4 of "#fe0001"
      And the pixel at 50, 50 of the window is within 4 of "#7e0081"
      And the pixel at 99, 50 of the window is within 4 of "#0100fe"
      And the pixel at 50, 5 of the window is within 4 of "#7e0081"
      And the pixel at 50, 95 of the window is within 4 of "#7e0081"

    Scenario: A stop in the middle is passed through
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          gradient: Gradient {
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 0.5; color: "#00ff00" }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        """
      Then the pixel at 50, 25 of the window is within 4 of "#7d8200"
      And the pixel at 50, 49 of the window is within 4 of "#03fc00"
      And the pixel at 50, 75 of the window is within 4 of "#007d82"

    Scenario: The first stop's colour is painted before it and the last stop's after it
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          gradient: Gradient {
            GradientStop { position: 0.25; color: "#ff0000" }
            GradientStop { position: 0.75; color: "#0000ff" }
          }
        }
        """
      Then the pixel at 50, 0 of the window is "#ff0000"
      And the pixel at 50, 20 of the window is "#ff0000"
      And the pixel at 50, 50 of the window is within 4 of "#7d0082"
      And the pixel at 50, 80 of the window is "#0000ff"
      And the pixel at 50, 99 of the window is "#0000ff"

    Scenario: The stops are painted in the order of their positions
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          gradient: Gradient {
            GradientStop { position: 1; color: "#0000ff" }
            GradientStop { position: 0; color: "#ff0000" }
          }
        }
        """
      Then the pixel at 50, 0 of the window is within 4 of "#fe0001"
      And the pixel at 50, 99 of the window is within 4 of "#0100fe"

    Scenario: A gradient with one stop is that stop's colour all over
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "green"
          gradient: Gradient { GradientStop { position: 0.5; color: "#ff0000" } }
        }
        """
      Then the pixel at 50, 0 of the window is "#ff0000"
      And the pixel at 50, 50 of the window is "#ff0000"
      And the pixel at 50, 99 of the window is "#ff0000"

    Scenario: A gradient without stops leaves the colour
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#008000"
          gradient: Gradient { }
        }
        """
      Then the pixel at 50, 0 of the window is "#008000"
      And the pixel at 50, 99 of the window is "#008000"

    Scenario: Of two stops at one position the one written first is painted
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          gradient: Gradient {
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 0.5; color: "#00ff00" }
            GradientStop { position: 0.5; color: "#0000ff" }
            GradientStop { position: 1; color: "#ffffff" }
          }
        }
        """
      Then the pixel at 50, 48 of the window is within 4 of "#08f700"
      And the pixel at 50, 52 of the window is within 4 of "#0dff0d"
      And the pixel at 50, 90 of the window is within 4 of "#ceffce"

    Scenario: A position outside 0 to 1 is where the stop is, off the rectangle
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          gradient: Gradient {
            GradientStop { position: -1; color: "#ff0000" }
            GradientStop { position: 2; color: "#0000ff" }
          }
        }
        """
      Then the pixel at 50, 0 of the window is within 4 of "#aa0055"
      And the pixel at 50, 99 of the window is within 4 of "#5500aa"

    Scenario: A stop that is transparent lets what is behind show, without its own colour
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Rectangle {
            width: 100; height: 100
            gradient: Gradient {
              GradientStop { position: 0; color: "#ff0000" }
              GradientStop { position: 1; color: "#000000ff" }
            }
          }
        }
        """
      Then the pixel at 50, 9 of the window is within 4 of "#ff1818"
      And the pixel at 50, 50 of the window is within 4 of "#ff8181"
      And the pixel at 50, 99 of the window is within 4 of "#fffefe"

    Scenario: A half transparent stop is blended with what is behind
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Rectangle {
            width: 100; height: 100
            gradient: Gradient {
              GradientStop { position: 0; color: "#80ff0000" }
              GradientStop { position: 1; color: "#800000ff" }
            }
          }
        }
        """
      Then the pixel at 50, 0 of the window is within 4 of "#ff7f80"
      And the pixel at 50, 99 of the window is within 4 of "#807fff"

    Scenario: The opacity of the rectangle applies to its gradient
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Rectangle {
            width: 100; height: 100
            opacity: 0.5
            gradient: Gradient {
              GradientStop { position: 0; color: "#ff0000" }
              GradientStop { position: 1; color: "#0000ff" }
            }
          }
        }
        """
      Then the pixel at 50, 0 of the window is within 4 of "#ff8081"
      And the pixel at 50, 99 of the window is within 4 of "#8180ff"

  @qt-parity
  Rule: Corners and borders

    Scenario: A rounded rectangle cuts its gradient at the corners
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Rectangle {
            width: 100; height: 100
            radius: 30
            gradient: Gradient {
              GradientStop { position: 0; color: "#ff0000" }
              GradientStop { position: 1; color: "#0000ff" }
            }
          }
        }
        """
      Then the pixel at 2, 2 of the window is "#ffffff"
      And the pixel at 97, 97 of the window is "#ffffff"
      And the pixel at 50, 2 of the window is within 4 of "#f90006"
      And the pixel at 2, 50 of the window is within 4 of "#7e0081"
      And the pixel at 50, 97 of the window is within 4 of "#0600f9"

    Scenario: The border is painted over a gradient that spans the whole rectangle
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          border.width: 10; border.color: "#00ff00"
          gradient: Gradient {
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        """
      Then the pixel at 50, 5 of the window is "#00ff00"
      And the pixel at 5, 50 of the window is "#00ff00"
      And the pixel at 50, 18 of the window is within 4 of "#d0002f"
      And the pixel at 50, 50 of the window is within 4 of "#7e0081"
      And the pixel at 50, 81 of the window is within 4 of "#2f00d0"

    Scenario: The gradient is not painted under the border
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Rectangle {
            width: 100; height: 100
            border.width: 10; border.color: "#8000ff00"
            gradient: Gradient {
              GradientStop { position: 0; color: "#ff0000" }
              GradientStop { position: 1; color: "#0000ff" }
            }
          }
        }
        """
      Then the pixel at 50, 5 of the window is within 2 of "#7fff7f"
      And the pixel at 5, 50 of the window is within 2 of "#7fff7f"
      And the pixel at 50, 50 of the window is within 4 of "#7e0081"

    Scenario: A rounded, bordered rectangle keeps its gradient inside the border
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Rectangle {
            width: 100; height: 100
            radius: 30
            border.width: 6; border.color: "#000000"
            gradient: Gradient {
              GradientStop { position: 0; color: "#ff0000" }
              GradientStop { position: 1; color: "#0000ff" }
            }
          }
        }
        """
      Then the pixel at 2, 2 of the window is "#ffffff"
      And the pixel at 50, 2 of the window is "#000000"
      And the pixel at 2, 50 of the window is "#000000"
      And the pixel at 50, 50 of the window is within 4 of "#7e0081"
      And the pixel at 50, 90 of the window is within 4 of "#1800e7"

  @qt-parity
  Rule: A gradient that changes

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: box
          objectName: "box"
          width: 100; height: 100
          color: "#008000"
          property int updates: 0
          property int turns: 0
          property bool ready: false
          property alias topColor: top.color
          property alias topPosition: top.position
          property alias orientation: grad.orientation
          gradient: Gradient {
            id: grad
            onUpdated: if (box.ready) box.updates++
            onOrientationChanged: box.turns++
            GradientStop { id: top; position: 0; color: "#ff0000" }
            GradientStop { position: 1; color: "#0000ff" }
          }
          Gradient { id: other; GradientStop { color: "#ffff00" } }
          function swap() { gradient = other }
          Component.onCompleted: ready = true
        }
        """

    Scenario: A stop's colour that changes is painted
      When the property "topColor" of "box" is set to "#00ff00"
      Then the pixel at 50, 0 of the window is within 4 of "#00fe01"
      And the pixel at 50, 99 of the window is within 4 of "#0001fe"

    Scenario: A stop's position that changes is painted
      When the property "topPosition" of "box" is set to 0.5
      Then the pixel at 50, 0 of the window is "#ff0000"
      And the pixel at 50, 40 of the window is "#ff0000"
      And the pixel at 50, 75 of the window is within 4 of "#7d0082"

    Scenario: An orientation that changes is painted
      When the property "orientation" of "box" is set to 1
      Then the pixel at 0, 50 of the window is within 4 of "#fe0001"
      And the pixel at 99, 50 of the window is within 4 of "#0100fe"
      And the property "turns" of "box" is 1

    Scenario Outline: The gradient tells that it was updated
      When the property "<property>" of "box" is set to <value>
      Then the property "updates" of "box" is 1

      Examples:
        | property    | value     |
        | topColor    | "#00ff00" |
        | topPosition | 0.5       |
        | orientation | 1         |

    Scenario: The colour of the rectangle is not an update of its gradient
      When the property "color" of "box" is set to "#123456"
      Then the property "updates" of "box" is 0
      And the pixel at 50, 0 of the window is within 4 of "#fe0001"

    Scenario: Another gradient may be assigned while the application runs
      When the method "swap" of "box" is called
      Then the pixel at 50, 0 of the window is "#ffff00"
      And the pixel at 50, 99 of the window is "#ffff00"

  @qt-parity
  Rule: A gradient taken away

    Scenario Outline: A gradient taken away leaves the colour
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          color: "#008000"
          property string reads: "a gradient"
          gradient: Gradient {
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 1; color: "#0000ff" }
          }
          function take() { gradient = <nothing>; reads = String(gradient) }
        }
        """
      When the method "take" of "box" is called
      Then the property "reads" of "box" is "<nothing>"
      And the property "gradient" of "box" is null
      And the pixel at 50, 0 of the window is "#008000"
      And the pixel at 50, 99 of the window is "#008000"

      Examples:
        | nothing   |
        | undefined |
        | null      |

  @qt-parity
  Rule: Stops that follow something

    Scenario: A stop bound to a property follows it
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          id: box
          objectName: "box"
          width: 100; height: 100
          property color accent: "#ff0000"
          property real where: 0
          property alias topColor: top.color
          property alias topPosition: top.position
          gradient: Gradient {
            GradientStop { id: top; position: box.where; color: box.accent }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        """
      When the property "accent" of "box" is set to "#00ff00"
      And the property "where" of "box" is set to 0.5
      Then the property "topColor" of "box" is "#00ff00"
      And the property "topPosition" of "box" is 0.5
      And the pixel at 50, 40 of the window is "#00ff00"
      And the pixel at 50, 75 of the window is within 4 of "#007d82"

    Scenario: A stop's colour may be animated
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          property alias topColor: top.color
          gradient: Gradient {
            GradientStop {
              id: top
              position: 0
              ColorAnimation on color { from: "#ff0000"; to: "#00ff00"; duration: 100 }
            }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        """
      When 300 milliseconds pass
      Then the property "topColor" of "box" is "#00ff00"
      And the pixel at 50, 0 of the window is within 4 of "#00fe01"

  @qt-parity
  Rule: What is not a gradient

    Scenario: An object that is not a gradient is refused with a warning
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          color: "#008000"
          gradient: Item { }
        }
        """
      Then the log has a warning matching "Can't assign Item to gradient property"
      And the property "gradient" of "box" is null
      And the pixel at 50, 50 of the window is "#008000"

    Scenario: A value that cannot name a gradient is refused with a warning
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          color: "#008000"
          gradient: true
        }
        """
      Then the log has a warning matching "Unknown gradient type. Expected int, string, or Gradient"
      And the property "gradient" of "box" is null
      And the pixel at 50, 50 of the window is "#008000"

    Scenario Outline: A preset that does not exist is refused with a warning
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          color: "#008000"
          gradient: <value>
        }
        """
      Then the log has a warning matching "No such gradient preset '<name>'"
      And the property "gradient" of "box" is null
      And the pixel at 50, 50 of the window is "#008000"

      Examples:
        | value  | name |
        | 5000   | 5000 |
        | 0      | 0    |
        | "nope" | nope |

  @qt-parity
  Rule: Presets

    # The presets are a table of 180 named gradients (the WebGradients
    # collection) that Qt carries; Ljós has none of them yet.
    @backlog
    Scenario Outline: A preset is given by its number or by its name
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          color: "#008000"
          gradient: <value>
        }
        """
      Then the property "gradient" of "box" is <read>
      And the pixel at 50, 0 of the window is within 4 of "#fbc2eb"
      And the pixel at 50, 99 of the window is within 4 of "#a18cd1"

      Examples:
        | value              | read        |
        | Gradient.NightFade | 2           |
        | "NightFade"        | "NightFade" |
