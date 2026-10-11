fn main() {
    let mut refused = 0;
    let mut changed = 0;
    let mut n = 0;
    let show = std::env::var("SHOW").is_ok();
    for path in std::env::args().skip(1) {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        n += 1;
        match qml_fmt::format(&src) {
            Ok(out) => {
                if out != src {
                    changed += 1;
                }
                if show {
                    print!("{out}");
                }
                if std::env::var("DIFF").is_ok() {
                    print!("{}", qml_fmt::unified_diff(&path, &src, &out));
                }
            }
            Err(e) => {
                refused += 1;
                eprintln!("{path}: {e}");
            }
        }
    }
    eprintln!("{n} files, {changed} would change, {refused} refused");
}
