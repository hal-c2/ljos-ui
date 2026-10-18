# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html (title)
#   https://doc.qt.io/qt-6/qtquick-accessibility.html
# Read from qml6 (Qt 6) through AT-SPI: the window is a frame named by its
# title, and a window without a title has no name. The Qt oracle cannot read
# the accessibility tree through QML alone, so these scenarios run on Ljós only.

@ljos-only @a11y
Feature: A window is named by its title
  A screen reader says which window it is in by the window's name, which is
  the title in its title bar.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "w"
        width: 300; height: 200
        title: "Thermostat"
        Button { text: "Heat" }
      }
      """

  Scenario: The window's name is its title
    Then the "button" named "Heat" is inside the "window" named "Thermostat"

  Scenario: A new title is a new name
    When the property "title" of "w" is set to "Rooms"
    Then the "button" named "Heat" is inside the "window" named "Rooms"

  Scenario: A window without a title has no name
    When the property "title" of "w" is set to ""
    Then the "button" named "Heat" is inside the "window" named ""
