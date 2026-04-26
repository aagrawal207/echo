mod orp;
mod rsvp;
mod source;

use std::process::ExitCode;

const DEFAULT_WPM: u32 = 300;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut wpm = DEFAULT_WPM;
    let mut path: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" | "--wpm" => {
                let Some(raw) = args.get(i + 1) else {
                    eprintln!("error: --wpm requires a value");
                    return ExitCode::from(2);
                };
                let Ok(parsed) = raw.parse::<u32>() else {
                    eprintln!("error: --wpm must be a positive integer");
                    return ExitCode::from(2);
                };
                if parsed == 0 {
                    eprintln!("error: --wpm must be greater than 0");
                    return ExitCode::from(2);
                }
                wpm = parsed;
                i += 2;
            }
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other if path.is_none() => {
                path = Some(other.to_string());
                i += 1;
            }
            other => {
                eprintln!("error: unexpected argument: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let text = match source::load(path.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = rsvp::play(&text, wpm) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn print_help() {
    println!(
        "ech — terminal RSVP reader (echo project)

USAGE:
    ech [OPTIONS] [FILE]

ARGS:
    <FILE>    Path to a .md/.markdown or plain text file. If omitted, reads from stdin.

OPTIONS:
    -w, --wpm <N>    Playback speed in words per minute [default: {DEFAULT_WPM}]
    -h, --help       Show this help"
    );
}
