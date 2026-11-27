import "@fontsource/space-grotesk/400.css";
import "@fontsource/space-grotesk/500.css";
import "@fontsource/space-grotesk/600.css";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

try {
  const cached = localStorage.getItem("veil.theme.resolved");
  if (cached === "light" || cached === "dark") {
    document.documentElement.dataset.theme = cached;
  }
} catch {
  /* the settings bootstrap corrects the theme */
}

const target = document.getElementById("app");
if (!target) throw new Error("Veil: #app mount point is missing");

const instance = mount(App, { target });
export default instance;
