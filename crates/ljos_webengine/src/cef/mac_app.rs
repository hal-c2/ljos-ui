//! The application object CEF expects on macOS.
//!
//! Chromium asks `NSApp` whether it is inside `-sendEvent:` and tells it
//! when it is (`CrAppProtocol`, `CrAppControlProtocol`, which CEF's
//! `CefAppProtocol` extends). An application made for CEF has an
//! `NSApplication` subclass that answers. A Ljós application's `NSApp` is
//! whatever its windowing library made, which knows nothing of them, so
//! the first event Chromium handles after a page is shown ends the process
//! with "unrecognized selector `isHandlingSendEvent`".
//!
//! So before CEF starts, the class of the running `NSApp` is taught those
//! messages: one flag for the process (there is one `NSApp`), set around
//! the class's own `-sendEvent:` as Chromium's applications set it.

use std::ffi::{CStr, CString};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::encode::Encode;
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, Bool, Imp, Sel};
use objc2::{MainThreadMarker, class, ffi, msg_send, sel};

/// Whether `NSApp` is inside `-sendEvent:`, as Chromium reads it.
static HANDLING: AtomicBool = AtomicBool::new(false);

/// The `-sendEvent:` the class had before it was taught.
static SEND_EVENT: OnceLock<Imp> = OnceLock::new();

/// Chromium's protocols, which a class conforms to as well when CEF's
/// framework registered them (`conformsToProtocol:` checks in Chromium).
const PROTOCOLS: [&CStr; 3] = [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"];

type SendEvent = unsafe extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject);
type IsHandling = unsafe extern "C-unwind" fn(&AnyObject, Sel) -> Bool;
type SetHandling = unsafe extern "C-unwind" fn(&AnyObject, Sel, Bool);

unsafe extern "C-unwind" fn is_handling_send_event(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::new(HANDLING.load(Ordering::Relaxed))
}

unsafe extern "C-unwind" fn set_handling_send_event(_this: &AnyObject, _cmd: Sel, handling: Bool) {
    HANDLING.store(handling.as_bool(), Ordering::Relaxed);
}

unsafe extern "C-unwind" fn send_event(this: &AnyObject, cmd: Sel, event: *mut AnyObject) {
    /// Puts the flag back however `-sendEvent:` ends: an event handled
    /// inside another (a nested run loop) leaves the outer one's flag.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            HANDLING.store(self.0, Ordering::Relaxed);
        }
    }
    let _restore = Restore(HANDLING.swap(true, Ordering::Relaxed));
    if let Some(original) = SEND_EVENT.get() {
        // SAFETY: it is the class's `-sendEvent:`, whose signature this
        // is; `teach` stored it before replacing it with this.
        unsafe {
            let original: SendEvent = std::mem::transmute::<Imp, SendEvent>(*original);
            original(this, cmd, event);
        }
    }
}

fn types(ret: &str, args: &[&str]) -> CString {
    let mut s = format!("{ret}{}{}", <*mut AnyObject>::ENCODING, Sel::ENCODING);
    for a in args {
        s.push_str(a);
    }
    CString::new(s).expect("encodings have no NUL")
}

/// Teaches the running `NSApp` (made now if nothing has made it) CEF's
/// application protocol. Once per process; it must run on the main thread,
/// where AppKit and CEF do.
pub(super) fn answer_cef_app_protocol() -> Result<(), String> {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| {
        if MainThreadMarker::new().is_none() {
            return Err(
                "CEF was started off the main thread, so NSApp could not be taught \
                        CEF's application protocol (isHandlingSendEvent): Chromium ends the \
                        process when it asks"
                    .to_owned(),
            );
        }
        // SAFETY: on the main thread, as checked; `sharedApplication`
        // returns the process's application, made once and never freed.
        let app: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };
        // SAFETY: `sharedApplication` never returns nil.
        let class = unsafe { &*app }.class();
        teach(class)
    })
    .clone()
}

/// Gives `class` the messages of CEF's application protocol, unless it
/// has them already (an application with its own CEF-aware subclass).
/// Nothing is changed unless all of it can be: a class half taught would
/// still end the process at Chromium's first message it lacks.
fn teach(class: &AnyClass) -> Result<(), String> {
    let name = class.name().to_string_lossy().into_owned();
    let failed = |what: &str| format!("could not teach {name} CEF's application protocol: {what}");
    match (
        class.responds_to(sel!(isHandlingSendEvent)),
        class.responds_to(sel!(setHandlingSendEvent:)),
    ) {
        (true, true) => return Ok(()),
        (false, false) => {}
        _ => {
            return Err(failed(
                "it answers only one of isHandlingSendEvent and setHandlingSendEvent:, \
                 and Chromium sends both",
            ));
        }
    }
    let cls = class as *const AnyClass as *mut AnyClass;
    let send = sel!(sendEvent:);
    let original = class
        .instance_method(send)
        .ok_or_else(|| failed("it has no -sendEvent:"))?
        .implementation();
    SEND_EVENT
        .set(original)
        .map_err(|_| failed("-sendEvent: was replaced once already"))?;
    let bool_ty = Bool::ENCODING.to_string();
    let obj_ty = <*mut AnyObject>::ENCODING.to_string();
    // SAFETY: each implementation has the signature its type string says,
    // and the class outlives the process (classes are never freed).
    unsafe {
        let is: IsHandling = is_handling_send_event;
        let set: SetHandling = set_handling_send_event;
        let send_imp: SendEvent = send_event;
        if !ffi::class_addMethod(
            cls,
            sel!(isHandlingSendEvent),
            std::mem::transmute::<IsHandling, Imp>(is),
            types(&bool_ty, &[]).as_ptr(),
        )
        .as_bool()
        {
            return Err(failed("isHandlingSendEvent was not added"));
        }
        if !ffi::class_addMethod(
            cls,
            sel!(setHandlingSendEvent:),
            std::mem::transmute::<SetHandling, Imp>(set),
            types("v", &[&bool_ty]).as_ptr(),
        )
        .as_bool()
        {
            return Err(failed("setHandlingSendEvent: was not added"));
        }
        // Replaced on the class itself: when -sendEvent: is inherited this
        // adds it there, leaving NSApplication's own as `original`.
        ffi::class_replaceMethod(
            cls,
            send,
            std::mem::transmute::<SendEvent, Imp>(send_imp),
            types("v", &[&obj_ty]).as_ptr(),
        );
        for protocol in PROTOCOLS.iter().filter_map(|p| AnyProtocol::get(p)) {
            ffi::class_addProtocol(cls, protocol);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, NSObject, Sel};
    use objc2::{ClassType, msg_send, sel};

    /// What `isHandlingSendEvent` said inside the class's own
    /// `-sendEvent:` (0 never asked, 1 no, 2 yes).
    static SEEN: AtomicU8 = AtomicU8::new(0);

    unsafe extern "C-unwind" fn own_send_event(
        this: &AnyObject,
        _cmd: Sel,
        _event: *mut AnyObject,
    ) {
        // SAFETY: the class was taught the message before this runs.
        let handling: Bool = unsafe { msg_send![this, isHandlingSendEvent] };
        SEEN.store(if handling.as_bool() { 2 } else { 1 }, Ordering::Relaxed);
    }

    /// A stand-in for a windowing library's application class: its own
    /// `-sendEvent:`, and none of Chromium's messages.
    fn library_application() -> &'static AnyClass {
        let mut builder =
            ClassBuilder::new(c"LjosTestApplication", NSObject::class()).expect("a new class");
        // SAFETY: the signature is `-sendEvent:`'s.
        unsafe {
            builder.add_method(
                sel!(sendEvent:),
                own_send_event as unsafe extern "C-unwind" fn(_, _, _),
            );
        }
        builder.register()
    }

    unsafe extern "C-unwind" fn never_handling(_this: &AnyObject, _cmd: Sel) -> Bool {
        Bool::NO
    }

    #[test]
    fn a_class_answering_half_of_the_protocol_is_left_as_it_was() {
        let mut builder =
            ClassBuilder::new(c"LjosHalfAwareApplication", NSObject::class()).expect("a new class");
        // SAFETY: the signatures are `-sendEvent:`'s and `-isHandlingSendEvent`'s.
        unsafe {
            builder.add_method(
                sel!(sendEvent:),
                own_send_event as unsafe extern "C-unwind" fn(_, _, _),
            );
            builder.add_method(
                sel!(isHandlingSendEvent),
                never_handling as unsafe extern "C-unwind" fn(_, _) -> _,
            );
        }
        let class = builder.register();
        let own_send = class
            .instance_method(sel!(sendEvent:))
            .expect("its own -sendEvent:")
            .implementation();

        let why = super::teach(class).expect_err("a half-aware class is refused");

        assert!(
            why.contains("LjosHalfAwareApplication") && why.contains("setHandlingSendEvent:"),
            "the warning names the class and what it lacks: {why}"
        );
        assert!(
            !class.responds_to(sel!(setHandlingSendEvent:)),
            "nothing was added"
        );
        assert_eq!(
            class
                .instance_method(sel!(sendEvent:))
                .expect("still there")
                .implementation() as usize,
            own_send as usize,
            "its -sendEvent: was not replaced"
        );
    }

    #[test]
    fn an_application_class_answers_what_chromium_asks_of_it() {
        let class = library_application();
        assert!(
            !class.responds_to(sel!(isHandlingSendEvent)),
            "the stand-in starts as a windowing library's class does"
        );

        super::teach(class).expect("taught");

        assert!(class.responds_to(sel!(isHandlingSendEvent)));
        assert!(class.responds_to(sel!(setHandlingSendEvent:)));
        // SAFETY: a plain object of a registered class, and messages it
        // now answers.
        unsafe {
            let app: *mut AnyObject = msg_send![class, new];
            let app = &*app;
            let before: Bool = msg_send![app, isHandlingSendEvent];
            assert!(!before.as_bool(), "not handling an event before one comes");

            let _: () = msg_send![app, sendEvent: std::ptr::null_mut::<AnyObject>()];
            assert_eq!(
                SEEN.load(Ordering::Relaxed),
                2,
                "the class's own -sendEvent: still runs, and inside it NSApp says it is handling one"
            );
            let after: Bool = msg_send![app, isHandlingSendEvent];
            assert!(!after.as_bool(), "the flag is put back after the event");

            let _: () = msg_send![app, setHandlingSendEvent: Bool::YES];
            let set: Bool = msg_send![app, isHandlingSendEvent];
            assert!(set.as_bool(), "Chromium can set it itself");
            let _: () = msg_send![app, setHandlingSendEvent: Bool::NO];
            let _: () = msg_send![app, release];
        }
    }
}
