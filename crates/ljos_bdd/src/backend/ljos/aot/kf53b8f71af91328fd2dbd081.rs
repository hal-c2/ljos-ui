use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        245, 59, 143, 113, 175, 145, 50, 143, 210, 219, 208, 129, 14, 202, 230, 145, 229, 15, 194,
        10, 66, 155, 3, 13, 140, 45, 92, 69, 125, 12, 207, 219,
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
