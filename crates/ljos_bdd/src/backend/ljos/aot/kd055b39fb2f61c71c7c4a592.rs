use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Level",
    content: [
        208, 85, 179, 159, 178, 246, 28, 113, 199, 196, 165, 146, 114, 53, 116, 246, 76, 212, 79,
        210, 35, 82, 23, 176, 139, 173, 36, 44, 171, 218, 197, 243,
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
