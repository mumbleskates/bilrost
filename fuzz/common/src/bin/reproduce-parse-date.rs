use std::str::from_utf8;

fn main() {
    let mut args = std::env::args();
    let program_name = args.next().unwrap();

    let mut ran = false;
    for filename in args {
        ran = true;
        let data =
            std::fs::read(&filename).unwrap_or_else(|_| panic!("Could not open file {filename:?}"));
        println!("file: {filename:?}");
        let s = from_utf8(&data);
        println!("string: {s:?}");
        common::test_parse_date(&data);
    }
    if !ran {
        println!("Usage: {program_name} <path-to-input> [...]");
        std::process::exit(1);
    }
}
