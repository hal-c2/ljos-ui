# Sources:
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengine.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webengineprofile.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webenginesettings.html
#   https://doc.qt.io/qt-6/qml-qtwebengine-webenginescript.html
# Verified on Qt 6.12.0 (qml6, offscreen).
# None of these load a page, so they run whether or not a web engine is
# there to show one.

@qt-parity
Feature: The types of QtWebEngine
  `import QtWebEngine` gives WebEngineView and the types around it: the
  WebEngine singleton with its default profile and scripts, a view's
  settings, its profile and the names of its load states and script
  worlds.

  @api-QtWebEngine.WebEngineView.zoomFactor @api-QtWebEngine.WebEngineView.loading @api-QtWebEngine.WebEngineView.backgroundColor
  Scenario: A view that has loaded nothing
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView { objectName: "view"; width: 200; height: 100 }
      """
    Then the property "zoomFactor" of "view" is 1
    And the property "loading" of "view" is false
    And the property "loadProgress" of "view" is 0
    And the property "title" of "view" is ""
    And the property "canGoBack" of "view" is false
    And the property "canGoForward" of "view" is false
    And the property "audioMuted" of "view" is false
    And the property "recentlyAudible" of "view" is false
    And the property "backgroundColor" of "view" is "#ffffff"
    And the property "settings.errorPageEnabled" of "view" is true
    And the property "settings.focusOnNavigationEnabled" of "view" is false
    And the property "settings.javascriptEnabled" of "view" is true

  Scenario: The load states and script worlds have their numbers
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        objectName: "root"
        property string loads: [WebEngineView.LoadStartedStatus, WebEngineView.LoadStoppedStatus,
          WebEngineView.LoadSucceededStatus, WebEngineView.LoadFailedStatus].join(" ")
        property string points: [WebEngineScript.Deferred, WebEngineScript.DocumentReady,
          WebEngineScript.DocumentCreation].join(" ")
        property string worlds: [WebEngineScript.MainWorld, WebEngineScript.ApplicationWorld,
          WebEngineScript.UserWorld].join(" ")
      }
      """
    Then the property "loads" of "root" is "0 1 2 3"
    And the property "points" of "root" is "0 1 2"
    And the property "worlds" of "root" is "0 1 2"

  @api-QtWebEngine.WebEngineNavigationRequest.navigationType @api-QtWebEngine.WebEngineNavigationRequest.action
  Scenario: The kinds of navigation and the answers to it have their numbers
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        objectName: "root"
        property string kinds: [WebEngineNavigationRequest.LinkClickedNavigation,
          WebEngineNavigationRequest.TypedNavigation, WebEngineNavigationRequest.FormSubmittedNavigation,
          WebEngineNavigationRequest.BackForwardNavigation, WebEngineNavigationRequest.ReloadNavigation,
          WebEngineNavigationRequest.OtherNavigation, WebEngineNavigationRequest.RedirectNavigation].join(" ")
        property string scoped: WebEngineNavigationRequest.NavigationType.ReloadNavigation
        property string answers: [WebEngineNavigationRequest.AcceptRequest,
          WebEngineNavigationRequest.IgnoreRequest].join(" ")
      }
      """
    Then the property "kinds" of "root" is "0 1 2 3 4 5 6"
    And the property "scoped" of "root" is "4"
    And the property "answers" of "root" is "0 255"

  @api-QtWebEngine.WebEnginePermission.permissionType @api-QtWebEngine.WebEnginePermission.state
  Scenario: The kinds of permission and their states have their numbers
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        objectName: "root"
        property string kinds: [WebEnginePermission.PermissionType.Unsupported,
          WebEnginePermission.PermissionType.MediaAudioCapture,
          WebEnginePermission.PermissionType.MediaVideoCapture,
          WebEnginePermission.PermissionType.MediaAudioVideoCapture,
          WebEnginePermission.PermissionType.DesktopVideoCapture,
          WebEnginePermission.PermissionType.DesktopAudioVideoCapture,
          WebEnginePermission.PermissionType.MouseLock,
          WebEnginePermission.PermissionType.Notifications,
          WebEnginePermission.PermissionType.Geolocation,
          WebEnginePermission.PermissionType.ClipboardReadWrite,
          WebEnginePermission.PermissionType.LocalFontsAccess].join(" ")
        property string states: [WebEnginePermission.State.Invalid, WebEnginePermission.State.Ask,
          WebEnginePermission.State.Granted, WebEnginePermission.State.Denied].join(" ")
      }
      """
    Then the property "kinds" of "root" is "0 1 2 3 4 5 6 7 8 9 10"
    And the property "states" of "root" is "0 1 2 3"

  @api-QtWebEngine.WebEngine.defaultProfile
  Scenario: The default profile keeps nothing on disk
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        objectName: "root"
        property bool offTheRecord: WebEngine.defaultProfile.offTheRecord
        property string storageName: WebEngine.defaultProfile.storageName
        property bool same: view.profile === WebEngine.defaultProfile
        WebEngineView { id: view }
      }
      """
    Then the property "offTheRecord" of "root" is true
    And the property "storageName" of "root" is ""
    And the property "same" of "root" is true

  @api-QtWebEngine.WebEngineProfile.persistentPermissionsPolicy @api-QtWebEngine.WebEngineProfile.persistentStoragePath @api-QtWebEngine.WebEngineProfile.cachePath
  Scenario: A profile on disk keeps answers on disk, and the places it is given
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        objectName: "root"
        WebEngineProfile { id: disk; storageName: "Kept"; offTheRecord: false }
        WebEngineProfile {
          id: asking; storageName: "Asking"; offTheRecord: false
          persistentStoragePath: "/data/app/browser"; cachePath: "/cache/app/browser"
          persistentPermissionsPolicy: WebEngineProfile.AskEveryTime
        }
        property string policies: [WebEngineProfile.AskEveryTime, WebEngineProfile.StoreInMemory,
          WebEngineProfile.StoreOnDisk].join(" ")
        property int kept: disk.persistentPermissionsPolicy
        property int asks: asking.persistentPermissionsPolicy
        property string places: asking.persistentStoragePath + " " + asking.cachePath
      }
      """
    Then the property "policies" of "root" is "0 1 2"
    And the property "kept" of "root" is 2
    And the property "asks" of "root" is 0
    And the property "places" of "root" is "/data/app/browser /cache/app/browser"

  @api-QtWebEngine.WebEngine.script
  Scenario: A script made by the singleton holds what it was given
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      Item {
        objectName: "root"
        property string seen: ""
        Component.onCompleted: {
          const s = WebEngine.script()
          s.name = "mark"; s.sourceCode = "window.marked = 1"
          s.injectionPoint = WebEngineScript.DocumentCreation
          s.worldId = WebEngineScript.MainWorld
          seen = [s.name, s.sourceCode, s.injectionPoint, s.worldId, s.runsOnSubFrames].join(" ")
        }
      }
      """
    Then the property "seen" of "root" is "mark window.marked = 1 2 0 false"

  @api-QtWebEngine.WebEngineView.userScripts
  Scenario: A view's scripts can be added and cleared
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        id: view; objectName: "view"
        property string seen: ""
        Component.onCompleted: {
          const s = WebEngine.script()
          s.name = "one"; s.sourceCode = "1"
          userScripts.insert(s)
          seen = userScripts.collection.length + " " + userScripts.contains(s)
          userScripts.clear()
          seen += " " + userScripts.collection.length
        }
      }
      """
    Then the property "seen" of "view" is "1 true 0"

  Scenario: A profile with a name of its own
    Given the QML:
      """qml
      import QtQuick
      import QtWebEngine
      WebEngineView {
        objectName: "view"
        profile: WebEngineProfile { storageName: "Example"; offTheRecord: false }
      }
      """
    Then the property "profile.storageName" of "view" is "Example"
    And the property "profile.offTheRecord" of "view" is false
