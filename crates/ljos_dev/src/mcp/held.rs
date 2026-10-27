//! Who holds one of `app_frames`' recordings: each of the timeline, the
//! wakes, the profiling and the growth samples runs exactly while one
//! client or more holds it.

use std::collections::HashSet;

use ljos_mcp_core::SurfaceError;

/// The most clients that hold one recording at once.
pub(crate) const MAX_HOLDERS: usize = 16;

#[derive(Default)]
pub(crate) struct Held {
    /// Asked for by a client that named no holder: it lasts until stopped.
    unheld: bool,
    held: HashSet<String>,
}

impl Held {
    pub(crate) fn any(&self) -> bool {
        self.unheld || !self.held.is_empty()
    }

    /// Starts or stops holding for whoever asks; `doing` is what the
    /// recording does, for the refusal when it has too many holders.
    pub(crate) fn ask(
        &mut self,
        record: bool,
        holder: Option<&str>,
        doing: &str,
    ) -> Result<(), SurfaceError> {
        match (record, holder) {
            (true, Some(h)) => {
                if !self.held.contains(h) && self.held.len() >= MAX_HOLDERS {
                    return Err(SurfaceError::invalid_argument(format!(
                        "`app_frames`: {MAX_HOLDERS} clients {doing} already"
                    )));
                }
                self.held.insert(h.to_owned());
            }
            (true, None) => self.unheld = true,
            (false, Some(h)) => {
                self.held.remove(h);
            }
            (false, None) => {
                self.unheld = false;
                self.held.clear();
            }
        }
        Ok(())
    }
}
