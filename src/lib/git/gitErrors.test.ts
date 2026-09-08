import { describe, expect, it } from "vitest";
import { hintFor, parseGitError } from "./gitErrors";

describe("parseGitError", () => {
  it("splits a classified prefix from the message", () => {
    const error = parseGitError(new Error("nonFastForward: The remote has commits you do not have."));
    expect(error.kind).toBe("nonFastForward");
    expect(error.message).toBe("The remote has commits you do not have.");
    expect(error.detail).toBeNull();
  });

  it("keeps hook output as detail", () => {
    const error = parseGitError("hookRejected: A Git hook rejected the operation.\npre-commit: lint failed\nsee above");
    expect(error.kind).toBe("hookRejected");
    expect(error.detail).toBe("pre-commit: lint failed\nsee above");
  });

  it("treats unknown prefixes and plain strings as other", () => {
    expect(parseGitError("weird: thing").kind).toBe("other");
    expect(parseGitError("weird: thing").message).toBe("weird: thing");
    expect(parseGitError("other: Could not push").message).toBe("Could not push");
    expect(parseGitError(undefined).message).toBe("Git operation failed");
  });

  it("offers a hint for actionable kinds only", () => {
    expect(hintFor("dirtyTree")).toContain("switch anyway");
    expect(hintFor("other")).toBeNull();
  });
});
