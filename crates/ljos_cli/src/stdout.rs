//! Standard output for the commands whose report is read by another
//! program. `print!` panics when that program stopped reading
//! (`ljos check | head -1`); a report nobody reads is no fault, so the
//! command ends quietly, with the status a shell gives a program a closed
//! pipe ended.

use std::io::{ErrorKind, Write as _};

/// 128 and SIGPIPE.
const BROKEN_PIPE: i32 = 128 + 13;

/// Writes `args` to stdout; ends the process when it cannot be written.
pub fn write(args: std::fmt::Arguments<'_>) {
    if let Err(e) = std::io::stdout().lock().write_fmt(args) {
        if e.kind() == ErrorKind::BrokenPipe {
            crate::exit(BROKEN_PIPE);
        }
        crate::stderr::line(&format!("ljos: cannot write to standard output: {e}"));
        crate::exit(1);
    }
}

/// `print!` that ends the process quietly when stdout is a closed pipe.
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::stdout::write(format_args!($($arg)*))
    };
}

/// `println!` that ends the process quietly when stdout is a closed pipe.
macro_rules! outln {
    ($($arg:tt)*) => {
        $crate::stdout::write(format_args!("{}\n", format_args!($($arg)*)))
    };
}

pub(crate) use {out, outln};
