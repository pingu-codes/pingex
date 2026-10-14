import { fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import SettingsView from "$lib/layout/SettingsView.svelte";
import { commands } from "$lib/services/homeRouting";
import { closeSettings, openSettings } from "$lib/services/settingsNav.svelte";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return { ...actual, commands: { ...actual.commands, writeAgentSettings: vi.fn(actual.commands.writeAgentSettings) } };
});

function setup() {
  const onClose = vi.fn();
  render(SettingsView, {
    account: { label: "ciaran@example.com", plan: "Pro", kind: "chatgpt" },
    codexHome: "~/.codex-personal",
    codexBinary: "codex",
    onClose,
  });
  return { onClose };
}

describe("SettingsView", () => {
  it("filters the navigation as you search", async () => {
    const user = userEvent.setup();
    setup();
    // All sections present initially.
    expect(screen.getAllByTestId("settings-nav-item").length).toBeGreaterThan(5);
    await user.type(screen.getByLabelText("Search settings"), "sandbox");
    const items = screen.getAllByTestId("settings-nav-item");
    expect(items).toHaveLength(1);
    expect(items[0]).toHaveTextContent("Agent");
  });

  it("renders a config control with its value and source badge", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("tab", { name: /Agent/ }));

    // Model is set in the preview config, so it shows the config.toml source.
    const model = await screen.findByRole("textbox", { name: "Model" });
    expect(model).toHaveValue("gpt-5.6-luna");

    const modelRow = model.closest("[data-testid='config-control']") as HTMLElement;
    expect(within(modelRow).getByText("config.toml")).toBeInTheDocument();

    // Approval policy is unset in the preview, so it shows the Default source.
    const approvalRow = document.querySelector(
      "[data-testid='config-control'][data-key='approval_policy']",
    ) as HTMLElement;
    expect(within(approvalRow).getByText("Default")).toBeInTheDocument();
  });

  it("toggles the message log preference from the Advanced section", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("tab", { name: /Advanced/ }));

    const toggle = screen.getByRole("switch", { name: "Message log" });
    expect(toggle).not.toBeChecked();
    // The log itself now lives in the thread's right sidebar, not inline here.
    expect(screen.queryByTestId("message-log")).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toBeChecked();
    expect(screen.queryByTestId("message-log")).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).not.toBeChecked();
  });

  it("closes via the header button", async () => {
    const user = userEvent.setup();
    const { onClose } = setup();
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("refuses an emptied or out-of-range agent limit instead of saving it", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("tab", { name: /Agent/ }));
    const enable = await screen.findByRole("checkbox", { name: /Run subagents as separate processes/ });
    if (!(enable as HTMLInputElement).checked) await user.click(enable);
    vi.mocked(commands.writeAgentSettings).mockClear();

    const timeLimit = screen.getByRole("spinbutton", { name: "Agent time limit in seconds" }) as HTMLInputElement;
    const stored = timeLimit.value;
    await fireEvent.change(timeLimit, { target: { value: "" } });
    expect(timeLimit.value).toBe(stored);
    await fireEvent.change(timeLimit, { target: { value: "5" } });
    expect(timeLimit.value).toBe(stored);
    expect(commands.writeAgentSettings).not.toHaveBeenCalled();

    await fireEvent.change(timeLimit, { target: { value: "600" } });
    expect(commands.writeAgentSettings).toHaveBeenCalledWith(expect.objectContaining({ timeoutSeconds: 600 }));
  });

  it("opens at the section a settingsNav request names", () => {
    openSettings("usage");
    setup();
    expect(screen.getByRole("tab", { name: /Usage/ })).toHaveAttribute("aria-selected", "true");
    closeSettings();
    openSettings("general");
    closeSettings();
  });

  it("lets Escape cancel shortcut recording without closing", async () => {
    const user = userEvent.setup();
    const { onClose } = setup();
    await user.click(screen.getByRole("tab", { name: /Keyboard/ }));
    await user.click(await screen.findByRole("button", { name: /CmdOrCtrl|Ctrl|Alt|Shift/ }));
    expect(screen.getByRole("button", { name: "Press keys…" })).toBeInTheDocument();

    await user.click(document.body);
    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Press keys…" })).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });
});
