import { TooltipProvider } from "../full-app/ui/Tooltip";
import React from "react";
import ReactDOM from "react-dom/client";
import RemoteApp from "./RemoteApp";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider><RemoteApp /></TooltipProvider>
  </React.StrictMode>,
);
