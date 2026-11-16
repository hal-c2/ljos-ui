# Sources:
#   docs/DIALECT.md (§4.3.1 No Qt in a file of Ljós's API, §4.13 Text, §8 ljos migrate)
#   docs/ARCHITECTURE.md (§15 Localisation)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#qsTr-method
#   https://doc.qt.io/qt-6/qtquick-internationalization.html
#   https://lingui.dev/ref/macro
# `ljos migrate` is Ljós's own; Qt's QML before it runs on Qt as catalogue.feature shows.

@ljos-only @headless
Feature: ljos migrate writes qsTr as a tagged template
  qsTr is Qt's, so a file of Ljós's API translates with `t` instead. `ljos
  migrate` rewrites each call: the numbered `%1` of `.arg` become the
  values themselves, a count becomes `plural`, and a disambiguation
  becomes the message's context. A call it cannot write out keeps its file
  as Qt's, which still runs.

  Scenario: Text, values and counts are written in the message
    Each `.arg` fills the lowest-numbered `%N` left, wherever it stands.
    Qt's `%n` is the count as it is, and `%Ln` the count grouped.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property int count: 3
        property string name: "Ada"
        property string open: qsTr("Open")
        property string of: qsTr("%1 of %2").arg(name).arg(count)
        property string back: qsTr("%2 of %1").arg(name).arg(count)
        property string files: qsTr("%n file(s)", "", count)
        property string grouped: qsTr("%Ln file(s)", "", count)
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
        property string back: t`${count} of ${name}`
        property string files: plural(count, { other: `${count} file(s)` })
        property string grouped: plural(count, { other: "# file(s)" })
      }
      """

  Scenario: Contexts and ids keep what they say
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property string a: qsTr("Open", "menu")
        property string b: qsTranslate("Dialog", "Close")
        property string c: qsTrId("nav.home")
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
      }
      """

  Scenario: The file lowered shows what the file as written shows
    Given the file "app/Main.qml":
      """qml
      import QtQuick
      Rectangle {
        width: 320; height: 40; color: "white"
        property int count: 12345
        property string name: "Ada"
        Text { text: qsTr("%1 has %n file(s)", "", count).arg(name) }
        Text { y: 20; text: qsTr("%Ln file(s)", "", count) }
      }
      """
    When the author runs "ljos migrate app"
    Then the command's output mentions "would lower app/Main.qml"
    When the author runs "ljos render app --out kept.png --settle 0 --compat keep"
    And the author runs "ljos render app --out lowered.png --settle 0 --compat lower"
    Then the pictures "kept.png" and "lowered.png" have the same pixels

  Scenario Outline: A count not known to be a whole number keeps its file
    Qt shows the count truncated, and a template shows it as it is.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property <type> count: 2
        property string files: qsTr("%n file(s)", "", <count>)
      }
      """
    When the author runs "ljos migrate ."
    Then the command's output mentions "kept ./Main.qml:4:"

    Examples:
      | type | count     |
      | real | count     |
      | var  | count     |
      | int  | count / 2 |

  Scenario: Values reordered with a call in them keep their file
    A template works out its values left to right, Qt's `.arg` in call order.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property int n: 0
        function next() { n += 1; return n }
        property string shown: qsTr("%2 of %1").arg(next()).arg(next())
      }
      """
    When the author runs "ljos migrate ."
    Then the command's output mentions "kept ./Main.qml:5:"

  Scenario: A plain value reordered beside one that changes something keeps its file
    `.arg(n).arg(n++)` reads `n` before it grows; written the other way round
    it would read it after.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property int n: 1
        property string shown: qsTr("%2 %1").arg(n).arg(n++)
      }
      """
    When the author runs "ljos migrate ."
    Then the command's output mentions "kept ./Main.qml:4:"

  Scenario: A length declared as a real number is no count
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        Item { id: r; property real length: 2.5 }
        property string files: qsTr("%n file(s)", "", r.length)
      }
      """
    When the author runs "ljos migrate ."
    Then the command's output mentions "kept ./Main.qml:4:"

  Scenario: A text marked for later keeps its file
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property string later: QT_TR_NOOP("Later")
      }
      """
    When the author runs "ljos migrate ."
    Then the command's output mentions "kept ./Main.qml:3:"

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
      kept ./Main.qml:4:26: `qsTr` of a value that is not written out
      1 files, 0 lowered, 1 kept
      """

  # Needs the extract lane: ljos migrate --catalogue
  @backlog
  Scenario: A text two files translate differently keeps each file's translation
    Given the file "FileMenu.qml":
      """qml
      import QtQuick
      Item { property string open: qsTr("Open") }
      """
    And the file "Door.qml":
      """qml
      import QtQuick
      Item { property string open: qsTr("Open") }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>FileMenu</name>
        <message><source>Open</source><translation>Opna</translation></message></context>
      <context><name>Door</name>
        <message><source>Open</source><translation>Opnaðu</translation></message></context>
      </TS>
      """
    When the author runs "ljos migrate --write --catalogue app_is.ts ."
    Then the file "FileMenu.qml" is:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item { property string open: t({ message: "Open", context: "FileMenu" }) }
      """
    And the file "Door.qml" is:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item { property string open: t({ message: "Open", context: "Door" }) }
      """

  # Needs the extract lane: ljos migrate --catalogue
  @backlog
  Scenario: Two calls that would be one message with two translations keep their files
    Qt's disambiguation and a qsTranslate context both become the context of `t`.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { property string open: qsTr("Open", "menu") }
      """
    And the file "Other.qml":
      """qml
      import QtQuick
      Item { property string open: qsTranslate("menu", "Open") }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>Open</source><comment>menu</comment><translation>Opna valmynd</translation></message></context>
      <context><name>menu</name>
        <message><source>Open</source><translation>Opna</translation></message></context>
      </TS>
      """
    When the author runs "ljos migrate --catalogue app_is.ts ."
    Then the command's output mentions "kept Main.qml"
    And the command's output mentions "kept Other.qml"

  # Needs the extract lane: ljos migrate --catalogue
  @backlog
  Scenario: One disambiguation two files translate differently keeps both files
    Given the file "FileMenu.qml":
      """qml
      import QtQuick
      Item { property string open: qsTr("Open", "menu") }
      """
    And the file "Door.qml":
      """qml
      import QtQuick
      Item { property string open: qsTr("Open", "menu") }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>FileMenu</name>
        <message><source>Open</source><comment>menu</comment><translation>Opna</translation></message></context>
      <context><name>Door</name>
        <message><source>Open</source><comment>menu</comment><translation>Opnaðu</translation></message></context>
      </TS>
      """
    When the author runs "ljos migrate --catalogue app_is.ts ."
    Then the command's output mentions "kept FileMenu.qml"
    And the command's output mentions "kept Door.qml"
