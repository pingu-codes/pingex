/**
 * Fails when `docs/SUPPORTED_VERSIONS.md` has drifted from the mirror or from
 * the constants the app warns with (`src/lib/app/codexVersion.svelte.ts`).
 *
 *   deno task versions:check
 *
 * The mirror is read as it is; run `git fetch --tags` there first when
 * checking for a new release.
 */
const MIRROR = new URL("../../codex-mirror/", import.meta.url).pathname;
const DOC = new URL("../docs/SUPPORTED_VERSIONS.md", import.meta.url).pathname;
const CONSTANTS = new URL("../src/lib/app/codexVersion.svelte.ts", import.meta.url).pathname;

interface Row {
  tier: string;
  version: string;
  ref: string;
  commit: string;
}

function parseTable(markdown: string): Row[] {
  const rows: Row[] = [];
  for (const line of markdown.split("\n")) {
    const match = line.match(
      /^\|\s*\*\*(Unstable|Stable|Last stable)\*\*\s*\|[^|]*\|\s*`([^`]+)`[^|]*\|\s*`([^`]+)`\s*\|\s*`([0-9a-f]{40})`\s*\|/,
    );
    if (match) rows.push({ tier: match[1], version: match[2], ref: match[3], commit: match[4] });
  }
  return rows;
}

async function git(...args: string[]): Promise<string> {
  const command = new Deno.Command("git", { args: ["-C", MIRROR, ...args], stdout: "piped", stderr: "piped" });
  const { code, stdout, stderr } = await command.output();
  if (code !== 0) throw new Error(`git ${args.join(" ")}: ${new TextDecoder().decode(stderr).trim()}`);
  return new TextDecoder().decode(stdout).trim();
}

const problems: string[] = [];
const rows = parseTable(await Deno.readTextFile(DOC));
for (const tier of ["Unstable", "Stable", "Last stable"]) {
  if (!rows.some((row) => row.tier === tier)) problems.push(`${tier}: no row in the tier table`);
}

for (const row of rows) {
  const ref = row.ref === "main" ? "origin/main" : row.ref;
  let actual: string;
  try {
    actual = await git("rev-parse", `${ref}^{commit}`);
  } catch (error) {
    problems.push(`${row.tier}: ${error instanceof Error ? error.message : String(error)}`);
    continue;
  }
  if (actual !== row.commit)
    problems.push(`${row.tier}: table says ${row.commit.slice(0, 10)} but ${ref} is ${actual.slice(0, 10)}`);
  if (row.ref !== "main" && !row.ref.endsWith(row.version)) {
    problems.push(`${row.tier}: tag ${row.ref} does not match version ${row.version}`);
  }
}

// The newest stable tag upstream must be the one the table calls Stable.
const tags = (await git("tag", "--list", "rust-v0.*", "--sort=-v:refname"))
  .split("\n")
  .filter((tag) => /^rust-v\d+\.\d+\.\d+$/.test(tag));
const stable = rows.find((row) => row.tier === "Stable");
if (stable && tags[0] && tags[0] !== stable.ref) {
  problems.push(`Stable: newest release tag in the mirror is ${tags[0]}, table says ${stable.ref}`);
}
const lastStable = rows.find((row) => row.tier === "Last stable");
if (stable && lastStable) {
  const previous = tags
    .slice(tags.indexOf(stable.ref) + 1)
    .find((tag) => tag.split(".")[1] !== stable.ref.split(".")[1]);
  if (previous && previous !== lastStable.ref) {
    problems.push(
      `Last stable: release before ${stable.ref} in the mirror is ${previous}, table says ${lastStable.ref}`,
    );
  }
}

// The banner's constants must be the table's versions.
const constants = await Deno.readTextFile(CONSTANTS);
const constant = (name: string) => constants.match(new RegExp(`export const ${name} = "([^"]+)"`))?.[1];
if (stable && constant("STABLE") !== stable.version) {
  problems.push(`STABLE in codexVersion.svelte.ts is ${constant("STABLE")}, table says ${stable.version}`);
}
if (lastStable && constant("LAST_STABLE") !== lastStable.version) {
  problems.push(
    `LAST_STABLE in codexVersion.svelte.ts is ${constant("LAST_STABLE")}, table says ${lastStable.version}`,
  );
}

if (problems.length > 0) {
  console.error("docs/SUPPORTED_VERSIONS.md is out of date:");
  for (const problem of problems) console.error(`  - ${problem}`);
  Deno.exit(1);
}
console.log(
  `docs/SUPPORTED_VERSIONS.md matches the mirror: ${rows.map((row) => `${row.tier} ${row.version}`).join(", ")}`,
);
