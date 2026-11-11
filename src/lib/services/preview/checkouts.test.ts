import { describe, expect, it } from "vitest";
import { commands } from "$lib/services/homeRouting";
import { seedPreviewDesk } from "$lib/services/preview/desk";
import { deskRepository, inboxPr } from "$lib/services/preview/factories";
import type { CheckoutTarget, DeskAction, RepositorySettings, StepPlan } from "$lib/types";
import recorded from "../../../../tests/fixtures/desk/step_plans.json";

type Spec = Pick<CheckoutTarget, "number" | "mine" | "stack"> & {
  headRepo: "same" | "fork" | null;
  stackBranches?: string[];
};
type Case = {
  name: string;
  settings?: Partial<RepositorySettings>;
  prs: { number: number; headRef: string }[];
  ensure?: Spec[];
  target: Spec;
  action: DeskAction;
  plan: Omit<StepPlan, "current"> & {
    current: Pick<StepPlan["current"], "step" | "path" | "gitBranch" | "managed" | "onPrBranch" | "setupPending">;
  };
};

const HOME = "/home/me/.pingex";
const REPO_DIR = "/projects/repo";
const SLUG = "pingex/desk";

/** `value` with `{home}` and `{repo}` filled in, as the Rust test fills them. */
function fill<T>(value: T): T {
  return JSON.parse(JSON.stringify(value).replaceAll("{home}", HOME).replaceAll("{repo}", "repo"));
}

// The same recorded cases the Rust desk answers in `review/desk/steps.rs`
// (`recorded_step_plans`): the preview's fake makes the same step plans.
describe("the preview desk's step plans", () => {
  it.each((recorded.cases as Case[]).map((entry) => [entry.name, entry] as const))("%s", async (_name, entry) => {
    const prs = entry.prs.map((pr) => inboxPr({ number: pr.number, title: `PR ${pr.number}`, headRef: pr.headRef }));
    seedPreviewDesk({
      home: HOME,
      repositories: [
        deskRepository(SLUG, {
          repository: { path: REPO_DIR, key: REPO_DIR, name: "repo" },
          prs,
          settings: entry.settings && fill(entry.settings),
        }),
      ],
    });
    const targetOf = (spec: Spec): CheckoutTarget => {
      const pr = prs.find((candidate) => candidate.number === spec.number);
      if (!pr) throw new Error(`#${spec.number} is not a PR of the case`);
      return {
        number: pr.number,
        headRef: pr.headRef,
        headSha: pr.headSha,
        mine: spec.mine,
        stack: spec.stack,
        headRepo: spec.headRepo === "same" ? SLUG : spec.headRepo === "fork" ? "someone/fork" : null,
        stackBranches: spec.stackBranches ?? [],
      };
    };
    for (const spec of entry.ensure ?? []) await commands.reviewEnsurePrWorktree(REPO_DIR, targetOf(spec));

    const plan = await commands.reviewPlanStep(REPO_DIR, targetOf(entry.target), entry.action);

    const { current } = plan;
    expect({
      stepUp: plan.stepUp,
      confirm: plan.confirm,
      confirmPush: plan.confirmPush,
      refused: plan.refused,
      switchBranch: plan.switchBranch,
      worktreePath: plan.worktreePath,
      current: {
        step: current.step,
        path: current.path,
        gitBranch: current.gitBranch,
        managed: current.managed,
        onPrBranch: current.onPrBranch,
        setupPending: current.setupPending,
      },
    }).toEqual(fill(entry.plan));
  });
});
