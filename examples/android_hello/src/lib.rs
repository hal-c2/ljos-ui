//! The smallest Ljós application for Android. The activity loads this
//! library and calls `android_main`; the application it starts lives for
//! the whole process, and every later activity only gives it a screen.
//!
//! `ljos android run examples/android_hello` builds it for the phone,
//! packs it into the Gradle template (`android/`) and starts it.

/// The application, ready to build or run.
#[must_use]
pub fn app() -> ljos::App {
    ljos::App::new("Ljós")
        .source(ljos::include_qml!("qml.pack"))
        .root("Main")
}

/// What GameActivity calls on its own thread for each activity.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: ljos::android::AndroidApp) {
    ljos::run_android(app, self::app);
}
