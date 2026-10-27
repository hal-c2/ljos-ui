use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        185, 59, 158, 1, 226, 131, 141, 78, 119, 33, 66, 190, 79, 116, 31, 15, 250, 29, 141, 148,
        228, 80, 126, 100, 63, 129, 91, 210, 229, 107, 102, 172,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "size",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    singletons: &[],
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
