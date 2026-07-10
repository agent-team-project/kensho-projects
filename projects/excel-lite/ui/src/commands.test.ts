import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CellFormatPayload } from "./commands";
import { createCommandClient } from "./commands";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock
}));

describe("command clients", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    vi.unstubAllGlobals();
  });

  it("invokes Tauri undo and redo without payloads", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce(null).mockResolvedValueOnce(null);

    const client = createCommandClient();
    await client.undo();
    await client.redo();

    expect(invokeMock).toHaveBeenNthCalledWith(1, "undo");
    expect(invokeMock).toHaveBeenNthCalledWith(2, "redo");
  });

  it("invokes Tauri file commands with paths", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValue({ workbookId: "book", activeSheet: 0, rows: 10, cols: 10 });

    const client = createCommandClient();
    await client.openWorkbook("/tmp/book.xlite");
    await client.saveWorkbook("/tmp/book.xlite");
    await client.importCsv("/tmp/data.csv");
    await client.importXlsx("/tmp/data.xlsx");
    await client.exportCsv("/tmp/sheet.csv");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "open_workbook", { path: "/tmp/book.xlite" });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "save_workbook", { path: "/tmp/book.xlite" });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "import_csv", { path: "/tmp/data.csv" });
    expect(invokeMock).toHaveBeenNthCalledWith(4, "import_xlsx", { path: "/tmp/data.xlsx" });
    expect(invokeMock).toHaveBeenNthCalledWith(5, "export_csv", { path: "/tmp/sheet.csv" });
  });

  it("invokes Tauri structural commands with sheet positions and counts", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValue({ changed: [], circular: [] });

    const client = createCommandClient();
    await client.insertRows(2, 3, 4);
    await client.deleteRows(2, 5, 6);
    await client.insertCols(2, 7, 8);
    await client.deleteCols(2, 9, 10);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "insert_rows", { sheet: 2, at: 3, count: 4 });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "delete_rows", { sheet: 2, at: 5, count: 6 });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "insert_cols", { sheet: 2, at: 7, count: 8 });
    expect(invokeMock).toHaveBeenNthCalledWith(4, "delete_cols", { sheet: 2, at: 9, count: 10 });
  });

  it("invokes Tauri format and resize commands with exact payloads", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValue({ changed: [], circular: [] });

    const client = createCommandClient();
    const format: CellFormatPayload = {
      number: { type: "currency", dp: 2 },
      align: "right",
      bold: true
    };

    await client.setFormat(1, { start: "B2", end: "C3" }, format);
    await client.resizeColumn(1, 4, 144);
    await client.resizeRow(1, 5, 40);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "set_format", {
      sheet: 1,
      rect: { start: "B2", end: "C3" },
      format
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "resize_column", { sheet: 1, col: 4, width: 144 });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "resize_row", { sheet: 1, row: 5, height: 40 });
  });

  it("returns null for empty browser-preview undo and redo history", async () => {
    const client = createCommandClient();
    await client.newWorkbook();

    await expect(client.undo()).resolves.toBeNull();
    await expect(client.redo()).resolves.toBeNull();
  });

  it("restores browser-preview raw values through undo and redo", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "old");
    await client.setCell(0, "A1", "new");

    const undo = await client.undo();
    expect(undo?.changed).toMatchObject([{ addr: "A1", raw: "old", display: "old" }]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "old", display: "old" });

    const redo = await client.redo();
    expect(redo?.changed).toMatchObject([{ addr: "A1", raw: "new", display: "new" }]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "new", display: "new" });
  });

  it("shifts browser-preview raw values for row inserts and deletes", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "one");
    await client.setCell(0, "A2", "two");

    const insert = await client.insertRows(0, 0, 1);
    expect(insert.changed).toMatchObject([
      { addr: "A1", raw: "" },
      { addr: "A2", raw: "one" },
      { addr: "A3", raw: "two" }
    ]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "" });
    await expect(client.getCell(0, "A2")).resolves.toMatchObject({ raw: "one" });
    await expect(client.getCell(0, "A3")).resolves.toMatchObject({ raw: "two" });

    const deletion = await client.deleteRows(0, 1, 1);
    expect(deletion.changed).toMatchObject([
      { addr: "A2", raw: "two" },
      { addr: "A3", raw: "" }
    ]);
    await expect(client.getCell(0, "A2")).resolves.toMatchObject({ raw: "two" });
    await expect(client.getCell(0, "A3")).resolves.toMatchObject({ raw: "" });
  });

  it("shifts browser-preview raw values for column inserts and deletes", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "left");
    await client.setCell(0, "B1", "right");

    const insert = await client.insertCols(0, 0, 1);
    expect(insert.changed).toMatchObject([
      { addr: "A1", raw: "" },
      { addr: "B1", raw: "left" },
      { addr: "C1", raw: "right" }
    ]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "" });
    await expect(client.getCell(0, "B1")).resolves.toMatchObject({ raw: "left" });
    await expect(client.getCell(0, "C1")).resolves.toMatchObject({ raw: "right" });

    const deletion = await client.deleteCols(0, 1, 1);
    expect(deletion.changed).toMatchObject([
      { addr: "B1", raw: "right" },
      { addr: "C1", raw: "" }
    ]);
    await expect(client.getCell(0, "B1")).resolves.toMatchObject({ raw: "right" });
    await expect(client.getCell(0, "C1")).resolves.toMatchObject({ raw: "" });
  });

  it("includes browser-preview structural edits in undo and redo history", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "value");
    await client.insertRows(0, 0, 1);

    const undo = await client.undo();
    expect(undo?.changed).toMatchObject([
      { addr: "A1", raw: "value" },
      { addr: "A2", raw: "" }
    ]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "value" });
    await expect(client.getCell(0, "A2")).resolves.toMatchObject({ raw: "" });

    const redo = await client.redo();
    expect(redo?.changed).toMatchObject([
      { addr: "A1", raw: "" },
      { addr: "A2", raw: "value" }
    ]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "" });
    await expect(client.getCell(0, "A2")).resolves.toMatchObject({ raw: "value" });
  });

  it("formats browser-preview numeric displays while preserving raw values", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "12.345");

    const fixed = await client.setFormat(0, { start: "A1", end: "A1" }, numberFormat("fixed", 2));
    expect(fixed.changed).toMatchObject([{ addr: "A1", raw: "12.345", display: "12.35" }]);
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "12.345", display: "12.35" });

    const percent = await client.setFormat(0, { start: "A1", end: "A1" }, numberFormat("percent", 1));
    expect(percent.changed).toMatchObject([{ addr: "A1", raw: "12.345", display: "1234.5%" }]);

    const currency = await client.setFormat(0, { start: "A1", end: "A1" }, numberFormat("currency", 2));
    expect(currency.changed).toMatchObject([{ addr: "A1", raw: "12.345", display: "$12.35" }]);

    const general = await client.setFormat(0, { start: "A1", end: "A1" }, numberFormat("general"));
    expect(general.changed).toMatchObject([{ addr: "A1", raw: "12.345", display: "12.345" }]);

    await client.setCell(0, "B1", "12");
    const text = await client.setFormat(0, { start: "B1", end: "B1" }, numberFormat("text"));
    expect(text.changed).toMatchObject([{ addr: "B1", raw: "12", display: "12.0" }]);
    await expect(client.getCell(0, "B1")).resolves.toMatchObject({ raw: "12", display: "12.0" });
  });

  it("keeps browser-preview format edits in undo and redo history and clears redo after new format edits", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "1.234");
    await client.setFormat(0, { start: "A1", end: "A1" }, {
      number: { type: "fixed", dp: 1 },
      align: "center",
      bold: true
    });

    const undo = await client.undo();
    expect(undo?.changed).toMatchObject([
      {
        addr: "A1",
        raw: "1.234",
        display: "1.234",
        format: { number: { type: "general" }, align: "default", bold: false }
      }
    ]);

    const redo = await client.redo();
    expect(redo?.changed).toMatchObject([
      {
        addr: "A1",
        raw: "1.234",
        display: "1.2",
        format: { number: { type: "fixed", dp: 1 }, align: "center", bold: true }
      }
    ]);

    await client.undo();
    await client.setFormat(0, { start: "A1", end: "A1" }, numberFormat("percent", 0));

    await expect(client.redo()).resolves.toBeNull();
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "1.234", display: "123%" });
  });

  it("keeps browser-preview resize actions in undo and redo history without changing raw values", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "kept");

    await expect(client.resizeColumn(0, 0, 134)).resolves.toEqual({ changed: [], circular: [] });
    await expect(client.resizeRow(0, 0, 40)).resolves.toEqual({ changed: [], circular: [] });
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "kept" });

    await expect(client.undo()).resolves.toEqual({ changed: [], circular: [] });
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "kept" });
    await expect(client.undo()).resolves.toEqual({ changed: [], circular: [] });
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "kept" });

    await expect(client.redo()).resolves.toEqual({ changed: [], circular: [] });
    await expect(client.getCell(0, "A1")).resolves.toMatchObject({ raw: "kept" });
  });

  it("clears browser-preview redo history after a new structural edit", async () => {
    const client = createCommandClient();
    await client.newWorkbook();
    await client.setCell(0, "A1", "value");
    await client.insertRows(0, 0, 1);
    await client.undo();

    await client.insertCols(0, 0, 1);

    await expect(client.redo()).resolves.toBeNull();
    await expect(client.getCell(0, "B1")).resolves.toMatchObject({ raw: "value" });
  });
});

function numberFormat(type: CellFormatPayload["number"]["type"], dp?: number): CellFormatPayload {
  return {
    number: dp === undefined ? { type } : { type, dp },
    align: "default",
    bold: false
  };
}
