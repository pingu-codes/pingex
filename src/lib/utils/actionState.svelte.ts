import { errorMessage } from "$lib/utils/errors";

export interface SubmitStateOptions {
  /** Turns a rejection into the `error` string; `errorMessage` by default. */
  describe?: (cause: unknown) => string;
  /**
   * By default a `run` while another is in flight is refused (returns false),
   * which is what a submit button wants. `false` lets runs overlap; `busy` is
   * then true while any of them is in flight.
   */
  exclusive?: boolean;
}

/**
 * Busy/error state for a view or dialog that performs work itself and reports
 * a failure inline (a dialog stays open with the error rather than resolving
 * and letting the caller report it out of context). `run` returns whether the
 * action succeeded and never throws.
 */
export function submitState(options: SubmitStateOptions = {}) {
  const describe = options.describe ?? errorMessage;
  const exclusive = options.exclusive ?? true;
  let pending = $state(0);
  let error = $state<string | null>(null);
  return {
    get busy() {
      return pending > 0;
    },
    get error() {
      return error;
    },
    set error(value: string | null) {
      error = value;
    },
    async run(action: () => Promise<unknown>): Promise<boolean> {
      if (exclusive && pending > 0) return false;
      pending += 1;
      error = null;
      try {
        await action();
        return true;
      } catch (cause) {
        error = describe(cause);
        return false;
      } finally {
        pending -= 1;
      }
    },
  };
}
