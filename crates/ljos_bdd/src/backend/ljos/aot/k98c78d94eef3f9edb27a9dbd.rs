use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        152, 199, 141, 148, 238, 243, 249, 237, 178, 122, 157, 189, 174, 151, 136, 163, 85, 151,
        44, 156, 146, 86, 119, 90, 88, 179, 157, 158, 122, 183, 255, 171,
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
