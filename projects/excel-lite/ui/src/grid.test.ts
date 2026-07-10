import { describe, expect, it } from "vitest";
import {
  cellAddress,
  clampViewportOrigin,
  clipboardActionForKey,
  columnLabel,
  directionForKey,
  expandSelection,
  expandSelectionToRows,
  isCellInViewport,
  moveCellAddress,
  normalizeSelection,
  parseTsv,
  parseCellAddress,
  serializeTsv,
  undoRedoActionForKey,
  viewportIndexes,
  viewportOriginForCell,
  viewportOriginFromScroll,
  viewportRectFromBounds
} from "./grid";

describe("grid address helpers", () => {
  it("converts one-based column indexes into spreadsheet labels", () => {
    expect(columnLabel(1)).toBe("A");
    expect(columnLabel(26)).toBe("Z");
    expect(columnLabel(27)).toBe("AA");
    expect(columnLabel(703)).toBe("AAA");
  });

  it("round-trips cell addresses", () => {
    expect(cellAddress(28, 12)).toBe("AB12");
    expect(parseCellAddress("ab12")).toEqual({ column: 28, row: 12 });
  });

  it("clamps keyboard navigation to workbook bounds", () => {
    expect(moveCellAddress("A1", "left", 1_048_576, 16_384)).toBe("A1");
    expect(moveCellAddress("A1", "up", 1_048_576, 16_384)).toBe("A1");
    expect(moveCellAddress("J20", "right", 1_048_576, 16_384)).toBe("K20");
    expect(moveCellAddress("K20", "down", 1_048_576, 16_384)).toBe("K21");
    expect(moveCellAddress("XFD1048576", "right", 1_048_576, 16_384)).toBe("XFD1048576");
  });

  it("maps keys to selection directions", () => {
    expect(directionForKey({ key: "Tab", shiftKey: false })).toBe("right");
    expect(directionForKey({ key: "Tab", shiftKey: true })).toBe("left");
    expect(directionForKey({ key: "Enter", shiftKey: false })).toBeNull();
  });

  it("maps command undo and redo shortcuts", () => {
    expect(undoRedoActionForKey({ key: "z", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false })).toBe("undo");
    expect(undoRedoActionForKey({ key: "Z", metaKey: false, ctrlKey: true, shiftKey: false, altKey: false })).toBe("undo");
    expect(undoRedoActionForKey({ key: "z", metaKey: true, ctrlKey: false, shiftKey: true, altKey: false })).toBe("redo");
    expect(undoRedoActionForKey({ key: "z", metaKey: false, ctrlKey: true, shiftKey: true, altKey: false })).toBe("redo");
  });

  it("maps command clipboard shortcuts", () => {
    expect(clipboardActionForKey({ key: "c", metaKey: true, ctrlKey: false, altKey: false })).toBe("copy");
    expect(clipboardActionForKey({ key: "X", metaKey: false, ctrlKey: true, altKey: false })).toBe("cut");
    expect(clipboardActionForKey({ key: "v", metaKey: true, ctrlKey: false, altKey: false })).toBe("paste");
    expect(clipboardActionForKey({ key: "v", metaKey: false, ctrlKey: false, altKey: false })).toBeNull();
    expect(clipboardActionForKey({ key: "v", metaKey: false, ctrlKey: true, altKey: true })).toBeNull();
  });

  it("ignores printable keys without command modifiers", () => {
    expect(undoRedoActionForKey({ key: "a", metaKey: false, ctrlKey: false, shiftKey: false, altKey: false })).toBeNull();
    expect(undoRedoActionForKey({ key: "z", metaKey: false, ctrlKey: false, shiftKey: false, altKey: false })).toBeNull();
    expect(undoRedoActionForKey({ key: "z", metaKey: false, ctrlKey: true, shiftKey: false, altKey: true })).toBeNull();
  });

  it("creates viewport rectangles from the current origin", () => {
    const size = { rows: 20, columns: 10 };
    const bounds = { rows: 1_048_576, columns: 16_384 };

    expect(viewportRectFromBounds({ row: 1, column: 1 }, size, bounds)).toEqual({ start: "A1", end: "J20" });
    expect(viewportRectFromBounds({ row: 2, column: 2 }, size, bounds)).toEqual({ start: "B2", end: "K21" });
    expect(viewportRectFromBounds({ row: 1_048_570, column: 16_380 }, size, bounds)).toEqual({
      start: "XEU1048557",
      end: "XFD1048576"
    });
  });

  it("derives visible row and column indexes from a viewport origin", () => {
    expect(viewportIndexes(21, 20, 1_048_576)).toEqual([
      21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40
    ]);
    expect(viewportIndexes(16_380, 10, 16_384)).toEqual([16_380, 16_381, 16_382, 16_383, 16_384]);
  });

  it("keeps the active cell inside the rendered viewport", () => {
    const size = { rows: 20, columns: 10 };
    const bounds = { rows: 1_048_576, columns: 16_384 };

    expect(viewportOriginForCell("K21", { row: 1, column: 1 }, size, bounds)).toEqual({ row: 2, column: 2 });
    expect(isCellInViewport("K21", { row: 2, column: 2 }, size, bounds)).toBe(true);
    expect(viewportOriginForCell("A1", { row: 2, column: 2 }, size, bounds)).toEqual({ row: 1, column: 1 });
  });

  it("maps deterministic scroll offsets to clamped viewport origins", () => {
    const size = { rows: 20, columns: 10 };
    const bounds = { rows: 1_048_576, columns: 16_384 };

    expect(viewportOriginFromScroll(118, 640, 118, 32, size, bounds)).toEqual({ row: 21, column: 2 });
    expect(viewportOriginFromScroll(9_999_999, 99_999_999, 118, 32, size, bounds)).toEqual({
      row: 1_048_557,
      column: 16_375
    });
  });

  it("clamps viewport origins to the last full window", () => {
    expect(clampViewportOrigin({ row: 99, column: 99 }, { rows: 20, columns: 10 }, { rows: 25, columns: 12 })).toEqual({
      row: 6,
      column: 3
    });
  });

  it("normalizes reversed rectangular selections", () => {
    expect(normalizeSelection({ anchor: "C3", active: "A1" })).toEqual({
      start: "A1",
      end: "C3",
      minColumn: 1,
      maxColumn: 3,
      minRow: 1,
      maxRow: 3,
      rows: 3,
      columns: 3
    });
  });

  it("clamps selections to workbook bounds", () => {
    expect(normalizeSelection({ anchor: "B2", active: "E5" }, { rows: 3, columns: 4 })).toMatchObject({
      start: "B2",
      end: "D3",
      rows: 2,
      columns: 3
    });
  });

  it("expands selections to row-major cell addresses", () => {
    const selection = { anchor: "B2", active: "D3" };
    expect(expandSelectionToRows(selection)).toEqual([
      ["B2", "C2", "D2"],
      ["B3", "C3", "D3"]
    ]);
    expect(expandSelection(selection)).toEqual(["B2", "C2", "D2", "B3", "C3", "D3"]);
  });

  it("serializes rectangular raw values as TSV", () => {
    expect(
      serializeTsv([
        ["=A1+B1", "plain"],
        ["", "tail"]
      ])
    ).toBe("=A1+B1\tplain\n\ttail");
  });

  it("round-trips TSV fields containing tabs, newlines, and quotes", () => {
    const rows = [["a\tb", "line\nbreak", 'say "yes"']];
    const tsv = serializeTsv(rows);

    expect(tsv).toBe('"a\tb"\t"line\nbreak"\t"say ""yes"""');
    expect(parseTsv(tsv)).toEqual(rows);
  });

  it("preserves blank cells and trailing empty cells when parsing TSV", () => {
    expect(parseTsv("\t\nvalue\t")).toEqual([
      ["", ""],
      ["value", ""]
    ]);
    expect(parseTsv("")).toEqual([[""]]);
  });

  it("parses CRLF TSV without adding a row for a terminal newline", () => {
    expect(parseTsv("A\tB\r\nC\tD\r\n")).toEqual([
      ["A", "B"],
      ["C", "D"]
    ]);
  });
});
