// Dev-only preview of the tray flyout (App) with mocked Tauri IPC: `npm run dev`, then open
// /src/flyout-preview.html at a 400x640 window. ?fixture=planning|hover|locked picks the champion select
// state; ?error makes a toggle and a switch fail (error banner). Never part of the app build. Fixture data only.
import { createRoot } from "react-dom/client";
import { TooltipProvider } from "./full-app/ui/Tooltip";
import { installFlyoutMock } from "./flyout-preview-mock";
import App from "./App";

installFlyoutMock();
createRoot(document.getElementById("root")!).render(<TooltipProvider><App /></TooltipProvider>);
