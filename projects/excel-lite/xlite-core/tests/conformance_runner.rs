use std::{
    collections::HashMap,
    fs, panic,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use serde::Deserialize;
use xlite_core::{CellId, Coord, ErrorValue, RecalcEngine, Value};

#[derive(Debug, Deserialize)]
struct Suite {
    case: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
struct Case {
    id: String,
    #[serde(rename = "desc")]
    _desc: String,
    kind: Option<String>,
    #[serde(default)]
    setup: HashMap<String, toml::Value>,
    formula: String,
    expect: Expect,
    target: Option<String>,
    tol: Option<f64>,
    timeout_ms: Option<u64>,
    #[serde(rename = "deviation")]
    _deviation: Option<String>,
    #[serde(default)]
    steps: Vec<Step>,
}

#[derive(Clone, Debug, Deserialize)]
struct Step {
    #[serde(default)]
    set: HashMap<String, toml::Value>,
    formula: String,
    expect: Expect,
}

#[derive(Clone, Debug, Deserialize)]
struct Expect {
    number: Option<f64>,
    text: Option<String>,
    bool: Option<bool>,
    error: Option<String>,
    blank: Option<bool>,
}

#[test]
fn conformance_cases_pass() {
    for path in conformance_files() {
        let text = fs::read_to_string(&path).expect("read conformance TOML");
        let suite: Suite = toml::from_str(&text).expect("parse conformance TOML");
        for case in suite.case {
            run_case_with_timeout(&path, case);
        }
    }
}

fn conformance_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut files: Vec<_> = fs::read_dir(&root)
        .expect("read conformance directory")
        .map(|entry| entry.expect("read conformance entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no conformance TOML files found");
    files
}

fn run_case_with_timeout(path: &Path, case: Case) {
    let Some(timeout_ms) = case.timeout_ms else {
        run_case(case);
        return;
    };

    let case_id = case.id.clone();
    let path = path.display().to_string();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = panic::catch_unwind(|| run_case(case));
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => panic::resume_unwind(payload),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{path}: {case_id}: timed out after {timeout_ms}ms")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{path}: {case_id}: runner thread disconnected")
        }
    }
}

fn run_case(case: Case) {
    let mut engine = RecalcEngine::default();

    apply_cells(&mut engine, case.setup);
    assert_formula(
        &mut engine,
        &case.id,
        &case.formula,
        case.target.as_deref(),
        &case.expect,
        case.tol,
    );

    if case.kind.as_deref() == Some("recalc") || !case.steps.is_empty() {
        for (index, step) in case.steps.into_iter().enumerate() {
            let step_id = format!("{}/step-{}", case.id, index + 1);
            apply_cells(&mut engine, step.set);
            assert_formula(
                &mut engine,
                &step_id,
                &step.formula,
                case.target.as_deref(),
                &step.expect,
                case.tol,
            );
        }
    }
}

fn apply_cells(engine: &mut RecalcEngine, cells: HashMap<String, toml::Value>) {
    let mut entries: Vec<_> = cells.into_iter().collect();
    entries.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
    for (addr, value) in entries {
        engine.set_cell(id(&addr), &raw_from_toml(value));
    }
}

fn assert_formula(
    engine: &mut RecalcEngine,
    case_id: &str,
    formula: &str,
    target: Option<&str>,
    expect: &Expect,
    tol: Option<f64>,
) {
    let target = target.unwrap_or("Z1");
    engine.set_cell(id(target), formula);
    let actual = engine.cell_value(id(target));
    assert_value(case_id, &actual, expect, tol);
}

fn raw_from_toml(value: toml::Value) -> String {
    match value {
        toml::Value::String(value) => value,
        toml::Value::Integer(value) => value.to_string(),
        toml::Value::Float(value) => value.to_string(),
        toml::Value::Boolean(value) => {
            if value {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        other => panic!("unsupported setup value: {other:?}"),
    }
}

fn assert_value(case_id: &str, actual: &Value, expect: &Expect, tol: Option<f64>) {
    let populated = [
        expect.number.is_some(),
        expect.text.is_some(),
        expect.bool.is_some(),
        expect.error.is_some(),
        expect.blank.unwrap_or(false),
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    assert_eq!(
        populated, 1,
        "{case_id}: exactly one expectation is required"
    );

    if let Some(expected) = expect.number {
        match actual {
            Value::Number(actual) if within_tolerance(*actual, expected, tol.unwrap_or(1e-9)) => {}
            Value::Number(actual) => assert!(
                within_tolerance(*actual, expected, tol.unwrap_or(1e-9)),
                "{case_id}: expected {expected}, got {actual}"
            ),
            other => panic!("{case_id}: expected number {expected}, got {other:?}"),
        }
    } else if let Some(expected) = &expect.text {
        assert_eq!(actual, &Value::Text(expected.clone()), "{case_id}");
    } else if let Some(expected) = expect.bool {
        assert_eq!(actual, &Value::Boolean(expected), "{case_id}");
    } else if let Some(expected) = &expect.error {
        assert_eq!(
            actual,
            &Value::Error(ErrorValue::from_code(expected).expect("known error code")),
            "{case_id}"
        );
    } else if expect.blank.unwrap_or(false) {
        assert_eq!(actual, &Value::Blank, "{case_id}");
    }
}

fn within_tolerance(actual: f64, expected: f64, tol: f64) -> bool {
    let scale = expected.abs().max(1.0);
    (actual - expected).abs() <= tol * scale
}

fn id(addr: &str) -> CellId {
    CellId {
        sheet: 0,
        coord: Coord::from_a1(addr).unwrap(),
    }
}
