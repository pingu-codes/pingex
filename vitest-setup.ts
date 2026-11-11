import { afterAll, beforeEach } from "vitest";
import { forgetPlaces } from "./src/lib/app/places";

// The query cache outlives components, so a test would otherwise see answers
// an earlier test in the same file read. It registers its reset when loaded.
beforeEach(() => (globalThis as { __resetQueryCache?: () => void }).__resetQueryCache?.());

// Likewise what each page had open when it was left.
beforeEach(forgetPlaces);

// And the browser preview's review desk, which the command fakes answer from:
// back to the demo before each test, which then seeds what it needs. It
// registers its reset when loaded.
beforeEach(() => (globalThis as { __resetPreviewDesk?: () => void }).__resetPreviewDesk?.());

// Pure-logic test files opt into `// @vitest-environment node`, which skips
// booting jsdom. Everything below is a DOM shim, so only install it there.
if (typeof window !== "undefined") {
  await import("@testing-library/jest-dom/vitest");
  installDomShims();
  // Unmounting a Skeleton dialog queues a zero-delay focus-return timer
  // (zag focus-trap). Drain it before jsdom is torn down, or it throws
  // "document is not defined" after the file's last test.
  afterAll(() => new Promise((resolve) => setTimeout(resolve, 0)));
}

function installDomShims(): void {
  // jsdom has no layout engine; real browsers provide this scrolling method.
  Object.defineProperty(Element.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: () => {},
  });

  // Some runtimes (Deno, recent Node) expose a process-backed localStorage
  // that parallel Vitest workers can share or find read-only. Tests need the browser semantics, so
  // use a per-worker in-memory implementation instead.
  const localValues = new Map<string, string>();
  const memoryLocalStorage: Storage = {
    get length() {
      return localValues.size;
    },
    clear: () => localValues.clear(),
    getItem: (key) => localValues.get(key) ?? null,
    key: (index) => [...localValues.keys()][index] ?? null,
    removeItem: (key) => localValues.delete(key),
    setItem: (key, value) => localValues.set(key, String(value)),
  };
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: memoryLocalStorage });
  Object.defineProperty(window, "localStorage", { configurable: true, value: memoryLocalStorage });

  class ResizeObserverStub implements ResizeObserver {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }

  Object.defineProperty(globalThis, "ResizeObserver", {
    configurable: true,
    value: ResizeObserverStub,
  });

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string): MediaQueryList => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });

  Object.defineProperty(window, "requestAnimationFrame", {
    configurable: true,
    value: (callback: FrameRequestCallback) => window.setTimeout(() => callback(performance.now()), 0),
  });

  Object.defineProperty(window, "cancelAnimationFrame", {
    configurable: true,
    value: (handle: number) => window.clearTimeout(handle),
  });
}
