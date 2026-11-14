# Sources:
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineloadinginforequest.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webenginescript.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webenginescriptcollection.html
#   https://doc.qt.io/qt-6/qtwebengine-index.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Every page here is a file beside the QML or on the scenario's own web
# server: nothing is fetched from the internet. A run without a web engine
# leaves these scenarios out and says why.

@qt-parity @chromium
Feature: A web page in a WebEngineView
  A WebEngineView shows a live web page as one more item of the scene: it
  is laid out, clipped, moved and stacked like any other. It says where it
  is (url), what the page is called (title), how far a load has come
  (loading, loadProgress) and how each load ended (loadingChanged), keeps
  a history to go back and forward in, and runs scripts in the page.

  Background:
    Given the text file "page.html":
      """
      <!doctype html>
      <html><head><title>Fixture Page</title></head>
      <body style="margin:0; background:#00ff00"></body></html>
      """
    And the text file "second.html":
      """
      <!doctype html>
      <html><head><title>Second Page</title></head>
      <body style="margin:0; background:#0000ff"></body></html>
      """

  @api-QtWebEngine.WebEngineView.loadingChanged @api-QtWebEngine.WebEngineView.title @api-QtWebEngine.WebEngineView.loadProgress
  Scenario: A page loads, says so, and takes its title
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => seen += (seen ? " " : "") + info.status + ":" + info.errorCode
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "0:0 2:200"
    And the property "title" of "view" is "Fixture Page"
    And the property "loading" of "view" is false
    And the property "loadProgress" of "view" is 100

  @api-QtWebEngine.WebEngineView.url
  Scenario: A page is drawn where the view is
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Rectangle {
        width: 200; height: 100
        color: "#ff0000"
        WebEngineView {
          x: 100; width: 100; height: 100
          url: Qt.resolvedUrl("page.html")
        }
      }
      """
    When 2 seconds pass
    Then the pixel at 50, 50 of the window is "#ff0000"
    And the pixel at 150, 50 of the window is "#00ff00"

  @api-QtQuick.Item.grabToImage
  Scenario: A picture of the view holds the page it shows
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Rectangle {
        id: root; objectName: "root"
        width: 200; height: 100
        color: "#ff0000"
        property var asked: "not yet"
        function grab() { asked = view.grabToImage(r => shot.source = r.url) }
        WebEngineView {
          id: view
          width: 100; height: 100
          url: Qt.resolvedUrl("page.html")
        }
        Image { id: shot; x: 100 }
      }
      """
    When 2 seconds pass
    And the method "grab" of "root" is called
    And 1 second passes
    Then the property "asked" of "root" is true
    And the pixel at 150, 50 of the window is "#00ff00"

  @api-QtWebEngine.WebEngineView.url
  Scenario: A page that changes one part at a time is drawn whole after each change
    Given the text file "halves.html":
      """
      <!doctype html>
      <html><body style="margin:0; display:flex">
      <div id="a" style="width:100px; height:100px; background:#000000"></div>
      <div id="b" style="width:100px; height:100px; background:#000000"></div>
      </body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        url: Qt.resolvedUrl("halves.html")
        function paint(id, color) {
          runJavaScript("document.getElementById('" + id + "').style.background = '" + color + "'")
        }
      }
      """
    When 2 seconds pass
    And the method "paint" of "view" is called with "a", "#ff0000"
    And 1 second passes
    Then the pixel at 50, 50 of the window is "#ff0000"
    And the pixel at 150, 50 of the window is "#000000"
    When the method "paint" of "view" is called with "b", "#0000ff"
    And 1 second passes
    Then the pixel at 50, 50 of the window is "#ff0000"
    And the pixel at 150, 50 of the window is "#0000ff"
    When the method "paint" of "view" is called with "a", "#00ff00"
    And 1 second passes
    Then the pixel at 50, 50 of the window is "#00ff00"
    And the pixel at 150, 50 of the window is "#0000ff"
    When the method "paint" of "view" is called with "b", "#ffff00"
    And 1 second passes
    Then the pixel at 50, 50 of the window is "#00ff00"
    And the pixel at 150, 50 of the window is "#ffff00"

  @api-QtWebEngine.WebEngineView.loadingChanged
  Scenario: A file that is not there fails to load and says why
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("missing.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadFailedStatus)
          seen = info.errorCode + " " + info.errorString + " " + info.errorDomain }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "-6 net::ERR_FILE_NOT_FOUND 1"
    And the property "loading" of "view" is false

  @api-QtWebEngine.WebEngineView.loadingChanged
  Scenario: The first page in a window is loaded once and succeeds
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Window {
        width: 200; height: 100; visible: true
        WebEngineView {
          id: view; objectName: "view"
          anchors.fill: parent
          property string seen: ""
          url: Qt.resolvedUrl("page.html")
          onLoadingChanged: info => seen += info.status
        }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "02"

  @api-QtWebEngine.WebEngineView.url
  Scenario: A page the view was sent to that fails becomes the view's url
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool there: url.toString().endsWith("/missing.html")
        url: Qt.resolvedUrl("page.html")
        function follow() { runJavaScript("location.href = 'missing.html'") }
      }
      """
    When 2 seconds pass
    Then the property "there" of "view" is false
    When the method "follow" of "view" is called
    And 2 seconds pass
    Then the property "there" of "view" is true
    And the property "loading" of "view" is false

  @api-QtWebEngine.WebEngineView.newWindowRequested @api-QtWebEngine.WebEngineNewWindowRequest.destination
  Scenario: A page asking for a new window says what kind of window it asks for
    Given the text file "links.html":
      """
      <!doctype html>
      <html><body style="margin:0">
      <a href="second.html" target="_blank"
         style="display:block; height:50px; background:#ff0000"></a>
      <div onclick="window.open('second.html', '_blank', 'width=200,height=100')"
           style="height:50px; background:#0000ff"></div>
      </body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        property bool stayed: url.toString().endsWith("/links.html")
        url: Qt.resolvedUrl("links.html")
        onNewWindowRequested: request => seen += request.destination
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 25
    And the user releases the pointer at 100, 25
    And 1 second passes
    And the user presses the pointer at 100, 75
    And the user releases the pointer at 100, 75
    And 1 second passes
    And the user clicks at 100, 25 holding "Ctrl"
    And 1 second passes
    And the user clicks at 100, 25 holding "Shift"
    And 1 second passes
    Then the property "seen" of "view" is "1230"
    And the property "stayed" of "view" is true

  @api-QtWebEngine.WebEngineView.triggerWebAction @api-QtWebEngine.WebEngineNewWindowRequest.requestedUrl
  Scenario: Viewing a page's source asks for a new window showing it
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus)
          triggerWebAction(WebEngineView.ViewSource) }
        onNewWindowRequested: request => seen = WebEngineView.ViewSource + " "
          + (request.requestedUrl.toString() === "view-source:" + url) + " "
          + request.destination + " " + request.userInitiated
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "32 true 1 true"
    And the property "title" of "view" is "Fixture Page"

  # As in Qt, the view asks even with no page: the request is the
  # application's to turn down.
  @api-QtWebEngine.WebEngineView.triggerWebAction @api-QtWebEngine.WebEngineNewWindowRequest.requestedUrl
  Scenario: Viewing the source of a view showing no page asks for an empty source
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: "nothing"
        Timer { interval: 500; running: true; onTriggered: view.triggerWebAction(WebEngineView.ViewSource) }
        onNewWindowRequested: request => seen = request.requestedUrl.toString()
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "view-source:"

  # A failed load is reported once and stays failed: the view does not
  # try the page again by itself, and a test waits for nothing more (a
  # wait given up on says so in the log). With error pages off, the view
  # stays titled as it was before the load.
  @api-QtWebEngine.WebEngineView.loadingChanged @api-QtWebEngine.WebEngineSettings.errorPageEnabled
  Scenario: A page whose server refuses the connection fails to load and says why
    Given the web server serves the text file "served.html":
      """
      <!doctype html><html><head><title>Served Page</title></head><body></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        property string failure: ""
        settings.errorPageEnabled: false
        url: "$SERVER/served.html"
        function goAway() { url = "$SERVER/refused" }
        onLoadingChanged: info => { seen += (seen ? " " : "") + info.status
          if (info.status === WebEngineView.LoadFailedStatus)
            failure = info.errorCode + " " + info.errorString + " " + info.errorDomain + " [" + title + "]" }
      }
      """
    When 2 seconds pass
    And the method "goAway" of "view" is called
    And 2 seconds pass
    And 2 seconds pass
    And 2 seconds pass
    Then the property "failure" of "view" is "-102 net::ERR_CONNECTION_REFUSED 2 [Served Page]"
    And the property "seen" of "view" is "0 2 0 3"
    And the property "title" of "view" is "Served Page"
    And the property "loading" of "view" is false
    And the log has no warning matching "gave up waiting"

  @api-QtWebEngine.WebEngineView.loadingChanged @api-QtWebEngine.WebEngineSettings.errorPageEnabled
  Scenario: A page that redirects to itself fails to load and says why
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        property string failure: ""
        settings.errorPageEnabled: false
        url: "$SERVER/loop"
        onLoadingChanged: info => { seen += (seen ? " " : "") + info.status
          if (info.status === WebEngineView.LoadFailedStatus)
            failure = info.errorCode + " " + info.errorString + " " + info.errorDomain + " [" + title + "]" }
      }
      """
    When 2 seconds pass
    And 2 seconds pass
    And 2 seconds pass
    Then the property "failure" of "view" is "-310 net::ERR_TOO_MANY_REDIRECTS 4 []"
    And the property "seen" of "view" is "0 3"
    And the property "title" of "view" is ""
    And the property "loading" of "view" is false
    And the log has no warning matching "gave up waiting"

  Scenario: A port a browser does not visit fails to load and says why
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: "http://127.0.0.1:1/page.html"
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadFailedStatus)
          seen = info.errorCode + " " + info.errorString + " " + info.errorDomain }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "-312 net::ERR_UNSAFE_PORT 4"

  Scenario: A page from a web server loads as one from a file does
    Given the web server serves the text file "served.html":
      """
      <!doctype html><html><head><title>Served Page</title></head><body></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: "$SERVER/served.html"
        onLoadingChanged: info => seen += (seen ? " " : "") + info.status + ":" + info.errorCode + ":" + info.errorDomain
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "0:0:0 2:200:7"
    And the property "title" of "view" is "Served Page"

  # The profile is made for the view: its first page loads as soon as the
  # profile can hold what the page keeps, not before, and does not fail.
  @api-QtWebEngine.WebEngineView.profile @api-QtWebEngine.WebEngineProfile.offTheRecord
  Scenario: The first page of a profile kept on disk loads
    Given the web server serves the text file "served.html":
      """
      <!doctype html><html><head><title>Served Page</title></head><body></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        profile: WebEngineProfile { storageName: "FirstPage"; offTheRecord: false }
        url: "$SERVER/served.html"
        onLoadingChanged: info => seen += (seen ? " " : "") + info.status + ":" + info.errorCode + ":" + info.errorDomain
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "0:0:0 2:200:7"
    And the property "title" of "view" is "Served Page"

  Scenario: A page the server does not have fails with the status it replied
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: "$SERVER/status/404/Not%20Found"
        onLoadingChanged: info => { if (info.status !== WebEngineView.LoadStartedStatus)
          seen = info.status + ":" + info.errorCode + ":" + info.errorDomain }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "3:404:7"

  @api-QtWebEngine.WebEngineView.stop
  Scenario: A load stopped as it starts says it was stopped
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => {
          if (info.status === WebEngineView.LoadStartedStatus) view.stop()
          else seen = info.status + " " + info.errorCode + " " + info.errorString + " " + info.errorDomain
        }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "1 -3 net::ERR_ABORTED 1"
    And the log has no warning matching "gave up waiting"

  # A load that stops shows what it got that far; time passing waits for
  # that picture, as it does for a page that loaded, and for no more.
  # How long time passing waits is Ljós's own: Qt's offscreen view is not
  # drawn the same way.
  @ljos-only @api-QtWebEngine.WebEngineView.stop
  Scenario: A page stopped while it loads is drawn as far as it got
    Given the web server serves the text file "stopping.html":
      """
      <!doctype html>
      <html><head><title>Stopping</title></head>
      <body style="margin:0; background:#00ff00">
      <script>window.stop()</script>
      </body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: "$SERVER/stopping.html"
        onLoadingChanged: info => seen += (seen ? " " : "") + info.status
      }
      """
    When 1 millisecond passes
    Then the property "seen" of "view" is "0 1"
    And the pixel at 50, 50 of the window is "#00ff00"
    And the log has no warning matching "gave up waiting"

  # A load another replaces stopped: time passing waits for the new page's
  # picture, not for the one that never came. The first page is never
  # answered, so it is still loading when the view goes elsewhere.
  @ljos-only @api-QtWebEngine.WebEngineView.loadingChanged
  Scenario: A load replaced while it loads says it stopped and the next page is drawn
    Given the web server serves the text file "green.html":
      """
      <!doctype html>
      <html><head><title>Green</title></head>
      <body style="margin:0; background:#00ff00"></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: "$SERVER/held"
        onLoadingChanged: info => seen += (seen ? " " : "") + info.status
        onLoadProgressChanged: if (loadProgress > 0 && url.toString().endsWith("/held"))
          url = "$SERVER/green.html"
      }
      """
    When 1 millisecond passes
    Then the property "seen" of "view" is "0 1 0 2"
    And the property "title" of "view" is "Green"
    And the pixel at 50, 50 of the window is "#00ff00"
    And the log has no warning matching "gave up waiting"

  @api-QtWebEngine.WebEngineView.goBack @api-QtWebEngine.WebEngineView.goForward @api-QtWebEngine.WebEngineView.canGoBack @api-QtWebEngine.WebEngineView.canGoForward
  Scenario: The view goes back and forward through the pages it showed
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        url: Qt.resolvedUrl("page.html")
        function show(name) { url = Qt.resolvedUrl(name) }
      }
      """
    When 2 seconds pass
    Then the property "canGoBack" of "view" is false
    When the method "show" of "view" is called with "second.html"
    And 2 seconds pass
    Then the property "title" of "view" is "Second Page"
    And the property "canGoBack" of "view" is true
    And the property "canGoForward" of "view" is false
    When the method "goBack" of "view" is called
    And 2 seconds pass
    Then the property "title" of "view" is "Fixture Page"
    And the property "canGoBack" of "view" is false
    And the property "canGoForward" of "view" is true
    When the method "goForward" of "view" is called
    And 2 seconds pass
    Then the property "title" of "view" is "Second Page"

  @api-QtWebEngine.WebEngineView.reload
  Scenario: Reloading loads the page again
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property int loads: 0
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) loads++ }
      }
      """
    When 2 seconds pass
    And the method "reload" of "view" is called
    And 2 seconds pass
    Then the property "loads" of "view" is 2
    And the property "canGoBack" of "view" is false

  @api-QtWebEngine.WebEngineView.runJavaScript
  Scenario: A script run in the page hands its result back
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) {
          runJavaScript("document.title", r => seen += r)
          runJavaScript("1 + 2", r => seen += " " + r + " " + typeof r)
          runJavaScript("({a: 1, b: [2, 3]})", r => seen += " " + r.a + "/" + r.b.join("/"))
        } }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "Fixture Page 3 number 1/2/3"

  Scenario: A script that throws hands back nothing
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: "?"
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus)
          runJavaScript("throw new Error('x')", r => seen = String(r)) }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "undefined"

  Scenario: A script in another world sees the page but not the page's scripts
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) {
          runJavaScript("window.shared = 5")
          runJavaScript("document.title + ' ' + window.shared", WebEngineScript.ApplicationWorld, r => seen = r)
        } }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "Fixture Page undefined"

  @api-QtWebEngine.WebEngineView.userScripts
  Scenario: A script given to the view runs in every page it loads
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        Component.onCompleted: userScripts.collection = [{ name: "mark",
          sourceCode: "window.marked = 'yes'",
          injectionPoint: WebEngineScript.DocumentCreation, worldId: WebEngineScript.MainWorld }]
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus)
          runJavaScript("window.marked", r => seen += (seen ? " " : "") + r) }
        function show(name) { url = Qt.resolvedUrl(name) }
      }
      """
    When 2 seconds pass
    And the method "show" of "view" is called with "second.html"
    And 2 seconds pass
    Then the property "seen" of "view" is "yes yes"

  @api-QtWebEngine.WebEngineView.javaScriptConsoleMessage
  Scenario: What the page writes to its console reaches the view with its level
    Given the text file "console.html":
      """
      <!doctype html><html><head><title>C</title>
      <script>console.log('hi'); console.warn('careful'); console.error('broken')</script>
      </head><body></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("console.html")
        onJavaScriptConsoleMessage: (level, message, line, source) =>
          seen += (seen ? " " : "") + level + ":" + message + ":" + line
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "0:hi:3 1:careful:3 2:broken:3"

  @api-QtWebEngine.WebEngineView.zoomFactor
  Scenario: The zoom factor keeps to what a page can be zoomed to
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        url: Qt.resolvedUrl("page.html")
      }
      """
    When 2 seconds pass
    And the property "zoomFactor" of "view" is set to 3
    Then the property "zoomFactor" of "view" is 3
    When the property "zoomFactor" of "view" is set to 10
    Then the property "zoomFactor" of "view" is 3
    When the property "zoomFactor" of "view" is set to 0.1
    Then the property "zoomFactor" of "view" is 3

  @ljos-only
  Scenario: A page that has loaded and does not move leaves the application idle
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        width: 200; height: 100
        url: Qt.resolvedUrl("page.html")
      }
      """
    When 2 seconds pass
    Then the application has nothing scheduled

  Scenario: A moving page in a view nobody can see leaves the application idle
    Given the text file "moving.html":
      """
      <!doctype html>
      <html><head><style>
      @keyframes spin { to { transform: rotate(360deg) } }
      div { width: 40px; height: 40px; background: #0000ff; animation: spin 1s linear infinite }
      </style></head><body><div></div></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        width: 200; height: 100
        Item {
          anchors.fill: parent
          visible: false
          WebEngineView {
            anchors.fill: parent
            url: Qt.resolvedUrl("moving.html")
          }
        }
      }
      """
    When 2 seconds pass
    Then the application has nothing scheduled

  # A page whose scripts never stop keeps the browser working for good.
  # Time passing waits for what the scene asked of it, not for that work
  # to end, and the page still answers what it is asked.
  @api-QtWebEngine.WebEngineView.runJavaScript
  Scenario: A page that never stops working lets time pass and still answers
    Given the text file "busy.html":
      """
      <!doctype html>
      <html><head><title>Busy</title></head><body><script>
      let n = 0;
      setInterval(() => { n += 1; document.body.textContent = String(n); }, 1);
      </script></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("busy.html")
        Timer {
          interval: 2500; running: true
          onTriggered: view.runJavaScript("n > 0", r => view.seen = String(r))
        }
      }
      """
    When 2 seconds pass
    And 1 second passes
    And 1 second passes
    Then the property "seen" of "view" is "true"
    And the property "title" of "view" is "Busy"
    And the log has no warning matching "gave up waiting"

  # A load that fails shows the browser's error page, which is not always
  # drawn: time passing does not wait for its picture.
  @api-QtWebEngine.WebEngineView.loadingChanged @api-QtWebEngine.WebEngineSettings.errorPageEnabled
  Scenario Outline: A page that fails to load with error pages on lets time pass
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string failure: ""
        url: "$SERVER/<path>"
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadFailedStatus)
          failure = info.errorCode + " " + info.errorString + " " + info.errorDomain }
      }
      """
    When 2 seconds pass
    And 2 seconds pass
    And 2 seconds pass
    Then the property "failure" of "view" is "<failure>"
    And the property "loading" of "view" is false
    And the log has no warning matching "gave up waiting"

    Examples:
      | path     | failure                             |
      | refused  | -102 net::ERR_CONNECTION_REFUSED 2  |
      | loop     | -310 net::ERR_TOO_MANY_REDIRECTS 4  |
