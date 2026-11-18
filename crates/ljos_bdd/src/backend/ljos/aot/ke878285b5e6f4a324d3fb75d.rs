use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        232, 120, 40, 91, 94, 111, 74, 50, 77, 63, 183, 93, 146, 44, 70, 155, 87, 170, 213, 40,
        223, 49, 68, 31, 146, 243, 186, 180, 56, 213, 207, 136,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "start",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &["ping"],
    methods: &[],
    exprs: &[
        None, // statement: a variable declared in a switch case
        Some(e1),
    ],
};

// Scenario:14:27
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let s0 = f.signal(o0, 0)?;
    let _ = {
        let l1 = rt::Value::Real(f.num(o0, p0)?);
        f.emit(o0, s0, &[l1])?
    };
    return Ok(rt::Value::Undefined);
}
