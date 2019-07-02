#![feature(never_type)]

macro_rules! lsb {
    ($n:expr) => {
        (1 << $n) - 1
    };
}

mod coder;
mod objs;
