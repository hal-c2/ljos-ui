# Design fixtures

Bad and good QML pairs, one directory per rule code in `ljos_design::RULES`:

    fixtures/<rule-code>/<case>.bad.qml
    fixtures/<rule-code>/<case>.good.qml

- A `.bad.qml` starts with `// expect: <code> at <line>[, <code> at <line>…]`,
  one entry per finding it must produce. The line is the QML line (counting the
  comment lines) the finding points at.
- A runtime case may add `// audit: scheme=dark contrast=high font-scale=2
  size=800x600 reduced-motion=true` (any subset): the matrix point where the
  defect shows. Without it the default audit matrix applies.
- The `.good.qml` is the same interface written right. It must produce no
  finding of its directory's code.
- Files are minimal and self-contained (QtQuick, QtQuick.Controls,
  QtQuick.Layouts only), generic, and written from scratch.
- A rule that no runner implements yet is skipped and listed as pending (the
  static runner keeps its set in `ljos_mcp_author::IMPLEMENTED`); the
  fixtures wait for it.

## `ux-` cases

Cases prefixed `ux-` reduce the defects a real desktop application's UX audit
found to generic interfaces (a settings dialog, a file list, a toolbar), so the
checks are held to real failures. Each is also loaded by `qml6` as a black box,
so the pair is valid QML on both engines. Where an expect line names a
finding the rule's check may anchor on a neighbouring item, the line is the
item a reader would fix.

| code | case | defect shown |
|---|---|---|
| `a11y-ambiguous-name` | `ux-same-label-two-menus` | two buttons named alike open different menus |
| `a11y-current-unmarked` | `ux-combo-popup-no-marker` | combo popup delegates give the current index no mark |
| `a11y-current-unmarked` | `ux-list-no-current-marker` | list rows show no sign of the current one |
| `a11y-duplicate-name` | `ux-menu-items-same-label` | two menu items share one label |
| `a11y-hidden-exposed` | `ux-filtered-rows-opacity` | filtered rows hidden with opacity 0 stay in the tree |
| `a11y-icon-button-no-tooltip` | `ux-toolbar-icons` | icon-only tool buttons with no tooltip |
| `a11y-tab-trap` | `ux-textarea-takes-tab` | a text area swallows Tab as text |
| `a11y-tab-unreachable` | `ux-rows-skip-tab` | clickable rows are skipped by Tab |
| `a11y-unnamed-interactive` | `ux-clickable-delegate` | a MouseArea delegate has no accessible name |
| `color-literal` | `ux-label-literal-black` | a label colour written as `black` |
| `color-literal` | `ux-surface-literal-grey` | a surface colour written as a hex literal |
| `color-nontext-contrast` | `ux-field-same-as-page` | a text field drawn in the page colour, no border |
| `color-nontext-contrast` | `ux-icon-same-as-surface` | an icon in the colour of its surface |
| `color-scheme-ignored` | `ux-field-white-background` | a field background fixed to white |
| `color-scheme-ignored` | `ux-fixed-dark-popup` | a popup with a fixed near-black surface |
| `color-scheme-ignored` | `ux-switch-own-colours` | a switch indicator with fixed on/off colours |
| `color-state-only` | `ux-toggle-text-colour-only` | a toggle button changes only its text colour |
| `color-text-contrast` | `ux-warning-text-on-tint` | orange text on a pale orange tint, light scheme |
| `color-translucent-surface` | `ux-menu-over-text` | a menu at 0.8 alpha over body text |
| `content-disabled-no-reason` | `ux-disabled-menu-item` | a disabled menu item gives no reason |
| `content-duplicate-control` | `ux-two-combos-same-property` | two combo boxes edit one property |
| `content-empty-state` | `ux-empty-grid-no-placeholder` | an empty grid shows nothing |
| `content-empty-state` | `ux-empty-list-bare-heading` | an empty list shows only its heading |
| `content-identical-rows` | `ux-same-label-rows` | five rows all read `Run command` |
| `content-inconsistent-words` | `ux-retry-vs-try-again` | `Try again` and `Retry` in one window |
| `content-raw-identifier` | `ux-absolute-path-row` | a row labelled by an absolute path |
| `content-raw-identifier` | `ux-uuid-in-toast` | a message that prints a UUID |
| `content-repeated-text` | `ux-error-repeats-message` | two labels say the same sentence |
| `focus-escape-dismiss` | `ux-dialog-escape-disabled` | a dialog that Escape does not close |
| `focus-hover-only` | `ux-hover-revealed-button` | a button shown only while the row is hovered |
| `focus-indicator-missing` | `ux-borderless-combo` | a combo box with no focus drawing |
| `focus-indicator-missing` | `ux-custom-row-no-ring` | a focusable row draws no focus ring |
| `focus-pointer-only` | `ux-mouse-only-row` | an action on a MouseArea with no key path |
| `focus-popup-stranded` | `ux-modal-no-focus-no-close` | a modal popup with no focus and no close policy |
| `focus-shortcut-under-modal` | `ux-global-shortcut-behind-dialog` | a window shortcut stays live under a modal |
| `layout-fixed-size` | `ux-popup-with-changing-list` | a popup of literal 320x360 holding a changing list |
| `layout-menu-ungrouped` | `ux-long-flat-menu` | twelve menu items, no separator |
| `layout-misaligned` | `ux-combo-widths` | one combo box wider than its siblings |
| `layout-misaligned` | `ux-menu-icon-slots` | menu labels start at different x (icon on one item only) |
| `layout-occluded` | `ux-panel-over-header-buttons` | a side panel drawn over the last header buttons |
| `layout-occluded` | `ux-toast-over-buttons` | a toast anchored over the button row |
| `layout-offscreen` | `ux-footer-below-window` | a list fills the height and pushes the footer out |
| `layout-offscreen` | `ux-header-clipped-at-large-font` | a clipped header row loses buttons at font scale 2 |
| `layout-offscreen` | `ux-selected-row-below-view` | the selected row is never scrolled into view |
| `layout-offscreen` | `ux-tabbar-overflow` | five 120 px tabs in a 400 px bar |
| `layout-popup-fit` | `ux-empty-list-collapses-popup` | an empty list collapses its popup to zero height |
| `layout-popup-fit` | `ux-tall-popup-small-content` | a 640 px popup for three lines |
| `layout-resize-handle` | `ux-invisible-split-handle` | a split handle that draws nothing |
| `layout-shift` | `ux-header-moves-after-load` | a header jumps from centre to top after load |
| `layout-shift` | `ux-status-line-pushes-rows` | a status line inserted above the rows |
| `layout-stacked-scrims` | `ux-two-dimmed-popups` | two modal popups both dim the window |
| `layout-under-minimum` | `ux-row-squeezes-viewer` | a fixed panel squeezes a viewer below its minimum |
| `layout-under-minimum` | `ux-sidebar-below-minimum` | a 208 px sidebar given 160 px |
| `layout-window-controls` | `ux-controls-pushed-to-middle` | window controls end up mid-window beside a panel |
| `layout-window-controls` | `ux-frameless-without-controls` | a frameless window with no close control |
| `layout-wrap-separator` | `ux-separator-before-each-item` | every wrapped line starts with a separator |
| `text-elided-hidden` | `ux-header-no-tooltip` | elided text with no way to read it in full |
| `text-elided-hidden` | `ux-menu-item-long-reason` | a long menu label in a 200 px menu, elided with no way to read it |
| `text-font-literal` | `ux-label-font-family` | controls that set `font.family` themselves |
| `text-raw-markup` | `ux-angle-bracket-placeholder-row` | a `<default>` placeholder shown as a row |
| `text-raw-markup` | `ux-backticks-in-plain-text` | backticks and `**` in plain text |
| `text-raw-markup` | `ux-envelope-tag-bubble` | an internal result tag shown as characters |
| `text-raw-markup` | `ux-http-error-line` | a raw HTTP status and URL as the message |

`ljos_design::fixture` reads the headers for every runner, and
`tests/fixtures.rs` checks the structure.
