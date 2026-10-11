//! What a scenario sees of the `top` overlay (ARCHITECTURE.md §3.4,
//! §19.10), in words no backend owns.

/// The overlay as it stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopSeen {
    /// What it shows; `None` while it is hidden.
    pub sheet: Option<TopSheet>,
    /// Why a host of this application would run another frame now, if it
    /// would. The overlay must never be the reason.
    pub frame_wanted: Option<String>,
}

/// The overlay's sheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopSheet {
    pub title: String,
    /// The lanes it lists, in the order it lists them.
    pub lanes: Vec<String>,
    /// The owners charged in any frame since it was shown, largest first.
    pub charged: Vec<String>,
}

impl TopSeen {
    pub fn sheet(&self) -> Result<&TopSheet, String> {
        self.sheet
            .as_ref()
            .ok_or_else(|| "the top overlay is hidden".to_owned())
    }
}
