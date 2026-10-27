use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        176, 193, 89, 237, 94, 183, 82, 43, 117, 202, 35, 41, 159, 133, 139, 51, 107, 61, 15, 84,
        126, 206, 111, 255, 126, 52, 222, 129, 67, 14, 140, 4,
    ],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite {
            name: "parent",
            kind: rt::Kind::Object,
            write: false,
        },
        rt::PropSite {
            name: "width",
            kind: rt::Kind::Number,
            write: false,
        },
    ],
    signals: &[],
    singletons: &[],
    methods: &[],
    exprs: &[
        Some(e0),
        Some(e1),
        Some(e2),
        Some(e3),
        Some(e4),
        Some(e5),
        None, // type: var property held
    ],
};

// Scenario:8:22
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 1)?;
    let o1 = f.via(o0, 0)?;
    let p0 = f.prop(o1, 1)?;
    return Ok(rt::Value::Real((f.num(o1, p0)? / 2.0_f64)));
}

// Scenario:9:35
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 1)?;
    let o1 = f.via(o0, 0)?;
    let p0 = f.prop(o1, 1)?;
    return Ok(rt::Value::Bool((f.num(o1, p0)? > 50.0_f64)));
}

// Scenario:12:26
fn e2(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 2)?;
    let o1 = f.via(o0, 0)?;
    let o2 = f.via(o1, 0)?;
    let p0 = f.prop(o2, 1)?;
    let p1 = f.prop(o1, 1)?;
    return Ok(rt::Value::Real((f.num(o2, p0)? - f.num(o1, p1)?)));
}

// Scenario:17:29
fn e3(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 3)?;
    let o1 = f.via(o0, 0)?;
    return Ok(o1.value());
}

// Scenario:18:34
fn e4(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 3)?;
    let o1 = f.via(o0, 0)?;
    return Ok(o1.value());
}

// Scenario:19:35
fn e5(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 3)?;
    let o1 = f.via(o0, 0)?;
    return Ok(o1.value());
}
