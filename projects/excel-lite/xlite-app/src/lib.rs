mod workbook;

use std::sync::{Mutex, MutexGuard};

use tauri::State;
use workbook::{AppCellFormat, CellSnapshot, RecalcDelta, RectA1, WorkbookAdapter, WorkbookMeta};

type CommandResult<T> = Result<T, String>;

#[derive(Default)]
struct AppState {
    workbook: Mutex<WorkbookAdapter>,
}

fn lock_workbook<'state>(
    state: &'state State<'_, AppState>,
) -> CommandResult<MutexGuard<'state, WorkbookAdapter>> {
    state
        .workbook
        .lock()
        .map_err(|_| "workbook state is unavailable".to_string())
}

#[tauri::command]
async fn new_workbook(state: State<'_, AppState>) -> CommandResult<WorkbookMeta> {
    let mut workbook = lock_workbook(&state)?;
    Ok(workbook.new_workbook())
}

#[tauri::command]
async fn set_cell(
    state: State<'_, AppState>,
    sheet: u16,
    addr: String,
    raw: String,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.set_cell(sheet, &addr, &raw)
}

#[tauri::command]
async fn insert_rows(
    state: State<'_, AppState>,
    sheet: u16,
    at: u32,
    count: u32,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.insert_rows(sheet, at, count)
}

#[tauri::command]
async fn delete_rows(
    state: State<'_, AppState>,
    sheet: u16,
    at: u32,
    count: u32,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.delete_rows(sheet, at, count)
}

#[tauri::command]
async fn insert_cols(
    state: State<'_, AppState>,
    sheet: u16,
    at: u32,
    count: u32,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.insert_cols(sheet, at, count)
}

#[tauri::command]
async fn delete_cols(
    state: State<'_, AppState>,
    sheet: u16,
    at: u32,
    count: u32,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.delete_cols(sheet, at, count)
}

#[tauri::command]
async fn set_format(
    state: State<'_, AppState>,
    sheet: u16,
    rect: RectA1,
    format: AppCellFormat,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.set_format(sheet, rect, format)
}

#[tauri::command]
async fn resize_column(
    state: State<'_, AppState>,
    sheet: u16,
    col: u32,
    width: f32,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.resize_column(sheet, col, width)
}

#[tauri::command]
async fn resize_row(
    state: State<'_, AppState>,
    sheet: u16,
    row: u32,
    height: f32,
) -> CommandResult<RecalcDelta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.resize_row(sheet, row, height)
}

#[tauri::command]
async fn open_workbook(state: State<'_, AppState>, path: String) -> CommandResult<WorkbookMeta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.open_workbook(path)
}

#[tauri::command]
async fn save_workbook(state: State<'_, AppState>, path: String) -> CommandResult<()> {
    let workbook = lock_workbook(&state)?;
    workbook.save_workbook(path)
}

#[tauri::command]
async fn import_csv(state: State<'_, AppState>, path: String) -> CommandResult<WorkbookMeta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.import_csv(path)
}

#[tauri::command]
async fn import_xlsx(state: State<'_, AppState>, path: String) -> CommandResult<WorkbookMeta> {
    let mut workbook = lock_workbook(&state)?;
    workbook.import_xlsx(path)
}

#[tauri::command]
async fn export_csv(state: State<'_, AppState>, path: String) -> CommandResult<()> {
    let workbook = lock_workbook(&state)?;
    workbook.export_csv(path)
}

#[tauri::command]
async fn undo(state: State<'_, AppState>) -> CommandResult<Option<RecalcDelta>> {
    let mut workbook = lock_workbook(&state)?;
    workbook.undo()
}

#[tauri::command]
async fn redo(state: State<'_, AppState>) -> CommandResult<Option<RecalcDelta>> {
    let mut workbook = lock_workbook(&state)?;
    workbook.redo()
}

#[tauri::command]
async fn get_cell(
    state: State<'_, AppState>,
    sheet: u16,
    addr: String,
) -> CommandResult<CellSnapshot> {
    let workbook = lock_workbook(&state)?;
    workbook.get_cell(sheet, &addr)
}

#[tauri::command]
async fn get_viewport(
    state: State<'_, AppState>,
    sheet: u16,
    rect: RectA1,
) -> CommandResult<Vec<CellSnapshot>> {
    let workbook = lock_workbook(&state)?;
    workbook.get_viewport(sheet, rect)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            new_workbook,
            set_cell,
            insert_rows,
            delete_rows,
            insert_cols,
            delete_cols,
            set_format,
            resize_column,
            resize_row,
            open_workbook,
            save_workbook,
            import_csv,
            import_xlsx,
            export_csv,
            undo,
            redo,
            get_cell,
            get_viewport
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Excel Lite shell");
}
