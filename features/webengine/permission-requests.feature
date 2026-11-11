# Sources:
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html
#   https://doc.qt.io/qt-6/qwebenginepermission.html
#   https://doc.qt.io/qt-6/qwebengineprofile.html
# Verified on Qt 6.12.0 (qml6, offscreen).
# Every page here is a file beside the QML or on the scenario's own web
# server (loopback): nothing is fetched from the internet. A run without a web engine
# leaves these scenarios out and says why.

@qt-parity @chromium
Feature: A view answers what a page asks permission for
  A page that wants to show notifications, know where it is, read the
  clipboard or use a camera asks first. The WebEngineView emits
  permissionRequested with a WebEnginePermission: who asks (origin), for
  what (permissionType) and how it stands (state, isValid). grant() or
  deny() answers the page, then or later; a request nobody answers keeps
  the page waiting. An answer to what a page keeps being allowed
  (notifications, its location, the clipboard, local fonts) is kept for
  the profile, so the page is not asked again.

  Background:
    Given the web server serves the text file "notify.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      Notification.requestPermission().then(answer => document.title = answer)
      </script></body></html>
      """

  @api-QtWebEngine.WebEngineView.permissionRequested @api-QtWebEngine.WebEnginePermission.grant @api-QtWebEngine.WebEnginePermission.deny @api-QtWebEngine.WebEnginePermission.permissionType @api-QtWebEngine.WebEnginePermission.origin @api-QtWebEngine.WebEnginePermission.isValid
  Scenario Outline: A page asking to show notifications hears the view's answer
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: "$SERVER/notify.html"
        onPermissionRequested: permission => {
          seen = permission.permissionType + " " + permission.state + " " + permission.isValid
          permission.<answer>()
          seen += " " + permission.state }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "7 1 true <state>"
    And the property "title" of "view" is "<page hears>"

    Examples:
      | answer | state | page hears |
      | grant  | 2     | granted    |
      | deny   | 3     | denied     |

  @api-QtWebEngine.WebEnginePermission.deny
  Scenario: A page refused its location hears it was refused
    Given the text file "where.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      navigator.geolocation.getCurrentPosition(() => document.title = "found",
        error => document.title = "error " + error.code)
      </script></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property int asked: -1
        url: Qt.resolvedUrl("where.html")
        onPermissionRequested: permission => { asked = permission.permissionType; permission.deny() }
      }
      """
    When 2 seconds pass
    Then the property "asked" of "view" is 8
    And the property "title" of "view" is "error 1"

  @api-QtWebEngine.WebEngineView.permissionRequested
  Scenario: A request nobody answers keeps the page waiting
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        url: "$SERVER/notify.html"
      }
      """
    When 3 seconds pass
    Then the property "title" of "view" is "waiting"

  @api-QtWebEngine.WebEnginePermission.grant
  Scenario: A request kept and answered later answers the page then
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property var kept: null
        url: "$SERVER/notify.html"
        onPermissionRequested: permission => kept = permission
        Timer { interval: 1500; running: true; onTriggered: view.kept.grant() }
      }
      """
    When 1 second passes
    Then the property "title" of "view" is "waiting"
    When 2 seconds pass
    Then the property "title" of "view" is "granted"

  @api-QtWebEngine.WebEngineView.permissionRequested
  Scenario: A page granted notifications is not asked again
    Given the web server serves the text file "twice.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      Notification.requestPermission()
        .then(first => Notification.requestPermission().then(second => document.title = first + " " + second))
      </script></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property int asked: 0
        url: "$SERVER/twice.html"
        onPermissionRequested: permission => { asked++; permission.grant() }
      }
      """
    When 2 seconds pass
    Then the property "title" of "view" is "granted granted"
    And the property "asked" of "view" is 1

  @api-QtWebEngine.WebEnginePermission.origin
  Scenario: A page from a web server asks as its origin
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool fromServer: false
        url: "$SERVER/notify.html"
        onPermissionRequested: permission => { fromServer = permission.origin.toString() === "$SERVER/"
          permission.grant() }
      }
      """
    When 2 seconds pass
    Then the property "fromServer" of "view" is true
    And the property "title" of "view" is "granted"

  @api-QtWebEngine.WebEnginePermission.origin
  Scenario: A page from a file asks as the files' origin
    Given the text file "local.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      Notification.requestPermission().then(answer => document.title = answer)
      </script></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string origin: ""
        url: Qt.resolvedUrl("local.html")
        onPermissionRequested: permission => { origin = permission.origin.toString(); permission.deny() }
      }
      """
    When 2 seconds pass
    Then the property "origin" of "view" is "file:///"
    And the property "title" of "view" is "denied"

  # Qt lets a page from a file show notifications once the view grants
  # them. Chromium as CEF builds it never lets a file's page show them, so
  # such a page hears that it was refused whatever the view answers.
  @qt-diverges @api-QtWebEngine.WebEnginePermission.grant
  Scenario: A page from a file granted notifications still hears it was refused
    Given the text file "local.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      Notification.requestPermission().then(answer => document.title = answer)
      </script></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property int asked: 0
        url: Qt.resolvedUrl("local.html")
        onPermissionRequested: permission => { asked++; permission.grant() }
      }
      """
    When 2 seconds pass
    Then the property "asked" of "view" is 1
    And the property "title" of "view" is "denied"
