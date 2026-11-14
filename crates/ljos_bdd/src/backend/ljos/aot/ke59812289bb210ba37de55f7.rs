use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        229, 152, 18, 40, 155, 178, 16, 186, 55, 222, 85, 247, 243, 26, 183, 122, 196, 227, 54,
        195, 33, 142, 171, 189, 186, 71, 33, 67, 89, 13, 231, 39,
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
