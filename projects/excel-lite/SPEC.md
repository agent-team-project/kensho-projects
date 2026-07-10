# Excel-Lite — Build-Ready Specification

**Status:** Ready for build · **Version:** 1.0 (v1 scope) · **Audience:** Kensho parallel agent org (builders, reviewers)
**Reference semantics oracle:** LibreOffice Calc 24.8 (primary), Microsoft Excel 365 (secondary; divergences enumerated in Appendix A)

> This document is the single source of truth for the v1 build. It is written to be executed by a parallelized organization of AI coding agents building in reviewed slices. Interfaces are given as concrete type/function signatures, not prose. Every worksheet function's acceptance is an objective, machine-checkable test case. Where a design decision is a documented deviation from Excel, it is called out explicitly and pinned by a conformance case.

---

## Table of Contents

1. [Overview, Goals, Demonstration Purpose](#1-overview-goals-demonstration-purpose)
2. [Non-Goals / Out of Scope](#2-non-goals--out-of-scope)
3. [Architecture + Module Map with Interfaces](#3-architecture--module-map-with-interfaces)
4. [Tech Stack + Rationale; Engine/UI Decoupling](#4-tech-stack--rationale-engineui-decoupling)
5. [The Acceptance Bar](#5-the-acceptance-bar)
6. [Test Strategy](#6-test-strategy)
7. [Milestones as Incremental Slices](#7-milestones-as-incremental-slices)
8. [Epic → Issue Breakdown (Project Board Seed)](#8-epic--issue-breakdown-project-board-seed)
9. [Risks and Mitigations](#9-risks-and-mitigations)
- [Appendix A: Documented Deviations from Excel](#appendix-a-documented-deviations-from-excel)
- [Appendix B: Conformance Case File Schema](#appendix-b-conformance-case-file-schema)
- [Appendix C: Glossary](#appendix-c-glossary)

---

## 1. Overview, Goals, Demonstration Purpose

### 1.1 What we are building

**Excel-Lite** is a fully local desktop spreadsheet application for macOS. A person opens it, sees a grid, types `=SUM(A1:A10)` or `=VLOOKUP("SKU-42", A1:C100, 3, FALSE)` into a cell, presses Enter, and sees the correct result. Changing a precedent cell recalculates dependents automatically. Files save and load losslessly from local disk. No cloud, no account, no network.

### 1.2 Goals

- **G1 — Correctness first.** A headless calculation engine whose behavior matches documented spreadsheet semantics (LibreOffice Calc as oracle), verified by an exhaustive, data-driven conformance suite. Correctness is the primary deliverable; the UI is the demonstration surface.
- **G2 — Breadth via parallelism.** A worksheet **function library** of 140+ functions (148 enumerated in §8), structured so each function is an independent, individually testable unit behind one uniform interface. Dozens of agents implement functions concurrently with **zero shared-file edits and zero merge collisions**.
- **G3 — Legibility.** The engine is decoupled from the UI and testable in isolation (`cargo test`, no GUI). The UI is a thin client over a stable command API.
- **G4 — Incremental, reviewable delivery.** A walking skeleton lands first; every subsequent slice is independently reviewable and independently shippable.
- **G5 — Self-contained.** Ships as a signed `.app` / `.dmg`. All state on local disk. Works fully offline.

### 1.3 Demonstration purpose

Excel-Lite is a **public demonstration of autonomous, parallelized software engineering**. Its value as a demo comes from three properties, all of which this spec is engineered to produce:

1. **Legible outcome.** Anyone can judge success in ten seconds: type a formula, get the right answer.
2. **Embarrassingly parallel core.** The function library is a fan-out of ~148 disjoint units, so the org's throughput is visibly gated by review capacity, not by serialization. This is the flagship demonstration of the parallel-agent model.
3. **Objective quality bar.** Success is not a matter of opinion. The conformance suite is green or it is not. Recalc propagates or it does not. Round-trips are lossless or they are not.

### 1.4 Definition of done (v1)

- The **acceptance bar** in §5 is fully green in CI.
- The walking skeleton through Slice 5 (§7) are all merged.
- A first-run user can: create a sheet, enter numbers/text/formulas, use any of the 148 functions, edit a precedent and watch recalc, save to a native file, reopen it, export/import CSV, and undo/redo — all offline.

---

## 2. Non-Goals / Out of Scope

Explicitly **out of scope for v1**. Agents must not build these; reviewers must reject PRs that introduce them without a scope change.

| # | Out of scope (v1) | Notes / possible v2 |
|---|---|---|
| N1 | **Charts / graphs / sparklines** | No plotting of any kind. |
| N2 | **Macros / scripting / VBA / any code execution from a cell** | `INDIRECT` is allowed (it is a function, not code execution). |
| N3 | **Real-time collaboration / multi-user / presence** | Single local user only. |
| N4 | **Cloud sync, accounts, telemetry, auto-update-over-network, license checks** | Zero network at runtime. CI/build tooling may use network; the shipped app may not. |
| N5 | **Pivot tables** | v2 candidate. |
| N6 | **Conditional formatting** | v2 candidate. |
| N7 | **Cell styling beyond minimal** | v1 supports: number display formats (general/fixed/percent/currency/date), horizontal alignment, bold. No fonts, colors, borders, fills, merged cells. |
| N8 | **Multiple sheets/tabs** | v1 is **single-sheet** per workbook. The model reserves a `sheet_index` dimension so multi-sheet is additive in v2. Cross-sheet refs (`Sheet2!A1`) parse but resolve to `#REF!` in v1 (pinned by a conformance case). |
| N9 | **`.xlsx` write** | v1 does **read-only** best-effort `.xlsx` import. Native format is our own. |
| N10 | **Array/spill formulas (dynamic arrays)** | Functions return scalars in v1. Range **arguments** are fully supported; array **results** are not. `MATCH`/`INDEX` return scalars. |
| N11 | **Iterative calculation for circular references** | Circular refs are **detected and flagged** (see §3.5), not iteratively solved. |
| N12 | **Custom/user-defined functions, add-ins, plugins** | The function set is the fixed built-in library. |
| N13 | **Full Excel function parity** | We implement the 148 enumerated functions (§8). Unknown function names resolve to `#NAME?` (pinned by a conformance case). |
| N14 | **Localization / non-`en-US` locale** | Decimal point `.`, argument separator `,`, `en-US` function names only. |
| N15 | **Print / PDF export** | v2 candidate. CSV export is in scope. |

---

## 3. Architecture + Module Map with Interfaces

### 3.1 System shape

```
┌─────────────────────────────────────────────────────────────────┐
│  Desktop app (Tauri v2, macOS .app)                               │
│                                                                   │
│  ┌───────────────────────────┐      IPC (typed commands)          │
│  │  UI (webview)             │◄────────────────────────┐          │
│  │  TypeScript + Svelte      │                         │          │
│  │  - canvas grid renderer   │      ┌──────────────────▼───────┐  │
│  │  - selection / editing    │      │  Tauri command layer      │  │
│  │  - formula bar            │─────►│  (crate: xlite-app)       │  │
│  │  - number formatting view │      │  thin adapter, no logic   │  │
│  └───────────────────────────┘      └──────────────┬───────────┘  │
│                                                     │              │
│                       ┌─────────────────────────────▼───────────┐ │
│                       │  ENGINE  (crate: xlite-core)             │ │
│                       │  headless · std-only · UI-free           │ │
│                       │  ┌───────────┐  ┌────────────────────┐   │ │
│                       │  │ lexer     │  │ value/error model  │   │ │
│                       │  │ parser    │  │ cell model         │   │ │
│                       │  │ AST       │  │ workbook           │   │ │
│                       │  └─────┬─────┘  └─────────┬──────────┘   │ │
│                       │  ┌─────▼─────┐  ┌─────────▼──────────┐   │ │
│                       │  │ evaluator │◄─┤ function registry  │   │ │
│                       │  │ coercion  │  │ (automod+inventory)│   │ │
│                       │  └─────┬─────┘  └─────────┬──────────┘   │ │
│                       │        │        ┌─────────▼──────────┐   │ │
│                       │  ┌─────▼──────┐ │ FUNCTION LIBRARY   │   │ │
│                       │  │ dep graph  │ │ math/stats/logical │   │ │
│                       │  │ recalc     │ │ text/date/lookup   │   │ │
│                       │  │ cycles     │ │ financial/info     │   │ │
│                       │  └────────────┘ └────────────────────┘   │ │
│                       │  ┌────────────┐ ┌────────────────────┐   │ │
│                       │  │ file format│ │ undo/redo commands │   │ │
│                       │  └────────────┘ └────────────────────┘   │ │
│                       └──────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

**Crates**
- `xlite-core` — the engine. **Zero UI or Tauri dependencies.** Compiles and tests standalone. Target: `cargo test -p xlite-core` runs the entire conformance suite headless. Also compilable to `wasm32-unknown-unknown` (see §4.4).
- `xlite-app` — the Tauri command layer + build config. Thin. Translates IPC calls to `xlite-core` API calls. Contains no calculation logic.
- `ui/` — the TypeScript/Svelte webview app.

**The hard rule:** `xlite-app` and `ui/` may depend on `xlite-core`; `xlite-core` may depend on **neither**. A reviewer rejects any PR that adds a UI/Tauri/DOM dependency to `xlite-core`. This is the decoupling that makes the engine verifiable without the GUI.

### 3.2 Cell / value model (`xlite-core::model`)

The value model is the vocabulary every other module speaks. It is defined **first** (Slice 0) and frozen early.

```rust
/// A fully-evaluated cell value. This is what the evaluator produces
/// and what the UI renders. Blank is distinct from empty-string Text.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),          // all numeric values incl. dates/times (serial), percentages, currency
    Text(String),
    Boolean(bool),        // TRUE / FALSE
    Error(ErrorValue),
    Blank,                // an empty cell referenced in a formula; coerces to 0 or "" by context
}

/// The seven canonical spreadsheet error values, plus one documented deviation (Circular).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorValue {
    Null,     // #NULL!   — invalid range intersection
    Div0,     // #DIV/0!  — division by zero (or MOD by zero, etc.)
    Value,    // #VALUE!  — wrong type / uncoercible argument
    Ref,      // #REF!    — reference to a deleted/invalid cell
    Name,     // #NAME?   — unknown function or unparseable name
    Num,      // #NUM!    — numeric domain error (e.g. SQRT(-1))
    Na,       // #N/A     — value not available (e.g. VLOOKUP no match)
    Circular, // #CIRC!   — DEVIATION: circular reference (Excel iterates; we flag). See Appendix A.1
}

impl ErrorValue {
    /// Canonical display string, e.g. ErrorValue::Div0 => "#DIV/0!".
    pub fn code(self) -> &'static str { /* ... */ }
}
```

**A1 addressing.** Internal coordinates are 0-based `(row, col)`; external (`A1`) is 1-based with letter columns. Conversion lives in `model::addr`.

```rust
/// Zero-based internal cell coordinate within a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Coord { pub row: u32, pub col: u32 }   // col 0 == "A", row 0 == "1"

/// A cell identity across the workbook (sheet reserved for v2; always 0 in v1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellId { pub sheet: u16, pub coord: Coord }

impl Coord {
    pub fn to_a1(self) -> String;                 // (0,0)   -> "A1"
    pub fn from_a1(s: &str) -> Result<Coord, ParseError>; // "AA10" -> (9,26)
}

/// Grid limits (Excel-compatible ceilings; virtualized, not allocated).
pub const MAX_ROWS: u32 = 1_048_576;   // 1-based row 1..=1048576
pub const MAX_COLS: u32 = 16_384;      // columns A..XFD
```

**Cell.** A cell stores its *source* (what the user typed) and its *cached evaluation*. Source is the source of truth; cache is derived and recomputed.

```rust
pub struct Cell {
    /// Exactly what the user committed: "42", "=SUM(A1:A2)", "hello", "3.14%".
    pub raw: String,
    /// Parsed once on commit. None for literals (raw is a literal value).
    pub ast: Option<ast::Expr>,
    /// The literal or last-computed value. Recomputed by the engine; never authoritative.
    pub cached: Value,
    /// Display/number format (see model::format).
    pub format: CellFormat,
}

pub struct Sheet {
    pub name: String,
    /// Sparse: only non-blank cells are stored.
    cells: std::collections::HashMap<Coord, Cell>,
    pub col_widths: HashMap<u32, f32>,   // px; default DEFAULT_COL_WIDTH
    pub row_heights: HashMap<u32, f32>,
}

pub struct Workbook {
    pub sheets: Vec<Sheet>,              // v1: exactly one
    pub date_system: DateSystem,         // Excel1900 (default). See §3.6.
}
```

**Number formatting** (`model::format`) is display-only; it never affects computed values.

```rust
pub enum NumberFormat { General, Fixed(u8), Percent(u8), Currency(u8), Date(DateFormat), Text }
pub struct CellFormat { pub number: NumberFormat, pub align: Align, pub bold: bool }
pub enum Align { Left, Center, Right, Default } // Default: numbers right, text left, bool/error center

/// The single entry point the UI uses to render a value as a string.
pub fn display(value: &Value, format: &CellFormat) -> String;
```

### 3.3 Formula lexer / parser (`xlite-core::syntax`)

#### 3.3.1 Tokens (lexer)

```rust
pub enum Token {
    Number(f64),          // 42, 3.14, 1e-3, .5
    Str(String),          // "hello", "" ; "" inside is escaped as ""
    Bool(bool),           // TRUE / FALSE (case-insensitive)
    CellRef(RawRef),      // A1, $A$1, $A1, A$1, Sheet2!A1
    Ident(String),        // function names, e.g. SUM (uppercased on lex)
    Op(OpToken),          // + - * / ^ & = <> < > <= >= %  and unary +/-
    LParen, RParen,
    Comma,                // argument separator
    Colon,                // range operator A1:B2
    Eof,
}
```

The lexer is whitespace-insensitive except inside string literals. Leading `=` (or `+`/`-` as a formula-start convenience) is stripped by the commit layer before lexing; the lexer sees the expression body.

#### 3.3.2 Grammar (EBNF)

Precedence encoded by the rule stack, lowest → highest. This is Excel's precedence (Appendix A.2 pins the surprising cases: `-2^2 == 4`, `2^3^2 == 64`).

```ebnf
formula      = expr ;

expr         = comparison ;
comparison   = concat  { ( "=" | "<>" | "<" | ">" | "<=" | ">=" ) concat } ;   (* left-assoc *)
concat       = addsub  { "&" addsub } ;                                        (* left-assoc *)
addsub       = muldiv  { ( "+" | "-" ) muldiv } ;                              (* left-assoc *)
muldiv       = power   { ( "*" | "/" ) power } ;                               (* left-assoc *)
power        = unary   { "^" unary } ;                                         (* left-assoc: 2^3^2 = 64 *)
unary        = ( "-" | "+" ) unary | postfix ;                                 (* unary binds tighter than ^ *)
postfix      = primary { "%" } ;                                               (* 50% => 0.5 *)
primary      = number
             | string
             | boolean
             | function_call
             | reference
             | "(" expr ")" ;

function_call = ident "(" [ arg { "," arg } ] ")" ;
arg           = expr | (* empty *) ;                                           (* IF(A1,,0) — empty arg is Blank *)

reference     = ref_atom [ ":" ref_atom ] ;                                    (* A1  or  A1:B2 *)
ref_atom      = [ sheet "!" ] [ "$" ] column [ "$" ] row ;
sheet         = ident ;
column        = "A".."Z" { "A".."Z" } ;
row           = digit { digit } ;
```

Notes pinned by conformance cases:
- **Unary minus outranks `^`**: `-2^2` parses as `(-2)^2 = 4` (Appendix A.2, case `PREC-001`).
- **`^` is left-associative**: `2^3^2 = (2^3)^2 = 64` (case `PREC-002`).
- **Empty argument** (`IF(TRUE,,5)`) → the argument is `Value::Blank` at the AST level.
- **Percent postfix**: `50%` → `0.5`; binds tighter than everything except references.

#### 3.3.3 AST

```rust
pub enum Expr {
    Literal(Value),                                   // Number/Text/Boolean/Blank
    Ref(CellRef),                                     // A1 (absolute flags retained for edit-time rewriting)
    Range(RangeRef),                                  // A1:B2
    Unary { op: UnaryOp, rhs: Box<Expr> },            // Neg, Pos, Percent
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Call { name: String, args: Vec<Expr> },           // name is UPPERCASE
}

pub enum UnaryOp  { Neg, Pos, Percent }
pub enum BinaryOp { Add, Sub, Mul, Div, Pow, Concat, Eq, Ne, Lt, Gt, Le, Ge }

pub struct CellRef  { pub sheet: Option<u16>, pub col: u32, pub row: u32,
                      pub col_abs: bool, pub row_abs: bool }
pub struct RangeRef { pub start: CellRef, pub end: CellRef }
```

**Parser interface** (the stable boundary — one function; errors are values, never panics):

```rust
/// Parse a formula body (leading '=' already stripped). Never panics.
/// A syntax error yields Value::Error(ErrorValue::Name) at commit time,
/// but the parser returns a typed error for good UI messages.
pub fn parse(body: &str) -> Result<Expr, ParseError>;

pub struct ParseError { pub kind: ParseErrorKind, pub pos: usize, pub message: String }
```

### 3.4 AST + evaluator + coercion contract (`xlite-core::eval`)

#### 3.4.1 Evaluator

```rust
/// Everything the evaluator needs to resolve references and call functions.
/// Implemented by the workbook+recalc layer; the evaluator itself is pure.
pub trait EvalContext {
    /// Value of a single cell. Blank cells return Value::Blank.
    fn cell_value(&self, r: CellRef) -> Value;
    /// A rectangular region as a lazy view (no materialization for large ranges).
    fn range_view(&self, r: RangeRef) -> RangeView<'_>;
    /// Resolve a function by uppercase name. None => #NAME?.
    fn function(&self, name: &str) -> Option<&dyn Function>;
    /// Workbook date system (for date functions).
    fn date_system(&self) -> DateSystem;
    /// The cell currently being evaluated (for ROW()/COLUMN() with no args).
    fn current_cell(&self) -> CellId;
}

/// Pure evaluation of one AST node. No I/O, no mutation. Deterministic
/// except for RAND/NOW/TODAY, which read a Clock/Rng carried by the context impl.
pub fn eval(expr: &Expr, ctx: &dyn EvalContext) -> Value;
```

#### 3.4.2 Coercion & error-propagation contract (normative)

This contract is the single most conformance-critical piece of prose in the document. It is pinned by the `COERCE-*` and `ERRPROP-*` conformance families. Helper functions live on `FnContext` (§3.7) so every function coerces identically.

**Error propagation.** Any operand or argument that is `Value::Error(e)` makes the whole expression evaluate to that same error, **left-to-right first-error-wins**, *except* for functions explicitly designed to trap errors: `IFERROR`, `IFNA`, `ISERROR`, `ISERR`, `ISNA`, `ERROR.TYPE`, `N`, `TYPE`. Those receive the error as a value.
- `ERRPROP-001`: `=1/0 + 5` → `#DIV/0!`
- `ERRPROP-002`: `=IFERROR(1/0, 99)` → `99`
- `ERRPROP-003`: `=A1+B1` where A1 is `#REF!` → `#REF!`

**Scalar coercion for arithmetic operators** (`+ - * / ^` and unary `-`/`+`/`%`):

| Operand | → number |
|---|---|
| `Number(n)` | `n` |
| `Boolean(true/false)` | `1` / `0` |
| `Blank` | `0` |
| `Text` that parses as a number (`"5"`, `"3.14"`, `"  2 "` trimmed, `"1e3"`) | that number |
| `Text` that does **not** parse | **`#VALUE!`** |
| `Error(e)` | propagate `e` |

- `COERCE-001`: `="5"+1` → `6`
- `COERCE-002`: `="x"+1` → `#VALUE!`
- `COERCE-003`: `=TRUE+1` → `2`
- `COERCE-004`: `=A1+1` where A1 blank → `1`
- `COERCE-005`: `=1/0` → `#DIV/0!`

**Concatenation `&`** coerces every operand to text: `Number` via general format, `Boolean` → `"TRUE"/"FALSE"`, `Blank` → `""`, `Error` propagates.
- `COERCE-010`: `=1&2` → `"12"`
- `COERCE-011`: `="a"&TRUE` → `"aTRUE"`
- `COERCE-012`: `=1.5&"x"` → `"1.5x"`

**Comparison operators** compare within a type order **Number < Text < Boolean** (Excel's ordering), text compares **case-insensitively** (`"a" = "A"` → TRUE), `Blank` compares as `0` against numbers and as `""` against text.
- `COERCE-020`: `="a"="A"` → `TRUE`
- `COERCE-021`: `=1<"a"` → `TRUE` (number sorts before text)
- `COERCE-022`: `=A1=0` where A1 blank → `TRUE`

**Aggregation vs. argument coercion (critical distinction).** Functions that scan **ranges/arrays** (SUM, AVERAGE, MAX, MIN, COUNT, …) **ignore** text and blank cells found *in the range* (COUNT ignores; COUNTA counts non-blank; AVERAGE divides by count of numerics). But **direct scalar arguments** that are numeric text coerce. This mirrors Excel exactly.
- `AGG-001`: `A1=1, A2="x", A3=3`; `=SUM(A1:A3)` → `4` (text ignored)
- `AGG-002`: `=SUM("5", 2)` → `7` (scalar numeric text coerces)
- `AGG-003`: `A1=1, A2="x", A3=3`; `=COUNT(A1:A3)` → `2`
- `AGG-004`: same cells; `=COUNTA(A1:A3)` → `3`
- `AGG-005`: same cells; `=AVERAGE(A1:A3)` → `2`
- `AGG-006`: `A1=1,A2=2`; `=AVERAGE(A1:A3)` (A3 blank) → `1.5` (blank not counted)

Each function's spec (§8 issue) states which discipline it uses; the default helper `FnContext::iter_numbers(range)` implements the "ignore text/blank" scan.

### 3.5 Dependency graph, incremental recalc, cycle detection (`xlite-core::recalc`)

The recalc engine is the workbook's `EvalContext` implementer. It owns the dependency DAG and the dirty-propagation algorithm.

```rust
pub struct RecalcEngine { /* workbook + graph + topo cache */ }

pub struct RecalcDelta {
    /// Cells whose displayed value changed, for the UI to patch.
    pub changed: Vec<(CellId, Value)>,
    /// Cells detected in a circular reference this pass (set to #CIRC!).
    pub circular: Vec<CellId>,
}

impl RecalcEngine {
    /// Commit a user edit to one cell: reparse, rewire deps, recompute the
    /// transitive dirty set in topological order, detect cycles. O(affected).
    pub fn set_cell(&mut self, id: CellId, raw: &str) -> RecalcDelta;

    /// Full recompute (used on load). Topologically orders all formula cells.
    pub fn recalc_all(&mut self) -> RecalcDelta;

    /// Direct precedents (cells this cell reads).
    pub fn precedents(&self, id: CellId) -> Vec<CellId>;
    /// Direct dependents (cells that read this cell).
    pub fn dependents(&self, id: CellId) -> Vec<CellId>;
}
```

**Algorithm (normative):**
1. On `set_cell`, reparse the new AST, extract its precedent set (expanding ranges into their member cells — with a spilled-range optimization for large ranges, see Risk R6), and replace this cell's outgoing edges in the graph.
2. Compute the **dirty set** = the changed cell plus the transitive closure of its dependents (reverse edges).
3. **Topologically sort** the dirty set. If a cycle is found (Kahn's algorithm leaves nodes with nonzero in-degree, or DFS finds a back-edge), every cell **on** a cycle is assigned `Value::Error(ErrorValue::Circular)` and reported in `RecalcDelta.circular`. Cells that merely *depend on* a circular cell propagate `#CIRC!` normally (it is an error value). **The engine never infinite-loops**; cycle detection is a hard guarantee enforced by a timeout-guarded test family.
4. Evaluate the (acyclic remainder of the) dirty set in topological order, writing `cached` values, and collect changed cells into the delta.

**Incremental guarantee:** a `set_cell` touching cell X recomputes *only* X's transitive dependents, never the whole sheet. This is asserted by instrumenting an evaluation counter (see `RECALC-*` test family, §5.3).

**Cycle semantics** (documented deviation, Appendix A.1): Excel offers iterative calc and shows `0` with a warning; we deterministically flag `#CIRC!`. This is deliberate — a deterministic, machine-checkable result beats Excel's stateful warning for our acceptance bar. LibreOffice's `Err:522` is the analogous behavior.

- `CYCLE-001`: `A1: =A1` → A1 = `#CIRC!`, no hang.
- `CYCLE-002`: `A1: =B1`, `B1: =A1` → both `#CIRC!`.
- `CYCLE-003`: three-cell cycle A→B→C→A → all three `#CIRC!`.
- `CYCLE-004`: break the cycle (`B1: =5`) → A1 recovers to a real value.
- `CYCLE-005`: `C1: =A1` where A1∈cycle → C1 = `#CIRC!` (propagation), C1 not itself reported in `circular`.

### 3.6 Date system (`xlite-core::model::date`)

v1 uses the **Excel 1900 date system**, **including** the historical phantom leap day (serial 60 = 1900-02-29), for `.xlsx` round-trip compatibility. This is a deliberate, documented quirk pinned by conformance cases.

```rust
pub enum DateSystem { Excel1900 }  // v2 may add Mac1904

/// Serial 1 == 1900-01-01. Serial 60 is the fictitious 1900-02-29 (Excel bug, preserved).
pub fn serial_to_ymd(serial: f64, sys: DateSystem) -> (i32, u32, u32);
pub fn ymd_to_serial(y: i32, m: u32, d: u32, sys: DateSystem) -> f64;
pub fn time_to_fraction(h: u32, min: u32, s: u32) -> f64; // 12:00 -> 0.5
```

- `DATE-SER-001`: `=DATE(1900,1,1)` → `1`
- `DATE-SER-002`: `=DATE(2020,1,1)` → `43831`
- `DATE-SER-003`: `=DATE(1900,2,28)` → `59`; serial `60` renders `1900-02-29` (phantom day, Appendix A.3)
- `DATE-SER-004`: `=DATE(2024,3,1)-DATE(2024,2,1)` → `29` (2024 real leap year)

### 3.7 The Function Library + registry (`xlite-core::functions`) — **flagship parallel epic**

This is the core of the parallel-agent demonstration. Every worksheet function is an independent unit behind one uniform trait, in its own file, with **zero shared-file edits**.

#### 3.7.1 The uniform function interface

```rust
/// Every worksheet function implements this. Stateless, Send + Sync.
pub trait Function: Send + Sync {
    /// Uppercase canonical name, e.g. "VLOOKUP".
    fn name(&self) -> &'static str;
    /// Inclusive arity bounds. max None == variadic.
    fn arity(&self) -> (usize, Option<usize>);
    /// Evaluate. `args` are already arity-checked. Return a Value (errors are values).
    fn call(&self, args: &[Arg], ctx: &FnContext) -> Value;
}

/// A single function argument, either a scalar value or a lazy range view.
/// Ranges are NOT materialized — SUM(A1:A1000000) iterates cheaply.
pub enum Arg<'a> {
    Value(Value),
    Range(RangeView<'a>),
}

/// Lazy, read-only view over a rectangular region. Iterates non-blank-aware.
pub struct RangeView<'a> { /* … */ }
impl<'a> RangeView<'a> {
    pub fn rows(&self) -> u32;
    pub fn cols(&self) -> u32;
    pub fn get(&self, r: u32, c: u32) -> Value;        // (0,0)-relative to the range
    pub fn iter(&self) -> impl Iterator<Item = Value>; // row-major
}

/// Coercion + helper surface. THE shared contract (§3.4.2) lives here so every
/// function coerces identically. Agents implementing functions call these, never
/// re-implement coercion.
pub struct FnContext<'a> { /* wraps EvalContext */ }
impl<'a> FnContext<'a> {
    // --- scalar coercion (arithmetic discipline) ---
    pub fn to_number(&self, v: &Value) -> Result<f64, ErrorValue>;
    pub fn to_text(&self, v: &Value) -> Result<String, ErrorValue>;
    pub fn to_bool(&self, v: &Value) -> Result<bool, ErrorValue>;
    pub fn to_serial_date(&self, v: &Value) -> Result<f64, ErrorValue>;

    // --- range scanning (aggregation discipline: ignore text/blank) ---
    pub fn iter_numbers<'b>(&self, arg: &'b Arg) -> impl Iterator<Item = f64> + 'b;
    pub fn count_numbers(&self, arg: &Arg) -> usize;
    pub fn count_nonblank(&self, arg: &Arg) -> usize;

    // --- criteria matching for *IF/*IFS families (">5", "<>x", "apple", wildcards) ---
    pub fn matches_criteria(&self, cell: &Value, criteria: &Value) -> bool;

    // --- environment (deterministic in tests via injected Clock/Rng) ---
    pub fn today_serial(&self) -> f64;
    pub fn now_serial(&self) -> f64;
    pub fn rand(&self) -> f64;

    pub fn date_system(&self) -> DateSystem;
    pub fn current_cell(&self) -> CellId;
}
```

#### 3.7.2 Zero-collision registration (the parallelism enabler)

Two crates eliminate **both** kinds of shared-file edit that normally serialize a function fan-out:

1. **`automod`** — generates the `mod` declarations for a directory at build time. Dropping `functions/math/sqrt.rs` makes it a module automatically; **no `mod.rs` edit**.
2. **`inventory`** — link-time distributed collection. Each function file self-registers; **no central registry edit**.

Each function file is fully self-contained:

```rust
// xlite-core/src/functions/math/sqrt.rs   (ONE FILE — one agent — one PR)
use crate::functions::prelude::*;

pub struct Sqrt;
impl Function for Sqrt {
    fn name(&self) -> &'static str { "SQRT" }
    fn arity(&self) -> (usize, Option<usize>) { (1, Some(1)) }
    fn call(&self, args: &[Arg], ctx: &FnContext) -> Value {
        match ctx.to_number(args[0].as_value()) {
            Err(e) => Value::Error(e),
            Ok(n) if n < 0.0 => Value::Error(ErrorValue::Num), // SQRT(-1) => #NUM!
            Ok(n) => Value::Number(n.sqrt()),
        }
    }
}

// Self-registration. No other file is touched.
inventory::submit! { FunctionEntry(&Sqrt as &dyn Function) }

#[cfg(test)]
mod tests {
    use super::*;
    // Unit tests co-located; conformance cases live in the corpus (Appendix B).
    #[test] fn positive() { /* SQRT(9) == 3 */ }
    #[test] fn negative_is_num_error() { /* SQRT(-1) == #NUM! */ }
}
```

The registry assembles once at startup:

```rust
// xlite-core/src/functions/registry.rs  (written ONCE, never edited per-function)
pub struct FunctionEntry(pub &'static dyn Function);
inventory::collect!(FunctionEntry);

pub fn build_registry() -> HashMap<&'static str, &'static dyn Function> {
    inventory::iter::<FunctionEntry>().map(|e| (e.0.name(), e.0)).collect()
}
```

`automod::dir!("src/functions/math")` (etc.) in `functions/mod.rs` pulls every file in. **Result: adding a function = adding exactly one new file. No merge conflicts across dozens of concurrent function PRs.** A CI check (`REG-DUP`) asserts no two entries share a name and that every enumerated §8 function is present.

> **Fallback (Risk R1):** if `inventory`/`automod` misbehave under `wasm32`, a `build.rs` codegen step globs `functions/**/*.rs` and generates the registry + mod list. Same one-file-per-function authoring contract; validated in Slice 0.

### 3.8 File format (`xlite-core::io`)

**Native format — `.xlite`.** A single, human-legible, versioned JSON document. Formula *source* is authoritative; cached values are stored for fast open but recomputed and reconciled on load. Round-trip must be **lossless** at the model level (§5.4).

```jsonc
{
  "format_version": 1,
  "app": "excel-lite",
  "date_system": "1900",
  "sheets": [{
    "name": "Sheet1",
    "cells": {
      "A1": { "raw": "42",              "format": { "number": "general" } },
      "A2": { "raw": "=SUM(A1:A1)",     "format": { "number": "general" } },
      "B1": { "raw": "hello",           "format": { "align": "left" } }
    },
    "col_widths": { "A": 120 },
    "row_heights": {}
  }]
}
```

```rust
pub fn save_native(wb: &Workbook, path: &Path) -> io::Result<()>;
pub fn load_native(path: &Path) -> Result<Workbook, LoadError>;   // then recalc_all()

/// CSV: import creates literals (numbers parsed, everything else text; a leading '=' is
/// kept as a formula on import — documented). Export writes DISPLAYED values.
pub fn import_csv(path: &Path) -> Result<Workbook, LoadError>;
pub fn export_csv(wb: &Workbook, path: &Path) -> io::Result<()>;

/// Optional, read-only, best-effort .xlsx import via the `calamine` crate.
/// Formulas imported as text where available; else the cached value as a literal.
pub fn import_xlsx(path: &Path) -> Result<Workbook, LoadError>;
```

- Native round-trip: `load_native(save_native(wb)) ≡ wb` at the model level (formulas, literals, formats, computed values). Pinned by property test `ROUNDTRIP-*` (§5.4).
- CSV export writes what the user sees; CSV import round-trips **values** (not formulas), pinned by `CSV-*`.

### 3.9 Undo / redo + structural edits (`xlite-core::history`)

Command pattern. Every mutation is a reversible `Command`. Two stacks (undo/redo); a new command clears redo.

```rust
pub trait Command {
    /// Apply, returning the recalc delta for the UI.
    fn apply(&mut self, wb: &mut RecalcEngine) -> RecalcDelta;
    /// Produce the inverse command (captures prior state at apply time).
    fn invert(&self) -> Box<dyn Command>;
    /// Short label for UI ("Edit A1", "Delete row 4").
    fn label(&self) -> String;
}

pub struct History { undo: Vec<Box<dyn Command>>, redo: Vec<Box<dyn Command>> }
impl History {
    pub fn exec(&mut self, cmd: Box<dyn Command>, wb: &mut RecalcEngine) -> RecalcDelta;
    pub fn undo(&mut self, wb: &mut RecalcEngine) -> Option<RecalcDelta>;
    pub fn redo(&mut self, wb: &mut RecalcEngine) -> Option<RecalcDelta>;
}
```

Concrete commands (each its own issue): `SetCell`, `SetRange` (paste/fill), `ClearRange`, `InsertRows`, `DeleteRows`, `InsertCols`, `DeleteCols`, `SetFormat`, `ResizeColumn`, `ResizeRow`.

**Reference rewriting** on insert/delete rows/cols is the sharp edge: all formulas' relative/absolute refs shift; refs into deleted cells become `#REF!`. This logic is isolated in `history::refshift` and property-tested (`REFSHIFT-*`).
- `REFSHIFT-001`: `B1:=A1`, insert a column before A → `B1` becomes `C1:=B1` (ref follows).
- `REFSHIFT-002`: `B1:=A1`, delete column A → `A1:=#REF!`.
- `REFSHIFT-003`: `A3:=SUM(A1:A2)`, insert row at 2 → `A4:=SUM(A1:A3)` (range grows).

### 3.10 Grid UI (`ui/`) + IPC command API

The UI is a **thin client**. All state lives in `xlite-core` behind the Tauri command layer. The UI holds only view state (scroll offset, selection, edit buffer) and a cache of the visible viewport's snapshots.

**IPC command API (the UI↔core contract).** Typed Tauri commands; every mutation returns a `RecalcDelta` so the UI patches only changed cells.

```rust
// xlite-app command signatures (async; serialized over Tauri IPC)
new_workbook() -> WorkbookMeta
open_workbook(path: String) -> WorkbookMeta                 // load + recalc_all
save_workbook(path: String) -> ()
set_cell(sheet: u16, addr: String, raw: String) -> RecalcDelta
get_cell(sheet: u16, addr: String) -> CellSnapshot          // { raw, value, display }
get_viewport(sheet: u16, rect: RectA1) -> Vec<CellSnapshot> // batched visible-window read
clear_range(sheet: u16, rect: RectA1) -> RecalcDelta
set_format(sheet: u16, rect: RectA1, fmt: CellFormat) -> RecalcDelta
insert_rows(sheet: u16, at: u32, n: u32) -> RecalcDelta
delete_rows(sheet: u16, at: u32, n: u32) -> RecalcDelta     // + cols variants
resize_column(sheet: u16, col: u32, px: f32) -> ()
undo() -> Option<RecalcDelta>
redo() -> Option<RecalcDelta>
import_csv(path: String) -> WorkbookMeta                    // + export_csv, import_xlsx

struct CellSnapshot { addr: String, raw: String, value: JsonValue, display: String }
```

**Rendering** (`ui/src/grid/`):
- **Virtualized `<canvas>` grid.** Only the visible window (plus a small overscan) is drawn. Column/row headers (`A B C…`, `1 2 3…`), grid lines, cell text via `display()`. Scroll → recompute visible rect → `get_viewport`. Target: 60 fps scroll over a 1,000,000-row sheet.
- **Selection model.** Active cell, single rectangular range (drag or Shift+arrows). Copy/paste of a range (internal clipboard + OS clipboard TSV).
- **Editing.** DOM `<input>` overlay positioned over the active cell. Enter modes: type-to-replace, F2/double-click-to-edit-in-place. Commit on Enter/Tab (moves selection), cancel on Esc. The **formula bar** mirrors the active cell's `raw` and edits it.
- **Keyboard nav.** Arrows, Tab/Shift-Tab, Enter/Shift-Enter, Ctrl+arrow (jump to region edge), Ctrl+Z/Ctrl+Shift+Z (undo/redo), Ctrl+C/V/X, Delete (clear).
- **Value display.** Numbers right-aligned, text left, booleans/errors centered; errors render as their `code()` (`#DIV/0!`), colored red. Number formats applied via `display()`.

UI logic is deliberately minimal so it can be tested lightly (§6.4) while the engine carries the correctness burden.

---

## 4. Tech Stack + Rationale; Engine/UI Decoupling

### 4.1 Chosen stack

| Layer | Choice | Version target |
|---|---|---|
| **Engine core** | **Rust** (`xlite-core`, headless crate) | Rust 1.79+, edition 2021 |
| **Desktop shell** | **Tauri v2** (macOS `.app`/`.dmg`) | Tauri 2.x |
| **UI** | **TypeScript + Svelte**, canvas grid | Svelte 5, TS 5.x, Vite |
| **Function registry** | `inventory` (link-time collection) + `automod` (dir modules) | latest |
| **`.xlsx` read** | `calamine` (pure-Rust, read-only) | latest |
| **Serialization** | `serde` + `serde_json` (native `.xlite`), `csv` crate | latest |
| **Conformance oracle** | LibreOffice Calc 24.8 headless (`soffice` macro) to regenerate expected values | build-time only |

### 4.2 Why Rust core

- **Exhaustive unit-testability, headless.** `xlite-core` is a library crate with **no UI dependency**. The entire conformance suite runs as `cargo test -p xlite-core` — no GUI, no browser, no display server. This directly serves G1 and G3.
- **The `Function` trait + `inventory`/`automod` gives true zero-collision parallelism.** One file per function, no shared registry/mod edits — the mechanical enabler for the flagship epic (G2). This pattern is idiomatic and battle-tested in Rust; it is awkward to reproduce as cleanly in a dynamically-loaded TS module graph without a shared barrel file (a collision point).
- **Correctness ergonomics.** Exhaustive `match` on `Value`/`ErrorValue` means the compiler forces every function to handle every value/error case — errors are values, never exceptions, so error propagation is explicit and testable. No GC pauses during large recalcs.
- **Performance headroom.** Incremental recalc over up-to-XFD×1M grids, and 60 fps virtualized scrolling backed by fast viewport reads, are comfortable in Rust.
- **One core, two targets.** `xlite-core` compiles natively (tests + Tauri backend) **and** to `wasm32` (optional in-webview path, §4.4). The correctness artifact is the same binary logic in both.

### 4.3 Why Tauri v2 (over Electron)

- Small, self-contained macOS bundle; system WebView (no bundled Chromium); low memory. Fully offline by construction.
- First-class Rust backend: the Tauri command layer calls `xlite-core` directly with no FFI ceremony.
- Strict CSP + no network needed at runtime satisfies the "no cloud, self-contained" constraint (N4).

**Why not TypeScript core?** A TS core is viable and lowers the barrier for some agents, but it weakens the two properties we most need: (a) the *enforced* engine/UI decoupling (a Rust crate boundary is a hard wall; a TS module boundary is a convention), and (b) zero-collision registration (TS needs a barrel/index file that every function PR would edit — a serialization point). We choose the stack that maximizes parallelism and verifiability.

### 4.4 Engine/UI decoupling — enforced, not aspirational

- `xlite-core` has **zero** `tauri`, `wry`, DOM, or `web-sys` dependencies. Enforced by a CI check (`DECOUPLE-01`) that greps `xlite-core/Cargo.toml` for forbidden deps and fails the build if present.
- The engine's public API (`RecalcEngine`, `Workbook`, `parse`, `eval`, `Function`, `save_native`/`load_native`, `History`) is the **only** surface the app layer may touch. No UI type crosses into the core.
- **Two integration paths, same core:**
  - **v1 default — IPC.** Core runs in the Tauri backend; UI calls typed commands (§3.10). Simple, and recalc is fast enough that per-commit IPC round-trips are imperceptible; viewport reads are batched.
  - **Optimization path — WASM.** `xlite-core` compiled to `wasm32` and run inside the webview for zero-latency reads on huge sheets. Gated behind a benchmark showing IPC is the bottleneck; the core code is identical.
- **Consequence:** the acceptance bar (§5) is met entirely against `xlite-core`. The GUI can be broken and the engine's correctness is still provable. This is the decoupling the brief demands.

---

## 5. The Acceptance Bar

The build is "done" when **all** of the following are green in CI. Each is objective and machine-checkable.

### 5.1 The formula conformance suite

A **data-driven corpus** of test cases under `xlite-core/tests/conformance/*.toml` (schema in Appendix B). A single harness (`conformance_runner`) loads every case, constructs a workbook from `setup`, evaluates `formula` in a target cell, and asserts the result equals `expect` (numeric cases within `tol`, default `1e-9` relative). Cases are grouped by family: `FN-<FUNC>-*`, `COERCE-*`, `ERRPROP-*`, `AGG-*`, `PREC-*`, `DATE-*`, plus per-function families.

**Coverage targets (all mandatory):**
1. **Every implemented function has ≥ 5 conformance cases**, covering at minimum: (a) a nominal case, (b) a boundary/edge case, (c) an error-input case, (d) a type-coercion case, (e) an empty/blank-cell case. Functions with mode arguments (VLOOKUP exact/approx, ROUND direction, WEEKDAY types) add cases per mode. The **average is expected to be 8–12 cases/function**; across 148 functions plus the shared families, the corpus is on the order of **1,200+ cases**.
2. **Composite/nested formulas:** ≥ 40 cases exercising nesting and operator precedence, e.g. `=IF(SUM(A1:A3)>10, VLOOKUP(...), "")`, `PREC-*`.
3. **Error semantics:** every `ErrorValue` variant is produced by ≥ 3 distinct cases (`#REF!`, `#DIV/0!`, `#VALUE!`, `#NAME?`, `#NUM!`, `#N/A`, `#NULL!`, `#CIRC!`).
4. **Code coverage gate:** `xlite-core/src/functions/**` and `xlite-core/src/eval/**` at **≥ 90% line coverage** (`cargo llvm-cov`), enforced in CI.
5. **Registry completeness (`REG-DUP`, `REG-COMPLETE`):** no duplicate function names; every function enumerated in §8 is registered and has a conformance file.

**Oracle discipline.** Expected values are validated against **LibreOffice Calc 24.8**. A build-time script (`scripts/regen_oracle.py`, headless `soffice`) can regenerate every case's `expect` from its `setup`+`formula`, guaranteeing the corpus reflects real spreadsheet semantics rather than an author's memory. Divergences from Excel are enumerated in Appendix A and each is pinned by a case tagged `deviation = "..."`.

### 5.2 Recalc correctness (`RECALC-*`)

Instrumented tests that assert both the *values* and the *incrementality*:
- `RECALC-001` propagation: `A1=1`, `B1==A1+1` → B1=2; set `A1=5` → B1=6.
- `RECALC-002` cascade: `A1→B1→C1→D1` chain; edit A1, all downstream update.
- `RECALC-003` diamond: `A1→{B1,C1}→D1`; D1 computed once, correct.
- `RECALC-004` fan-out: `B1..B1000 == A1*2`; edit A1 updates all 1000.
- `RECALC-005` **incrementality**: with 10,000 formula cells, editing a leaf recomputes only its transitive dependents — asserted via an evaluation counter exposed in test builds. Recomputing the whole sheet fails the test.
- `RECALC-006` range dependency: `S1==SUM(A1:A100)`; editing any of A1..A100 updates S1; editing A101 does not.

### 5.3 Cycle detection (`CYCLE-*`)

Per §3.5, all cycle cases run under a **hard timeout** (the harness fails if any case exceeds 1s — proving no infinite loop). Cases `CYCLE-001..005` above are mandatory; additional: self-reference through a range (`A1==SUM(A1:A2)`), and cycle-recovery after edit.

### 5.4 Load/save round-trip fidelity (`ROUNDTRIP-*`, `CSV-*`)

- **Native property test:** generate random valid workbooks (formulas, literals, formats, varied cell types incl. every error value), `save_native` then `load_native` + `recalc_all`, assert **model equality** (raw formulas, literal values, formats, and recomputed values all identical). ≥ 1,000 generated cases via `proptest`.
- **CSV value round-trip:** `import_csv(export_csv(wb))` preserves displayed values.
- **`.xlsx` import smoke:** a fixture `.xlsx` (numbers, text, a SUM, a VLOOKUP) imports without error and its formula/values match expected (`XLSX-001..005`).

### 5.5 The bar, summarized

| Gate | Threshold | Enforced by |
|---|---|---|
| Conformance suite | 100% pass, ≥5 cases/function, ≥1,000 cases total | `conformance_runner`, CI |
| Function/eval line coverage | ≥ 90% | `cargo llvm-cov`, CI |
| Recalc correctness + incrementality | 100% pass | `RECALC-*`, CI |
| Cycle detection (no hang) | 100% pass under 1s timeout | `CYCLE-*`, CI |
| Native round-trip | lossless, ≥1000 proptest cases | `ROUNDTRIP-*`, CI |
| CSV round-trip | values preserved | `CSV-*`, CI |
| Registry completeness/no-dupes | all §8 functions present, unique | `REG-*`, CI |
| Engine/UI decoupling | no forbidden deps in `xlite-core` | `DECOUPLE-01`, CI |

---

## 6. Test Strategy

### 6.1 The headless engine suite is central

Correctness lives in `xlite-core` and is verified without any GUI. Three layers:

1. **Co-located unit tests** (`#[cfg(test)] mod tests` in each source file). Each function file ships its own unit tests (fast feedback for the implementing agent). Lexer/parser/eval/recalc/refshift each have unit tests.
2. **The data-driven conformance corpus** (§5.1, Appendix B). The authoritative, reviewer-legible bar. A new function's PR is incomplete without its `conformance/fn_<name>.toml`.
3. **Property tests** (`proptest`): round-trip fidelity (§5.4), refshift invariants, parser round-trip (`parse → format → parse` is stable), and "no panic on arbitrary input" fuzzing of the lexer/parser.

### 6.2 What each PR must include (Definition of Done per unit)

- **Function PR:** the `functions/<group>/<name>.rs` file (impl + `inventory::submit!` + unit tests) **and** `tests/conformance/fn_<name>.toml` with ≥5 cases spanning the mandated categories. CI runs the new cases against the oracle-validated expectations. No edits to any shared file.
- **Module PR (parser, recalc, io, …):** unit tests + the relevant `*-*` conformance/property families green.

### 6.3 Oracle regeneration

`scripts/regen_oracle.py` drives headless LibreOffice to compute expected values for the whole corpus, catching author errors. Run in CI as an advisory job (network/soffice needed) and locally when authoring; the committed `expect` values are the enforced gate.

### 6.4 UI tested separately and lighter

The UI carries no correctness burden, so it is tested proportionally:
- **Component tests** (Vitest + Testing Library) for the grid model glue: viewport math (which cells are visible for a scroll offset), selection reducer, edit-buffer state machine, keyboard-nav reducer, TSV clipboard (de)serialization. These are pure TS units, no engine.
- **A small end-to-end smoke** (Tauri driver / WebDriver): launch the app, type `=SUM(A1:A3)` after entering three numbers, assert the rendered display; type a `VLOOKUP`; edit a precedent and assert the dependent's display updates; save then reopen a file. ~6 scripted flows — the "a human opens it and it works" guarantee (G3), automated.
- No pixel-diff or heavy visual regression in v1.

### 6.5 CI pipeline

Single workflow, gates in order: `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test --workspace` (incl. conformance) → `cargo llvm-cov` (≥90% gate on core) → `DECOUPLE-01`/`REG-*` checks → `vitest` (UI units) → `cargo build --release` (Tauri bundle) → e2e smoke. Red on any gate blocks merge.

---

## 7. Milestones as Incremental Slices

Each slice is independently reviewable and leaves `main` green and demoable. The function-library fan-out (Slice 2) is where the org's parallel throughput is showcased.

### Slice 0 — Walking skeleton *(serial, small team; unblocks everything)*
**Goal:** a human types a number and a formula and sees a result; recalc works for a 3-function engine.
- `xlite-core`: `Value`/`ErrorValue`/`Coord`/`Cell`/`Workbook` model; lexer+parser+AST for numbers, refs, ranges, `+ - * /`, and function calls; evaluator with the coercion contract skeleton; `RecalcEngine::set_cell`/`recalc_all` with dependency graph + cycle detection; the `Function` trait + `inventory`/`automod` registry **proven end-to-end with exactly 3 functions: `SUM`, `IF`, `AVERAGE`**; the conformance harness (`conformance_runner`) reading Appendix-B TOML.
- `xlite-app` + `ui/`: Tauri shell; canvas grid (non-virtualized is acceptable here) rendering a small range; click a cell, type, commit; formula bar; `set_cell`/`get_viewport` IPC.
- **Exit test:** enter `1`,`2`,`3` in A1:A3; `=SUM(A1:A3)`→6; `=AVERAGE(A1:A3)`→2; `=IF(A1>0,"pos","neg")`→"pos"; edit A1→5, SUM updates to 10. `CYCLE-001` passes (no hang). Registry pattern proven: adding a 4th function file requires editing no existing file.

### Slice 1 — Engine hardening *(small team; unblocks the fan-out)*
**Goal:** freeze the interfaces the function fan-out depends on.
- Full grammar (comparisons, `&`, `^`, `%`, unary, precedence `PREC-*`); complete coercion & error-propagation contract (`COERCE-*`, `ERRPROP-*`, `AGG-*`); `FnContext` helper surface finalized (`to_number`, `iter_numbers`, `matches_criteria`, date/rand/clock injection); `RangeView` lazy iteration; incremental-recalc instrumentation (`RECALC-005` counter); full `CYCLE-*`.
- **Exit test:** all `COERCE-*`, `ERRPROP-*`, `AGG-*`, `PREC-*`, `RECALC-*`, `CYCLE-*` families green. **`FnContext` and `Function` are frozen** — this is the "interface freeze" that lets dozens of function agents run without stepping on each other.

### Slice 2 — **Function library fan-out** *(the flagship parallel epic; dozens of agents concurrently)*
**Goal:** implement the full 148-function library, one file/one PR per function, zero shared-file edits.
- Every function issue in Epics F–M (§8). Each PR = one `functions/<group>/<name>.rs` + one `conformance/fn_<name>.toml` (≥5 cases). Grouped agents can be scheduled by category but there is **no inter-function dependency** — pure fan-out.
- **Exit test:** §5.1 conformance targets met (≥5 cases/function, ≥1,000 cases, ≥90% coverage on `functions/`); `REG-COMPLETE` green.

### Slice 3 — File format, CSV, undo/redo, structural edits *(parallel sub-teams)*
**Goal:** persistence and editing history.
- Native `.xlite` save/load (`ROUNDTRIP-*`); CSV import/export (`CSV-*`); optional `.xlsx` read (`XLSX-*`); undo/redo command stack; insert/delete rows/cols with reference rewriting (`REFSHIFT-*`).
- **Exit test:** all §5.4 round-trip gates green; undo/redo and refshift families green.

### Slice 4 — Grid UI polish *(UI sub-team)*
**Goal:** the legible, fluid spreadsheet experience.
- Virtualized canvas (60 fps over 1M rows); full selection/range model; keyboard nav; in-cell editing + formula bar parity; copy/paste (internal + OS TSV clipboard); column/row resize; number-format display (general/fixed/percent/currency/date), alignment, bold; red error rendering.
- **Exit test:** §6.4 e2e smoke flows green; virtualization benchmark met.

### Slice 5 — Packaging & acceptance *(small team)*
**Goal:** shippable, self-contained macOS app; full bar green.
- Signed `.app`/`.dmg` via Tauri bundler; offline-verification check (no network calls at runtime); final acceptance-bar sweep (§5) green end-to-end; first-run UX (empty workbook, sample file).
- **Exit test:** the §1.4 Definition of Done, fully offline.

---

## 8. Epic → Issue Breakdown (Project Board Seed)

IDs are `EL-###`. Function issues (Epics F–M) are the flagship fan-out: **each is disjoint, one file, one PR, no shared-file edit.** Every function issue's acceptance is "impl + `inventory::submit!` + ≥5 conformance cases (nominal, edge, error, coercion, empty) validated against the LibreOffice oracle; unit tests co-located."

### Epic A — Core value & error model *(Slice 0)*
- **EL-001** `Value` enum (Number/Text/Boolean/Error/Blank) + equality/tolerance helpers
- **EL-002** `ErrorValue` enum + `code()` display strings (all 8 variants)
- **EL-003** `Coord`/`CellId` + A1↔(row,col) conversion, column-letter codec, grid ceilings
- **EL-004** `Cell`/`Sheet`/`Workbook` model (sparse storage)
- **EL-005** `NumberFormat`/`CellFormat`/`Align` + `display(value, format)` general format

### Epic B — Lexer / Parser / AST *(Slice 0→1)*
- **EL-010** Lexer: tokens (numbers incl. `1e-3`/`.5`, strings w/ `""` escape, bools, refs, ops, `%`)
- **EL-011** `RawRef` reference lexing (`A1`, `$A$1`, `A$1`, `Sheet!A1`)
- **EL-012** `Expr` AST types (Literal/Ref/Range/Unary/Binary/Call)
- **EL-013** Recursive-descent parser: precedence ladder per EBNF (`PREC-*`)
- **EL-014** Unary/percent/empty-argument handling; leading `=`/`+`/`-` strip layer
- **EL-015** `ParseError` typed errors + positions (for UI messages)
- **EL-016** Parser property tests (no panic on arbitrary input; parse→format→parse stable)

### Epic C — Evaluator & coercion contract *(Slice 0→1)*
- **EL-020** `EvalContext` trait + pure `eval(expr, ctx)`
- **EL-021** Arithmetic operators + scalar coercion (`COERCE-001..005`)
- **EL-022** Concatenation `&` coercion (`COERCE-010..012`)
- **EL-023** Comparison operators + type ordering + case-insensitive text (`COERCE-020..022`)
- **EL-024** Error propagation (first-error-wins) + trap-function exemptions (`ERRPROP-*`)
- **EL-025** Aggregation vs argument coercion discipline; `FnContext` helper surface (`AGG-*`)
- **EL-026** `RangeView` lazy rectangular view + iterators
- **EL-027** `Arg` enum + arity checking layer feeding `Function::call`

### Epic D — Dependency graph, incremental recalc, cycle detection *(Slice 0→1)*
- **EL-030** Dependency DAG: precedent extraction (range expansion), edge maintenance
- **EL-031** Dirty-set transitive closure over reverse edges
- **EL-032** Topological order + incremental evaluation (`RECALC-001..006`)
- **EL-033** Cycle detection → `#CIRC!` assignment + propagation (`CYCLE-*`), timeout guarantee
- **EL-034** Evaluation-counter instrumentation for incrementality tests (`RECALC-005`)

### Epic E — Function library infrastructure *(Slice 0→1; unblocks fan-out)*
- **EL-040** `Function` trait + `arity()` contract
- **EL-041** `FunctionEntry` + `inventory::collect!` registry + `build_registry()`
- **EL-042** `automod` dir wiring for `functions/{math,stats,logical,text,date,lookup,financial,info}`
- **EL-043** `functions::prelude` (shared imports for function files)
- **EL-044** `FnContext` implementation (coercion + criteria + clock/rng injection)
- **EL-045** `matches_criteria` DSL (`">5"`, `"<>x"`, `"a*"`, `"?"` wildcards) for `*IF/*IFS`
- **EL-046** Registry checks `REG-DUP`/`REG-COMPLETE`; `build.rs` codegen fallback (Risk R1)
- **EL-047** `conformance_runner` harness (loads Appendix-B TOML, runs, tolerant compare)

### Epic F — Function library: **Math / Trig** *(Slice 2 — parallel)*
- **EL-100** SUM · **EL-101** PRODUCT · **EL-102** POWER · **EL-103** ABS · **EL-104** SQRT
- **EL-105** MOD (sign-of-divisor; `MOD(-3,2)=1`) · **EL-106** QUOTIENT · **EL-107** INT (floor toward −∞; `INT(-2.5)=-3`) · **EL-108** TRUNC (`TRUNC(-2.7)=-2`)
- **EL-109** ROUND (half away from zero; `ROUND(2.5,0)=3`) · **EL-110** ROUNDUP · **EL-111** ROUNDDOWN · **EL-112** MROUND
- **EL-113** CEILING · **EL-114** FLOOR · **EL-115** SIGN · **EL-116** EXP · **EL-117** LN (`LN(0)=#NUM!`) · **EL-118** LOG · **EL-119** LOG10 · **EL-120** PI
- **EL-121** GCD · **EL-122** LCM · **EL-123** SUMSQ
- **EL-124** SUMIF · **EL-125** SUMIFS · **EL-126** SUMPRODUCT
- **EL-127** SIN · **EL-128** COS · **EL-129** TAN · **EL-130** SQRTPI
- **EL-131** RAND (injected RNG; deterministic in tests) · **EL-132** RANDBETWEEN

### Epic G — Function library: **Statistics** *(Slice 2 — parallel)*
- **EL-140** AVERAGE · **EL-141** AVERAGEIF · **EL-142** AVERAGEIFS
- **EL-143** COUNT · **EL-144** COUNTA · **EL-145** COUNTBLANK · **EL-146** COUNTIF · **EL-147** COUNTIFS
- **EL-148** MAX · **EL-149** MIN · **EL-150** MAXIFS · **EL-151** MINIFS
- **EL-152** MEDIAN · **EL-153** MODE · **EL-154** STDEV (sample) · **EL-155** STDEVP (population)
- **EL-156** VAR · **EL-157** VARP · **EL-158** LARGE · **EL-159** SMALL · **EL-160** RANK
- **EL-161** PERCENTILE · **EL-162** QUARTILE

### Epic H — Function library: **Logical** *(Slice 2 — parallel)*
- **EL-170** IF · **EL-171** IFS · **EL-172** AND · **EL-173** OR · **EL-174** NOT · **EL-175** XOR
- **EL-176** IFERROR · **EL-177** IFNA · **EL-178** SWITCH · **EL-179** TRUE · **EL-180** FALSE

### Epic I — Function library: **Text** *(Slice 2 — parallel)*
- **EL-190** CONCAT · **EL-191** CONCATENATE · **EL-192** TEXTJOIN
- **EL-193** LEFT · **EL-194** RIGHT · **EL-195** MID · **EL-196** LEN
- **EL-197** LOWER · **EL-198** UPPER · **EL-199** PROPER · **EL-200** TRIM
- **EL-201** SUBSTITUTE · **EL-202** REPLACE · **EL-203** FIND (case-sensitive) · **EL-204** SEARCH (case-insensitive, wildcards)
- **EL-205** TEXT (format codes) · **EL-206** VALUE · **EL-207** NUMBERVALUE · **EL-208** REPT
- **EL-209** EXACT · **EL-210** CHAR · **EL-211** CODE

### Epic J — Function library: **Date / Time** *(Slice 2 — parallel)*
- **EL-220** DATE · **EL-221** TIME · **EL-222** TODAY (injected clock) · **EL-223** NOW (injected clock)
- **EL-224** YEAR · **EL-225** MONTH · **EL-226** DAY · **EL-227** HOUR · **EL-228** MINUTE · **EL-229** SECOND
- **EL-230** WEEKDAY (return-type modes 1/2/3) · **EL-231** WEEKNUM
- **EL-232** EDATE · **EL-233** EOMONTH · **EL-234** DATEDIF (units "Y"/"M"/"D") · **EL-235** DAYS
- **EL-236** NETWORKDAYS · **EL-237** WORKDAY · **EL-238** DATEVALUE · **EL-239** TIMEVALUE · **EL-240** YEARFRAC (basis 0–4)

### Epic K — Function library: **Lookup / Reference** *(Slice 2 — parallel)*
- **EL-250** VLOOKUP (exact + approx modes) · **EL-251** HLOOKUP
- **EL-252** INDEX · **EL-253** MATCH (match-types −1/0/1) · **EL-254** LOOKUP
- **EL-255** CHOOSE · **EL-256** OFFSET · **EL-257** INDIRECT
- **EL-258** ROW · **EL-259** COLUMN · **EL-260** ROWS · **EL-261** COLUMNS · **EL-262** ADDRESS

### Epic L — Function library: **Financial** *(Slice 2 — parallel)*
- **EL-270** PMT · **EL-271** FV · **EL-272** PV · **EL-273** RATE · **EL-274** NPER
- **EL-275** NPV · **EL-276** IRR · **EL-277** IPMT · **EL-278** PPMT
- **EL-279** SLN · **EL-280** DB · **EL-281** DDB · **EL-282** CUMIPMT

### Epic M — Function library: **Information** *(Slice 2 — parallel)*
- **EL-290** ISBLANK · **EL-291** ISNUMBER · **EL-292** ISTEXT · **EL-293** ISNONTEXT
- **EL-294** ISERROR · **EL-295** ISERR · **EL-296** ISNA · **EL-297** ISLOGICAL
- **EL-298** NA · **EL-299** TYPE · **EL-300** N · **EL-301** ERROR.TYPE

### Epic N — File format *(Slice 3)*
- **EL-310** Native `.xlite` schema + `serde` model + `save_native`
- **EL-311** `load_native` + reconcile via `recalc_all` (`ROUNDTRIP-*`)
- **EL-312** Round-trip property tests (`proptest`, ≥1000 cases)
- **EL-313** CSV import (`import_csv`) · **EL-314** CSV export (`export_csv`) (`CSV-*`)
- **EL-315** `.xlsx` read via `calamine` (optional, best-effort; `XLSX-*`)

### Epic O — Undo/redo & structural edits *(Slice 3)*
- **EL-320** `Command` trait + `History` (undo/redo stacks, redo-clear)
- **EL-321** `SetCell`/`SetRange`/`ClearRange` commands
- **EL-322** `InsertRows`/`DeleteRows`/`InsertCols`/`DeleteCols` commands
- **EL-323** Reference rewriting `history::refshift` (`REFSHIFT-*`)
- **EL-324** `SetFormat`/`ResizeColumn`/`ResizeRow` commands

### Epic P — Grid UI *(Slice 0 seed → Slice 4)*
- **EL-330** Canvas grid renderer (headers, grid lines, cell text)
- **EL-331** Virtualization + scrolling (60 fps / 1M rows) + overscan
- **EL-332** Selection model (active cell, rectangular range)
- **EL-333** Keyboard navigation reducer (arrows/Tab/Enter/Ctrl-arrow)
- **EL-334** In-cell editor overlay + edit-mode state machine
- **EL-335** Formula bar (raw mirror + commit)
- **EL-336** Copy/paste (internal + OS TSV clipboard)
- **EL-337** Column/row resize UI
- **EL-338** Number-format display + alignment + bold + red error rendering
- **EL-339** UI component tests (Vitest) for viewport math, selection, edit reducers

### Epic Q — Tauri shell, IPC, packaging *(Slice 0 seed → Slice 5)*
- **EL-350** Tauri v2 scaffold (macOS), Vite/Svelte wiring, CSP no-network
- **EL-351** IPC command layer (`set_cell`/`get_viewport`/`open`/`save`/`undo`/… → core)
- **EL-352** `RecalcDelta`/`CellSnapshot` serialization contract
- **EL-353** `.app`/`.dmg` bundling + signing + first-run sample workbook
- **EL-354** Offline-verification test (no runtime network) + e2e smoke flows (§6.4)

### Epic R — Conformance suite & CI *(spans all slices)*
- **EL-360** `conformance/` TOML schema (Appendix B) + fixtures loader
- **EL-361** Coercion/precedence/error corpus (`COERCE-*`,`PREC-*`,`ERRPROP-*`,`AGG-*`)
- **EL-362** Recalc + cycle corpus (`RECALC-*`,`CYCLE-*`)
- **EL-363** `scripts/regen_oracle.py` (headless LibreOffice expected-value regenerator)
- **EL-364** Coverage gate (`cargo llvm-cov` ≥90% on core) in CI
- **EL-365** `DECOUPLE-01` (no UI deps in core) + `REG-DUP`/`REG-COMPLETE` checks
- **EL-366** CI workflow (fmt/clippy/test/coverage/checks/vitest/build/e2e)
- **EL-367** Appendix A deviation-case pinning (`PREC-001/002`, `DATE-SER-003`, `CYCLE-*`, cross-sheet `#REF!`, unknown-fn `#NAME?`)

### Issue count

| Epic | Area | Issues |
|---|---|---|
| A | Core value/error model | 5 |
| B | Lexer/Parser/AST | 7 |
| C | Evaluator & coercion | 8 |
| D | Dep graph/recalc/cycles | 5 |
| E | Function infra | 8 |
| **F–M** | **FUNCTION LIBRARY (fan-out)** | **148** |
| N | File format | 6 |
| O | Undo/redo & structural | 5 |
| P | Grid UI | 10 |
| Q | Tauri/IPC/packaging | 5 |
| R | Conformance & CI | 8 |
| **Total** | | **215** |

Function-library issues alone (Epics F–M): **148 distinct, one-function-per-file issues** — Math/Trig 33, Statistics 23, Logical 11, Text 22, Date/Time 21, Lookup/Reference 13, Financial 13, Information 12 — far exceeding the 40+ target and forming the embarrassingly-parallel flagship. Every function issue is disjoint and collision-free by construction (§3.7.2): one new file, one PR, no shared-file edit. The remaining 67 issues (Epics A–E, N–R) are the engine, file format, UI, and CI scaffolding, mostly front-loaded into Slices 0–1 before the fan-out begins.

---

## 9. Risks and Mitigations

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| **R1** | `inventory`/`automod` link-time collection fails or is flaky under `wasm32` / release LTO | Breaks the zero-collision registration that the whole fan-out depends on | **Prove in Slice 0** end-to-end (native + wasm). Ship the `build.rs` codegen fallback (EL-046) that globs `functions/**/*.rs` and generates the registry + mod list — same one-file-per-function authoring contract, no shared edits. |
| **R2** | Floating-point divergence between our engine, LibreOffice, and Excel (rounding, financial iteration) | Conformance cases fail spuriously or hide real bugs | Tolerant compare (`tol`, default `1e-9` relative). Oracle regeneration (EL-363) pins expected values to a real engine. Document reference (LibreOffice 24.8) and every Excel divergence (Appendix A). Financial iterative funcs (RATE/IRR) specify convergence tolerance + max iterations. |
| **R3** | Circular-reference semantics deviate from Excel (we flag `#CIRC!`; Excel iterates) | User surprise; "not like Excel" criticism | Deliberate, documented deviation (Appendix A.1) chosen for **deterministic, machine-checkable** results. Pinned by `CYCLE-*`. Iterative calc is an explicit non-goal (N11), a clean v2 add. |
| **R4** | Reference rewriting on insert/delete rows/cols is subtle (ranges grow/shrink, deleted refs → `#REF!`, absolute vs relative) | Silent data corruption on structural edits | Isolate in `history::refshift` (EL-323) behind property tests (`REFSHIFT-*`) covering grow/shrink/delete/absolute cases. Ships in Slice 3, after the engine is frozen, not entangled with recalc. |
| **R5** | Excel 1900 date system quirks (phantom 1900-02-29, serial base) | Off-by-one date bugs; `.xlsx` mismatch | Adopt Excel serial semantics **including** the phantom leap day for round-trip compatibility (§3.6), pinned by `DATE-SER-*`. Single date module (`model::date`) — all date functions go through it. |
| **R6** | Large-range dependencies (`=SUM(A1:A1000000)`) explode the dependency graph if every member cell is a node | Memory blow-up, slow recalc, defeats incrementality | `RangeView` never materializes; the dep graph stores **range-precedent edges** (a formula depends on a rectangle) and dirty-propagation intersects an edited cell against range rectangles, rather than expanding ranges into per-cell edges. Benchmarked in `RECALC-004/006`. |
| **R7** | IPC round-trip latency makes per-keystroke editing or fast scroll feel laggy on big sheets | Poor "it just works" demo (G3) | Batch viewport reads (`get_viewport`); commit-on-Enter (not per-keystroke) sends one `set_cell`; `RecalcDelta` patches only changed cells. **WASM path** (§4.4) reserved as the escalation, gated by a benchmark — same core code. |
| **R8** | Parallel-agent merge collisions on shared files (registry, `mod.rs`, big enums) | Serializes the fan-out; defeats the demo's thesis | One-file-per-function + `automod`+`inventory` (§3.7.2) removes per-function shared edits entirely. Interfaces frozen at end of Slice 1 before the fan-out starts (Slice 2). Enum/model churn confined to Epics A–E, done before fan-out. |
| **R9** | Coercion-contract ambiguity ⇒ each function agent invents its own rules | Inconsistent semantics, conformance whack-a-mole | The coercion contract (§3.4.2) is normative and implemented **once** in `FnContext` helpers (EL-044/045). Function agents call helpers; re-implementing coercion is a reviewer-reject. Pinned by `COERCE-*`/`AGG-*`. |
| **R10** | Scope creep (charts, multi-sheet, dynamic arrays) sneaks in via "helpful" PRs | Timeline blow-out; instability | Non-goals (§2) are enumerated and enforceable; reviewers reject out-of-scope PRs. Cross-sheet refs and unknown functions have **pinned** defined behaviors (`#REF!`, `#NAME?`) so their absence is tested, not accidental. |
| **R11** | `.xlsx` import fidelity varies wildly across real-world files | Import feels broken | It is explicitly **read-only, best-effort, optional** (N9, EL-315). Smoke-tested against a controlled fixture only; failures degrade gracefully (import cached values as literals), never crash. |
| **R12** | Coverage/perf gates flake in CI (llvm-cov, 60fps bench, soffice oracle) | Red builds unrelated to correctness | Coverage gate scoped to `core` only. Perf/oracle jobs are separated: perf is a benchmark with a margin; the soffice oracle is advisory (committed `expect` values are the enforced gate, EL-363). |

---

## Appendix A: Documented Deviations from Excel

Each deviation is intentional and pinned by a conformance case tagged `deviation`.

- **A.1 — Circular references.** Excel offers iterative calculation and otherwise shows `0` with a warning. We deterministically assign `#CIRC!` to every cell on a cycle (analogous to LibreOffice `Err:522`). Pinned: `CYCLE-001..005`. Rationale: machine-checkable determinism for the acceptance bar; iterative calc is a v2 non-goal (N11).
- **A.2 — Operator precedence surprises (these MATCH Excel; pinned to prevent regressions).** `-2^2 = 4` (unary minus binds tighter than `^`) — `PREC-001`. `2^3^2 = 64` (`^` left-associative) — `PREC-002`. `50% = 0.5`, `="1"+1 = 2`.
- **A.3 — 1900 date system phantom leap day.** Serial 60 renders `1900-02-29` (a date that never existed), matching Excel for `.xlsx` compatibility. Pinned: `DATE-SER-003`.
- **A.4 — Cross-sheet references.** v1 is single-sheet (N8). `Sheet2!A1` parses successfully but evaluates to `#REF!`. Pinned: `XSHEET-001`.
- **A.5 — Unknown functions.** Any function name not in the registry evaluates to `#NAME?` (Excel behaves the same for unknown names). Pinned: `NAME-001` (`=FOObar(1)` → `#NAME?`).
- **A.6 — Locale.** `en-US` only: `.` decimal, `,` argument separator, uppercase `TRUE`/`FALSE`, English function names (N14).

## Appendix B: Conformance Case File Schema

One TOML file per function (`tests/conformance/fn_<name>.toml`) plus shared families (`coerce.toml`, `precedence.toml`, `recalc.toml`, `cycle.toml`, …). The `conformance_runner` (EL-047) loads all files.

```toml
# tests/conformance/fn_vlookup.toml
[[case]]
id = "FN-VLOOKUP-001"                 # unique, stable
desc = "exact match returns 3rd column"
# setup: literal cell contents keyed by A1 address. Values may be numbers,
# strings, or "=..." formulas. Blank cells are simply omitted.
setup = { A1 = "SKU-1", B1 = 10, C1 = "red",
          A2 = "SKU-2", B2 = 20, C2 = "green" }
formula = "=VLOOKUP(\"SKU-2\", A1:C2, 3, FALSE)"
expect = { text = "green" }           # exactly one of: number | text | bool | error | blank

[[case]]
id = "FN-VLOOKUP-002"
desc = "no match, exact mode => #N/A"
setup = { A1 = "SKU-1", B1 = 10 }
formula = "=VLOOKUP(\"SKU-9\", A1:B1, 2, FALSE)"
expect = { error = "#N/A" }

[[case]]
id = "FN-VLOOKUP-003"
desc = "approximate match (sorted) returns largest <= key"
setup = { A1 = 10, B1 = "a", A2 = 20, B2 = "b", A3 = 30, B3 = "c" }
formula = "=VLOOKUP(25, A1:B3, 2, TRUE)"
expect = { text = "b" }

[[case]]
id = "FN-VLOOKUP-004"
desc = "col index out of range => #REF!"
setup = { A1 = 1, B1 = 2 }
formula = "=VLOOKUP(1, A1:B1, 5, FALSE)"
expect = { error = "#REF!" }

[[case]]
id = "FN-VLOOKUP-005"
desc = "empty lookup range cell ignored; blank result coerces"
setup = { A1 = "k", A2 = "k2", B2 = 7 }   # B1 blank
formula = "=VLOOKUP(\"k\", A1:B2, 2, FALSE)"
expect = { number = 0 }                    # blank cell -> 0
tol = 0                                     # optional; default 1e-9 relative for numbers

# optional metadata
# oracle = "libreoffice-24.8"   (default)
# deviation = "A.3"             (present only for pinned deviations)
```

**Result matching rules:** `number` compares within `tol` (relative, default `1e-9`); `text` is exact; `bool` exact; `error` matches the `code()` string; `blank` matches `Value::Blank`. Exactly one result key is allowed per case.

## Appendix C: Glossary

- **Precedent / dependent** — cells a formula reads / cells that read a formula.
- **Dirty set** — the transitive closure of dependents needing recompute after an edit.
- **Coercion** — converting a `Value` to the type an operator/function needs (contract §3.4.2).
- **Aggregation discipline** — range scans ignore text and blanks (SUM/AVERAGE/COUNT).
- **Argument discipline** — direct scalar arguments coerce numeric text (`SUM("5",2)=7`).
- **Oracle** — the reference engine (LibreOffice Calc 24.8) whose results define expected values.
- **Conformance case** — one objective input→expected-output test in the corpus (Appendix B).
- **Fan-out** — the parallel implementation of independent function issues (Slice 2 / Epics F–M).
```
