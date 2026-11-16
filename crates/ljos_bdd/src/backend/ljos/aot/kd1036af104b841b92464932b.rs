use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [209, 3, 106, 241, 4, 184, 65, 185, 36, 100, 147, 43, 176, 188, 41, 100, 92, 163, 221, 64, 69, 82, 118, 249, 168, 48, 179, 115, 176, 77, 225, 135],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite { name: "count", kind: rt::Kind::Number, write: false },
    ],
    signals: &[],
    methods: &[
    ],
    exprs: &[
        Some(e0),
    ],
};

// Scenario:5:31
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
let o0 = f.object(0, 0)?;
let p0 = f.prop(o0, 0)?;
let () = ();
return Ok(rt::Value::Real((f.num(o0, p0)? / 2.0_f64)));
}
