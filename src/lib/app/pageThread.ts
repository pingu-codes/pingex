// "New thread" from a page about a stack or a PR starts in that stack's or
// PR's worktree, not in whichever project the sidebar showed last: the
// Stack page keeps the previous project selected, and a thread drafted there
// would be told about another worktree's stack.
//
// The page shown provides how it starts one while it is mounted. A Stack page
// shows a PR beside it, and the stack's worktree wins over the PR's.

type Start = () => void | Promise<void>;

const starts: { stack: Start | null; pr: Start | null } = { stack: null, pr: null };

/** While the page is shown, "New thread" calls `start`. Returns the undo,
 *  for the page's teardown. */
export function providePageThread(kind: "stack" | "pr", start: Start): () => void {
  starts[kind] = start;
  return () => {
    if (starts[kind] === start) starts[kind] = null;
  };
}

/** How the page shown starts a thread, if it has its own way. */
export function pageThreadStart(): Start | null {
  return starts.stack ?? starts.pr;
}
