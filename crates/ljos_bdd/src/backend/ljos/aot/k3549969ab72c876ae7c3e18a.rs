use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        53, 73, 150, 154, 183, 44, 135, 106, 231, 195, 225, 138, 31, 70, 207, 241, 41, 241, 63,
        201, 205, 202, 255, 148, 228, 51, 191, 58, 20, 114, 20, 21,
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
    singletons: &[],
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
