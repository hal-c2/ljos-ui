# External acceptance suites

A suite here is somebody else's Gherkin features: written in their repository, in their words, about
their application. Ljós runs the ones it is told to on its own backend and holds a ratchet, so the
suite becomes a measure of how close Ljós is to running that application, without Ljós adopting the
suite's words or the suite's code (ARCHITECTURE.md 18.4, principle 9).

Ljós **never edits the suite's files**. The checkout is read in place; the only file the runner
writes is `RATCHET`, and only on `--raise`.

## Layout

```
acceptance/<suite>/
  PIN              which revision of the suite these results are about
  features.globs   which of its feature files Ljós runs
  RATCHET          scenarios that must keep passing
  README.md        what the suite is, where it lives, how to get a checkout
  steps/           the adapter crate: the suite's step wording mapped onto Backend
```

`acceptance/example/` is a small suite that lives in this repository (`fixture/` stands for the suite's
own repository). It proves the mechanism and is the model to copy. There is deliberately no suite for
any particular application here until its owner decides there is one.

## `PIN`

`key = value` lines, `#` comments.

| Key | Meaning |
|---|---|
| `repo` | where the suite comes from; only shown in messages |
| `commit` | the git commit the checkout must be at (7 or more hex digits, a prefix is enough) |
| `digest` | `blake3:<hex>` of every file of the checkout; for a suite that is a plain directory and has no git history |
| `env` | name of the environment variable that holds the checkout's path |
| `path` | the checkout's path when nothing else says, relative to the suite's directory |

Exactly one of `commit` and `digest`. The checkout is found from `--path`, else the `env` variable,
else `path`. A checkout at another commit or digest is refused (exit 2) before anything runs. For a
`commit` PIN, uncommitted changes to tracked files are refused too (`--allow-dirty` lifts that), as a
modified feature file is not the pinned suite. `--digest` prints the digest of the checkout, for
writing a `digest` PIN.

## `features.globs`

One glob per line, relative to the checkout (`features/**/*.feature`). `!glob` takes files out again.
Every line has to match at least one file: an allowlist that silently matches nothing (the suite moved a
directory) would turn the suite into zero scenarios and a green result, so it is an error (exit 2).

## `RATCHET`

One scenario id per line, `#` comments. An id is `<file> :: <scenario>`, with the rule's name between
them for a scenario inside a `Rule:`, ` [value | value]` after an outline's name for its example row,
and ` #2`, ` #3` for scenarios in one file that share a name. The run prints the ids, so they can be
copied.

A held scenario that fails, is no longer in the suite, or has been tagged `@backlog-ljos` fails the
run (exit 1). `--raise` adds the scenarios that pass now. It refuses to write anything while a held
scenario fails, and it never removes a line: taking one out is a decision a person makes and shows in
a diff. The comment block at the top of the file is kept.

`@backlog-ljos` scenarios are not run; the summary counts them.

## The adapter crate

`steps/` is a crate with a binary. It writes `#[given]`, `#[when]` and `#[then]` steps in the suite's
own wording on `ljos_bdd::acceptance::AcceptanceWorld` and ends in

```rust
fn main() -> std::process::ExitCode {
    ljos_bdd::acceptance::main(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}
```

A step reaches the application only through `world.backend()`, the `Backend` trait the Ljós ledger
drives: `click`, `key`, `get`, `a11y_tree`, `advance` and the rest. It loads QML with `world.stage`
and `world.load`. An adapter never sees egui, the engine or the compiler.

The repository's workspace is `crates/*`, so an adapter is a workspace of its own (its `Cargo.toml`
says `[workspace]`; like any crate outside the repository it needs nothing else to build against
Ljós, no `[patch]` included). The task seeds its
`Cargo.lock` from the root one on first use, so both resolve the same versions; the copy is not
committed. The adapter names `cucumber` itself because the step macros expand to paths in it; keep
its version equal to the root pin.

## Running

```
mise run features:acceptance example                  # run, fail on a regression
mise run features:acceptance example --raise          # add what passes to RATCHET
mise run features:acceptance <suite> --path ~/src/x   # a checkout elsewhere
mise run features:acceptance-test                     # the mechanism's own tests
```

Exit status: 0 the ratchet holds, 1 a held scenario does not pass, 2 the suite could not be run (wrong
PIN, a glob that matches nothing, a feature file that does not parse, no checkout).

## Adding a suite

1. Make `acceptance/<suite>/` with a `PIN`, `features.globs` and an empty `RATCHET`.
2. Write `steps/` until scenarios pass, starting from `acceptance/example/steps`.
3. `mise run features:acceptance <suite> --raise` and commit the `RATCHET`.
4. Work in the suite's own repository is not done here: if its steps need a hook in the application
   (a test mode, a stable `objectName`), that goes to that repository.
