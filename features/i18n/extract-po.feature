# Sources:
#   docs/ARCHITECTURE.md (§15 Localisation: catalogues, extraction)
#   https://lingui.dev/ref/catalog-formats
#   https://www.gnu.org/software/gettext/manual/html_node/PO-Files.html
#   https://unicode-org.github.io/icu/userguide/format_parse/messages/
# `ljos i18n extract` is Ljós's own; Qt's lupdate reads qsTr, which catalogue-comments.feature covers.

@ljos-only @headless @backlog
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
        property int count: 2
        property string name: "Ada"
        property string hello: t`Hello ${name}`
        property string files: plural(count, { one: "# file", other: "# files" })
        property string open: t({ message: "Open", context: "menu", comment: "The File menu" })
        property string home: t({ id: "nav.home", message: "Home" })
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

      #: Main.qml:6
      msgid "Hello {name}"
      msgstr ""

      #. The File menu
      #: Main.qml:8
      msgctxt "menu"
      msgid "Open"
      msgstr ""

      #. js-lingui-explicit-id
      #. Default: Home
      #: Main.qml:9
      msgid "nav.home"
      msgstr ""

      #: Main.qml:7
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
      is.po: 4 messages, 3 new, 1 translated, 1 obsolete
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

      #: Main.qml:6
      msgid "Hello {name}"
      msgstr "Halló {name}"

      #. The File menu
      #: Main.qml:8
      msgctxt "menu"
      msgid "Open"
      msgstr ""

      #. js-lingui-explicit-id
      #. Default: Home
      #: Main.qml:9
      msgid "nav.home"
      msgstr ""

      #: Main.qml:7
      msgid "{count, plural, one {# file} other {# files}}"
      msgstr ""

      #~ msgid "Save"
      #~ msgstr "Vista"
      """

  Scenario: The check fails when the catalogue is behind the sources
    When the author runs "ljos i18n extract . -o is.po --language is"
    And the file "Main.qml" is saved on disk as:
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        property string bye: t`Goodbye`
      }
      """
    And the author runs "ljos i18n extract . -o is.po --check"
    Then the command exits with status 1
    And the command's output mentions "is.po is behind the sources: 1 new, 4 gone"
