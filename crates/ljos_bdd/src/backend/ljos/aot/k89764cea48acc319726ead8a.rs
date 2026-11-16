use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Combo",
    content: [
        137, 118, 76, 234, 72, 172, 195, 25, 114, 110, 173, 138, 36, 231, 83, 13, 234, 67, 168,
        180, 230, 252, 73, 93, 139, 213, 48, 160, 1, 42, 192, 230,
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
