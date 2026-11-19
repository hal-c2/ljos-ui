use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        68, 104, 79, 51, 156, 133, 70, 215, 63, 100, 5, 138, 180, 152, 242, 31, 39, 202, 7, 214,
        85, 164, 200, 35, 139, 82, 143, 99, 85, 158, 127, 106,
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
