# Sources:
#   docs/ARCHITECTURE.md (§15 Localisation: catalogues, extraction)
#   https://lingui.dev/ref/catalog-formats
#   https://www.gnu.org/software/gettext/manual/html_node/PO-Files.html
#   https://unicode-org.github.io/icu/userguide/format_parse/messages/
# `ljos i18n extract` is Ljós's own; Qt's lupdate reads qsTr, which catalogue-comments.feature covers.

@ljos-only @headless
Feature: ljos i18n extract writes the messages to a PO catalogue
  A translator gets a PO file: each message once, in ICU MessageFormat,
  with where it is used, its context and the note its author left.
  Extracting again keeps every translation, and keeps a translated message
  that left the sources as obsolete, so nothing a translator did is lost.

  Background:
    Given the file "Main.qml":
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string name: "Ada"
        property string hello: t`Hello ${name}`
        property string open: t({ message: "Open", context: "menu", comment: "The File menu" })
      }
      """

  Scenario: Every message is written once, sorted, with where it is used
    When the author runs "ljos i18n extract . -o is.po --language is"
    Then the file "is.po" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "Language: is\n"
      "X-Generator: ljos i18n extract\n"

      #: Main.qml:5
      msgid "Hello {name}"
      msgstr ""

      #. The File menu
      #: Main.qml:6
      msgctxt "menu"
      msgid "Open"
      msgstr ""
      """

  Scenario: An id and a count are written the way a translator reads them
    Given the file "Main.qml":
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property int count: 2
        property string files: plural(count, { one: "# file", other: "# files" })
        property string home: t({ id: "nav.home", message: "Home" })
      }
      """
    When the author runs "ljos i18n extract . -o is.po --language is"
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
      #: Main.qml:6
      msgid "nav.home"
      msgstr ""

      #: Main.qml:5
      msgid "{count, plural, one {# file} other {# files}}"
      msgstr ""
      """

  Scenario: Extracting again keeps translations and keeps a gone one as obsolete
    Given the file "is.po":
      """po
      msgid ""
      msgstr ""
      "Language: is\n"

      msgid "Hello {name}"
      msgstr "Halló {name}"

      msgid "Save"
      msgstr "Vista"
      """
    When the author runs "ljos i18n extract . -o is.po"
    Then the command prints:
      """
      is.po: 2 messages, 1 new, 1 translated, 1 obsolete
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

      #: Main.qml:5
      msgid "Hello {name}"
      msgstr "Halló {name}"

      #. The File menu
      #: Main.qml:6
      msgctxt "menu"
      msgid "Open"
      msgstr ""

      #~ msgid "Save"
      #~ msgstr "Vista"
      """

  Scenario: The check fails when the catalogue is behind the sources
    Given the author has run "ljos i18n extract . -o is.po --language is"
    And the file "Main.qml":
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string bye: t`Goodbye`
      }
      """
    When the author runs "ljos i18n extract . -o is.po --check"
    Then the command exits with status 1
    And the command's output mentions "is.po is behind the sources: 1 new, 2 gone"

  Scenario: A template has every message and no translation
    Given the file "is.po":
      """po
      msgid "Hello {name}"
      msgstr "Halló {name}"

      msgid "Save"
      msgstr "Vista"
      """
    When the author runs "ljos i18n extract . -o messages.pot"
    Then the file "messages.pot" is:
      """po
      msgid ""
      msgstr ""
      "MIME-Version: 1.0\n"
      "Content-Type: text/plain; charset=UTF-8\n"
      "Content-Transfer-Encoding: 8bit\n"
      "X-Generator: ljos i18n extract\n"

      #: Main.qml:5
      msgid "Hello {name}"
      msgstr ""

      #. The File menu
      #: Main.qml:6
      msgctxt "menu"
      msgid "Open"
      msgstr ""
      """

  Scenario: A module's messages are extracted, and qsTr calls are left to their own catalogue
    Given the file "greeting.ts":
      """ts
      import { t } from "@ljos/core";

      export function greet(name: string): string {
        return t`Hi ${name}`;
      }
      """
    And the file "Old.qml":
      """qml
      import QtQuick
      Item {
        property string quit: qsTr("Quit")
      }
      """
    When the author runs "ljos i18n extract . -o is.po --language is"
    Then the command prints:
      """
      1 qsTr call(s) go to a .ts catalogue: ljos migrate rewrites them as t messages
      is.po: 3 messages, 3 new, 0 translated, 0 obsolete
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

      #: Main.qml:5
      msgid "Hello {name}"
      msgstr ""

      #: greeting.ts:4
      msgid "Hi {name}"
      msgstr ""

      #. The File menu
      #: Main.qml:6
      msgctxt "menu"
      msgid "Open"
      msgstr ""
      """
