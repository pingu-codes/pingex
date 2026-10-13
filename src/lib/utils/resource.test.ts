import { describe, expect, it } from "vitest";
import { resource } from "$lib/utils/resource.svelte";

/** A fetch whose answer the test hands over by hand, in any order. */
function controlled<T>() {
  const calls: { args: unknown[]; resolve: (value: T) => void; reject: (cause: unknown) => void }[] = [];
  const fetcher = (...args: unknown[]) =>
    new Promise<T>((resolve, reject) => {
      calls.push({ args, resolve, reject });
    });
  return { calls, fetcher };
}

describe("resource", () => {
  it("starts idle and loads on reload", async () => {
    const { calls, fetcher } = controlled<string>();
    const res = resource(fetcher);
    expect(res.value).toBeUndefined();
    expect(res.loading).toBe(false);
    expect(calls).toHaveLength(0);

    const done = res.reload();
    expect(res.loading).toBe(true);
    calls[0].resolve("one");
    await done;
    expect(res.value).toBe("one");
    expect(res.loading).toBe(false);
    expect(res.error).toBeNull();
  });

  it("starts from `initial` and passes reload arguments to the fetcher", async () => {
    const { calls, fetcher } = controlled<number>();
    const res = resource((path: string, limit: number) => fetcher(path, limit), { initial: 0 });
    expect(res.value).toBe(0);
    const done = res.reload("a.ts", 10);
    expect(calls[0].args).toEqual(["a.ts", 10]);
    calls[0].resolve(7);
    await done;
    expect(res.value).toBe(7);
  });

  it("lets the latest request win when an older one answers last", async () => {
    const { calls, fetcher } = controlled<string>();
    const res = resource(fetcher);
    const first = res.reload();
    const second = res.reload();

    calls[1].resolve("new");
    await second;
    expect(res.value).toBe("new");
    expect(res.loading).toBe(false);

    calls[0].resolve("stale");
    await first;
    expect(res.value).toBe("new");
  });

  it("drops a stale failure and keeps loading on for the request still in flight", async () => {
    const { calls, fetcher } = controlled<string>();
    const res = resource(fetcher);
    const first = res.reload();
    const second = res.reload();

    calls[0].reject(new Error("stale failure"));
    await first;
    expect(res.error).toBeNull();
    expect(res.loading).toBe(true);

    calls[1].resolve("fresh");
    await second;
    expect(res.value).toBe("fresh");
    expect(res.loading).toBe(false);
  });

  it("keeps the last value on failure and clears the error on the next reload", async () => {
    const { calls, fetcher } = controlled<string>();
    const res = resource(fetcher, { describe: (cause) => `Could not load: ${String(cause)}` });
    const ok = res.reload();
    calls[0].resolve("kept");
    await ok;

    const failing = res.reload();
    calls[1].reject("offline");
    await failing;
    expect(res.value).toBe("kept");
    expect(res.error).toBe("Could not load: offline");

    void res.reload();
    expect(res.error).toBeNull();
    expect(res.value).toBe("kept");
  });

  it("set() replaces the value and drops the request in flight", async () => {
    const { calls, fetcher } = controlled<string>();
    const res = resource(fetcher);
    const pending = res.reload();
    res.set("from mutation");
    expect(res.loading).toBe(false);

    calls[0].resolve("stale read");
    await pending;
    expect(res.value).toBe("from mutation");
  });

  it("reports a fetcher that throws synchronously", async () => {
    const res = resource((): Promise<string> => {
      throw new Error("bad input");
    });
    await res.reload();
    expect(res.error).toBe("bad input");
    expect(res.loading).toBe(false);
  });
});
