# Sources:
#   docs/DIALECT.md (§4.3.1 No Qt in a file of Ljós's API, §4.13 Text, §5.1 Rules)
#   docs/ARCHITECTURE.md (§15 Localisation)
#   https://lingui.dev/ref/macro
# `ljos check` is Ljós's own: there is nothing of Qt's to compare with.

@ljos-only @headless
Feature: Checking translated text
  `ljos check` holds a file of Ljós's API to its own way of translating:
  a message is written out where it is, so a translator gets all of it,
  and `qsTr` is Qt's, for Qt's QML only.

  Scenario: qsTr in a file of Ljós's API names the tagged template
    Given the file "Title.qml" without its indentation:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string title: qsTr("Open")
      }
      """
    When the files are checked
    Then the check reports:
      """
      Title.qml:4:26: error: `qsTr` is Qt's: use t`…` (Ljos.Core) [qt-tr]
      1 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  Scenario: A text marked for later is written with msg by hand
    Given the file "Later.qml" without its indentation:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string later: QT_TR_NOOP("Later")
      }
      """
    When the files are checked
    Then the check reports:
      """
      Later.qml:4:26: error: `QT_TR_NOOP` is Qt's: write msg`…` (Ljos.Core) and translate it with t(…) where it is shown [qt-tr]
      1 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  Scenario: A form that is not written out cannot be translated
    Given the file "Files.qml" without its indentation:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property int count: 2
        property string many: "# files"
        property string files: plural(count, { one: "# file", other: many })
      }
      """
    When the files are checked
    Then the check reports:
      """
      Files.qml:6:64: error: a form of `plural` is translated as written, so it is a string or a template, not `many` [i18n-literal]
      1 files, 1 errors, 0 warnings
      ljos: 1 errors
      """

  Scenario: A count and the words around it are one message
    Given the file "Ago.qml" without its indentation:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property int minutes: 5
        property string ago: minutes + " " + t`minutes ago`
      }
      """
    When the files are checked
    Then the check reports:
      """
      Ago.qml:5:24: warning: a translated text joined to another value is two messages: write one, t`${minutes} minutes ago` [i18n-concat]
      1 files, 0 errors, 1 warnings
      """

  Scenario: Forms without other are an error
    Given the file "Role.qml" without its indentation:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string role: "admin"
        property string label: select(role, { admin: "Administrator" })
      }
      """
    When the files are checked
    Then the check reports the lines:
      """
      Role.qml:5:39: error: the forms of `select` need `other`, the form shown when none matches [i18n-other]
      """
