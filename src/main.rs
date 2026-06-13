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
    let (policy, positional) = match args.as_slice() {
        [flag, policy, rest @ ..] if flag == "--policy" => {
            let policy = nacre::ExecutionPolicy::from_file(Path::new(policy))
                .map_err(|error| error.to_string())?;
            (policy, rest)
        }
        _ => (nacre::ExecutionPolicy::deny_all(), args.as_slice()),
    };
    match positional {
        [input] => {
            let output = nacre::compile_file_with_policy(Path::new(input), &policy)
                .map_err(|error| error.to_string())?;
            print!("{output}");
            Ok(())
        }
        [input, output] => {
            let compiled = nacre::compile_file_with_policy(Path::new(input), &policy)
                .map_err(|error| error.to_string())?;
            fs::write(output, compiled)
                .map_err(|error| format!("failed to write {output}: {error}"))
        }
        _ => Err("usage: nacre [--policy policy.toml] <input.ncr> [output.sh]".to_string()),
    }
}
