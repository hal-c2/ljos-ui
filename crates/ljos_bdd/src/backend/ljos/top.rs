//! The `top` overlay in a scenario: the model a development build paints
//! (`ljos_dev::Top`), given the frames the scenario's host runs anyway.

use super::{Live, LjosBackend, Result};
use crate::backend::top::{TopSeen, TopSheet};

/// How many owners a scenario can ask about.
const OWNERS: usize = 16;

impl Live {
    /// After a frame: what its creation jobs cost goes to the overlay
    /// while it is shown. Hidden, the report is left where it is, for the
    /// inspector's measurements.
    pub(super) fn note_frame(&mut self) {
        if !self.top.visible() {
            return;
        }
        let report = self.scene.take_incubation_report().unwrap_or_default();
        self.top.record(&self.engine, &report);
    }
}

impl LjosBackend {
    /// Shows or hides the overlay. No frame is run for it: it shows the
    /// frames the application runs.
    pub(super) fn show_top(&mut self, shown: bool) -> Result<()> {
        let live = self.live()?;
        if shown && !live.top.visible() {
            // What a frame cost before the overlay was shown is not the
            // cost of the first frame it sees.
            let _ = live.scene.take_incubation_report();
        }
        live.top.set_visible(shown);
        Ok(())
    }

    pub(super) fn top_as_seen(&mut self) -> Result<TopSeen> {
        let live = self.live()?;
        let sheet = live.top.view().map(|view| TopSheet {
            title: view.title,
            lanes: view.lanes.into_iter().map(|row| row.name).collect(),
            charged: live
                .top
                .stats()
                .owners(OWNERS)
                .into_iter()
                .map(|(owner, _)| ljos_dev::top::owner_name(&live.engine, owner))
                .collect(),
        });
        let frame_wanted = if live.scene.needs_frame() {
            Some("the scene asks for a frame".to_owned())
        } else {
            live.wake
                .map(|at| format!("the scene asks to be woken at {at:?}"))
        };
        Ok(TopSeen {
            sheet,
            frame_wanted,
        })
    }
}
