use std::process;
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match rtrace::run(&args) {
        Ok(code) => process::exit(code), 
        Err(err) => {
            eprintln!("rtrace: {err}");
            process::exit(1);
        }
    }
}
