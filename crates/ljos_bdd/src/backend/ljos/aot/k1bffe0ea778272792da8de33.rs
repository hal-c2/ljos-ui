use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        27, 255, 224, 234, 119, 130, 114, 121, 45, 168, 222, 51, 82, 1, 70, 120, 232, 178, 66, 34,
        33, 153, 249, 198, 230, 103, 235, 128, 40, 37, 88, 103,
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
