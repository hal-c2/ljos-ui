use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        222, 95, 158, 37, 28, 133, 53, 252, 178, 229, 131, 255, 236, 241, 178, 65, 252, 107, 151,
        125, 229, 110, 32, 254, 90, 156, 10, 71, 217, 56, 171, 61,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "modifiers",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    singletons: &[],
    methods: &[],
    exprs: &[Some(e0), Some(e1)],
};

// Scenario:6:34
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Bool(
        !(rt::js::bit_and(f.num(o0, p0)?, 67108864.0_f64) == 0.0_f64),
    ));
}

// Scenario:7:32
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Bool((f.num(o0, p0)? == 49.0_f64)));
}
