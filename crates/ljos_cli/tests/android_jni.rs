//! Every method `ljos_host` calls on the activity through JNI is one the
//! template's `LjosActivity.java` declares, by name and signature. JNI looks
//! them up as the app runs, and a call that finds none only logs.
//!
//! Both files are read as text: it needs neither a JDK nor the Android SDK,
//! and says nothing of whether the Java compiles.

/// Included, so a change of either file runs this test (scripts/ci_changes.py).
const HOST: &str = include_str!("../../ljos_host/src/android.rs");
const ACTIVITY: &str =
    include_str!("../../../android/app/src/main/java/dev/ljos/LjosActivity.java");

/// What the activity has from `android.app.Activity`, as Android documents it.
const INHERITED: [(&str, &str); 2] = [("finish", "()V"), ("moveTaskToBack", "(Z)Z")];

/// A type's JNI descriptor, from Java's name for it or `jni_sig!`'s.
fn descriptor(ty: &str) -> String {
    if let Some(of) = ty.strip_suffix("[]") {
        return format!("[{}", descriptor(of));
    }
    match ty {
        "void" => "V".into(),
        "boolean" | "jboolean" => "Z".into(),
        "int" | "jint" => "I".into(),
        "long" | "jlong" => "J".into(),
        "float" | "jfloat" => "F".into(),
        "double" | "jdouble" => "D".into(),
        "String" => "Ljava/lang/String;".into(),
        qualified => format!("L{};", qualified.replace('.', "/")),
    }
}

/// `text` up to the parenthesis that closes the one opened just before it.
fn inside(text: &str) -> &str {
    let mut depth = 0;
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return &text[..at],
            ')' => depth -= 1,
            _ => {}
        }
    }
    panic!("a parenthesis is never closed: {text:.80}");
}

/// The name and signature of every JNI call in `rust`, all made on the activity.
fn called(rust: &str) -> Vec<(String, String)> {
    // A method called often is looked up once, on the activity's class,
    // and called by its ID.
    let looked_up = rust.split("env.get_method_id(").skip(1).map(|rest| {
        let call = inside(rest);
        assert!(call.trim_start().starts_with("&class,"), "{call}");
        call
    });
    let calls: Vec<_> = rust
        .split("env.call_method(")
        .skip(1)
        .map(|rest| {
            let call = inside(rest);
            assert!(call.trim_start().starts_with("&activity,"), "{call}");
            call
        })
        .chain(looked_up)
        .map(|call| {
            let (_, name) = call.split_once("jni_str!(\"").unwrap();
            let (name, _) = name.split_once('"').unwrap();
            let (_, sig) = call.split_once("jni_sig!(").unwrap();
            let sig = inside(sig).trim();
            let sig = if let Some(written_out) = sig.strip_prefix('"') {
                written_out.trim_end_matches('"').to_owned()
            } else {
                let (takes, gives) = sig.split_once("->").unwrap();
                let takes: String = takes
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split(',')
                    .map(str::trim)
                    .filter(|ty| !ty.is_empty())
                    .map(descriptor)
                    .collect();
                format!("({takes}){}", descriptor(gives.trim()))
            };
            (name.to_owned(), sig)
        })
        .collect();
    assert_eq!(
        calls.len(),
        rust.matches("jni_str!(").count(),
        "a JNI call of a form this test doesn't read"
    );
    calls
}

/// The name and signature of the public instance methods of the class itself,
/// whose members are indented once.
fn declared(java: &str) -> Vec<(String, String)> {
    java.split("\n    public ")
        .skip(1)
        .filter_map(|member| {
            let (head, rest) = member.split_once('(')?;
            if head.contains([';', '{', '=']) {
                return None;
            }
            let head: Vec<&str> = head.split_whitespace().collect();
            let [.., gives, name] = head.as_slice() else {
                return None;
            };
            if head.contains(&"static") {
                return None;
            }
            let takes: String = inside(rest)
                .split(',')
                .filter_map(|param| param.split_whitespace().rev().nth(1))
                .map(descriptor)
                .collect();
            let sig = format!("({takes}){}", descriptor(gives));
            Some(((*name).to_owned(), sig))
        })
        .collect()
}

#[test]
fn every_method_the_host_calls_on_the_activity_is_declared_as_it_calls_it() {
    let called = called(HOST);
    for name in [
        "textInputWriteComing",
        "textInputWriteDropped",
        "textInputToldWrite",
        "textInputHanded",
        "inputStateChanged",
    ] {
        assert!(
            called.iter().any(|(called, _)| called == name),
            "{name} isn't among the calls read: {called:?}"
        );
    }
    let declared = declared(ACTIVITY);
    let missing: Vec<_> = called
        .iter()
        .filter(|(name, sig)| {
            !declared.contains(&(name.clone(), sig.clone()))
                && !INHERITED.contains(&(name.as_str(), sig.as_str()))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "called by ljos_host and not declared so by LjosActivity.java: {missing:?}\n\
         it declares {declared:?}"
    );
}
