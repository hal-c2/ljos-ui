# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-roundbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: RoundButton
  A round button is a button whose corners are rounded by `radius`: half
  its shorter side unless it is given one, which makes a square button a
  circle.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        width: 400; height: 400
        function takeBack() { given.radius = undefined }
        RoundButton { objectName: "r"; text: "+" }
        RoundButton { objectName: "wide"; x: 100; width: 100; height: 40 }
        RoundButton { id: given; objectName: "set"; x: 250; radius: 5 }
      }
      """

  @qt-parity
  Rule: The Basic style

    Scenario Outline: A round button is a circle that takes the keyboard
      Then the property "<property>" of "r" is <value>

      Examples:
        | property          | value |
        | implicitWidth     | 40    |
        | implicitHeight    | 40    |
        | padding           | 6     |
        | radius            | 20    |
        | background.radius | 20    |
        | focusPolicy       | 11    |
        | flat              | false |
        | highlighted       | false |

  @qt-parity
  @api-QtQuick.Controls.RoundButton.radius
  Rule: The radius

    Scenario: The radius is half the shorter side
      Then the property "radius" of "wide" is 20
      When the property "height" of "wide" is set to 60
      Then the property "radius" of "wide" is 30
      And the property "background.radius" of "wide" is 30

    Scenario: A radius that is given stays
      Then the property "radius" of "set" is 5
      And the property "background.radius" of "set" is 5
      When the property "width" of "set" is set to 100
      Then the property "radius" of "set" is 5

    Scenario: Taking the radius back makes the button round again
      When the method "takeBack" of "root" is called
      Then the property "radius" of "set" is 20
      When the property "height" of "set" is set to 30
      Then the property "radius" of "set" is 15
