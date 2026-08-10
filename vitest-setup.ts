import "@testing-library/jest-dom/vitest";

// Deno exposes a process-backed localStorage whose SQLite database may be
// read-only in parallel Vitest workers. Tests need the browser semantics, so
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
