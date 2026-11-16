use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [164, 15, 164, 183, 71, 7, 184, 192, 174, 142, 200, 217, 141, 125, 39, 217, 101, 121, 8, 239, 202, 15, 167, 55, 87, 196, 76, 98, 96, 173, 183, 54],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite { name: "size", kind: rt::Kind::Number, write: false },
    ],
    signals: &[],
    methods: &[
    ],
    exprs: &[
        None, // type: var property items
        Some(e1),
        None, // array
    ],
};

// Scenario:7:31
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
let o0 = f.object(0, 0)?;
let p0 = f.prop(o0, 0)?;
return Ok(rt::Value::Real((f.num(o0, p0)? * 2.0_f64)));
}
