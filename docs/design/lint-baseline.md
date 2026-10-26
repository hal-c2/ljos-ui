# Static lint baseline

What the static design lints (`ljos check`, `qml_lint`) find in QML nobody
wrote for them, per rule. A change to a check shows up as a change in its
count, and a count far above the rest says to read that rule's findings for
false positives before trusting it. These are not targets. Code that is not
your own is reported, never gated (`ljos check --lint=warn`).

## How to reproduce it

```text
cargo run -q -p ljos_mcp_author --example lint_baseline -- corpus <other tree>…
cargo run -q -p ljos_mcp_author --example lint_baseline -- corpus --list <code>
```

Each file is linted with its own directory as the project. The first form
prints the tables below. The second prints every finding of one code, so you
can read through them.

## Counts

`corpus/` (Qt documentation snippets and demos): 526 files, 0 not analysed

| code | severity | findings | files |
|---|---|---:|---:|
| `a11y-icon-button-no-tooltip` | warning | 28 | 14 |
| `a11y-placeholder-label` | warning | 3 | 3 |
| `a11y-unnamed-icon-button` | error | 18 | 12 |
| `a11y-unnamed-image` | warning | 201 | 90 |
| `a11y-unnamed-interactive` | error | 48 | 37 |
| `color-literal` | hint | 648 | 185 |
| `color-scheme-ignored` | warning | 145 | 95 |
| `content-menu-ellipsis` | hint | 2 | 1 |
| `content-vague-action` | hint | 2 | 1 |
| `focus-destructive-undo` | hint | 1 | 1 |
| `focus-pointer-only` | warning | 42 | 32 |
| `layout-fixed-size` | warning | 6 | 6 |
| `layout-spacing-scale` | hint | 416 | 119 |
| `motion-bounce` | warning | 15 | 12 |
| `motion-default-easing` | warning | 77 | 32 |
| `motion-duration` | warning | 75 | 26 |
| `motion-ease-in-enter` | error | 1 | 1 |
| `motion-frequent` | warning | 3 | 2 |
| `motion-layout-property` | warning | 32 | 7 |
| `motion-reduced-ignored` | hint | 22 | 8 |
| `motion-scale-from-zero` | error | 2 | 2 |
| `text-font-literal` | hint | 111 | 56 |
| `text-unbounded-dynamic` | warning | 21 | 13 |

A desktop application's QML: 127 files, 0 not analysed

| code | severity | findings | files |
|---|---|---:|---:|
| `a11y-placeholder-label` | warning | 10 | 8 |
| `a11y-unnamed-image` | warning | 5 | 5 |
| `a11y-unnamed-interactive` | error | 24 | 14 |
| `color-literal` | hint | 6 | 3 |
| `color-scheme-ignored` | warning | 1 | 1 |
| `content-vague-action` | hint | 3 | 2 |
| `focus-destructive-undo` | hint | 11 | 9 |
| `focus-hover-only` | warning | 1 | 1 |
| `focus-pointer-only` | warning | 22 | 13 |
| `layout-fixed-size` | warning | 1 | 1 |
| `layout-spacing-scale` | hint | 236 | 63 |
| `motion-default-easing` | warning | 11 | 8 |
| `motion-frequent` | warning | 1 | 1 |
| `motion-layout-property` | warning | 2 | 1 |
| `text-font-literal` | hint | 5 | 5 |
| `text-unbounded-dynamic` | warning | 32 | 24 |

Measured on 2026-10-26.

## False positives found here and fixed

- `layout-anchors-in-layout` fired on an item a layout holds in its source
  that names another `parent:`. The layout never lays that item out, so its
  anchors are right. The fixtures `layout-anchors-in-layout/reparented.*`
  cover it.
- `a11y-unnamed-interactive` (an error) fired on handlers that are a
  second way to something (13 of 34 findings in the desktop application when the first baseline was taken):
  - a secondary button (`acceptedButtons: Qt.RightButton` for a context menu);
  - a click with a modifier held (`acceptedModifiers`);
  - a double click alone (a title bar that maximises).

  An error has to be certain. The plain way in is the one that has to be
  named. The fixtures `a11y-unnamed-interactive/second-way.*` cover it.
- `color-scheme-ignored` reported once per file. It now reports each
  surface whose `color` binding is filled with written, opaque colours and
  names no palette, colour scheme or theme.

## The second batch

Nine rules were added (`motion-reduced-ignored`, `motion-flash`,
`motion-endless`, `motion-origin`, `a11y-placeholder-label`,
`content-vague-action`, `content-menu-ellipsis`, `layout-nested-radius`,
`focus-destructive-undo`). Over these trees `motion-flash`, `motion-endless`,
`motion-origin` and `layout-nested-radius` find nothing: only their fixtures
exercise them. The desktop application's tree has moved on since the first
table (127 files now, three more `a11y-unnamed-interactive`), so the old
rows are not comparable to the letter.

Up to ten findings of each of the others were read:

- `motion-reduced-ignored` (22, all in game demos): sprites that idle for
  ever and a logo that pulses. All real, and a hint. One false positive was
  found and fixed: a hand-built busy indicator (`BusyIndicator.qml` spinning
  a `Rectangle`) was reported. A file named for a busy indicator, spinner,
  loading or progress display is now treated as the indicator the built-in
  one is.
- `a11y-placeholder-label` (3 in the corpus, 10 in the desktop application):
  all fields with a placeholder and no name or `Label`.
- `content-vague-action`: `Yes` and `No` in a dialog style, `Continue` in a
  wizard. All real.
- `content-menu-ellipsis`: `Open File` and `Open URL` in a menu bar that
  open a dialog. Both real.
- `focus-destructive-undo` (11 in the desktop application, 1 in the corpus):
  a button that deletes or removes at once through a dispatch the file does
  not show a confirmation for; the check cannot see a confirmation made
  elsewhere, so it stays a hint. One false positive was found and fixed:
  `Clear color`, which resets a setting. `color`, `colour`, `field`,
  `filter`, `search`, `selection` and `query` after the verb now exempt it.

The errors left in the corpus were read one by one, and each is real:

- `motion-ease-in-enter`: a `Behavior on y` with `Easing.InQuad`.
- `motion-scale-from-zero`: scales from `0` and from `0.1`.
- `a11y-unnamed-icon-button`: icon-only buttons with no `text`, `action` or
  `Accessible.name`.
