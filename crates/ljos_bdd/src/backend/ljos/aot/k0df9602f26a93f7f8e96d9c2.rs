use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        13, 249, 96, 47, 38, 169, 63, 127, 142, 150, 217, 194, 252, 13, 252, 121, 248, 127, 149, 9,
        1, 216, 57, 17, 130, 191, 150, 53, 88, 234, 87, 46,
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
    singletons: &[],
    methods: &[],
    exprs: &[Some(e0), Some(e1)],
};

// Scenario:10:18
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o0, 1)?;
    return Ok(rt::Value::Real(rt::js::min(
        0.0_f64,
        (f.num(o0, p0)? - f.num(o0, p1)?),
    )));
}

// Scenario:11:18
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o0, 2)?;
    return Ok(rt::Value::Real(rt::js::min(
        0.0_f64,
        (f.num(o0, p0)? - f.num(o0, p1)?),
    )));
}
