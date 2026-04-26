mod config;
mod orp;
mod rsvp;
mod source;

use std::process::ExitCode;

fn main() -> ExitCode {
    let cfg = config::load();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut wpm = cfg.wpm;
    let start_paused = cfg.start_paused;
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
                print_help(cfg.wpm);
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

    if let Err(e) = rsvp::play(&text, wpm, start_paused) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn print_help(default_wpm: u32) {
    println!(
        "ech — terminal RSVP reader (echo project)

USAGE:
    ech [OPTIONS] [FILE]

ARGS:
    <FILE>    Path to a .md/.markdown or plain text file. If omitted, reads from stdin.

OPTIONS:
    -w, --wpm <N>    Playback speed in words per minute [default: {default_wpm}]
    -h, --help       Show this help

CONFIG:
    Loaded from $ECHO_CONFIG, else $XDG_CONFIG_HOME/echo/config.toml,
    else ~/.config/echo/config.toml. Keys:

        wpm            integer, default 300
        start_paused   bool,    default true

    CLI flags override config values."
    );
}
