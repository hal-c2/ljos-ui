//! The executable CEF starts its renderer, GPU and utility processes from.
//!
//! An app that shows web pages keeps its own `main`: CEF is told this
//! helper's path (`browser_subprocess_path`) and starts every other
//! process from it, so nothing in the app has to look at its arguments
//! first. Ship it beside the app's executable, or name it in
//! `LJOS_WEBENGINE_HELPER`; on macOS it runs as each of the helper
//! applications of the app's bundle (`ljos-webengine-bundle`).

fn main() {
    // On macOS the helper is `<Name> Helper.app` inside the application's
    // bundle, and loads the framework from there before calling CEF.
    #[cfg(target_os = "macos")]
    let _framework = {
        let exe = std::env::current_exe().expect("the helper's path");
        // The loader takes for granted that the framework is there.
        if ljos_webengine::mac_bundle::framework_of_helper(&exe).is_none() {
            eprintln!(
                "ljos-webengine-helper: {} is not a helper application of a bundle with CEF's \
                 framework (make one with ljos-webengine-bundle)",
                exe.display()
            );
            std::process::exit(1);
        }
        let loader = cef::library_loader::LibraryLoader::new(&exe, true);
        if !loader.load() {
            eprintln!("ljos-webengine-helper: CEF's framework did not load");
            std::process::exit(1);
        }
        loader
    };
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let code = cef::execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    std::process::exit(code.max(0));
}
