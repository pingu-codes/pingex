/** Review page preferences local to this app, persisted in localStorage. */

import { readFlag, writeFlag } from "$lib/utils/storedFlag";

const REVIEW_COLLAPSED_KEY = "pingex-review-panel-collapsed";
const STACK_TREE_COLLAPSED_KEY = "pingex-stack-tree-collapsed";

/** Whether the Your review panel is folded to its strip. */
export const loadReviewCollapsed = () => readFlag(REVIEW_COLLAPSED_KEY);
export const saveReviewCollapsed = (collapsed: boolean) => writeFlag(REVIEW_COLLAPSED_KEY, collapsed);

/** Whether the Stack page's tree is folded to its strip. */
export const loadStackTreeCollapsed = () => readFlag(STACK_TREE_COLLAPSED_KEY);
export const saveStackTreeCollapsed = (collapsed: boolean) => writeFlag(STACK_TREE_COLLAPSED_KEY, collapsed);
