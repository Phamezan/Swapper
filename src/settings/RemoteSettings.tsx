import { useState } from "react";
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
  onError: (reason: unknown) => void;
};

export function RemoteSettings({ remote, transport, onTransportChange, onToggle, onError }: Props) {
  const [copied, setCopied] = useState(false);
  const [showQr, setShowQr] = useState(false);
  const [qrUrl, setQrUrl] = useState<string | null>(null);
  const [openInfo, setOpenInfo] = useState(false);

  const changeTransport = (next: "lan" | "tailscale") => {
    setShowQr(false);
    setQrUrl(null);
    onTransportChange(next);
  };

  // Disabling hides the QR panel, exactly like before the tabs split.
  const toggle = (enabled: boolean) => {
    if (!enabled) {
      setShowQr(false);
      setQrUrl(null);
    }
    onToggle(enabled);
  };

  async function copyAddress() {
    const address = remote.tailscaleAddress;
    if (!address) return;
    try {
      await navigator.clipboard.writeText(address);
    } catch {
      const field = document.createElement("textarea");
      field.value = address;
      field.style.position = "fixed";
      field.style.opacity = "0";
      document.body.appendChild(field);
      field.select();
      document.execCommand("copy");
      field.remove();
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  async function toggleQr() {
    if (showQr) {
      setShowQr(false);
      return;
    }
    try {
      const url = transport === "lan"
        ? await invoke<string>("create_lan_pairing_url")
        : remote.tailscaleAddress;
      if (!url) return;
      setQrUrl(url);
      setShowQr(true);
    } catch (reason) {
      onError(reason);
    }
  }

  async function resetLanAccess() {
    try {
      const url = await invoke<string>("reset_lan_access");
      setQrUrl(url);
      setShowQr(true);
    } catch (reason) {
      onError(reason);
    }
  }

  return (
    <>
      <div className="setting-row remote-setting">
        <div className="setting-copy">
          <strong>Remote Control</strong>
          <button className="info-button" aria-label="About Remote Control" aria-expanded={openInfo} onClick={() => setOpenInfo((open) => !open)}><Info size={13} /></button>
          {openInfo && <p>Open the existing League remote on your phone over a trusted private Wi-Fi network or your Tailscale network.</p>}
        </div>
        <Switch checked={remote.enabled} onCheckedChange={toggle} aria-label="Remote Control" />
      </div>
      {remote.state === "starting" && <div className="setting-status"><LoaderCircle className="spin" size={13} /> Starting the remote service…</div>}
      {remote.enabled && <>
        <div className="remote-transport-row">
          <div className="remote-transport" role="group" aria-label="Remote Control transport">
            <button type="button" className={transport === "lan" ? "is-active" : ""} aria-pressed={transport === "lan"} onClick={() => changeTransport("lan")}>LAN</button>
            <button type="button" className={transport === "tailscale" ? "is-active" : ""} aria-pressed={transport === "tailscale"} onClick={() => changeTransport("tailscale")}>Tailscale</button>
          </div>
          <span className="remote-transport-help">
            <button type="button" className="info-button remote-transport-info" aria-label="About LAN and Tailscale" aria-describedby="remote-transport-tooltip"><Info size={13} /></button>
            <span id="remote-transport-tooltip" className="remote-transport-tooltip" role="tooltip">LAN connects over your local Wi-Fi or Ethernet and works without Tailscale. It uses HTTP and requires a trusted Private Windows network. Tailscale uses your Tailscale network over HTTPS; Tailscale must be connected on your PC and phone.</span>
          </span>
        </div>
        {transport === "lan" ? (
          <>
            <div className="setting-status"><span className={`remote-status-dot ${remote.lanAddress ? "is-good" : "is-bad"}`} />Local network · {remote.lanAddress ? "Ready" : "Unavailable"}</div>
            {remote.lanAddress ? <>
              <div className="remote-address"><Button variant="outline" className="remote-qr-toggle" aria-label={showQr ? "Hide LAN pairing QR code" : "Generate LAN pairing QR code"} aria-controls="remote-qr-panel" aria-expanded={showQr} onClick={() => void toggleQr()}><QrCode size={15} />{showQr ? "Hide QR" : "QR"}</Button><Button variant="outline" onClick={() => void resetLanAccess()}>Reset LAN Access</Button></div>
              {showQr && qrUrl && <div id="remote-qr-panel" className="remote-qr-panel">
                <div className="remote-qr-image"><QRCodeSVG value={qrUrl} size={176} level="M" marginSize={4} bgColor="#ffffff" fgColor="#18181b" title="LAN pairing QR code" /></div>
                <p>Scan on a phone connected to this trusted private network. Pairing links expire after five minutes.</p>
              </div>}
              {remote.localAddress && <p className="remote-transport-message">Discovery: {remote.localAddress}. A paired phone that switches to it once keeps working when the PC IP changes.</p>}
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
              <Button variant="outline" disabled={copied} onClick={() => void copyAddress()}>{copied ? "Copied" : "Copy"}</Button>
              <Button variant="outline" className="remote-qr-toggle" aria-label={showQr ? "Hide Tailscale QR code" : "Show Tailscale QR code"} aria-controls="remote-qr-panel" aria-expanded={showQr} onClick={() => void toggleQr()}><QrCode size={15} /> QR</Button>
            </div>
            {showQr && qrUrl && <div id="remote-qr-panel" className="remote-qr-panel">
              <div className="remote-qr-image"><QRCodeSVG value={qrUrl} size={176} level="M" marginSize={4} bgColor="#ffffff" fgColor="#18181b" title="Tailscale remote address QR code" /></div>
              <p>Scan with a phone connected to your Tailscale network.</p>
            </div>}
          </> : <>
            <div className="setting-status"><span className="remote-status-dot is-bad" />Tailscale · {remote.tailscaleInstalled ? "Not connected" : "Not installed"}</div>
            <p className="remote-transport-message">{!remote.tailscaleInstalled
              ? "Install Tailscale to use this transport. LAN remains available on a Private network."
              : !remote.tailscaleRunning
                ? "Connect Tailscale on this PC and your phone to use this transport."
                : remote.message ?? "Tailscale Serve could not be started."}</p>
          </>
        )}
        <PairedDevices />
      </>}
    </>
  );
}
