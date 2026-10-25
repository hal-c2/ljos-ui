//! Whether a screen reader runs, as the platform says
//! (`ljos_quick::set_screen_reader_from_platform`, ARCHITECTURE.md §14).
//!
//! On Linux it is the AT-SPI bus's `ScreenReaderEnabled`
//! (`org.a11y.Status` on `org.a11y.Bus`), which a screen reader such as
//! Orca sets. It is not "something reads the accessibility tree": a test
//! driver or an inspector reads it as well, and Qt keeps no more of a view
//! for either. The property is read once and then followed on a thread of
//! its own, which sleeps until the bus says it changed.
//!
//! `LJOS_SCREEN_READER=on|off` says it instead, on every system: for a
//! sandbox whose driver sets `ScreenReaderEnabled` to have the tree, and
//! that measures what a user without a screen reader gets. Elsewhere, with
//! neither, whatever turns the accessibility tree on stands in for a
//! screen reader.

use ljos_engine::Engine;

/// Starts telling `engine` whether a screen reader runs.
pub(crate) fn watch(engine: &Engine) {
    match forced(std::env::var("LJOS_SCREEN_READER").ok().as_deref()) {
        Some(said) => ljos_quick::set_screen_reader_from_platform(engine, said),
        #[cfg(target_os = "linux")]
        None => atspi::watch(engine),
        #[cfg(not(target_os = "linux"))]
        None => {}
    }
}

/// What `LJOS_SCREEN_READER` says; none for anything else.
fn forced(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => Some(true),
        "0" | "off" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
mod atspi {
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

    use ljos_engine::Engine;
    use ljos_platform::{Platform, Reply};

    const PROPERTY: &str = "ScreenReaderEnabled";

    pub(super) fn watch(engine: &Engine) {
        // Until the bus answers there is none, and a system without an
        // AT-SPI bus has none.
        ljos_quick::set_screen_reader_from_platform(engine, false);
        let first = Platform::of(engine).expect_standing("screen reader setting");
        // Each answer, when the Engine runs it, hands the thread the reply
        // for the next: one at a time, so nothing queues behind a frame.
        let (next_tx, next_rx) = sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("ljos-screen-reader".into())
            .spawn(move || follow(first, &next_tx, &next_rx));
    }

    fn follow(first: Reply, next_tx: &SyncSender<Reply>, next_rx: &Receiver<Reply>) {
        let Ok(bus) = zbus::blocking::Connection::session() else {
            return;
        };
        let Ok(status) =
            zbus::blocking::Proxy::new(&bus, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status")
        else {
            return;
        };
        let changes = status.receive_property_changed::<bool>(PROPERTY);
        let mut told = false;
        let mut reply = first;
        let now = status.get_property::<bool>(PROPERTY).unwrap_or(false);
        let values = std::iter::once(now).chain(changes.filter_map(|c| c.get().ok()));
        for active in values {
            if active == told {
                continue;
            }
            told = active;
            let next = next_tx.clone();
            reply.send(move |e| {
                ljos_quick::set_screen_reader_from_platform(e, active);
                let _ = next.try_send(Platform::of(e).expect_standing("screen reader setting"));
            });
            match next_rx.recv() {
                Ok(r) => reply = r,
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::forced;

    #[test]
    fn the_variable_says_on_or_off_or_nothing() {
        assert_eq!(forced(Some("on")), Some(true));
        assert_eq!(forced(Some("1")), Some(true));
        assert_eq!(forced(Some(" Off ")), Some(false));
        assert_eq!(forced(Some("0")), Some(false));
        assert_eq!(forced(Some("maybe")), None);
        assert_eq!(forced(None), None);
    }
}
