# Typography

Text is most of an interface. The rules check that it fits, that it can be read in full, and that it follows the application's font and scale. The audit draws every text at font scales `limits::audit::FONT_SCALES` (up to 200 %, which Apple and WCAG 1.4.4 both ask for), so text that fits at 100 % and not at 200 % is found.

## A type scale

Use a few sizes, each with its own line height, and set hierarchy with weight and colour before size. Material 3's scale is a sound default (size / line height in px):

| Role | Size / line height | Use |
|---|---|---|
| Headline small | 24 / 32 | Page titles |
| Title large | 22 / 28 | Dialog titles |
| Body large | 16 / 24 | Reading text |
| Body medium | 14 / 20 | Most interface text |
| Body small | 12 / 16 | Secondary lines, captions |
| Label small | 11 / 16 | The smallest label |

Desktop platforms run a little smaller (macOS's body is 13 pt). Nothing goes under `limits::TEXT_MIN_PX`. Line height for paragraphs is about 1.5 times the size, and a paragraph is about 60 to 75 characters wide on a desktop. Text must survive the spacing WCAG 1.4.12 lets a user set: line height 1.5, paragraph spacing 2, letter spacing 0.12 and word spacing 0.16 times the size.

## Rules

<a id="text-clipped"></a>
### `text-clipped`

**Error, runtime.** Text is cut off — by its parent's clip, a sibling or the window edge — without eliding or wrapping, at some font scale or window size. Half a word is worse than an ellipsis: nobody can tell anything is missing.

```qml
// Bad: a fixed-height box that 200 % text overflows
Rectangle { height: 24; clip: true; Label { text: qsTr("Synchronising your library") } }
// Good: the box takes its height from its text
Pane { Label { width: parent.width; text: qsTr("Synchronising your library"); wrapMode: Text.Wrap } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html , https://developer.apple.com/design/human-interface-guidelines/typography

<a id="text-elided-hidden"></a>
### `text-elided-hidden`

**Warning, runtime.** Text is elided and the full text is nowhere a user can get it. A list of `Project report – fi…` rows cannot be told apart.

```qml
// Bad
Label { width: 160; elide: Text.ElideRight; text: model.title }
// Good
Label {
    id: title
    width: 160; elide: Text.ElideRight; text: model.title
    HoverHandler { id: hover }
    ToolTip.visible: title.truncated && hover.hovered
    ToolTip.text: model.title
}
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/reflow.html , https://vercel.com/design/guidelines

<a id="text-unbounded-dynamic"></a>
### `text-unbounded-dynamic`

**Warning, static.** Text from a binding (a name, a translation, a count) sits in a box of bounded width with neither `elide` nor `wrapMode`. It fits the test data and overflows the first long name, or the German translation.

```qml
// Bad
Label { Layout.preferredWidth: 120; text: model.name }
// Good
Label { Layout.preferredWidth: 120; text: model.name; elide: Text.ElideRight }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html , https://vercel.com/design/guidelines

<a id="text-raw-markup"></a>
### `text-raw-markup`

**Warning, runtime.** Plain text shows markup as characters: backticks, `**bold**`, `<b>` tags or an internal envelope. It looks broken and it buries the words.

```qml
// Bad
Label { text: "**Saved** to `notes.txt`" }
// Good
Label { textFormat: Text.MarkdownText; text: "**Saved** to `notes.txt`" }
```

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/ (aesthetic and minimalist design)

<a id="text-font-literal"></a>
### `text-font-literal`

**Hint, static.** A text item sets `font.family` to a literal, so it stops following the application's font, the platform font, and any font a user picked for legibility. Code and tabular text may want a monospace face; take it from the theme too.

```qml
// Bad
Label { font.family: "Helvetica" }
// Good: leave it unset and it inherits the window's font
Label { text: qsTr("Notes") }
```

Sources: https://developer.apple.com/design/human-interface-guidelines/typography , https://m3.material.io/styles/typography/applying-type

<a id="text-too-small"></a>
### `text-too-small`

**Warning, static and runtime.** Text is drawn smaller than `limits::TEXT_MIN_PX`, the smallest size any platform allows for reading (macOS's 10 pt; iOS stops at 11 pt). Small text used to make a line "secondary" is just text people cannot read; make it secondary with weight or a muted colour that still passes `color-text-contrast`.

```qml
// Bad
Label { text: qsTr("Updated just now"); font.pixelSize: 9 }
// Good
Label { text: qsTr("Updated just now"); font.pixelSize: 12; color: palette.placeholderText }
```

Sources: https://developer.apple.com/design/human-interface-guidelines/typography , https://m3.material.io/styles/typography/type-scale-tokens

## Not checked yet

- **Line length** (proposed `text-line-length`): a paragraph wider than about 75 characters is hard to track back to the next line; set `Layout.maximumWidth` on reading text.
- **Tabular figures** (proposed `text-tabular-figures`): numbers that change in place (timers, counters, table columns) should use tabular figures (`font.features: { "tnum": 1 }`) so they do not jitter.
- **Don't change weight on hover**: the text reflows and the row jumps (Rauno Freiberg). https://interfaces.rauno.me/

## Sources

- WCAG 2.2, resize text, reflow and text spacing: https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html , https://www.w3.org/WAI/WCAG22/Understanding/reflow.html , https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html
- Material 3 type scale: https://m3.material.io/styles/typography/type-scale-tokens
- Apple HIG typography: https://developer.apple.com/design/human-interface-guidelines/typography
- Vercel web interface guidelines: https://vercel.com/design/guidelines
- Butterick's Practical Typography, line length: https://practicaltypography.com/line-length.html
