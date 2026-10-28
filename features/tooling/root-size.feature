# Sources:
#   https://doc.qt.io/qt-6/qtquick-qml-runtime.html
# Verified on Qt 6.11.2 under XWayland in a floating sway: `qml6` opens a
# root `Rectangle { width: 300; height: 200 }` in a 300 by 200 window, one
# with only an implicit size at that size, and a `Window` at its own size.
# A root `Text { text: "Hello there"; font.pixelSize: 60 }` opens at 290 by
# 68 (291 by 68 in `ljos run`, whose font differs by a pixel).
# `ljos run` opens its window as `ljos render` sizes its picture, by
# crates/ljos_shell; that one opens a window, so the picture stands for it.

@ljos-only @headless
Feature: A window as large as the root says
  Without `--size`, `ljos run` and `ljos render` show the root at its own
  width and height, as `qml6` does, so that a file written for a 300 by 200
  window is laid out as its author saw it. A Text is laid out first, so
  that it opens at its implicit size and not at 800 by 600 first. A side
  the root leaves at 0 is 800 by 600's, and `--size` is the size whatever
  the root says.

  Scenario Outline: The picture is as large as the root
    Given the file "Main.qml":
      """qml
      import QtQuick
      <root>
      """
    When the author runs "ljos render . --out shot.png --settle 0 <options>"
    Then the picture "shot.png" is <width> by <height> pixels

    Examples:
      | root                                                     | options         | width | height |
      | Rectangle { width: 300; height: 200 }                    |                 | 300   | 200    |
      | Item { implicitWidth: 120; implicitHeight: 90 }          |                 | 120   | 90     |
      | Text { text: "Hello"; font.pixelSize: 20 }               |                 | 46    | 23     |
      | Rectangle { width: 300 }                                 |                 | 300   | 600    |
      | Item {}                                                  |                 | 800   | 600    |
      | Window { width: 320; height: 240 }                       |                 | 320   | 240    |
      | Rectangle { width: 300; height: 200 }                    | --size 640x480  | 640   | 480    |

  Scenario: The audit sizes a window root to the size it is given
    Given the file "Main.qml":
      """qml
      import QtQuick
      import QtQuick.Controls

      ApplicationWindow {
          width: 1280
          height: 820
          visible: true

          Button {
              anchors.right: parent.right
              anchors.bottom: parent.bottom
              anchors.margins: 8
              text: "Save draft"
          }
      }
      """
    When the author runs "ljos audit Main.qml --matrix current --size 1280x800"
    Then the command exits with status 0
    And the command's output does not mention "layout-offscreen"
