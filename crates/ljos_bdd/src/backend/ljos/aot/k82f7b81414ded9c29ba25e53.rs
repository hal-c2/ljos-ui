use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        130, 247, 184, 20, 20, 222, 217, 194, 155, 162, 94, 83, 249, 186, 41, 140, 83, 41, 70, 58,
        135, 14, 78, 131, 6, 151, 162, 157, 7, 150, 90, 47,
    ],
    context: "",
    texts: &[],
    props: &[
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
        rt::PropSite {
            name: "r",
            kind: rt::Kind::Number,
            write: false,
        },
    ],
    signals: &["ping"],
    methods: &[],
    exprs: &[Some(e0), Some(e1)],
};

// Scenario:8:19
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    let p1 = f.prop(o0, 1)?;
    let mut a0 = f.arg_num(0)?;
    let _ = {
        let l1 = rt::js::concat(
            &(rt::js::concat(&(f.str(o0, p1)?), &(rt::Atom::from("p")))),
            &rt::js::number_text(a0),
        );
        f.set(o0, p0, rt::Value::String(l1.clone()))?;
        l1
    };
    Ok(rt::Value::Undefined)
}

// Scenario:9:23
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 2)?;
    let s0 = f.signal(o0, 0)?;
    let _ = {
        let l1 = rt::Value::Real(f.num(o0, p0)?);
        f.emit(o0, s0, &[l1])?
    };
    return Ok(rt::Value::Undefined);
}
