/**
 * Short-lived feedback state, such as the tick a copy button shows for a
 * moment after a click. Re-triggering restarts the timer rather than letting
 * the earlier one cut the new flash short.
 */

/** Which of several things flashed last (`null` once the flash has passed). */
export function flashKey<K>(ms = 1500) {
  let current = $state<K | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    get current(): K | null {
      return current;
    },
    trigger(key: K) {
      current = key;
      clearTimeout(timer);
      timer = setTimeout(() => {
        current = null;
      }, ms);
    },
    /** End the flash now. */
    clear() {
      clearTimeout(timer);
      current = null;
    },
  };
}

/** A flag that is on for `ms` after the latest `trigger()`. */
export function flashFlag(ms = 1500) {
  const flash = flashKey<true>(ms);
  return {
    get on(): boolean {
      return flash.current !== null;
    },
    trigger() {
      flash.trigger(true);
    },
    /** End the flash now. */
    clear: flash.clear,
  };
}
