// Dev-only contact sheet: /src/full-app/ui/icons/icons-preview.html. Never part of the app build.
import { createRoot } from "react-dom/client";
import "../../../theme.css";
import * as icons from "./index";

const sizes = [16, 20, 24];
document.body.style.margin = "0";
const cell = { display: "grid", justifyItems: "center", gap: 10, padding: "16px 8px", borderRadius: 12, background: "var(--ui-surface-2)", border: "1px solid var(--ui-line-1)" } as const;

createRoot(document.getElementById("root")!).render(
  <main style={{ minHeight: "100vh", padding: 32, background: "var(--ui-surface-1)", color: "var(--ui-text-1)", fontFamily: "Outfit, sans-serif" }}>
    <h1 style={{ margin: "0 0 4px", fontSize: 20 }}>Swapper icons</h1>
    <p style={{ margin: "0 0 20px", color: "var(--ui-text-2)", fontSize: 13 }}>16 / 20 / 24px on graphite. Then the active nav state at 24 and 48px.</p>
    <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))", gap: 12 }}>
      {Object.entries(icons).map(([name, Glyph]) => <div key={name} style={cell}>
        <div style={{ display: "flex", alignItems: "center", gap: 16, color: "var(--ui-text-1)" }}>
          {sizes.map(size => <Glyph key={size} size={size} />)}
          <Glyph size={24} className="is-active" /><Glyph size={48} className="is-active" />
        </div>
        <small style={{ fontSize: 12, color: "var(--ui-text-3)" }}>{name.replace(/Icon$/, "")}</small>
      </div>)}
    </div>
  </main>,
);
