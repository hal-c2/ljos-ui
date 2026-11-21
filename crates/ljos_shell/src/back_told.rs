//! What a phone's activity was last told of whether the application takes
//! Back, so that it is told only when that changes. The application lives
//! on past its activity; a new one starts out taking Back itself, so it
//! is told anew whatever the last one heard.

#![cfg_attr(not(target_os = "android"), allow(dead_code))]

#[derive(Debug, Default)]
pub(crate) struct BackTold(Option<bool>);

impl BackTold {
    /// What the activity is to be told when the application `taken` Back
    /// now: nothing when it heard that already.
    pub(crate) fn tell(&mut self, taken: bool) -> Option<bool> {
        (self.0 != Some(taken)).then(|| {
            self.0 = Some(taken);
            taken
        })
    }

    /// A surface came, maybe with a new activity that heard nothing.
    pub(crate) fn surface_came(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_activity_is_told_only_what_changed() {
        let mut told = BackTold::default();
        assert_eq!(told.tell(false), Some(false));
        assert_eq!(told.tell(false), None);
        assert_eq!(told.tell(true), Some(true));
    }

    #[test]
    fn a_new_activity_is_told_though_nothing_changed() {
        let mut told = BackTold::default();
        assert_eq!(told.tell(false), Some(false));
        told.surface_came();
        assert_eq!(told.tell(false), Some(false));
    }
}
