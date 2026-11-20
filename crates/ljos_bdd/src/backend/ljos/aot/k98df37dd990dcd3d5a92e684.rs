use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        152, 223, 55, 221, 153, 13, 205, 61, 90, 146, 230, 132, 151, 45, 163, 82, 41, 229, 199,
        209, 27, 173, 56, 92, 58, 174, 122, 3, 52, 100, 131, 188,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "shown",
        kind: rt::Kind::Bool,
        write: false,
    }],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0)],
};

// Scenario:8:21
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Bool(f.bool(o0, p0)?));
}
