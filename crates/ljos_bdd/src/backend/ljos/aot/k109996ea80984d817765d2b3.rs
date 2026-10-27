use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        16, 153, 150, 234, 128, 152, 77, 129, 119, 101, 210, 179, 225, 10, 127, 239, 162, 36, 155,
        162, 137, 246, 7, 102, 28, 240, 126, 100, 180, 81, 58, 194,
    ],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite {
            name: "count",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "order",
            kind: rt::Kind::String,
            write: true,
        },
        rt::PropSite {
            name: "order",
            kind: rt::Kind::String,
            write: false,
        },
    ],
    signals: &[],
    singletons: &[],
    methods: &[],
    exprs: &[Some(e0), Some(e1)],
};

// Scenario:5:31
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real((f.num(o0, p0)? / 2.0_f64)));
}

// Scenario:7:27
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o0, 2)?;
    let p2 = f.prop(o0, 0)?;
    let _ = {
        let l1 = rt::js::concat(
            &(rt::js::concat(&(f.str(o0, p1)?), &(rt::Atom::from("a")))),
            &rt::js::number_text(f.num(o0, p2)?),
        );
        f.set(o0, p0, rt::Value::String(l1.clone()))?;
        l1
    };
    return Ok(rt::Value::Undefined);
}
