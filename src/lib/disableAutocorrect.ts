const TEXT_FIELDS = "input, textarea, [contenteditable]";
const NON_TEXT_INPUTS = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

function apply(el: Element): void {
  if (el instanceof HTMLInputElement && NON_TEXT_INPUTS.has(el.type)) return;
  el.setAttribute("autocorrect", "off");
  el.setAttribute("autocapitalize", "off");
  el.setAttribute("spellcheck", "false");
}

function applyWithin(root: ParentNode): void {
  if (root instanceof Element && root.matches(TEXT_FIELDS)) apply(root);
  for (const el of root.querySelectorAll(TEXT_FIELDS)) apply(el);
}

/**
 * Turns off macOS/WebKit autocorrect, auto-capitalisation and spellcheck on
 * every text field in the app, including ones mounted later. Done globally so
 * no component has to remember the attributes.
 */
export function disableAutocorrect(root: Document = document): void {
  applyWithin(root);
  new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node instanceof Element) applyWithin(node);
      }
    }
  }).observe(root.documentElement, { childList: true, subtree: true });
}
