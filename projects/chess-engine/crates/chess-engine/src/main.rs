use chess_core::parse_fen;
use chess_search::{DefaultSearcher, SearchLimits, Searcher, StopToken, run_tactical_suite};
use std::env;
use std::fs;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const UCI_SMOKE_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("uci") => run_uci_command(),
        Some("perft") => run_perft_command(&args[1..]),
        Some("perft-suite") => run_perft_suite_command(&args[1..]),
        Some("bench") => run_bench_command(&args[1..]),
        Some("tactics") => run_tactics_command(&args[1..]),
        Some("uci-smoke") => run_uci_smoke_command(&args[1..]),
        Some("baseline-tournament") => run_baseline_tournament_command(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command '{other}'");
            print_help();
            ExitCode::from(2)
        }
        None => {
            println!("{}", chess_engine::version_line());
            ExitCode::SUCCESS
        }
    }
}

fn run_uci_command() -> ExitCode {
    match chess_uci::run_uci_loop(
        BufReader::new(io::stdin()),
        io::stdout(),
        DefaultSearcher::default(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run_perft_command(args: &[String]) -> ExitCode {
    let mut fen = None;
    let mut depth = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fen" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--fen requires a value");
                    return ExitCode::from(2);
                };
                fen = Some(value.clone());
                index += 2;
            }
            "--depth" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--depth requires a value");
                    return ExitCode::from(2);
                };
                match value.parse::<u8>() {
                    Ok(value) => depth = Some(value),
                    Err(_) => {
                        eprintln!("invalid --depth value '{value}'");
                        return ExitCode::from(2);
                    }
                }
                index += 2;
            }
            other => {
                eprintln!("unexpected argument '{other}'");
                return ExitCode::from(2);
            }
        }
    }

    let Some(fen) = fen else {
        eprintln!("usage: chess-engine perft --fen <fen> --depth N");
        return ExitCode::from(2);
    };
    let Some(depth) = depth else {
        eprintln!("usage: chess-engine perft --fen <fen> --depth N");
        return ExitCode::from(2);
    };

    let mut position = match parse_fen(&fen) {
        Ok(position) => position,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let nodes = chess_perft::perft(&mut position, depth);
    println!("nodes {nodes}");
    ExitCode::SUCCESS
}

fn run_perft_suite_command(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: chess-engine perft-suite <path> [--max-depth N]");
        return ExitCode::from(2);
    }

    let path = PathBuf::from(&args[0]);
    let mut max_depth = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--max-depth" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--max-depth requires a value");
                    return ExitCode::from(2);
                };
                match value.parse::<u8>() {
                    Ok(depth) => max_depth = Some(depth),
                    Err(_) => {
                        eprintln!("invalid --max-depth value '{value}'");
                        return ExitCode::from(2);
                    }
                }
                index += 2;
            }
            other => {
                eprintln!("unexpected argument '{other}'");
                return ExitCode::from(2);
            }
        }
    }

    match chess_perft::run_perft_suite(&path, max_depth) {
        Ok(report) => {
            for case in &report.cases {
                println!(
                    "{} depth {}: {}",
                    case.id,
                    case.depth,
                    if case.passed() { "ok" } else { "failed" }
                );
            }
            println!(
                "perft suite passed: {} cases, {} failures",
                report.passed(),
                report.failed()
            );
            ExitCode::SUCCESS
        }
        Err(chess_perft::PerftError::SuiteMismatch(report)) => {
            for case in &report.cases {
                if case.passed() {
                    println!("{} depth {}: ok", case.id, case.depth);
                } else {
                    println!(
                        "{} depth {}: expected {}, got {}",
                        case.id, case.depth, case.expected, case.actual
                    );
                }
            }
            eprintln!(
                "perft suite failed: {} cases, {} failures",
                report.passed(),
                report.failed()
            );
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run_bench_command(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: chess-engine bench");
        return ExitCode::from(2);
    }

    let mut searcher = DefaultSearcher::default();
    searcher.set_position(chess_core::startpos(), Vec::new());
    let started = Instant::now();
    let result = searcher.search(SearchLimits::fixed_depth(4), StopToken::new());
    let elapsed = started.elapsed();
    let latest = result.info.last();
    let nodes = latest.map_or(0, |info| info.nodes);
    let nps = if elapsed.as_millis() == 0 {
        0
    } else {
        ((u128::from(nodes) * 1_000) / elapsed.as_millis()) as u64
    };
    let bestmove = result
        .best_move
        .map_or_else(|| "0000".to_owned(), |mv| mv.to_string());

    println!("bench depth 4 bestmove {bestmove}");
    println!("bench nodes {nodes}");
    println!("bench time_ms {}", elapsed.as_millis());
    println!("bench nps {nps}");
    ExitCode::SUCCESS
}

fn run_tactics_command(args: &[String]) -> ExitCode {
    let mut paths = Vec::new();
    let mut movetime = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--movetime" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--movetime requires a value");
                    return ExitCode::from(2);
                };
                match value.parse::<u64>() {
                    Ok(value) => movetime = Some(Duration::from_millis(value)),
                    Err(_) => {
                        eprintln!("invalid --movetime value '{value}'");
                        return ExitCode::from(2);
                    }
                }
                index += 2;
            }
            option if option.starts_with('-') => {
                eprintln!("unexpected argument '{option}'");
                return ExitCode::from(2);
            }
            path => {
                paths.push(PathBuf::from(path));
                index += 1;
            }
        }
    }

    if paths.is_empty() || movetime.is_none() {
        eprintln!("usage: chess-engine tactics <epd...> --movetime MS");
        return ExitCode::from(2);
    }

    let limits = SearchLimits::fixed_movetime(movetime.expect("checked above"));
    let mut total = 0usize;
    let mut failed = 0usize;
    for path in paths {
        match run_tactical_suite(&path, limits.clone()) {
            Ok(report) => {
                for case in &report.cases {
                    let id = case.id.as_deref().unwrap_or("unnamed");
                    let returned = case
                        .best_move
                        .map_or_else(|| "none".to_owned(), |best_move| best_move.to_string());
                    println!(
                        "{} line {}: {} returned {} depth {} score {} nodes {}",
                        id,
                        case.line,
                        if case.passed() { "ok" } else { "failed" },
                        returned,
                        case.depth,
                        case.score.0,
                        case.nodes
                    );
                }
                if report.failed() > 0 {
                    eprint!("{}", report.failure_summary());
                }
                total += report.cases.len();
                failed += report.failed();
            }
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::FAILURE;
            }
        }
    }

    println!(
        "tactics passed: {} cases, {} failures",
        total.saturating_sub(failed),
        failed
    );
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_uci_smoke_command(args: &[String]) -> ExitCode {
    if args.len() != 1 {
        eprintln!("usage: chess-engine uci-smoke <transcript>");
        return ExitCode::from(2);
    }

    match run_uci_smoke(Path::new(&args[0])) {
        Ok(report) => {
            println!(
                "uci smoke passed: {} commands, {} expected tokens",
                report.commands, report.expected_tokens
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run_baseline_tournament_command(args: &[String]) -> ExitCode {
    let mut config = chess_engine::BaselineTournamentConfig::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--games" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--games requires a value");
                    return ExitCode::from(2);
                };
                match value.parse::<usize>() {
                    Ok(value) => config.games = value,
                    Err(_) => {
                        eprintln!("invalid --games value '{value}'");
                        return ExitCode::from(2);
                    }
                }
                index += 2;
            }
            "--max-plies" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--max-plies requires a value");
                    return ExitCode::from(2);
                };
                match value.parse::<usize>() {
                    Ok(value) => config.max_plies = value,
                    Err(_) => {
                        eprintln!("invalid --max-plies value '{value}'");
                        return ExitCode::from(2);
                    }
                }
                index += 2;
            }
            "--engine-depth" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--engine-depth requires a value");
                    return ExitCode::from(2);
                };
                match value.parse::<u8>() {
                    Ok(value) => config.engine_depth = value,
                    Err(_) => {
                        eprintln!("invalid --engine-depth value '{value}'");
                        return ExitCode::from(2);
                    }
                }
                index += 2;
            }
            "--out-dir" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--out-dir requires a value");
                    return ExitCode::from(2);
                };
                config.output_dir = PathBuf::from(value);
                index += 2;
            }
            other => {
                eprintln!("unexpected argument '{other}'");
                return ExitCode::from(2);
            }
        }
    }

    match chess_engine::run_baseline_tournament(config) {
        Ok(report) => {
            print!("{}", report.summary_text());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UciSmokeReport {
    commands: usize,
    expected_tokens: usize,
}

#[derive(Debug)]
enum UciSmokeError {
    Io(io::Error),
    Parse {
        line: usize,
        message: String,
    },
    Spawn(io::Error),
    Timeout(Duration),
    Exit(i32),
    Stderr(String),
    MissingToken {
        token: String,
        after_byte: usize,
        stdout: String,
    },
}

impl std::fmt::Display for UciSmokeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read UCI smoke transcript: {err}"),
            Self::Parse { line, message } => {
                write!(f, "invalid UCI smoke transcript line {line}: {message}")
            }
            Self::Spawn(err) => write!(f, "failed to spawn UCI engine: {err}"),
            Self::Timeout(timeout) => write!(
                f,
                "UCI smoke engine did not exit within {} ms",
                timeout.as_millis()
            ),
            Self::Exit(code) => write!(f, "UCI smoke engine exited with status {code}"),
            Self::Stderr(stderr) => write!(f, "UCI smoke produced stderr: {stderr}"),
            Self::MissingToken {
                token,
                after_byte,
                stdout,
            } => write!(
                f,
                "UCI smoke missing token '{token}' after byte {after_byte}; stdout was:\n{stdout}"
            ),
        }
    }
}

impl std::error::Error for UciSmokeError {}

impl From<io::Error> for UciSmokeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

fn run_uci_smoke(path: &Path) -> Result<UciSmokeReport, UciSmokeError> {
    let (commands, expected) = parse_uci_smoke(path)?;
    let exe = env::current_exe().map_err(UciSmokeError::Spawn)?;
    let mut child = Command::new(exe)
        .arg("uci")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(UciSmokeError::Spawn)?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            UciSmokeError::Spawn(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "child stdin unavailable",
            ))
        })?;
        for command in &commands {
            writeln!(stdin, "{command}")?;
        }
    }
    drop(child.stdin.take());

    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() >= UCI_SMOKE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UciSmokeError::Timeout(UCI_SMOKE_TIMEOUT));
        }
        thread::sleep(Duration::from_millis(10));
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(UciSmokeError::Exit(output.status.code().unwrap_or(-1)));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !stderr.is_empty() {
        return Err(UciSmokeError::Stderr(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    validate_expected_tokens(&stdout, &expected)?;
    Ok(UciSmokeReport {
        commands: commands.len(),
        expected_tokens: expected.len(),
    })
}

fn parse_uci_smoke(path: &Path) -> Result<(Vec<String>, Vec<String>), UciSmokeError> {
    let content = fs::read_to_string(path)?;
    let mut commands = Vec::new();
    let mut expected = Vec::new();
    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(command) = line.strip_prefix('>') {
            commands.push(command.trim().to_owned());
        } else if let Some(token) = line.strip_prefix('<') {
            expected.push(token.trim().to_owned());
        } else {
            return Err(UciSmokeError::Parse {
                line: line_number,
                message: "expected line to start with '>' or '<'".to_owned(),
            });
        }
    }
    Ok((commands, expected))
}

fn validate_expected_tokens(stdout: &str, expected: &[String]) -> Result<(), UciSmokeError> {
    let mut cursor = 0;
    for token in expected {
        let Some(offset) = stdout[cursor..].find(token) else {
            return Err(UciSmokeError::MissingToken {
                token: token.clone(),
                after_byte: cursor,
                stdout: stdout.to_owned(),
            });
        };
        cursor += offset + token.len();
    }
    Ok(())
}

fn print_help() {
    println!("{}", chess_engine::version_line());
    println!("usage:");
    println!("  chess-engine uci");
    println!("  chess-engine perft --fen <fen> --depth N");
    println!("  chess-engine perft-suite <path> [--max-depth N]");
    println!("  chess-engine bench");
    println!("  chess-engine tactics <epd...> --movetime MS");
    println!("  chess-engine uci-smoke <transcript>");
    println!(
        "  chess-engine baseline-tournament [--games N] [--max-plies N] [--engine-depth N] [--out-dir DIR]"
    );
}
