//! The `QtWebEngine` module for Ljós: `WebEngineView` and the types it
//! hands out (`WebEngineProfile`, `WebEngineScript`, the script
//! collection, loading info, new-window requests) and the `WebEngine`
//! singleton.
//!
//! A host registers it beside the standard modules:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_webengine::register(&engine);
//! ```
//!
//! ARCHITECTURE.md §7.9 is the design. In short:
//!
//! - **Pages** are drawn by Chromium through the Chromium Embedded
//!   Framework in its windowless mode (the `cef` feature): each picture,
//!   with the rectangles that changed, becomes an image of the item's
//!   display list, so a page is clipped, transformed and stacked as any
//!   item is. Without the feature, or without a runtime beside the
//!   program, the types still load and every load fails, saying why once.
//! - **Waking**: CEF says when it next wants to run; a deadline on a
//!   thread of its own posts one job to the Engine thread then. A static
//!   page costs no CPU.
//! - **Processes**: Chromium's other processes run a small helper
//!   program, `ljos-webengine-helper`, found beside the app's, so an app
//!   keeps its own `main`.
//! - **Scripts** (`runJavaScript`, `userScripts`) go through the DevTools
//!   protocol from the browser process, so the helper needs no code of
//!   its own in the page's process.

// Without a web engine nothing makes events, so what turns them into
// properties is unreachable but kept, to keep one shape.
#![cfg_attr(not(feature = "cef"), allow(dead_code))]

mod backend;
#[cfg(feature = "cef")]
mod cef;
mod errors;
mod input;
pub mod mac_bundle;
mod types;
mod view;

use ljos_engine::{Engine, Version};

pub use backend::{shutdown, unavailable};
pub use types::{
    WebEngine, WebEngineDesktopMediaRequest, WebEngineLoadingInfo, WebEngineNavigationRequest,
    WebEngineNewWindowRequest, WebEnginePermission, WebEngineProfile, WebEngineScript,
    WebEngineScriptCollection, WebEngineSettings,
};
pub use view::WebEngineView;

/// The logging category of every warning this module gives.
const CATEGORY: &str = "qt.webengine";

/// Registers `QtWebEngine` on `engine`: the one seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtWebEngine" 6.0 => [
        WebEngineView,
        WebEngineProfile,
        WebEngineScript,
        WebEngineScriptCollection,
        WebEngineSettings,
        WebEngineLoadingInfo,
        WebEngineNewWindowRequest,
        WebEngineNavigationRequest,
        WebEnginePermission,
        WebEngineDesktopMediaRequest,
    ]));
    engine.register_singleton_with::<WebEngine>("QtWebEngine", Version::new(6, 0), WebEngine::make);
    view::register_paint(engine);
    engine.add_memory_owner(pages);
}

/// The pictures of the pages views show.
fn pages() -> ljos_engine::OwnerMemory {
    let (bytes, count) = view::frames_held();
    ljos_engine::OwnerMemory {
        key: "webengine",
        label: "web pages",
        bytes,
        count,
        of: "pictures",
    }
}
