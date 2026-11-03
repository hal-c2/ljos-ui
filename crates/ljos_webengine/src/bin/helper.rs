//! The executable CEF starts its renderer, GPU and utility processes from.
//!
//! An app that shows web pages keeps its own `main`: CEF is told this
//! helper's path (`browser_subprocess_path`) and starts every other
//! process from it, so nothing in the app has to look at its arguments
//! first. Ship it beside the app's executable, or name it in
//! `LJOS_WEBENGINE_HELPER`.

fn main() {
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let code = cef::execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    std::process::exit(code.max(0));
}
