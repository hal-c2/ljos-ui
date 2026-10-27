---
title: Localisation
description: Translate text and plurals with qsTr, format locale-aware numbers, switch UI languages at run time, and mirror layouts for right-to-left scripts.
section: controls
order: 8
---
# Localisation

Use `qsTr()`, `String.arg()`, `Qt.locale()`, `Qt.uiLanguage`, and `LayoutMirroring` to translate user-interface strings, handle plural forms, format numbers with locale-specific digits and separators, and mirror layouts for right-to-left languages.

## Translating text and plurals with `qsTr`

Wrap user-facing strings in `qsTr(source, disambiguation, count)`. When the active catalogue has a translation for the current UI language, `qsTr` returns it; when no translation is found, it falls back to the source string as written. If a call includes a disambiguation comment that is not in the catalogue, Ljós falls back to the translation without a comment. A marker function such as `QT_TR_NOOP` marks text for extraction without translating it on the spot, so a later `qsTr` call on the marked value does the actual lookup.

For plural strings, pass the count as the third argument to `qsTr`:

- `%n` is replaced with the count in the plural form chosen for that language (if you omit the count argument, `%n` remains literally in the string).
- `%Ln` formats the count using the active locale's digit grouping.
- Plural rules follow ICU4X `PluralRules` with a small override table where Qt's numerus forms differ from CLDR—for example, Icelandic uses two forms, where the first form applies to counts ending in `1` except `11`. Strings looked up by ID also support plural forms, while a translation entry written without plural forms serves only its first form.

Write this as `Main.qml` and run `ljos check` on its directory:

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 8
    property int count: 3

    Label {
        text: qsTr("%n file(s)", "", count)
    }

    Button {
        text: qsTr("Save")
    }
}
```

With no catalogue loaded, the label reads `"3 file(s)"`. When an Icelandic catalogue is active, `count: 3` selects the plural form provided by the translator.

Translators edit standard Qt `.ts` files, which Ljós reads and writes while preserving comments and source locations. Catalogue entries are keyed by context (the QML file stem or a `Translator` pragma), source text, and disambiguation. During development, Ljós reads and watches `.ts` files directly so translations hot-reload; at run time, it loads memory-mapped binary `.ljl` catalogues and walks a language fallback chain such as `is-IS` → `is` → `en`.

## Switching `Qt.uiLanguage` and mirroring layouts

The UI language (`Qt.uiLanguage`) and the formatting locale (`Qt.locale()`) are separate settings, though both initialise from the system environment at startup.

Assigning a new language tag to `Qt.uiLanguage` immediately re-evaluates every translated binding and function on screen, and any components created afterwards start in the new language (if `Qt.uiLanguage` has not been set to a translated language, source strings are shown).

The language tag may be written with an underscore or a hyphen, and a more specific tag finds a catalogue for a less specific one: `de_AT` and `de-CH` both use a `de` catalogue. Assigning the empty string means no language, so source strings are shown. At startup `Qt.uiLanguage` and `Qt.locale()` come from `LC_ALL`, then `LC_MESSAGES`, then `LANG`; `Qt.uiLanguage` has a change signal, `uiLanguageChanged`.

`Qt.application.layoutDirection` follows `Qt.uiLanguage` automatically: switching to a right-to-left language sets `Qt.application.layoutDirection` to `Qt.RightToLeft`, and switching back restores left-to-right. However, items do not mirror their positions automatically unless you ask them to. Bind `LayoutMirroring.enabled` to `Qt.application.layoutDirection` on the container you want to mirror:

```qml
import QtQuick
import QtQuick.Controls.Basic

Row {
    padding: 12
    spacing: 8
    LayoutMirroring.enabled: Qt.application.layoutDirection === Qt.RightToLeft

    Button {
        text: qsTr("Back")
    }

    Button {
        text: qsTr("Next")
    }
}
```

## Formatting numbers and locales

You can insert values into translated strings with `String.arg()`, which replaces numbered placeholders `%1` through `%9` (up to two-digit markers, filling the lowest-numbered placeholder first using `QString::arg` semantics).

To format numbers according to the locale's digit grouping and numeral system:

- Use `"%L1".arg(number)` for locale-grouped numbers in placeholder strings.
- Call `Qt.locale().toString(number)` (or pass a partial or complete locale name to `Qt.locale(name)`) to format a number using that locale's digits and separators—for example, Arabic locales use Arabic-Indic digits, English groups `1234567` as `"1,234,567"`, and German groups it as `"1.234.567"`.
- Controls and delegates created early (before the translation catalogue finishes loading) still resolve the active locale properly.

```qml
import QtQuick
import QtQuick.Controls.Basic

Column {
    padding: 12
    spacing: 8

    Label {
        text: "%L1 kg".arg(1234567)
    }

    Label {
        text: qsTr("Total: %1").arg(Qt.locale().toString(1234567))
    }
}
```

## Differences from Qt

- Ljós does not read `.qm` files; it reads `.ts` files directly in development (with hot-reload) and its own binary `.ljl` catalogue format at run time.
- `Qt.uiLanguage` is a writable `QString` property of the `Qt` object with a `uiLanguageChanged` signal, as in Qt, and assigning it re-translates every translated binding.

## See also

- [Quick Controls Overview](./overview.md)
- [Value Inputs](./value-inputs.md)
