import { mount } from "svelte";
import "./app.css";

const target = document.getElementById("app")!;

// The quick-chat window loads the same bundle with `?window=quick`; branch here
// so it mounts the lightweight composer instead of the full app.
if (new URLSearchParams(window.location.search).get("window") === "quick") {
  const { default: QuickChat } = await import("$lib/quick/QuickChat.svelte");
  mount(QuickChat, { target });
} else {
  const { default: App } = await import("./App.svelte");
  mount(App, { target });
}
