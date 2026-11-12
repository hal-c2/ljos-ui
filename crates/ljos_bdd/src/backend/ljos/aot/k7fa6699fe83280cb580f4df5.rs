use ljos_engine::aot as rt;

pub static FILE: rt::File = rt::File {
    name: "Scenario",
    content: [
        127, 166, 105, 159, 232, 50, 128, 203, 88, 15, 77, 245, 80, 74, 18, 213, 73, 143, 75, 190,
        176, 129, 215, 163, 89, 8, 192, 237, 201, 126, 64, 72,
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
