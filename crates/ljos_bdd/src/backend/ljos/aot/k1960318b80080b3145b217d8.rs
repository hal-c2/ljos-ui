use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "NativePanel",
    content: [
        25, 96, 49, 139, 128, 8, 11, 49, 69, 178, 23, 216, 188, 1, 227, 255, 91, 229, 37, 171, 115,
        49, 25, 13, 70, 5, 156, 188, 239, 202, 8, 222,
    ],
    context: "",
    texts: &[],
    props: &[
        rt::PropSite {
            name: "writes",
            kind: rt::Kind::Number,
            write: false,
        },
        rt::PropSite {
            name: "log",
            kind: rt::Kind::String,
            write: false,
        },
        rt::PropSite {
            name: "log",
            kind: rt::Kind::String,
            write: true,
        },
        rt::PropSite {
            name: "seen",
            kind: rt::Kind::String,
            write: false,
        },
    ],
    signals: &[],
    methods: &[],
    exprs: &[
        None, // type: var property source
        None, // object as a value: later
        None, // type: var property held
        Some(e3),
        None, // type: var property held
        Some(e5),
    ],
};

// NativePanel:15:32
fn e3(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 1)?;
    let p0 = f.prop(o0, 0)?;
    return Ok(rt::Value::Bool((f.num(o0, p0)? > 0.0_f64)));
}

// NativePanel:19:32
fn e5(f: &rt::Frame<'_>) -> Result<rt::Value, rt::Stop> {
    let o0 = f.object(0, 0)?;
    let p0 = f.prop(o0, 1)?;
    let p1 = f.prop(o0, 2)?;
    let p2 = f.prop(o0, 3)?;
    let _ = {
        let l1 = rt::js::concat(
            &(f.str(o0, p0)?),
            &(rt::js::concat(&(rt::Atom::from("completed")), &(f.str(o0, p2)?))),
        );
        f.set(o0, p1, rt::Value::String(l1.clone()))?;
        l1
    };
    return Ok(rt::Value::Undefined);
}
