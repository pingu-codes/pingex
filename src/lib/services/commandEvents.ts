// Every command the `commands` proxy runs is announced here once it settles,
// so caches can drop what a mutation made stale without each call site
// remembering to. Kept apart from `homeRouting` so tests that mock the proxy
// still get a working hook.

export type SettledCommand = {
  name: string;
  args: readonly unknown[];
  /** The command's argument names, in order (`window` last). */
  names: readonly string[];
};

type Listener = (command: SettledCommand) => void;

const listeners = new Set<Listener>();

export function onCommandSettled(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function emitCommandSettled(command: SettledCommand): void {
  for (const listener of listeners) {
    try {
      listener(command);
    } catch {
      // A cache that fails to invalidate must not fail the command.
    }
  }
}

/** The value of the argument called `name`, if the command has one. */
export function argNamed(command: SettledCommand, name: string): unknown {
  const index = command.names.indexOf(name);
  return index === -1 ? undefined : command.args[index];
}
