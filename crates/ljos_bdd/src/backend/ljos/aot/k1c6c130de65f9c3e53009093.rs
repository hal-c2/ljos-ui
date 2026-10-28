use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        28, 108, 19, 13, 230, 95, 156, 62, 83, 0, 144, 147, 255, 30, 54, 2, 109, 168, 31, 141, 229,
        26, 100, 185, 62, 44, 66, 171, 80, 165, 158, 247,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "r",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    singletons: &[],
    methods: &[],
    exprs: &[Some(e0)],
};

// Scenario:5:31
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real((f.num(o0, p0)? * 2.0_f64)));
}
