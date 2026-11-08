use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        79, 24, 35, 56, 81, 105, 82, 96, 128, 213, 40, 159, 252, 128, 186, 62, 174, 88, 253, 44,
        234, 130, 113, 43, 13, 66, 45, 188, 67, 50, 91, 65,
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
