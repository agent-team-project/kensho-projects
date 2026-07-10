import type { CellSnapshot } from "./commands";

export type Direction = "up" | "down" | "left" | "right";
export type UndoRedoAction = "undo" | "redo";
export type ClipboardAction = "copy" | "cut" | "paste";
export interface ViewportOrigin {
  row: number;
  column: number;
}

export interface ViewportSize {
  rows: number;
  columns: number;
}

export interface SheetBounds {
  rows: number;
  columns: number;
}

export interface CellSelection {
  anchor: string;
  active: string;
}

export interface NormalizedSelection {
  start: string;
  end: string;
  minColumn: number;
  maxColumn: number;
  minRow: number;
  maxRow: number;
  rows: number;
  columns: number;
}

export function columnLabel(column: number): string {
  if (!Number.isInteger(column) || column < 1) {
    throw new Error(`Invalid column index: ${column}`);
  }

  let label = "";
  let current = column;
  while (current > 0) {
    current -= 1;
    label = String.fromCharCode(65 + (current % 26)) + label;
    current = Math.floor(current / 26);
  }
  return label;
}

export function cellAddress(column: number, row: number): string {
  if (!Number.isInteger(row) || row < 1) {
    throw new Error(`Invalid row index: ${row}`);
  }
  return `${columnLabel(column)}${row}`;
}

export function parseCellAddress(addr: string): { column: number; row: number } {
  const match = /^([A-Za-z]+)([1-9][0-9]*)$/.exec(addr.trim());
  if (!match) {
    throw new Error(`Invalid cell address: ${addr}`);
  }

  const letters = match[1].toUpperCase();
  let column = 0;
  for (const letter of letters) {
    column = column * 26 + (letter.charCodeAt(0) - 64);
  }

  return { column, row: Number.parseInt(match[2], 10) };
}

export function moveCellAddress(
  addr: string,
  direction: Direction,
  maxRows: number,
  maxColumns: number
): string {
  const current = parseCellAddress(addr);
  const next = {
    column: current.column,
    row: current.row
  };

  if (direction === "up") next.row -= 1;
  if (direction === "down") next.row += 1;
  if (direction === "left") next.column -= 1;
  if (direction === "right") next.column += 1;

  next.row = Math.min(Math.max(next.row, 1), maxRows);
  next.column = Math.min(Math.max(next.column, 1), maxColumns);

  return cellAddress(next.column, next.row);
}

export function normalizeSelection(selection: CellSelection, bounds?: SheetBounds): NormalizedSelection {
  if (bounds) {
    validatePositiveInteger(bounds.rows, "sheet rows");
    validatePositiveInteger(bounds.columns, "sheet columns");
  }

  const anchor = parseSelectionCell(selection.anchor, bounds);
  const active = parseSelectionCell(selection.active, bounds);
  const minColumn = Math.min(anchor.column, active.column);
  const maxColumn = Math.max(anchor.column, active.column);
  const minRow = Math.min(anchor.row, active.row);
  const maxRow = Math.max(anchor.row, active.row);

  return {
    start: cellAddress(minColumn, minRow),
    end: cellAddress(maxColumn, maxRow),
    minColumn,
    maxColumn,
    minRow,
    maxRow,
    rows: maxRow - minRow + 1,
    columns: maxColumn - minColumn + 1
  };
}

export function expandSelectionToRows(selection: CellSelection, bounds?: SheetBounds): string[][] {
  const normalized = normalizeSelection(selection, bounds);
  const rows: string[][] = [];

  for (let row = normalized.minRow; row <= normalized.maxRow; row += 1) {
    const currentRow: string[] = [];
    for (let column = normalized.minColumn; column <= normalized.maxColumn; column += 1) {
      currentRow.push(cellAddress(column, row));
    }
    rows.push(currentRow);
  }

  return rows;
}

export function expandSelection(selection: CellSelection, bounds?: SheetBounds): string[] {
  return expandSelectionToRows(selection, bounds).flat();
}

export function isCellInSelection(addr: string, selection: CellSelection, bounds?: SheetBounds): boolean {
  const parsed = parseCellAddress(addr);
  const normalized = normalizeSelection(selection, bounds);

  return (
    parsed.row >= normalized.minRow &&
    parsed.row <= normalized.maxRow &&
    parsed.column >= normalized.minColumn &&
    parsed.column <= normalized.maxColumn
  );
}

export function clampViewportOrigin(
  origin: ViewportOrigin,
  size: ViewportSize,
  bounds: SheetBounds
): ViewportOrigin {
  validatePositiveInteger(size.rows, "viewport rows");
  validatePositiveInteger(size.columns, "viewport columns");
  validatePositiveInteger(bounds.rows, "sheet rows");
  validatePositiveInteger(bounds.columns, "sheet columns");

  const maxRowOrigin = Math.max(1, bounds.rows - size.rows + 1);
  const maxColumnOrigin = Math.max(1, bounds.columns - size.columns + 1);

  return {
    row: clampInteger(origin.row, 1, maxRowOrigin),
    column: clampInteger(origin.column, 1, maxColumnOrigin)
  };
}

export function viewportIndexes(start: number, count: number, max: number): number[] {
  validatePositiveInteger(count, "viewport count");
  validatePositiveInteger(max, "sheet bound");

  const first = clampInteger(start, 1, max);
  const last = Math.min(max, first + count - 1);
  return Array.from({ length: last - first + 1 }, (_, index) => first + index);
}

export function viewportRectFromBounds(
  origin: ViewportOrigin,
  size: ViewportSize,
  bounds: SheetBounds
): { start: string; end: string } {
  const clamped = clampViewportOrigin(origin, size, bounds);
  const endRow = Math.min(bounds.rows, clamped.row + size.rows - 1);
  const endColumn = Math.min(bounds.columns, clamped.column + size.columns - 1);

  return {
    start: cellAddress(clamped.column, clamped.row),
    end: cellAddress(endColumn, endRow)
  };
}

export function isCellInViewport(
  addr: string,
  origin: ViewportOrigin,
  size: ViewportSize,
  bounds: SheetBounds
): boolean {
  const parsed = parseCellAddress(addr);
  const clamped = clampViewportOrigin(origin, size, bounds);
  const endRow = Math.min(bounds.rows, clamped.row + size.rows - 1);
  const endColumn = Math.min(bounds.columns, clamped.column + size.columns - 1);

  return (
    parsed.row >= clamped.row &&
    parsed.row <= endRow &&
    parsed.column >= clamped.column &&
    parsed.column <= endColumn
  );
}

export function viewportOriginForCell(
  addr: string,
  origin: ViewportOrigin,
  size: ViewportSize,
  bounds: SheetBounds
): ViewportOrigin {
  const parsed = parseCellAddress(addr);
  let row = origin.row;
  let column = origin.column;

  if (parsed.row < row) {
    row = parsed.row;
  } else if (parsed.row >= row + size.rows) {
    row = parsed.row - size.rows + 1;
  }

  if (parsed.column < column) {
    column = parsed.column;
  } else if (parsed.column >= column + size.columns) {
    column = parsed.column - size.columns + 1;
  }

  return clampViewportOrigin({ row, column }, size, bounds);
}

export function viewportOriginFromScroll(
  scrollLeft: number,
  scrollTop: number,
  cellWidth: number,
  rowHeight: number,
  size: ViewportSize,
  bounds: SheetBounds
): ViewportOrigin {
  if (!Number.isFinite(cellWidth) || cellWidth <= 0) {
    throw new Error(`Invalid cell width: ${cellWidth}`);
  }
  if (!Number.isFinite(rowHeight) || rowHeight <= 0) {
    throw new Error(`Invalid row height: ${rowHeight}`);
  }

  return clampViewportOrigin(
    {
      row: Math.floor(Math.max(0, scrollTop) / rowHeight) + 1,
      column: Math.floor(Math.max(0, scrollLeft) / cellWidth) + 1
    },
    size,
    bounds
  );
}

export function blankSnapshot(addr: string): CellSnapshot {
  return {
    addr,
    raw: "",
    value: { kind: "blank" },
    display: ""
  };
}

export function isPrintableEditKey(event: Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey">): boolean {
  return event.key.length === 1 && !event.altKey && !event.ctrlKey && !event.metaKey;
}

export function directionForKey(event: Pick<KeyboardEvent, "key" | "shiftKey">): Direction | null {
  if (event.key === "ArrowUp") return "up";
  if (event.key === "ArrowDown") return "down";
  if (event.key === "ArrowLeft") return "left";
  if (event.key === "ArrowRight") return "right";
  if (event.key === "Tab") return event.shiftKey ? "left" : "right";
  return null;
}

export function undoRedoActionForKey(
  event: Pick<KeyboardEvent, "key" | "shiftKey" | "ctrlKey" | "metaKey" | "altKey">
): UndoRedoAction | null {
  if (event.altKey || (!event.ctrlKey && !event.metaKey) || event.key.toLowerCase() !== "z") {
    return null;
  }

  return event.shiftKey ? "redo" : "undo";
}

export function clipboardActionForKey(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey">
): ClipboardAction | null {
  if (event.altKey || (!event.ctrlKey && !event.metaKey)) {
    return null;
  }

  const key = event.key.toLowerCase();
  if (key === "c") return "copy";
  if (key === "x") return "cut";
  if (key === "v") return "paste";
  return null;
}

export function serializeTsv(rows: readonly (readonly string[])[]): string {
  return rows.map((row) => row.map(serializeTsvField).join("\t")).join("\n");
}

export function parseTsv(text: string): string[][] {
  if (text === "") {
    return [[""]];
  }

  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let inQuotes = false;
  let fieldStarted = false;
  let endedWithRecordSeparator = false;

  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    endedWithRecordSeparator = false;

    if (inQuotes) {
      if (char === '"') {
        if (text[index + 1] === '"') {
          field += '"';
          index += 1;
        } else {
          inQuotes = false;
        }
      } else {
        field += char;
      }
      continue;
    }

    if (char === '"' && !fieldStarted) {
      inQuotes = true;
      fieldStarted = true;
      continue;
    }

    if (char === "\t") {
      row.push(field);
      field = "";
      fieldStarted = false;
      continue;
    }

    if (char === "\n" || char === "\r") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
      fieldStarted = false;
      endedWithRecordSeparator = true;
      if (char === "\r" && text[index + 1] === "\n") {
        index += 1;
      }
      continue;
    }

    field += char;
    fieldStarted = true;
  }

  if (!endedWithRecordSeparator || fieldStarted || field !== "" || row.length > 0) {
    row.push(field);
    rows.push(row);
  }

  return rows;
}

function parseSelectionCell(addr: string, bounds?: SheetBounds): { column: number; row: number } {
  const parsed = parseCellAddress(addr);
  if (!bounds) {
    return parsed;
  }

  return {
    column: clampInteger(parsed.column, 1, bounds.columns),
    row: clampInteger(parsed.row, 1, bounds.rows)
  };
}

function serializeTsvField(value: string): string {
  if (!/["\t\r\n]/.test(value)) {
    return value;
  }

  return `"${value.replaceAll('"', '""')}"`;
}

function clampInteger(value: number, min: number, max: number): number {
  if (!Number.isInteger(value)) {
    throw new Error(`Invalid integer: ${value}`);
  }

  return Math.min(Math.max(value, min), max);
}

function validatePositiveInteger(value: number, label: string): void {
  if (!Number.isInteger(value) || value < 1) {
    throw new Error(`Invalid ${label}: ${value}`);
  }
}
