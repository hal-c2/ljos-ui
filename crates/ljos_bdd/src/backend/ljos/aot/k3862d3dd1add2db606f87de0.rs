use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        56, 98, 211, 221, 26, 221, 45, 182, 6, 248, 125, 224, 120, 181, 105, 245, 60, 173, 116, 7,
        81, 46, 183, 233, 97, 11, 246, 156, 238, 188, 134, 2,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "shown",
        kind: rt::Kind::Bool,
        write: false,
    }],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0)],
};

// Scenario:8:21
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Bool(f.bool(o0, p0)?));
}
