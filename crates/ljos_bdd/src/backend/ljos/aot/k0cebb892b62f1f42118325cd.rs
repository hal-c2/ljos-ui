use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Combo",
    content: [
        12, 235, 184, 146, 182, 47, 31, 66, 17, 131, 37, 205, 49, 2, 37, 163, 27, 68, 218, 132, 33,
        102, 178, 156, 217, 62, 103, 24, 165, 182, 52, 29,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "width",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0)],
};

// Combo:6:54
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real(rt::js::max(f.num(o0, p0)?, 160.0_f64)));
}
