#![doc = "UCI protocol parser and engine loop for the local chess engine."]

use chess_core::{FenError, Move, Position, make_move, parse_fen, startpos};
use chess_search::{SearchInfo, SearchLimits, SearchResult, Searcher, StopToken, move_from_uci};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, Write};
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub const ENGINE_NAME: &str = "Kensho Chess Engine";
pub const ENGINE_AUTHOR: &str = "Kensho chess-engine contributors";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UciCommand {
    Uci,
    Debug(bool),
    IsReady,
    SetOption {
        name: String,
        value: Option<String>,
    },
    UciNewGame,
    Position {
        startpos: bool,
        fen: Option<String>,
        moves: Vec<String>,
    },
    Go(SearchLimits),
    Stop,
    Quit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UciParseError {
    Empty,
    UnknownCommand(String),
    MissingArgument {
        command: String,
        argument: &'static str,
    },
    InvalidArgument {
        command: String,
        argument: &'static str,
        value: String,
    },
    UnexpectedToken {
        command: String,
        token: String,
    },
    InvalidPosition(String),
}

impl fmt::Display for UciParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty UCI command"),
            Self::UnknownCommand(command) => write!(f, "unknown UCI command '{command}'"),
            Self::MissingArgument { command, argument } => {
                write!(f, "{command} is missing {argument}")
            }
            Self::InvalidArgument {
                command,
                argument,
                value,
            } => write!(f, "{command} has invalid {argument} '{value}'"),
            Self::UnexpectedToken { command, token } => {
                write!(f, "{command} has unexpected token '{token}'")
            }
            Self::InvalidPosition(message) => write!(f, "invalid position command: {message}"),
        }
    }
}

impl Error for UciParseError {}

#[derive(Debug)]
pub enum UciError {
    Io(io::Error),
    SearchThreadPanicked,
}

impl fmt::Display for UciError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "UCI I/O failed: {err}"),
            Self::SearchThreadPanicked => f.write_str("search thread panicked"),
        }
    }
}

impl Error for UciError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::SearchThreadPanicked => None,
        }
    }
}

impl From<io::Error> for UciError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UciOptions {
    pub hash_mb: u16,
    pub threads: u16,
    pub move_overhead: Duration,
}

impl Default for UciOptions {
    fn default() -> Self {
        Self {
            hash_mb: 64,
            threads: 1,
            move_overhead: Duration::from_millis(30),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UciOptionError {
    UnknownOption(String),
    MissingValue(String),
    InvalidInteger {
        name: String,
        value: String,
    },
    OutOfRange {
        name: String,
        value: u32,
        min: u32,
        max: u32,
    },
}

impl fmt::Display for UciOptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOption(name) => write!(f, "unknown option '{name}'"),
            Self::MissingValue(name) => write!(f, "option '{name}' requires a value"),
            Self::InvalidInteger { name, value } => {
                write!(f, "option '{name}' has invalid integer value '{value}'")
            }
            Self::OutOfRange {
                name,
                value,
                min,
                max,
            } => write!(
                f,
                "option '{name}' value {value} is outside range {min}..={max}"
            ),
        }
    }
}

impl Error for UciOptionError {}

impl UciOptions {
    pub fn set_option(&mut self, name: &str, value: Option<&str>) -> Result<(), UciOptionError> {
        match normalize_option_name(name).as_str() {
            "hash" => {
                self.hash_mb = parse_spin_option(name, value, 1, 4096)? as u16;
                Ok(())
            }
            "threads" => {
                self.threads = parse_spin_option(name, value, 1, 512)? as u16;
                Ok(())
            }
            "move overhead" => {
                let ms = parse_spin_option(name, value, 0, 60_000)?;
                self.move_overhead = Duration::from_millis(u64::from(ms));
                Ok(())
            }
            _ => Err(UciOptionError::UnknownOption(name.to_owned())),
        }
    }
}

#[derive(Clone, Debug)]
struct UciState {
    options: UciOptions,
    position: Position,
    history: Vec<Move>,
}

pub fn parse_uci_line(line: &str) -> Result<UciCommand, UciParseError> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let Some(command) = tokens.first().copied() else {
        return Err(UciParseError::Empty);
    };

    match command {
        "uci" => parse_no_args(command, &tokens).map(|()| UciCommand::Uci),
        "debug" => parse_debug(&tokens),
        "isready" => parse_no_args(command, &tokens).map(|()| UciCommand::IsReady),
        "setoption" => parse_setoption(&tokens),
        "ucinewgame" => parse_no_args(command, &tokens).map(|()| UciCommand::UciNewGame),
        "position" => parse_position(&tokens),
        "go" => parse_go(&tokens),
        "stop" => parse_no_args(command, &tokens).map(|()| UciCommand::Stop),
        "quit" => parse_no_args(command, &tokens).map(|()| UciCommand::Quit),
        _ => Err(UciParseError::UnknownCommand(command.to_owned())),
    }
}

pub fn run_uci_loop<R, W, S>(input: R, mut output: W, engine: S) -> Result<(), UciError>
where
    R: BufRead + Send + 'static,
    W: Write,
    S: Searcher + Send + 'static,
{
    let (events_tx, events_rx) = mpsc::channel();
    spawn_input_reader(input, events_tx.clone());

    let mut state = UciState {
        options: UciOptions::default(),
        position: startpos(),
        history: Vec::new(),
    };
    let mut engine = Some(engine);
    if let Some(engine) = engine.as_mut() {
        engine.set_position(state.position.clone(), state.history.clone());
    }
    let mut active = None;
    let mut pending_inputs = VecDeque::new();

    loop {
        let event = if let Some(input) = pending_inputs.pop_front() {
            UciEvent::Input(input)
        } else {
            match events_rx.recv() {
                Ok(event) => event,
                Err(_) => {
                    finish_active_search(
                        &mut active,
                        &mut engine,
                        &mut output,
                        &events_rx,
                        &mut pending_inputs,
                        FinishMode::Stop,
                    )?;
                    output.flush()?;
                    return Ok(());
                }
            }
        };

        let input = match event {
            UciEvent::Input(input) => input,
            UciEvent::SearchFinished(outcome) => {
                complete_active_search(&mut active, &mut engine, &mut output, outcome)?;
                continue;
            }
        };

        let line = match input {
            InputEvent::Line(line_result) => line_result?,
            InputEvent::Eof => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Wait,
                )?;
                output.flush()?;
                return Ok(());
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let command = match parse_uci_line(line) {
            Ok(command) => command,
            Err(err) => {
                writeln!(output, "info string ignored command: {err}")?;
                output.flush()?;
                continue;
            }
        };

        match command {
            UciCommand::Uci => {
                write_uci_identity(&mut output)?;
                output.flush()?;
            }
            UciCommand::Debug(_) => {}
            UciCommand::IsReady => {
                writeln!(output, "readyok")?;
                output.flush()?;
            }
            UciCommand::SetOption { name, value } => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Wait,
                )?;
                match state.options.set_option(&name, value.as_deref()) {
                    Ok(()) => {
                        if let Some(engine) = engine.as_mut() {
                            apply_engine_option(engine, &name, &state.options);
                        }
                    }
                    Err(err) => {
                        writeln!(output, "info string ignored setoption: {err}")?;
                        output.flush()?;
                    }
                }
            }
            UciCommand::UciNewGame => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Wait,
                )?;
                state.position = startpos();
                state.history.clear();
                if let Some(engine) = engine.as_mut() {
                    engine.new_game();
                    engine.set_position(state.position.clone(), state.history.clone());
                }
            }
            UciCommand::Position {
                startpos,
                fen,
                moves,
            } => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Wait,
                )?;
                match build_position(startpos, fen.as_deref(), &moves) {
                    Ok((position, history)) => {
                        state.position = position;
                        state.history = history;
                        if let Some(engine) = engine.as_mut() {
                            engine.set_position(state.position.clone(), state.history.clone());
                        }
                    }
                    Err(err) => {
                        writeln!(output, "info string ignored position: {err}")?;
                        output.flush()?;
                    }
                }
            }
            UciCommand::Go(limits) => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Wait,
                )?;
                let limits = apply_move_overhead(limits, state.options.move_overhead);
                if should_search_in_background(&limits) {
                    start_background_search(&mut active, &mut engine, &events_tx, limits);
                } else if let Some(searcher) = engine.as_mut() {
                    let result = searcher.search(limits, StopToken::new());
                    write_search_result(&mut output, &result)?;
                    output.flush()?;
                }
            }
            UciCommand::Stop => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Stop,
                )?;
            }
            UciCommand::Quit => {
                finish_active_search(
                    &mut active,
                    &mut engine,
                    &mut output,
                    &events_rx,
                    &mut pending_inputs,
                    FinishMode::Stop,
                )?;
                output.flush()?;
                return Ok(());
            }
        }
    }
}

fn parse_no_args(command: &str, tokens: &[&str]) -> Result<(), UciParseError> {
    if tokens.len() == 1 {
        Ok(())
    } else {
        Err(UciParseError::UnexpectedToken {
            command: command.to_owned(),
            token: tokens[1].to_owned(),
        })
    }
}

fn parse_debug(tokens: &[&str]) -> Result<UciCommand, UciParseError> {
    match tokens {
        [_, "on"] => Ok(UciCommand::Debug(true)),
        [_, "off"] => Ok(UciCommand::Debug(false)),
        [command] => Err(UciParseError::MissingArgument {
            command: (*command).to_owned(),
            argument: "on/off",
        }),
        [command, value, ..] => Err(UciParseError::InvalidArgument {
            command: (*command).to_owned(),
            argument: "debug flag",
            value: (*value).to_owned(),
        }),
        [] => Err(UciParseError::Empty),
    }
}

fn parse_setoption(tokens: &[&str]) -> Result<UciCommand, UciParseError> {
    if tokens.get(1) != Some(&"name") {
        return Err(UciParseError::MissingArgument {
            command: "setoption".to_owned(),
            argument: "name",
        });
    }

    let mut name_parts = Vec::new();
    let mut index = 2;
    while let Some(token) = tokens.get(index) {
        if *token == "value" {
            break;
        }
        name_parts.push(*token);
        index += 1;
    }

    if name_parts.is_empty() {
        return Err(UciParseError::MissingArgument {
            command: "setoption".to_owned(),
            argument: "option name",
        });
    }

    let value = if tokens.get(index) == Some(&"value") {
        let value_parts = &tokens[index + 1..];
        if value_parts.is_empty() {
            None
        } else {
            Some(value_parts.join(" "))
        }
    } else {
        None
    };

    Ok(UciCommand::SetOption {
        name: name_parts.join(" "),
        value,
    })
}

fn parse_position(tokens: &[&str]) -> Result<UciCommand, UciParseError> {
    let Some(kind) = tokens.get(1).copied() else {
        return Err(UciParseError::MissingArgument {
            command: "position".to_owned(),
            argument: "startpos/fen",
        });
    };

    match kind {
        "startpos" => {
            let mut moves = Vec::new();
            if let Some(token) = tokens.get(2) {
                if *token != "moves" {
                    return Err(UciParseError::UnexpectedToken {
                        command: "position".to_owned(),
                        token: (*token).to_owned(),
                    });
                }
                moves.extend(tokens[3..].iter().map(|token| (*token).to_owned()));
            }
            Ok(UciCommand::Position {
                startpos: true,
                fen: None,
                moves,
            })
        }
        "fen" => {
            if tokens.len() < 8 {
                return Err(UciParseError::InvalidPosition(
                    "fen requires six fields".to_owned(),
                ));
            }
            let fen = tokens[2..8].join(" ");
            let mut moves = Vec::new();
            if let Some(token) = tokens.get(8) {
                if *token != "moves" {
                    return Err(UciParseError::UnexpectedToken {
                        command: "position".to_owned(),
                        token: (*token).to_owned(),
                    });
                }
                moves.extend(tokens[9..].iter().map(|token| (*token).to_owned()));
            }
            Ok(UciCommand::Position {
                startpos: false,
                fen: Some(fen),
                moves,
            })
        }
        token => Err(UciParseError::UnexpectedToken {
            command: "position".to_owned(),
            token: token.to_owned(),
        }),
    }
}

fn parse_go(tokens: &[&str]) -> Result<UciCommand, UciParseError> {
    let mut limits = SearchLimits::default();
    let mut index = 1;
    while let Some(token) = tokens.get(index).copied() {
        match token {
            "depth" => {
                limits.depth = Some(parse_next(tokens, &mut index, "go", "depth")?);
            }
            "nodes" => {
                limits.nodes = Some(parse_next(tokens, &mut index, "go", "nodes")?);
            }
            "movetime" => {
                let millis: u64 = parse_next(tokens, &mut index, "go", "movetime")?;
                limits.movetime = Some(Duration::from_millis(millis));
            }
            "wtime" => {
                let millis: u64 = parse_next(tokens, &mut index, "go", "wtime")?;
                limits.white_time = Some(Duration::from_millis(millis));
            }
            "btime" => {
                let millis: u64 = parse_next(tokens, &mut index, "go", "btime")?;
                limits.black_time = Some(Duration::from_millis(millis));
            }
            "winc" => {
                let millis: u64 = parse_next(tokens, &mut index, "go", "winc")?;
                limits.white_increment = Some(Duration::from_millis(millis));
            }
            "binc" => {
                let millis: u64 = parse_next(tokens, &mut index, "go", "binc")?;
                limits.black_increment = Some(Duration::from_millis(millis));
            }
            "movestogo" => {
                limits.moves_to_go = Some(parse_next(tokens, &mut index, "go", "movestogo")?);
            }
            "infinite" => {
                limits.infinite = true;
                index += 1;
            }
            "ponder" => {
                index += 1;
            }
            token => {
                return Err(UciParseError::UnexpectedToken {
                    command: "go".to_owned(),
                    token: token.to_owned(),
                });
            }
        }
    }
    Ok(UciCommand::Go(limits))
}

fn parse_next<T>(
    tokens: &[&str],
    index: &mut usize,
    command: &'static str,
    argument: &'static str,
) -> Result<T, UciParseError>
where
    T: std::str::FromStr,
{
    let Some(value) = tokens.get(*index + 1) else {
        return Err(UciParseError::MissingArgument {
            command: command.to_owned(),
            argument,
        });
    };
    let parsed = value
        .parse::<T>()
        .map_err(|_| UciParseError::InvalidArgument {
            command: command.to_owned(),
            argument,
            value: (*value).to_owned(),
        })?;
    *index += 2;
    Ok(parsed)
}

fn normalize_option_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn parse_spin_option(
    name: &str,
    value: Option<&str>,
    min: u32,
    max: u32,
) -> Result<u32, UciOptionError> {
    let value = value.ok_or_else(|| UciOptionError::MissingValue(name.to_owned()))?;
    let parsed = value
        .parse::<u32>()
        .map_err(|_| UciOptionError::InvalidInteger {
            name: name.to_owned(),
            value: value.to_owned(),
        })?;
    if !(min..=max).contains(&parsed) {
        return Err(UciOptionError::OutOfRange {
            name: name.to_owned(),
            value: parsed,
            min,
            max,
        });
    }
    Ok(parsed)
}

fn apply_engine_option(engine: &mut impl Searcher, name: &str, options: &UciOptions) {
    match normalize_option_name(name).as_str() {
        "hash" => engine.set_hash_size_mb(usize::from(options.hash_mb)),
        "threads" => engine.set_thread_count(options.threads),
        "move overhead" => {}
        _ => {}
    }
}

fn write_uci_identity(output: &mut impl Write) -> Result<(), UciError> {
    writeln!(output, "id name {ENGINE_NAME}")?;
    writeln!(output, "id author {ENGINE_AUTHOR}")?;
    writeln!(
        output,
        "option name Hash type spin default 64 min 1 max 4096"
    )?;
    writeln!(
        output,
        "option name Threads type spin default 1 min 1 max 512"
    )?;
    writeln!(
        output,
        "option name Move Overhead type spin default 30 min 0 max 60000"
    )?;
    writeln!(output, "uciok")?;
    Ok(())
}

fn apply_move_overhead(mut limits: SearchLimits, overhead: Duration) -> SearchLimits {
    limits.movetime = limits
        .movetime
        .map(|duration| subtract_overhead(duration, overhead));
    limits.white_time = limits
        .white_time
        .map(|duration| subtract_overhead(duration, overhead));
    limits.black_time = limits
        .black_time
        .map(|duration| subtract_overhead(duration, overhead));
    limits
}

fn subtract_overhead(duration: Duration, overhead: Duration) -> Duration {
    duration
        .checked_sub(overhead)
        .unwrap_or_else(|| Duration::from_millis(1))
        .max(Duration::from_millis(1))
}

fn build_position(
    use_startpos: bool,
    fen: Option<&str>,
    moves: &[String],
) -> Result<(Position, Vec<Move>), PositionCommandError> {
    let mut position = if use_startpos {
        startpos()
    } else {
        parse_fen(fen.unwrap_or_default()).map_err(PositionCommandError::Fen)?
    };
    let mut history = Vec::with_capacity(moves.len());

    for move_text in moves {
        let Some(mv) = move_from_uci(&position, move_text) else {
            return Err(PositionCommandError::IllegalMove {
                move_text: move_text.clone(),
            });
        };
        make_move(&mut position, mv).map_err(|_| PositionCommandError::IllegalMove {
            move_text: move_text.clone(),
        })?;
        history.push(mv);
    }

    Ok((position, history))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PositionCommandError {
    Fen(FenError),
    IllegalMove { move_text: String },
}

impl fmt::Display for PositionCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fen(err) => write!(f, "{err}"),
            Self::IllegalMove { move_text } => write!(f, "illegal move '{move_text}'"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinishMode {
    Wait,
    Stop,
}

enum InputEvent {
    Line(io::Result<String>),
    Eof,
}

enum SearchOutcome<S> {
    Finished(S, SearchResult),
    Panicked,
}

enum UciEvent<S> {
    Input(InputEvent),
    SearchFinished(SearchOutcome<S>),
}

struct ActiveSearch {
    stop: StopToken,
    infinite: bool,
}

fn spawn_input_reader<R, S>(input: R, events: mpsc::Sender<UciEvent<S>>)
where
    R: BufRead + Send + 'static,
    S: Send + 'static,
{
    thread::spawn(move || {
        for line_result in input.lines() {
            let should_stop = line_result.is_err();
            if events
                .send(UciEvent::Input(InputEvent::Line(line_result)))
                .is_err()
            {
                return;
            }
            if should_stop {
                return;
            }
        }
        let _ = events.send(UciEvent::Input(InputEvent::Eof));
    });
}

fn should_search_in_background(limits: &SearchLimits) -> bool {
    limits.infinite
        || limits.movetime.is_some()
        || limits.white_time.is_some()
        || limits.black_time.is_some()
        || limits.white_increment.is_some()
        || limits.black_increment.is_some()
}

fn start_background_search<S>(
    active: &mut Option<ActiveSearch>,
    engine: &mut Option<S>,
    events: &mpsc::Sender<UciEvent<S>>,
    limits: SearchLimits,
) where
    S: Searcher + Send + 'static,
{
    let Some(mut searcher) = engine.take() else {
        return;
    };

    let infinite = limits.infinite;
    let stop = StopToken::new();
    let worker_stop = stop.clone();
    let events = events.clone();
    thread::spawn(move || {
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            let result = searcher.search(limits, worker_stop);
            (searcher, result)
        }))
        .map_or(SearchOutcome::Panicked, |(searcher, result)| {
            SearchOutcome::Finished(searcher, result)
        });
        let _ = events.send(UciEvent::SearchFinished(outcome));
    });
    *active = Some(ActiveSearch { stop, infinite });
}

fn finish_active_search<W, S>(
    active: &mut Option<ActiveSearch>,
    engine: &mut Option<S>,
    output: &mut W,
    events: &mpsc::Receiver<UciEvent<S>>,
    pending_inputs: &mut VecDeque<InputEvent>,
    mode: FinishMode,
) -> Result<(), UciError>
where
    W: Write,
{
    let Some(search) = active.as_ref() else {
        return Ok(());
    };

    if mode == FinishMode::Stop || search.infinite {
        search.stop.stop();
    }

    loop {
        match events.recv() {
            Ok(UciEvent::Input(input)) => pending_inputs.push_back(input),
            Ok(UciEvent::SearchFinished(outcome)) => {
                return complete_active_search(active, engine, output, outcome);
            }
            Err(_) => return Err(UciError::SearchThreadPanicked),
        }
    }
}

fn complete_active_search<W, S>(
    active: &mut Option<ActiveSearch>,
    engine: &mut Option<S>,
    output: &mut W,
    outcome: SearchOutcome<S>,
) -> Result<(), UciError>
where
    W: Write,
{
    let Some(_) = active.take() else {
        return Ok(());
    };

    match outcome {
        SearchOutcome::Finished(searcher, result) => {
            *engine = Some(searcher);
            write_search_result(output, &result)?;
            output.flush()?;
            Ok(())
        }
        SearchOutcome::Panicked => Err(UciError::SearchThreadPanicked),
    }
}

fn write_search_result(output: &mut impl Write, result: &SearchResult) -> Result<(), UciError> {
    for info in &result.info {
        write_search_info(output, info)?;
    }

    let bestmove = result
        .best_move
        .map_or_else(|| "0000".to_owned(), |mv| mv.to_string());
    if let Some(ponder) = result.ponder {
        writeln!(output, "bestmove {bestmove} ponder {ponder}")?;
    } else {
        writeln!(output, "bestmove {bestmove}")?;
    }
    Ok(())
}

fn write_search_info(output: &mut impl Write, info: &SearchInfo) -> Result<(), UciError> {
    write!(
        output,
        "info depth {} seldepth {} score cp {} nodes {} nps {}",
        info.depth, info.seldepth, info.score.0, info.nodes, info.nps
    )?;
    if let Some(hashfull) = info.hashfull {
        write!(output, " hashfull {hashfull}")?;
    }
    if !info.pv.is_empty() {
        write!(
            output,
            " pv {}",
            info.pv
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        )?;
    }
    writeln!(output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_core::{MoveKind, Square, to_fen};
    use chess_eval::Score;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[derive(Clone, Default)]
    struct RecordingSearcher {
        positions: Arc<Mutex<Vec<String>>>,
        hash_sizes: Arc<Mutex<Vec<usize>>>,
        thread_counts: Arc<Mutex<Vec<u16>>>,
    }

    impl Searcher for RecordingSearcher {
        fn new_game(&mut self) {}

        fn set_position(&mut self, pos: Position, history: Vec<Move>) {
            self.positions
                .lock()
                .expect("positions mutex should not be poisoned")
                .push(format!("{} moves={}", to_fen(&pos), history.len()));
        }

        fn search(&mut self, limits: SearchLimits, stop: StopToken) -> SearchResult {
            let waits_for_stop = limits.infinite
                || limits.movetime.is_some()
                || limits.white_time.is_some()
                || limits.black_time.is_some()
                || limits.white_increment.is_some()
                || limits.black_increment.is_some();
            while waits_for_stop && !stop.is_stopped() {
                std::thread::sleep(Duration::from_millis(1));
            }
            let best = Move::new(Square(12), Square(28), MoveKind::DoublePawnPush);
            SearchResult {
                best_move: Some(best),
                ponder: None,
                info: vec![SearchInfo {
                    depth: limits.depth.unwrap_or(1),
                    seldepth: limits.depth.unwrap_or(1),
                    score: Score(12),
                    nodes: 42,
                    nps: 21_000,
                    hashfull: Some(0),
                    pv: vec![best],
                }],
            }
        }

        fn set_hash_size_mb(&mut self, mb: usize) {
            self.hash_sizes
                .lock()
                .expect("hash sizes mutex should not be poisoned")
                .push(mb);
        }

        fn set_thread_count(&mut self, threads: u16) {
            self.thread_counts
                .lock()
                .expect("thread counts mutex should not be poisoned")
                .push(threads);
        }
    }

    #[test]
    fn parses_required_commands() {
        assert_eq!(parse_uci_line("uci"), Ok(UciCommand::Uci));
        assert_eq!(parse_uci_line("debug on"), Ok(UciCommand::Debug(true)));
        assert_eq!(parse_uci_line("isready"), Ok(UciCommand::IsReady));
        assert_eq!(
            parse_uci_line("setoption name Move Overhead value 25"),
            Ok(UciCommand::SetOption {
                name: "Move Overhead".to_owned(),
                value: Some("25".to_owned())
            })
        );
        assert_eq!(
            parse_uci_line("position startpos moves e2e4 e7e5"),
            Ok(UciCommand::Position {
                startpos: true,
                fen: None,
                moves: vec!["e2e4".to_owned(), "e7e5".to_owned()]
            })
        );
        assert_eq!(
            parse_uci_line("position fen 4k3/P6p/8/8/8/8/7P/4K3 w - - 0 1 moves a7a8q"),
            Ok(UciCommand::Position {
                startpos: false,
                fen: Some("4k3/P6p/8/8/8/8/7P/4K3 w - - 0 1".to_owned()),
                moves: vec!["a7a8q".to_owned()]
            })
        );
    }

    #[test]
    fn parses_go_limits() {
        let command = parse_uci_line(
            "go depth 7 nodes 1000 movetime 250 wtime 5000 btime 6000 winc 25 binc 50 movestogo 12",
        )
        .unwrap();

        let UciCommand::Go(limits) = command else {
            panic!("expected go command");
        };
        assert_eq!(limits.depth, Some(7));
        assert_eq!(limits.nodes, Some(1000));
        assert_eq!(limits.movetime, Some(Duration::from_millis(250)));
        assert_eq!(limits.white_time, Some(Duration::from_millis(5000)));
        assert_eq!(limits.black_time, Some(Duration::from_millis(6000)));
        assert_eq!(limits.white_increment, Some(Duration::from_millis(25)));
        assert_eq!(limits.black_increment, Some(Duration::from_millis(50)));
        assert_eq!(limits.moves_to_go, Some(12));
    }

    #[test]
    fn rejects_malformed_commands_without_panicking() {
        assert!(matches!(
            parse_uci_line("unknown"),
            Err(UciParseError::UnknownCommand(_))
        ));
        assert!(matches!(
            parse_uci_line("go depth nope"),
            Err(UciParseError::InvalidArgument { .. })
        ));
        assert!(matches!(
            parse_uci_line("position fen 8/8/8/8/8/8/8/8 w - -"),
            Err(UciParseError::InvalidPosition(_))
        ));
    }

    #[test]
    fn option_state_accepts_required_options() {
        let mut options = UciOptions::default();

        options.set_option("Hash", Some("128")).unwrap();
        options.set_option("Threads", Some("4")).unwrap();
        options.set_option("Move Overhead", Some("75")).unwrap();

        assert_eq!(options.hash_mb, 128);
        assert_eq!(options.threads, 4);
        assert_eq!(options.move_overhead, Duration::from_millis(75));
        assert!(options.set_option("Hash", Some("0")).is_err());
    }

    #[test]
    fn loop_handles_smoke_commands_and_infinite_stop() {
        let input = b"uci
isready
setoption name Hash value 16
position startpos moves e2e4 e7e5
go depth 2
go infinite
stop
quit
";
        let mut output = Vec::new();
        let searcher = RecordingSearcher::default();
        let hash_sizes = searcher.hash_sizes.clone();
        let thread_counts = searcher.thread_counts.clone();

        run_uci_loop(io::Cursor::new(input.to_vec()), &mut output, searcher).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("id name Kensho Chess Engine"));
        assert!(output.contains("option name Move Overhead"));
        assert!(output.contains("readyok"));
        assert!(output.contains("info depth 2"));
        assert_eq!(output.matches("bestmove e2e4").count(), 2);
        assert_eq!(*hash_sizes.lock().unwrap(), vec![16]);
        assert!(thread_counts.lock().unwrap().is_empty());
    }

    #[test]
    fn loop_interrupts_timed_and_clock_searches_with_stop() {
        let input = b"position startpos
go movetime 2000
stop
go wtime 5000 btime 5000 movestogo 20
stop
quit
";
        let mut output = Vec::new();
        let started = Instant::now();

        run_uci_loop(
            io::Cursor::new(input.to_vec()),
            &mut output,
            RecordingSearcher::default(),
        )
        .unwrap();

        let elapsed = started.elapsed();
        let output = String::from_utf8(output).unwrap();
        assert!(
            elapsed < Duration::from_millis(200),
            "timed UCI stop took {elapsed:?}; output was:\n{output}"
        );
        assert_eq!(output.matches("bestmove e2e4").count(), 2);
    }

    #[test]
    fn invalid_position_does_not_replace_current_position() {
        let searcher = RecordingSearcher::default();
        let positions = searcher.positions.clone();
        let input = b"position startpos moves e2e4
position startpos moves e2e5
quit
";
        let mut output = Vec::new();

        run_uci_loop(io::Cursor::new(input.to_vec()), &mut output, searcher).unwrap();

        let recorded = positions.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert!(recorded[1].contains("moves=1"));
        assert!(String::from_utf8(output).unwrap().contains("illegal move"));
    }
}
