---
title: Install Ljós
description: Install the Ljós toolchain, run the repository checks and add the ljos crate to a Rust project.
section: start
order: 2
---

# Install Ljós

This page shows you how to set up the Ljós development environment, verify that the command-line tool and test suites work on your machine, and depend on the `ljos` crate from a Rust project.

## What you need

Ljós is written in Rust (2024 edition) and uses [`mise`](https://mise.jdx.dev/) to manage tools and repository tasks.

You do not need Qt installed to run QML applications, check files with `ljos check`, format QML with `ljos fmt`, or run `tst_*.qml` tests with `ljos test`. Ljós reads QML against its own type descriptions when a local Qt type table is not present. A local `qml6` installation (Qt 6.11 or newer) is only needed if you run the `@qt-parity` comparison tasks or `ljos bench` against Qt.

<!-- check: verify whether pre-built binaries or `cargo install` instructions should be added once a release channel is published; the source bundle only covers `mise install` and workspace path dependencies -->

## Set up the toolchain from the repository

From a checkout of the `ljos-ui` repository, install the pinned tools with `mise` and run the verification tasks:

```sh
mise install
mise run check
mise run test
mise run features
```

Here is what each task does:

- `mise install` installs the Rust toolchain and tools declared for the workspace.
- `mise run check` runs the formatter check, `clippy` and `parse_check`.
- `mise run test` runs the Rust unit and integration tests across the workspace crates.
- `mise run features` runs the Gherkin behaviour scenarios headlessly on a software adapter (no GPU is required) and prints an `ok`/`FAILED` summary.

<!-- check: verify the exact CLI invocation or `cargo build --bin ljos` / `PATH` step recommended for putting the `ljos` binary on the user's `PATH` -->

Once built, check that the `ljos` command is available:

```sh
ljos --help
```

```text
Ljós — the QML toolchain

Usage: ljos <COMMAND>

Commands:
  mcp      Serve the authoring MCP server on stdio
  i18n     Translation catalogues
  check    Compile QML projects and report what the compiler says; with `--stats`, how many expressions run in the tier-1 binding VM
  run      Open a window on a QML file; with `--watch`, apply every save to the running application
  render   Draw a project to a PNG without a window, after letting a stated time pass
  test     Run `TestCase`s of `tst_*.qml` files on a virtual clock and print results as Qt's runner does; exit status is number of failed tests
  fmt      Lay QML files out the one way the formatter knows (`-` for stdin/stdout)
  shaders  Compile GLSL 440 shaders of `ShaderEffect`s to `.ljsh` files beside them, for an application that compiles none at run time
  bench    Measure QML apps on Ljós and on Qt inside a memory/CPU limit
  observe  Open a window on a running application, like Erlang's observer
  attach   Serve a running application's inspector on stdio for an MCP client that starts servers as programs
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## Use the `ljos` crate in a Rust project

If you are building a pure QML application, you only need the `ljos` command-line tool. If you want to register native Rust types with `#[derive(QmlObject)]` and `#[qml_impl]`, add the `ljos` crate to your `Cargo.toml`:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[dependencies]
ljos = { path = "../../crates/ljos" }
```

<!-- check: verify crate version or git dependency URL to recommend for projects outside the ljos-ui workspace -->

In your Rust code, import `ljos::prelude::*`, create an engine with `ljos::engine()`, and register your module with `engine.register_module(ljos::module!(...))`.

## Editor and agent setup

The `ljos-ui` repository includes a `.mcp.json` configuration file at its root. Coding agents that read `.mcp.json` can start `ljos mcp` automatically with no extra configuration. To wire an MCP client manually to your own project directory, run:

```sh
ljos mcp --project .
```

## See also

- [What is Ljós?](./index.md)
- [Your first app](./first-app.md)
- [Command-line reference](./cli.md)
- [MCP servers for agents](./mcp.md)
