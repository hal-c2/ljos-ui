//! What a window shows in place of its root: a root the application asks
//! for ([`replace_root`]), and a window a root stands for
//! ([`RootSwaps::present`]), as a `ShellLoader` stands for a shell that is
//! a `Window`.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::Rectangle;
use ljos_quick::roots::{RootError, RootSwaps, replace_root};

/// A root with no parent, as an application makes one.
fn root(e: &Engine) -> ObjectId {
    make::<Rectangle>(e, None)
}

/// What a loader makes of a shell that is a window of its own: the
/// loader owns it, and it is in no item's tree.
fn window_of(e: &Engine, holder: ObjectId) -> ObjectId {
    let w = e.create_native(Rectangle::default(), Some(holder)).unwrap();
    e.complete(w).unwrap();
    w
}

/// A host of one window, which makes what was asked before each frame.
struct Host {
    root: ObjectId,
}

impl Host {
    fn open(e: &Engine, root: ObjectId) -> Self {
        RootSwaps::of(e).shows(e, None, root);
        Host { root }
    }

    fn frame(&mut self, e: &Engine) {
        self.frame_showing(e, true);
    }

    /// `can`: whether the window can show what it is given.
    fn frame_showing(&mut self, e: &Engine, can: bool) {
        let (root, _) = RootSwaps::of(e).swap_in(e, self.root, &mut |_, _| can);
        self.root = root;
    }
}

/// A loader at the root of the window, whose shell is a window shown in
/// its place: `(host, loader, shell)`.
fn loader_showing_its_shell(e: &Engine) -> (Host, ObjectId, ObjectId) {
    let loader = root(e);
    let shell = window_of(e, loader);
    // A loader makes its shell before a host shows it.
    assert!(!RootSwaps::of(e).present(e, loader, Some(shell)));
    let mut host = Host::open(e, loader);
    host.frame(e);
    assert_eq!(host.root, shell);
    (host, loader, shell)
}

#[test]
fn a_root_in_place_of_a_loader_that_stands_for_the_window_takes_the_window() {
    // The application names the loader, its root, or the window it sees.
    for name_the_window in [false, true] {
        let e = engine();
        let (mut host, loader, shell) = loader_showing_its_shell(&e);
        let new = root(&e);
        let old = if name_the_window { shell } else { loader };
        replace_root(&e, old, new).unwrap();
        host.frame(&e);
        assert_eq!(host.root, new);
        assert!(e.is_going(loader), "the loader goes");
        assert!(e.is_going(shell), "and its shell with it");
        assert!(!e.is_going(new));
        assert!(!RootSwaps::of(&e).is_pending());
        // The new root is the one shown: it is replaced in turn.
        let next = root(&e);
        replace_root(&e, new, next).unwrap();
        host.frame(&e);
        assert_eq!(host.root, next);
    }
}

#[test]
fn a_root_asked_for_before_the_window_shows_the_loaders_shell_takes_the_window() {
    let e = engine();
    let loader = root(&e);
    let shell = window_of(&e, loader);
    RootSwaps::of(&e).present(&e, loader, Some(shell));
    let mut host = Host::open(&e, loader);
    assert!(RootSwaps::of(&e).is_pending_for(loader));

    let new = root(&e);
    replace_root(&e, loader, new).unwrap();
    host.frame(&e);
    assert_eq!(host.root, new);
    assert!(e.is_going(loader) && e.is_going(shell));
    assert!(!e.is_going(new));
    assert!(!RootSwaps::of(&e).is_pending());
}

#[test]
fn a_window_that_cannot_show_the_loader_keeps_the_shell_until_the_next_one() {
    let e = engine();
    let (mut host, loader, shell) = loader_showing_its_shell(&e);
    let swaps = RootSwaps::of(&e);
    // The loader lets go of its shell; the window cannot show the loader.
    assert!(swaps.present(&e, loader, None));
    host.frame_showing(&e, false);
    assert_eq!(host.root, shell, "a window shows something");
    assert!(!swaps.is_pending(), "and asks for nothing every frame");

    // The loader's next shell takes the window, and the old one goes.
    let next = window_of(&e, loader);
    assert!(!swaps.present(&e, loader, Some(next)));
    host.frame(&e);
    assert_eq!(host.root, next);
    assert!(e.is_going(shell));
    assert!(!e.is_going(next));
}

#[test]
fn a_root_shown_by_the_host_drops_what_was_asked_for_in_its_place() {
    let e = engine();
    let old = root(&e);
    let mut host = Host::open(&e, old);
    let asked = root(&e);
    replace_root(&e, old, asked).unwrap();
    // A reload in the same pass shows a root of its own there first.
    let reloaded = root(&e);
    RootSwaps::of(&e).shows(&e, Some(old), reloaded);
    host.root = reloaded;

    assert!(!RootSwaps::of(&e).is_pending(), "nothing waits");
    assert!(
        e.is_going(asked),
        "what was asked for is not shown, and goes"
    );
    assert_eq!(
        replace_root(&e, asked, root(&e)),
        Err(RootError::NotShown(asked))
    );
    let next = root(&e);
    replace_root(&e, reloaded, next).unwrap();
    host.frame(&e);
    assert_eq!(host.root, next);
}
