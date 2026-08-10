import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UserInputRequest } from "$lib/services/codexEvents.svelte";

const mocks = vi.hoisted(() => ({
  respondUserInput: vi.fn(),
  removeUserInputRequest: vi.fn(),
  clearUnanswered: vi.fn(),
}));

vi.mock("$lib/services/api", () => ({ respondUserInput: mocks.respondUserInput }));
vi.mock("$lib/services/codexEvents.svelte", () => ({
  removeUserInputRequest: mocks.removeUserInputRequest,
  clearUnanswered: mocks.clearUnanswered,
}));

import QuestionCard from "$lib/thread/QuestionCard.svelte";

const request: UserInputRequest = {
  requestId: 7,
  threadId: "thread-1",
  turnId: "turn-1",
  itemId: "item-1",
  questions: [
    {
      id: "approach",
      header: "Approach",
      question: "How should I implement the cache layer?",
      options: [
        { label: "In-memory", description: "Fast, but resets on restart" },
        { label: "SQLite", description: "Persistent, slightly slower" },
      ],
    },
  ],
};

describe("QuestionCard", () => {
  beforeEach(() => {
    mocks.respondUserInput.mockReset().mockResolvedValue(undefined);
    mocks.removeUserInputRequest.mockReset();
    mocks.clearUnanswered.mockReset();
  });

  it("disables Send until an option is picked or a note is typed", async () => {
    const user = userEvent.setup();
    render(QuestionCard, { request });

    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: /In-memory/ }));
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
  });

  it("sends the selected option", async () => {
    const user = userEvent.setup();
    render(QuestionCard, { request });

    await user.click(screen.getByRole("button", { name: /SQLite/ }));
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(mocks.respondUserInput).toHaveBeenCalledWith(
      7,
      { approach: { answers: ["SQLite"] } },
      {
        threadId: "thread-1",
        turnId: "turn-1",
        itemId: "item-1",
        item: {
          type: "userInputAnswered",
          id: "item-1",
          questions: request.questions,
          answers: { approach: { answers: ["SQLite"] } },
        },
      },
    );
    await waitFor(() => expect(mocks.removeUserInputRequest).toHaveBeenCalledWith(7));
  });

  it("notifies onAnswered with a synthetic item, masking secret answers", async () => {
    const user = userEvent.setup();
    const onAnswered = vi.fn();
    const secretRequest: UserInputRequest = {
      ...request,
      questions: [{ id: "token", header: "Token", question: "API token?", isSecret: true }],
    };
    render(QuestionCard, { request: secretRequest, onAnswered });

    await user.type(screen.getByPlaceholderText("Enter value…"), "hunter2");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(mocks.respondUserInput).toHaveBeenCalledWith(
      7,
      { token: { answers: ["hunter2"] } },
      expect.objectContaining({
        item: expect.objectContaining({ answers: { token: { answers: ["••••"] } } }),
      }),
    );
    await waitFor(() =>
      expect(onAnswered).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "userInputAnswered",
          id: "item-1",
          answers: { token: { answers: ["••••"] } },
        }),
      ),
    );
  });

  it("sends a note alone without any option selected", async () => {
    const user = userEvent.setup();
    render(QuestionCard, { request });

    await user.type(screen.getByPlaceholderText(/Add a note/), "Use Redis instead");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(mocks.respondUserInput).toHaveBeenCalledWith(
      7,
      { approach: { answers: ["Use Redis instead"] } },
      expect.anything(),
    );
  });

  it("appends the note after the selected option", async () => {
    const user = userEvent.setup();
    render(QuestionCard, { request });

    await user.click(screen.getByRole("button", { name: /In-memory/ }));
    await user.type(screen.getByPlaceholderText(/Add a note/), "cap it at 100MB");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(mocks.respondUserInput).toHaveBeenCalledWith(
      7,
      { approach: { answers: ["In-memory", "Note: cap it at 100MB"] } },
      expect.anything(),
    );
  });

  it("steers instead of answering, skipping every question", async () => {
    const user = userEvent.setup();
    const onAnswered = vi.fn();
    render(QuestionCard, { request, onAnswered });

    await user.type(screen.getByPlaceholderText(/Skip the questions/), "Stop and just run the tests");
    await user.click(screen.getByRole("button", { name: "Steer instead" }));

    expect(mocks.respondUserInput).toHaveBeenCalledWith(
      7,
      { approach: { answers: ["Stop and just run the tests"] } },
      expect.objectContaining({
        item: expect.objectContaining({
          type: "userInputAnswered",
          steer: "Stop and just run the tests",
          answers: {},
        }),
      }),
    );
    await waitFor(() => expect(mocks.removeUserInputRequest).toHaveBeenCalledWith(7));
  });

  it("routes a stranded question through a new turn instead of the dead request", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    render(QuestionCard, { request: { ...request, requestId: null }, onResume });

    await user.click(screen.getByRole("button", { name: /SQLite/ }));
    await user.click(screen.getByRole("button", { name: "Send as new message" }));

    expect(onResume).toHaveBeenCalledWith("How should I implement the cache layer?\nSQLite");
    // Persisted so the question stops being flagged, but never sent to a
    // request id that no longer exists.
    expect(mocks.respondUserInput).toHaveBeenCalledWith(null, expect.anything(), expect.anything());
    expect(mocks.removeUserInputRequest).not.toHaveBeenCalled();
    await waitFor(() => expect(mocks.clearUnanswered).toHaveBeenCalledWith("thread-1"));
  });

  it("dismisses a stranded question without sending anything", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    render(QuestionCard, { request: { ...request, requestId: null }, onResume });

    await user.click(screen.getByRole("button", { name: "Dismiss" }));

    expect(onResume).not.toHaveBeenCalled();
    expect(mocks.respondUserInput).toHaveBeenCalledWith(
      null,
      {},
      expect.objectContaining({ item: expect.objectContaining({ dismissed: true }) }),
    );
    await waitFor(() => expect(mocks.clearUnanswered).toHaveBeenCalledWith("thread-1"));
  });

  it("deselects an option when clicked again", async () => {
    const user = userEvent.setup();
    render(QuestionCard, { request });

    await user.click(screen.getByRole("button", { name: /In-memory/ }));
    await user.click(screen.getByRole("button", { name: /In-memory/ }));
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  });
});
