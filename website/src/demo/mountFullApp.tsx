import { useEffect } from "react";
import { emit } from "@tauri-apps/api/event";
import "./app-tailwind.css";
import { installFullAppMock } from "../../../src/full-app/full-app-preview-mock";
import { createRealisticDemoRuntime } from "./realisticData";
import { DEMO_PROFILES, type DemoSyncMessage } from "./profiles";

let mounted = false;
let controller: ReturnType<typeof installFullAppMock> | null = null;

export function FullAppDemoHost() {
  useEffect(() => {
    if (!mounted) {
      mounted = true;
      const params = new URLSearchParams(location.search);
      const initialActiveId = params.get("active") ?? (params.has("draft-guide") ? "vesper" : DEMO_PROFILES[0].account.id);
      const runtime = createRealisticDemoRuntime();

      controller = installFullAppMock({
        accounts: runtime.accounts,
        initialActiveId,
        hotkey: null,
        overrides({ getActiveId, isLive, isDraft }) {
          const shared = runtime.createSharedHandlers(getActiveId);
          // Settings → Remote: a LAN-ready PC on an invented private address, so "Pair a phone" shows its QR code.
          let remoteOn = false;
          const remote = () => ({
            enabled: remoteOn, state: remoteOn ? "running" : "disabled", address: null, tailscaleAddress: null,
            lanAddress: remoteOn ? "192.168.1.20" : null, lanMessage: null, lanFirewallNeeded: false, message: null,
            tailscaleInstalled: false, tailscaleRunning: false, dnsName: null, localAddress: remoteOn ? "swapper.local" : null,
            leagueRunning: true, lcuConnected: true, lanReconnectNeeded: false, lanReconnectOldAddress: null,
            lanReconnectNewAddress: null, lanReconnectSince: null,
          });
          const pairingUrl = () => "http://192.168.1.20:7878/pair?x=1";
          return {
            ...shared,
            probe_remote: remote,
            set_remote_enabled: (a) => ((remoteOn = Boolean(a.enabled)), remote()),
            create_lan_pairing_url: pairingUrl,
            reset_lan_access: pairingUrl,
            live_game_view: (a) => (isLive() ? shared.live_game_view(a) : null),
            live_game_live: (a) => (isLive() ? shared.live_game_live(a) : null),
            get_runes: (a) => (isDraft() ? shared.get_runes(a) : undefined),
            full_app_draft: (a) => (isDraft() ? shared.full_app_draft(a) : undefined),
          };
        },
        onSwitch(id) {
          if (window.parent !== window) {
            const msg: DemoSyncMessage = { type: "swapper-demo:switched", id, source: "full" };
            window.parent.postMessage(msg, location.origin);
          }
        },
      });

      void import("../../../src/full-app/main").then(() => {
        if (params.has("draft") || params.has("draft-guide") || params.has("glass-draft")) {
          const targetId = params.has("draft-guide") ? "vesper" : initialActiveId;
          const b = runtime.bundles.get(targetId) ?? runtime.bundles.get(DEMO_PROFILES[0].account.id)!;
          setTimeout(() => {
            void emit("champ_select", {
              phase: "ChampSelect",
              championId: b.draftView.runes.championId,
              championName: b.draftView.runes.championName,
              position: b.draftView.runes.position,
              locked: true,
            });
          }, 150);
        }
      });
    }

    const onMessage = (event: MessageEvent) => {
      if (event.origin !== location.origin) return;
      const data = event.data as DemoSyncMessage | null;
      if (data?.type === "swapper-demo:sync" && typeof data.id === "string") {
        controller?.syncActiveId(data.id);
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  return null;
}
