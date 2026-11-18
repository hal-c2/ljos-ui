use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Level",
    content: [
        89, 130, 65, 159, 110, 244, 204, 68, 111, 63, 38, 68, 246, 202, 113, 64, 57, 170, 27, 155,
        123, 70, 151, 22, 222, 119, 78, 68, 146, 49, 186, 92,
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
