# The QuickJS fork

`vendor/rquickjs-sys` is [`rquickjs-sys`](https://crates.io/crates/rquickjs-sys) **0.14.0** from crates.io, which builds quickjs-ng from C, with one fix to its number-to-text conversion. `vendor/rquickjs-core` is [`rquickjs-core`](https://crates.io/crates/rquickjs-core) **0.14.0** unchanged except for its manifest: it is there only so that it reaches the fork by path. Each is a package of its own, `ljos_rquickjs_sys` and `ljos_rquickjs_core`, so neither is mistaken for the published crate; their libraries keep their names, `rquickjs_sys` and `rquickjs_core`. The root `Cargo.toml` names the core by path under the workspace's `rquickjs` key:

```toml
[workspace.dependencies]
rquickjs = { package = "ljos_rquickjs_core", version = "0.14.0", path = "vendor/rquickjs-core", default-features = false }
```

Crates write `rquickjs.workspace = true` and `use rquickjs::…` as before. The published `rquickjs` crate only re-exports `rquickjs-core` (plus macros, which Ljós does not use), so the workspace key goes straight to the core. As with `vendor/oxc_parser` (FORK.md), the fork is reached by path and not by `[patch.crates-io]`, so a crate outside this repository that depends on Ljós gets the fixed QuickJS with no line of its own.

## Why a fork

ECMAScript's Number::toString writes a number with the fewest significant digits that read back as the same double ([§6.1.6.1.20](https://tc39.es/ecma262/#sec-numeric-types-number-tostring)). Qt's engine does. QuickJS's `js_dtoa` (`quickjs/dtoa.c`, free format) tries each digit count P in turn, but only with the P-digit mantissa nearest the double. That is enough except at a power of two: there the double below is twice as close as the double above, so the double's rounding interval reaches twice as far up as down. The nearest P digits can lie below the double and outside the interval while the next P digits up, farther away but on the wide side, are inside it. QuickJS then goes on to P + 1 and writes one digit too many:

| Number | QuickJS 0.14.0 | Shortest (qml6, the fork, tier 1) |
|---|---|---|
| `2 ** 378` | `6.1565634681866374e+113` | `6.156563468186638e+113` |
| `2 ** 89` | `6.1897001964269014e+26` | `6.189700196426902e+26` |
| `2 ** -1017` | `7.1202363472230444e-307` | `7.120236347223045e-307` |

The text is still a valid round-trip, so nothing fails in a single tier. But tier 1 (`ljos_engine::number_to_string`) writes the shortest, and the debug cross-check between tiers and `tier1_and_tier2_expressions_agree_in_value_and_dependencies` compare the two, so `` `${Math.max(2 ** 378, sz.width)}` `` was red on CI. A QML program can see the difference too: the same number shown by a binding and by a function differed in its text (`features/language/number-text.feature`).

Routing around it in JavaScript (wrapping `String`, template literals, `Number.prototype.toString`, `+ ""`) would miss conversions QuickJS makes inside itself (`Array.prototype.join`, `JSON.stringify`, property keys) and cost a call each time. The fix belongs where the digits are made.

### Upstream

On 2026-10-26: the newest `rquickjs-sys` is 0.14.0 (2026-09-18); quickjs-ng's `master` `dtoa.c` is the same as 0.14.0's; Bellard's quickjs 2026-06-04 has the same free-format loop. Nothing upstream fixes it, so there is no newer release to move to. Offer the fix to quickjs-ng (it is theirs and Bellard's code, and the case is a few lines); if it lands, take the release and drop the fork.

## What the fork changes

`mise run quickjs:diff` prints the full diff against the registry tarballs.

| File | Change |
|---|---|
| `rquickjs-sys/quickjs/dtoa.c` | In `js_dtoa`'s free-format loop: when the round-trip fails and the double is a power of two (`m == 1 << 52`), try the next P-digit mantissa up (`mant + 1`, carried into the exponent if it reaches `radix^P`, trailing zeros removed as before) and test it the same way. Two saved values and one `if`, 23 lines, marked `ljos:`. Every other double takes the unchanged path, so the fix costs nothing outside powers of two. |
| `rquickjs-sys/Cargo.toml` | Package name `ljos_rquickjs_sys`, `description`, `publish = false`, and a header comment. |
| `rquickjs-core/Cargo.toml` | Package name `ljos_rquickjs_core`, `description`, `publish = false`, a header comment, and `path`/`package` on its `rquickjs-sys` dependency. |
| `*/rustfmt.toml` (ours) | Formatting off, since the workspace's `cargo fmt --all` reaches path dependencies; upstream's formatting keeps the diff clean. |

Why only powers of two: for every other double the interval is symmetric, so the nearest P digits are in it whenever any P digits are. Why not P-digit search in both directions: the nearest-below case cannot happen (the narrow side is below), and a smaller change rebases more easily.

### What is left out

The fork keeps only what `build.rs` reads, plus QuickJS's `LICENSE`: in `quickjs/`, the `.c` files it compiles (`quickjs.c`, `libregexp.c`, `libunicode.c`, `dtoa.c`) and the headers they include. Left out: quickjs-ng's docs, tests, examples, generated sources and CI files, the crate's `Makefile` and `Cargo.lock`, and `vendor/wasi-libc` (a 2.6 MB prebuilt `libc.a`, used only for WebAssembly targets (`wasm32-wasip*` and `wasm32-unknown-unknown`), which nothing in Ljós builds). DECISION: if Ljós ever builds QuickJS for WebAssembly, copy `vendor/wasi-libc` back from the same release. `mise run quickjs:diff` lists what is left out as `Only in` lines.

## Checking it

- `crates/ljos_engine/tests/number_text.rs` compares QuickJS with `number_to_string` and with the digit count of Rust's shortest form over every power of two (±, subnormals included), every power of ten from 1e-323 to 1e308, and 20,000 random doubles.
- `crates/ljos_engine/tests/tier1.rs` pins `2 ** 378`, `2 ** 89`, `2 ** -24` and `-(2 ** -1017)` among the expressions both tiers must agree on.
- `features/language/number-text.feature` (`@qt-parity`) shows the numbers from a binding and from a function, on qml6 and on Ljós.

Measured when the fork was made, with a probe over the same sets and 3,000,000 random doubles: QuickJS 0.14.0 differed from the shortest at 54 of the 2,098 powers of two (108 of 4,092 counting signs), and at none of 632 powers of ten or 3,000,000 random doubles. With the fix: none.

## Rebase policy

- **When.** On a security fix, or when a QuickJS or rquickjs change Ljós wants lands. Rebase on the release `rquickjs-sys` pins; the workspace pins one exact version for both crates.
- **How.** Save `mise run quickjs:diff`. Replace both directories with the new releases' crate contents, trimmed as above (keep `rustfmt.toml`). Make the manifest changes again, re-apply the `dtoa.c` hunk, and update the versions in the root `Cargo.toml` and in this file. If upstream fixed the free-format loop, drop the hunk, and if nothing else is left, drop the fork and go back to the published `rquickjs`.
- **Gates.** `cargo test -q -p ljos_js -p ljos_engine` (the number-text and tier-1 tests above), `cargo test -p ljos_quick --test budget`, and `mise run features:parity features/language/number-text.feature`.
