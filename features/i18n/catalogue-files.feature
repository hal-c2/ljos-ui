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
        Text {
          text: t`Open`
          Component.onCompleted: console.warn("shown: " + text)
        }
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
    When the author runs "ljos render . --out translated.png --settle 0 --language is"
    Then the command's output mentions "shown: Opna"

  Scenario: A language without a catalogue keeps the source text
    When the author runs "ljos render . --out untranslated.png --settle 0 --language de"
    Then the command's output mentions "shown: Open"

  Scenario: A catalogue without a language is named by its file
    Given the file "i18n/fr.po":
      """po
      msgid "Open"
      msgstr "Ouvrir"
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language fr"
    Then the command's output mentions "shown: Ouvrir"

  Scenario: A catalogue that does not read is said, and the others still load
    Given the file "i18n/de.po":
      """po
      msgid "Open
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language is"
    Then the command's output mentions "i18n/de.po"
    And the command's output mentions "shown: Opna"

  Scenario: An i18n that is not a directory is said, and the application still runs
    Given the file "app/Main.qml":
      """qml
      import Ljos.Ui
      import Ljos.Core
      Item { Component.onCompleted: console.warn("shown: " + t`Open`) }
      """
    And the file "app/i18n":
      """
      not a directory
      """
    When the author runs "ljos render app --out app.png --settle 0 --language is"
    Then the command's output mentions "no catalogue is loaded"
    And the command's output mentions "shown: Open"

  Scenario: A TypeScript module kept in i18n is not a catalogue
    Given the file "i18n/messages.ts":
      """ts
      import { msg } from "@ljos/core";

      export const open = msg`Open`;
      """
    When the author runs "ljos render . --out translated.png --settle 0 --language is"
    Then the command's output does not mention "messages.ts"
    And the command's output mentions "shown: Opna"
