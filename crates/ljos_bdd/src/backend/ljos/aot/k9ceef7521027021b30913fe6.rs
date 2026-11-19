use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        156, 238, 247, 82, 16, 39, 2, 27, 48, 145, 63, 230, 251, 108, 73, 75, 36, 208, 54, 140, 1,
        180, 217, 52, 57, 217, 103, 141, 159, 174, 53, 44,
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
