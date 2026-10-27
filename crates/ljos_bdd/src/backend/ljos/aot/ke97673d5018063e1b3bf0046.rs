use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        233, 118, 115, 213, 1, 128, 99, 225, 179, 191, 0, 70, 83, 8, 176, 24, 225, 84, 66, 57, 54,
        52, 167, 213, 156, 41, 128, 162, 207, 106, 226, 117,
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
