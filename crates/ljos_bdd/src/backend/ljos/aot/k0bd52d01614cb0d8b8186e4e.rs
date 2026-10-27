use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        11, 213, 45, 1, 97, 76, 176, 216, 184, 24, 110, 78, 92, 185, 178, 116, 237, 131, 181, 225,
        196, 122, 140, 224, 40, 117, 77, 192, 189, 40, 91, 170,
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
    signals: &["ping"],
    methods: &[],
    exprs: &[Some(e0), Some(e1), Some(e2), Some(e3)],
};

// Scenario:6:31
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real((f.num(o0, p0)? / 2.0_f64)));
}

// Scenario:7:34
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::String(rt::js::concat(
        &(rt::Atom::from("n=")),
        &rt::js::number_text(f.num(o0, p0)?),
    )));
}

// Scenario:10:19
fn e2(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o0, 2)?;
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

// Scenario:11:27
fn e3(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o0, 2)?;
    let p2 = f.prop(o0, 0)?;
    let s0 = f.signal(o0, 0)?;
    let _ = {
        let l1 = rt::js::concat(&(f.str(o0, p1)?), &(rt::Atom::from("c")));
        f.set(o0, p0, rt::Value::String(l1.clone()))?;
        l1
    };
    let _ = {
        let l2 = rt::Value::Real(f.num(o0, p2)?);
        f.emit(o0, s0, &[l2])?
    };
    let _ = {
        let l3 = rt::js::concat(&(f.str(o0, p1)?), &(rt::Atom::from("e")));
        f.set(o0, p0, rt::Value::String(l3.clone()))?;
        l3
    };
    Ok(rt::Value::Undefined)
}
