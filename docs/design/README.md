# Design guidelines

These pages are the reference every design finding links to, and what an author or an agent reads before writing QML. They hold for any application on Ljós or on Qt. Each rule has a code (`motion-ease-in-enter`), a section at `docs/design/<page>#<code>`, its numbers in `crates/ljos_design/src/limits.rs` (named here as `limits::…`), a bad and a good QML snippet, and its sources. The rule table itself is `ljos_design::RULES`; when this text and the table disagree, the table is right and this text is a bug (`crates/ljos_design/tests/guides.rs` keeps them together).

| Page | Area | Codes start with |
|---|---|---|
| [accessibility.md](accessibility.md) | names, roles, targets, the tab walk | `a11y-` |
| [color.md](color.md) | contrast, palettes, colour schemes, colour theory | `color-` |
| [typography.md](typography.md) | clipping, eliding, sizes, fonts | `text-` |
| [layout.md](layout.md) | windows, popups, layouts, spacing | `layout-` |
| [interaction.md](interaction.md) | focus, keyboard, pointer, hover | `focus-` |
| [motion.md](motion.md) | easing, duration, springs, reduced motion | `motion-` |
| [content.md](content.md) | words, empty states, identifiers | `content-` |
| [agent-brief.md](agent-brief.md) | every rule as one imperative line, for agents | all |

## The model

Guidelines that are only prose get skipped, so Ljós makes them checks, in four layers that all read the same table (ARCHITECTURE.md §14, "Design guardrails"):

1. **Tokens make the right thing easy.** A design token module gives colours by role from one seed (OKLCH ramps whose text and surface pairs already pass contrast, in light and dark, normal and high contrast), motion by kind and frequency (duration, easing curve, spring, reduced-motion fallback), a spacing scale and a type scale. It is plain QML, so an application that also runs on Qt can take it along. If you take colours, durations and spacing from the tokens, most rules below never fire.
2. **Static lints** (`ljos check`, the authoring MCP's `qml_lint`) read the source and report what is visible there: ease-in on an entrance, a scale from 0, an animated width, anchors inside a layout, dynamic text with no elide or wrap, a pointer-only action, a fixed popup size, a literal colour.
3. **The runtime audit** (`ljos audit`, the inspector's `app_audit`, BDD steps) lays the real scene out and draws it over a matrix — light and dark, normal and high contrast, font scales `limits::audit::FONT_SCALES` (up to 200 %), and a narrow and short window (`limits::audit::NARROW_FRACTION`, `limits::audit::SHORT_FRACTION`) — and measures what was drawn: contrast against the composited background, clipped text, controls off the window or covered, focus that is not drawn, popups that do not fit.
4. **MCP design tools** answer while you write: `design_palette`, `design_contrast`, `design_motion`, `design_rules`.

A finding names the rule, the item (role, name, locator), its source line, what was measured against which limit (`2.9:1 < 4.5:1`), the matrix point where it shows, the fix and the link to its section here.

### Severity is confidence

Severity says how sure the check is that a user is hurt, not how much.

| Severity | Meaning | Effect |
|---|---|---|
| Error | Certainly wrong for some user. | Fails `ljos check` and `ljos audit` (non-zero exit). |
| Warning | Very likely wrong. | Must be fixed or kept with a reason. |
| Hint | Advice; often right to ignore. | Reported only. |

Third-party code (the qtdoc corpus under `corpus/`) is reported, never gated: `ljos check --lint=warn` turns Errors into Warnings for such trees.

### Keeping a warning, with a reason

Write the reason where the finding is: on its line, the line above, or the first line of the enclosing object.

```qml
NumberAnimation {
    // ljos-allow(motion-duration): the onboarding tour is watched once, on purpose
    duration: 900
}
```

<a id="lint-allow-without-reason"></a>
### `lint-allow-without-reason`

An allow with no reason after the colon (`// ljos-allow(motion-duration)`) is an Error: the reason is the point, since the next reader has to know why the guideline does not apply. An allow that keeps nothing (the finding is gone) is a Hint, `lint-allow-unused`; delete it.

Sources: https://www.w3.org/TR/WCAG22/ (the guidelines being waived), https://eslint.org/docs/latest/use/configure/rules#disabling-rules (the same convention in another linter)

<a id="lint-allow-unused"></a>
### `lint-allow-unused`

An allow comment whose rule no longer finds anything there. Remove it, so allows stay a list of real exceptions.

Sources: https://eslint.org/docs/latest/use/command-line-interface#--report-unused-disable-directives

## The tools

These surfaces are being built in wave 11; this is how ARCHITECTURE.md §14 states them.

| Tool | What it does |
|---|---|
| `ljos check <path>` | Compiles and lints. Error findings exit non-zero; `--lint=warn` makes them warnings; `--no-lint` leaves the lints out. |
| `qml_lint` (authoring MCP, `ljos mcp`) | The same lints on a file or a snippet, with codes and guide links; `ljos://lints` lists every rule. |
| `ljos audit <file.qml> [--matrix …] [--json]` | Loads the file headless, audits it over the matrix, exits non-zero on any Error. |
| `app_audit` (inspector, `ljos run --mcp`) | Audits a running application. Args: `root`, `matrix` (`"current"`, the default and cheap; `"full"`; or named points), `rules` (codes or areas), `max_findings`. Restores the window exactly as it was. |
| BDD steps | "the design audit finds nothing", "the design audit reports `<code>` on …"; see features/README.md. |
| `design_palette` | Seed, scheme, contrast, harmony → roles, ramps, every pair's WCAG ratio and APCA Lc, and QML using the tokens. |
| `design_contrast` | Foreground, background, optional size and weight and the layers between → ratio, AA/AAA, APCA advice, and the nearest passing foreground of the same hue. |
| `design_motion` | Kind, frequency, size → duration, easing, spring, reduced-motion fallback and a QML snippet. |
| `design_rules` | Lists or explains rule codes with severity, check, fix and guide link. |
| Token module (planned name `Ljos.Design`) | Singletons for colour roles, motion, spacing and type; ready enter/exit transitions for popups. `ljos design export <dir>` writes it out as plain files for Qt. |

## Pre-flight: answer before you write a component

1. **Who uses it, how often?** Many times a day (a list selection, a tab, a shortcut): no motion, or at most `limits::motion::FREQUENT_MAX_MS`. Tens of times: small, quick motion. Rarely: a panel or a dialog may take `limits::motion::STANDARD_MAX_MS`. Once: you may spend up to `limits::motion::EMPHASIS_MAX_MS`. (motion.md)
2. **What is each motion for?** Feedback that the input landed, where something came from or went, a change of state, or avoiding a jarring jump. No purpose, no motion.
3. **Colour weight 60-30-10.** About 60 % tinted neutral surfaces, 30 % the primary hue in structure (navigation, selection), 10 % accent for the one or two things that must be seen. Danger, warning, success and info keep their own hues. (color.md)
4. **States.** Draw each: rest, hover, pressed, keyboard focus (`visualFocus`), disabled (and why), checked/selected, loading, empty, error. A state never differs by hue alone.
5. **Keyboard path.** Tab reaches it; Enter or Space acts; arrows move inside a group (list, tabs, radio group, menu); Escape closes what opened. Write down the order Tab takes. (interaction.md)
6. **Names.** Every control has an accessible name that starts with its visible text; icon-only buttons have `text` with `display: AbstractButton.IconOnly` and a tooltip.
7. **The smallest window and 200 % text.** What wraps, what elides (and where the full text is), what scrolls, what moves into an overflow menu. Nothing operable leaves the window. (layout.md, typography.md)
8. **Dark mode and high contrast.** Every colour comes from a palette role or a token that has a dark and a high-contrast value.

## Not checked yet

Guidance below is in the pages, marked "not checked yet", with a proposed code for when a check is written. These are candidates for new rules.

| Proposed code | Page | Guidance |
|---|---|---|
| `a11y-placeholder-label` | accessibility.md | A field labelled only by its placeholder loses its label as soon as one types (WCAG 3.3.2). |
| `a11y-live-status` | accessibility.md | A status or toast text that changes has no `Accessible.role` of status bar or alert, so it is never announced. |
| `a11y-drag-only` | accessibility.md | Something done by dragging has no single-pointer or keyboard way (WCAG 2.5.7). |
| `color-complement-adjacent` | color.md | Two complementary hues at full chroma touch, and vibrate. |
| `color-semantic-hue` | color.md | A brand accent sits in the hue band reserved for danger, warning, success or info. |
| `color-accent-weight` | color.md | The accent covers much more than a tenth of the window. |
| `text-line-length` | typography.md | A paragraph runs wider than about 75 characters. |
| `text-tabular-figures` | typography.md | A number that changes in place (a timer, a counter, a column) shifts as its digits change. |
| `layout-nested-radius` | layout.md | A child's corner radius is larger than its parent's. |
| `layout-reflow` | layout.md | At 320 logical px wide, reading needs scrolling in two directions (WCAG 1.4.10). |
| `focus-arrow-keys` | interaction.md | A composite widget (tabs, a radio group, a list) takes one Tab stop per item instead of arrows inside it. |
| `focus-busy-feedback` | interaction.md | An action that takes more than a second shows no busy state. |
| `focus-destructive-undo` | interaction.md | A destructive action has neither a confirmation nor an undo. |
| `motion-flash` | motion.md | Something flashes more than three times a second (WCAG 2.3.1). |
| `motion-endless` | motion.md | An infinite animation that is not a progress indicator runs past 5 s with no way to stop it (WCAG 2.2.2). |
| `motion-origin` | motion.md | A popup grows from its centre instead of from the control that opened it. |
| `motion-theme-switch` | motion.md | Colours animate while the theme switches. |
| `content-vague-action` | content.md | A button says `OK` or `Continue` where a verb and its object fit (`Save draft`). |
| `content-menu-ellipsis` | content.md | A menu item that opens a dialog for more input does not end with `…`. |

Guidance that no machine can check (what a toast says, how settings pages are titled, where status goes) is written without a code in content.md and interaction.md.

## Sources

- WCAG 2.2: https://www.w3.org/TR/WCAG22/ and its Understanding documents, https://www.w3.org/WAI/WCAG22/Understanding/
- WAI-ARIA Authoring Practices, patterns: https://www.w3.org/WAI/ARIA/apg/patterns/
- axe-core rule list: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md
- Material 3: https://m3.material.io/ ; Apple Human Interface Guidelines: https://developer.apple.com/design/human-interface-guidelines/
- animations.dev, the easing blueprint: https://animations.dev/learn/animation-theory/the-easing-blueprint ; Emil Kowalski's skills (MIT): https://github.com/emilkowalski/skills
- web.dev, UX basics and animations: https://web.dev/articles/ux-basics , https://web.dev/articles/animations-guide
- ui-ux-pro-max quick reference: https://github.com/nextlevelbuilder/ui-ux-pro-max-skill
- Vercel web interface guidelines: https://vercel.com/design/guidelines ; Rauno Freiberg: https://interfaces.rauno.me/
- Nielsen's heuristics and response times: https://www.nngroup.com/articles/ten-usability-heuristics/ , https://www.nngroup.com/articles/response-times-3-important-limits/
- Laws of UX: https://lawsofux.com/
- Books: Rex Hartson and Pardha Pyla, *The UX Book* (2nd ed., Morgan Kaufmann, 2018), https://www.elsevier.com/books/catalog/isbn/9780128053423 ; Val Head, *Designing Interface Animation* (Rosenfeld Media, 2016), https://www.rosenfeldmedia.com/books/designing-interface-animation/ ; Issara Willenskomer, "Creating Usability with Motion: The UX in Motion Manifesto", https://uxmag.com/articles/creating-usability-with-motion-the-ux-in-motion-manifesto ; Adam Wathan and Steve Schoger, *Refactoring UI*, https://www.refactoringui.com/
