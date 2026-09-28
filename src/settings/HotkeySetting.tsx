import { useEffect, useRef, useState } from "react";
import { Info } from "lucide-react";
import { Switch } from "@/components/ui/switch";

// Modifier names in the canonical order the backend stores them.
const MODIFIERS = ["ctrl", "alt", "shift", "super"] as const;
type Modifier = (typeof MODIFIERS)[number];

// Keyboard codes that only modify a combination; pressing one alone never
// completes a recording.
const MODIFIER_CODES = new Set([
  "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight",
  "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

// Keys the backend accepts, as KeyboardEvent.code names. Kept in sync with
// the codes asserted in src-tauri/src/hotkey.rs.
const RECORDABLE_CODES = new Set([
  ..."ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("").map((letter) => `Key${letter}`),
  ...Array.from({ length: 10 }, (_, digit) => `Digit${digit}`),
  ...Array.from({ length: 24 }, (_, index) => `F${index + 1}`),
  "Space", "Backquote", "Backslash", "BracketLeft", "BracketRight", "Comma",
  "Equal", "Minus", "Period", "Quote", "Semicolon", "Slash",
  "Insert", "Delete", "Home", "End", "PageUp", "PageDown",
  "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PrintScreen",
]);

const KEY_LABELS: Record<string, string> = {
  Space: "Space", Backquote: "`", Backslash: "\\", BracketLeft: "[",
  BracketRight: "]", Comma: ",", Equal: "=", Minus: "-", Period: ".",
  Quote: "'", Semicolon: ";", Slash: "/",
  Insert: "Insert", Delete: "Delete", Home: "Home", End: "End",
  PageUp: "Page Up", PageDown: "Page Down",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→",
  PrintScreen: "Print Screen",
};

function prettyToken(token: string) {
  switch (token) {
    case "ctrl": return "Ctrl";
    case "alt": return "Alt";
    case "shift": return "Shift";
    case "super": return "Win";
    default: {
      if (KEY_LABELS[token]) return KEY_LABELS[token];
      if (token.startsWith("Key")) return token.slice(3);
      if (token.startsWith("Digit")) return token.slice(5);
      return token;
    }
  }
}

function prettyShortcut(value: string) {
  return value.split("+").map(prettyToken).join(" + ");
}

// The stored form of a pressed key combination, or null while only modifiers
// are held down.
function comboFromEvent(event: KeyboardEvent) {
  if (MODIFIER_CODES.has(event.code)) return null;
  const modifiers: Modifier[] = [];
  if (event.ctrlKey) modifiers.push("ctrl");
  if (event.altKey) modifiers.push("alt");
  if (event.shiftKey) modifiers.push("shift");
  if (event.metaKey) modifiers.push("super");
  return [...modifiers, event.code].join("+");
}

type HotkeySettingProps = {
  hotkey: string | null;
  /** Whether the saved combination is actually registered with the OS. */
  active: boolean;
  disabled: boolean;
  onSet: (value: string | null) => Promise<void>;
};

export function HotkeySetting({ hotkey, active, disabled, onSet }: HotkeySettingProps) {
  const [recording, setRecording] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [infoOpen, setInfoOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!recording) return;
    const setValue = async (value: string | null, message: string | null) => {
      setRecording(false);
      setStatus(message);
      if (value === null) return;
      setSaving(true);
      try {
        await onSet(value);
        setStatus(null);
      } catch (reason) {
        // Keep recording so the rejection (invalid or conflicting
        // combination) stays visible and can be retried immediately.
        setStatus(String(reason));
        setRecording(true);
      } finally {
        setSaving(false);
      }
    };
    const capture = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) return;
      if (event.key === "Escape") {
        void setValue(null, "Shortcut recording canceled.");
        return;
      }
      const combo = comboFromEvent(event);
      if (combo === null) return;
      const key = combo.split("+").pop() as string;
      if (!RECORDABLE_CODES.has(key)) {
        setStatus("That key is not available. Try a letter, number, F-key or Space.");
        return;
      }
      if (!combo.includes("+")) {
        setStatus("Add Ctrl or Alt so the shortcut never fires while you type.");
        return;
      }
      void setValue(combo, null);
    };
    // Clicking anywhere outside the recorder cancels, like a blur would.
    const cancelOnPointerDown = (event: PointerEvent) => {
      if (!(event.target as Element).closest(".hotkey-trigger")) {
        setRecording(false);
        setStatus(null);
      }
    };
    const cancelOnBlur = () => {
      setRecording(false);
      setStatus(null);
    };
    window.addEventListener("keydown", capture, true);
    window.addEventListener("blur", cancelOnBlur);
    document.addEventListener("pointerdown", cancelOnPointerDown, true);
    return () => {
      window.removeEventListener("keydown", capture, true);
      window.removeEventListener("blur", cancelOnBlur);
      document.removeEventListener("pointerdown", cancelOnPointerDown, true);
    };
  }, [recording, onSet]);

  useEffect(() => {
    if (recording) triggerRef.current?.focus();
  }, [recording]);

  const locked = disabled || saving;

  async function handleToggle(enabled: boolean) {
    if (!enabled) {
      setSaving(true);
      try {
        await onSet(null);
        setStatus(null);
      } catch (reason) {
        setStatus(String(reason));
      } finally {
        setSaving(false);
      }
      return;
    }
    setStatus(null);
    setRecording(true);
  }

  return (
    <>
      <div className="setting-row">
        <div className="setting-copy">
          <strong>Global hotkey</strong>
          <button
            className="info-button"
            aria-label="About the global hotkey"
            aria-expanded={infoOpen}
            onClick={() => setInfoOpen((open) => !open)}
          ><Info size={13} /></button>
          {infoOpen && <p>Open or hide the Swapper flyout from anywhere with a keyboard combination. Off by default.</p>}
        </div>
        <Switch
          checked={hotkey !== null}
          disabled={locked}
          onCheckedChange={(checked) => void handleToggle(checked)}
          aria-label="Global hotkey"
        />
      </div>
      {(hotkey !== null || recording) && (
        <div className="hotkey-recorder">
          <button
            type="button"
            ref={triggerRef}
            className={`hotkey-trigger${recording ? " is-recording" : ""}`}
            disabled={locked}
            onClick={() => { setStatus(null); setRecording(true); }}
          >
            {recording ? (saving ? "Saving…" : "Press a combination…") : prettyShortcut(hotkey as string)}
          </button>
          <span className="hotkey-hint">
            {recording ? "Press the keys to use. Esc cancels." : "Click to record a different combination."}
          </span>
          {hotkey !== null && !active && !recording && (
            <span className="hotkey-status">Another application is using this shortcut. Record a different one.</span>
          )}
          {status && <span className="hotkey-status" role="alert">{status}</span>}
        </div>
      )}
    </>
  );
}
