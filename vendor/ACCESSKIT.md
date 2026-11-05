# The AccessKit patch

`vendor/accesskit_winit` is [`accesskit_winit`](https://crates.io/crates/accesskit_winit) **0.34.1**, `vendor/accesskit_unix` is [`accesskit_unix`](https://crates.io/crates/accesskit_unix) **0.24.0** and `vendor/accesskit_atspi_common` is [`accesskit_atspi_common`](https://crates.io/crates/accesskit_atspi_common) **0.21.0**, all from crates.io, all built from AccessKit commit [`ce8164ba92995cfa86005b6259115e08c8244253`](https://github.com/AccessKit/accesskit/tree/ce8164ba92995cfa86005b6259115e08c8244253) (`adapters/winit`, `adapters/unix`, `adapters/atspi-common`). They are the versions egui-winit 0.37 brings, under egui's `accesskit` 0.25. AccessKit is `MIT OR Apache-2.0`, its winit adapter `Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`, `AUTHORS` and, for the code atspi-common derives from Chromium, `LICENSE.chromium` are taken unchanged from that commit (the published crates do not carry them). Keep them with the copies and update them on a rebase. `rustfmt.toml` (ours) turns formatting off, so the copies diff cleanly against the releases.

Only the AT-SPI adapter is patched; the winit adapter is copied so that it can reach the patched one by path. Like the other forks here (FORK.md, QUICKJS.md), each copy is a package of its own, `ljos_accesskit_winit`, `ljos_accesskit_unix` and `ljos_accesskit_atspi_common`, whose library keeps its name; each manifest says at its top what it changes from the published one (the package name, `description`, `publish`, and the path of the copy it depends on). The root `Cargo.toml` names the winit adapter by path:

```toml
[workspace.dependencies]
accesskit_winit = { package = "ljos_accesskit_winit", version = "0.34.1", path = "vendor/accesskit_winit" }
```

A crate outside this repository that depends on Ljós (by path or by git) gets the fork with no line of its own; a `[patch.crates-io]` would reach no build but this workspace's.

The shell holds the adapter itself (`Surface::accesskit` in `ljos_shell`), so egui-winit's `accesskit` feature is off: it would bring the published adapter. The shell creates the adapter with the window, tells it every window event before egui-winit hears it, puts what an assistive technology asks for into egui's input as `egui::Event::AccessKitActionRequest`, and hands it egui's tree updates, as egui-winit did. `crates/ljos_shell/tests/accesskit_fork.rs` holds the shell's build to the fork. eframe still turns the feature on where it is linked (the `eframe` feature, and `egui_kittest` in tests): a host that shows a `ShellApp` in an eframe application of its own gets eframe's adapter there, the published one.

`mise run accesskit:diff` prints the patch against the published crates, the manifests' marked lines included. Each change is a commit of its own on top of the pristine import, written to be offered upstream; the drafts of the issues are kept with the lead's notes, not here.

## What the patch changes

The gaps showed when a computer-use driver drove a Ljós app over AT-SPI, off-screen (`mise run desktop:cua`).

| Gap | Change |
|---|---|

## Rebase policy

When AccessKit takes a change, drop its commit. When egui-winit moves to another `accesskit_winit`, rebase onto the releases it then brings: import them pristine first, make the manifest changes again, then the remaining commits. Drop the fork, and turn egui-winit's `accesskit` feature back on, once upstream has all three.
