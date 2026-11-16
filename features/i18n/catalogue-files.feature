# Sources:
#   docs/ARCHITECTURE.md (§15 Localisation: catalogues)
#   https://lingui.dev/ref/catalog-formats
#   https://www.gnu.org/software/gettext/manual/html_node/PO-Files.html
# Where an application keeps its catalogues is Ljós's own convention; a Qt program loads its
# .qm files itself with a QTranslator, which catalogue.feature covers.

@ljos-only @headless
Feature: An application finds the catalogues beside it
  The catalogues of an application are in the directory `i18n` beside its
  main file, one file per language: `is.po` as `ljos i18n extract` writes
  it, a `.ts` file, or a compiled `.ljl`. Running or rendering the
  application loads every one of them, and the UI language picks among
  them as it does among catalogues installed any other way.

  Background:
    Given the file "Main.qml":
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item {
        width: 120; height: 40
        Text { text: t`Open` }
      }
      """
    And the file "i18n/is.po":
      """po
      msgid ""
      msgstr ""
      "Language: is\n"

      msgid "Open"
      msgstr "Opna"
      """

  Scenario: The UI language's catalogue translates the application
    Given the file "Opna.qml":
      """qml
      import Ljos.Ui
      Item {
        width: 120; height: 40
        Text { text: "Opna" }
      }
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language is"
    And the author runs "ljos render . --entry Opna.qml --out expected.png --settle 0"
    Then the pictures "translated.png" and "expected.png" have the same pixels

  Scenario: A language without a catalogue keeps the source text
    Given the file "Open.qml":
      """qml
      import Ljos.Ui
      Item {
        width: 120; height: 40
        Text { text: "Open" }
      }
      """
    When the author runs "ljos render . --out untranslated.png --settle 0 --language de"
    And the author runs "ljos render . --entry Open.qml --out expected.png --settle 0"
    Then the pictures "untranslated.png" and "expected.png" have the same pixels

  Scenario: A catalogue without a language is named by its file
    Given the file "i18n/fr.po":
      """po
      msgid "Open"
      msgstr "Ouvrir"
      """
    And the file "Ouvrir.qml":
      """qml
      import Ljos.Ui
      Item {
        width: 120; height: 40
        Text { text: "Ouvrir" }
      }
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language fr"
    And the author runs "ljos render . --entry Ouvrir.qml --out expected.png --settle 0"
    Then the pictures "translated.png" and "expected.png" have the same pixels

  Scenario: A catalogue that does not read is said, and the others still load
    Given the file "i18n/de.po":
      """po
      msgid "Open
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language is"
    Then the command's output mentions "i18n/de.po"

  Scenario: A TypeScript module kept in i18n is not a catalogue
    Given the file "i18n/messages.ts":
      """ts
      import { msg } from "@ljos/core";

      export const open = msg`Open`;
      """
    And the file "Opna.qml":
      """qml
      import Ljos.Ui
      Item {
        width: 120; height: 40
        Text { text: "Opna" }
      }
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language is"
    Then the command's output does not mention "messages.ts"
    When the author runs "ljos render . --entry Opna.qml --out expected.png --settle 0"
    Then the pictures "translated.png" and "expected.png" have the same pixels
