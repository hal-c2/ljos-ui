# Sources:
#   docs/DIALECT.md (§4.3.1 No Qt in a file of Ljós's API, §4.13 Text, §8 ljos migrate)
#   docs/ARCHITECTURE.md (§15 Localisation)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTr-method
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
#   https://lingui.dev/ref/macro
# `ljos migrate` is Ljós's own; Qt's QML before it runs on Qt as catalogue.feature shows.

@ljos-only @headless @backlog
Feature: ljos migrate writes qsTr as a tagged template
  qsTr is Qt's, so a file of Ljós's API translates with `t` instead. `ljos
  migrate` rewrites each call: the numbered `%1` of `.arg` become the
  values themselves, a count becomes `plural`, and a disambiguation
  becomes the message's context. A call it cannot write out keeps its file
  as Qt's, which still runs.

  Scenario: Text, values and counts are written in the message
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property int count: 3
        property string name: "Ada"
        property string open: qsTr("Open")
        property string of: qsTr("%1 of %2").arg(name).arg(count)
        property string files: qsTr("%n file(s)", "", count)
      }
      """
    When the author runs "ljos migrate --write ."
    Then the file "Main.qml" is:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property int count: 3
        property string name: "Ada"
        property string open: t`Open`
        property string of: t`${name} of ${count}`
        property string files: plural(count, { other: "# file(s)" })
      }
      """

  Scenario: Contexts, ids and marked texts keep what they say
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property string a: qsTr("Open", "menu")
        property string b: qsTranslate("Dialog", "Close")
        property string c: qsTrId("nav.home")
        property var d: QT_TR_NOOP("Later")
      }
      """
    When the author runs "ljos migrate --write ."
    Then the file "Main.qml" is:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string a: t({ message: "Open", context: "menu" })
        property string b: t({ message: "Close", context: "Dialog" })
        property string c: t({ id: "nav.home" })
        property var d: msg`Later`
      }
      """

  Scenario: The file lowered shows what the file as written shows
    Given the file "app/Main.qml":
      """qml
      import QtQuick
      Rectangle {
        width: 240; height: 40; color: "white"
        property int count: 3
        property string name: "Ada"
        Text { text: qsTr("%1 has %n file(s)", "", count).arg(name) }
      }
      """
    When the author runs "ljos migrate app"
    Then the command's output mentions "would lower app/Main.qml"
    When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
    And the author runs "ljos render app --out lowered.png --settle 0 --compat lower"
    Then the pictures "kept.png" and "lowered.png" have the same pixels

  Scenario: qsTr of a text that is not written out keeps its file
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property string key: "Open"
        property string shown: qsTr(key)
      }
      """
    When the author runs "ljos migrate ."
    Then the command prints:
      """
      kept Main.qml:4:26: `qsTr` of a value that is not written out
      1 files, 0 lowered, 1 kept
      """
