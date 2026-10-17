# Sources:
#   https://doc.qt.io/qt-6/qml-qtsensors-sensor.html
#   https://doc.qt.io/qt-6/qml-qtsensors-accelerometer.html
#   https://doc.qt.io/qt-6/qml-qtsensors-accelerometerreading.html
#   https://doc.qt.io/qt-6/qsensor.html#start
# The qml6 on the machines these were written on has no QtSensors module,
# and no step can give Qt a sensor backend that reads what a scenario
# says, so the scenarios run on Ljós only. What a sensor does without a
# backend is what Qt documents: start() fails and the sensor stays
# inactive.

@ljos-only
Feature: An accelerometer, read from the system
  A sensor reads what the system's sensor service measures. Connected to
  the system's sensor, it has a reading from the start, all zero, and its
  coming is the first readingChanged. A sensor that is not active is given
  nothing. A reading that comes sooner than the sensor's dataRate allows
  waits, and only the latest one is shown.

  Scenario: Without a sensor on the system, an accelerometer cannot start
    Given the QML:
      """qml
      import QtQuick
      import QtSensors
      Item {
        Accelerometer {
          id: acc; objectName: "acc"
          property bool started: true
          property bool hasReading: reading !== null
          Component.onCompleted: started = start()
        }
      }
      """
    Then the property "connectedToBackend" of "acc" is false
    And the property "started" of "acc" is false
    And the property "active" of "acc" is false
    And the property "hasReading" of "acc" is false

  Scenario: An inactive accelerometer is given no readings
    Given the system has an accelerometer
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      Accelerometer {
        objectName: "acc"
        property int changes: 0
        property real x: reading ? reading.x : -1
        onReadingChanged: changes++
      }
      """
    When the accelerometer reads 1, 2, 3
    Then the property "connectedToBackend" of "acc" is true
    And the property "active" of "acc" is false
    And the property "changes" of "acc" is 1
    And the property "x" of "acc" is 0

  Scenario: An active accelerometer shows each reading, until it is stopped
    Given the system has an accelerometer
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      Accelerometer {
        objectName: "acc"; active: true
        property int changes: 0
        property real x: reading.x
        property real z: reading.z
        onReadingChanged: changes++
      }
      """
    Then the property "changes" of "acc" is 1
    And the property "x" of "acc" is 0
    When the accelerometer reads 1, 2, 9.8
    Then the property "changes" of "acc" is 2
    And the property "x" of "acc" is 1
    And the property "z" of "acc" is 9.8
    When the accelerometer reads 4, 5, 6
    Then the property "changes" of "acc" is 3
    And the property "x" of "acc" is 4
    When the method "stop" of "acc" is called
    And the accelerometer reads 7, 8, 9
    Then the property "active" of "acc" is false
    And the property "changes" of "acc" is 3
    And the property "x" of "acc" is 4

  Scenario: start() starts an accelerometer the system has
    Given the system has an accelerometer
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      Accelerometer {
        objectName: "acc"
        property bool started: false
        property real y: reading.y
        function go() { started = start() }
      }
      """
    When the method "go" of "acc" is called
    And the accelerometer reads 1, 2, 3
    Then the property "started" of "acc" is true
    And the property "active" of "acc" is true
    And the property "y" of "acc" is 2

  Scenario: Readings that come faster than the dataRate wait, and the latest is shown
    Given the system has an accelerometer
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      Accelerometer {
        objectName: "acc"; active: true; dataRate: 25
        property int changes: 0
        property real x: reading.x
        onReadingChanged: changes++
      }
      """
    When the accelerometer reads 1, 0, 0
    And the accelerometer reads 2, 0, 0
    And the accelerometer reads 3, 0, 0
    Then the property "changes" of "acc" is 2
    And the property "x" of "acc" is 1
    When 40 milliseconds pass
    Then the property "changes" of "acc" is 3
    And the property "x" of "acc" is 3
    When 100 milliseconds pass
    And the accelerometer reads 4, 0, 0
    Then the property "changes" of "acc" is 4
    And the property "x" of "acc" is 4

  Scenario: An accelerometer that skips duplicates does not show the same reading twice
    Given the system has an accelerometer
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      Accelerometer {
        objectName: "acc"; active: true; skipDuplicates: true
        property int changes: 0
        onReadingChanged: changes++
      }
      """
    When the accelerometer reads 1, 2, 3
    And the accelerometer reads 1, 2, 3
    And the accelerometer reads 1, 2, 4
    Then the property "changes" of "acc" is 3

  Scenario: A binding reads the reading as an AccelerometerReading
    Given the system has an accelerometer
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      Accelerometer {
        objectName: "acc"; dataRate: 25; active: true
        readonly property vector3d force: {
          if (!reading)
            return Qt.vector3d(0, 0, 0)
          let r = reading as AccelerometerReading
          return Qt.vector3d(r.x, r.y, r.z)
        }
        property real fx: force.x
        property real fz: force.z
        property bool stamped: (reading as AccelerometerReading).timestamp > 0
      }
      """
    Then the property "fx" of "acc" is 0
    When 1 second passes
    And the accelerometer reads 0.5, 0, -9.5
    Then the property "fx" of "acc" is 0.5
    And the property "fz" of "acc" is -9.5
    And the property "stamped" of "acc" is true

  Scenario Outline: The other motion and environment sensors read what the system measures
    Given the system has <device>
    And the QML:
      """qml
      import QtQuick
      import QtSensors
      <type> {
        objectName: "s"; active: true
        property real value: reading.<property>
      }
      """
    When the <sensor> reads <values>
    Then the property "connectedToBackend" of "s" is true
    And the property "value" of "s" is <value>

    Examples:
      | device                 | sensor               | type               | values       | property         | value  |
      | a gyroscope            | gyroscope            | Gyroscope          | 10, 20, 30   | y                | 20     |
      | a magnetometer         | magnetometer         | Magnetometer       | 1, 2, 3, 0.5 | calibrationLevel | 0.5    |
      | a compass              | compass              | Compass            | 270, 1       | azimuth          | 270    |
      | a pressure sensor      | pressure sensor      | PressureSensor     | 101325, 21   | pressure         | 101325 |
      | an ambient light sensor | ambient light sensor | AmbientLightSensor | 4            | lightLevel       | 4      |
      | an orientation sensor  | orientation sensor   | OrientationSensor  | 5            | orientation      | 5      |
