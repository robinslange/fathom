import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { applyTheme, readStoredPreference } from "./lib/theme.js";

// Apply persisted theme before mount to avoid a light→dark flash.
applyTheme(readStoredPreference());

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
