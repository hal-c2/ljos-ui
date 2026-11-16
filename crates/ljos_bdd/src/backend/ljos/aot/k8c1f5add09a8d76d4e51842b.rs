use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        140, 31, 90, 221, 9, 168, 215, 109, 78, 81, 132, 43, 72, 100, 128, 27, 206, 85, 132, 25,
        255, 241, 12, 126, 6, 37, 239, 141, 144, 18, 175, 151,
    ],
    context: "",
    texts: &[],
    props: &[],
    signals: &[],
    methods: &[],
    exprs: &[
        None, // type: QtObject property first
        None, // object literal
        None, // type: QtObject property shown
    ],
};
