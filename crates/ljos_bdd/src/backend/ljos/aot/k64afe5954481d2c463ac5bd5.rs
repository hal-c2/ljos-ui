use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        100, 175, 229, 149, 68, 129, 210, 196, 99, 172, 91, 213, 178, 227, 184, 35, 118, 176, 186,
        144, 104, 63, 74, 94, 239, 187, 95, 87, 37, 22, 29, 123,
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
