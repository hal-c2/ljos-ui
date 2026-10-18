//! Colours in JS: one object per distinct colour, with the members and the
//! text Qt gives a `color` value (taken from qml6 6.11.2 runs).

use std::{rc::Rc, time::Duration};

use ljos_js::{HostError, JsConfig, JsFault, JsHost, JsValue, ModuleHandle, Scope, Slice};

const RED: JsValue = JsValue::Color([65535, 0, 0, 65535]);

fn host() -> JsHost {
    JsHost::new(JsConfig::default()).unwrap()
}

fn module(host: &JsHost, source: &str) -> ModuleHandle {
    host.load_module("m.js", source).unwrap()
}

fn call(host: &JsHost, m: &ModuleHandle, name: &str, args: &[JsValue]) -> JsValue {
    host.call(m.function(name).unwrap(), args).unwrap()
}

/// `#rrggbb` or `#aarrggbb` as the 16-bit channels it stands for.
fn hex(text: &str) -> JsValue {
    let digits = text.trim_start_matches('#');
    let byte = |i: usize| u16::from_str_radix(&digits[i..i + 2], 16).unwrap() * 257;
    match digits.len() {
        6 => JsValue::Color([byte(0), byte(2), byte(4), 65535]),
        _ => JsValue::Color([byte(2), byte(4), byte(6), byte(0)]),
    }
}

#[test]
fn a_colour_crosses_both_ways_unchanged() {
    let host = host();
    let m = module(&host, "export function id(x) { return x; }");
    for c in [
        RED,
        JsValue::Color([0, 0, 0, 0]),
        JsValue::Color([1, 2, 3, 4]),
        JsValue::Color([32768, 16384, 8192, 49151]),
        JsValue::Array(vec![RED, JsValue::Color([0, 0, 65535, 65535])]),
    ] {
        assert_eq!(call(&host, &m, "id", std::slice::from_ref(&c)), c);
    }
}

#[test]
fn equal_colours_are_the_same_object() {
    let host = host();
    let m = module(
        &host,
        "export function same(a, b) { return a === b; }
         export function find(list, c) { return list.indexOf(c); }
         export function kind(c) { return typeof c; }",
    );
    assert_eq!(call(&host, &m, "same", &[RED, RED]), JsValue::Bool(true));
    assert_eq!(
        call(
            &host,
            &m,
            "same",
            &[RED, JsValue::Color([65534, 0, 0, 65535])]
        ),
        JsValue::Bool(false)
    );
    let list = JsValue::Array(vec![hex("#0000ff"), RED]);
    assert_eq!(call(&host, &m, "find", &[list, RED]), JsValue::Number(1.0));
    assert_eq!(call(&host, &m, "kind", &[RED]), JsValue::from("object"));
}

#[test]
fn a_colour_kept_by_script_is_still_the_one_handed_out_later() {
    let host = host();
    let m = module(
        &host,
        "let kept; export function keep(c) { kept = c; } export function isKept(c) { return c === kept; }",
    );
    call(&host, &m, "keep", &[RED]);
    host.collect_now();
    assert_eq!(call(&host, &m, "isKept", &[RED]), JsValue::Bool(true));
}

/// A colour that crossed and was let go is the one made before when it
/// crosses again: not made again, members and all, each time it crosses.
#[test]
fn a_colour_let_go_is_not_made_again_while_it_is_recent() {
    let host = host();
    let m = module(
        &host,
        "let seen; export function watch(c) { seen = new WeakRef(c); } \
         export function same(c) { return seen.deref() === c; }",
    );
    call(&host, &m, "watch", &[RED]);
    host.collect_now();
    assert_eq!(call(&host, &m, "same", &[RED]), JsValue::Bool(true));
}

#[test]
fn a_colour_reads_as_its_hex_text() {
    let host = host();
    let m = module(
        &host,
        "export function text(c) { return 'is ' + c; }
         export function loose(c, s) { return c == s; }
         export function strict(c, s) { return c === s; }",
    );
    let text = |c: JsValue| call(&host, &m, "text", &[c]);
    assert_eq!(text(RED), JsValue::from("is #ff0000"));
    assert_eq!(text(hex("#80804000")), JsValue::from("is #80804000"));
    // Alpha is left out only when it is opaque in eight bits.
    assert_eq!(
        text(JsValue::Color([65535, 0, 0, 65469])),
        JsValue::from("is #ff0000")
    );
    assert_eq!(
        text(JsValue::Color([65535, 0, 0, 65338])),
        JsValue::from("is #feff0000")
    );
    let eq = |f: &str, s: &str| call(&host, &m, f, &[RED, JsValue::from(s)]);
    assert_eq!(eq("loose", "#ff0000"), JsValue::Bool(true));
    assert_eq!(eq("loose", "red"), JsValue::Bool(false));
    assert_eq!(eq("strict", "#ff0000"), JsValue::Bool(false));
    assert_eq!(
        host.enter("d", host.config().slice, |s| Ok(s.display(&RED)))
            .unwrap(),
        "#ff0000"
    );
}

/// Every line is what qml6 prints for
/// `[c.r, c.g, c.b, c.a, c.hsvHue, c.hsvSaturation, c.hsvValue, c.hslHue,
/// c.hslSaturation, c.hslLightness, c.valid].join(" ")`.
#[test]
fn the_members_have_the_values_qt_gives() {
    let host = host();
    let m = module(
        &host,
        "export function show(c) {
           return [c.r, c.g, c.b, c.a, c.hsvHue, c.hsvSaturation, c.hsvValue,
                   c.hslHue, c.hslSaturation, c.hslLightness, c.valid].join(' ');
         }
         export function keys(c) { return Object.keys(c).join(' '); }",
    );
    let cases = [
        (
            hex("#204080"),
            "0.125490203499794 0.250980406999588 0.501960813999176 1 0.6111111044883728 0.7499961853027344 0.501960813999176 0.6111111044883728 0.6000000238418579 0.3137255012989044 true",
        ),
        (
            hex("#808080"),
            "0.501960813999176 0.501960813999176 0.501960813999176 1 -1 0 0.501960813999176 -1 0 0.501960813999176 true",
        ),
        (hex("#000000"), "0 0 0 1 -1 0 0 -1 0 0 true"),
        (hex("#ffffff"), "1 1 1 1 -1 0 1 -1 0 1 true"),
        (
            hex("#ff8000"),
            "1 0.501960813999176 0 1 0.08366666734218597 1 1 0.08366666734218597 1 0.5000076293945312 true",
        ),
        (
            hex("#123456"),
            "0.07058823853731155 0.20392157137393951 0.33725491166114807 1 0.5833333134651184 0.7906919717788696 0.33725491166114807 0.5833333134651184 0.6538490653038025 0.20392157137393951 true",
        ),
        (
            hex("#fedcba"),
            "0.9960784316062927 0.8627451062202454 0.729411780834198 1 0.0833333358168602 0.2677195370197296 0.9960784316062927 0.0833333358168602 0.9714351296424866 0.8627451062202454 true",
        ),
        (
            hex("#00ff00"),
            "0 1 0 1 0.3333333432674408 1 1 0.3333333432674408 1 0.5000076293945312 true",
        ),
        (
            hex("#0000ff"),
            "0 0 1 1 0.6666666865348816 1 1 0.6666666865348816 1 0.5000076293945312 true",
        ),
        (
            hex("#ff00ff"),
            "1 0 1 1 0.8333333134651184 1 1 0.8333333134651184 1 0.5000076293945312 true",
        ),
        (
            hex("#010203"),
            "0.003921568859368563 0.007843137718737125 0.0117647061124444 1 0.5833333134651184 0.6666666865348816 0.0117647061124444 0.5833333134651184 0.5000076293945312 0.007843137718737125 true",
        ),
        (
            hex("#7f7f80"),
            "0.49803921580314636 0.49803921580314636 0.501960813999176 1 0.6666666865348816 0.00781261920928955 0.501960813999176 0.6666666865348816 0.003921568859368563 0.5000076293945312 true",
        ),
        (
            hex("#80ff8000"),
            "1 0.501960813999176 0 0.501960813999176 0.08366666734218597 1 1 0.08366666734218597 1 0.5000076293945312 true",
        ),
        (
            hex("#c0ffee"),
            "0.7529411911964417 1 0.9333333373069763 1 0.4550277888774872 0.24705882370471954 1 0.4550277888774872 1 0.8764781951904297 true",
        ),
        (
            hex("#336699"),
            "0.20000000298023224 0.4000000059604645 0.6000000238418579 1 0.5833333134651184 0.6666666865348816 0.6000000238418579 0.5833333134651184 0.5000076293945312 0.4000000059604645 true",
        ),
        (
            hex("#ff0001"),
            "1 0 0.003921568859368563 1 0.9993333220481873 1 1 0.9993333220481873 1 0.5000076293945312 true",
        ),
        (hex("#00000000"), "0 0 0 0 -1 0 0 -1 0 0 true"),
        // Qt.rgba(0.5, 0.25, 0.125, 0.75), Qt.rgba(0.1, 0.2, 0.3, 1) and
        // Qt.rgba(0.9, 0.9, 0.3, 0.1): channels that are not whole bytes.
        (
            JsValue::Color([32768, 16384, 8192, 49151]),
            "0.5000076293945312 0.2500038146972656 0.1250019073486328 0.7499961853027344 0.0555555559694767 0.7499961853027344 0.5000076293945312 0.0555555559694767 0.6000000238418579 0.31250476837158203 true",
        ),
        (
            JsValue::Color([6554, 13107, 19661, 65535]),
            "0.10000763088464737 0.20000000298023224 0.3000076413154602 1 0.5833333134651184 0.6666514277458191 0.3000076413154602 0.5833333134651184 0.49997711181640625 0.20001526176929474 true",
        ),
        (
            JsValue::Color([58982, 58982, 19661, 6554]),
            "0.9000076055526733 0.9000076055526733 0.3000076413154602 0.10000763088464737 0.1666666716337204 0.6666666865348816 0.9000076055526733 0.1666666716337204 0.7500114440917969 0.6000152826309204 true",
        ),
    ];
    for (color, expected) in cases {
        assert_eq!(
            call(&host, &m, "show", std::slice::from_ref(&color)),
            JsValue::from(expected),
            "{color:?}"
        );
    }
    assert_eq!(
        call(&host, &m, "keys", &[RED]),
        JsValue::from(
            "r g b a hsvHue hsvSaturation hsvValue hslHue hslSaturation hslLightness valid"
        )
    );
}

#[test]
fn a_colour_cannot_be_changed_in_place() {
    let host = host();
    let m = module(
        &host,
        "export function write(c) { c.b = 1; return c; }
         export function sloppy(c) { return Function('c', 'c.b = 1; return c.b;')(c); }",
    );
    // One object stands for every red there is: a write must not turn them
    // all magenta.
    let err = host.call(m.function("write").unwrap(), &[RED]).unwrap_err();
    assert!(matches!(err, JsFault::JsException { .. }), "{err:?}");
    assert_eq!(call(&host, &m, "sloppy", &[RED]), JsValue::Number(0.0));
}

#[test]
fn an_object_on_the_colour_prototype_without_channels_is_not_a_colour() {
    let host = host();
    let m = module(
        &host,
        "export function fake(c) { return Object.create(Object.getPrototypeOf(c)); }
         export function plain() { return { r: 1, g: 0, b: 0, a: 1 }; }",
    );
    assert!(matches!(
        call(&host, &m, "fake", &[RED]),
        JsValue::Handle(_)
    ));
    assert!(matches!(call(&host, &m, "plain", &[]), JsValue::Handle(_)));
}

#[test]
fn colours_nothing_holds_are_forgotten() {
    let host = host();
    host.set_global_function(
        "color",
        Rc::new(|_s: &Scope<'_>, args: &[JsValue]| match args {
            [JsValue::Number(r), JsValue::Number(g)] => {
                Ok(JsValue::Color([*r as u16, *g as u16, 0, 65535]))
            }
            _ => Err(HostError::msg("color needs two numbers")),
        }),
    )
    .unwrap();
    let m = module(
        &host,
        "export function churn(from, to) {
           for (let i = from; i < to; i++) color(i & 0xffff, i >> 16);
         }",
    );
    let churn = |from: f64, to: f64| {
        let slice = Slice {
            soft: Duration::from_secs(60),
            hard: Duration::from_secs(60),
        };
        host.enter("churn", slice, |s| {
            s.call(m.function("churn").unwrap(), &[from.into(), to.into()])
        })
        .unwrap();
    };
    // Warm up, so the table and its first sweeps are already paid for.
    churn(0.0, 2_000.0);
    host.collect_now();
    let before = host.memory_stats().unwrap().malloc_size;
    churn(2_000.0, 150_000.0);
    host.collect_now();
    let after = host.memory_stats().unwrap().malloc_size;
    // Unswept, 148 000 dead entries would cost megabytes.
    assert!(after < before + 256 * 1024, "{before} -> {after}");
}
