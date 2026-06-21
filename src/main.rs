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
    let mut policy = nacre::ExecutionPolicy::deny_all();
    let mut diagnostic_format = DiagnosticFormat::Text;
    let mut write_lock = false;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--policy" => {
                let Some(path) = args.get(index + 1) else {
                    return Err(usage());
                };
                policy = nacre::ExecutionPolicy::from_file(Path::new(path))
                    .map_err(|error| format_error(error, diagnostic_format))?;
                index += 2;
            }
            "--diagnostic-format" => {
                let Some(format) = args.get(index + 1) else {
                    return Err(usage());
                };
                diagnostic_format = DiagnosticFormat::parse(format)?;
                index += 2;
            }
            "--write-lock" => {
                write_lock = true;
                index += 1;
            }
            arg if arg.starts_with("--") => return Err(usage()),
            _ => {
                positional.push(args[index].clone());
                index += 1;
            }
        }
    }

    if write_lock {
        let Some(input) = positional.first() else {
            return Err(usage());
        };
        nacre::write_lockfile_for(Path::new(input))
            .map_err(|error| format_error(error, diagnostic_format))?;
    }

    match positional.as_slice() {
        [input] => {
            let output = nacre::compile_file_with_policy(Path::new(input), &policy)
                .map_err(|error| format_error(error, diagnostic_format))?;
            print!("{output}");
            Ok(())
        }
        [input, output] => {
            let compiled = nacre::compile_file_with_policy(Path::new(input), &policy)
                .map_err(|error| format_error(error, diagnostic_format))?;
            fs::write(output, compiled)
                .map_err(|error| format!("failed to write {output}: {error}"))
        }
        _ => Err(usage()),
    }
}

#[derive(Clone, Copy)]
enum DiagnosticFormat {
    Text,
    Json,
}

impl DiagnosticFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err("diagnostic format must be `text` or `json`".to_string()),
        }
    }
}

fn format_error(error: nacre::CompileError, format: DiagnosticFormat) -> String {
    match format {
        DiagnosticFormat::Text => error.to_string(),
        DiagnosticFormat::Json => error.to_json(),
    }
}

fn usage() -> String {
    "usage: nacre [--policy policy.toml] [--diagnostic-format text|json] [--write-lock] <input.ncr> [output.sh]".to_string()
}
