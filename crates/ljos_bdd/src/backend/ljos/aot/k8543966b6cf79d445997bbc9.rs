use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        133, 67, 150, 107, 108, 247, 157, 68, 89, 151, 187, 201, 98, 205, 93, 12, 163, 42, 76, 177,
        154, 166, 162, 198, 188, 48, 222, 244, 251, 98, 69, 122,
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
