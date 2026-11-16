use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Combo",
    content: [251, 60, 236, 236, 84, 61, 91, 202, 151, 213, 29, 96, 207, 153, 51, 168, 129, 1, 80, 50, 236, 232, 76, 88, 50, 255, 79, 126, 98, 246, 20, 134],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite { name: "width", kind: rt::Kind::Number, write: false },
    ],
    signals: &[],
    methods: &[
    ],
    exprs: &[
        Some(e0),
    ],
};

// Combo:6:54
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
let o0 = f.object(0, 0)?;
let p0 = f.prop(o0, 0)?;
return Ok(rt::Value::Real(rt::js::max(f.num(o0, p0)?, 160.0_f64)));
}
