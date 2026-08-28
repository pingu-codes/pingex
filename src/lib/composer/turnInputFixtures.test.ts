/**
 * Golden `turn/start` inputs, exactly as the composer would send them.
 *
 * The live end-to-end suite (`src-tauri/tests/live_codex`) replays this file
 * against a real `codex app-server`, so the file must be what
 * `buildTurnInput` actually produces — not a hand-written approximation. This
 * test regenerates it and fails when the checked-in copy is stale, so a change
 * to the composer's wire format is visible in the diff and gets re-verified
 * downstream. Rewrite it with `deno task test -u src/lib/composer/turnInputFixtures.test.ts`.
 *
 * Placeholders (`${CWD}`, `${SKILL_PATH}`, `${IMAGE_PATH}`) are substituted by
 * the Rust harness with real paths from the running server.
 */
import { afterEach, describe, expect, it } from "vitest";
import { type AttachmentPart, buildTurnInput, type ComposerPart } from "$lib/composer/composerParts";
import { insertSkillChip, readParts, renderPartsWith } from "$lib/composer/richInput";
import type { TurnInputItem } from "$lib/types";

/** Relative to this file, as `toMatchFileSnapshot` wants. */
const FIXTURE_PATH = "../../../tests/fixtures/protocol/turn-inputs.json";
/** `${NAME}` placeholders the Rust harness fills in; spelled out so the linter
 * does not read them as template strings. */
const placeholder = (name: string) => `$\{${name}}`;
const CWD = placeholder("CWD");
const SKILL_PATH = placeholder("SKILL_PATH");
const IMAGE_PATH = placeholder("IMAGE_PATH");

/** One replayable case: what the harness sends and what it expects back. */
interface Fixture {
  name: string;
  /** How the parts were produced — for reading the diff, not used downstream. */
  via: "parts" | "dom" | "draft";
  input: TurnInputItem[];
  /** A token the model is asked to echo; the harness asserts it appears. */
  expectReply?: string;
}

const noopHandlers = { onRetry: () => {} };

function editor(): HTMLElement {
  const root = document.createElement("div");
  root.contentEditable = "true";
  document.body.append(root);
  return root;
}

function caretAtEnd(root: HTMLElement): Range {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  return range;
}

function attachment(overrides: Partial<AttachmentPart>): AttachmentPart {
  return {
    type: "attachment",
    id: "a1",
    filename: "pixel.png",
    mime: "image/png",
    size: 68,
    path: IMAGE_PATH,
    kind: "image",
    state: "ready",
    ...overrides,
  };
}

/** Skill chip typed into the editor with the picker, then read back. */
function skillViaDom(trailing: string): ComposerPart[] {
  const root = editor();
  root.append(document.createTextNode("Please "));
  const range = caretAtEnd(root);
  insertSkillChip(range, "e2e-skill", SKILL_PATH, "e2e-skill");
  root.append(document.createTextNode(trailing));
  return readParts(root);
}

/** Parts saved as a draft (JSON) and rendered back the way `Composer` restores them. */
function viaDraft(parts: ComposerPart[]): ComposerPart[] {
  const stored = JSON.stringify(parts);
  const root = editor();
  renderPartsWith(root, JSON.parse(stored) as ComposerPart[], noopHandlers);
  return readParts(root);
}

function fixtures(): Fixture[] {
  return [
    {
      name: "text",
      via: "parts",
      input: buildTurnInput([{ type: "text", text: "Reply with exactly PONG" }], CWD),
      expectReply: "PONG",
    },
    {
      name: "skill-chip-from-picker",
      via: "dom",
      input: buildTurnInput(skillViaDom(" use the skill and reply as it instructs."), CWD),
      expectReply: "E2E-SKILL-OK",
    },
    {
      name: "skill-restored-from-draft",
      via: "draft",
      input: buildTurnInput(
        viaDraft([
          { type: "skill", name: "e2e-skill", path: SKILL_PATH, label: "e2e-skill" },
          { type: "text", text: " follow the skill." },
        ]),
        CWD,
      ),
      expectReply: "E2E-SKILL-OK",
    },
    {
      name: "mention",
      via: "parts",
      input: buildTurnInput(
        [
          { type: "text", text: "Read " },
          { type: "mention", name: "MARKER.md", path: `${CWD}/MARKER.md` },
          { type: "text", text: " and reply with only the token it contains." },
        ],
        CWD,
      ),
      expectReply: "MENTION-OK",
    },
    {
      name: "local-image",
      via: "parts",
      input: buildTurnInput(
        [{ type: "text", text: "An image is attached. Reply with exactly IMG-OK." }, attachment({})],
        CWD,
      ),
      expectReply: "IMG-OK",
    },
    {
      name: "file-attachment",
      via: "parts",
      input: buildTurnInput(
        [
          { type: "text", text: "Read the attached file and reply with only the token it contains." },
          attachment({
            id: "f1",
            filename: "MARKER.md",
            mime: "text/markdown",
            kind: "file",
            path: `${CWD}/MARKER.md`,
          }),
        ],
        CWD,
      ),
      expectReply: "MENTION-OK",
    },
    {
      name: "staging-attachment-dropped",
      via: "parts",
      input: buildTurnInput(
        [{ type: "text", text: "Reply with exactly PONG" }, attachment({ state: "staging", path: "" })],
        CWD,
      ),
      expectReply: "PONG",
    },
  ];
}

afterEach(() => {
  document.body.replaceChildren();
  window.getSelection()?.removeAllRanges();
});

describe("turn/start input fixtures", () => {
  it("every skill item carries a path (the field the app-server rejects without)", () => {
    for (const fixture of fixtures()) {
      for (const item of fixture.input) {
        if (item.type === "skill") expect(item.path, fixture.name).toBe(SKILL_PATH);
      }
    }
  });

  it("matches the checked-in golden file (rerun with -u to regenerate)", async () => {
    const generated = `${JSON.stringify(fixtures(), null, 2)}\n`;
    await expect(generated).toMatchFileSnapshot(FIXTURE_PATH);
  });
});
