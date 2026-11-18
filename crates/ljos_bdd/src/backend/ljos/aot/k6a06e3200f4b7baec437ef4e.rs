use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        106, 6, 227, 32, 15, 75, 123, 174, 196, 55, 239, 78, 224, 251, 79, 98, 92, 159, 25, 136,
        100, 62, 194, 70, 53, 221, 5, 21, 199, 14, 99, 44,
    ],
    context: "",
    texts: &[],
    props: &[rt::PropSite {
        name: "count",
        kind: rt::Kind::Number,
        write: false,
    }],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0)],
};

// Scenario:5:31
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let () = ();
    return Ok(rt::Value::Real((f.num(o0, p0)? / 2.0_f64)));
}
