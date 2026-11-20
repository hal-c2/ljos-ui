# Sources:
#   docs/ARCHITECTURE.md §7.3 (one platform window for the root) and principle 6

# Qt has no way to show another object in a window a root already has.
@ljos-only @desktop
Feature: A window shows the root put in it at once
  A root put in the window of another changes nothing the old root had,
  and the window keeps its size, yet the window shows another picture.
  A `Window` that declares a size of its own is shown at that size: the
  platform's window takes it, and the `Window` is not told the size the
  one it replaced had. A `Window` that asks for no other size than the
  one it replaced is told the size the window has, which may be the size
  the one before it declared. A fallback put back is shown at its own size.
  The new picture shows without another input event: a window that
  waited for one would show the old root until the pointer moved over it.

  Background:
    Given the file "Default.qml":
      """qml
      import QtQuick
      Window { objectName: "default-window"; width: 200; height: 100; visible: true; color: "#ff0000" }
      """
    And the file "Blue.qml":
      """qml
      import QtQuick
      Window { width: 200; height: 100; visible: true; color: "#0000ff" }
      """
    And the file "Unsized.qml":
      """qml
      import QtQuick
      Window {
        objectName: "second-unsized"; visible: true; color: "#ffff00"
        Rectangle { width: parent.width; height: parent.height; color: "#0000ff" }
      }
      """
    And the QML:
      """qml
      import Ljos.Blocks
      ShellLoader { objectName: "loader"; source: "Shell.qml"; fallback: "Default.qml" }
      """

  Scenario: A shell saved while the application runs is drawn in its window
    Then the pixel at 100, 50 of the window is "#ff0000"
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Window { width: 200; height: 100; visible: true; color: "#0000ff" }
      """
    Then the pixel at 100, 50 of the window is "#0000ff"
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Rectangle { width: 200; height: 100; color: "#00ff00" }
      """
    Then the pixel at 100, 50 of the window is "#00ff00"
    When the file "Shell.qml" is removed
    Then the pixel at 100, 50 of the window is "#ff0000"

  # The loader stands for the Window it shows and is in no window's tree,
  # yet it is the application's root: an agent finds it to read and set.
  Scenario: An agent finds the loader that stands for the window
    Then the pixel at 100, 50 of the window is "#ff0000"
    And the property "status" of "loader" is 2
    When the property "source" of "loader" is set to "Blue.qml"
    Then the pixel at 100, 50 of the window is "#0000ff"
    And the property "status" of "loader" is 1

  Scenario: A Window shell that declares another size is shown at that size
    Then the pixel at 100, 50 of the window is "#ff0000"
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Window { objectName: "large-window"; width: 300; height: 200; visible: true; color: "#0000ff" }
      """
    Then the pixel at 250, 150 of the window is "#0000ff"
    And the window's picture is 300 by 200 pixels
    And the property "width" of "large-window" is 300
    And the property "height" of "large-window" is 200

  Scenario: A Window shell that asks for no size after one that declared it keeps that size
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Window { objectName: "large-window"; width: 300; height: 200; visible: true; color: "#0000ff" }
      """
    Then the pixel at 250, 150 of the window is "#0000ff"
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Window {
        objectName: "unsized-after-large"; visible: true; color: "#ffff00"
        Rectangle { width: parent.width; height: parent.height; color: "#0000ff" }
      }
      """
    Then the pixel at 250, 150 of the window is "#0000ff"
    And the window's picture is 300 by 200 pixels
    And the property "width" of "unsized-after-large" is 300
    And the property "height" of "unsized-after-large" is 200

  Scenario: The fallback put back after a Window shell of another size is shown at its own size
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Window { objectName: "large-window"; width: 300; height: 200; visible: true; color: "#0000ff" }
      """
    Then the pixel at 250, 150 of the window is "#0000ff"
    When the file "Shell.qml" is removed
    Then the pixel at 100, 50 of the window is "#ff0000"
    And the window's picture is 200 by 100 pixels
    And the property "width" of "default-window" is 200
    And the property "height" of "default-window" is 100

  Scenario: A Window shell that asks for no other size is told the size its window has
    When the file "Shell.qml" is changed to:
      """qml
      import QtQuick
      Window { objectName: "first-unsized"; visible: true; color: "#00ff00" }
      """
    Then the pixel at 100, 50 of the window is "#00ff00"
    And the property "width" of "first-unsized" is 200
    And the property "height" of "first-unsized" is 100
    When the property "source" of "loader" is set to "Unsized.qml"
    Then the pixel at 190, 90 of the window is "#0000ff"
    And the property "width" of "second-unsized" is 200
    And the property "height" of "second-unsized" is 100
