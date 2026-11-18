use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        67, 93, 231, 109, 60, 2, 214, 180, 163, 30, 182, 146, 162, 96, 47, 160, 80, 167, 74, 139,
        1, 85, 33, 223, 175, 51, 200, 136, 3, 167, 74, 135,
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
        rt::PropSite {
            name: "doubled",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "count",
            kind: rt::Kind::Number,
            write: true,
        },
    ],
    signals: &[],
    methods: &[],
    exprs: &[Some(e0), Some(e1), Some(e2)],
};

// Scenario:6:33
fn e0(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Real((f.num(o0, p0)? * 2.0_f64)));
}

// Scenario:8:29
fn e1(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o0, 2)?;
    let p2 = f.prop(o0, 3)?;
    let _ = {
        let l1 = rt::js::concat(
            &(rt::js::concat(&(f.str(o0, p1)?), &(rt::Atom::from("d")))),
            &rt::js::number_text(f.num(o0, p2)?),
        );
        f.set(o0, p0, rt::Value::String(l1.clone()))?;
        l1
    };
    return Ok(rt::Value::Undefined);
}

// Scenario:9:34
fn e2(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o0, 2)?;
    let p2 = f.prop(o0, 4)?;
    let _ = {
        let l1 = rt::js::concat(&(f.str(o0, p1)?), &(rt::Atom::from("c")));
        f.set(o0, p0, rt::Value::String(l1.clone()))?;
        l1
    };
    let _ = {
        let l2 = 5.0_f64;
        f.set(o0, p2, rt::Value::Real(l2))?;
        l2
    };
    Ok(rt::Value::Undefined)
}
