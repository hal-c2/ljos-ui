# Sources:
#   docs/DIALECT.md §4.14 ("Canvas, Shape: the same")
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shape.html
#   https://doc.qt.io/qt-6/qml-qtquick-shapes-shapepath.html
#   https://doc.qt.io/qt-6/qml-qtquick-pathsvg.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# The Qt file is `@qt-parity`. `Ljos.Ui` is Ljós's, so the same shapes written in it, and lowering a file
# to it, are `@ljos-only`.

Feature: Shapes in Ljós's API
  `Ljos.Ui` has Qt's shapes as they are: `Shape`, `ShapePath`, the
  elements of a `Path` (`PathSvg`, `PathLine` and the rest), and the
  gradients that fill a path with their `GradientStop`s. A file that
  imports `QtQuick.Shapes` is lowered with nothing changed in its shapes,
  and the import is gone, since `Ljos.Ui` has them.

  Rule: A shape in Ljós's API is the shape in Qt's

    @qt-parity
    Scenario: A path filled with a colour and a path filled with a gradient
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Shapes
        Item {
          width: 200; height: 60
          Shape {
            objectName: "shape"; preferredRendererType: Shape.CurveRenderer
            ShapePath {
              objectName: "outline"; fillColor: "#c03030"; strokeColor: "#202020"; strokeWidth: 4
              capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin; fillRule: ShapePath.WindingFill
              PathSvg { path: "M 10 10 L 90 10 L 90 50 L 10 50 Z" }
            }
            ShapePath {
              strokeWidth: 0; startX: 110; startY: 10
              fillGradient: LinearGradient {
                x1: 110; x2: 190
                GradientStop { position: 0; color: "#3030c0" }
                GradientStop { position: 1; color: "#3030c0" }
              }
              PathLine { x: 190; y: 10 } PathLine { x: 190; y: 50 }
              PathLine { x: 110; y: 50 } PathLine { x: 110; y: 10 }
            }
          }
        }
        """
      Then the property "capStyle" of "outline" is 32
      And the property "boundingRect.x" of "shape" is within 0.01 of 8
      And the property "boundingRect.width" of "shape" is within 0.01 of 182
      And the pixel at 50, 30 of the window is "#c03030"
      And the pixel at 150, 30 of the window is "#3030c0"

    @ljos-only
    Scenario: The same shapes, imported from Ljos.Ui
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          width: 200; height: 60
          Shape {
            objectName: "shape"; preferredRendererType: Shape.CurveRenderer
            ShapePath {
              objectName: "outline"; fillColor: "#c03030"; strokeColor: "#202020"; strokeWidth: 4
              capStyle: ShapePath.RoundCap; joinStyle: ShapePath.RoundJoin; fillRule: ShapePath.WindingFill
              PathSvg { path: "M 10 10 L 90 10 L 90 50 L 10 50 Z" }
            }
            ShapePath {
              strokeWidth: 0; startX: 110; startY: 10
              fillGradient: LinearGradient {
                x1: 110; x2: 190
                GradientStop { position: 0; color: "#3030c0" }
                GradientStop { position: 1; color: "#3030c0" }
              }
              PathLine { x: 190; y: 10 } PathLine { x: 190; y: 50 }
              PathLine { x: 110; y: 50 } PathLine { x: 110; y: 10 }
            }
          }
        }
        """
      Then the property "capStyle" of "outline" is 32
      And the property "boundingRect.x" of "shape" is within 0.01 of 8
      And the property "boundingRect.width" of "shape" is within 0.01 of 182
      And the pixel at 50, 30 of the window is "#c03030"
      And the pixel at 150, 30 of the window is "#3030c0"

  Rule: A file that imports QtQuick.Shapes is lowered, its shapes as they are

    @ljos-only @headless
    Scenario: ljos migrate lowers a file of shapes, which draws the same
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Shapes
        Rectangle {
          width: 120; height: 60; color: "white"
          Shape {
            ShapePath {
              fillColor: "#30c030"; strokeColor: "#202020"; strokeWidth: 2
              joinStyle: ShapePath.MiterJoin
              PathSvg { path: "M 10 10 L 110 10 L 60 50 Z" }
            }
          }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (3 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui

        Rectangle {
          width: 120; height: 60; fill: "white"
          Shape {
            ShapePath {
              fillColor: "#30c030"; strokeColor: "#202020"; strokeWidth: 2
              joinStyle: ShapePath.MiterJoin
              PathSvg { path: "M 10 10 L 110 10 L 60 50 Z" }
            }
          }
        }
        """

    # `S.Shape` names a module the file names, which `Ljos.Ui` has no name for.
    @ljos-only @headless
    Scenario: QtQuick.Shapes imported under a name keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Shapes as S
        Item {
          S.Shape { S.ShapePath { PathSvg { path: "M 0 0 L 10 10" } } }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:2:1: it imports `QtQuick.Shapes` under a name
        1 files, 0 lowered, 1 kept
        """
