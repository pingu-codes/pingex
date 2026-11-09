// @vitest-environment node
import { describe, expect, it } from "vitest";
import { type FileBox, fileInView } from "$lib/review/reviewPage";

/** Files stacked from y = 0, each `[height, open]`. */
function stack(...files: [number, boolean][]): FileBox[] {
  let top = 0;
  return files.map(([height, open]) => {
    const box = { top, bottom: top + height, open };
    top = box.bottom;
    return box;
  });
}

const inView = (boxes: FileBox[], viewTop: number, viewBottom = viewTop + 500) =>
  fileInView(boxes.length, (index) => boxes[index], viewTop, viewBottom);

describe("the file being read", () => {
  const open = stack([1000, true], [1000, true], [1000, true]);

  it("is the open file at the top of the view", () => {
    expect(inView(open, 0)).toBe(0);
    expect(inView(open, 1500)).toBe(1);
    expect(inView(open, 2000)).toBe(2);
  });

  it("moves on when only a sliver of a file still shows", () => {
    expect(inView(open, 950)).toBe(0);
    expect(inView(open, 970)).toBe(1);
  });

  it("passes over collapsed files to the first open one showing", () => {
    const boxes = stack([40, false], [40, false], [1000, true], [1000, true]);
    expect(inView(boxes, 0)).toBe(2);
  });

  it("is the first file showing when every file showing is collapsed", () => {
    const boxes = stack([60, false], [60, false], [60, false]);
    expect(inView(boxes, 0)).toBe(0);
    // The open file is below the fold.
    const above = stack([60, false], [60, false], [1000, true]);
    expect(inView(above, 0, 100)).toBe(0);
  });

  it("is none when nothing is laid out", () => {
    expect(inView([], 0)).toBe(-1);
    const hidden = { top: 0, bottom: 0, open: true };
    expect(fileInView(2, () => hidden, 0, 0)).toBe(-1);
  });

  it("asks about few files on a long page", () => {
    const boxes = stack(...Array.from({ length: 1024 }, (): [number, boolean] => [100, true]));
    let asked = 0;
    const box = (index: number) => {
      asked += 1;
      return boxes[index];
    };
    expect(fileInView(boxes.length, box, 50_000, 50_500)).toBe(500);
    expect(asked).toBeLessThan(15);
  });
});
