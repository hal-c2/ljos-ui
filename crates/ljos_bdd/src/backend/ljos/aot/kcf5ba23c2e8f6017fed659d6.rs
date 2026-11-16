use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        207, 91, 162, 60, 46, 143, 96, 23, 254, 214, 89, 214, 157, 194, 105, 12, 215, 44, 37, 183,
        114, 17, 91, 160, 87, 22, 54, 33, 91, 58, 33, 156,
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
