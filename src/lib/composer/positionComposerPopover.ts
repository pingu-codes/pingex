/** Position composer pickers above their trigger, including triggers inside More. */
export function positionComposerPopover(node: HTMLElement) {
  const anchor = node.parentElement!;
  const place = () => {
    const rect = anchor.getBoundingClientRect();
    node.style.position = "fixed";
    node.style.maxWidth = "calc(100vw - 16px)";
    node.style.maxHeight = "calc(100vh - 16px)";
    node.style.overflowY = "auto";
    node.style.zIndex = "70";
    const width = node.getBoundingClientRect().width;
    const height = node.getBoundingClientRect().height;
    node.style.left = `${Math.max(8, Math.min(rect.left, window.innerWidth - width - 8))}px`;
    node.style.top = `${Math.max(8, Math.min(rect.top - height - 8, window.innerHeight - height - 8))}px`;
    node.style.bottom = "auto";
  };
  place();
  const observer = new ResizeObserver(place);
  observer.observe(node);
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
  return {
    destroy() {
      observer.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    },
  };
}
