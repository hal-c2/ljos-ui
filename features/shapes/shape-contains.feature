# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shape.html#containsMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#contains-method
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Which points a Shape contains
  A Shape contains the points of its rectangle, or with
  containsMode: Shape.FillContains the points its paths fill. That decides
  where the pointer reaches it.

  @qt-parity
  Rule: The points a Shape contains

    Scenario Outline: A Shape contains its rectangle unless told otherwise
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real px: -1
          property real py: -1
          property bool held: shape.contains(Qt.point(px, py))
          Shape {
            id: shape
            ShapePath {
              strokeWidth: 0; startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
            }
          }
        }
        """
      When the property "px" of "root" is set to <x>
      And the property "py" of "root" is set to <y>
      Then the property "held" of "root" is <held>

      Examples:
        | x  | y  | held  |
        | 12 | 12 | true  |
        | 50 | 60 | true  |
        | 95 | 50 | false |

    Scenario Outline: With FillContains a Shape contains what its path fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real px: -1
          property real py: -1
          property bool held: shape.contains(Qt.point(px, py))
          Shape {
            id: shape
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: 10; fillColor: "transparent"
              startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
            }
          }
        }
        """
      When the property "px" of "root" is set to <x>
      And the property "py" of "root" is set to <y>
      Then the property "held" of "root" is <held>

      # The path need not be closed or have a colour, and its stroke does
      # not count.
      Examples:
        | x  | y  | held  |
        | 12 | 12 | false |
        | 50 | 60 | true  |
        | 50 | 89 | true  |
        | 50 | 93 | false |
        | 30 | 49 | false |
        | 31 | 51 | true  |

    Scenario Outline: A stroke alone fills nothing
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real py: -1
          property bool held: shape.contains(Qt.point(50, py))
          Shape {
            id: shape
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: 10; strokeColor: "red"; startY: 50
              PathLine { x: 100; y: 50 }
            }
          }
        }
        """
      When the property "py" of "root" is set to <y>
      Then the property "held" of "root" is false

      Examples:
        | y  |
        | 50 |
        | 52 |

    Scenario Outline: A point in any of a Shape's paths is contained
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real at: -1
          property bool held: shape.contains(Qt.point(at, at))
          Shape {
            id: shape
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: -1
              PathRectangle { width: 10; height: 10 }
            }
            ShapePath {
              strokeWidth: -1
              PathRectangle { x: 50; y: 50; width: 10; height: 10 }
            }
          }
        }
        """
      When the property "at" of "root" is set to <at>
      Then the property "held" of "root" is <held>

      Examples:
        | at | held  |
        | 5  | true  |
        | 55 | true  |
        | 30 | false |

    Scenario Outline: What is filled counts, not the Shape's own size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real px: -1
          property real py: -1
          property bool held: shape.contains(Qt.point(px, py))
          Shape {
            id: shape
            width: 20; height: 20
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: -1; startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
            }
          }
        }
        """
      When the property "px" of "root" is set to <x>
      And the property "py" of "root" is set to <y>
      Then the property "held" of "root" is <held>

      Examples:
        | x  | y  | held  |
        | 50 | 60 | true  |
        | 5  | 5  | false |

    Scenario Outline: A path's scale moves what it fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real at: -1
          property bool held: shape.contains(Qt.point(at, at))
          Shape {
            id: shape
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: -1; scale: Qt.size(2, 2)
              PathLine { x: 20; y: 0 }
              PathLine { x: 20; y: 20 }
              PathLine { x: 0; y: 20 }
            }
          }
        }
        """
      When the property "at" of "root" is set to <at>
      Then the property "held" of "root" is <held>

      Examples:
        | at | held  |
        | 30 | true  |
        | 45 | false |

    Scenario Outline: A part a path goes round twice is not contained, whatever the fill rule
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property real py: -1
          property bool held: shape.contains(Qt.point(50, py))
          Shape {
            id: shape
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: -1; fillRule: ShapePath.<rule>
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
      When the property "py" of "root" is set to <y>
      Then the property "held" of "root" is <held>

      Examples:
        | rule        | y  | held  |
        | OddEvenFill | 50 | false |
        | OddEvenFill | 20 | true  |
        | WindingFill | 50 | false |
        | WindingFill | 20 | true  |

    Scenario: The contained points follow the path
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          objectName: "root"
          property alias reach: far.x
          property real py: -1
          property bool held: shape.contains(Qt.point(60, py))
          Shape {
            id: shape
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: -1
              PathLine { id: far; x: 40; y: 0 }
              PathLine { x: far.x; y: 100 }
              PathLine { x: 0; y: 100 }
            }
          }
        }
        """
      When the property "py" of "root" is set to 50
      Then the property "held" of "root" is false
      When the property "reach" of "root" is set to 80
      And the property "py" of "root" is set to 51
      Then the property "held" of "root" is true

  @qt-parity
  Rule: Where the pointer reaches a Shape

    Scenario: A tap outside what a FillContains Shape fills goes past it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          id: root
          objectName: "root"
          width: 100; height: 100
          property int taps: 0
          property int behind: 0
          TapHandler { onTapped: root.behind++ }
          Shape {
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: -1; fillColor: "black"; startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
            }
            TapHandler { onTapped: root.taps++ }
          }
        }
        """
      When the user touches 50, 60
      Then the property "taps" of "root" is 1
      When the user touches 15, 20
      Then the property "taps" of "root" is 1
      And the property "behind" of "root" is at least 1

    Scenario: A tap anywhere in the rectangle of a plain Shape reaches it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          id: root
          objectName: "root"
          width: 100; height: 100
          property int taps: 0
          Shape {
            ShapePath {
              strokeWidth: 0; fillColor: "black"; startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
            }
            TapHandler { onTapped: root.taps++ }
          }
        }
        """
      When the user touches 15, 20
      Then the property "taps" of "root" is 1
      When the user touches 95, 50
      Then the property "taps" of "root" is 1

    Scenario: A MouseArea in a FillContains Shape is reached where it is, not where the Shape fills
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          id: root
          objectName: "root"
          width: 100; height: 100
          property int clicks: 0
          Shape {
            containsMode: Shape.FillContains
            ShapePath {
              strokeWidth: 0; fillColor: "black"; startX: 50; startY: 10
              PathLine { x: 90; y: 90 }
              PathLine { x: 10; y: 90 }
            }
            MouseArea { width: 30; height: 30; onClicked: root.clicks++ }
          }
        }
        """
      When the user touches 15, 20
      Then the property "clicks" of "root" is 1
