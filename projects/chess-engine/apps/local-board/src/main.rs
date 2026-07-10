use local_board::{SmokeOptions, default_engine_path, launch_gui, run_smoke};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--smoke") => run_smoke_command(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unexpected argument '{other}'");
            print_help();
            ExitCode::from(2)
        }
        None => match launch_gui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("{err}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_smoke_command(args: &[String]) -> ExitCode {
    let mut options = SmokeOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--engine" => {
                let Some(path) = args.get(index + 1) else {
                    eprintln!("--engine requires a path");
                    return ExitCode::from(2);
                };
                options.engine_path = PathBuf::from(path);
                index += 2;
            }
            other => {
                eprintln!("unexpected argument '{other}'");
                return ExitCode::from(2);
            }
        }
    }

    match run_smoke(options) {
        Ok(report) => {
            for check in report.checks {
                println!("ok: {check}");
            }
            println!("gui smoke passed: bestmove {}", report.bestmove);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("{}", local_board::app_name());
    println!("usage:");
    println!("  local-board");
    println!(
        "  local-board --smoke [--engine {}]",
        default_engine_path().display()
    );
}
