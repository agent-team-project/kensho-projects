<script lang="ts">
  import {
    AlignCenter,
    AlignLeft,
    AlignRight,
    Bold,
    Columns4,
    DollarSign,
    FileInput,
    FileOutput,
    FilePlus2,
    FileSpreadsheet,
    FolderOpen,
    Hash,
    Minus,
    Percent,
    Plus,
    Redo2,
    Rows4,
    Save,
    Type,
    Undo2
  } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import type { CellFormatPayload, CellSnapshot, FormatAlign, RecalcDelta, WorkbookMeta } from "./commands";
  import { createCommandClient } from "./commands";
  import {
    fileOperationLabel,
    runWorkbookFileOperation,
    type WorkbookFileOperation
  } from "./fileOperations";
  import type { CellSelection, ClipboardAction, Direction, UndoRedoAction } from "./grid";
  import {
    blankSnapshot,
    cellAddress,
    clampViewportOrigin,
    clipboardActionForKey,
    columnLabel,
    directionForKey,
    expandSelection,
    expandSelectionToRows,
    isCellInSelection,
    isCellInViewport,
    isPrintableEditKey,
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
  import type { SheetBounds, ViewportOrigin, ViewportSize } from "./grid";

  const sheet = 0;
  const visibleRows = 20;
  const visibleColumns = 10;
  const viewportSize: ViewportSize = { rows: visibleRows, columns: visibleColumns };
  const defaultWorkbookBounds: SheetBounds = { rows: 1_048_576, columns: 16_384 };
  const commands = createCommandClient();
  type StructuralAction = "insertRow" | "deleteRow" | "insertColumn" | "deleteColumn";
  type NumberFormatAction = "general" | "fixed" | "percent" | "currency" | "text";
  type ResizeDirection = "increase" | "decrease";
  type ClipboardSource = "system" | "internal";
  type LocalUiState = {
    cellFormats: Record<string, CellFormatPayload>;
    columnWidths: Record<number, number>;
    rowHeights: Record<number, number>;
  };
  type LocalUiHistoryEntry = {
    before: LocalUiState;
    after: LocalUiState;
  };
  type CellWrite = {
    addr: string;
    raw: string;
  };
  const defaultColumnWidth = 118;
  const defaultRowHeight = 32;
  const columnResizeStep = 16;
  const rowResizeStep = 8;
  const minColumnWidth = 48;
  const minRowHeight = 20;
  const defaultCellFormat: CellFormatPayload = {
    number: { type: "general" },
    align: "default",
    bold: false
  };

  let activeCell = "A1";
  let selectionAnchor = "A1";
  let workbookBounds = { ...defaultWorkbookBounds };
  let viewportOrigin: ViewportOrigin = { row: 1, column: 1 };
  let viewportScrollTop = 0;
  let viewportScrollLeft = 0;
  let viewportRequestId = 0;
  let editAddr: string | null = null;
  let editValue = "";
  let formulaValue = "";
  let viewportCache: Record<string, CellSnapshot> = {};
  let cellFormats: Record<string, CellFormatPayload> = {};
  let columnWidths: Record<number, number> = {};
  let rowHeights: Record<number, number> = {};
  let localUndoStack: LocalUiHistoryEntry[] = [];
  let localRedoStack: LocalUiHistoryEntry[] = [];
  let internalClipboardText = "";
  let statusText = "Ready";
  let sheetViewportElement: HTMLElement | undefined;
  let gridElement: HTMLTableElement | undefined;
  let editInput: HTMLInputElement | undefined;
  let formulaInput: HTMLInputElement | undefined;

  $: rows = viewportIndexes(viewportOrigin.row, visibleRows, workbookBounds.rows);
  $: columns = viewportIndexes(viewportOrigin.column, visibleColumns, workbookBounds.columns);
  $: virtualCanvasStyle = `width: ${workbookBounds.columns * defaultColumnWidth}px; height: ${
    workbookBounds.rows * defaultRowHeight
  }px;`;
  $: gridPositionStyle = `top: ${viewportScrollTop}px; left: ${viewportScrollLeft}px;`;
  $: activeSnapshot = viewportCache[activeCell] ?? blankSnapshot(activeCell);
  $: activeFormat = cloneFormat(cellFormats[activeCell] ?? defaultCellFormat);
  $: selectedRange = normalizeSelection({ anchor: selectionAnchor, active: activeCell }, workbookBounds);

  function snapshotFor(addr: string): CellSnapshot {
    return viewportCache[addr] ?? blankSnapshot(addr);
  }

  function currentSelection(): CellSelection {
    return {
      anchor: selectionAnchor,
      active: activeCell
    };
  }

  function selectionLabel(selection: CellSelection = currentSelection()): string {
    const range = normalizeSelection(selection, workbookBounds);
    return range.start === range.end ? range.start : `${range.start}:${range.end}`;
  }

  function clampAddressToWorkbook(addr: string): string {
    const parsed = parseCellAddress(addr);
    return cellAddress(
      Math.min(Math.max(parsed.column, 1), workbookBounds.columns),
      Math.min(Math.max(parsed.row, 1), workbookBounds.rows)
    );
  }

  function isSelected(addr: string): boolean {
    return isCellInSelection(addr, currentSelection(), workbookBounds);
  }

  function cloneFormat(format: CellFormatPayload): CellFormatPayload {
    return {
      number: { ...format.number },
      align: format.align,
      bold: format.bold
    };
  }

  function isDefaultFormat(format: CellFormatPayload): boolean {
    return (
      format.number.type === "general" &&
      format.number.dp === undefined &&
      format.align === "default" &&
      !format.bold
    );
  }

  function cloneFormatRecord(formats: Record<string, CellFormatPayload>): Record<string, CellFormatPayload> {
    return Object.fromEntries(Object.entries(formats).map(([addr, format]) => [addr, cloneFormat(format)]));
  }

  function captureLocalUiState(): LocalUiState {
    return {
      cellFormats: cloneFormatRecord(cellFormats),
      columnWidths: { ...columnWidths },
      rowHeights: { ...rowHeights }
    };
  }

  function restoreLocalUiState(state: LocalUiState): void {
    cellFormats = cloneFormatRecord(state.cellFormats);
    columnWidths = { ...state.columnWidths };
    rowHeights = { ...state.rowHeights };
  }

  function pushLocalUiHistory(before: LocalUiState): void {
    localUndoStack = [...localUndoStack, { before, after: captureLocalUiState() }];
    localRedoStack = [];
  }

  function pushLocalNoopHistory(): void {
    const state = captureLocalUiState();
    localUndoStack = [...localUndoStack, { before: state, after: state }];
    localRedoStack = [];
  }

  function applyLocalUndoRedo(kind: UndoRedoAction): void {
    const source = kind === "undo" ? localUndoStack : localRedoStack;
    const entry = source.at(-1);
    if (!entry) return;

    if (kind === "undo") {
      localUndoStack = localUndoStack.slice(0, -1);
      localRedoStack = [...localRedoStack, entry];
      restoreLocalUiState(entry.before);
    } else {
      localRedoStack = localRedoStack.slice(0, -1);
      localUndoStack = [...localUndoStack, entry];
      restoreLocalUiState(entry.after);
    }
  }

  function setCellFormat(addr: string, format: CellFormatPayload): void {
    const next = { ...cellFormats };
    if (isDefaultFormat(format)) {
      delete next[addr];
    } else {
      next[addr] = cloneFormat(format);
    }
    cellFormats = next;
  }

  function numberFormatForAction(action: NumberFormatAction): CellFormatPayload["number"] {
    if (action === "fixed") return { type: "fixed", dp: 2 };
    if (action === "percent") return { type: "percent", dp: 1 };
    if (action === "currency") return { type: "currency", dp: 2 };
    return { type: action };
  }

  function columnWidth(column: number): number {
    return columnWidths[column] ?? defaultColumnWidth;
  }

  function rowHeight(row: number): number {
    return rowHeights[row] ?? defaultRowHeight;
  }

  function columnSizeStyle(width: number): string {
    return `width: ${width}px; min-width: ${width}px; max-width: ${width}px;`;
  }

  function rowHeaderStyle(height: number): string {
    return `height: ${height}px;`;
  }

  function cellSizeStyle(width: number, height: number): string {
    return `${columnSizeStyle(width)} height: ${height}px; line-height: ${Math.max(16, height - 1)}px;`;
  }

  function workbookBoundsFromMeta(meta: WorkbookMeta): SheetBounds {
    return {
      rows: Math.max(1, Math.trunc(meta.rows)),
      columns: Math.max(1, Math.trunc(meta.cols))
    };
  }

  function sameViewportOrigin(left: ViewportOrigin, right: ViewportOrigin): boolean {
    return left.row === right.row && left.column === right.column;
  }

  function setViewportOrigin(origin: ViewportOrigin, syncScroll: boolean): boolean {
    const next = clampViewportOrigin(origin, viewportSize, workbookBounds);
    if (sameViewportOrigin(next, viewportOrigin)) {
      return false;
    }

    viewportOrigin = next;
    if (syncScroll) {
      syncViewportScrollToOrigin(next);
    }
    return true;
  }

  function syncViewportScrollToOrigin(origin: ViewportOrigin): void {
    const left = (origin.column - 1) * defaultColumnWidth;
    const top = (origin.row - 1) * defaultRowHeight;
    viewportScrollLeft = left;
    viewportScrollTop = top;
    sheetViewportElement?.scrollTo({ left, top });
  }

  async function ensureCellVisible(addr: string): Promise<void> {
    const next = viewportOriginForCell(addr, viewportOrigin, viewportSize, workbookBounds);
    if (setViewportOrigin(next, true)) {
      await refreshViewport();
    }
  }

  function handleViewportScroll(event: Event): void {
    const target = event.currentTarget as HTMLElement;
    viewportScrollLeft = target.scrollLeft;
    viewportScrollTop = target.scrollTop;

    const next = viewportOriginFromScroll(
      target.scrollLeft,
      target.scrollTop,
      defaultColumnWidth,
      defaultRowHeight,
      viewportSize,
      workbookBounds
    );
    if (setViewportOrigin(next, false)) {
      void refreshViewport();
    }
  }

  function setFormulaFromActive(): void {
    formulaValue = snapshotFor(activeCell).raw;
  }

  function isVisible(addr: string): boolean {
    return isCellInViewport(addr, viewportOrigin, viewportSize, workbookBounds);
  }

  function patchViewport(delta: RecalcDelta): void {
    const next = { ...viewportCache };
    for (const snapshot of delta.changed) {
      if (isVisible(snapshot.addr)) {
        next[snapshot.addr] = snapshot;
      }
    }
    viewportCache = next;
  }

  async function applyDelta(delta: RecalcDelta, moveAfter?: Direction): Promise<void> {
    patchViewport(delta);
    if (moveAfter) {
      await moveSelection(moveAfter);
    } else {
      await refreshViewport();
    }
  }

  async function refreshViewport(): Promise<void> {
    const requestId = (viewportRequestId += 1);
    const activeAddr = activeCell;
    const snapshots = await commands.getViewport(sheet, viewportRectFromBounds(viewportOrigin, viewportSize, workbookBounds));
    const nextCache = Object.fromEntries(snapshots.map((snapshot) => [snapshot.addr, snapshot]));

    if (!nextCache[activeAddr]) {
      const active = await commands.getCell(sheet, activeAddr);
      nextCache[active.addr] = active;
    }

    if (requestId !== viewportRequestId) {
      return;
    }

    viewportCache = nextCache;
    setFormulaFromActive();
  }

  async function resetWorkbook(): Promise<void> {
    statusText = "Creating workbook";
    const meta = await commands.newWorkbook();
    workbookBounds = workbookBoundsFromMeta(meta);
    viewportOrigin = { row: 1, column: 1 };
    syncViewportScrollToOrigin(viewportOrigin);
    activeCell = "A1";
    selectionAnchor = "A1";
    editAddr = null;
    cellFormats = {};
    columnWidths = {};
    rowHeights = {};
    localUndoStack = [];
    localRedoStack = [];
    await refreshViewport();
    statusText = "Ready";
  }

  function resetWorkbookMirroredUiState(): void {
    editAddr = null;
    editValue = "";
    cellFormats = {};
    columnWidths = {};
    rowHeights = {};
    localUndoStack = [];
    localRedoStack = [];
    clampSelectionToWorkbook();
  }

  async function runFileOperation(operation: WorkbookFileOperation): Promise<void> {
    if (editAddr) return;

    const label = fileOperationLabel(operation);
    statusText = `${label} selecting file`;
    try {
      const result = await runWorkbookFileOperation(commands, operation);
      if (result.kind === "loaded") {
        workbookBounds = workbookBoundsFromMeta(result.meta);
        resetWorkbookMirroredUiState();
        await ensureCellVisible(activeCell);
        await refreshViewport();
      }
      statusText = result.message;
    } catch (error) {
      statusText = `${label} failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  function setActiveCell(addr: string, extendSelection: boolean): void {
    activeCell = clampAddressToWorkbook(addr);
    if (!extendSelection) {
      selectionAnchor = activeCell;
    }
    editAddr = null;
    setFormulaFromActive();
  }

  function selectCell(addr: string, extendSelection = false): void {
    setActiveCell(addr, extendSelection);
    gridElement?.focus();
  }

  async function moveSelection(direction: "up" | "down" | "left" | "right", extendSelection = false): Promise<void> {
    setActiveCell(moveCellAddress(activeCell, direction, workbookBounds.rows, workbookBounds.columns), extendSelection);
    await ensureCellVisible(activeCell);
    setFormulaFromActive();
  }

  function clampSelectionToWorkbook(): void {
    activeCell = clampAddressToWorkbook(activeCell);
    selectionAnchor = clampAddressToWorkbook(selectionAnchor);
    editAddr = null;
    setViewportOrigin(viewportOriginForCell(activeCell, viewportOrigin, viewportSize, workbookBounds), true);
  }

  async function startEdit(initialValue: string): Promise<void> {
    editAddr = activeCell;
    editValue = initialValue;
    await tick();
    editInput?.focus();
    editInput?.select();
  }

  function cancelEdit(): void {
    editAddr = null;
    editValue = "";
    setFormulaFromActive();
  }

  async function commitActiveCell(raw: string, moveAfter?: "up" | "down" | "left" | "right"): Promise<void> {
    const committedAddr = activeCell;
    editAddr = null;
    statusText = `Committing ${committedAddr}`;
    const delta = await commands.setCell(sheet, committedAddr, raw);
    pushLocalNoopHistory();
    await applyDelta(delta, moveAfter);
    statusText = delta.circular.length > 0 ? `Circular reference: ${delta.circular.join(", ")}` : `${committedAddr} committed`;
  }

  async function commitFormulaBar(): Promise<void> {
    await commitActiveCell(formulaValue);
    formulaInput?.focus();
  }

  function clipboardSourceLabel(source: ClipboardSource): string {
    return source === "system" ? "system clipboard" : "internal clipboard";
  }

  async function writeClipboardText(text: string): Promise<ClipboardSource> {
    internalClipboardText = text;
    if (navigator.clipboard?.writeText) {
      try {
        await navigator.clipboard.writeText(text);
        return "system";
      } catch {
        return "internal";
      }
    }

    return "internal";
  }

  async function readClipboardText(): Promise<{ text: string; source: ClipboardSource }> {
    if (navigator.clipboard?.readText) {
      try {
        return {
          text: await navigator.clipboard.readText(),
          source: "system"
        };
      } catch {
        return {
          text: internalClipboardText,
          source: "internal"
        };
      }
    }

    return {
      text: internalClipboardText,
      source: "internal"
    };
  }

  async function rawRowsForSelection(selection: CellSelection): Promise<string[][]> {
    const addressRows = expandSelectionToRows(selection, workbookBounds);
    return Promise.all(
      addressRows.map((row) =>
        Promise.all(
          row.map(async (addr) => {
            const cached = viewportCache[addr];
            if (cached) return cached.raw;
            return (await commands.getCell(sheet, addr)).raw;
          })
        )
      )
    );
  }

  function circularRefsForDeltas(deltas: RecalcDelta[]): string[] {
    return [...new Set(deltas.flatMap((delta) => delta.circular))];
  }

  async function applyCellWrites(writes: CellWrite[]): Promise<RecalcDelta[]> {
    const deltas: RecalcDelta[] = [];
    for (const write of writes) {
      const delta = await commands.setCell(sheet, write.addr, write.raw);
      pushLocalNoopHistory();
      patchViewport(delta);
      deltas.push(delta);
    }

    await refreshViewport();
    return deltas;
  }

  async function runCopyCut(action: Extract<ClipboardAction, "copy" | "cut">): Promise<void> {
    if (editAddr) return;

    const selection = currentSelection();
    const label = selectionLabel(selection);
    statusText = action === "copy" ? `Copying ${label}` : `Cutting ${label}`;

    try {
      const tsv = serializeTsv(await rawRowsForSelection(selection));
      const source = await writeClipboardText(tsv);

      if (action === "cut") {
        const writes = expandSelection(selection, workbookBounds).map((addr) => ({ addr, raw: "" }));
        const deltas = await applyCellWrites(writes);
        const circular = circularRefsForDeltas(deltas);
        statusText =
          circular.length > 0
            ? `Circular reference: ${circular.join(", ")}`
            : `Cut ${label} to ${clipboardSourceLabel(source)}`;
      } else {
        statusText = `Copied ${label} to ${clipboardSourceLabel(source)}`;
      }
    } catch (error) {
      statusText = `${action === "copy" ? "Copy" : "Cut"} failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  async function runPaste(): Promise<void> {
    if (editAddr) return;

    const pasteStart = activeCell;
    statusText = `Pasting at ${pasteStart}`;

    try {
      const { text, source } = await readClipboardText();
      const parsedRows = parseTsv(text);
      const start = parseCellAddress(pasteStart);
      const parsedWidth = Math.max(1, ...parsedRows.map((row) => row.length));
      const endRow = Math.min(workbookBounds.rows, start.row + parsedRows.length - 1);
      const endColumn = Math.min(workbookBounds.columns, start.column + parsedWidth - 1);
      const clipped =
        start.row + parsedRows.length - 1 > workbookBounds.rows ||
        start.column + parsedWidth - 1 > workbookBounds.columns;
      const writes: CellWrite[] = [];

      for (const [rowOffset, row] of parsedRows.entries()) {
        const targetRow = start.row + rowOffset;
        for (const [columnOffset, raw] of row.entries()) {
          const targetColumn = start.column + columnOffset;
          if (targetRow <= workbookBounds.rows && targetColumn <= workbookBounds.columns) {
            writes.push({
              addr: cellAddress(targetColumn, targetRow),
              raw
            });
          }
        }
      }

      if (writes.length === 0) {
        statusText = `Paste clipped outside sheet bounds`;
        gridElement?.focus();
        return;
      }

      const deltas = await applyCellWrites(writes);
      selectionAnchor = pasteStart;
      activeCell = cellAddress(endColumn, endRow);
      await ensureCellVisible(activeCell);
      setFormulaFromActive();

      const circular = circularRefsForDeltas(deltas);
      if (circular.length > 0) {
        statusText = `Circular reference: ${circular.join(", ")}`;
      } else {
        const pastedLabel = selectionLabel();
        const clipStatus = clipped ? " clipped to sheet bounds" : "";
        statusText = `Pasted ${pastedLabel} from ${clipboardSourceLabel(source)}${clipStatus}`;
      }
    } catch (error) {
      statusText = `Paste failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  async function runClipboardAction(action: ClipboardAction): Promise<void> {
    if (action === "paste") {
      await runPaste();
    } else {
      await runCopyCut(action);
    }
  }

  function handleGridKeydown(event: KeyboardEvent): void {
    if (editAddr) return;

    const direction = directionForKey(event);
    if (direction) {
      event.preventDefault();
      void moveSelection(direction, event.shiftKey && event.key.startsWith("Arrow"));
      return;
    }

    if (event.key === "Enter" || event.key === "F2") {
      event.preventDefault();
      void startEdit(activeSnapshot.raw);
      return;
    }

    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      void commitActiveCell("");
      return;
    }

    if (isPrintableEditKey(event)) {
      event.preventDefault();
      void startEdit(event.key);
    }
  }

  function handleEditKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void commitActiveCell(editValue, event.shiftKey ? "up" : "down");
    }

    if (event.key === "Tab") {
      event.preventDefault();
      void commitActiveCell(editValue, event.shiftKey ? "left" : "right");
    }

    if (event.key === "Escape") {
      event.preventDefault();
      cancelEdit();
    }
  }

  function isTextEditingTarget(target: EventTarget | null): boolean {
    return (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    );
  }

  function statusForUndoRedo(kind: UndoRedoAction, delta: RecalcDelta): string {
    if (delta.circular.length > 0) {
      return `Circular reference: ${delta.circular.join(", ")}`;
    }

    return kind === "undo" ? "Undo complete" : "Redo complete";
  }

  async function runUndoRedo(kind: UndoRedoAction): Promise<void> {
    if (editAddr) return;

    statusText = kind === "undo" ? "Undoing" : "Redoing";
    const delta = kind === "undo" ? await commands.undo() : await commands.redo();
    if (!delta) {
      statusText = kind === "undo" ? "Nothing to undo" : "Nothing to redo";
      return;
    }

    applyLocalUndoRedo(kind);
    await applyDelta(delta);
    statusText = statusForUndoRedo(kind, delta);
    gridElement?.focus();
  }

  function statusForStructuralAction(action: StructuralAction): string {
    const { column, row } = parseCellAddress(activeCell);
    if (action === "insertRow") return `Inserted row ${row}`;
    if (action === "deleteRow") return `Deleted row ${row}`;
    if (action === "insertColumn") return `Inserted column ${columnLabel(column)}`;
    return `Deleted column ${columnLabel(column)}`;
  }

  function structuralActionLabel(action: StructuralAction): string {
    if (action === "insertRow") return "Insert row";
    if (action === "deleteRow") return "Delete row";
    if (action === "insertColumn") return "Insert column";
    return "Delete column";
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  async function structuralDeltaForAction(action: StructuralAction, at: number): Promise<RecalcDelta> {
    switch (action) {
      case "insertRow":
        return commands.insertRows(sheet, at, 1);
      case "deleteRow":
        return commands.deleteRows(sheet, at, 1);
      case "insertColumn":
        return commands.insertCols(sheet, at, 1);
      case "deleteColumn":
        return commands.deleteCols(sheet, at, 1);
    }
  }

  function shiftedPositionForStructuralAction(position: number, action: StructuralAction, at: number): number | null {
    if (action === "insertRow" || action === "insertColumn") {
      return position <= at ? position : position + 1;
    }

    const deletedPosition = at + 1;
    if (position < deletedPosition) return position;
    if (position === deletedPosition) return null;
    return position - 1;
  }

  function shiftedAddrForStructuralAction(addr: string, action: StructuralAction, at: number): string | null {
    const parsed = parseCellAddress(addr);
    if (action === "insertRow" || action === "deleteRow") {
      const row = shiftedPositionForStructuralAction(parsed.row, action, at);
      return row === null ? null : cellAddress(parsed.column, row);
    }

    const column = shiftedPositionForStructuralAction(parsed.column, action, at);
    return column === null ? null : cellAddress(column, parsed.row);
  }

  function shiftedDimensionOverrides(
    overrides: Record<number, number>,
    action: StructuralAction,
    at: number
  ): Record<number, number> {
    const next: Record<number, number> = {};
    for (const [indexText, size] of Object.entries(overrides)) {
      const shiftedIndex = shiftedPositionForStructuralAction(Number(indexText), action, at);
      if (shiftedIndex !== null) {
        next[shiftedIndex] = size;
      }
    }
    return next;
  }

  function applyLocalStructuralEdit(action: StructuralAction, at: number): void {
    const nextFormats: Record<string, CellFormatPayload> = {};
    for (const [addr, format] of Object.entries(cellFormats)) {
      const shiftedAddr = shiftedAddrForStructuralAction(addr, action, at);
      if (shiftedAddr) {
        nextFormats[shiftedAddr] = cloneFormat(format);
      }
    }
    cellFormats = nextFormats;

    if (action === "insertRow" || action === "deleteRow") {
      rowHeights = shiftedDimensionOverrides(rowHeights, action, at);
    } else {
      columnWidths = shiftedDimensionOverrides(columnWidths, action, at);
    }
  }

  async function runStructuralAction(action: StructuralAction): Promise<void> {
    if (editAddr) return;

    const { column, row } = parseCellAddress(activeCell);
    const at = action === "insertRow" || action === "deleteRow" ? row - 1 : column - 1;
    const before = captureLocalUiState();

    statusText = `${structuralActionLabel(action)} running`;
    try {
      const delta = await structuralDeltaForAction(action, at);
      applyLocalStructuralEdit(action, at);
      pushLocalUiHistory(before);
      clampSelectionToWorkbook();
      await applyDelta(delta);
      statusText =
        delta.circular.length > 0
          ? `Circular reference: ${delta.circular.join(", ")}`
          : statusForStructuralAction(action);
    } catch (error) {
      statusText = `${structuralActionLabel(action)} failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  function numberFormatActionLabel(action: NumberFormatAction): string {
    if (action === "general") return "General number";
    if (action === "fixed") return "Fixed decimal";
    if (action === "percent") return "Percent";
    if (action === "currency") return "Currency";
    return "Text";
  }

  function alignLabel(align: FormatAlign): string {
    if (align === "left") return "Left align";
    if (align === "center") return "Center align";
    if (align === "right") return "Right align";
    return "Default align";
  }

  async function runSetFormat(format: CellFormatPayload, label: string): Promise<void> {
    if (editAddr) return;

    const before = captureLocalUiState();
    const selection = currentSelection();
    const range = normalizeSelection(selection, workbookBounds);
    statusText = `${label} running`;
    try {
      const delta = await commands.setFormat(sheet, { start: range.start, end: range.end }, format);
      for (const addr of expandSelection(selection, workbookBounds)) {
        setCellFormat(addr, format);
      }
      pushLocalUiHistory(before);
      await applyDelta(delta);
      statusText =
        delta.circular.length > 0
          ? `Circular reference: ${delta.circular.join(", ")}`
          : `${label} applied to ${selectionLabel(selection)}`;
    } catch (error) {
      statusText = `${label} failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  async function runNumberFormatAction(action: NumberFormatAction): Promise<void> {
    const format = cloneFormat(activeFormat);
    format.number = numberFormatForAction(action);
    await runSetFormat(format, numberFormatActionLabel(action));
  }

  async function toggleBold(): Promise<void> {
    const format = cloneFormat(activeFormat);
    format.bold = !format.bold;
    await runSetFormat(format, "Bold");
  }

  async function runAlignAction(align: FormatAlign): Promise<void> {
    const format = cloneFormat(activeFormat);
    format.align = align;
    await runSetFormat(format, alignLabel(align));
  }

  async function resizeActiveColumn(direction: ResizeDirection): Promise<void> {
    if (editAddr) return;

    const { column } = parseCellAddress(activeCell);
    const currentWidth = columnWidth(column);
    const nextWidth = Math.max(
      minColumnWidth,
      currentWidth + (direction === "increase" ? columnResizeStep : -columnResizeStep)
    );
    if (nextWidth === currentWidth) {
      statusText = `Column ${columnLabel(column)} is at minimum width`;
      gridElement?.focus();
      return;
    }

    const before = captureLocalUiState();
    statusText = `Resize column ${columnLabel(column)} running`;
    try {
      const delta = await commands.resizeColumn(sheet, column - 1, nextWidth);
      columnWidths = { ...columnWidths, [column]: nextWidth };
      pushLocalUiHistory(before);
      await applyDelta(delta);
      statusText = `Column ${columnLabel(column)} width ${nextWidth}px`;
    } catch (error) {
      statusText = `Resize column failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  async function resizeActiveRow(direction: ResizeDirection): Promise<void> {
    if (editAddr) return;

    const { row } = parseCellAddress(activeCell);
    const currentHeight = rowHeight(row);
    const nextHeight = Math.max(
      minRowHeight,
      currentHeight + (direction === "increase" ? rowResizeStep : -rowResizeStep)
    );
    if (nextHeight === currentHeight) {
      statusText = `Row ${row} is at minimum height`;
      gridElement?.focus();
      return;
    }

    const before = captureLocalUiState();
    statusText = `Resize row ${row} running`;
    try {
      const delta = await commands.resizeRow(sheet, row - 1, nextHeight);
      rowHeights = { ...rowHeights, [row]: nextHeight };
      pushLocalUiHistory(before);
      await applyDelta(delta);
      statusText = `Row ${row} height ${nextHeight}px`;
    } catch (error) {
      statusText = `Resize row failed: ${errorMessage(error)}`;
    }

    gridElement?.focus();
  }

  function handleAppKeydown(event: KeyboardEvent): void {
    if (editAddr || isTextEditingTarget(event.target)) return;

    const clipboardAction = clipboardActionForKey(event);
    if (clipboardAction) {
      event.preventDefault();
      void runClipboardAction(clipboardAction);
      return;
    }

    const action = undoRedoActionForKey(event);
    if (!action) return;

    event.preventDefault();
    void runUndoRedo(action);
  }

  function handleToolbarButtonMouseDown(event: MouseEvent): void {
    if (editAddr) {
      event.preventDefault();
    }
  }

  function handleFormulaKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void commitFormulaBar();
    }

    if (event.key === "Escape") {
      setFormulaFromActive();
    }
  }

  onMount(() => {
    void resetWorkbook();
    window.addEventListener("keydown", handleAppKeydown);
    return () => window.removeEventListener("keydown", handleAppKeydown);
  });
</script>

<svelte:head>
  <title>Excel Lite</title>
</svelte:head>

<div class="app-shell">
  <header class="top-strip">
    <div class="workbook-title">
      <button class="icon-button" type="button" title="New workbook" aria-label="New workbook" on:click={resetWorkbook}>
        <FilePlus2 size={18} strokeWidth={1.9} />
      </button>
      <div>
        <strong>Excel Lite</strong>
        <span>Book 1</span>
      </div>
      <div class="toolbar-actions" role="toolbar" aria-label="Toolbar">
        <div class="toolbar-group" role="group" aria-label="File operations">
          <button
            class="icon-button"
            type="button"
            title="Open workbook"
            aria-label="Open workbook"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runFileOperation("openWorkbook")}
          >
            <FolderOpen size={18} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            type="button"
            title="Save workbook"
            aria-label="Save workbook"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runFileOperation("saveWorkbook")}
          >
            <Save size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            type="button"
            title="Import CSV"
            aria-label="Import CSV"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runFileOperation("importCsv")}
          >
            <FileInput size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            type="button"
            title="Import XLSX"
            aria-label="Import XLSX"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runFileOperation("importXlsx")}
          >
            <FileSpreadsheet size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            type="button"
            title="Export CSV"
            aria-label="Export CSV"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runFileOperation("exportCsv")}
          >
            <FileOutput size={17} strokeWidth={1.9} />
          </button>
        </div>
        <div class="toolbar-group" role="group" aria-label="History">
          <button
            class="icon-button"
            type="button"
            title="Undo"
            aria-label="Undo"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runUndoRedo("undo")}
          >
            <Undo2 size={18} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            type="button"
            title="Redo"
            aria-label="Redo"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runUndoRedo("redo")}
          >
            <Redo2 size={18} strokeWidth={1.9} />
          </button>
        </div>
        <div class="toolbar-group" role="group" aria-label="Number format">
          <button
            class="icon-button"
            class:active={activeFormat.number.type === "general"}
            type="button"
            title="General number"
            aria-label="General number"
            aria-pressed={activeFormat.number.type === "general"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runNumberFormatAction("general")}
          >
            <Hash size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.number.type === "fixed"}
            type="button"
            title="Fixed decimal"
            aria-label="Fixed decimal"
            aria-pressed={activeFormat.number.type === "fixed"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runNumberFormatAction("fixed")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Hash size={17} strokeWidth={1.9} />
              <Plus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.number.type === "percent"}
            type="button"
            title="Percent"
            aria-label="Percent"
            aria-pressed={activeFormat.number.type === "percent"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runNumberFormatAction("percent")}
          >
            <Percent size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.number.type === "currency"}
            type="button"
            title="Currency"
            aria-label="Currency"
            aria-pressed={activeFormat.number.type === "currency"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runNumberFormatAction("currency")}
          >
            <DollarSign size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.number.type === "text"}
            type="button"
            title="Text"
            aria-label="Text"
            aria-pressed={activeFormat.number.type === "text"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runNumberFormatAction("text")}
          >
            <Type size={17} strokeWidth={1.9} />
          </button>
        </div>
        <div class="toolbar-group" role="group" aria-label="Cell style">
          <button
            class="icon-button"
            class:active={activeFormat.bold}
            type="button"
            title="Bold"
            aria-label="Bold"
            aria-pressed={activeFormat.bold}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void toggleBold()}
          >
            <Bold size={17} strokeWidth={2.2} />
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.align === "left"}
            type="button"
            title="Left align"
            aria-label="Left align"
            aria-pressed={activeFormat.align === "left"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runAlignAction("left")}
          >
            <AlignLeft size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.align === "center"}
            type="button"
            title="Center align"
            aria-label="Center align"
            aria-pressed={activeFormat.align === "center"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runAlignAction("center")}
          >
            <AlignCenter size={17} strokeWidth={1.9} />
          </button>
          <button
            class="icon-button"
            class:active={activeFormat.align === "right"}
            type="button"
            title="Right align"
            aria-label="Right align"
            aria-pressed={activeFormat.align === "right"}
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runAlignAction("right")}
          >
            <AlignRight size={17} strokeWidth={1.9} />
          </button>
        </div>
        <div class="toolbar-group" role="group" aria-label="Cell size">
          <button
            class="icon-button"
            type="button"
            title="Increase column width"
            aria-label="Increase column width"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void resizeActiveColumn("increase")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Columns4 size={18} strokeWidth={1.9} />
              <Plus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="Decrease column width"
            aria-label="Decrease column width"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void resizeActiveColumn("decrease")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Columns4 size={18} strokeWidth={1.9} />
              <Minus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="Increase row height"
            aria-label="Increase row height"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void resizeActiveRow("increase")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Rows4 size={18} strokeWidth={1.9} />
              <Plus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="Decrease row height"
            aria-label="Decrease row height"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void resizeActiveRow("decrease")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Rows4 size={18} strokeWidth={1.9} />
              <Minus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
        </div>
        <div class="toolbar-group" role="group" aria-label="Rows and columns">
          <button
            class="icon-button"
            type="button"
            title="Insert row above"
            aria-label="Insert row above"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runStructuralAction("insertRow")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Rows4 size={18} strokeWidth={1.9} />
              <Plus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="Delete row"
            aria-label="Delete row"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runStructuralAction("deleteRow")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Rows4 size={18} strokeWidth={1.9} />
              <Minus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="Insert column before"
            aria-label="Insert column before"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runStructuralAction("insertColumn")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Columns4 size={18} strokeWidth={1.9} />
              <Plus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
          <button
            class="icon-button"
            type="button"
            title="Delete column"
            aria-label="Delete column"
            on:mousedown={handleToolbarButtonMouseDown}
            on:click={() => void runStructuralAction("deleteColumn")}
          >
            <span class="icon-stack" aria-hidden="true">
              <Columns4 size={18} strokeWidth={1.9} />
              <Minus size={10} strokeWidth={2.4} class="icon-badge" />
            </span>
          </button>
        </div>
      </div>
    </div>

    <div class="formula-bar">
      <div class="name-box" aria-label="Selection">
        {selectedRange.start === selectedRange.end ? activeCell : `${selectedRange.start}:${selectedRange.end}`}
      </div>
      <input
        bind:this={formulaInput}
        bind:value={formulaValue}
        aria-label="Formula bar"
        spellcheck="false"
        on:keydown={handleFormulaKeydown}
      />
    </div>
  </header>

  <main
    bind:this={sheetViewportElement}
    class="sheet-viewport"
    aria-label="Workbook grid"
    on:scroll={handleViewportScroll}
  >
    <div class="grid-virtual-canvas" style={virtualCanvasStyle} aria-hidden="true"></div>
    <table
      bind:this={gridElement}
      class="grid"
      style={gridPositionStyle}
      role="grid"
      aria-rowcount={workbookBounds.rows}
      aria-colcount={workbookBounds.columns}
      tabindex="0"
      on:keydown={handleGridKeydown}
    >
      <thead>
        <tr>
          <th class="corner" aria-hidden="true"></th>
          {#each columns as column}
            <th scope="col" style={columnSizeStyle(columnWidths[column] ?? defaultColumnWidth)}>{columnLabel(column)}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as row}
          <tr aria-rowindex={row}>
            <th scope="row" style={rowHeaderStyle(rowHeights[row] ?? defaultRowHeight)}>{row}</th>
            {#each columns as column}
              {@const addr = cellAddress(column, row)}
              {@const snapshot = viewportCache[addr] ?? blankSnapshot(addr)}
              {@const format = cellFormats[addr] ?? defaultCellFormat}
              <td
                class:selected={isSelected(addr)}
                class:active={addr === activeCell}
                class:editing={addr === editAddr}
                class:numeric={snapshot.value.kind === "number"}
                class:errorValue={snapshot.value.kind === "error"}
                class:format-bold={format.bold}
                class:align-left={format.align === "left"}
                class:align-center={format.align === "center"}
                class:align-right={format.align === "right"}
                style={cellSizeStyle(
                  columnWidths[column] ?? defaultColumnWidth,
                  rowHeights[row] ?? defaultRowHeight
                )}
                aria-selected={isSelected(addr)}
                aria-colindex={column}
                role="gridcell"
                tabindex="-1"
                on:click={(event) => selectCell(addr, event.shiftKey)}
                on:dblclick={() => void startEdit(snapshot.raw)}
              >
                {#if editAddr === addr}
                  <input
                    bind:this={editInput}
                    bind:value={editValue}
                    class="cell-editor"
                    aria-label={`Edit ${addr}`}
                    spellcheck="false"
                    on:keydown={handleEditKeydown}
                    on:blur={() => {
                      if (editAddr === addr) void commitActiveCell(editValue);
                    }}
                  />
                {:else}
                  <span>{snapshot.display}</span>
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </main>

  <footer class="status-bar">
    <span>{statusText}</span>
    <span>Raw: {activeSnapshot.raw || "blank"}</span>
  </footer>
</div>
