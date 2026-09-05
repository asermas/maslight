import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { IconContext } from "@phosphor-icons/react";

import App from "./App";
import "./styles.css";

const container = document.getElementById("root");
if (!container) throw new Error("missing #root");

// Every icon in MasLight sits next to its own label, or inside a button that
// carries an aria-label. None of them is the only way to read anything, so a
// screen reader announcing "graphic" before each one is pure noise. Hiding
// them once here is better than remembering it at ninety call sites.
//
// focusable="false" is for old Edge and IE, which put SVGs in the tab order
// and made every icon a stop on the way to the next control.
const icons = { "aria-hidden": true, focusable: "false" } as const;

createRoot(container).render(
  <StrictMode>
    <IconContext.Provider value={icons}>
      <App />
    </IconContext.Provider>
  </StrictMode>
);
