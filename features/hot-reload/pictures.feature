# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   https://doc.qt.io/qt-6/qml-qtquick-image.html
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: When a saved picture is shown again
  The pictures an application shows are watched with its QML files. Once
  the files have been quiet for 120 milliseconds, every image that shows a
  saved picture loads it again. Nothing else is made again, so what the
  user did is kept, and no reload of the QML happens.

  Background:
    Given a 40 by 20 image file "logo.png" filled with "#ff0000"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 100; height: 100
        property int presses: 0
        Image { objectName: "logo"; source: "logo.png" }
        BorderImage { objectName: "frame"; y: 50; source: "logo.png" }
      }
      """

  Scenario: A saved picture is shown after the quiet time
    When the image file "logo.png" is saved as a 60 by 30 image filled with "#00ff00"
    And 120 milliseconds pass
    Then the property "implicitWidth" of "logo" is 60
    And the property "implicitHeight" of "logo" is 30
    And the property "status" of "logo" is 1
    And the pixel at 5, 5 of the window is "#00ff00"
    And 0 reloads have happened
    And the log has no warnings

  Scenario: A saved picture waits for the quiet time
    When the image file "logo.png" is saved as a 60 by 30 image filled with "#00ff00"
    And 100 milliseconds pass
    Then the property "implicitWidth" of "logo" is 40
    And the pixel at 5, 5 of the window is "#ff0000"

  Scenario: Every kind of image that shows the picture shows the new one
    When the image file "logo.png" is saved as a 60 by 30 image filled with "#00ff00"
    And 120 milliseconds pass
    Then the property "implicitWidth" of "frame" is 60
    And the pixel at 5, 55 of the window is "#00ff00"

  Scenario: What the user did is kept
    When the property "presses" of "root" is set to 3
    And the image file "logo.png" is saved as a 60 by 30 image filled with "#00ff00"
    And 120 milliseconds pass
    Then the property "presses" of "root" is 3

  Scenario: A picture saved broken is an error of the image, until it is saved whole
    When the image file "logo.png" is saved as a file that is not an image
    And 120 milliseconds pass
    Then the property "status" of "logo" is 3
    And the log has a warning matching "logo.png"
    When the image file "logo.png" is saved as a 60 by 30 image filled with "#00ff00"
    And 120 milliseconds pass
    Then the property "status" of "logo" is 1
    And the property "implicitWidth" of "logo" is 60
