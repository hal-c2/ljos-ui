use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        104, 64, 53, 38, 191, 68, 122, 228, 142, 48, 217, 184, 13, 163, 187, 207, 123, 123, 57,
        178, 196, 57, 168, 45, 79, 18, 128, 196, 35, 78, 41, 191,
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
