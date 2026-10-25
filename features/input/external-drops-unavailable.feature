# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
# Ljós's own: a host that can bring no drags from other applications says
# so (the desktop shell does on a Wayland compositor with no data device
# manager). Qt has nothing to warn of.

@ljos-only
Feature: A drop target that files from other applications cannot reach says so once
  Where the window system brings no files dragged in from other
  applications, a DropArea that would take them warns once, with the
  reason the host gives, however many such areas there are and whichever
  is known first. An area that takes only other keys is not one of them,
  and where files do come nothing is said.

  Scenario: Two areas that would take files warn once
    Given the window system brings no files dragged in from other applications
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        DropArea { width: 100; height: 100 }
        DropArea { x: 100; width: 100; height: 100; keys: ["text/uri-list"] }
      }
      """
    Then the log has 1 warnings matching "files dragged in from other applications cannot reach a DropArea"

  Scenario: The window system says so after the area is made
    Given the QML:
      """qml
      import QtQuick
      DropArea { width: 100; height: 100 }
      """
    And the window system brings no files dragged in from other applications
    Then the log has 1 warnings matching "files dragged in from other applications cannot reach a DropArea"

  Scenario: An area that takes only other keys is not warned of
    Given the window system brings no files dragged in from other applications
    And the QML:
      """qml
      import QtQuick
      DropArea { width: 100; height: 100; keys: ["text/plain"] }
      """
    Then the log has no warning matching "files dragged in from other applications"

  Scenario: Where files from other applications come nothing is said
    Given the QML:
      """qml
      import QtQuick
      DropArea { width: 100; height: 100 }
      """
    Then the log has no warning matching "files dragged in from other applications"
