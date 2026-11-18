use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Combo",
    content: [
        248, 219, 246, 36, 196, 63, 247, 214, 71, 73, 206, 111, 91, 211, 219, 83, 28, 135, 37, 191,
        123, 163, 11, 198, 1, 193, 101, 59, 19, 37, 37, 224,
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
