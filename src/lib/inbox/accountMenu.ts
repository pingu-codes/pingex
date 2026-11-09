// The account menu on the Pull requests inbox, as pure functions over what
// the backend resolved for a repository (`review/github/account.rs`).

import type { MenuEntry } from "$lib/components/actionMenu";
import type { RepositoryAccount } from "$lib/types";

/** Why the repository runs as this account, for the button's tooltip. */
export function accountReason(account: RepositoryAccount): string {
  switch (account.source) {
    case "environment":
      return "A token in the environment (GH_TOKEN or GITHUB_TOKEN) decides the account";
    case "pinned":
      return "You chose this account for this repository";
    case "pinMissing":
      return `${account.login} is not signed in to gh, so this repository can't reach GitHub`;
    case "owner":
      return "Automatic: this account owns the repository";
    case "global":
      return "Automatic: the global account";
  }
}

export function accountEntries(account: RepositoryAccount): MenuEntry[] {
  if (account.source === "environment") {
    return [{ id: "environment", label: "Set by GH_TOKEN or GITHUB_TOKEN", disabled: true }];
  }
  const logins: MenuEntry[] = account.logins.map((login) => ({
    id: `pin:${login}`,
    label: login,
    checked: account.pin === login,
  }));
  if (account.source === "pinMissing" && account.pin) {
    logins.push({ id: "missing", label: account.pin, hint: "not signed in", checked: true, disabled: true });
  }
  const entries: MenuEntry[] = [
    { id: "auto", label: "Automatic", checked: account.pin === null },
    { kind: "separator" },
    ...logins,
  ];
  if (account.logins.length > 1) {
    entries.push(
      { kind: "separator" },
      {
        kind: "submenu",
        id: "global",
        label: "Global account",
        entries: account.logins.map((login) => ({
          id: `global:${login}`,
          label: login,
          checked: account.global === login,
        })),
      },
    );
  }
  return entries;
}

/** The inbox read as someone other than the repository's account: the token
 *  didn't take. Logins are compared as GitHub does. */
export function viewerMismatch(account: RepositoryAccount, viewer: string | null): boolean {
  if (!viewer || !account.login || account.source === "pinMissing") return false;
  return viewer.toLowerCase() !== account.login.toLowerCase();
}
