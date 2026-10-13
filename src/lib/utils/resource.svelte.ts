/**
 * Asynchronously loaded state where only the latest request may land. Every
 * `reload()` (and `set()`) supersedes the requests before it, so a slow older
 * response can never overwrite a newer one, clear its error or end its
 * loading state.
 */

import { untrack } from "svelte";
import { errorMessage } from "$lib/utils/errors";

export interface Resource<T, A extends unknown[] = []> {
  /** The last value that landed; kept on screen while a reload is in flight. */
  readonly value: T;
  /** Why the latest request failed, or `null`. Cleared when a reload starts. */
  readonly error: string | null;
  /** Whether the latest request is still in flight. */
  readonly loading: boolean;
  /** Fetch again. Resolves once this request settles, landed or superseded. */
  reload(...args: A): Promise<void>;
  /** Replace the value directly (e.g. with a mutation's result) and drop any
   *  request still in flight. */
  set(value: T): void;
}

export interface ResourceOptions<T> {
  initial?: T;
  /** Turns a rejection into the `error` string; `errorMessage` by default. */
  describe?: (cause: unknown) => string;
}

/**
 * Nothing is fetched until the first `reload()`; call it from `onMount` or an
 * `$effect`. The fetcher runs untracked, so an effect that reloads depends on
 * what the effect itself reads, not on whatever the fetcher happens to read.
 */
export function resource<T, A extends unknown[] = []>(
  fetcher: (...args: A) => Promise<T>,
  options: ResourceOptions<T> & { initial: T },
): Resource<T, A>;
export function resource<T, A extends unknown[] = []>(
  fetcher: (...args: A) => Promise<T>,
  options?: ResourceOptions<T>,
): Resource<T | undefined, A>;
export function resource<T, A extends unknown[] = []>(
  fetcher: (...args: A) => Promise<T>,
  options: ResourceOptions<T> = {},
): Resource<T | undefined, A> {
  const describe = options.describe ?? errorMessage;
  let value = $state<T | undefined>(options.initial);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let generation = 0;

  return {
    get value() {
      return value;
    },
    get error() {
      return error;
    },
    get loading() {
      return loading;
    },
    async reload(...args: A) {
      const mine = ++generation;
      loading = true;
      error = null;
      try {
        const next = await untrack(() => fetcher(...args));
        if (mine === generation) value = next;
      } catch (cause) {
        if (mine === generation) error = describe(cause);
      } finally {
        if (mine === generation) loading = false;
      }
    },
    set(next: T | undefined) {
      ++generation;
      value = next;
      error = null;
      loading = false;
    },
  };
}
