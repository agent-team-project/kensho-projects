import type { RectA1 } from "./commands";
import { cellAddress, parseCellAddress } from "./grid";

export function expandRect(rect: RectA1): string[] {
  const start = parseCellAddress(rect.start);
  const end = parseCellAddress(rect.end);
  const minColumn = Math.min(start.column, end.column);
  const maxColumn = Math.max(start.column, end.column);
  const minRow = Math.min(start.row, end.row);
  const maxRow = Math.max(start.row, end.row);
  const cells: string[] = [];

  for (let row = minRow; row <= maxRow; row += 1) {
    for (let column = minColumn; column <= maxColumn; column += 1) {
      cells.push(cellAddress(column, row));
    }
  }

  return cells;
}
