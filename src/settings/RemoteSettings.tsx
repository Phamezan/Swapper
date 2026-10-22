import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import { Info, LoaderCircle, QrCode, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { PairedDevices } from "./PairedDevices";
import type { RemoteStatus } from "../App";

type Props = {
  remote: RemoteStatus;
  transport: "lan" | "tailscale";
  onTransportChange: (transport: "lan" | "tailscale") => void;
  onToggle: (enabled: boolean) => void;
  /** Asks Windows (once, for Swapper) to allow LAN on Private networks. */
  onAllowFirewall: () => Promise<void>;
  /** Bumped by the desktop to open the QR panel, e.g. after an address change. */
  qrRequest: number;
  onError: (reason: unknown) => void;
};

export function RemoteSettings({ remote, transport, onTransportChange, onToggle, onAllowFirewall, qrRequest, onError }: Props) {
  const [copied, setCopied] = useState(false);
  const [allowing, setAllowing] = useState(false);
  const [showQr, setShowQr] = useState(false);
  const [qrUrl, setQrUrl] = useState<string | null>(null);
  const [openInfo, setOpenInfo] = useState(false);
  const [pendingReset, setPendingReset] = useState(false);
  const [qrBusy, setQrBusy] = useState(false);
  const requestId = useRef(0);
  const copyTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    setShowQr(false);
    setQrUrl(null);
    setPendingReset(false);
    setCopied(false);
    setQrBusy(false);
    return () => {
      requestId.current += 1;
      window.clearTimeout(copyTimer.current);
    };
  }, [transport, remote.enabled, remote.lanAddress, remote.tailscaleAddress]);

  // The desktop's reconnect banner asks the LAN QR panel to open.
  useEffect(() => {
    if (qrRequest === 0 || showQr) return;
    if (transport !== "lan" || !remote.lanAddress) return;
    void toggleQr();
  }, [qrRequest]);

  const changeTransport = (next: "lan" | "tailscale") => {
    if (next === transport) return;
    requestId.current += 1;
    setShowQr(false);
    setQrUrl(null);
    onTransportChange(next);
  };

  // Disabling hides the QR panel, exactly like before the tabs split.
  const toggle = (enabled: boolean) => {
    if (!enabled) {
      requestId.current += 1;
      setQrBusy(false);
      setShowQr(false);
      setQrUrl(null);
    }
    onToggle(enabled);
  };

  async function copyAddress(address: string | null) {
    if (!address) return;
    try {
      await navigator.clipboard.writeText(address);
    } catch (reason) {
      onError(reason);
      return;
    }
    setCopied(true);
    window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => setCopied(false), 1500);
  }

  async function toggleQr() {
    if (qrBusy) return;
    if (showQr) {
      setShowQr(false);
      return;
    }
    const request = ++requestId.current;
    setQrBusy(true);
    setCopied(false);
    try {
      const url = transport === "lan"
        ? await invoke<string>("create_lan_pairing_url")
        : remote.tailscaleAddress;
      if (!url || request !== requestId.current) return;
      setQrUrl(url);
      setShowQr(true);
    } catch (reason) {
      if (request === requestId.current) onError(reason);
    } finally {
      if (request === requestId.current) setQrBusy(false);
    }
  }

  async function resetLanAccess() {
    if (qrBusy) return;
    const request = ++requestId.current;
    setQrBusy(true);
    try {
      const url = await invoke<string>("reset_lan_access");
      if (request !== requestId.current) return;
      setQrUrl(url);
      setShowQr(true);
      setPendingReset(false);
      setCopied(false);
      window.requestAnimationFrame(() => document.getElementById("remote-qr-panel")?.scrollIntoView({ block: "nearest" }));
    } catch (reason) {
      if (request === requestId.current) onError(reason);
    } finally {
      if (request === requestId.current) setQrBusy(false);
    }
  }

  return (
    <>
      <div className="setting-row remote-setting">
        <div className="setting-copy">
          <label htmlFor="remote-switch">Remote Control</label>
          <button className="info-button" aria-label="About Remote Control" aria-expanded={openInfo} onClick={() => setOpenInfo((open) => !open)}><Info size={13} /></button>
          {openInfo && <p>Control League from your phone over private Wi-Fi or Tailscale.</p>}
        </div>
        <Switch id="remote-switch" checked={remote.enabled} onCheckedChange={toggle} aria-label="Remote Control" />
      </div>
      {remote.state === "starting" && <div className="setting-status"><LoaderCircle className="spin" size={13} /> Starting…</div>}
      {remote.enabled && <>
        <div className="remote-transport-row">
          <div className="remote-transport" role="group" aria-label="Remote Control transport">
            <button type="button" className={transport === "lan" ? "is-active" : ""} aria-pressed={transport === "lan"} onClick={() => changeTransport("lan")}>LAN</button>
            <button type="button" className={transport === "tailscale" ? "is-active" : ""} aria-pressed={transport === "tailscale"} onClick={() => changeTransport("tailscale")}>Tailscale</button>
          </div>
          <span className="remote-transport-help">
            <button type="button" className="info-button remote-transport-info" aria-label="About LAN and Tailscale" aria-describedby="remote-transport-tooltip"><Info size={13} /></button>
            <span id="remote-transport-tooltip" className="remote-transport-tooltip" role="tooltip">LAN: local Wi-Fi or Ethernet over HTTP, on a trusted Private Windows network. Tailscale: HTTPS over your Tailscale network, connected on PC and phone.</span>
          </span>
        </div>
        {transport === "lan" ? (
          <>
            {remote.lanReconnectNeeded && <p className="remote-reconnect-message" role="status">PC address changed. Scan the new QR code to reconnect.</p>}
            <div className="setting-status"><span className={`remote-status-dot ${remote.lanAddress ? "is-good" : "is-bad"}`} />Local network · {remote.lanAddress ? "Ready" : "Unavailable"}</div>
            {remote.lanAddress ? <>
              <div className="remote-address"><Button className="remote-qr-toggle" disabled={qrBusy} aria-controls="remote-qr-panel" aria-expanded={showQr} onClick={() => void toggleQr()}>{qrBusy ? <LoaderCircle className="spin" size={15} /> : <QrCode size={15} />}{showQr ? "Hide pairing code" : "Pair a phone"}</Button></div>
              {showQr && qrUrl && <div id="remote-qr-panel" className="remote-qr-panel">
                <div className="remote-qr-image"><QRCodeSVG value={qrUrl} size={176} level="M" marginSize={4} bgColor="#ffffff" fgColor="#18181b" title="LAN pairing QR code" /></div>
                <p>Scan on the same private Wi-Fi. Single-use, expires in 5 minutes.</p>
                <div className="remote-address">
                  <Input readOnly value={qrUrl} aria-label="Phone pairing link" onFocus={(e) => e.currentTarget.select()} />
                  <Button variant="outline" onClick={() => void copyAddress(qrUrl)}>{copied ? "Copied" : "Copy link"}</Button>
                </div>
                {remote.localAddress && <details className="remote-discovery-help"><summary>Keeping your phone connected</summary><p>After pairing, iPhone users can switch to {remote.localAddress} from the phone page to reconnect when the PC's IP changes. Android browsers cannot open .local addresses; use the QR code again if the PC's address changes.</p></details>}
              </div>}
            </> : remote.lanFirewallNeeded ? <>
              <p className="remote-transport-message">Windows Firewall must let Swapper in. Windows asks once.</p>
              <div className="remote-address">
                <Button variant="outline" disabled={allowing} onClick={() => { setAllowing(true); void onAllowFirewall().finally(() => setAllowing(false)); }}>
                  {allowing && <LoaderCircle className="spin" size={15} />} Allow on private networks
                </Button>
              </div>
            </> : <>
              <p className="remote-transport-message">{remote.lanMessage ?? "LAN needs an active Ethernet or Wi-Fi network."}</p>
              <div className="remote-network-help">
                <span>For a trusted network, set its Windows profile to Private.</span>
                <Button variant="outline" className="remote-settings-button" aria-label="Open Windows network settings" title="Open Windows network settings" onClick={() => void invoke("open_windows_network_settings").catch(onError)}><Settings2 size={14} /></Button>
              </div>
            </>}
          </>
        ) : (
          remote.tailscaleAddress ? <>
            <div className="setting-status"><span className="remote-status-dot is-good" />Tailscale · Ready</div>
            <div className="remote-address">
              <Input readOnly value={remote.tailscaleAddress} aria-label="Tailscale remote address" onFocus={(e) => e.currentTarget.select()} />
              <Button variant="outline" disabled={copied} onClick={() => void copyAddress(remote.tailscaleAddress)}>{copied ? "Copied" : "Copy"}</Button>
              <Button variant="outline" className="remote-qr-toggle" aria-label={showQr ? "Hide Tailscale QR code" : "Show Tailscale QR code"} aria-controls="remote-qr-panel" aria-expanded={showQr} onClick={() => void toggleQr()}><QrCode size={15} /> QR</Button>
            </div>
            {showQr && qrUrl && <div id="remote-qr-panel" className="remote-qr-panel">
              <div className="remote-qr-image"><QRCodeSVG value={qrUrl} size={176} level="M" marginSize={4} bgColor="#ffffff" fgColor="#18181b" title="Tailscale remote address QR code" /></div>
              <p>Scan on a phone connected to Tailscale.</p>
            </div>}
          </> : <>
            <div className="setting-status"><span className="remote-status-dot is-bad" />Tailscale · {remote.tailscaleInstalled ? "Not connected" : "Not installed"}</div>
            <p className="remote-transport-message">{!remote.tailscaleInstalled
              ? "Install Tailscale to use it. LAN still works on a Private network."
              : !remote.tailscaleRunning
                ? "Connect Tailscale on this PC and your phone."
                : remote.message ?? "Tailscale Serve could not be started."}</p>
          </>
        )}
        {transport === "lan" && <>
          <PairedDevices staleSince={remote.lanReconnectSince} />
          {remote.lanAddress && <details className="remote-advanced" onToggle={(event) => { if (!event.currentTarget.open) setPendingReset(false); }}>
            <summary>Advanced</summary>
            {pendingReset ? <div className="remote-reset-confirm" role="group" aria-label="Confirm reset LAN access">
              <p>Unpairs every phone and voids pairing links. Each phone must pair again.</p>
              <Button variant="outline" className="remote-reset" disabled={qrBusy} onClick={() => void resetLanAccess()}>{qrBusy ? "Resetting…" : "Reset and unpair all"}</Button>
              <Button variant="ghost" disabled={qrBusy} onClick={() => setPendingReset(false)}>Cancel</Button>
            </div> : <Button variant="ghost" className="remote-reset" onClick={() => setPendingReset(true)}>Reset LAN access…</Button>}
          </details>}
        </>}
      </>}
    </>
  );
}
