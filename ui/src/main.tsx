import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { getLang } from "./i18n";
import { applyTheme, loadTheme } from "./theme";

// Before the first paint: the stored theme and language.
applyTheme(loadTheme());
document.documentElement.lang = getLang();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
