import { open, save } from "@tauri-apps/plugin-dialog";
import type { WorkbookMeta, XliteCommands } from "./commands";

export type WorkbookFileOperation = "openWorkbook" | "saveWorkbook" | "importCsv" | "importXlsx" | "exportCsv";

type DialogMode = "open" | "save";

interface FileOperationSpec {
  mode: DialogMode;
  title: string;
  label: string;
  completeLabel: string;
  filters: Array<{
    name: string;
    extensions: string[];
  }>;
  defaultPath?: string;
}

export type FileDialogSelection =
  | { kind: "selected"; path: string }
  | { kind: "cancelled" }
  | { kind: "unavailable" };

export type FileOperationResult =
  | {
      kind: "cancelled" | "unavailable";
      operation: WorkbookFileOperation;
      message: string;
    }
  | {
      kind: "loaded";
      operation: "openWorkbook" | "importCsv" | "importXlsx";
      path: string;
      meta: WorkbookMeta;
      message: string;
    }
  | {
      kind: "written";
      operation: "saveWorkbook" | "exportCsv";
      path: string;
      message: string;
    };

export type SelectFilePath = (operation: WorkbookFileOperation) => Promise<FileDialogSelection>;

const fileOperationSpecs: Record<WorkbookFileOperation, FileOperationSpec> = {
  openWorkbook: {
    mode: "open",
    title: "Open Excel Lite workbook",
    label: "Open workbook",
    completeLabel: "Opened",
    filters: [{ name: "Excel Lite workbook", extensions: ["xlite"] }]
  },
  saveWorkbook: {
    mode: "save",
    title: "Save Excel Lite workbook",
    label: "Save workbook",
    completeLabel: "Saved",
    defaultPath: "Book1.xlite",
    filters: [{ name: "Excel Lite workbook", extensions: ["xlite"] }]
  },
  importCsv: {
    mode: "open",
    title: "Import CSV",
    label: "Import CSV",
    completeLabel: "Imported CSV",
    filters: [{ name: "CSV", extensions: ["csv"] }]
  },
  importXlsx: {
    mode: "open",
    title: "Import XLSX",
    label: "Import XLSX",
    completeLabel: "Imported XLSX",
    filters: [{ name: "Excel workbook", extensions: ["xlsx"] }]
  },
  exportCsv: {
    mode: "save",
    title: "Export CSV",
    label: "Export CSV",
    completeLabel: "Exported CSV",
    defaultPath: "Sheet1.csv",
    filters: [{ name: "CSV", extensions: ["csv"] }]
  }
};

export function fileOperationLabel(operation: WorkbookFileOperation): string {
  return fileOperationSpecs[operation].label;
}

export async function selectFilePath(operation: WorkbookFileOperation): Promise<FileDialogSelection> {
  if (!isTauriRuntime()) {
    return { kind: "unavailable" };
  }

  const spec = fileOperationSpecs[operation];
  const selected =
    spec.mode === "open"
      ? await open({
          title: spec.title,
          filters: spec.filters,
          multiple: false
        })
      : await save({
          title: spec.title,
          filters: spec.filters,
          defaultPath: spec.defaultPath
        });
  const path = selectedPath(selected);

  return path ? { kind: "selected", path } : { kind: "cancelled" };
}

export async function runWorkbookFileOperation(
  commands: XliteCommands,
  operation: WorkbookFileOperation,
  selectPath: SelectFilePath = selectFilePath
): Promise<FileOperationResult> {
  const selection = await selectPath(operation);
  if (selection.kind === "cancelled") {
    return {
      kind: "cancelled",
      operation,
      message: `${fileOperationLabel(operation)} cancelled`
    };
  }
  if (selection.kind === "unavailable") {
    return {
      kind: "unavailable",
      operation,
      message: "File dialogs are unavailable in browser preview"
    };
  }

  const path = selection.path;
  switch (operation) {
    case "openWorkbook":
      return loadedResult(operation, path, await commands.openWorkbook(path));
    case "importCsv":
      return loadedResult(operation, path, await commands.importCsv(path));
    case "importXlsx":
      return loadedResult(operation, path, await commands.importXlsx(path));
    case "saveWorkbook":
      await commands.saveWorkbook(path);
      return writtenResult(operation, path);
    case "exportCsv":
      await commands.exportCsv(path);
      return writtenResult(operation, path);
  }
}

function loadedResult(
  operation: "openWorkbook" | "importCsv" | "importXlsx",
  path: string,
  meta: WorkbookMeta
): FileOperationResult {
  return {
    kind: "loaded",
    operation,
    path,
    meta,
    message: `${fileOperationSpecs[operation].completeLabel} ${fileNameFromPath(path)}`
  };
}

function writtenResult(operation: "saveWorkbook" | "exportCsv", path: string): FileOperationResult {
  return {
    kind: "written",
    operation,
    path,
    message: `${fileOperationSpecs[operation].completeLabel} ${fileNameFromPath(path)}`
  };
}

function selectedPath(selected: string | string[] | null): string | null {
  if (Array.isArray(selected)) {
    return selected[0] ?? null;
  }

  return selected;
}

function fileNameFromPath(path: string): string {
  return path.split(/[/\\]/).at(-1) || path;
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}
