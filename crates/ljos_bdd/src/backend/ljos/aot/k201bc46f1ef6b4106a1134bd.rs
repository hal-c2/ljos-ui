use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        32, 27, 196, 111, 30, 246, 180, 16, 106, 17, 52, 189, 56, 35, 10, 83, 37, 5, 6, 239, 217,
        34, 219, 27, 138, 211, 229, 52, 150, 0, 82, 198,
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
