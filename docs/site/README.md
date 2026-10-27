# ljos.dev pages

The pages of the ljos.dev documentation, one directory per section: `start`, `language`, `ui`, `controls`, `data`, `rust` and `quality`. Each page is Markdown with front matter (`title`, `description`, `section`, `order`).

A change to behaviour fixes the page that describes it in the same commit. The QML examples are complete files; each one must pass `ljos check` with no errors. The exceptions are examples that use a second file or a type registered from Rust.

`<!-- check: ... -->` comments mark claims still to be verified against `features/` or the code.
