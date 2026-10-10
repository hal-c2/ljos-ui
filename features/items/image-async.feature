# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Image loaded in the background
  With "asynchronous" set, a local picture is read and decoded away from
  the user interface. The Image says "Loading" until it is there.

  Background:
    Given a 40 by 20 image file "red.png" filled with "#ff0000"
    And a 60 by 40 image file "blue.png" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property int changes: 0
        Image {
          objectName: "picture"
          asynchronous: true
          source: "red.png"
          onStatusChanged: root.changes += 1
        }
        Image { objectName: "other" }
      }
      """

  Scenario: The Image is loading first
    Then the property "status" of "picture" is 2
    And the property "progress" of "picture" is 0
    And the property "implicitWidth" of "picture" is 0

  Scenario: The picture arrives a moment later
    When 200 milliseconds pass
    Then the property "status" of "picture" is 1
    And the property "progress" of "picture" is 1
    And the property "implicitWidth" of "picture" is 40
    And the property "implicitHeight" of "picture" is 20
    And the property "paintedWidth" of "picture" is 40
    And the property "sourceSize.width" of "picture" is 40

  Scenario: The status changes twice, to loading and to ready
    Then the property "changes" of "root" is 1
    When 200 milliseconds pass
    Then the property "changes" of "root" is 2

  Scenario: The old size stays while the next picture loads
    When 200 milliseconds pass
    And the property "source" of "picture" is set to "blue.png"
    Then the property "status" of "picture" is 2
    And the property "progress" of "picture" is 0
    And the property "implicitWidth" of "picture" is 40
    And the property "width" of "picture" is 40

  Scenario: The next picture replaces the old one when it has loaded
    When 200 milliseconds pass
    And the property "source" of "picture" is set to "blue.png"
    And 200 milliseconds pass
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 60
    And the property "implicitHeight" of "picture" is 40

  Scenario: A picture that is already loaded is there at once
    When 200 milliseconds pass
    And the property "asynchronous" of "other" is set to true
    And the property "source" of "other" is set to "red.png"
    Then the property "status" of "other" is 1
    And the property "implicitWidth" of "other" is 40

  Scenario: A source cleared while loading leaves nothing
    When the property "source" of "picture" is set to ""
    And 200 milliseconds pass
    Then the property "status" of "picture" is 0
    And the property "implicitWidth" of "picture" is 0

  Scenario: A file that is not there is an error once it was looked for
    When the property "source" of "picture" is set to "missing.png"
    And 200 milliseconds pass
    Then the property "status" of "picture" is 3
    And the property "progress" of "picture" is 0
    And the property "implicitWidth" of "picture" is 0
    And the log has a warning matching "QML Image: Cannot open: file:///.*missing\.png$"

  Scenario: A picture written into the URL itself loads too
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "inline"
        source: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAYAAAAECAIAAAAiZtkUAAAAEElEQVR42mNgaPiPjsgVAgA9HyPp+PSt5QAAAABJRU5ErkJggg=="
      }
      """
    When 200 milliseconds pass
    Then the property "status" of "inline" is 1
    And the property "implicitWidth" of "inline" is 6
    And the property "implicitHeight" of "inline" is 4
