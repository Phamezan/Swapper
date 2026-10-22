// Dev-only preview of the first-run setup (open /src/onboarding/onboarding-preview.html). Never part of the app build.
import { createRoot } from "react-dom/client";
import "../theme.css";
import "../App.css";
import { Onboarding } from "./Onboarding";

createRoot(document.getElementById("root")!).render(<div className="shell dark"><Onboarding onClose={() => document.body.setAttribute("data-closed", "1")} /></div>);
