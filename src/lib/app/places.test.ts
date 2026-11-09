// @vitest-environment node
import { describe, expect, it } from "vitest";
import { forgetPlaces, place } from "$lib/app/places";

describe("places", () => {
  it("hands back the record a page left", () => {
    place("a", () => ({ scrollTop: 0 })).scrollTop = 120;
    expect(place("a", () => ({ scrollTop: 0 })).scrollTop).toBe(120);
    expect(place("b", () => ({ scrollTop: 0 })).scrollTop).toBe(0);
  });

  it("forgets the least recently opened first", () => {
    place("oldest", () => ({ kept: true })).kept = false;
    place("opened again", () => ({ kept: true })).kept = false;
    for (let index = 0; index < 98; index++) place(`filler ${index}`, () => ({}));
    place("opened again", () => ({ kept: true }));
    place("one too many", () => ({}));

    expect(place("opened again", () => ({ kept: true })).kept).toBe(false);
    expect(place("oldest", () => ({ kept: true })).kept).toBe(true);
  });

  it("forgets everything on request", () => {
    place("a", () => ({ scrollTop: 0 })).scrollTop = 120;
    forgetPlaces();
    expect(place("a", () => ({ scrollTop: 0 })).scrollTop).toBe(0);
  });
});
