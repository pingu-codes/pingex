import { tick } from "svelte";

/**
 * A message edited in place: the draft, its textarea, and the keys every inline
 * editor shares. Enter saves, Shift+Enter breaks the line and Escape abandons
 * the edit; neither fires mid-IME-composition. `save` only hears about a
 * non-empty draft that differs from what `original` returns.
 */
export class InlineEdit {
  draft = $state<string | null>(null);
  textarea = $state<HTMLTextAreaElement | null>(null);
  readonly #original: () => string;
  readonly #save: (text: string) => void;

  constructor(original: () => string, save: (text: string) => void) {
    this.#original = original;
    this.#save = save;
  }

  get editing(): boolean {
    return this.draft !== null;
  }

  start = async (): Promise<void> => {
    this.draft = this.#original();
    await tick();
    const element = this.textarea;
    element?.focus();
    element?.setSelectionRange(element.value.length, element.value.length);
  };

  cancel = (): void => {
    this.draft = null;
  };

  submit = (): void => {
    const text = this.draft?.trim() ?? "";
    this.draft = null;
    if (text && text !== this.#original()) this.#save(text);
  };

  /** Escape is claimed so the composer never reads it as "interrupt the turn". */
  onkeydown = (event: KeyboardEvent): void => {
    if (event.isComposing) return;
    if (event.key === "Escape") {
      event.preventDefault();
      this.cancel();
    } else if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      this.submit();
    }
  };
}
