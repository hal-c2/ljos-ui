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
    And the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      msgctxt "Door"
      msgid "Open"
      msgstr "Opnaðu"

      msgctxt "FileMenu"
      msgid "Open"
      msgstr "Opna"
      """

  Scenario: A file migrated in a later run finds the text still in its own context
    A project migrated a file at a time asks the whole catalogue, not only the
    files of one run, whether a text is translated two ways.
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
    When the author runs "ljos migrate --write --catalogue app_is.ts FileMenu.qml"
    And the author runs "ljos migrate --write --catalogue app_is.ts Door.qml"
    Then the file "FileMenu.qml" is:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item { property string open: t({ message: "Open", context: "FileMenu" }) }
      """
    And the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      msgctxt "Door"
      msgid "Open"
      msgstr "Opnaðu"

      msgctxt "FileMenu"
      msgid "Open"
      msgstr "Opna"
      """

  Scenario: A message the catalogue written already translates otherwise keeps its file
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { property string save: qsTr("Save") }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>Save</source><translation>Geyma</translation></message></context>
      </TS>
      """
    And the file "is.po":
      """po
      msgid "Save"
      msgstr "Vista"
      """
    When the author runs "ljos migrate --catalogue app_is.ts Main.qml"
    Then the command's output mentions "kept Main.qml:2:30: `Save` is one message to `t`, which is.po translates already as"

  Scenario: A translation the catalogue written keeps only for its translators does not keep a file
    A fuzzy translation is not one yet, and an obsolete one is kept only for
    the translator to see, so the translation in the `.ts` takes their place.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property string save: qsTr("Save")
        property string open: qsTr("Open")
      }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>Save</source><translation>Geyma</translation></message>
        <message><source>Open</source><translation>Opna</translation></message></context>
      </TS>
      """
    And the file "is.po":
      """po
      #, fuzzy
      msgid "Save"
      msgstr "Vista"

      #~ msgid "Open"
      #~ msgstr "Opnaðu"
      """
    When the author runs "ljos migrate --write --catalogue app_is.ts Main.qml"
    Then the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      msgid "Open"
      msgstr "Opna"

      msgid "Save"
      msgstr "Geyma"
      """

  Scenario: A translation that shows the values in another order shows each where the translator put it
    `t` numbers a value that is not a name in the order the text shows it,
    so the translation's numbers are the source's.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { property string of: qsTr("%1 of %2").arg(10).arg(20) }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>%1 of %2</source><translation>%2 alls, þar af %1</translation></message></context>
      </TS>
      """
    When the author runs "ljos migrate --write --catalogue app_is.ts Main.qml"
    Then the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      msgid "{0} of {1}"
      msgstr "{1} alls, þar af {0}"
      """

  Scenario: A catalogue whose language is not a language is not read
    The language names the `.po` written beside the catalogue.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { property string open: qsTr("Open") }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="../is">
      </TS>
      """
    When the author runs "ljos migrate --write --catalogue app_is.ts Main.qml"
    Then the command exits with status 2
    And the command's output mentions "app_is.ts: `../is` is not a language"

  Scenario: A catalogue not updated since the call gained its count keeps the file
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { property int n: 2; property string files: qsTr("%n file(s)", "", n) }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>%n file(s)</source><translation>skrár</translation></message></context>
      </TS>
      """
    When the author runs "ljos migrate --catalogue app_is.ts Main.qml"
    Then the command's output mentions "app_is.ts translates `%n file(s)` with one form, and the call is counted"

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
    Then the command's output mentions "kept ./Main.qml:2:30: `Open` in the context `menu` is one message to `t`"
    And the command's output mentions "kept ./Other.qml:2:30: `Open` in the context `menu` is one message to `t`"

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
    Then the command's output mentions "kept ./Door.qml:2:30:"
    And the command's output mentions "kept ./FileMenu.qml:2:30:"

  Scenario: The catalogue says each value by the name the message gives it
    A translation is written by the rewrite its call is: `%1` is the value
    `.arg` fills it with, and the numerus forms are the forms of `plural`.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        property int count: 3
        property string name: "Ada"
        property string of: qsTr("%1 of %2").arg(name).arg(count)
        property string files: qsTr("%n file(s)", "", count)
        property string grouped: qsTr("%Ln file(s)", "", count)
        property string home: qsTrId("nav.home")
      }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>%1 of %2</source><extracomment>Pages</extracomment>
          <translatorcomment>Read as a page count</translatorcomment>
          <translation>%2 alls, þar af %1</translation></message>
        <message numerus="yes"><source>%n file(s)</source><translation>
          <numerusform>%n skrá</numerusform><numerusform>%n skrár</numerusform></translation></message>
        <message numerus="yes"><source>%Ln file(s)</source><translation>
          <numerusform>%Ln skrá</numerusform><numerusform>%Ln skrár</numerusform></translation></message>
      </context>
      <context><name></name>
        <message id="nav.home"><source>Home</source><translation>Heim</translation></message>
      </context>
      </TS>
      """
    When the author runs "ljos migrate --write --catalogue app_is.ts ."
    Then the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      #. js-lingui-explicit-id
      #. Default: Home
      msgid "nav.home"
      msgstr "Heim"

      msgid "{count, plural, other {# file(s)}}"
      msgstr "{count, plural, one {# skrá} other {# skrár}}"

      msgid "{count, plural, other {{count} file(s)}}"
      msgstr "{count, plural, one {{count} skrá} other {{count} skrár}}"

      # Read as a page count
      #. Pages
      msgid "{name} of {count}"
      msgstr "{count} alls, þar af {name}"
      """

  Scenario: A text the catalogue has not translated yet waits for its translation
    Qt shows the text as written where its translation is unfinished, and so
    does `t` where the translation is empty.
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item { property string later: qsTr("Later") }
      """
    And the file "app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message><source>Later</source><translation type="unfinished"></translation></message>
      </context>
      </TS>
      """
    When the author runs "ljos migrate --write --catalogue app_is.ts ."
    Then the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      msgid "Later"
      msgstr ""
      """

  # Needs PR #506: `ljos render --language` and the catalogues in `i18n/`.
  @backlog
  Scenario: The catalogue written shows what Qt's showed
    The application as written, with Qt's catalogue, and the application
    migrated, with the catalogue written for it, show the same text. The
    `.ts` file stays beside the `.po`; the migrated file asks for none of
    its messages.
    Given the file "qt/Main.qml":
      """qml
      import QtQuick
      Rectangle {
        width: 320; height: 40; color: "white"
        property int count: 3
        property string name: "Ada"
        Text { text: qsTr("%1 has %n file(s)", "", count).arg(name) }
        Text { y: 20; text: qsTr("Open", "menu") }
      }
      """
    And the file "qt/i18n/app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message numerus="yes"><source>%1 has %n file(s)</source><translation>
          <numerusform>%1 á %n skrá</numerusform><numerusform>%1 á %n skrár</numerusform></translation></message>
        <message><source>Open</source><comment>menu</comment><translation>Opna</translation></message>
      </context>
      </TS>
      """
    And the file "app/Main.qml":
      """qml
      import QtQuick
      Rectangle {
        width: 320; height: 40; color: "white"
        property int count: 3
        property string name: "Ada"
        Text { text: qsTr("%1 has %n file(s)", "", count).arg(name) }
        Text { y: 20; text: qsTr("Open", "menu") }
      }
      """
    And the file "app/i18n/app_is.ts":
      """xml
      <TS version="2.1" language="is">
      <context><name>Main</name>
        <message numerus="yes"><source>%1 has %n file(s)</source><translation>
          <numerusform>%1 á %n skrá</numerusform><numerusform>%1 á %n skrár</numerusform></translation></message>
        <message><source>Open</source><comment>menu</comment><translation>Opna</translation></message>
      </context>
      </TS>
      """
    When the author runs "ljos migrate --write --catalogue app/i18n/app_is.ts app/Main.qml"
    Then the command's output mentions "wrote app/i18n/is.po"
    When the author runs "ljos render qt --out kept.png --settle 0 --compat keep --language is"
    And the author runs "ljos render app --out lowered.png --settle 0 --language is"
    Then the pictures "kept.png" and "lowered.png" have the same pixels
