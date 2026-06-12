/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly RENDERER_VITE_CLERK_PUBLISHABLE_KEY?: string;
  readonly RENDERER_VITE_CODEL00P_API_URL?: string;
  readonly RENDERER_VITE_INSTALL_DOCS_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

type LocalSession = {
  session_id: string;
  source: string;
  parent_session_id?: string | null;
  message_count: number;
  event_count: number;
};

type LocalSessionsResult = {
  available: boolean;
  error?: string;
  sessions: LocalSession[];
};

type BrowserSignInResult = { ticket?: string; error?: string };
type EngineStatus = { binaryFound: boolean };

interface Window {
  codel00p?: {
    platform: string;
    local: {
      sessions(): Promise<LocalSessionsResult>;
      engineStatus(): Promise<EngineStatus>;
    };
    auth: {
      signInWithBrowser(): Promise<BrowserSignInResult>;
    };
    openExternal(url: string): Promise<void>;
  };
}
