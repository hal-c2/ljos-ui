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
  the profile, so the page is not asked again. A question whose view went
  unanswered is not an answer: the page is asked again.

  Background:
    Given the web server serves the text file "notify.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      Notification.requestPermission().then(answer => document.title = answer)
      </script></body></html>
      """
    And the web server serves the text file "policy.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      const again = location.search === "?again"
      navigator.geolocation.getCurrentPosition(asked, asked, { timeout: 1000 })
      function asked() {
        Notification.requestPermission().then(() => {
          if (again) document.title = "asked twice"
          else location.search = "again"
        })
      }
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

  # A page from a web server asks the view to read the clipboard after a
  # click. The off-screen Qt oracle never shows this page: its title stays
  # the host and nothing is asked, so it cannot say what the page read.
  @ljos-only @api-QtWebEngine.WebEngineView.permissionRequested @api-QtWebEngine.WebEnginePermission.grant
  Scenario: A page granted the clipboard reads it
    Given the web server serves the text file "paste.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head>
      <body style="margin:0"><div style="height:100px; background:#00ff00" onclick="
        navigator.clipboard.writeText('copied').then(() => navigator.clipboard.readText())
          .then(text => document.title = 'read ' + text, error => document.title = error.name)"></div>
      </body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        focus: true
        property int asked: -1
        url: "$SERVER/paste.html"
        onPermissionRequested: permission => { asked = permission.permissionType; permission.grant() }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 2 seconds pass
    Then the property "asked" of "view" is 9
    And the property "title" of "view" is "read copied"

  @api-QtWebEngine.WebEngineView.permissionRequested @api-QtWebEngine.WebEnginePermission.grant
  Scenario: A page from a web server is asked for its location
    Given the web server serves the text file "where.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      navigator.geolocation.getCurrentPosition(() => document.title = "found",
        error => document.title = "error " + error.code, { timeout: 1000 })
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
        url: "$SERVER/where.html"
        onPermissionRequested: permission => { asked = permission.permissionType; permission.grant() }
      }
      """
    When 3 seconds pass
    Then the property "asked" of "view" is 8

  # A view that goes leaves its question unanswered, not refused: the
  # next view of the same page is asked again, however many went before.
  # Each view goes once it is asked (a moment later, not while it says
  # so), so how long a page takes to ask is not what is counted.
  @api-QtWebEngine.WebEngineView.permissionRequested
  Scenario: A page whose views went without answering is asked by the next one
    Given the web server serves the text file "where.html":
      """
      <!doctype html>
      <html><head><title>waiting</title></head><body><script>
      navigator.geolocation.getCurrentPosition(() => document.title = "found",
        error => document.title = "error " + error.code, { timeout: 5000 })
      </script></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property int asked: 0
        property int views: 1
        property string last: ""
        Component {
          id: page
          WebEngineView {
            url: "$SERVER/where.html"
            property var kept: null
            onPermissionRequested: permission => { root.asked++; kept = permission; if (root.views < 5) next.start() }
            onTitleChanged: root.last = title
          }
        }
        Loader { id: loader; anchors.fill: parent; sourceComponent: page }
        Timer {
          id: next; interval: 1
          onTriggered: { loader.active = false; loader.active = true; root.views++ }
        }
      }
      """
    When 10 seconds pass
    Then the property "asked" of "root" is 5
    And the property "last" of "root" is "waiting"

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

  # A profile whose policy is AskEveryTime keeps no answer: each view is
  # asked, and remembers what it was told for every page it shows. One
  # that stores answers is asked once, by the first view.
  @api-QtWebEngine.WebEngineProfile.persistentPermissionsPolicy
  Scenario Outline: A profile keeps the view's answer as long as its policy says
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property string asked: ""
        function ask(who, permission) { asked += (asked ? " " : "") + who + permission.permissionType; permission.grant() }
        WebEngineProfile {
          id: shared; storageName: "<name>"; offTheRecord: false
          persistentPermissionsPolicy: WebEngineProfile.<policy>
        }
        WebEngineView {
          width: 100; height: 100; profile: shared; url: "$SERVER/policy.html"
          onPermissionRequested: permission => root.ask("a", permission)
          onTitleChanged: if (title === "asked twice") later.active = true
        }
        Loader {
          id: later; x: 100; active: false
          sourceComponent: WebEngineView {
            width: 100; height: 100; profile: shared; url: "$SERVER/policy.html"
            onPermissionRequested: permission => root.ask("b", permission)
          }
        }
      }
      """
    When 8 seconds pass
    Then the property "asked" of "root" is "<asked>"

    Examples:
      | name    | policy        | asked       |
      | Asking  | AskEveryTime  | a8 a7 b8 b7 |
      | Keeping | StoreInMemory | a8 a7       |

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
