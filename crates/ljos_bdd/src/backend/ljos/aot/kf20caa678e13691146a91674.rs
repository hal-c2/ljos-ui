use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "NativePanel",
    content: [
        242, 12, 170, 103, 142, 19, 105, 17, 70, 169, 22, 116, 228, 62, 237, 160, 76, 99, 180, 131,
        170, 250, 240, 229, 161, 202, 148, 163, 178, 36, 131, 135,
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
