use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        187, 91, 200, 122, 64, 163, 221, 57, 203, 175, 237, 123, 156, 247, 94, 234, 112, 143, 38,
        191, 102, 187, 199, 25, 57, 186, 115, 246, 165, 45, 1, 50,
    ],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite {
            name: "width",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "minimumWidth",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "gap",
            kind: rt::Kind::Number,
            write: false,
        },
    ],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0), Some(e1)],
};

// Scenario:8:39
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o0, 1)?;
    return Ok(rt::Value::Real(rt::js::min(
        0.0_f64,
        (f.num(o0, p0)? - f.num(o0, p1)?),
    )));
}

// Scenario:9:39
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o0, 2)?;
    return Ok(rt::Value::Real(rt::js::min(
        0.0_f64,
        (f.num(o0, p0)? - f.num(o0, p1)?),
    )));
}
