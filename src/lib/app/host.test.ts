import { describe, expect, it } from "vitest";
import {
  hostFromOption,
  hostKey,
  hostLabel,
  hostOverride,
  hostToOption,
  NATIVE_HOST,
  sameHost,
  wslHost,
} from "$lib/app/host";

describe("host helpers", () => {
  it("labels only WSL hosts", () => {
    expect(hostLabel(NATIVE_HOST)).toBe("");
    expect(hostLabel(null)).toBe("");
    expect(hostLabel(wslHost("Ubuntu"))).toBe("WSL · Ubuntu");
  });

  it("keys hosts so the same path on two hosts stays distinct", () => {
    expect(hostKey(NATIVE_HOST)).toBe("native");
    expect(hostKey(wslHost("Ubuntu"))).toBe("wsl:Ubuntu");
    expect(sameHost(null, NATIVE_HOST)).toBe(true);
    expect(sameHost(wslHost("Ubuntu"), wslHost("Debian"))).toBe(false);
  });

  it("round-trips through a select option", () => {
    expect(hostToOption(NATIVE_HOST)).toBe("native");
    expect(hostFromOption("native")).toEqual(NATIVE_HOST);
    expect(hostFromOption("")).toEqual(NATIVE_HOST);
    expect(hostFromOption("Ubuntu")).toEqual(wslHost("Ubuntu"));
    expect(hostToOption(hostFromOption("Ubuntu"))).toBe("Ubuntu");
  });

  it("stores only a WSL host as an override", () => {
    expect(hostOverride(NATIVE_HOST)).toBeNull();
    expect(hostOverride(wslHost("Ubuntu"))).toEqual(wslHost("Ubuntu"));
  });
});
