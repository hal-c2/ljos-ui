use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Combo",
    content: [
        136, 154, 220, 202, 89, 101, 116, 172, 87, 80, 91, 82, 136, 106, 201, 104, 157, 132, 212,
        96, 127, 22, 204, 143, 186, 84, 196, 226, 227, 82, 51, 168,
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
