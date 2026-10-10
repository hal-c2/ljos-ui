# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html#easing-prop
#   https://doc.qt.io/qt-6/qeasingcurve.html
#   https://doc.qt.io/qt-6/qml-qtquick-animationcontroller.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Easing curves
  easing.type bends the path between from and to. Each curve is checked at a quarter and at three
  quarters of the way, with from 0 and to 1.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Rectangle { id: box; objectName: "box" }
        NumberAnimation {
          id: anim; objectName: "anim"
          target: box; property: "x"; from: 0; to: 1; duration: 1000
        }
        AnimationController { id: ctl; animation: anim }
        function show(curve, progress) {
          anim.easing.type = Easing[curve]
          ctl.reload()
          ctl.progress = progress
        }
        function curved(points, progress) {
          anim.easing.bezierCurve = points
          show("BezierSpline", progress)
        }
      }
      """

  Scenario Outline: <curve>
    When the method "show" of "root" is called with "<curve>", 0.25
    Then the property "x" of "box" is within 0.00001 of <quarter>
    When the method "show" of "root" is called with "<curve>", 0.75
    Then the property "x" of "box" is within 0.00001 of <three quarters>

    Examples:
      | curve        | quarter   | three quarters |
      | Linear       | 0.25      | 0.75           |
      | InQuad       | 0.0625    | 0.5625         |
      | OutQuad      | 0.4375    | 0.9375         |
      | InOutQuad    | 0.125     | 0.875          |
      | OutInQuad    | 0.375     | 0.625          |
      | InCubic      | 0.015625  | 0.421875       |
      | OutCubic     | 0.578125  | 0.984375       |
      | InOutCubic   | 0.0625    | 0.9375         |
      | OutInCubic   | 0.4375    | 0.5625         |
      | InQuart      | 0.003906  | 0.316406       |
      | OutQuart     | 0.683594  | 0.996094       |
      | InOutQuart   | 0.03125   | 0.96875        |
      | OutInQuart   | 0.46875   | 0.53125        |
      | InQuint      | 0.000977  | 0.237305       |
      | OutQuint     | 0.762695  | 0.999023       |
      | InOutQuint   | 0.015625  | 0.984375       |
      | OutInQuint   | 0.484375  | 0.515625       |
      | InSine       | 0.076120  | 0.617317       |
      | OutSine      | 0.382683  | 0.923880       |
      | InOutSine    | 0.146447  | 0.853553       |
      | OutInSine    | 0.353553  | 0.646447       |
      | InExpo       | 0.004524  | 0.175777       |
      | OutExpo      | 0.824047  | 0.995470       |
      | InOutExpo    | 0.015125  | 0.984867       |
      | OutInExpo    | 0.484859  | 0.515125       |
      | InCirc       | 0.031754  | 0.338562       |
      | OutCirc      | 0.661438  | 0.968246       |
      | InOutCirc    | 0.066987  | 0.933013       |
      | OutInCirc    | 0.433013  | 0.566987       |
      | InElastic    | -0.005524 | 0.088388       |
      | OutElastic   | 0.911612  | 1.005524       |
      | InOutElastic | -0.007813 | 1.007813       |
      | OutInElastic | 0.484375  | 0.468750       |
      | InBack       | -0.064137 | 0.182590       |
      | OutBack      | 0.817410  | 1.064137       |
      | InOutBack    | -0.099682 | 1.099682       |
      | OutInBack    | 0.543849  | 0.456151       |
      | InBounce     | 0.027344  | 0.527344       |
      | OutBounce    | 0.472656  | 0.972656       |
      | InOutBounce  | 0.117188  | 0.882813       |
      | OutInBounce  | 0.265625  | 0.734375       |
      | InCurve      | 0.167157  | 0.750000       |
      | OutCurve     | 0.250000  | 0.832843       |
      | SineCurve    | 0.500000  | 0.500000       |
      | CosineCurve  | 1.000000  | 0.000000       |

  Scenario Outline: easing.<parameter> <value> changes <curve>
    When the property "easing.<parameter>" of "anim" is set to <value>
    And the method "show" of "root" is called with "<curve>", 0.4
    Then the property "x" of "box" is within 0.00001 of <x>

    Examples:
      | curve        | parameter | value | x         |
      | OutElastic   | amplitude | 2     | 1.125     |
      | InElastic    | period    | 0.5   | 0.004828  |
      | OutElastic   | period    | 0.2   | 0.9375    |
      | InBack       | overshoot | 3     | -0.224    |
      | OutBack      | overshoot | 0.5   | 0.856     |
      | InOutBack    | overshoot | 3     | -0.0368   |
      | OutInBack    | overshoot | 3     | 0.544     |
      | InBounce     | amplitude | 2     | 0.455     |
      | OutBounce    | amplitude | 0.5   | 0.955     |
      | InOutBounce  | amplitude | 2     | 0.34875   |
      | OutInBounce  | amplitude | 2     | 0.38      |

  # Qt finds the point on a spline only roughly: its values are off the
  # curve by a few hundred-thousandths, so the two agree less closely here.
  Scenario Outline: A bezier curve with <segments> at <progress>
    When the method "curved" of "root" is called with <points>, <progress>
    Then the property "x" of "box" is within 0.0001 of <x>

    Examples:
      | segments     | points                                                         | progress | x        |
      | one segment  | [0.25, 0.1, 0.25, 1, 1, 1]                                     | 0.25     | 0.408495 |
      | one segment  | [0.25, 0.1, 0.25, 1, 1, 1]                                     | 0.5      | 0.802411 |
      | one segment  | [0.25, 0.1, 0.25, 1, 1, 1]                                     | 0.9      | 0.994316 |
      | two segments | [0.3, 0, 0.2, 0.6, 0.5, 0.5, 0.7, 0.4, 0.9, 1.3, 1, 1]         | 0.25     | 0.2875   |
      | two segments | [0.3, 0, 0.2, 0.6, 0.5, 0.5, 0.7, 0.4, 0.9, 1.3, 1, 1]         | 0.9      | 1.025043 |

  Scenario: BezierSpline without a curve is a straight line
    When the method "show" of "root" is called with "BezierSpline", 0.4
    Then the property "x" of "box" is within 0.00001 of 0.4
