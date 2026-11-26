import { useEffect } from "react";
import "./app-tailwind.css";
import { TooltipProvider } from "../../../src/full-app/ui/Tooltip";
import { installFlyoutMock } from "../../../src/flyout-preview-mock";
import App from "../../../src/App";
import { createRealisticDemoRuntime } from "./realisticData";
import { DEMO_ACCOUNTS, type DemoSyncMessage } from "./profiles";

let controller: ReturnType<typeof installFlyoutMock> | null = null;

function ensureMock() {
  if (controller) return controller;
  const initialActiveId = new URLSearchParams(location.search).get("active") ?? DEMO_ACCOUNTS[0].id;
  const runtime = createRealisticDemoRuntime();
  controller = installFlyoutMock({
    accounts: runtime.accounts,
    initialActiveId,
    hotkey: null,
    overrides({ getActiveId }) {
      const shared = runtime.createSharedHandlers(getActiveId);
      return {
        get_current_patch: shared.get_current_patch,
        get_tier_list: shared.get_tier_list,
        get_duo_list: shared.get_duo_list,
        get_champion_list: shared.get_champion_list,
        get_champion_overview: shared.get_champion_overview,
        get_champion_counters: shared.get_champion_counters,
        get_other_build: shared.get_other_build,
        get_keystone_build: shared.get_keystone_build,
        get_matchup: shared.get_matchup,
        get_matchup_guide: shared.get_matchup_guide,
        get_pro_builds: shared.get_pro_builds,
        get_runes: shared.get_runes,
        full_app_draft: shared.full_app_draft,
        full_app_item_names: shared.full_app_item_names,
        champion_abilities: shared.champion_abilities,
        full_app_abilities: shared.full_app_abilities,
        ability_icon: shared.ability_icon,
        champion_icon: shared.champion_icon,
        champion_art: shared.champion_art,
        item_icon: shared.item_icon,
        spell_icon: shared.spell_icon,
        rank_icon: (a) => `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests/${String(a.tier).replace(/_plus$/, "").replace(/^all$/, "unranked")}.svg`,
        list_paired_lan_devices: () => [],
      };
    },
    onSwitch(id) {
      if (window.parent !== window) {
        const msg: DemoSyncMessage = { type: "swapper-demo:switched", id, source: "tray" };
        window.parent.postMessage(msg, location.origin);
      }
    },
  });
  return controller;
}

export function TrayDemoHost() {
  const mock = ensureMock();

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.origin !== location.origin) return;
      const data = event.data as DemoSyncMessage | null;
      if (data?.type === "swapper-demo:sync" && typeof data.id === "string") {
        mock.syncActiveId(data.id);
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [mock]);

  return (
    <TooltipProvider>
      <App />
    </TooltipProvider>
  );
}
