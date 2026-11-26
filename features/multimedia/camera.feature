# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-camera.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-capturesession.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-cameradevice.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediadevices.html#videoInputs-prop
#   https://doc.qt.io/qt-6/permissions.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Camera and CaptureSession
  A Camera captures pictures from one of the system's cameras, the
  default one unless cameraDevice names another. A CaptureSession joins
  it to a VideoOutput, which shows what it captures: a viewfinder.
  start() and stop() set active. A camera that cannot capture says why
  in error and errorString; one lost while it captures stops, and says
  so through errorOccurred as well.

  @qt-parity
  @api-QtMultimedia.Camera.error @api-QtMultimedia.Camera.errorString
  @api-QtMultimedia.Camera.start @api-QtMultimedia.Camera.active
  @api-QtMultimedia.CaptureSession.camera @api-QtMultimedia.CaptureSession.videoOutput
  Scenario: Without a camera, a Camera says so and does not start
    Given the QML:
      """qml
      import QtQuick
      import QtMultimedia
      Item {
        objectName: "root"
        property string out: ""
        property int errors: 0
        Camera { id: camera; onErrorOccurred: errors++ }
        CaptureSession { id: session; camera: camera; videoOutput: output }
        VideoOutput { id: output }
        function check() {
          camera.start()
          out = [camera.active, camera.error === Camera.CameraError, camera.errorString,
                 session.camera === camera, session.videoOutput === output, errors].join(" ")
        }
      }
      """
    When the method "check" of "root" is called
    Then the property "out" of "root" is "false true No camera detected true true 0"

  # A camera the scenario gives the system: the oracle's machine has none.
  @ljos-only
  Rule: A camera the system has

    Background:
      Given the system has a camera named "Back camera"
      And the file "Viewfinder.qml":
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          id: root; objectName: "root"; width: 200; height: 200
          property alias camera: camera
          property int frames: 0
          property string log: ""
          property string errorText: ""
          Camera {
            id: camera; objectName: "camera"
            onActiveChanged: root.log += "active " + active + ";"
            onErrorOccurred: (error, errorString) => root.errorText = error + " " + errorString
          }
          CaptureSession { camera: camera; videoOutput: output }
          VideoOutput { id: output; objectName: "output"; anchors.fill: parent }
          Connections { target: output.videoSink; function onVideoFrameChanged() { root.frames++ } }
          readonly property string size: output.videoSink.videoSize.width + "x" + output.videoSink.videoSize.height
        }
        """

    @api-QtMultimedia.MediaDevices.videoInputs @api-QtMultimedia.MediaDevices.defaultVideoInput
    @api-QtMultimedia.Camera.cameraDevice
    Scenario: The system's camera is listed, and a Camera uses it
      Given the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Viewfinder {
          MediaDevices { id: devices }
          property string out: [devices.videoInputs.length, devices.defaultVideoInput.description,
                                devices.defaultVideoInput.isDefault, camera.cameraDevice.description,
                                camera.error === Camera.NoError, JSON.stringify(camera.errorString)].join(" ")
        }
        """
      Then the property "out" of "root" is "1 Back camera true Back camera true \"\""

    @api-QtMultimedia.Camera.stop
    Scenario: start() and stop() set active
      Given the QML:
        """qml
        import QtQuick
        Viewfinder { }
        """
      When the method "start" of "camera" is called
      And the method "stop" of "camera" is called
      Then the property "log" of "root" is "active true;active false;"

    Scenario: What a started camera captures is shown in the output
      Given the QML:
        """qml
        import QtQuick
        Viewfinder { Component.onCompleted: camera.start() }
        """
      When the camera "Back camera" captures a 64 by 48 picture
      And the camera "Back camera" captures a 64 by 48 picture
      And 10 milliseconds pass
      Then the property "size" of "root" is "64x48"
      And the property "frames" of "root" is at least 1

    Scenario: A camera that is not started shows nothing
      Given the QML:
        """qml
        import QtQuick
        Viewfinder { }
        """
      When the camera "Back camera" captures a 64 by 48 picture
      And 10 milliseconds pass
      Then the property "size" of "root" is "-1x-1"
      And the property "frames" of "root" is 0

    Scenario: A Camera with active set starts once it is made
      Given the QML:
        """qml
        import QtQuick
        Viewfinder { camera.active: true }
        """
      When the camera "Back camera" captures a 32 by 32 picture
      And 10 milliseconds pass
      Then the property "size" of "root" is "32x32"

    @api-QtMultimedia.Camera.errorOccurred
    Scenario: A camera lost while it captures stops and says why
      Given the QML:
        """qml
        import QtQuick
        Viewfinder { Component.onCompleted: camera.start() }
        """
      When the camera "Back camera" is disconnected
      And 10 milliseconds pass
      Then the property "log" of "root" is "active true;active false;"
      And the property "errorText" of "root" is "1 The camera was disconnected"
      And the property "errorString" of "camera" is "The camera was disconnected"

    # On a system that asks, a camera starts only once the user grants
    # its permission (doc.qt.io/qt-6/permissions.html).
    Scenario: Without the camera permission, a Camera does not start
      Given the system asks the user before it grants a permission
      And the QML:
        """qml
        import QtQuick
        Viewfinder { }
        """
      When the method "start" of "camera" is called
      Then the property "active" of "camera" is false
      And the property "errorString" of "camera" is "The camera permission is not granted"
      And the property "errorText" of "root" is "1 The camera permission is not granted"
