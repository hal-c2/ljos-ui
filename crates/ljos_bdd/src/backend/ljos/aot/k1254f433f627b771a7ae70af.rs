use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Level",
    content: [
        18, 84, 244, 51, 246, 39, 183, 113, 167, 174, 112, 175, 235, 39, 173, 92, 195, 195, 246,
        157, 157, 28, 61, 113, 216, 116, 27, 164, 240, 109, 120, 192,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "depth",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    methods: &[],
    exprs: &[
        Some(e0),
        None, // assignment: to a member of a value
    ],
};

// Level:4:19
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::String(if (f.num(o0, p0)? > 0.0_f64) {
        rt::Atom::from("Level.qml")
    } else {
        rt::Atom::from("Combo.qml")
    }));
}
