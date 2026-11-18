use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        106, 45, 243, 14, 156, 235, 7, 246, 46, 193, 228, 104, 135, 16, 43, 199, 228, 45, 147, 58,
        214, 156, 224, 79, 98, 60, 127, 80, 133, 227, 186, 136,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "size",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    methods: &[],
    exprs: &[
        None, // type: var property items
        Some(e1),
        None, // array
    ],
};

// Scenario:7:31
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real((f.num(o0, p0)? * 2.0_f64)));
}
