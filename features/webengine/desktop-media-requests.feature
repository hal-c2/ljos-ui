# Sources:
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html
#   https://doc.qt.io/qt-6/qwebenginedesktopmediarequest.html
#   https://doc.qt.io/qt-6/qwebenginepermission.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webenginesettings.html
# Verified on Qt 6.12.0 (qml6, offscreen).
# Every page here is a file beside the QML: nothing is fetched from the
# internet. A run without a web engine leaves these scenarios out and
# says why.

@qt-parity @chromium
Feature: A view chooses what a page that shares the screen is shown
  A page that asks to share the screen (getDisplayMedia) makes the
  WebEngineView emit desktopMediaRequested with a
  WebEngineDesktopMediaRequest. Its screensModel and windowsModel list
  what could be shared; selectScreen() or selectWindow() chooses, and
  cancel() refuses. A choice is then a permission like any other, asked
  for with permissionRequested, and refused while the view's
  settings.screenCaptureEnabled is off. A view with no handler refuses
  the page at once.

  Background:
    Given the text file "share.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head>
      <body style="margin:0; height:100px" onclick="navigator.mediaDevices.getDisplayMedia({ video: true })
        .then(() => document.title = 'shared', error => document.title = 'refused')"></body></html>
      """

  @api-QtWebEngine.WebEngineView.desktopMediaRequested
  Scenario: A page nobody answers is refused at once
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        url: Qt.resolvedUrl("share.html")
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 1 second passes
    Then the property "title" of "view" is "refused"

  @api-QtWebEngine.WebEngineDesktopMediaRequest.cancel
  Scenario: A request cancelled refuses the page
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        url: Qt.resolvedUrl("share.html")
        onDesktopMediaRequested: request => request.cancel()
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 1 second passes
    Then the property "title" of "view" is "refused"

  @api-QtWebEngine.WebEngineDesktopMediaRequest.screensModel
  Scenario: The screens a page can be shown are listed by name
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property var request: null
        property string names: ""
        url: Qt.resolvedUrl("share.html")
        onDesktopMediaRequested: r => request = r
        Repeater {
          model: view.request ? view.request.screensModel : null
          Item { Component.onCompleted: view.names += display }
        }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 1 second passes
    Then the property "names" of "view" is "Entire screen"
    And the property "title" of "view" is "waiting"

  @api-QtWebEngine.WebEngineDesktopMediaRequest.selectScreen @api-QtWebEngine.WebEngineSettings.screenCaptureEnabled
  Scenario: A screen chosen while screen capture is off refuses the page
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property int asked: 0
        url: Qt.resolvedUrl("share.html")
        onDesktopMediaRequested: request => request.selectScreen(request.screensModel.index(0, 0))
        onPermissionRequested: permission => asked++
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 1 second passes
    Then the property "title" of "view" is "refused"
    And the property "asked" of "view" is 0

  @api-QtWebEngine.WebEngineDesktopMediaRequest.selectScreen @api-QtWebEngine.WebEngineSettings.screenCaptureEnabled
  Scenario: A screen chosen asks for permission to capture it
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        settings.screenCaptureEnabled: true
        url: Qt.resolvedUrl("share.html")
        onDesktopMediaRequested: request => request.selectScreen(request.screensModel.index(0, 0))
        onPermissionRequested: permission => seen = permission.permissionType + " " + permission.state
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 1 second passes
    Then the property "seen" of "view" is "4 1"
    And the property "title" of "view" is "waiting"
