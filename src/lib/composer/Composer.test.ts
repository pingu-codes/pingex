import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Composer from "$lib/composer/Composer.svelte";
import { type ComposerPrefs, loadPrefs, loadScopedPrefs, savePrefs } from "$lib/composer/composerPrefs.svelte";
import { placeCaretBesideChip } from "$lib/composer/richInput";
import type { SubagentPolicy } from "$lib/types";

// The preview skill fixtures carry no `enabled` flag, which the picker filters on.
vi.mock("$lib/services/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/services/api")>()),
  listSkillsFor: vi.fn().mockResolvedValue([
    {
      name: "wrangler",
      path: "/skills/wrangler/SKILL.md",
      scope: "user",
      description: null,
      enabled: true,
      displayName: null,
      shortDescription: null,
    },
  ]),
}));

const textInput = (text: string) => [{ type: "text", text }];

function setup({
  prefs = {},
  ...props
}: {
  busy?: boolean;
  disabled?: boolean;
  plan?: string | null;
  cwd?: string;
  draftKey?: string;
  projectKey?: string;
  threadId?: string | null;
  history?: string[];
  subagentModelPolicy?: SubagentPolicy | null;
  subagentReasoningEffortPolicy?: SubagentPolicy | null;
  onImplementFresh?: ((input: unknown, options?: unknown) => void) | undefined;
  onGoal?: ((objective: string, edit: boolean) => Promise<boolean>) | undefined;
  threadHarness?: "codex" | "claude" | null;
  /** Stored prefs to render with; `null` leaves the store empty. */
  prefs?: Partial<ComposerPrefs> | null;
} = {}) {
  // Sending needs an explicit model and permission preset, so seed a valid pair
  // unless the test is about what happens without one.
  if (prefs) savePrefs({ ...loadPrefs(), model: "gpt-5.6-terra", permissionPreset: "auto", ...prefs });
  const onSend = vi.fn();
  const onInterrupt = vi.fn();
  const onCommand = vi.fn();
  const onSubagentPolicyChange = vi.fn();
  const onImplementFresh = "onImplementFresh" in props ? props.onImplementFresh : vi.fn();
  const onGoal = "onGoal" in props ? props.onGoal : vi.fn().mockResolvedValue(true);
  const { rerender, component } = render(Composer, {
    ...props,
    onSend,
    onInterrupt,
    onCommand,
    onSubagentPolicyChange,
    onImplementFresh,
    onGoal,
  });
  return {
    rerender,
    component,
    onSend,
    onInterrupt,
    onCommand,
    onSubagentPolicyChange,
    onImplementFresh,
    onGoal,
    textarea: screen.getByRole("textbox", {
      name: "Message Codex… (@ to attach files, / for commands)",
    }) as HTMLDivElement,
  };
}

beforeEach(() => localStorage.clear());

describe("Composer", () => {
  it("submits trimmed text with Enter and clears the input", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    await user.type(textarea, "  hello Codex  {Enter}");

    expect(onSend).toHaveBeenCalledWith(textInput("hello Codex"), expect.anything());
    expect(textarea).toBeEmptyDOMElement();
  });

  it("keeps a newline and does not submit on Shift+Enter", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    await user.type(textarea, "first{Shift>}{Enter}{/Shift}second");

    expect(onSend).not.toHaveBeenCalled();
    expect(textarea.querySelector("br")).not.toBeNull();

    await user.keyboard("{Enter}");
    expect(onSend).toHaveBeenCalledWith(textInput("first\nsecond"), expect.anything());
  });

  it("inserts exactly one break before a chip, where WebKit would write two", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "look at @util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    await user.type(textarea, "now");

    placeCaretBesideChip(textarea.querySelector("[data-mention-path]") as HTMLElement, "before");
    await fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true });

    expect(textarea.querySelectorAll("br")).toHaveLength(1);
    await user.keyboard("{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      [
        { type: "text", text: "look at \n" },
        { type: "text", text: "[utils.ts](src/lib/utils.ts)" },
        { type: "text", text: "now" },
      ],
      expect.anything(),
    );
  });

  it("undoes and redoes typing with Cmd+Z / Cmd+Shift+Z", async () => {
    const user = userEvent.setup();
    const { textarea } = setup();
    await user.type(textarea, "hello");
    // Past the coalescing window, so the second run is its own undo entry.
    await new Promise((resolve) => setTimeout(resolve, 600));
    await user.type(textarea, " world");
    expect(textarea.textContent).toBe("hello world");

    await user.keyboard("{Meta>}z{/Meta}");
    expect(textarea.textContent).toBe("hello");
    await user.keyboard("{Meta>}z{/Meta}");
    expect(textarea.textContent).toBe("");
    await user.keyboard("{Meta>}{Shift>}z{/Shift}{/Meta}");
    expect(textarea.textContent).toBe("hello");
    await user.keyboard("{Control>}y{/Control}");
    expect(textarea.textContent).toBe("hello world");
  });

  it("recalls earlier messages with ArrowUp and returns to the draft with ArrowDown", async () => {
    const user = userEvent.setup();
    const { textarea } = setup({ history: ["first", "second"] });

    await user.type(textarea, "draft");
    await user.keyboard("{ArrowUp}");
    expect(textarea.textContent).toBe("second");
    await user.keyboard("{ArrowUp}");
    expect(textarea.textContent).toBe("first");
    await user.keyboard("{ArrowUp}");
    expect(textarea.textContent).toBe("first");
    await user.keyboard("{ArrowDown}");
    expect(textarea.textContent).toBe("second");
    await user.keyboard("{ArrowDown}");
    expect(textarea.textContent).toBe("draft");
  });

  it("rejects whitespace-only input", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    await user.type(textarea, "   ");
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
    await user.keyboard("{Enter}");

    expect(onSend).not.toHaveBeenCalled();
  });

  it("disables editing and sending when disabled", () => {
    const { textarea } = setup({ disabled: true });

    expect(textarea).toHaveAttribute("contenteditable", "false");
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
  });

  it("shows the stop action while busy and still hands the message to onSend for queueing", async () => {
    const user = userEvent.setup();
    const { onInterrupt, onSend, textarea } = setup({ busy: true });

    await user.type(textarea, "still running{Enter}");
    expect(onSend).toHaveBeenCalledWith(textInput("still running"), expect.anything());
    await user.click(screen.getByRole("button", { name: "Stop" }));

    expect(onInterrupt).toHaveBeenCalledOnce();
  });

  it("selects a model and effort from the popover and sends them as turn options", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    await user.click(screen.getByRole("button", { name: "Select model and effort" }));
    await user.click(await screen.findByText("GPT-5.2"));
    await user.click(screen.getByRole("button", { name: "high" }));
    await user.keyboard("{Escape}");

    await user.type(textarea, "use the big model{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      textInput("use the big model"),
      expect.objectContaining({ model: "gpt-5.2", effort: "high" }),
    );
  });

  it("picks the harness for a draft from the menu", async () => {
    const user = userEvent.setup();
    setup({ projectKey: "/repo" });

    const trigger = screen.getByRole("button", { name: "Choose harness" });
    expect(trigger).toHaveTextContent("Codex");
    await user.click(trigger);
    await user.click(await screen.findByRole("menuitemradio", { name: "Claude Code" }));

    await waitFor(() => expect(trigger).toHaveTextContent("Claude Code"));
    expect(loadScopedPrefs("/repo", null).harness).toBe("claude");
    expect(loadScopedPrefs("/repo", null).model).toBeNull();
  });

  it("hides the harness menu on an existing thread", () => {
    setup({ threadId: "thr_1" });
    expect(screen.queryByRole("button", { name: "Choose harness" })).toBeNull();
  });

  describe("invalid settings", () => {
    const blocked = async (props: Parameters<typeof setup>[0], reason: string) => {
      const user = userEvent.setup();
      const { onSend, textarea } = setup(props);

      await user.type(textarea, "go{Enter}");

      expect(onSend).not.toHaveBeenCalled();
      expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
      expect(await screen.findByText(reason)).toBeInTheDocument();
    };

    it("refuses to send with no model chosen", async () => {
      await blocked({ prefs: { model: null } }, "Choose a model");
    });

    it("refuses to send with no permission preset chosen", async () => {
      await blocked({ prefs: { permissionPreset: null } }, "Choose a permission mode");
    });

    it("refuses to send when subagents are allowed no model", async () => {
      await blocked({ subagentModelPolicy: { allowed: [] } }, "Pick at least one subagent model");
    });

    it("refuses to send when subagents are allowed no effort level", async () => {
      await blocked({ subagentReasoningEffortPolicy: { allowed: [] } }, "Pick at least one subagent effort level");
    });

    it("hides the plan actions, which bypass the send button", async () => {
      const user = userEvent.setup();
      const { onSend } = setup({ plan: "1. do things", prefs: { permissionPreset: null } });

      await user.click(screen.getByRole("button", { name: "Toggle plan mode" }));

      expect(screen.queryByRole("button", { name: "Implement the plan" })).not.toBeInTheDocument();
      expect(onSend).not.toHaveBeenCalled();
    });

    it("sends again once the missing choice is made", async () => {
      const user = userEvent.setup();
      const { onSend, textarea } = setup({ prefs: { permissionPreset: null } });

      await user.click(screen.getByRole("button", { name: "Set permissions level" }));
      await user.click(screen.getByText("Read Only"));
      await user.type(textarea, "now go{Enter}");

      expect(onSend).toHaveBeenCalledWith(textInput("now go"), expect.anything());
    });
  });

  it("remembers the subagent policy for the next composer in the project", async () => {
    const user = userEvent.setup();
    setup({ projectKey: "/repo" });

    await user.click(screen.getByRole("button", { name: "Set subagent models and effort" }));
    await user.click(screen.getAllByRole("checkbox")[1]);

    expect(loadScopedPrefs("/repo", null).subagentModelPolicy).toEqual({ allowed: ["gpt-5.2-codex"] });
  });

  it("remembers the separate-processes choice", async () => {
    const user = userEvent.setup();
    setup({ projectKey: "/repo" });

    await user.click(screen.getByRole("button", { name: "Set subagent models and effort" }));
    await user.click(screen.getByRole("button", { name: "On" }));

    expect(loadScopedPrefs("/repo", null).appSubagents).toBe(true);
  });

  it("seeds a draft thread with the remembered subagent policy", async () => {
    const { onSubagentPolicyChange } = setup({
      projectKey: "/repo",
      prefs: { subagentModelPolicy: { allowed: ["gpt-5.2"] } },
    });

    await waitFor(() => expect(onSubagentPolicyChange).toHaveBeenCalledWith({ allowed: ["gpt-5.2"] }, null));
  });

  it("keeps subagent inclusion separate from the parent model picker", async () => {
    const user = userEvent.setup();
    const { onSubagentPolicyChange } = setup();

    await user.click(screen.getByRole("button", { name: "Select model and effort" }));
    await screen.findByText("GPT-5.2");
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");

    await user.click(screen.getByRole("button", { name: "Set subagent models and effort" }));
    const checkbox = screen.getAllByRole("checkbox")[1];
    await user.click(checkbox);

    expect(onSubagentPolicyChange).toHaveBeenCalledWith({ allowed: ["gpt-5.2-codex"] }, null);
  });

  it("sends the active subagent policies independently of the parent model", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup({
      subagentModelPolicy: { allowed: ["gpt-5.2"] },
      subagentReasoningEffortPolicy: { allowed: ["high"] },
    });

    await user.type(textarea, "delegate this{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      textInput("delegate this"),
      expect.objectContaining({
        subagentModelPolicy: { allowed: ["gpt-5.2"] },
        subagentReasoningEffortPolicy: { allowed: ["high"] },
      }),
    );
  });

  it("selects a permission preset and sends approval/sandbox overrides", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    await user.click(screen.getByRole("button", { name: "Set permissions level" }));
    await user.click(screen.getByText("Full Access"));

    await user.type(textarea, "go wild{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      textInput("go wild"),
      expect.objectContaining({ approvalPolicy: "never", sandboxMode: "danger-full-access" }),
    );
  });

  it("attaches files as mentions via the @ picker", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "@util");
    const hit = await screen.findByRole("option", { name: /utils\.ts/ });
    await user.click(hit);

    expect(screen.getByText("utils.ts")).toBeInTheDocument();
    await user.type(textarea, "explain this{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      [
        { type: "text", text: "[utils.ts](src/lib/utils.ts)" },
        { type: "text", text: "explain this" },
      ],
      expect.anything(),
    );
  });

  it("sends a picked directory with the trailing slash Codex uses for folders", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "@lib");
    await user.click(await screen.findByRole("option", { name: /^lib/ }));

    await user.type(textarea, "summarise{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      [
        { type: "text", text: "[lib](src/lib/)" },
        { type: "text", text: "summarise" },
      ],
      expect.anything(),
    );
  });

  it("removes an inline mention chip with Backspace", async () => {
    // Chips carry no × button: Backspace beside one is the only way out.
    const user = userEvent.setup();
    const { textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "@util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    const chip = textarea.querySelector("[data-mention-path]") as HTMLElement;
    expect(chip).not.toBeNull();
    expect(screen.queryByRole("button", { name: /^Remove/ })).toBeNull();

    placeCaretBesideChip(chip, "after");
    await fireEvent.keyDown(textarea, { key: "Backspace" });

    expect(textarea.querySelector("[data-mention-path]")).toBeNull();
  });

  it("keeps a chip reachable by arrow keys after the browser wraps it in its own line block", async () => {
    const user = userEvent.setup();
    const { textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "@util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    const chip = textarea.querySelector("[data-mention-path]") as HTMLElement;

    // What WebKit does to a newline typed before a chip: the chip lands in a
    // <div> line of its own, where it is no longer a sibling of the caret.
    const block = document.createElement("div");
    chip.before(block);
    block.append(chip);
    textarea.prepend(document.createTextNode("one"));
    await fireEvent.input(textarea);

    const flattened = textarea.querySelector("[data-mention-path]") as HTMLElement;
    expect(textarea.querySelector("div")).toBeNull();
    expect(flattened.parentNode).toBe(textarea);

    placeCaretBesideChip(flattened, "after");
    await fireEvent.keyDown(textarea, { key: "ArrowLeft" });

    const range = window.getSelection()?.getRangeAt(0);
    expect(range?.startContainer).toBe(textarea);
    expect(range?.startOffset).toBe([...textarea.childNodes].indexOf(flattened));
  });

  it("deletes the whole line on Cmd+Backspace rather than just the chip beside the caret", async () => {
    const user = userEvent.setup();
    const { textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "see @util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));

    placeCaretBesideChip(textarea.querySelector("[data-mention-path]") as HTMLElement, "after");
    await fireEvent.keyDown(textarea, { key: "Backspace", metaKey: true });

    expect(textarea.querySelector("[data-mention-path]")).toBeNull();
    expect(textarea).toHaveTextContent("");
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
  });

  it("closes the mention picker when Cmd+Backspace takes the line out from under it", async () => {
    const user = userEvent.setup();
    const { textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "see @util");
    await screen.findByRole("option", { name: /utils\.ts/ });
    await fireEvent.keyDown(textarea, { key: "Backspace", metaKey: true });

    expect(textarea).toHaveTextContent("");
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());

    // A pick after the line is gone must not land a chip at the start.
    await fireEvent.keyDown(textarea, { key: "Enter" });
    expect(textarea.querySelector("[data-mention-path]")).toBeNull();
  });

  it("keeps an existing chip usable after Cmd+Backspace with the picker open", async () => {
    const user = userEvent.setup();
    const { textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "see @util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    await user.type(textarea, "{Shift>}{Enter}{/Shift}then @ut");
    await screen.findByRole("option", { name: /utils\.ts/ });

    await fireEvent.keyDown(textarea, { key: "Backspace", metaKey: true });
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());

    const chips = textarea.querySelectorAll("[data-mention-path]");
    expect(chips).toHaveLength(1);
    const chip = chips[0] as HTMLElement;
    expect(chip.parentNode).toBe(textarea);
    expect(chip.previousSibling?.nodeType).toBe(Node.TEXT_NODE);
    expect(chip.nextSibling?.nodeType).toBe(Node.TEXT_NODE);

    placeCaretBesideChip(chip, "after");
    await fireEvent.keyDown(textarea, { key: "Backspace" });
    expect(textarea.querySelector("[data-mention-path]")).toBeNull();
  });

  it("closes the skill picker when Ctrl+Backspace deletes the trigger", async () => {
    const user = userEvent.setup();
    const { textarea } = setup({ cwd: "/proj" });

    await user.type(textarea, "see @util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    await user.type(textarea, " $wran");
    await screen.findByRole("option", { name: /wrangler/ });

    // Ctrl+Backspace word-deletes through the parts model once a chip is in
    // reach; here the word beside the caret is plain text, so only the picker
    // bookkeeping matters: Cmd+Backspace takes the whole line, chip included.
    await fireEvent.keyDown(textarea, { key: "Backspace", ctrlKey: true, metaKey: true });
    expect(textarea).toHaveTextContent("");
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
    await fireEvent.keyDown(textarea, { key: "Enter" });
    expect(textarea.querySelector("[data-skill-name]")).toBeNull();
  });

  it("sends collaborationMode plan when plan mode is toggled on", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    const toggle = screen.getByRole("button", { name: "Toggle plan mode" });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");

    await user.type(textarea, "plan this out{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      textInput("plan this out"),
      expect.objectContaining({ collaborationMode: expect.objectContaining({ mode: "plan" }) }),
    );
  });

  it("sends collaborationMode default after plan mode is toggled off, so the thread leaves plan mode", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    const toggle = screen.getByRole("button", { name: "Toggle plan mode" });
    await user.click(toggle);
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    await user.type(textarea, "just do it{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      textInput("just do it"),
      expect.objectContaining({ collaborationMode: expect.objectContaining({ mode: "default" }) }),
    );
  });

  it("offers plan actions once a plan exists in plan mode", async () => {
    const user = userEvent.setup();
    const { onSend } = setup({ plan: "1. do things" });

    await user.click(screen.getByRole("button", { name: "Toggle plan mode" }));
    const implement = screen.getByRole("button", { name: "Implement the plan" });
    await user.click(implement);

    expect(onSend).toHaveBeenCalledWith(
      textInput("Implement the plan."),
      expect.objectContaining({ collaborationMode: expect.objectContaining({ mode: "default" }) }),
    );
    expect(screen.getByRole("button", { name: "Toggle plan mode" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("button", { name: "Implement the plan" })).not.toBeInTheDocument();
  });

  it("hands the plan to a fresh thread with Clear context & implement", async () => {
    const user = userEvent.setup();
    const { onSend, onImplementFresh } = setup({ plan: "1. do things" });

    await user.click(screen.getByRole("button", { name: "Toggle plan mode" }));
    await user.click(screen.getByRole("button", { name: "Clear context and implement the plan" }));

    expect(onSend).not.toHaveBeenCalled();
    expect(onImplementFresh).toHaveBeenCalledWith(
      [{ type: "text", text: expect.stringContaining("1. do things") }],
      expect.objectContaining({ collaborationMode: expect.objectContaining({ mode: "default" }) }),
    );
    expect(screen.getByRole("button", { name: "Toggle plan mode" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("button", { name: "Implement the plan" })).not.toBeInTheDocument();
  });

  it("omits the fresh-thread action when the thread cannot host one", async () => {
    const user = userEvent.setup();
    setup({ plan: "1. do things", onImplementFresh: undefined });

    await user.click(screen.getByRole("button", { name: "Toggle plan mode" }));

    expect(screen.getByRole("button", { name: "Implement the plan" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Clear context and implement the plan" })).not.toBeInTheDocument();
  });

  it("keeps plan mode on and dismisses the plan actions with Keep planning", async () => {
    const user = userEvent.setup();
    const { onSend } = setup({ plan: "1. do things" });

    await user.click(screen.getByRole("button", { name: "Toggle plan mode" }));
    await user.click(screen.getByRole("button", { name: "Keep planning" }));

    expect(onSend).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Implement the plan" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Toggle plan mode" })).toHaveAttribute("aria-pressed", "true");
  });

  it("shows the slash command picker for a leading slash and filters it", async () => {
    const user = userEvent.setup();
    const { textarea } = setup();

    await user.type(textarea, "/");
    expect(await screen.findByRole("listbox", { name: "Slash commands" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /\/fork/ })).toBeInTheDocument();

    await user.type(textarea, "arch");
    expect(screen.queryByRole("option", { name: /\/fork/ })).not.toBeInTheDocument();
    expect(screen.getByRole("option", { name: /\/archive/ })).toBeInTheDocument();
  });

  it("does not open the picker for a slash mid-message", async () => {
    const user = userEvent.setup();
    const { textarea } = setup();

    await user.type(textarea, "look at src/lib");
    expect(screen.queryByRole("listbox", { name: "Slash commands" })).not.toBeInTheDocument();
  });

  it("runs a thread command via Enter without submitting a message", async () => {
    const user = userEvent.setup();
    const { onCommand, onSend, textarea } = setup();

    await user.type(textarea, "/fork{Enter}");

    expect(onCommand).toHaveBeenCalledWith("fork", "", "/fork");
    expect(onSend).not.toHaveBeenCalled();
    expect(textarea).toBeEmptyDOMElement();
  });

  it("passes an argument typed after a command, with the picker closed", async () => {
    const user = userEvent.setup();
    const { onCommand, onSend, textarea } = setup();

    await user.type(textarea, "/rename Tauri bridge");
    // The space ends command selection, so the picker gets out of the way.
    expect(screen.queryByRole("listbox", { name: "Slash commands" })).not.toBeInTheDocument();

    await user.type(textarea, "{Enter}");
    expect(onCommand).toHaveBeenCalledWith("rename", "Tauri bridge", "/rename Tauri bridge");
    expect(onSend).not.toHaveBeenCalled();
  });

  it("sends an unknown slash command as an ordinary message", async () => {
    const user = userEvent.setup();
    const { onCommand, onSend, textarea } = setup();

    await user.type(textarea, "/notacommand{Enter}");

    expect(onCommand).not.toHaveBeenCalled();
    expect(onSend).toHaveBeenCalledWith([{ type: "text", text: "/notacommand" }], expect.anything());
  });

  it("toggles plan mode with the /plan command", async () => {
    const user = userEvent.setup();
    const { onSend, textarea } = setup();

    await user.type(textarea, "/plan{Enter}");
    expect(screen.getByRole("button", { name: "Toggle plan mode" })).toHaveAttribute("aria-pressed", "true");

    await user.type(textarea, "plan it{Enter}");
    expect(onSend).toHaveBeenCalledWith(
      textInput("plan it"),
      expect.objectContaining({ collaborationMode: expect.objectContaining({ mode: "plan" }) }),
    );
  });

  it("closes the slash picker with Escape and keeps the text", async () => {
    const user = userEvent.setup();
    const { onCommand, textarea } = setup();

    await user.type(textarea, "/fo");
    await user.keyboard("{Escape}");

    expect(screen.queryByRole("listbox", { name: "Slash commands" })).not.toBeInTheDocument();
    expect(onCommand).not.toHaveBeenCalled();
    expect(textarea).toHaveTextContent("/fo");
  });

  it("closes the slash picker when the click lands outside the composer", async () => {
    const user = userEvent.setup();
    const { onCommand, textarea } = setup();

    await user.type(textarea, "/fo");
    expect(await screen.findByRole("listbox", { name: "Slash commands" })).toBeInTheDocument();

    await user.click(document.body);

    expect(screen.queryByRole("listbox", { name: "Slash commands" })).not.toBeInTheDocument();
    expect(onCommand).not.toHaveBeenCalled();
    expect(textarea).toHaveTextContent("/fo");
  });

  it("keeps the picker open long enough for a click on one of its rows to land", async () => {
    const user = userEvent.setup();
    const { onCommand, textarea } = setup();

    await user.type(textarea, "/fork");
    await user.click(await screen.findByRole("option", { name: /\/fork/ }));

    expect(onCommand).toHaveBeenCalledWith("fork", "", "/fork");
    expect(screen.queryByRole("listbox", { name: "Slash commands" })).not.toBeInTheDocument();
  });

  // Outside Tauri the draft API persists to localStorage under pingex-draft:<project>.
  describe("per-project drafts", () => {
    const draftKey = "/tmp/project";
    const stored = () => localStorage.getItem(`pingex-draft:${draftKey}`);

    it("restores the project's saved draft on mount", async () => {
      localStorage.setItem(`pingex-draft:${draftKey}`, JSON.stringify([{ type: "text", text: "unfinished thought" }]));
      const { textarea } = setup({ draftKey });

      await waitFor(() => expect(textarea).toHaveTextContent("unfinished thought"));
    });

    it("saves typed text as the project's draft", async () => {
      const user = userEvent.setup();
      const { textarea } = setup({ draftKey });

      await user.type(textarea, "half a message");

      await waitFor(() => expect(stored()).toBe(JSON.stringify([{ type: "text", text: "half a message" }])), {
        timeout: 2000,
      });
    });

    it("clears the draft once the message is sent", async () => {
      localStorage.setItem(`pingex-draft:${draftKey}`, JSON.stringify([{ type: "text", text: "ready to go" }]));
      const user = userEvent.setup();
      const { onSend, textarea } = setup({ draftKey });
      await waitFor(() => expect(textarea).toHaveTextContent("ready to go"));

      textarea.focus();
      await user.keyboard("{Enter}");

      expect(onSend).toHaveBeenCalledWith(textInput("ready to go"), expect.anything());
      await waitFor(() => expect(stored()).toBeNull());
    });
  });

  describe("per-thread prefs", () => {
    const planButton = () => screen.getByRole("button", { name: "Toggle plan mode" });
    const store = () => JSON.parse(localStorage.getItem("pingex-composer-prefs") ?? "{}");
    const seed = (scope: "projects" | "threads", key: string, planMode: boolean) =>
      localStorage.setItem(
        "pingex-composer-prefs",
        JSON.stringify({ version: 2, fallback: null, projects: {}, threads: {}, [scope]: { [key]: { planMode } } }),
      );

    it("restores the thread's own plan mode", () => {
      seed("threads", "thread-a", true);
      setup({ projectKey: "/repo", threadId: "thread-a" });

      expect(planButton()).toHaveAttribute("aria-pressed", "true");
    });

    it("seeds a thread with no prefs from the project's last-used", () => {
      seed("projects", "/repo", true);
      setup({ projectKey: "/repo", threadId: "thread-new" });

      expect(planButton()).toHaveAttribute("aria-pressed", "true");
    });

    it("does not leak one thread's choices into another project", () => {
      seed("threads", "thread-a", true);
      setup({ projectKey: "/other", threadId: "thread-b" });

      expect(planButton()).toHaveAttribute("aria-pressed", "false");
    });

    it("remembers a choice for the thread and as the project's last-used", async () => {
      const user = userEvent.setup();
      setup({ projectKey: "/repo", threadId: "thread-a" });

      await user.click(planButton());

      expect(store().threads["thread-a"].planMode).toBe(true);
      expect(store().projects["/repo"].planMode).toBe(true);
    });

    it("carries draft-thread choices into the thread it becomes", async () => {
      const user = userEvent.setup();
      const { rerender } = setup({ projectKey: "/repo", threadId: null });
      await user.click(planButton());

      await rerender({ projectKey: "/repo", threadId: "thread-1" });

      await waitFor(() => expect(store().threads["thread-1"].planMode).toBe(true));
      expect(planButton()).toHaveAttribute("aria-pressed", "true");
    });

    it("keeps the thread's prefs after the message is sent", async () => {
      const user = userEvent.setup();
      const { textarea } = setup({ projectKey: "/repo", threadId: "thread-a" });
      await user.click(planButton());

      await user.type(textarea, "go{Enter}");

      expect(store().threads["thread-a"].planMode).toBe(true);
      expect(planButton()).toHaveAttribute("aria-pressed", "true");
    });
  });
});

describe("goal mode", () => {
  const toggle = () => screen.getByRole("button", { name: "Toggle goal mode" });

  it("sets the goal from the whole line, chips flattened, without starting a turn", async () => {
    const user = userEvent.setup();
    const { onSend, onGoal, textarea } = setup({ cwd: "/proj" });

    await user.click(toggle());
    expect(toggle()).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Set goal" })).toBeInTheDocument();

    await user.type(textarea, "keep @util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    await user.type(textarea, " green with $wran");
    await user.click(await screen.findByRole("option", { name: /wrangler/ }));
    await user.keyboard("{Enter}");

    expect(onSend).not.toHaveBeenCalled();
    expect(onGoal).toHaveBeenCalledWith(
      "keep [utils.ts](src/lib/utils.ts) green with $wrangler (skill: /skills/wrangler/SKILL.md)",
      false,
    );
    expect(textarea).toBeEmptyDOMElement();
    await waitFor(() => expect(toggle()).toHaveAttribute("aria-pressed", "false"));
  });

  it("gives the line back and stays in goal mode when the goal was not set", async () => {
    const user = userEvent.setup();
    const onGoal = vi.fn().mockResolvedValue(false);
    const { onSend, textarea } = setup({ cwd: "/proj", onGoal });

    await user.click(toggle());
    await user.type(textarea, "ship @util");
    await user.click(await screen.findByRole("option", { name: /utils\.ts/ }));
    await user.keyboard("{Enter}");

    expect(onSend).not.toHaveBeenCalled();
    await waitFor(() => expect(textarea).toHaveTextContent("ship"));
    expect(textarea.querySelector("[data-mention-path]")).not.toBeNull();
    expect(toggle()).toHaveAttribute("aria-pressed", "true");
  });

  it("toggles with a bare /goal, while /goal <objective> and /goal clear still run as commands", async () => {
    const user = userEvent.setup();
    const { onCommand, textarea } = setup();

    await user.type(textarea, "/goal{Enter}");
    expect(toggle()).toHaveAttribute("aria-pressed", "true");
    expect(onCommand).not.toHaveBeenCalled();

    await user.type(textarea, "/goal clear{Enter}");
    expect(onCommand).toHaveBeenCalledWith("goal", "clear", "/goal clear");
    await user.type(textarea, "/goal ship it{Enter}");
    expect(onCommand).toHaveBeenCalledWith("goal", "ship it", "/goal ship it");
  });

  it("is absent on a Claude thread and without a goal handler", () => {
    setup({ threadHarness: "claude", threadId: "t1" });
    expect(screen.queryByRole("button", { name: "Toggle goal mode" })).toBeNull();
  });

  it("edits an existing goal in the composer, parking the draft until confirm or cancel", async () => {
    const user = userEvent.setup();
    const { onGoal, textarea, component } = setup({ cwd: "/proj" });

    await user.type(textarea, "half a thought");
    component.startGoalEdit("keep the build green");
    await waitFor(() => expect(textarea).toHaveTextContent("keep the build green"));
    expect(screen.getByTestId("goal-edit-bar")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update goal" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Cancel goal edit" }));
    expect(screen.queryByTestId("goal-edit-bar")).toBeNull();
    expect(textarea).toHaveTextContent("half a thought");
    expect(toggle()).toHaveAttribute("aria-pressed", "false");
    expect(onGoal).not.toHaveBeenCalled();

    component.startGoalEdit("keep the build green");
    await waitFor(() => expect(textarea).toHaveTextContent("keep the build green"));
    await user.click(textarea);
    await user.keyboard(" and fast{Enter}");
    expect(onGoal).toHaveBeenCalledWith("keep the build green and fast", true);
    await waitFor(() => expect(screen.queryByTestId("goal-edit-bar")).toBeNull());
    expect(textarea).toHaveTextContent("half a thought");
  });

  it("abandons a goal edit on Escape", async () => {
    const user = userEvent.setup();
    const { onGoal, textarea, component } = setup();

    component.startGoalEdit("keep the build green");
    await waitFor(() => expect(textarea).toHaveTextContent("keep the build green"));
    await user.keyboard("{Escape}");

    expect(screen.queryByTestId("goal-edit-bar")).toBeNull();
    expect(textarea).toBeEmptyDOMElement();
    expect(onGoal).not.toHaveBeenCalled();
  });
});
