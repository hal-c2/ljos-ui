# Design brief for agents writing QML

Follow every line. `[code]` is the rule that finds the mistake; its section is `docs/design/<page>#<code>` (README.md has the model and the tools). Before you write, call `design_rules`; for colours `design_palette` / `design_contrast`; for motion `design_motion`. After you write, run `ljos check` and `ljos audit`. Keep a Warning only with `// ljos-allow(<code>): <reason>` [lint-allow-without-reason] and delete allows that keep nothing [lint-allow-unused].

## Accessibility
- Give everything that reacts to a click, tap or key an accessible name. [a11y-unnamed-interactive]
- Give icon-only buttons `text` and `display: AbstractButton.IconOnly`. [a11y-unnamed-icon-button]
- Name every image, or set `Accessible.ignored: true` on decoration. [a11y-unnamed-image]
- Never give two sibling controls the same name. [a11y-duplicate-name]
- Never reuse one name for two different actions in a window. [a11y-ambiguous-name]
- Give every icon-only button a `ToolTip`. [a11y-icon-button-no-tooltip]
- Make targets at least 24 px (44 on touch), or space them out. [a11y-target-size]
- Let Tab leave every area it enters; set `KeyNavigation.tab` on multi-line text areas. [a11y-tab-trap]
- Make everything the pointer operates reachable by Tab (`activeFocusOnTab`). [a11y-tab-unreachable]
- Tell each virtualised row its position and the list's size. [a11y-set-size]
- Hide invisible items with `visible: false`, not opacity, size or clipping. [a11y-hidden-exposed]
- Mark the current item with `highlighted` / `checked` and a 3:1 visual mark. [a11y-current-unmarked]
- Start the accessible name with the visible text. [a11y-name-mismatch]
- Never put `Accessible.ignored` on something that takes focus or holds controls. [a11y-ignored-focusable]
- Never nest a control inside another clickable item. [a11y-nested-interactive]
- Give every dialog a `title` or accessible name. [a11y-unnamed-dialog]
- Name every text field with `Accessible.name` or a `Label`, never the placeholder alone. [a11y-placeholder-label]
- Mark a changing status line `Accessible.StatusBar`, an urgent one `Accessible.AlertMessage`. [a11y-live-status]

## Colour
- Keep text at 4.5:1 (3:1 at 24 px, or 18.66 px bold) in light, dark and high contrast. [color-text-contrast]
- Keep borders, icons, focus rings and indicators at 3:1. [color-nontext-contrast]
- Never show a state by colour alone; add a word, icon or shape. [color-state-only]
- Take colours from `palette` roles or tokens, never literals in components. [color-literal]
- Bind every surface to the palette or colour scheme. [color-scheme-ignored]
- Make popups, menus and dialogs opaque (alpha ≥ 0.95) or blurred. [color-translucent-surface]
- Set a floating surface apart by colour step, border or shadow. [color-surface-merge]
- Pick hues in OKLCH, keep semantic hues for danger/warning/success/info, weigh colour 60-30-10.

## Text
- Let text wrap or elide; let its container grow with `implicitHeight`. [text-clipped]
- Show elided text in full in a `ToolTip` while `truncated`. [text-elided-hidden]
- Give bound text in a bounded box `elide` or `wrapMode`. [text-unbounded-dynamic]
- Render markup with `textFormat`, or strip it. [text-raw-markup]
- Leave `font.family` unset or bind it to the theme. [text-font-literal]
- Never draw text below 10 px; mute secondary text by weight or colour. [text-too-small]
- Keep reading text under 80 characters a line; bound its width. [text-line-length]
- Test at 200 % text and the smallest window.

## Layout
- Keep every control inside the window at every size and font scale; scroll or overflow. [layout-offscreen]
- Never cover a control with a toast, banner or popup. [layout-occluded]
- Let layouts place text and controls, so they never overlap. [layout-overlap]
- Make the window's minimum size add up to its parts' minimums. [layout-under-minimum]
- Size popups from content, with a maximum and a scroll. [layout-popup-fit]
- Never give a popup with changing content a literal width and height. [layout-fixed-size]
- Align a column's controls in one layout; give fields of a kind one width. [layout-misaligned]
- Never set anchors on a layout's direct children; use `Layout.*`. [layout-anchors-in-layout]
- Use multiples of 4 px for spacing, margins and padding. [layout-spacing-scale]
- Reserve space for late content; never push what is under the pointer. [layout-shift]
- Let only one overlay dim the window. [layout-stacked-scrims]
- Never start or end a wrapped line with a separator. [layout-wrap-separator]
- Group menus of more than 10 items with `MenuSeparator` or submenus. [layout-menu-ungrouped]
- Draw resize handles at 3:1 with a hover state. [layout-resize-handle]
- Give frameless windows a close control at the top corner. [layout-window-controls]
- Make a rectangle filling another have a radius no larger than the outer one's. [layout-nested-radius]
- Let reading text wrap at 320 px wide; never make it scroll both ways. [layout-reflow]

## Focus and input
- Draw `visualFocus` on every focusable control. [focus-indicator-missing]
- Declare items in reading order so Tab follows it. [focus-order]
- Give modal popups `focus: true` and `Popup.CloseOnEscape`. [focus-popup-stranded]
- Use controls, not bare `MouseArea`s, for actions. [focus-pointer-only]
- Show hover-revealed things on `hovered || visualFocus` too. [focus-hover-only]
- Close every closable popup or overlay on Escape. [focus-escape-dismiss]
- Disable shortcuts for the window behind an open modal. [focus-shortcut-under-modal]
- Ask before, or offer undo after, delete, remove, discard and clear. [focus-destructive-undo]
- Use arrows inside tabs, lists, radio groups and menus; one Tab stop each.
- Show busy state after 1 s and progress after 10 s; offer undo for destructive actions.
- Show what Enter does in a field; keep a visible way to reopen a collapsed panel.
- Put status beside its control; say when a setting is overridden elsewhere.

## Motion
- Ease out on enter and response; never ease in. [motion-ease-in-enter]
- Always set `easing.type`. [motion-default-easing]
- Keep micro motion ≤ 200 ms, panels ≤ 350, rare emphasis ≤ 500; never under 50. [motion-duration]
- Start entrances at scale 0.95 with a fade, never 0. [motion-scale-from-zero]
- Never bounce controls or panels. [motion-bounce]
- Animate `opacity`, `scale` and `x`/`y`, never `width`, `height` or margins. [motion-layout-property]
- Make exits shorter than entrances. [motion-exit-slower]
- Stagger ≤ 50 ms a step and ≤ 200 ms in all. [motion-stagger]
- Make frequent actions instant, ≤ 100 ms. [motion-frequent]
- Animate with animations or `Behavior`, never a repeating `Timer`. [motion-timer-driven]
- Gate endless movement on `motionPreference`, or pulse opacity instead. [motion-reduced-ignored]
- Keep every frame under 16.7 ms while animating. [motion-dropped-frames]
- Never flash more than three times a second. [motion-flash]
- Bind `running` on endless animations and timers that change what is shown. [motion-endless]
- Set `transformOrigin` toward the trigger of a popup that scales in. [motion-origin]
- Grow popups from their trigger; under reduced motion cross-fade only.

## Content
- Use one word for one action everywhere. [content-inconsistent-words]
- Never show UUIDs, paths, hashes or object dumps; show names. [content-raw-identifier]
- Never make two adjacent texts say the same thing. [content-repeated-text]
- Give every empty view a placeholder that says why and what to do, and a loading state. [content-empty-state]
- Label rows by what they are, not by their kind. [content-identical-rows]
- Never put two controls for one property in one view. [content-duplicate-control]
- Say why a control is disabled. [content-disabled-no-reason]
- Name buttons by verb and object, never `OK`, `Yes` or `Continue`. [content-vague-action]
- End a menu item that opens a dialog for more input with `…`. [content-menu-ellipsis]
- Write toasts with what, how many and an undo; title settings pages one way.
- Show a row's state on the row; never mention features the build lacks; avoid ambiguous one-word statuses.

## Sources
- The full guide and its sources: docs/design/README.md
- WCAG 2.2: https://www.w3.org/TR/WCAG22/
- Material 3: https://m3.material.io/ ; Apple HIG: https://developer.apple.com/design/human-interface-guidelines/
- animations.dev easing blueprint: https://animations.dev/learn/animation-theory/the-easing-blueprint
