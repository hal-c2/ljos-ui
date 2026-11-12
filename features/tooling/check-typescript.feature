# Sources:
#   docs/DIALECT.md §4.4 (the declarations are generated from the table of types the checker reads)
#   https://www.typescriptlang.org/docs/handbook/declaration-files/introduction.html
#   https://www.typescriptlang.org/tsconfig/#strict
# `Ljos.Ui`, its declarations and its checker are Ljós's own: Qt has nothing to compare them with.
# A scenario that needs TypeScript passes when the machine has none and the check says it skipped it
# (`SKIPPED: no tsc`): `deno` on the PATH, or the program `LJOS_TSC` names, is TypeScript.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Ljós's API as TypeScript declarations, and its scripts checked by TypeScript
  The scripts of a file in Ljós's API are TypeScript, and the types they
  read and write are declared for TypeScript from the same table the
  checker reads: a renamed member is renamed in both. A file TypeScript
  accepts is one the checker accepts, so TypeScript is the checker's
  oracle, and `ljos check --tsc` asks both.

  Rule: The declarations are the table's

    Scenario: A member Ljós's API renamed is declared by its new name only
      When the declarations of Ljós's API are written
      Then the declarations have the line "interface Rectangle extends Item {"
      And the declaration of "Rectangle" has the member "fill: string;"
      And the declaration of "Rectangle" has no member "color"

    Scenario: A readonly property is declared readonly and an enum as an enum
      When the declarations of Ljós's API are written
      Then the declaration of "PointerArea" has the member "readonly pressed: boolean;"
      And the declaration of "Text" has the member "align: Text.HAlignment;"
      And the declarations have the line "const AlignLeft: Text.HAlignment;"

  Rule: TypeScript reads a file in Ljós's API as the checker does

    Scenario: A script that keeps to the subset is accepted by TypeScript
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          property bool full: level >= 1
          function fill(by: real) { if (full === false) { level = level + by } }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing

    Scenario: TypeScript's error is reported where it is in the QML file
      Given the file "Gauge.qml":
        """qml
        import Ljos.Ui
        Item {
          property real level: 0
          function reset() { level = "0" }
        }
        """
      When the files are checked with TypeScript
      Then the check reports the lines:
        """
        Gauge.qml:4:38: error: `level` is a real; this gives it a string [ts-assign-type]
        """
      And TypeScript reports the lines:
        """
        Gauge.qml:4:30: error: Type 'string' is not assignable to type 'number'. [TS2322]
        """

    # What a handler is handed when its signal says no more than an object goes by the
    # class TypeScript declares it as: `QtObject`, or beside Qt's table Qt's `QObject`.
    @no-qt-table
    Scenario: A handler is given the types of its signal's arguments
      Given the file "Pad.qml":
        """qml
        import Ljos.Ui
        PointerArea {
          property string said: ""
          onClicked: mouse => { said = mouse }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports the lines:
        """
        Pad.qml:4:33: error: Type 'QtObject' is not assignable to type 'string'. [TS2322]
        """

    @qt-table
    Scenario: A handler is given the types of its signal's arguments, beside Qt's table
      Given the file "Pad.qml":
        """qml
        import Ljos.Ui
        PointerArea {
          property string said: ""
          onClicked: mouse => { said = mouse }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports the lines:
        """
        Pad.qml:4:33: error: Type 'QObject' is not assignable to type 'string'. [TS2322]
        """

    # `Timer` is Ljos.Core's, which the file does not import; TypeScript suggests the
    # declared name nearest the one it cannot find.
    @no-qt-table
    Scenario: A type a file writes is one of the modules it imports, for TypeScript too
      Given the file "Holder.qml":
        """qml
        import Ljos.Ui
        Item {
          property Timer held
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports the lines:
        """
        Holder.qml:3:11: error: Cannot find name 'Timer'. Did you mean 'TimerId'? [TS2552]
        """

    # `TextEdit` declares the `HAlignment` of the base it shares with `TextInput` again,
    # with `AlignJustify`, as Qt's `TextEdit` alone has it: the `horizontalAlignment` it
    # inherits holds its own.
    Scenario: A property holds the enum its type declares again
      Given the file "Page.qml":
        """qml
        import Ljos.Ui
        Item {
          TextEdit { id: body; horizontalAlignment: TextEdit.AlignJustify }
          function left(): void { body.horizontalAlignment = TextEdit.AlignLeft }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing

    # QML reads a key of an enum a type inherits through the type's own name, as qml6 does
    # (`TextInput.AlignRight` is 2): without Qt's table the editors inherit their enums from
    # the base they share, beside it every item inherits `Item`'s.
    Scenario: A key of an enum the type inherits is read through the type's name
      Given the file "Page.qml":
        """qml
        import Ljos.Ui
        Item {
          TextInput { id: field; horizontalAlignment: TextInput.AlignRight }
          TextEdit { wrapMode: TextEdit.Wrap; horizontalAlignment: TextEdit.AlignJustify }
          Rectangle { transformOrigin: Rectangle.TopLeft }
          function wrap(): void { field.wrapMode = TextInput.WordWrap }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing

    Scenario: A key only another type's enum has is no key of the type
      Given the file "Field.qml":
        """qml
        import Ljos.Ui
        TextInput {
          horizontalAlignment: TextInput.AlignJustify
        }
        """
      When the files are checked with TypeScript
      Then the check reports the lines:
        """
        Field.qml:3:42: error: `AlignJustify` has no type: the type TextInput has no member of that name [ts-no-any]
        """
      And TypeScript reports the lines:
        """
        Field.qml:3:42: error: Property 'AlignJustify' does not exist on type 'typeof TextInput'. [TS2339]
        """

    Scenario: A type of the project's own a file writes is found, for TypeScript too
      Given the file "Card.qml":
        """qml
        import Ljos.Ui
        Rectangle {}
        """
      And the file "Deck.qml":
        """qml
        import Ljos.Ui
        Item {
          component Slot: Item {}
          property Card top
          property list<Slot> slots
        }
        """
      When the files are checked
      Then the check reports:
        """
        2 files, 0 errors, 0 warnings
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing

  Rule: A file in Qt's QML is not given to TypeScript

    Scenario: The same script in Qt's QML has nothing for TypeScript to report
      Given the file "Gauge.qml":
        """qml
        import QtQuick
        Item {
          property real level: 0
          function reset() { level = "0" }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing

    Scenario: A file whose scripts are still JavaScript is not given to TypeScript
      Given the file "Gauge.qml":
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        Item {
          property real level: 0
          function reset() { level = "0" }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing
