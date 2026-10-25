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
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--policy" => {
                let Some(path) = args.next() else {
                    return Err(usage());
                };
                policy = nacre::ExecutionPolicy::from_file(Path::new(&path))
                    .map_err(|error| format_error(error, diagnostic_format))?;
            }
            "--diagnostic-format" => {
                let Some(format) = args.next() else {
                    return Err(usage());
                };
                diagnostic_format = DiagnosticFormat::parse(&format)?;
            }
            "--write-lock" => {
                write_lock = true;
            }
            arg if arg.starts_with("--") => return Err(usage()),
            _ => positional.push(arg),
        }
    }

    if write_lock {
        let Some(input) = positional.first() else {
            return Err(usage());
        };
        nacre::write_lockfile_for(Path::new(input))
            .map_err(|error| format_error(error, diagnostic_format))?;
    }

    let (input, output) = match positional.as_slice() {
        [input] => (input, None),
        [input, output] => (input, Some(output)),
        _ => return Err(usage()),
    };
    let compiled = nacre::compile_file_with_policy(Path::new(input), &policy)
        .map_err(|error| format_error(error, diagnostic_format))?;
    if let Some(output) = output {
        fs::write(output, compiled).map_err(|error| format!("failed to write {output}: {error}"))
    } else {
        print!("{compiled}");
        Ok(())
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
