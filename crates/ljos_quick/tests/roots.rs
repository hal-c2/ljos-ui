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

#[test]
fn a_loader_is_no_replacement_for_the_shell_it_stands_for() {
    // `let_go`: the loader let go of its shell, and its window could not
    // show the loader, so the shell stays shown as the loader's.
    for let_go in [false, true] {
        let e = engine();
        let (mut host, loader, shell) = loader_showing_its_shell(&e);
        let swaps = RootSwaps::of(&e);
        if let_go {
            assert!(swaps.present(&e, loader, None));
            host.frame_showing(&e, false);
        }
        assert_eq!(host.root, shell);
        assert_eq!(swaps.holder_of(shell), Some(loader));

        assert_eq!(
            replace_root(&e, shell, loader),
            Err(RootError::Shown(loader)),
            "let go: {let_go}"
        );
        assert!(!swaps.is_pending(), "nothing is asked of the host");
        // Whether the window could show something or not, nothing goes.
        for can in [true, false] {
            host.frame_showing(&e, can);
            assert_eq!(host.root, shell);
            assert!(!e.is_going(loader) && !e.is_going(shell));
        }

        // Another root still takes the window, the loader and shell going.
        let new = root(&e);
        replace_root(&e, shell, new).unwrap();
        host.frame(&e);
        assert_eq!(host.root, new);
        assert!(e.is_going(loader) && e.is_going(shell));
    }
}

#[test]
fn a_shell_that_stays_shown_is_the_loaders_until_what_replaces_it_is_shown() {
    let e = engine();
    let (mut host, loader, shell) = loader_showing_its_shell(&e);
    let swaps = RootSwaps::of(&e);
    assert!(swaps.present(&e, loader, None));
    host.frame_showing(&e, false);
    assert_eq!(host.root, shell);
    // What looks into the application finds the loader behind the shell.
    assert_eq!(swaps.holder_of(shell), Some(loader));

    // The loader's next shell goes before the window shows it.
    let gone = window_of(&e, loader);
    assert!(!swaps.present(&e, loader, Some(gone)));
    e.destroy(gone);
    host.frame(&e);
    assert_eq!(host.root, shell, "the shell shown stays");
    assert!(!e.is_going(shell));
    assert_eq!(swaps.holder_of(shell), Some(loader));

    // The one after takes the window.
    let next = window_of(&e, loader);
    assert!(!swaps.present(&e, loader, Some(next)));
    host.frame(&e);
    assert_eq!(host.root, next);
    assert!(e.is_going(shell));
    assert_eq!(swaps.holder_of(next), Some(loader));
}

#[test]
fn a_root_asked_back_before_the_frame_stays_in_its_window() {
    // Back at once, or after one more in between.
    for between in [false, true] {
        let e = engine();
        let first = root(&e);
        let mut host = Host::open(&e, first);
        let asked = root(&e);
        replace_root(&e, first, asked).unwrap();
        let last = if between {
            let next = root(&e);
            replace_root(&e, asked, next).unwrap();
            next
        } else {
            asked
        };
        replace_root(&e, last, first).unwrap();
        assert!(!RootSwaps::of(&e).is_pending(), "nothing is to change");
        assert!(e.is_going(asked) && e.is_going(last));

        for can in [true, false] {
            host.frame_showing(&e, can);
            assert_eq!(host.root, first, "between: {between}");
            assert!(!e.is_going(first), "between: {between}");
        }
        // The window's root is the one replaced next.
        let next = root(&e);
        replace_root(&e, first, next).unwrap();
        host.frame(&e);
        assert_eq!(host.root, next);
        assert!(e.is_going(first));
    }
}

#[test]
fn a_loader_asked_for_and_replaced_before_the_frame_goes_with_its_shell() {
    // Whether the window can show the last one asked for or not.
    for can in [true, false] {
        let e = engine();
        let first = root(&e);
        let mut host = Host::open(&e, first);
        let loader = root(&e);
        let shell = window_of(&e, loader);
        RootSwaps::of(&e).present(&e, loader, Some(shell));
        replace_root(&e, first, loader).unwrap();
        let last = root(&e);
        replace_root(&e, loader, last).unwrap();
        // The loader never got the window: it goes now, its shell with it.
        assert!(e.is_going(loader) && e.is_going(shell), "can: {can}");

        host.frame_showing(&e, can);
        let (shown, gone) = if can { (last, first) } else { (first, last) };
        assert_eq!(host.root, shown, "can: {can}");
        assert!(!e.is_going(shown) && e.is_going(gone), "can: {can}");
        assert!(!RootSwaps::of(&e).is_pending());
    }
}

#[test]
fn a_shell_whose_next_one_goes_before_the_frame_is_the_loaders_again() {
    // The loader lets go of its shell first, as a ShellLoader does, or
    // stands for the next one at once.
    for let_go_first in [true, false] {
        let e = engine();
        let (mut host, loader, shell) = loader_showing_its_shell(&e);
        let swaps = RootSwaps::of(&e);
        if let_go_first {
            assert!(swaps.present(&e, loader, None));
        }
        let gone = window_of(&e, loader);
        swaps.present(&e, loader, Some(gone));
        e.destroy(gone);
        host.frame(&e);
        assert_eq!(host.root, shell, "the shell shown stays");
        assert!(!e.is_going(shell));
        assert_eq!(
            swaps.holder_of(shell),
            Some(loader),
            "let go first: {let_go_first}"
        );

        // The loader's next shell takes the window, and the old one goes.
        let next = window_of(&e, loader);
        swaps.present(&e, loader, Some(next));
        host.frame(&e);
        assert_eq!(host.root, next, "let go first: {let_go_first}");
        assert!(e.is_going(shell));
        assert_eq!(swaps.holder_of(next), Some(loader));
    }
}

#[test]
fn a_loader_asked_back_before_the_frame_stands_for_its_window_still() {
    for can in [true, false] {
        let e = engine();
        let (mut host, loader, shell) = loader_showing_its_shell(&e);
        let swaps = RootSwaps::of(&e);
        let asked = root(&e);
        replace_root(&e, loader, asked).unwrap();
        replace_root(&e, asked, loader).unwrap();
        assert!(!swaps.is_pending(), "nothing is to change");
        assert!(e.is_going(asked));

        host.frame_showing(&e, can);
        assert_eq!(host.root, shell, "can: {can}");
        assert!(!e.is_going(loader) && !e.is_going(shell), "can: {can}");
        assert_eq!(swaps.holder_of(shell), Some(loader));

        // The loader is what is replaced next, its shell with it.
        let next = root(&e);
        replace_root(&e, loader, next).unwrap();
        host.frame(&e);
        assert_eq!(host.root, next, "can: {can}");
        assert!(e.is_going(loader) && e.is_going(shell), "can: {can}");
    }
}

#[test]
fn a_loader_that_let_go_of_its_shell_keeps_it_until_what_replaces_it_shows() {
    for can in [true, false] {
        let e = engine();
        let (mut host, loader, shell) = loader_showing_its_shell(&e);
        let swaps = RootSwaps::of(&e);
        // Its shell is shown, and would go with it.
        assert_eq!(
            replace_root(&e, loader, shell),
            Err(RootError::Shown(shell))
        );
        assert!(swaps.present(&e, loader, None));
        assert_eq!(
            replace_root(&e, loader, shell),
            Err(RootError::Owned(shell))
        );
        let asked = root(&e);
        replace_root(&e, loader, asked).unwrap();
        assert!(!e.is_going(loader) && !e.is_going(shell), "can: {can}");

        host.frame_showing(&e, can);
        if can {
            assert_eq!(host.root, asked);
            assert!(e.is_going(loader) && e.is_going(shell));
            continue;
        }
        assert_eq!(host.root, shell);
        assert!(!e.is_going(loader) && !e.is_going(shell) && e.is_going(asked));
        assert_eq!(swaps.holder_of(shell), Some(loader));
        // The loader's next shell takes the window.
        let next = window_of(&e, loader);
        swaps.present(&e, loader, Some(next));
        host.frame(&e);
        assert_eq!(host.root, next);
        assert!(e.is_going(shell));
    }
}
