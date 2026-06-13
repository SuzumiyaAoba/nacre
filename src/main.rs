use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.as_slice() {
        [input] => {
            let output =
                nacre::compile_file(Path::new(input)).map_err(|error| error.to_string())?;
            print!("{output}");
            Ok(())
        }
        [input, output] => {
            let compiled =
                nacre::compile_file(Path::new(input)).map_err(|error| error.to_string())?;
            fs::write(output, compiled)
                .map_err(|error| format!("failed to write {output}: {error}"))
        }
        _ => Err("usage: nacre <input.ncr> [output.sh]".to_string()),
    }
}
