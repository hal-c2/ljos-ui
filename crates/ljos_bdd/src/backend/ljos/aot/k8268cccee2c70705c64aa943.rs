use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Level",
    content: [130, 104, 204, 206, 226, 199, 7, 5, 198, 74, 169, 67, 29, 152, 196, 104, 221, 59, 2, 160, 37, 29, 14, 60, 248, 232, 29, 125, 10, 242, 163, 30],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite { name: "depth", kind: rt::Kind::Number, write: false },
    ],
    signals: &[],
    methods: &[
    ],
    exprs: &[
        Some(e0),
        None, // assignment: to a member of a value
    ],
};

// Level:4:19
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
let o0 = f.object(0, 0)?;
let p0 = f.prop(o0, 0)?;
return Ok(rt::Value::String(if (f.num(o0, p0)? > 0.0_f64) { rt::Atom::from("Level.qml") } else { rt::Atom::from("Combo.qml") }));
}
