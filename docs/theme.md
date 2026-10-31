# The theme file

A theme file sets the roles of `Ljos.Design` (`Colors`, `Spacing`,
`Typography`, `Motion`) without a change to the QML and without Rust. This
page is the contract: every runtime that reads a theme (the desktop runner,
`ljos render`, `ljos audit`, the dev host, ljos-web) finds the same file and
reads it the same way. How the native runner applies it, and how a running
application takes a changed one, is in `ARCHITECTURE.md` section 12.1
(Theme files).

## Which file is read

1. A file named on the command line (`ljos run|render|audit --theme FILE`,
   or the host's `open_themed` with a path) is read in place of any other. A
   named file that is not there is an error.
2. Otherwise the file is `theme.json`, looked for in the entry's directory,
   then in each directory above it up to the project's, and the **nearest
   one is read**. A module's own theme beside its entry therefore wins over
   the project's.
3. **Nothing above the project is read.** The project is `--project DIR`,
   or the entry's directory when none is given (so a bare `ljos run
   app/Main.qml` reads only `app/theme.json`). An application with several
   roots looks from its first entry up to its first root.
4. No file: the module's own roles. A run that watches its files and found
   none watches `theme.json` at the project's root, so one written there is
   picked up.

Only one file is read; themes are not merged.
`ljos_design::theme::find(entry_dir, project)` is the search; a runtime
without a filesystem (the web) looks the same names up in what it was
given, in the same order.

## The format

A JSON object with at most one object per singleton, each mapping a role to
its value. Every key is optional; a role the theme does not give keeps the
module's value.

```json
{
  "Colors": {
    "seedHue": 280,
    "accent": { "light": "#6a1b9a", "dark": "plum" },
    "scrim": "#80000000"
  },
  "Spacing": { "md": 20 },
  "Typography": { "bodySize": 15 },
  "Motion": { "press": 90, "easeOut": [0.2, 0, 0, 1, 1, 1] }
}
```

| Singleton | Roles | Value |
|---|---|---|
| `Colors` | `seedHue` | a number, the hue the palette is made from |
| `Colors` | the colour roles below | a colour, or an object with any of `light`, `dark`, `lightHigh`, `darkHigh`, each a colour |
| `Spacing` | `unit`, `none`, `xxs`, `xs`, `sm`, `md`, `lg`, `xl`, `xxl`, `xxxl` | a whole number of pixels, at least 0 |
| `Typography` | `{step}Size`, `{step}LineHeight` | a whole number of pixels, at least 0 |
| `Typography` | `{step}Weight` | a whole number, 1 to 1000 |
| `Motion` | the durations, and `press` | a whole number of milliseconds, at least 0 |
| `Motion` | `easeOut`, `easeOutSoft`, `easeInOut`, `decelerate`, `drawer`, `linear` | a list of numbers, as `bezierCurve` takes |
| `Motion` | `enterScale`, `pressScale`, `gesture{Small,Panel,Screen}{Spring,Damping,Mass,Epsilon}` | a number, at least 0 |

The colour roles: `canvas`, `surface`, `surfaceRaised`, `surfaceHover`,
`surfacePressed`, `surfaceSelected`, `border`, `borderStrong`, `focusRing`,
`accent`, `accentHover`, `accentPressed`, `textOnAccent`, `text`,
`textMuted`, `textDisabled`, `link`, `danger`, `textOnDanger`, `warning`,
`textOnWarning`, `success`, `textOnSuccess`, `info`, `textOnInfo`, `scrim`,
`shadow`.

The type steps (`{step}`): `caption`, `body`, `bodyStrong`, `subtitle`,
`title`, `headline`, `display`.

The durations are `{kind}{Size}{Frequency}`: kind one of `enter`, `exit`,
`move`, `expand`, `collapse`, `sheetIn`, `sheetOut`, `gesture`, `loop`;
size `Small`, `Panel` or `Screen`; frequency `Rare`, nothing, or
`Frequent` (`enterSmall`, `moveScreenRare`, `sheetInPanelFrequent`).

`ljos_design::theme::roles()` is the list of record: the module's QML files
are generated from it and a test holds the two together, so a role added
there is a role here.

A colour is a string QML takes as a colour: `#rgb`, `#rrggbb`, `#aarrggbb`
(alpha first, as QML has it) or an SVG colour name. A single string sets
light and dark and leaves both high-contrast variants to the module; a
variant the object does not give is the module's own.

## What is refused

A theme with any of these is refused whole; every mistake is told as
`file:line:column: message`, and a running application keeps the theme it
had.

- JSON that cannot be read, or a key given twice.
- A singleton or role the module does not have (`Colors has no role
  "acent"`).
- A value of the wrong kind or out of range (`Colors.accent is a colour: a
  string such as "#6a1b9a"`).
- The user's settings, which are not the theme's to set: `Colors.dark`,
  `Colors.highContrast`, `Motion.reduced`. `Colors.table` and
  `Colors.roles` are not roles.
- A colour that breaks the contrast the palette promises. Each of the four
  variants is checked as the module checks its own palette (WCAG 2.2: 4.5:1
  for text at standard contrast, more at high contrast, 3:1 for marks and
  borders), against the palette at the application's seed hue, rounded to the
  30° step the module ships, with the theme's colours put in. Only a pair
  that includes one of the theme's colours is told, at that colour's line:
  `Colors.text on canvas is 1.61:1 in light; text needs 4.5:1`. A ground
  (`canvas`, `surface`, …) the theme gives must be opaque. A theme that
  gives no colour is not checked.

## What a runtime does with it

Each role overrides the singleton's property, so whatever binds to it
follows, and taking the theme away restores the module's value. A colour
with variants follows `Colors.dark` and `Colors.highContrast` as the
module's own colours do. Nothing is made again: changing the theme keeps
all state.
