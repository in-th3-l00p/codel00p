import { createRoot } from "react-dom/client";
import { ClerkProvider } from "@clerk/clerk-react";

import { App } from "./App";
import { MissingKeyNotice } from "./components/auth/missing-key-notice";
import "./styles.css";

const publishableKey = import.meta.env.RENDERER_VITE_CLERK_PUBLISHABLE_KEY as
  | string
  | undefined;

const root = createRoot(document.getElementById("root")!);

if (!publishableKey) {
  root.render(<MissingKeyNotice />);
} else {
  root.render(
    <ClerkProvider publishableKey={publishableKey} afterSignOutUrl="/">
      <App />
    </ClerkProvider>
  );
}
