import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { QRCodeSVG } from "qrcode.react";
import { BrandIcon } from "../components/BrandIcon";
import { announceSettingsChanged } from "../settingsEvents";
import { Button } from "../components/ui/button";
import { Switch } from "../components/ui/switch";
import { RUN_EVENT, STEP_TITLES, changedKeys, clampStep, type Choices } from "./onboardingModel";
import "./onboarding.css";

type OverlaySettings = { enabled: boolean; gold: boolean; objectives: boolean; skills: boolean; goldHotkey: number };
type Pairing = { busy: boolean; url: string | null; error: string; firewall: boolean };
type RemoteStatus = { lanAddress: string | null; lanFirewallNeeded: boolean; lanMessage: string | null; state: string };

/** Waits for the LAN service after it starts, then mints a pairing link, or says the firewall step is needed. */
async function pairingUrl(): Promise<Omit<Pairing, "busy">> {
  for (let attempt = 0; attempt < 12; attempt++) {
    const remote = (await invoke<{ remote: RemoteStatus }>("get_state")).remote;
    if (remote.lanAddress) return { url: await invoke<string>("create_lan_pairing_url"), error: "", firewall: false };
    if (remote.lanFirewallNeeded) return { url: null, error: "", firewall: true };
    if (attempt === 11) throw new Error(remote.lanMessage ?? "Remote Control could not start.");
    await new Promise(resolve => setTimeout(resolve, 400));
  }
  throw new Error("Remote Control could not start.");
}

type Snapshot = { choices: Choices; overlay: OverlaySettings | null; riotExe: string | null; ltkInstalled: boolean };

const BROWSER_SNAPSHOT: Snapshot = {
  choices: { fullApp: false, startup: false, overlay: false, gold: true, objectives: true, skills: true, deceive: false, koreanFont: false, remote: false },
  overlay: null, riotExe: null, ltkInstalled: false,
};

async function loadSnapshot(): Promise<Snapshot> {
  if (!isTauri()) return BROWSER_SNAPSHOT;
  const [windowSettings, overlay, state, startup] = await Promise.all([
    invoke<{ fullApp: boolean }>("window_settings"),
    invoke<OverlaySettings>("get_overlay_settings"),
    invoke<{ useDeceive: boolean; riotExe: string | null; koreanFont: boolean; ltkInstalled: boolean; remote: { enabled: boolean } }>("get_state"),
    isAutostartEnabled().catch(() => false),
  ]);
  return {
    choices: {
      fullApp: windowSettings.fullApp, startup, overlay: overlay.enabled, gold: overlay.gold, objectives: overlay.objectives,
      skills: overlay.skills, deceive: state.useDeceive, koreanFont: state.koreanFont, remote: state.remote.enabled,
    },
    overlay, riotExe: state.riotExe, ltkInstalled: state.ltkInstalled,
  };
}

/** Writes only what changed, through the commands Settings uses. Returns the failures. */
async function save(snapshot: Snapshot, next: Choices): Promise<string[]> {
  if (!isTauri()) return [];
  const changed = new Set(changedKeys(snapshot.choices, next));
  const errors: string[] = [];
  const run = async (label: string, work: () => Promise<unknown>) => {
    try { await work(); } catch (reason) { errors.push(`${label}: ${reason}`); }
  };
  if (snapshot.overlay && (["overlay", "gold", "objectives", "skills"] as const).some(key => changed.has(key))) {
    await run("Overlay", () => invoke("set_overlay_settings", { settings: { ...snapshot.overlay, enabled: next.overlay, gold: next.gold, objectives: next.objectives, skills: next.skills } }));
  }
  if (changed.has("deceive")) await run("Deceive", () => invoke("save_settings", { useDeceive: next.deceive, riotExe: snapshot.riotExe }));
  if (changed.has("koreanFont")) await run("Korean font", () => invoke("set_korean_font", { enabled: next.koreanFont }));
  if (changed.has("remote")) await run("Remote Control", () => invoke("set_remote_enabled", { enabled: next.remote }));
  if (changed.has("startup")) await run("Start with Windows", () => next.startup ? enableAutostart() : disableAutostart());
  await run("Setup", () => invoke("set_onboarded"));
  // Last: switching to or from the Fullsized App closes this window.
  if (changed.has("fullApp")) await run("Window", () => invoke("set_full_app_enabled", { enabled: next.fullApp }));
  return errors;
}

function Row({ id, title, children, checked, disabled, onChange }: { id: string; title: string; children: ReactNode; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void }) {
  return <div className="ob-row">
    <div><label htmlFor={id}>{title}</label><p>{children}</p></div>
    <Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onChange} />
  </div>;
}

const OPTIONS = [
  { value: false, title: "Tray flyout", text: "A small panel from the tray icon. Quick account switching and drafts.", art: <svg viewBox="0 0 96 64" aria-hidden><rect x="1" y="1" width="94" height="62" rx="6" className="ob-art-bg" /><rect x="64" y="14" width="26" height="38" rx="4" className="ob-art-on" /><rect x="68" y="20" width="18" height="4" rx="2" className="ob-art-line" /><rect x="68" y="28" width="18" height="4" rx="2" className="ob-art-line" /><rect x="6" y="56" width="84" height="3" rx="1.5" className="ob-art-line" /></svg> },
  { value: true, title: "Fullsized App", text: "A full window with stats, builds, matchups and live game too.", art: <svg viewBox="0 0 96 64" aria-hidden><rect x="1" y="1" width="94" height="62" rx="6" className="ob-art-bg" /><rect x="8" y="8" width="16" height="48" rx="3" className="ob-art-line" /><rect x="28" y="8" width="60" height="10" rx="3" className="ob-art-on" /><rect x="28" y="22" width="28" height="34" rx="3" className="ob-art-line" /><rect x="60" y="22" width="28" height="34" rx="3" className="ob-art-line" /></svg> },
];

export function Onboarding({ onClose }: { onClose: () => void }) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [choices, setChoices] = useState<Choices | null>(null);
  const [step, setStep] = useState(0);
  const [confirmSkip, setConfirmSkip] = useState(false);
  const [saving, setSaving] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  // Remote Control is applied the moment it is switched on, so the pairing code can show right here.
  const [pairing, setPairing] = useState<Pairing>({ busy: false, url: null, error: "", firewall: false });
  const dialog = useRef<HTMLDivElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    let live = true;
    loadSnapshot().then(loaded => { if (live) { setSnapshot(loaded); setChoices(loaded.choices); } })
      .catch(reason => { if (live) setErrors([String(reason)]); });
    return () => { live = false; };
  }, []);
  useEffect(() => { heading.current?.focus(); }, [step]);

  const set = (patch: Partial<Choices>) => setChoices(current => current && { ...current, ...patch });
  const skip = async () => {
    setSaving(true);
    try { if (isTauri()) await invoke("set_onboarded"); onClose(); }
    catch (reason) { setErrors([`Setup: ${reason}`]); setConfirmSkip(false); setSaving(false); }
  };
  const finish = async () => {
    if (!snapshot || !choices) return;
    setSaving(true);
    const failed = await save(snapshot, choices);
    setSaving(false);
    announceSettingsChanged();
    if (failed.length) setErrors(failed); else onClose();
  };
  const showPairing = async () => {
    setPairing({ busy: true, url: null, error: "", firewall: false });
    try { setPairing({ ...(await pairingUrl()), busy: false }); }
    catch (reason) { setPairing({ busy: false, url: null, error: String(reason).replace(/^Error: /, ""), firewall: false }); }
  };
  const toggleRemote = async (on: boolean) => {
    set({ remote: on });
    if (!isTauri()) return;
    setPairing({ busy: on, url: null, error: "", firewall: false });
    try {
      await invoke("set_remote_enabled", { enabled: on });
      // Already saved: Done must not toggle it again.
      setSnapshot(current => current && { ...current, choices: { ...current.choices, remote: on } });
      announceSettingsChanged();
      if (on) await showPairing();
    } catch (reason) {
      setPairing({ busy: false, url: null, error: String(reason).replace(/^Error: /, ""), firewall: false });
    }
  };
  // Only on this click does Windows ask for firewall permission, as in Settings → Remote.
  const allowFirewall = async () => {
    setPairing(current => ({ ...current, busy: true }));
    try { await invoke("allow_lan_firewall"); await showPairing(); }
    catch (reason) { setPairing({ busy: false, url: null, error: String(reason).replace(/^Error: /, ""), firewall: true }); }
  };
  // Esc asks before skipping; Tab stays inside the dialog.
  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") { event.preventDefault(); setConfirmSkip(true); return; }
    if (event.key !== "Tab" || !dialog.current) return;
    const items = [...dialog.current.querySelectorAll<HTMLElement>("button:not(:disabled), h2[tabindex]")];
    const first = items[0], last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  };

  const last = step === STEP_TITLES.length - 1;
  const failedToSave = errors.length > 0 && Boolean(choices);
  return <div className="ob-scrim">
    <div ref={dialog} className="ob-card" role="dialog" aria-modal="true" aria-labelledby="ob-title" onKeyDown={onKeyDown}>
      <header>
        <BrandIcon size={22} />
        <ol className="ob-steps" aria-hidden>
          {STEP_TITLES.map((title, index) => <li key={title} className={index === step ? "is-current" : index < step ? "is-done" : ""} />)}
        </ol>
        <small>Step {step + 1} of {STEP_TITLES.length}</small>
      </header>
      <h2 id="ob-title" ref={heading} tabIndex={-1}>{STEP_TITLES[step]}</h2>
      <div className="ob-body">
        {!choices ? <p className="ob-note" role="status">{errors[0] ?? "Loading your settings…"}</p> : step === 0 ? <>
          <div className="ob-choices" role="radiogroup" aria-label="How you use Swapper">
            {OPTIONS.map(option => <button key={option.title} type="button" role="radio" aria-checked={choices.fullApp === option.value} className="ob-choice" onClick={() => set({ fullApp: option.value })}>
              {option.art}<strong>{option.title}</strong><span>{option.text}</span>
            </button>)}
          </div>
          <Row id="ob-startup" title="Start with Windows" checked={choices.startup} onChange={startup => set({ startup })}>Swapper starts in the tray when you sign in.</Row>
        </> : step === 1 ? <>
          <div className="ob-preview"><img src="/overlay-fixture.png" alt="The overlay in a game: gold difference and objective timers" /><small>Preview</small></div>
          <Row id="ob-overlay" title="In-game overlay" checked={choices.overlay} onChange={overlay => set({ overlay })}>Shows over League. Needs Borderless or Windowed mode.</Row>
          <Row id="ob-gold" title="Gold difference" checked={choices.gold} disabled={!choices.overlay} onChange={gold => set({ gold })}>Compare the value of held items while you hold your overlay key.</Row>
          <Row id="ob-objectives" title="Objective timers" checked={choices.objectives} disabled={!choices.overlay} onChange={objectives => set({ objectives })}>Keep track of upcoming neutral objectives.</Row>
          <Row id="ob-skills" title="Skill level-up hint" checked={choices.skills} disabled={!choices.overlay} onChange={skills => set({ skills })}>See the next ability to level from your cached build.</Row>
        </> : <>
          <Row id="ob-deceive" title="Deceive" checked={choices.deceive} onChange={deceive => set({ deceive })}>Appear offline to friends when Swapper starts League.</Row>
          <Row id="ob-korean" title="Korean font" checked={choices.koreanFont} disabled={!snapshot?.ltkInstalled} onChange={koreanFont => set({ koreanFont })}>
            Use at your own risk: changes League's game files while it runs.{!snapshot?.ltkInstalled && " Install LTK Manager in Settings → General first."}
          </Row>
          <Row id="ob-remote" title="Remote Control" checked={choices.remote} disabled={pairing.busy} onChange={remote => void toggleRemote(remote)}>Draft from your phone.</Row>
          {choices.remote && (pairing.url || pairing.busy || pairing.error || pairing.firewall) && <div className="ob-pair" role="status">
            {pairing.url ? <><div className="ob-qr"><QRCodeSVG value={pairing.url} size={132} level="M" marginSize={3} bgColor="#ffffff" fgColor="#18181b" title="Phone pairing QR code" /></div>
              <p><strong>Scan with your phone</strong>On the same Wi-Fi as this PC. Single use, expires in 5 minutes. More options in Settings → Remote.</p></>
              : pairing.busy ? <p>Starting Remote Control…</p>
              : pairing.firewall ? <><p><strong>Allow Swapper in Windows Firewall</strong>Phones on your Wi-Fi need it. Windows asks once.{pairing.error && <span className="ob-pair-error"> {pairing.error}</span>}</p>
                <Button variant="outline" className="ob-quiet" onClick={() => void allowFirewall()}>Allow</Button></>
              : <p className="ob-pair-error">{pairing.error} Pair later in Settings → Remote.</p>}
          </div>}
        </>}
        {failedToSave && <div className="ob-error" role="alert">{errors.map(error => <p key={error}>{error}</p>)}</div>}
      </div>
      {confirmSkip ? <footer className="ob-confirm" role="alertdialog" aria-label="Skip setup">
        <p>Skip setup? Swapper keeps its defaults. You can run it again from Settings → General.</p>
        <Button variant="outline" className="ob-quiet" onClick={() => setConfirmSkip(false)} autoFocus>Keep going</Button>
        <Button disabled={saving} onClick={() => void skip()}>Skip setup</Button>
      </footer> : <footer>
        <Button variant="ghost" className="ob-quiet" disabled={saving} onClick={() => setConfirmSkip(true)}>Skip setup</Button>
        <span />
        {step > 0 && !failedToSave && <Button variant="outline" className="ob-quiet" disabled={saving} onClick={() => setStep(clampStep(step - 1))}>Back</Button>}
        {failedToSave ? <Button onClick={onClose}>Close</Button>
          : <Button disabled={!choices || saving} onClick={() => last ? void finish() : setStep(clampStep(step + 1))}>{last ? (saving ? "Saving…" : "Done") : "Next"}</Button>}
      </footer>}
    </div>
  </div>;
}

/** Opens the setup once on a new install, and again when Settings asks. */
export function OnboardingGate() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const show = () => setOpen(true);
    window.addEventListener(RUN_EVENT, show);
    if (isTauri()) invoke<boolean>("onboarding_pending").then(pending => { if (pending) show(); }).catch(() => {});
    else if (new URLSearchParams(location.search).has("onboarding")) show();
    return () => window.removeEventListener(RUN_EVENT, show);
  }, []);
  return open ? <Onboarding onClose={() => setOpen(false)} /> : null;
}
