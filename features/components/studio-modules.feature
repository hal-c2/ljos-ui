@ljos-only
Feature: The modules Design Studio's output imports
  A file Design Studio exported imports QtQuick.Studio.Components for its
  vector shapes and QtQuick.Studio.DesignEffects for its effects, whether or
  not the application brings those modules along. Ljós has the ones here
  built in. Qt ships them apart from its own modules, so this is checked
  against what their documentation says they do.

  Rule: An SvgPathItem draws its path

    Background:
      Given the QML:
        """
        import QtQuick
        import QtQuick.Studio.Components
        Rectangle {
          width: 100; height: 100; color: "#ffffff"
          SvgPathItem {
            objectName: "shape"
            anchors.fill: parent
            strokeColor: "transparent"
            fillColor: "#ff0000"
            path: "M 10 10 L 50 10 L 50 50 L 10 50 Z"
          }
        }
        """

    Scenario: The path is filled, and nothing outside it
      Then the pixel at 30, 30 of the window is "#ff0000"
      And the pixel at 70, 70 of the window is "#ffffff"

    Scenario: Its colour follows fillColor
      When the property "fillColor" of "shape" is set to "#0000ff"
      Then the pixel at 30, 30 of the window is "#0000ff"

    Scenario: Its shape follows path
      When the property "path" of "shape" is set to "M 60 60 L 90 60 L 90 90 L 60 90 Z"
      Then the pixel at 30, 30 of the window is "#ffffff"
      And the pixel at 70, 70 of the window is "#ff0000"

  Rule: A DesignEffect blurs what is behind its parent

    Background:
      Given the QML:
        """
        import QtQuick
        import QtQuick.Studio.DesignEffects
        Item {
          width: 200; height: 100
          Item {
            id: scene
            anchors.fill: parent
            Rectangle { x: 0; width: 100; height: 100; color: "#ff0000" }
            Rectangle { x: 100; width: 100; height: 100; color: "#0000ff" }
          }
          Rectangle {
            objectName: "button"
            x: 60; y: 20; width: 80; height: 60
            color: "transparent"
            DesignEffect {
              id: effect
              objectName: "effect"
              backgroundBlurRadius: 40
              backgroundLayer: scene
            }
          }
        }
        """

    Scenario: The edge behind the parent is mixed; outside it, not
      Then the pixel at 40, 50 of the window is "#ff0000"
      And the pixel at 160, 50 of the window is "#0000ff"
      And the pixel at 100, 50 of the window is within 70 of "#800080"

    Scenario: The blur follows its parent
      When the property "x" of "button" is set to 120
      Then the pixel at 100, 50 of the window is "#0000ff"
      And the pixel at 150, 50 of the window is "#0000ff"
      And the pixel at 98, 50 of the window is "#ff0000"

    Scenario: With its blur not visible, what is behind stays sharp
      When the property "backgroundBlurVisible" of "effect" is set to false
      Then the pixel at 98, 50 of the window is "#ff0000"
      And the pixel at 102, 50 of the window is "#0000ff"

    Scenario: Without a background layer it draws nothing
      When the property "backgroundLayer" of "effect" is set to null
      Then the pixel at 98, 50 of the window is "#ff0000"
      And the pixel at 102, 50 of the window is "#0000ff"
