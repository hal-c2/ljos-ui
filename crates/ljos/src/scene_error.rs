//! What showing a scene without a window fails with, for
//! [`crate::render`] and [`crate::testing`].

use ljos_drive::DriveError;
use ljos_engine::EngineError;

/// Why a scene shown without a window was not shown, did not come to
/// rest, or could not be drawn.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SceneError {
    /// The Engine refused: the root is not an item or a window, or a
    /// property could not be written.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// The scene did not come to rest, or could not be drawn.
    #[error(transparent)]
    Drive(#[from] DriveError),
}
