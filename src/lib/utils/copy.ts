import { copyText } from "$lib/services/api";

/** Svelte action: delegated handler for the `.code-copy` buttons that
 *  `renderMarkdown` injects into code blocks. */
export function copyCode(node: HTMLElement) {
  const onClick = (event: MouseEvent) => {
    const button = (event.target as HTMLElement).closest?.(".code-copy");
    if (!(button instanceof HTMLElement) || !node.contains(button)) return;
    const code = button.closest(".code-block")?.querySelector("pre code")?.textContent ?? "";
    copyText(code).catch(() => {});
    button.classList.add("copied");
    setTimeout(() => button.classList.remove("copied"), 1500);
  };
  node.addEventListener("click", onClick);
  return { destroy: () => node.removeEventListener("click", onClick) };
}
