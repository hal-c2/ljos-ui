use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Combo",
    content: [
        15, 162, 122, 221, 184, 11, 192, 221, 100, 126, 73, 158, 96, 49, 51, 239, 166, 53, 250, 82,
        127, 72, 135, 50, 13, 214, 179, 97, 150, 218, 164, 40,
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
            name: "count",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "padding",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "height",
            kind: rt::Kind::Number,
            write: false,
        },
    ],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0), Some(e1), Some(e2)],
};

// Combo:8:20
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real(rt::js::max(f.num(o0, p0)?, 160.0_f64)));
}

// Combo:9:29
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let o1 = f.object(0, 1)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o1, 2)?;
    return Ok(rt::Value::Real(
        ((rt::js::min(
            f.num(o0, p0)?,
            rt::js::floor(((320.0_f64 - (2.0_f64 * f.num(o1, p1)?)) / 30.0_f64)),
        ) * 30.0_f64)
            + (2.0_f64 * f.num(o1, p1)?)),
    ));
}

// Combo:11:16
fn e2(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 3)?;
    return Ok(rt::Value::Real(f.num(o0, p0)?));
}
