# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediadevices.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-audiodevice.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-audiooutput.html#device-prop

# What devices there are is the machine's; a scenario runs where sound
# goes nowhere, which the oracle's machine cannot be made to be.
@ljos-only
Feature: MediaDevices where there is no sound card
  With no device to play on, sound goes nowhere and every player still
  plays, at the pace of its own clock. The devices say so.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtMultimedia
      Item {
        objectName: "root"
        MediaDevices { id: devices; objectName: "devices" }
        AudioOutput { id: output; objectName: "output" }
        property bool same: output.device === devices.defaultAudioOutput
      }
      """

  @api-QtMultimedia.MediaDevices.audioOutputs @api-QtMultimedia.MediaDevices.audioInputs
  @api-QtMultimedia.MediaDevices.videoInputs
  Scenario Outline: No device is listed in <property>
    Then the property "<property>.length" of "devices" is 0

    Examples:
      | property     |
      | audioOutputs |
      | audioInputs  |
      | videoInputs  |

  @api-QtMultimedia.MediaDevices.defaultAudioOutput @api-QtMultimedia.AudioDevice.mode
  Scenario: The default output is a device that goes nowhere
    Then the property "defaultAudioOutput.mode" of "devices" is 0
    And the property "defaultAudioOutput.isDefault" of "devices" is false

  @api-QtMultimedia.AudioOutput.device
  Scenario: An output plays on the default device
    Then the property "same" of "root" is true
