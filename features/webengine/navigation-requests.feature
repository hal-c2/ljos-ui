# Sources:
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webenginenavigationrequest.html
#   https://doc.qt.io/qt-6/qwebenginenavigationrequest.html
# Verified on Qt 6.12.0 (qml6, offscreen).
# Every page here is a file beside the QML: nothing is fetched from the
# internet. A run without a web engine leaves these scenarios out and
# says why.

@qt-parity @chromium
Feature: A view is asked before its page goes somewhere
  Before a page goes anywhere, a WebEngineView emits navigationRequested
  with a WebEngineNavigationRequest: where it goes (url), whether the
  page itself or a frame in it goes (isMainFrame) and why
  (navigationType). The handler accepts or rejects it there and then; a
  request nobody rejects is accepted. A rejected request leaves the page
  where it was.

  Background:
    Given the text file "page.html":
      """
      <!doctype html>
      <html><head><title>Fixture Page</title></head>
      <body style="margin:0">
      <a href="second.html" style="display:block; height:50px; background:#ff0000"></a>
      <form action="second.html"><button style="display:block; width:200px; height:50px"></button></form>
      </body></html>
      """
    And the text file "second.html":
      """
      <!doctype html>
      <html><head><title>Second Page</title></head><body></body></html>
      """

  @api-QtWebEngine.WebEngineView.navigationRequested @api-QtWebEngine.WebEngineNavigationRequest.navigationType @api-QtWebEngine.WebEngineNavigationRequest.isMainFrame @api-QtWebEngine.WebEngineNavigationRequest.url
  Scenario: A URL given to the view is asked for before its load starts
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onNavigationRequested: request => seen += "ask:" + request.navigationType + ":"
          + request.isMainFrame + ":" + request.url.toString().endsWith("/page.html")
        onLoadingChanged: info => seen += " load:" + info.status
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "ask:1:true:true load:0 load:2"

  @api-QtWebEngine.WebEngineNavigationRequest.navigationType
  Scenario Outline: A page asked to go somewhere says why
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool loaded: false
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) loaded = true }
        onNavigationRequested: request => { if (loaded) seen += request.navigationType }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, <y>
    And the user releases the pointer at 100, <y>
    And 2 seconds pass
    Then the property "seen" of "view" is "<type>"
    And the property "title" of "view" is "Second Page"

    Examples:
      | what            | y  | type |
      | a link clicked  | 25 | 0    |
      | a form sent     | 75 | 2    |

  @api-QtWebEngine.WebEngineNavigationRequest.navigationType
  Scenario: Reloading asks as a reload
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property int loads: 0
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus && ++loads === 1) reload() }
        onNavigationRequested: request => seen += (seen ? " " : "") + request.navigationType
      }
      """
    When 3 seconds pass
    Then the property "seen" of "view" is "1 4"
    And the property "loads" of "view" is 2

  @api-QtWebEngine.WebEngineNavigationRequest.isMainFrame
  Scenario: A frame in the page asks as a frame of the page
    Given the text file "framed.html":
      """
      <!doctype html>
      <html><body><iframe src="second.html" width="50" height="20"></iframe></body></html>
      """
    And the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property string seen: ""
        url: Qt.resolvedUrl("framed.html")
        onNavigationRequested: request => { if (!request.isMainFrame)
          seen = request.navigationType + ":" + request.url.toString().endsWith("/second.html") }
      }
      """
    When 2 seconds pass
    Then the property "seen" of "view" is "5:true"

  @api-QtWebEngine.WebEngineNavigationRequest.reject
  Scenario: A link whose request is rejected leaves the page where it was
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool loaded: false
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (loaded) seen += (seen ? " " : "") + info.status + ":" + info.errorCode
          if (info.status === WebEngineView.LoadSucceededStatus) loaded = true }
        onNavigationRequested: request => { if (loaded) request.reject() }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 25
    And the user releases the pointer at 100, 25
    And 2 seconds pass
    Then the property "seen" of "view" is "0:0 1:-3"
    And the property "title" of "view" is "Fixture Page"

  @api-QtWebEngine.WebEngineNavigationRequest.reject @api-QtWebEngine.WebEngineView.url
  Scenario: A link after a URL within the same page is still asked for
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool moved: false
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus && !moved) {
          moved = true; url = Qt.resolvedUrl("page.html#below") } }
        onNavigationRequested: request => { if (request.url.toString().endsWith("/second.html")) {
          seen = "asked"; request.reject() } }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 25
    And the user releases the pointer at 100, 25
    And 2 seconds pass
    Then the property "seen" of "view" is "asked"
    And the property "title" of "view" is "Fixture Page"

  @api-QtWebEngine.WebEngineNavigationRequest.navigationType
  Scenario: Reloading after a URL within the same page is asked for
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool moved: false
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus && !moved) {
          moved = true; url = Qt.resolvedUrl("page.html#below") } }
        onNavigationRequested: request => {
          if (request.navigationType === WebEngineNavigationRequest.ReloadNavigation) seen = "asked" }
      }
      """
    When 2 seconds pass
    And the method "reload" of "view" is called
    And 2 seconds pass
    Then the property "seen" of "view" is "asked"
    And the property "loading" of "view" is false

  @api-QtWebEngine.WebEngineNavigationRequest.reject @api-QtWebEngine.WebEngineView.url
  Scenario: A URL whose request is rejected leaves the view at the page before
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool loaded: false
        property bool stayed: url.toString().endsWith("/page.html")
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus && !loaded) {
          loaded = true; url = Qt.resolvedUrl("second.html") } }
        onNavigationRequested: request => { if (loaded) request.reject() }
      }
      """
    When 3 seconds pass
    Then the property "stayed" of "view" is true
    And the property "title" of "view" is "Fixture Page"

  @api-QtWebEngine.WebEngineNavigationRequest.action @api-QtWebEngine.WebEngineNavigationRequest.accept
  Scenario: A request nobody answers is accepted
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        width: 200; height: 100
        property bool loaded: false
        property string seen: ""
        url: Qt.resolvedUrl("page.html")
        onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) loaded = true }
        onNavigationRequested: request => { if (loaded) seen = "" + request.action }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 25
    And the user releases the pointer at 100, 25
    And 2 seconds pass
    Then the property "seen" of "view" is "0"
    And the property "title" of "view" is "Second Page"

  # Qt hands a link to another application (mailto:) past
  # navigationRequested; a view here is asked about it first, so an
  # application can hand the link on itself or keep it from the system.
  @qt-diverges @api-QtWebEngine.WebEngineView.navigationRequested
  Scenario: A link to another application is asked for as a link
    Given the text file "mail.html":
      """
      <!doctype html>
      <html><head><title>Mail Page</title></head><body style="margin:0">
      <a href="mailto:someone@example.com" style="display:block; height:100px; background:#ff0000"></a>
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
        url: Qt.resolvedUrl("mail.html")
        onNavigationRequested: request => { if (request.url.toString().startsWith("mailto:")) {
          seen = request.url + " " + request.navigationType; request.reject() } }
      }
      """
    When 2 seconds pass
    And the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    And 1 second passes
    Then the property "seen" of "view" is "mailto:someone@example.com 0"
    And the property "title" of "view" is "Mail Page"
