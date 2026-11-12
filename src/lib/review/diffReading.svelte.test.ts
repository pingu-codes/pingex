import { flushSync, tick } from "svelte";
import { SvelteSet } from "svelte/reactivity";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DiffReading } from "$lib/review/diffReading.svelte";
import type { LineRange } from "$lib/review/reviewPage";
import type { PrFile } from "$lib/types";

/** The view's height. */
const VIEW = 500;
/** A folded file is its header. */
const HEADER = 40;

const file = (path: string) => ({ path, patch: "", hunks: [] }) as unknown as PrFile;

/**
 * A page of diffs laid out as a browser would: files stacked from the top, a
 * folded one only its header tall. Scroll events are the test's to send, as a
 * browser sends them after the scroll position changes.
 */
function page(heights: Record<string, number>) {
  let files = $state(Object.keys(heights).map(file));
  const folds = new SvelteSet<string>();
  const viewed = new SvelteSet<string>();
  const added: { range: LineRange; body: string; replacement: string | null }[] = [];
  const shown = vi.fn();
  /** Layout reads of the files' boxes. */
  let asked = 0;

  const height = (path: string) => (folds.has(path) ? HEADER : heights[path]);
  const top = (path: string) => {
    let at = 0;
    for (const { path: other } of files) {
      if (other === path) return at;
      at += height(other);
    }
    return at;
  };
  const scroller = document.createElement("div");
  Object.defineProperty(scroller, "clientHeight", { value: VIEW });
  Object.defineProperty(scroller, "scrollHeight", {
    get: () => files.reduce((sum, { path }) => sum + height(path), 0),
  });
  scroller.getBoundingClientRect = () => ({ top: 0, bottom: VIEW }) as DOMRect;
  for (const path of Object.keys(heights)) {
    const element = document.createElement("section");
    element.dataset.file = path;
    element.getBoundingClientRect = () => {
      asked += 1;
      const at = top(path) - scroller.scrollTop;
      return { top: at, bottom: at + height(path) } as DOMRect;
    };
    // Its top to the top of the view, as far as the page goes.
    element.scrollIntoView = () => {
      scroller.scrollTop = Math.min(top(path), scroller.scrollHeight - VIEW);
    };
    scroller.append(element);
  }

  let reading!: DiffReading;
  let detach: (() => void) | undefined;
  const stop = $effect.root(() => {
    reading = new DiffReading({
      files: () => files,
      folds,
      viewed: {
        has: (path) => viewed.has(path),
        set: (targets, value) => {
          for (const target of targets) {
            if (value) viewed.add(target.path);
            else viewed.delete(target.path);
          }
        },
      },
      addEntry: (range, body, replacement) => added.push({ range, body, replacement }),
      showFiles: shown,
    });
    detach = reading.attach(scroller) as (() => void) | undefined;
  });
  flushSync();
  stops.push(() => {
    detach?.();
    stop();
  });

  return {
    reading,
    folds,
    viewed,
    added,
    shown,
    scroller,
    get asked() {
      return asked;
    },
    setFiles(paths: string[]) {
      files = paths.map(file);
      flushSync();
    },
    /** Scroll to `to`, and let the frame after the scroll event run. */
    async scrollTo(to: number) {
      scroller.scrollTop = to;
      scroller.dispatchEvent(new Event("scroll"));
      await frame();
    },
    /** Where a file's top sits from the top of the view. */
    offset: (path: string) => top(path) - scroller.scrollTop,
    press(key: string, init: KeyboardEventInit = {}) {
      const event = new KeyboardEvent("keydown", { key, cancelable: true, ...init });
      reading.onKeydown(event, pageKeys);
      return event;
    },
  };
}

const pageKeys = { nextPr: vi.fn() };
let stops: (() => void)[] = [];

afterEach(() => {
  for (const stop of stops) stop();
  stops = [];
  pageKeys.nextPr.mockClear();
});

const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** A fold's change has been drawn, its place kept, and the next frame measured. */
async function settle() {
  flushSync();
  await tick();
  await tick();
  await frame();
}

/** `count` files `height` tall, named f0, f1, … */
const even = (count: number, height: number) =>
  Object.fromEntries(Array.from({ length: count }, (_, index) => [`f${index}`, height]));

describe("the file being read", () => {
  it("is the first file until the page is measured", () => {
    const { reading } = page({ "a.ts": 1000, "b.ts": 1000 });
    expect(reading.currentPath).toBe("a.ts");
  });

  it("is the open file at the top of the view, and moves on when only a sliver of it shows", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000 });
    await at.scrollTo(1500);
    expect(at.reading.currentPath).toBe("b.ts");
    await at.scrollTo(950);
    expect(at.reading.currentPath).toBe("a.ts");
    await at.scrollTo(970);
    expect(at.reading.currentPath).toBe("b.ts");
  });

  it("passes over folded files to the first open one showing", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000, "d.ts": 1000 });
    at.folds.add("a.ts");
    at.folds.add("b.ts");
    await settle();
    expect(at.reading.currentPath).toBe("c.ts");
  });

  it("is the first file showing when every file showing is folded", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000 });
    at.reading.foldAll(true);
    await settle();
    // a.ts shows no more than its header: that one is read.
    expect(at.reading.currentPath).toBe("b.ts");
  });

  it("is measured again when a file folds from elsewhere, as marks read on load fold theirs", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000 });
    at.folds.add("a.ts");
    await settle();
    expect(at.reading.currentPath).toBe("b.ts");
  });

  it("falls back to the first file when its own leaves the list", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000 });
    await at.scrollTo(2500);
    expect(at.reading.currentPath).toBe("c.ts");
    at.setFiles(["a.ts", "b.ts"]);
    expect(at.reading.currentPath).toBe("a.ts");
  });

  it("asks about few files on a long page", async () => {
    const at = page(even(1024, 100));
    await at.scrollTo(50_000);
    expect(at.reading.currentPath).toBe("f500");
    expect(at.asked).toBeLessThan(15);
  });

  it("is the last file at the end of the page, scrolling down, though it can't reach the top", async () => {
    const at = page(even(10, 100));
    await at.scrollTo(500);
    expect(at.reading.currentPath).toBe("f9");
    await at.scrollTo(450);
    expect(at.reading.currentPath).toBe("f4");
  });
});

describe("a file the reader chose", () => {
  it("stays current while the page moves by less than a diff row, and lets go after", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000 });
    at.reading.hold("b.ts");
    await at.scrollTo(10);
    expect(at.reading.currentPath).toBe("b.ts");
    await at.scrollTo(40);
    expect(at.reading.currentPath).toBe("a.ts");
  });

  it("lets go when it folds: the next file is current", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000 });
    at.reading.jumpTo("b.ts");
    await settle();
    expect(at.offset("b.ts")).toBe(0);
    at.reading.toggle("b.ts");
    await settle();
    expect(at.reading.isOpen("b.ts")).toBe(false);
    expect(at.reading.currentPath).toBe("c.ts");
  });

  it("a jump opens the file, shows the file list, and holds a file that can't reach the top", async () => {
    const at = page(even(10, 100));
    at.folds.add("f8");
    at.reading.jumpTo("f8");
    expect(at.shown).toHaveBeenCalled();
    expect(at.reading.isOpen("f8")).toBe(true);
    await settle();
    expect(at.scroller.scrollTop).toBe(500);
    expect(at.reading.currentPath).toBe("f8");
  });

  it("a line selected in a file holds it", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000 });
    at.reading.selectLine("b.ts", "RIGHT", 3, false);
    await settle();
    expect(at.reading.currentPath).toBe("b.ts");
  });
});

describe("folding keeps the reader's place", () => {
  it("the file being read folds from inside: its header comes back to the top of the view", async () => {
    const at = page(even(4, 1000));
    await at.scrollTo(1500);
    at.reading.toggle("f1");
    await settle();
    expect(at.offset("f1")).toBe(0);
  });

  it("folding every other file keeps the one clicked where it is on screen", async () => {
    const at = page(even(10, 1000));
    await at.scrollTo(7700);
    expect(at.offset("f8")).toBe(300);
    at.reading.toggle("f8", true);
    await settle();
    expect(at.offset("f8")).toBe(300);
    expect(at.reading.currentPath).toBe("f8");
    expect([...at.folds].sort()).toEqual(["f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f9"]);
  });

  it("v walks down the files, each folding in place", async () => {
    const at = page(even(8, 2000));
    await at.scrollTo(7000);
    expect(at.reading.currentPath).toBe("f3");

    at.press("v");
    await settle();
    expect(at.viewed.has("f3")).toBe(true);
    expect(at.offset("f3")).toBe(0);
    expect(at.reading.currentPath).toBe("f4");

    at.press("v");
    await settle();
    expect(at.viewed.has("f4")).toBe(true);
    expect(at.offset("f3")).toBe(0);
    expect(at.reading.currentPath).toBe("f5");
  });

  it("unfolding them all keeps the file being read where it is", async () => {
    const at = page(even(6, 1000));
    at.reading.foldAll(true);
    await settle();
    at.reading.hold("f3");
    at.reading.foldAll(false);
    await settle();
    expect(at.folds.size).toBe(0);
    expect(at.offset("f3")).toBe(3 * HEADER);
  });
});

describe("viewed", () => {
  it("marking a file viewed folds it; unmarking opens it again", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000 });
    at.reading.setViewed([file("a.ts"), file("b.ts")], true);
    expect([...at.viewed]).toEqual(["a.ts", "b.ts"]);
    expect(at.reading.isOpen("a.ts")).toBe(false);
    at.reading.setViewed([file("a.ts")], false);
    expect(at.viewed.has("a.ts")).toBe(false);
    expect(at.reading.isOpen("a.ts")).toBe(true);
    expect(at.reading.isOpen("b.ts")).toBe(false);
  });
});

describe("selected lines and the composer", () => {
  it("extends a selection on one side of one file, and starts afresh across", () => {
    const { reading } = page({ "a.ts": 1000, "b.ts": 1000 });
    reading.selectLine("a.ts", "RIGHT", 5, false);
    reading.selectLine("a.ts", "RIGHT", 3, true);
    expect(reading.selection).toEqual({ path: "a.ts", side: "RIGHT", anchor: 5, start: 3, end: 5 });
    reading.selectLine("a.ts", "LEFT", 4, true);
    expect(reading.selection).toEqual({ path: "a.ts", side: "LEFT", anchor: 4, start: 4, end: 4 });
    reading.selectLine("b.ts", "LEFT", 9, true);
    expect(reading.selection?.path).toBe("b.ts");
  });

  it("c composes on the selection, and the text becomes the page's entry", () => {
    const at = page({ "a.ts": 1000 });
    at.press("c");
    expect(at.reading.composer).toBeNull();

    at.reading.selectLine("a.ts", "RIGHT", 2, false);
    at.press("c");
    expect(at.reading.composer?.mode).toBe("comment");
    at.reading.add("Guard this", null);

    expect(at.added).toEqual([
      { range: { path: "a.ts", side: "RIGHT", anchor: 2, start: 2, end: 2 }, body: "Guard this", replacement: null },
    ]);
    expect(at.reading.composer).toBeNull();
    expect(at.reading.selection).toBeNull();
  });

  it("selecting another line closes the composer, and Escape drops the selection", () => {
    const at = page({ "a.ts": 1000 });
    at.reading.selectLine("a.ts", "RIGHT", 2, false);
    at.reading.compose("suggestion");
    at.reading.selectLine("a.ts", "RIGHT", 4, false);
    expect(at.reading.composer).toBeNull();

    at.press("Escape");
    expect(at.reading.selection).toBeNull();
  });
});

describe("the keys", () => {
  it("] and [ walk the files, stopping at the ends", async () => {
    const at = page({ "a.ts": 1000, "b.ts": 1000, "c.ts": 1000 });
    at.press("[");
    await settle();
    expect(at.reading.currentPath).toBe("a.ts");
    at.press("]");
    await settle();
    at.press("]");
    await settle();
    at.press("]");
    await settle();
    expect(at.reading.currentPath).toBe("c.ts");
    expect(at.offset("c.ts")).toBe(0);
  });

  it("hands the page its own commands, and leaves alone the ones nobody runs", () => {
    const at = page({ "a.ts": 1000 });
    expect(at.press("j").defaultPrevented).toBe(true);
    expect(pageKeys.nextPr).toHaveBeenCalledOnce();
    expect(at.press("n").defaultPrevented).toBe(false);
    expect(at.press("v").defaultPrevented).toBe(true);
  });

  it("leaves keys typed into a field, or already taken, to them", () => {
    const at = page({ "a.ts": 1000 });
    const field = document.createElement("textarea");
    document.body.append(field);
    const typed = new KeyboardEvent("keydown", { key: "v", cancelable: true, bubbles: true });
    field.addEventListener("keydown", (event) => at.reading.onKeydown(event));
    field.dispatchEvent(typed);
    field.remove();
    expect(at.viewed.size).toBe(0);

    const taken = new KeyboardEvent("keydown", { key: "v", cancelable: true });
    taken.preventDefault();
    at.reading.onKeydown(taken);
    expect(at.viewed.size).toBe(0);
  });
});
