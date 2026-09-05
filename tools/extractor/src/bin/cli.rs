//! Native harness: same code path as the wasm module, so the extraction can be
//! diffed against the Python reference without a wasm runtime in the loop.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <base-rom.sfc> <out.dat> [language-rom.sfc ...] [--no-include-rom]", args[0]);
        std::process::exit(2);
    }
    let mut flags = 0;
    let mut inputs = vec![std::fs::read(&args[1]).expect("read rom")];
    for a in &args[3..] {
        match a.as_str() {
            "--no-include-rom" => flags |= zelda3_restool::FLAG_NO_INCLUDE_ROM,
            other if other.starts_with("--") => {
                eprintln!("unknown flag {other}");
                std::process::exit(2);
            }
            other => inputs.push(std::fs::read(other).expect("read language rom")),
        }
    }
    match zelda3_restool::run_extraction(inputs, flags) {
        Ok(e) => {
            for w in &e.warnings {
                eprintln!("{w}");
            }
            std::fs::write(&args[2], &e.data).expect("write output");
            eprintln!("wrote {} ({} bytes)", args[2], e.data.len());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
