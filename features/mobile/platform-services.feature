# Sources:
#   https://developer.android.com/privacy-and-security/keystore
#   https://developer.android.com/reference/android/content/ClipboardManager
#   https://developer.android.com/reference/android/content/ClipData#newHtmlText(java.lang.CharSequence,%20java.lang.CharSequence,%20java.lang.String)
#   https://developer.android.com/develop/ui/views/touch-and-input/copy-paste#Content
#   https://developer.android.com/reference/androidx/core/content/FileProvider
#   https://developer.android.com/reference/android/graphics/BitmapFactory.Options#inSampleSize
#   https://developer.android.com/reference/android/content/Intent#ACTION_VIEW
#   https://developer.android.com/training/data-storage/app-specific
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#openUrlExternally-method
#   https://doc.qt.io/qt-6/qstandardpaths.html
#   docs/ARCHITECTURE.md (§10.1: the platform's services; §15)
#   https://github.com/hal-c2/ljos-ui/issues/616
# On a phone the clipboard, the opener of links, the places an application
# keeps its files and the store of its secrets are the system's, reached
# through its Java side. These scenarios need the phone: anywhere else the
# services are the in-process ones that other features pin.
@ljos-only @android
Feature: An application on a phone uses the system's services
  What the application copies, other applications paste; a link it opens
  opens in the browser; its files are its own, in the places the system
  keeps for it; and a secret it keeps is there after the system ended its
  process, unreadable to anyone but the application.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtCore
      Window {
        id: w; objectName: "w"; visible: true
        property string data: StandardPaths.writableLocation(StandardPaths.AppDataLocation)
        property string cache: StandardPaths.writableLocation(StandardPaths.CacheLocation)
        property bool opened: false
        function open(url) { opened = Qt.openUrlExternally(url) }
        TextInput { id: input; objectName: "input"; text: "copied on a phone"; focus: true }
        TextEdit { id: rich; objectName: "rich"; textFormat: TextEdit.RichText; text: "a<b>bold</b>c" }
        TextEdit { id: target; objectName: "target"; textFormat: TextEdit.RichText }
        property string pasted: target.getText(0, target.length)
        property bool pastedBold: target.text.indexOf("font-weight:700") >= 0
        // A binding hears of a change only when the clipboard is said to
        // have changed: reading canPaste itself would ask anew.
        property bool pastable: input.canPaste
      }
      """

  Scenario: What the application copies is on the system's clipboard
    When the method "selectAll" of "input" is called
    And the method "copy" of "input" is called
    Then the clipboard holds "copied on a phone"

  Scenario: What another application copied is pasted
    When "from elsewhere" is copied in another application
    And the method "selectAll" of "input" is called
    And the method "paste" of "input" is called
    Then the property "text" of "input" is "from elsewhere"

  Scenario: Formatted text goes through the system's clipboard with its formats
    When the method "select" of "rich" is called with 1, 5
    And the method "copy" of "rich" is called
    And the method "paste" of "target" is called
    Then the property "pasted" of "w" is "bold"
    And the property "pastedBold" of "w" is true

  Scenario: A picture the application copies is on the system's clipboard
    When the application copies a picture 3 by 2 pixels
    Then the clipboard holds a picture 3 by 2 pixels

  Scenario: A copied picture is not pasted into text
    When the method "select" of "input" is called with 12, 17
    And the method "copy" of "input" is called
    And the application copies a picture 3 by 2 pixels
    Then the property "canPaste" of "input" is false
    When the method "selectAll" of "input" is called
    And the method "paste" of "input" is called
    Then the property "text" of "input" is "copied on a phone"

  Scenario: The application hears of a copy in another application
    When the application copies a picture 3 by 2 pixels
    Then the property "pastable" of "w" is false
    When "from elsewhere" is copied in another application
    Then the property "pastable" of "w" is true

  Scenario: A link opens in the browser
    When the method "open" of "w" is called with "https://example.com/"
    Then the property "opened" of "w" is true
    And the application is in the background

  Scenario: The application's files are its own
    Then the property "data" of "w" is a directory of the application's own
    And the property "cache" of "w" is a directory of the application's own

  Scenario: A secret outlives the application's process
    When the application keeps the secret "s3cret" as "token" of "accounts"
    And the system ends the application's process
    And the user opens the application again
    Then the application reads the secret "s3cret" as "token" of "accounts"
    And no file of the application holds "s3cret"

  Scenario: A secret removed is gone
    When the application keeps the secret "s3cret" as "token" of "accounts"
    And the application removes the secret "token" of "accounts"
    Then the application reads no secret as "token" of "accounts"
