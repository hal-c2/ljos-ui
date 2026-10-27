use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        187, 227, 5, 77, 146, 195, 206, 218, 97, 13, 117, 217, 21, 12, 47, 216, 170, 216, 61, 222,
        0, 228, 232, 153, 174, 119, 82, 31, 107, 89, 221, 187,
    ],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite {
            name: "base",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "step",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "name",
            kind: rt::Kind::String,
            write: false,
        },
        rt::PropSite {
            name: "base",
            kind: rt::Kind::Number,
            write: true,
        },
    ],
    signals: &[],
    singletons: &[rt::Single::File { file: "Theme" }],
    methods: &[],
    exprs: &[Some(e0), Some(e1), Some(e2)],
};

// Scenario:6:32
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.singleton(0)?;
    let o1 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o1, 1)?;
    return Ok(rt::Value::Real((f.num(o0, p0)? * f.num(o1, p1)?)));
}

// Scenario:7:34
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.singleton(0)?;
    let o1 = f.object(0, 0)?;
    let p0 = f.prop(o0, 2)?;
    let p1 = f.prop(o1, 1)?;
    return Ok(rt::Value::String(rt::js::concat(
        &(f.str(o0, p0)?),
        &rt::js::number_text(f.num(o1, p1)?),
    )));
}

// Scenario:8:26
fn e2(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.singleton(0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o0, 3)?;
    let _ = {
        let l1 = (f.num(o0, p0)? + 1.0_f64);
        f.set(o0, p1, rt::Value::Real(l1))?;
        l1
    };
    return Ok(rt::Value::Undefined);
}
