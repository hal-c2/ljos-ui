//! The platform's own windows, for what only the platform's API does: the
//! host hands each window it opens to what the application asked to see
//! them, through `raw-window-handle`.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// A window of the platform's, as a host opened it: the handles of the
/// window and of its display. Borrowed for the call it is handed to.
pub trait NativeWindow: HasWindowHandle + HasDisplayHandle {}

impl<T: HasWindowHandle + HasDisplayHandle + ?Sized> NativeWindow for T {}
