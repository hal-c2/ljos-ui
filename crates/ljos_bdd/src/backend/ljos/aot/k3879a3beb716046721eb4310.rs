use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        56, 121, 163, 190, 183, 22, 4, 103, 33, 235, 67, 16, 71, 155, 154, 227, 185, 7, 204, 93,
        61, 38, 124, 176, 65, 84, 55, 93, 193, 107, 188, 211,
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
