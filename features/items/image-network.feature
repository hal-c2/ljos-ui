# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html
#   https://doc.qt.io/qt-6/qml-qtquick-borderimage.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-button.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Pictures from the network
  An Image, a BorderImage and a button's icon may name a picture on a web
  server. It is always fetched in the background, whatever "asynchronous"
  says: the item says "Loading" until the picture is there, or "Error"
  with a warning when it cannot be had.

  Background:
    Given the web server serves a 30 by 20 image file "red.png" filled with "#ff0000"
    And the web server serves a file "bad.png" that is not an image
    And a 40 by 10 image file "blue.png" filled with "#0000ff"
    And a 30 by 20 image file "red.png" filled with "#ff0000"

  Scenario: A picture from the network is loading at first, also when not asynchronous
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int seen: -1
        Image {
          id: picture; objectName: "picture"
          asynchronous: false
          source: "$SERVER/red.png"
        }
        Component.onCompleted: root.seen = picture.status
      }
      """
    Then the property "seen" of "root" is 2
    And the property "status" of "picture" is 2
    And the property "progress" of "picture" is 0
    And the property "implicitWidth" of "picture" is 0

  Scenario: The picture arrives with its size
    Given the QML:
      """qml
      import QtQuick
      Item {
        Image { objectName: "picture"; source: "$SERVER/red.png" }
      }
      """
    When 1 second passes
    Then the property "status" of "picture" is 1
    And the property "progress" of "picture" is 1
    And the property "implicitWidth" of "picture" is 30
    And the property "implicitHeight" of "picture" is 20
    And the property "sourceSize.width" of "picture" is 30
    And the log has no warnings

  Scenario: Size and progress are in place when the status says ready
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string seen: "?"
        Image {
          source: "$SERVER/red.png"
          onStatusChanged: if (status === Image.Ready)
            root.seen = progress + " " + implicitWidth + " " + sourceSize.height
        }
      }
      """
    When 1 second passes
    Then the property "seen" of "root" is "1 30 20"

  Scenario: A picture the server does not have is an error that says what it replied
    Given the QML:
      """qml
      import QtQuick
      Item {
        Image { objectName: "picture"; source: "$SERVER/missing.png" }
      }
      """
    When 1 second passes
    Then the property "status" of "picture" is 3
    And the property "implicitWidth" of "picture" is 0
    And the log has a warning matching "QML Image: Error transferring http://127\.0\.0\.1:\d+/missing\.png - server replied: Not Found$"

  # Nothing listens on port 9 of the loopback interface.
  Scenario: A server that is not listening is an error that says so
    Given the QML:
      """qml
      import QtQuick
      Item {
        Image { objectName: "picture"; source: "http://127.0.0.1:9/red.png" }
      }
      """
    Then the property "status" of "picture" is 2
    When 1 second passes
    Then the property "status" of "picture" is 3
    And the log has a warning matching "QML Image: Connection refused$"

  Scenario: A file from the network that is not an image is an error
    Given the QML:
      """qml
      import QtQuick
      Item {
        Image { objectName: "picture"; source: "$SERVER/bad.png" }
      }
      """
    When 1 second passes
    Then the property "status" of "picture" is 3
    And the log has a warning matching "QML Image: Error decoding: http://127\.0\.0\.1:\d+/bad\.png: Unsupported image format$"

  Scenario: A source changed while the picture is on its way shows the new one
    Given the QML:
      """qml
      import QtQuick
      Item {
        Image {
          id: picture; objectName: "picture"
          source: "$SERVER/red.png"
        }
        Component.onCompleted: picture.source = "blue.png"
      }
      """
    When 1 second passes
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 40
    And the log has no warnings

  Scenario: A BorderImage from the network loads in the background
    Given the QML:
      """qml
      import QtQuick
      Item {
        BorderImage {
          objectName: "frame"
          border { left: 5; right: 5; top: 5; bottom: 5 }
          source: "$SERVER/red.png"
        }
      }
      """
    Then the property "status" of "frame" is 2
    When 1 second passes
    Then the property "status" of "frame" is 1
    And the property "implicitWidth" of "frame" is 30
    And the property "sourceSize.height" of "frame" is 20

  Scenario: A button's icon from the network sizes the button as a local one does
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        id: root; objectName: "root"
        property bool same: remote.implicitContentWidth === local.implicitContentWidth
        property bool grew: remote.implicitContentWidth > bare.implicitContentWidth
        Button { id: remote; text: "A"; icon.source: "$SERVER/red.png" }
        Button { id: local; text: "A"; icon.source: "red.png" }
        Button { id: bare; text: "A" }
      }
      """
    When 1 second passes
    Then the property "same" of "root" is true
    And the property "grew" of "root" is true

  # Qt fetches again a picture that arrived after every item that asked
  # for it had gone, small (300 by 200) or large. Ljós does for a picture
  # whose bytes outweigh the pixels it would keep of it, and keeps a
  # smaller one as its bytes, so the next item decodes it and fetches
  # nothing: only the large case is the same on both.
  Scenario: A large picture that arrives after its item has gone is fetched again
    Given the web server serves a 2000 by 1500 noisy image file "noise.png"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 400; height: 300
        property int made: 0
        property int again: -1
        Loader {
          id: loader; objectName: "loader"
          sourceComponent: Image {
            objectName: "picture"
            width: 100; height: 100
            fillMode: Image.PreserveAspectFit
            source: "$SERVER/noise.png"
            Component.onCompleted: if (++root.made === 2) root.again = status
          }
        }
      }
      """
    When the property "active" of "loader" is set to false
    And 2 seconds pass
    And the property "active" of "loader" is set to true
    And 2 seconds pass
    Then the property "again" of "root" is 2
    And the property "status" of "picture" is 1
    And the property "sourceSize.width" of "picture" is 2000

