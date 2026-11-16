use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        255, 179, 54, 244, 8, 250, 248, 143, 219, 57, 149, 66, 238, 113, 187, 129, 79, 96, 193,
        126, 8, 20, 2, 186, 178, 113, 74, 230, 31, 114, 215, 212,
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
