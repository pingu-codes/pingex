// Pingex Browser Helper.
//
// Pingex already drives Chrome over the DevTools Protocol; what CDP cannot do
// is touch tab groups. This worker is the missing half: Pingex attaches to it
// as a CDP target and calls `pingex.*` with Runtime.evaluate. Changes the user
// makes by hand (dragging a tab into or out of an agent's group, closing it)
// are reported back through the `pingexNotify` binding Pingex installs on
// attach. There is no native-messaging host and no network access.

const HEARTBEAT = "pingex-heartbeat";

function notify(event) {
  // Absent until Pingex attaches (and again after the worker restarts).
  if (typeof globalThis.pingexNotify === "function") {
    globalThis.pingexNotify(JSON.stringify(event));
  }
}

function tabInfo(tab) {
  return {
    tabId: tab.id,
    groupId: tab.groupId,
    windowId: tab.windowId,
    active: tab.active,
    title: tab.title ?? "",
    url: tab.url ?? tab.pendingUrl ?? "",
  };
}

async function groupInto(tabId, groupId, title, color) {
  if (groupId != null) {
    try {
      await chrome.tabGroups.get(groupId);
      await chrome.tabs.group({ groupId, tabIds: [tabId] });
      return groupId;
    } catch {
      // The user closed or ungrouped it; start a new one below.
    }
  }
  const created = await chrome.tabs.group({ tabIds: [tabId] });
  await chrome.tabGroups.update(created, { title, color, collapsed: false });
  return created;
}

globalThis.pingex = {
  hello() {
    return { version: chrome.runtime.getManifest().version };
  },

  /** CDP target id ↔ Chrome tab id, the one mapping CDP cannot give. */
  async targets() {
    const targets = await chrome.debugger.getTargets();
    return targets
      .filter((target) => target.type === "page" && target.tabId != null)
      .map((target) => ({ targetId: target.id, tabId: target.tabId }));
  },

  async tab(tabId) {
    return tabInfo(await chrome.tabs.get(tabId));
  },

  async activeTab() {
    const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
    return tab ? tabInfo(tab) : null;
  },

  /** Open a background tab already inside the thread's group. */
  async createTab(url, groupId, title, color) {
    let windowId;
    if (groupId != null) {
      try {
        windowId = (await chrome.tabGroups.get(groupId)).windowId;
      } catch {
        groupId = null;
      }
    }
    const tab = await chrome.tabs.create({ url, active: false, windowId });
    const group = await groupInto(tab.id, groupId, title, color);
    const targets = await chrome.debugger.getTargets();
    const target = targets.find((candidate) => candidate.tabId === tab.id);
    return { tabId: tab.id, groupId: group, targetId: target?.id ?? null };
  },

  async addToGroup(tabId, groupId, title, color) {
    return await groupInto(tabId, groupId, title, color);
  },

  async ungroup(tabId) {
    try {
      await chrome.tabs.ungroup(tabId);
    } catch {
      // Already closed or not grouped.
    }
    return true;
  },

  async updateGroup(groupId, title, color) {
    try {
      await chrome.tabGroups.update(groupId, { title, color });
      return true;
    } catch {
      return false;
    }
  },

  async focus(tabId) {
    const tab = await chrome.tabs.update(tabId, { active: true });
    await chrome.windows.update(tab.windowId, { focused: true });
    return true;
  },
};

chrome.tabs.onUpdated.addListener((tabId, change) => {
  if (change.groupId !== undefined) {
    notify({ type: "tabGroupChanged", tabId, groupId: change.groupId });
  }
});
chrome.tabs.onRemoved.addListener((tabId) => notify({ type: "tabRemoved", tabId }));
chrome.tabGroups.onRemoved.addListener((group) => notify({ type: "groupRemoved", groupId: group.id }));
// A tab Pingex opens wakes this worker if it was asleep (see onUpdated), and
// the heartbeat does the same every 30 s, so Pingex can attach soon after
// Chrome starts. Once Pingex is attached the worker stays alive.
chrome.tabs.onCreated.addListener(() => {});
chrome.alarms.onAlarm.addListener(() => {});
chrome.runtime.onInstalled.addListener(() => chrome.alarms.create(HEARTBEAT, { periodInMinutes: 0.5 }));
chrome.runtime.onStartup.addListener(() => chrome.alarms.create(HEARTBEAT, { periodInMinutes: 0.5 }));
