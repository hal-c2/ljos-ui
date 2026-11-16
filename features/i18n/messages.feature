# Sources:
#   docs/ARCHITECTURE.md (§15 Localisation)
#   docs/DIALECT.md (§4.3 Ljos.Core, §4.13 Text)
#   https://lingui.dev/ref/macro
#   https://unicode-org.github.io/icu/userguide/format_parse/messages/
#   https://tc39.es/ecma262/#sec-tagged-templates
# Qt translates with qsTr, which a file of Ljós's API does not have (§4.3.1): these messages are
# Ljós's own, so there is nothing of Qt's to compare with. qsTr in Qt's QML is in catalogue.feature.

@ljos-only @headless
Feature: Messages: translated text written as a tagged template
  A file of Ljós's API translates its text with `t`, a tagged template
  from `Ljos.Core`: `` t`Hello ${name}` ``. The catalogue holds the message
  in ICU MessageFormat (`Hello {name}`), so a translator sees each value by
  name and can move it. Counts and choices are `plural`, `select` and
  `selectOrdinal`. A translated binding is worked out again when the
  language changes, and when a value it shows does.

  Background:
    Given the translation file "is.po":
      """po
      msgid ""
      msgstr ""
      "Language: is\n"

      msgid "Open"
      msgstr "Opna"

      msgid "Hello {name}"
      msgstr "Halló {name}"

      msgid "{count, plural, one {# file} other {# files}}"
      msgstr "{count, plural, one {# skrá} other {# skrár}}"

      msgid "{role, select, admin {Administrator} other {Member}}"
      msgstr "{role, select, admin {Stjórnandi} other {Meðlimur}}"

      msgctxt "menu"
      msgid "Open"
      msgstr "Opna valmynd"

      msgid "nav.home"
      msgstr "Heim"

      msgid "Goodbye {name}"
      msgstr "Bless {nme}"
      """

  Rule: A message is its source text until the UI language has it

    Scenario: A message follows the UI language
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string open: t`Open`
        }
        """
      Then the property "open" of "root" is "Open"
      When the UI language is set to "is"
      Then the property "open" of "root" is "Opna"
      When the UI language is set to ""
      Then the property "open" of "root" is "Open"

    Scenario: A more specific language finds the catalogue of a less specific one
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string open: t`Open`
        }
        """
      When the UI language is set to "is-IS"
      Then the property "open" of "root" is "Opna"

    Scenario: A message the catalogue does not have is shown as written, with its values
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property int left: 4
          property string shown: t`${left} left`
        }
        """
      When the UI language is set to "is"
      Then the property "shown" of "root" is "4 left"

    Scenario: A translation that names a value the message does not have is not used
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string name: "Ada"
          property string bye: t`Goodbye ${name}`
        }
        """
      When the UI language is set to "is"
      Then the property "bye" of "root" is "Goodbye Ada"
      And the log has a warning matching "is\.po:.*Goodbye \{name\}.*\{nme\}"

  Rule: Values are placeholders named after them

    Scenario Outline: Text written in the message is shown as written
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property int count: 3
          property string shown: <message>
        }
        """
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | message                                   | shown        |
        | t`Use {braces}`                           | Use {braces} |
        | t`Don't save`                             | Don't save   |
        | t`'{quoted}'`                             | '{quoted}'   |
        | plural(count, { other: "# in '#'1" })     | 3 in #1      |
        | plural(count, { other: "# don't fit" })   | 3 don't fit  |

    Scenario: A value is shown where the translation puts it, and again when it changes
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string name: "Ada"
          property string hello: t`Hello ${name}`
        }
        """
      When the UI language is set to "is"
      Then the property "hello" of "root" is "Halló Ada"
      When the property "name" of "root" is set to "Jón"
      Then the property "hello" of "root" is "Halló Jón"

  Rule: Counts and choices are written in the message

    Scenario Outline: A count picks the language's plural form
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property int count: <count>
          property string files: plural(count, { one: "# file", other: "# files" })
        }
        """
      Then the property "files" of "root" is "<english>"
      When the UI language is set to "is"
      Then the property "files" of "root" is "<icelandic>"

      Examples:
        | count | english  | icelandic |
        | 1     | 1 file   | 1 skrá    |
        | 3     | 3 files  | 3 skrár   |
        | 21    | 21 files | 21 skrá   |

    Scenario Outline: A number form is chosen before the language's forms
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property int count: <count>
          property string files: plural(count, { 0: "No files", one: "# file", other: "# files" })
        }
        """
      Then the property "files" of "root" is "<shown>"

      Examples:
        | count | shown    |
        | 0     | No files |
        | 1     | 1 file   |
        | 2     | 2 files  |

    Scenario Outline: An ordinal picks the language's ordinal form
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property int place: <place>
          property string shown: selectOrdinal(place, { one: "#st", two: "#nd", few: "#rd", other: "#th" })
        }
        """
      Then the property "shown" of "root" is "<shown>"

      Examples:
        | place | shown |
        | 1     | 1st   |
        | 2     | 2nd   |
        | 3     | 3rd   |
        | 4     | 4th   |
        | 11    | 11th  |
        | 22    | 22nd  |

    Scenario: A choice picks a form by its value, and other when none matches
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string role: "admin"
          property string label: select(role, { admin: "Administrator", other: "Member" })
        }
        """
      When the UI language is set to "is"
      Then the property "label" of "root" is "Stjórnandi"
      When the property "role" of "root" is set to "guest"
      Then the property "label" of "root" is "Meðlimur"

    Scenario: A count inside a sentence is one message
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string name: "Ada"
          property int n: 1000
          property string shown: t`${name} has ${plural(n, { one: "# file", other: "# files" })}`
        }
        """
      Then the property "shown" of "root" is "Ada has 1,000 files"

  Rule: A message can say its context and its id

    Scenario: The same text in another context is another message
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string plain: t`Open`
          property string menu: t({ message: "Open", context: "menu" })
        }
        """
      When the UI language is set to "is"
      Then the property "plain" of "root" is "Opna"
      And the property "menu" of "root" is "Opna valmynd"

    Scenario: A message with an id is found by its id and shows its message until then
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string home: t({ id: "nav.home", message: "Home" })
        }
        """
      Then the property "home" of "root" is "Home"
      When the UI language is set to "is"
      Then the property "home" of "root" is "Heim"

  Rule: The application changes the language

    Scenario: Assigning App.language translates what is on screen again
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property string open: t`Open`
          function speakIcelandic() { App.language = "is" }
        }
        """
      When the method "speakIcelandic" of "root" is called
      Then the property "open" of "root" is "Opna"

  Rule: Messages live in modules too

    Scenario: A module's function translates when a binding calls it
      Given the file "greet.ts":
        """ts
        import { t } from "@ljos/core"
        export function hello(name: string): string {
          return t`Hello ${name}`
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "greet.ts" as Greet
        Item {
          objectName: "root"
          property string hello: Greet.hello("Ada")
        }
        """
      When the UI language is set to "is"
      Then the property "hello" of "root" is "Halló Ada"

    Scenario: A message marked with msg is translated where it is shown
      Given the file "menu.ts":
        """ts
        import { msg, type MessageDescriptor } from "@ljos/core"
        export const items: readonly [MessageDescriptor, MessageDescriptor] =
          [msg`Open`, msg({ id: "nav.home", message: "Home" })]
        """
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import "menu.ts" as Menu
        Item {
          objectName: "root"
          property string first: t(Menu.items[0])
          property string second: t(Menu.items[1])
        }
        """
      Then the property "first" of "root" is "Open"
      When the UI language is set to "is"
      Then the property "first" of "root" is "Opna"
      And the property "second" of "root" is "Heim"

    Scenario: A module's import is bound wherever it is written
      Given the file "greet.ts":
        """ts
        export function hello(name: string): string {
          return t`Hello ${name}`
        }
        import { t } from "@ljos/core"
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "greet.ts" as Greet
        Item {
          objectName: "root"
          property string hello: Greet.hello("Ada")
        }
        """
      When the UI language is set to "is"
      Then the property "hello" of "root" is "Halló Ada"

    Scenario: A parameter named t is the module's own, not the macro
      Given the file "shout.ts":
        """ts
        import { t } from "@ljos/core"
        function render(t: (parts: TemplateStringsArray) => string): string {
          return t`Open`
        }
        export function shout(): string {
          return render((parts) => parts[0].toUpperCase())
        }
        export function open(): string {
          return t`Open`
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "shout.ts" as Shout
        Item {
          objectName: "root"
          property string shouted: Shout.shout()
          property string open: Shout.open()
        }
        """
      When the UI language is set to "is"
      Then the property "shouted" of "root" is "OPEN"
      And the property "open" of "root" is "Opna"

    Scenario: A choice the module binds itself is a value the message shows
      Given the file "role.ts":
        """ts
        import { t } from "@ljos/core"
        function select(role: string, forms: { other: string }): string {
          return `${forms.other} (${role})`
        }
        export function label(role: string): string {
          return t`${select(role, { other: "Member" })}`
        }
        """
      And the QML:
        """qml
        import Ljos.Ui
        import "role.ts" as Role
        Item {
          objectName: "root"
          property string label: Role.label("admin")
        }
        """
      When the UI language is set to "is"
      Then the property "label" of "root" is "Member (admin)"

    Scenario: A value named __proto__ is shown like any other
      Given the file "proto.ts":
        """ts
        import { msg, type MessageDescriptor } from "@ljos/core"
        const __proto__ = "Ada"
        export const greeting: MessageDescriptor = msg`Hello ${__proto__}`
        """
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import "proto.ts" as Proto
        Item {
          objectName: "root"
          property string greeting: t(Proto.greeting)
        }
        """
      Then the property "greeting" of "root" is "Hello Ada"

    Scenario: A message made where t is given it is lowered on its own
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          objectName: "root"
          property int count: 2
          property string files: t(msg`${plural(count, { one: "# file", other: "# files" })}`)
        }
        """
      Then the property "files" of "root" is "2 files"
      When the UI language is set to "is"
      Then the property "files" of "root" is "2 skrár"
