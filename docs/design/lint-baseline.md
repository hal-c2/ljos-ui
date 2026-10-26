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
| `a11y-unnamed-icon-button` | error | 18 | 12 |
| `a11y-unnamed-image` | warning | 201 | 90 |
| `a11y-unnamed-interactive` | error | 48 | 37 |
| `color-literal` | hint | 648 | 185 |
| `color-scheme-ignored` | warning | 145 | 95 |
| `focus-pointer-only` | warning | 42 | 32 |
| `layout-fixed-size` | warning | 6 | 6 |
| `layout-spacing-scale` | hint | 416 | 119 |
| `motion-bounce` | warning | 15 | 12 |
| `motion-default-easing` | warning | 77 | 32 |
| `motion-duration` | warning | 75 | 26 |
| `motion-ease-in-enter` | error | 1 | 1 |
| `motion-frequent` | warning | 3 | 2 |
| `motion-layout-property` | warning | 32 | 7 |
| `motion-scale-from-zero` | error | 2 | 2 |
| `text-font-literal` | hint | 111 | 56 |
| `text-unbounded-dynamic` | warning | 21 | 13 |

A desktop application's QML: 129 files, 0 not analysed

| code | severity | findings | files |
|---|---|---:|---:|
| `a11y-unnamed-image` | warning | 5 | 5 |
| `a11y-unnamed-interactive` | error | 21 | 13 |
| `color-literal` | hint | 6 | 3 |
| `color-scheme-ignored` | warning | 1 | 1 |
| `focus-hover-only` | warning | 1 | 1 |
| `focus-pointer-only` | warning | 22 | 13 |
| `layout-fixed-size` | warning | 1 | 1 |
| `layout-spacing-scale` | hint | 235 | 63 |
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
  second way to something (13 of 34 findings in the desktop application):
  - a secondary button (`acceptedButtons: Qt.RightButton` for a context menu);
  - a click with a modifier held (`acceptedModifiers`);
  - a double click alone (a title bar that maximises).

  An error has to be certain. The plain way in is the one that has to be
  named. The fixtures `a11y-unnamed-interactive/second-way.*` cover it.
- `color-scheme-ignored` reported once per file. It now reports each
  surface whose `color` binding is filled with written, opaque colours and
  names no palette, colour scheme or theme.

The errors left in the corpus were read one by one, and each is real:

- `motion-ease-in-enter`: a `Behavior on y` with `Easing.InQuad`.
- `motion-scale-from-zero`: scales from `0` and from `0.1`.
- `a11y-unnamed-icon-button`: icon-only buttons with no `text`, `action` or
  `Accessible.name`.
