use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Level",
    content: [
        253, 201, 28, 105, 236, 210, 153, 169, 59, 220, 216, 186, 177, 189, 251, 79, 94, 118, 199,
        98, 179, 210, 26, 141, 51, 89, 94, 224, 78, 233, 164, 187,
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
