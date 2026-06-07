import { createRoot } from "react-dom/client";
import { ProductShell } from "@codel00p/ui";

import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <ProductShell
    eyebrow="Desktop"
    title="Session control"
    description="Supervise agents, review memory, and inspect project knowledge from the desktop workspace."
  />
);
