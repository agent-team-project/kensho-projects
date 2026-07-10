import { invoke } from "@tauri-apps/api/core";
import { cellAddress, parseCellAddress } from "./grid";

export type CellValue =
  | { kind: "blank" }
  | { kind: "number"; value: number }
  | { kind: "text"; value: string }
  | { kind: "boolean"; value: boolean }
  | { kind: "error"; code: string };

export interface WorkbookMeta {
  workbookId: string;
  activeSheet: number;
  rows: number;
  cols: number;
}

export interface RectA1 {
  start: string;
  end: string;
}

export type NumberFormatPayloadType = "general" | "fixed" | "percent" | "currency" | "dateIso" | "text";
export type FormatAlign = "default" | "left" | "center" | "right";

export interface NumberFormatPayload {
  type: NumberFormatPayloadType;
  dp?: number;
}

export interface CellFormatPayload {
  number: NumberFormatPayload;
  align: FormatAlign;
  bold: boolean;
}

export interface CellSnapshot {
  addr: string;
  raw: string;
  value: CellValue;
  display: string;
  format?: CellFormatPayload;
}

export interface RecalcDelta {
  changed: CellSnapshot[];
  circular: string[];
}

export interface XliteCommands {
  newWorkbook(): Promise<WorkbookMeta>;
  openWorkbook(path: string): Promise<WorkbookMeta>;
  saveWorkbook(path: string): Promise<void>;
  importCsv(path: string): Promise<WorkbookMeta>;
  importXlsx(path: string): Promise<WorkbookMeta>;
  exportCsv(path: string): Promise<void>;
  setCell(sheet: number, addr: string, raw: string): Promise<RecalcDelta>;
  insertRows(sheet: number, at: number, count: number): Promise<RecalcDelta>;
  deleteRows(sheet: number, at: number, count: number): Promise<RecalcDelta>;
  insertCols(sheet: number, at: number, count: number): Promise<RecalcDelta>;
  deleteCols(sheet: number, at: number, count: number): Promise<RecalcDelta>;
  setFormat(sheet: number, rect: RectA1, format: CellFormatPayload): Promise<RecalcDelta>;
  resizeColumn(sheet: number, col: number, width: number): Promise<RecalcDelta>;
  resizeRow(sheet: number, row: number, height: number): Promise<RecalcDelta>;
  undo(): Promise<RecalcDelta | null>;
  redo(): Promise<RecalcDelta | null>;
  getCell(sheet: number, addr: string): Promise<CellSnapshot>;
  getViewport(sheet: number, rect: RectA1): Promise<CellSnapshot[]>;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function createCommandClient(): XliteCommands {
  if (typeof window !== "undefined" && window.__TAURI_INTERNALS__) {
    return new TauriCommandClient();
  }

  return new BrowserPreviewCommandClient();
}

class TauriCommandClient implements XliteCommands {
  newWorkbook(): Promise<WorkbookMeta> {
    return invoke<WorkbookMeta>("new_workbook");
  }

  openWorkbook(path: string): Promise<WorkbookMeta> {
    return invoke<WorkbookMeta>("open_workbook", { path });
  }

  saveWorkbook(path: string): Promise<void> {
    return invoke<void>("save_workbook", { path });
  }

  importCsv(path: string): Promise<WorkbookMeta> {
    return invoke<WorkbookMeta>("import_csv", { path });
  }

  importXlsx(path: string): Promise<WorkbookMeta> {
    return invoke<WorkbookMeta>("import_xlsx", { path });
  }

  exportCsv(path: string): Promise<void> {
    return invoke<void>("export_csv", { path });
  }

  setCell(sheet: number, addr: string, raw: string): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("set_cell", { sheet, addr, raw });
  }

  insertRows(sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("insert_rows", { sheet, at, count });
  }

  deleteRows(sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("delete_rows", { sheet, at, count });
  }

  insertCols(sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("insert_cols", { sheet, at, count });
  }

  deleteCols(sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("delete_cols", { sheet, at, count });
  }

  setFormat(sheet: number, rect: RectA1, format: CellFormatPayload): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("set_format", { sheet, rect, format });
  }

  resizeColumn(sheet: number, col: number, width: number): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("resize_column", { sheet, col, width });
  }

  resizeRow(sheet: number, row: number, height: number): Promise<RecalcDelta> {
    return invoke<RecalcDelta>("resize_row", { sheet, row, height });
  }

  undo(): Promise<RecalcDelta | null> {
    return invoke<RecalcDelta | null>("undo");
  }

  redo(): Promise<RecalcDelta | null> {
    return invoke<RecalcDelta | null>("redo");
  }

  getCell(sheet: number, addr: string): Promise<CellSnapshot> {
    return invoke<CellSnapshot>("get_cell", { sheet, addr });
  }

  getViewport(sheet: number, rect: RectA1): Promise<CellSnapshot[]> {
    return invoke<CellSnapshot[]>("get_viewport", { sheet, rect });
  }
}

type StructuralAxis = "row" | "column";
type StructuralKind = "insert" | "delete";

interface BrowserPreviewState {
  cells: Map<string, string>;
  formats: Map<string, CellFormatPayload>;
  columnWidths: Map<number, number>;
  rowHeights: Map<number, number>;
}

interface BrowserHistoryEntry {
  before: BrowserPreviewState;
  after: BrowserPreviewState;
  changedAddrs: string[];
}

class BrowserPreviewCommandClient implements XliteCommands {
  private cells = new Map<string, string>();
  private formats = new Map<string, CellFormatPayload>();
  private columnWidths = new Map<number, number>();
  private rowHeights = new Map<number, number>();
  private undoStack: BrowserHistoryEntry[] = [];
  private redoStack: BrowserHistoryEntry[] = [];

  async newWorkbook(): Promise<WorkbookMeta> {
    this.cells.clear();
    this.formats.clear();
    this.columnWidths.clear();
    this.rowHeights.clear();
    this.undoStack = [];
    this.redoStack = [];
    return {
      workbookId: "browser-preview",
      activeSheet: 0,
      rows: 1_048_576,
      cols: 16_384
    };
  }

  async openWorkbook(_path: string): Promise<WorkbookMeta> {
    throw browserPreviewFileOperationError();
  }

  async saveWorkbook(_path: string): Promise<void> {
    throw browserPreviewFileOperationError();
  }

  async importCsv(_path: string): Promise<WorkbookMeta> {
    throw browserPreviewFileOperationError();
  }

  async importXlsx(_path: string): Promise<WorkbookMeta> {
    throw browserPreviewFileOperationError();
  }

  async exportCsv(_path: string): Promise<void> {
    throw browserPreviewFileOperationError();
  }

  async setCell(_sheet: number, addr: string, raw: string): Promise<RecalcDelta> {
    const before = this.captureState();
    this.writeCell(addr, raw);

    return this.commitBrowserEdit(before, [addr]);
  }

  async insertRows(_sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return this.applyStructuralEdit("row", "insert", at, count);
  }

  async deleteRows(_sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return this.applyStructuralEdit("row", "delete", at, count);
  }

  async insertCols(_sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return this.applyStructuralEdit("column", "insert", at, count);
  }

  async deleteCols(_sheet: number, at: number, count: number): Promise<RecalcDelta> {
    return this.applyStructuralEdit("column", "delete", at, count);
  }

  async setFormat(_sheet: number, rect: RectA1, format: CellFormatPayload): Promise<RecalcDelta> {
    const { expandRect } = await import("./rect");
    const before = this.captureState();
    const changedAddrs = expandRect(rect);
    for (const addr of changedAddrs) {
      if (isDefaultFormat(format)) {
        this.formats.delete(addr);
      } else {
        this.formats.set(addr, cloneCellFormat(format));
      }
    }

    return this.commitBrowserEdit(before, changedAddrs);
  }

  async resizeColumn(_sheet: number, col: number, width: number): Promise<RecalcDelta> {
    this.validateResize("column", col, width);
    const before = this.captureState();
    this.columnWidths.set(col, width);
    return this.commitBrowserEdit(before, []);
  }

  async resizeRow(_sheet: number, row: number, height: number): Promise<RecalcDelta> {
    this.validateResize("row", row, height);
    const before = this.captureState();
    this.rowHeights.set(row, height);
    return this.commitBrowserEdit(before, []);
  }

  async undo(): Promise<RecalcDelta | null> {
    const entry = this.undoStack.pop();
    if (!entry) return null;

    this.restoreState(entry.before);
    this.redoStack.push(entry);
    return this.deltaFor(entry.changedAddrs);
  }

  async redo(): Promise<RecalcDelta | null> {
    const entry = this.redoStack.pop();
    if (!entry) return null;

    this.restoreState(entry.after);
    this.undoStack.push(entry);
    return this.deltaFor(entry.changedAddrs);
  }

  async getCell(_sheet: number, addr: string): Promise<CellSnapshot> {
    return this.snapshot(addr);
  }

  async getViewport(_sheet: number, rect: RectA1): Promise<CellSnapshot[]> {
    const { expandRect } = await import("./rect");
    return expandRect(rect).map((addr) => this.snapshot(addr));
  }

  private writeCell(addr: string, raw: string): void {
    if (raw.trim() === "") {
      this.cells.delete(addr);
    } else {
      this.cells.set(addr, raw);
    }
  }

  private applyStructuralEdit(axis: StructuralAxis, kind: StructuralKind, at: number, count: number): RecalcDelta {
    if (!Number.isInteger(at) || at < 0 || !Number.isInteger(count) || count < 1) {
      throw new Error("Invalid structural edit bounds");
    }

    const before = this.captureState();
    const next = new Map<string, string>();
    for (const [addr, raw] of before.cells) {
      const shiftedAddr = this.shiftAddress(addr, axis, kind, at, count);
      if (shiftedAddr) {
        next.set(shiftedAddr, raw);
      }
    }

    const nextFormats = new Map<string, CellFormatPayload>();
    for (const [addr, format] of before.formats) {
      const shiftedAddr = this.shiftAddress(addr, axis, kind, at, count);
      if (shiftedAddr) {
        nextFormats.set(shiftedAddr, cloneCellFormat(format));
      }
    }

    this.cells = next;
    this.formats = nextFormats;
    return this.commitBrowserEdit(before);
  }

  private shiftAddress(
    addr: string,
    axis: StructuralAxis,
    kind: StructuralKind,
    at: number,
    count: number
  ): string | null {
    const parsed = parseCellAddress(addr);
    const position = axis === "row" ? parsed.row : parsed.column;
    let nextPosition = position;

    if (kind === "insert") {
      if (position <= at) return addr;
      nextPosition = position + count;
    } else {
      const firstDeleted = at + 1;
      const lastDeleted = at + count;
      if (position < firstDeleted) return addr;
      if (position <= lastDeleted) return null;
      nextPosition = position - count;
    }

    return axis === "row"
      ? cellAddress(parsed.column, nextPosition)
      : cellAddress(nextPosition, parsed.row);
  }

  private commitBrowserEdit(before: BrowserPreviewState, changedAddrs?: string[]): RecalcDelta {
    const after = this.captureState();
    const resolvedChangedAddrs = changedAddrs ?? this.changedAddrs(before, after);
    this.undoStack.push({ before, after, changedAddrs: resolvedChangedAddrs });
    this.redoStack = [];
    return this.deltaFor(resolvedChangedAddrs);
  }

  private changedAddrs(before: BrowserPreviewState, after: BrowserPreviewState): string[] {
    const addrs = new Set([
      ...before.cells.keys(),
      ...after.cells.keys(),
      ...before.formats.keys(),
      ...after.formats.keys()
    ]);
    return [...addrs]
      .filter(
        (addr) =>
          (before.cells.get(addr) ?? "") !== (after.cells.get(addr) ?? "") ||
          !sameFormat(formatForState(before, addr), formatForState(after, addr))
      )
      .sort(compareCellAddresses);
  }

  private deltaFor(addrs: string[]): RecalcDelta {
    return {
      changed: addrs.map((addr) => this.snapshot(addr)),
      circular: []
    };
  }

  private snapshot(addr: string): CellSnapshot {
    const raw = this.cells.get(addr) ?? "";
    const numeric = raw.trim() !== "" && Number.isFinite(Number(raw)) ? Number(raw) : null;
    const format = this.formatFor(addr);
    return {
      addr,
      raw,
      value:
        raw === ""
          ? { kind: "blank" }
          : numeric === null
            ? { kind: "text", value: raw }
            : { kind: "number", value: numeric },
      display: displayPreviewValue(raw, numeric, format),
      format
    };
  }

  private captureState(): BrowserPreviewState {
    return {
      cells: new Map(this.cells),
      formats: cloneFormatMap(this.formats),
      columnWidths: new Map(this.columnWidths),
      rowHeights: new Map(this.rowHeights)
    };
  }

  private restoreState(state: BrowserPreviewState): void {
    this.cells = new Map(state.cells);
    this.formats = cloneFormatMap(state.formats);
    this.columnWidths = new Map(state.columnWidths);
    this.rowHeights = new Map(state.rowHeights);
  }

  private formatFor(addr: string): CellFormatPayload {
    return cloneCellFormat(this.formats.get(addr) ?? defaultCellFormat);
  }

  private validateResize(axis: "column" | "row", index: number, size: number): void {
    if (!Number.isInteger(index) || index < 0 || !Number.isFinite(size) || size <= 0) {
      throw new Error(`Invalid ${axis} resize`);
    }
  }
}

function compareCellAddresses(left: string, right: string): number {
  const a = parseCellAddress(left);
  const b = parseCellAddress(right);
  return a.row - b.row || a.column - b.column;
}

const defaultCellFormat: CellFormatPayload = {
  number: { type: "general" },
  align: "default",
  bold: false
};

function cloneCellFormat(format: CellFormatPayload): CellFormatPayload {
  return {
    number: { ...format.number },
    align: format.align,
    bold: format.bold
  };
}

function cloneFormatMap(formatMap: Map<string, CellFormatPayload>): Map<string, CellFormatPayload> {
  return new Map([...formatMap].map(([addr, format]) => [addr, cloneCellFormat(format)]));
}

function formatForState(state: BrowserPreviewState, addr: string): CellFormatPayload {
  return state.formats.get(addr) ?? defaultCellFormat;
}

function isDefaultFormat(format: CellFormatPayload): boolean {
  return sameFormat(format, defaultCellFormat);
}

function sameFormat(left: CellFormatPayload, right: CellFormatPayload): boolean {
  return (
    left.number.type === right.number.type &&
    (left.number.dp ?? null) === (right.number.dp ?? null) &&
    left.align === right.align &&
    left.bold === right.bold
  );
}

function displayPreviewValue(raw: string, numeric: number | null, format: CellFormatPayload): string {
  if (raw === "") return "";
  if (numeric === null) return raw;

  switch (format.number.type) {
    case "fixed":
      return numeric.toFixed(decimalPlaces(format.number, 2));
    case "percent":
      return `${(numeric * 100).toFixed(decimalPlaces(format.number, 0))}%`;
    case "currency":
      return `$${numeric.toFixed(decimalPlaces(format.number, 2))}`;
    case "text":
      return displayDebugNumber(numeric);
    case "dateIso":
    case "general":
      return displayGeneralNumber(numeric);
  }
}

function decimalPlaces(format: NumberFormatPayload, fallback: number): number {
  return Math.max(0, Math.trunc(format.dp ?? fallback));
}

function displayGeneralNumber(value: number): string {
  return Number.isInteger(value) && Number.isFinite(value) ? value.toFixed(0) : String(value);
}

function displayDebugNumber(value: number): string {
  return Number.isInteger(value) && Number.isFinite(value) ? value.toFixed(1) : String(value);
}

function browserPreviewFileOperationError(): Error {
  return new Error("File operations are unavailable in browser preview");
}
