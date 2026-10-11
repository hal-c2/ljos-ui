# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#property-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#target-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#targetScaleMultiplier-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#targetTransformAroundCursor-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: WheelHandler changes a property of its target
  The handler adds what the wheel turned to the property it names, on its
  parent or on another item. A rotation and a scale happen around the
  pointer unless the handler is told otherwise.

  Rule: The property and the target

    Scenario: The named property of the parent grows by the degrees turned
      Given the file "tst_property.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: box
            x: 40; width: 100; height: 100
            WheelHandler { property: "x" }
          }
          TestCase {
            name: "Property"
            when: windowShown
            function test_property() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.x, 55)
              mouseWheel(box, 10, 10, 0, -240)
              compare(box.x, 25)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Property::test_property()
        """

    Scenario: A property of the item's own can be the one that changes
      Given the file "tst_own.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property real level: 1.5
          property int steps: 4
          WheelHandler { property: "level" }
          WheelHandler { property: "steps"; rotationScale: 1 / 15 }
          TestCase {
            name: "Own"
            when: windowShown
            function test_own() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.level, 16.5)
              compare(box.steps, 5)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Own::test_own()
        """

    Scenario: Another item can be the target
      Given the file "tst_other.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          Item { id: other; opacity: 0.5 }
          WheelHandler { id: handler; target: other; property: "opacity"; rotationScale: 0.01 }
          TestCase {
            name: "Other"
            when: windowShown
            function test_other() {
              mouseWheel(box, 10, 10, 0, 120)
              fuzzyCompare(other.opacity, 0.65, 0.000001)
              compare(box.opacity, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Other::test_other()
        """

    Scenario: A handler with no target only adds up its rotation
      Given the file "tst_none.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; target: null; property: "x" }
          TestCase {
            name: "None"
            when: windowShown
            function test_none() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, 15)
              compare(box.x, 0)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : None::test_none()
        """

  Rule: A rotation and a scale happen around the pointer

    Scenario: The target rotates around the point under the pointer
      Given the file "tst_rotate.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 300; height: 200
          Item {
            id: box
            x: 110; y: 10; width: 80; height: 80
            WheelHandler { property: "rotation" }
          }
          TestCase {
            name: "Rotate"
            when: windowShown
            function test_rotate() {
              mouseWheel(box.parent, 130, 30, 0, 120)
              compare(box.rotation, 15)
              fuzzyCompare(box.x, 104.142, 0.001)
              fuzzyCompare(box.y, 14.495, 0.001)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Rotate::test_rotate()
        """

    Scenario: The target rotates around its own origin when it is told to
      Given the file "tst_origin.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 300; height: 200
          Item {
            id: box
            x: 110; y: 10; width: 80; height: 80
            WheelHandler { property: "rotation"; targetTransformAroundCursor: false }
          }
          TestCase {
            name: "Origin"
            when: windowShown
            function test_origin() {
              mouseWheel(box.parent, 130, 30, 0, 120)
              compare([box.rotation, box.x, box.y], [15, 110, 10])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Origin::test_origin()
        """

    Scenario: A notch multiplies the scale by the cube root of two, around the pointer
      Given the file "tst_zoom.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 300; height: 200
          Item {
            id: box
            x: 210; y: 10; width: 80; height: 80
            WheelHandler { property: "scale" }
          }
          TestCase {
            name: "Zoom"
            when: windowShown
            function test_zoom() {
              mouseWheel(box.parent, 230, 30, 0, 120)
              fuzzyCompare(box.scale, 1.259921, 0.00001)
              fuzzyCompare(box.x, 215.198, 0.001)
              fuzzyCompare(box.y, 15.198, 0.001)
              mouseWheel(box.parent, 230, 30, 0, -240)
              fuzzyCompare(box.scale, 0.793701, 0.00001)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Zoom::test_zoom()
        """

    Scenario: The scale multiplier says how much a notch zooms
      Given the file "tst_multiplier.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 300; height: 200
          Item {
            id: box
            width: 80; height: 80
            WheelHandler {
              property: "scale"
              targetScaleMultiplier: 2
              targetTransformAroundCursor: false
            }
          }
          TestCase {
            name: "Multiplier"
            when: windowShown
            function test_multiplier() {
              mouseWheel(box, 40, 40, 0, 240)
              compare([box.scale, box.x, box.y], [4, 0, 0])
              mouseWheel(box, 40, 40, 0, -360)
              compare(box.scale, 0.5)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Multiplier::test_multiplier()
        """
