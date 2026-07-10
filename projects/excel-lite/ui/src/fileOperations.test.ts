import { beforeEach, describe, expect, it, vi } from "vitest";
import type { XliteCommands } from "./commands";
import {
  runWorkbookFileOperation,
  selectFilePath,
  type FileDialogSelection,
  type WorkbookFileOperation
} from "./fileOperations";

const openMock = vi.hoisted(() => vi.fn());
const saveMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openMock,
  save: saveMock
}));

describe("file operation dialogs", () => {
  beforeEach(() => {
    openMock.mockReset();
    saveMock.mockReset();
    vi.unstubAllGlobals();
  });

  it("reports file dialogs as unavailable in browser preview", async () => {
    await expect(selectFilePath("openWorkbook")).resolves.toEqual({ kind: "unavailable" });

    expect(openMock).not.toHaveBeenCalled();
    expect(saveMock).not.toHaveBeenCalled();
  });

  it("opens native file dialogs with operation-specific filters", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    openMock.mockResolvedValueOnce("/tmp/book.xlite");
    openMock.mockResolvedValueOnce("/tmp/data.csv");
    openMock.mockResolvedValueOnce("/tmp/data.xlsx");

    await expect(selectFilePath("openWorkbook")).resolves.toEqual({
      kind: "selected",
      path: "/tmp/book.xlite"
    });
    await expect(selectFilePath("importCsv")).resolves.toEqual({ kind: "selected", path: "/tmp/data.csv" });
    await expect(selectFilePath("importXlsx")).resolves.toEqual({ kind: "selected", path: "/tmp/data.xlsx" });

    expect(openMock).toHaveBeenNthCalledWith(1, {
      title: "Open Excel Lite workbook",
      filters: [{ name: "Excel Lite workbook", extensions: ["xlite"] }],
      multiple: false
    });
    expect(openMock).toHaveBeenNthCalledWith(2, {
      title: "Import CSV",
      filters: [{ name: "CSV", extensions: ["csv"] }],
      multiple: false
    });
    expect(openMock).toHaveBeenNthCalledWith(3, {
      title: "Import XLSX",
      filters: [{ name: "Excel workbook", extensions: ["xlsx"] }],
      multiple: false
    });
  });

  it("opens native save dialogs with operation-specific filters and default filenames", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    saveMock.mockResolvedValueOnce("/tmp/book.xlite");
    saveMock.mockResolvedValueOnce("/tmp/sheet.csv");

    await expect(selectFilePath("saveWorkbook")).resolves.toEqual({
      kind: "selected",
      path: "/tmp/book.xlite"
    });
    await expect(selectFilePath("exportCsv")).resolves.toEqual({ kind: "selected", path: "/tmp/sheet.csv" });

    expect(saveMock).toHaveBeenNthCalledWith(1, {
      title: "Save Excel Lite workbook",
      filters: [{ name: "Excel Lite workbook", extensions: ["xlite"] }],
      defaultPath: "Book1.xlite"
    });
    expect(saveMock).toHaveBeenNthCalledWith(2, {
      title: "Export CSV",
      filters: [{ name: "CSV", extensions: ["csv"] }],
      defaultPath: "Sheet1.csv"
    });
  });
});

describe("file operation runner", () => {
  it("does not invoke workbook commands when a dialog is cancelled", async () => {
    const commands = commandSpies();

    await expect(runWorkbookFileOperation(commands, "openWorkbook", cancelledSelection)).resolves.toMatchObject({
      kind: "cancelled",
      message: "Open workbook cancelled"
    });

    expectNoFileCommands(commands);
  });

  it("does not invoke workbook commands when dialogs are unavailable", async () => {
    const commands = commandSpies();

    await expect(runWorkbookFileOperation(commands, "saveWorkbook", unavailableSelection)).resolves.toMatchObject({
      kind: "unavailable",
      message: "File dialogs are unavailable in browser preview"
    });

    expectNoFileCommands(commands);
  });

  it("invokes the selected workbook command and returns loaded or written results", async () => {
    const commands = commandSpies();

    await expect(runWorkbookFileOperation(commands, "openWorkbook", selectedSelection("/tmp/book.xlite"))).resolves.toMatchObject({
      kind: "loaded",
      message: "Opened book.xlite"
    });
    await expect(runWorkbookFileOperation(commands, "saveWorkbook", selectedSelection("/tmp/book.xlite"))).resolves.toMatchObject({
      kind: "written",
      message: "Saved book.xlite"
    });
    await expect(runWorkbookFileOperation(commands, "importCsv", selectedSelection("/tmp/data.csv"))).resolves.toMatchObject({
      kind: "loaded",
      message: "Imported CSV data.csv"
    });
    await expect(runWorkbookFileOperation(commands, "importXlsx", selectedSelection("/tmp/data.xlsx"))).resolves.toMatchObject({
      kind: "loaded",
      message: "Imported XLSX data.xlsx"
    });
    await expect(runWorkbookFileOperation(commands, "exportCsv", selectedSelection("/tmp/sheet.csv"))).resolves.toMatchObject({
      kind: "written",
      message: "Exported CSV sheet.csv"
    });

    expect(commands.openWorkbook).toHaveBeenCalledWith("/tmp/book.xlite");
    expect(commands.saveWorkbook).toHaveBeenCalledWith("/tmp/book.xlite");
    expect(commands.importCsv).toHaveBeenCalledWith("/tmp/data.csv");
    expect(commands.importXlsx).toHaveBeenCalledWith("/tmp/data.xlsx");
    expect(commands.exportCsv).toHaveBeenCalledWith("/tmp/sheet.csv");
  });
});

function selectedSelection(path: string): () => Promise<FileDialogSelection> {
  return async () => ({ kind: "selected", path });
}

async function cancelledSelection(_operation: WorkbookFileOperation): Promise<FileDialogSelection> {
  return { kind: "cancelled" };
}

async function unavailableSelection(_operation: WorkbookFileOperation): Promise<FileDialogSelection> {
  return { kind: "unavailable" };
}

function commandSpies(): XliteCommands {
  return {
    newWorkbook: vi.fn(),
    openWorkbook: vi.fn().mockResolvedValue(workbookMeta()),
    saveWorkbook: vi.fn().mockResolvedValue(undefined),
    importCsv: vi.fn().mockResolvedValue(workbookMeta()),
    importXlsx: vi.fn().mockResolvedValue(workbookMeta()),
    exportCsv: vi.fn().mockResolvedValue(undefined),
    setCell: vi.fn(),
    insertRows: vi.fn(),
    deleteRows: vi.fn(),
    insertCols: vi.fn(),
    deleteCols: vi.fn(),
    setFormat: vi.fn(),
    resizeColumn: vi.fn(),
    resizeRow: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
    getCell: vi.fn(),
    getViewport: vi.fn()
  };
}

function expectNoFileCommands(commands: XliteCommands): void {
  expect(commands.openWorkbook).not.toHaveBeenCalled();
  expect(commands.saveWorkbook).not.toHaveBeenCalled();
  expect(commands.importCsv).not.toHaveBeenCalled();
  expect(commands.importXlsx).not.toHaveBeenCalled();
  expect(commands.exportCsv).not.toHaveBeenCalled();
}

function workbookMeta() {
  return {
    workbookId: "book",
    activeSheet: 0,
    rows: 10,
    cols: 10
  };
}
