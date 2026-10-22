import { invoke } from "@tauri-apps/api/core";
import { Wrench, X } from "lucide-react";
import { BackIcon, ErrorIcon, InfoIcon } from "./full-app/ui/icons";
import { Button } from "@/components/ui/button";
import { BrandIcon } from "@/components/BrandIcon";
import { RunesPanel } from "./runes/RunesPanel";
import { ChampionTab } from "./runes/ChampionTab";
import { PairingNotice } from "./settings/PairedDevices";
import { SettingsView } from "./settings/SettingsView";
import { PageHeader } from "./full-app/ui/PageHeader";
import { OnboardingGate } from "./onboarding/Onboarding";
import { AccountsView, AccountForms, AccountMenu } from "./app/AccountViews";
import { useAppController } from "./app/useAppController";
import type { FullAppBridge } from "./app/types";
import "./App.css";
import "./flyout.css";

export type { Account, DetectOutcome, RemoteStatus, AppState, View, FullAppBridge } from "./app/types";

function App({ fullApp }: { fullApp?: FullAppBridge } = {}) {
  const RunesSurface = fullApp?.Runes ?? RunesPanel;
  const SettingsSurface = fullApp?.Settings ?? SettingsView;
  const controller = useAppController(fullApp);
  const {
    data, view, inChampSelect, modeSpin, setModeSpin, busy, error, setError, repairableId, setRepairableId,
    useDeceive, koreanFont, ltkInstalling, ltkInstallError, riotExe, setRiotExe, remoteTransport,
    setRemoteTransport, settingsTab, setSettingsTab, remoteQrRequest, startOnStartup, runes, runesLoading,
    runesBusy, runesError, shopNotice, runeSession, native, showError, loadRunes, importItemBuild,
    applyRunes, pickSpell, allowLanFirewall, setImportItemsWithRunes, setApplySpellsWithRunes,
    loadProBuilds, loadKeystoneBuild, loadMatchup, championApi, setAutoApply, setRuneTier,
    setNotifications, setReadyCheckNotifications, setKoreanFontSetting, installLtkManager, setRemote,
    setStartOnStartupSetting, navigate, startRepair, pageTitle, footerText, footerLoggedIn, manualPath,
    configuredPath, applySettings, changeRiotPathMode, commitRiotPath, applyHotkey, browseRiotPath,
    toolsOpen, brandMode, otherMode, switchLabel, themed, modeName,
  } = controller;

  return (
    <div className={`shell dark${fullApp ? " full-app-embedded" : " flyout-topbar"}${themed ? " flyout-themed" : ""}`}>
      {!fullApp && <header className="topbar">
        <button type="button" className={`brand-button mode-${brandMode}`} aria-label={switchLabel} title={switchLabel} onClick={() => {
          setModeSpin((spin) => spin + 180);
          navigate(otherMode);
        }}>
          <span className="brand-icon" style={{ transform: `rotate(${modeSpin}deg)` }}>
            <span style={{ transform: `rotate(${-modeSpin}deg)` }}>
              {brandMode === "accounts" ? <BrandIcon className="brand-mark" size={22} /> : (
                <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path fillRule="evenodd" d="M12 1.8C6.6 1.8 3.4 5.8 3.4 10.6v5L7.2 20.4v2.2l3-2V22h3.6v-1.4l3 2v-2.2l3.8-4.8v-5C20.6 5.8 17.4 1.8 12 1.8zM6 9.8h5V19l-2.2 1.4v-8.2H6zM18 9.8h-5V19l2.2 1.4v-8.2H18z" />
                </svg>
              )}
            </span>
          </span>
          <span className="brand-copy">
            <strong>{brandMode === "accounts" ? "Swapper" : "Champions"}{toolsOpen && inChampSelect && <i className="brand-live-dot" aria-label="Live champion select" />}</strong>
            <span>⇄ <span className={`mode-${toolsOpen ? "accounts" : "champions"}`}>{modeName(toolsOpen ? "accounts" : "champions")}</span></span>
          </span>
        </button>
        <button className="icon-button close-button" aria-label="Hide Swapper" onClick={() => { if (native) void invoke("hide_flyout"); }}> <X size={16} /> </button>
      </header>}

      <main className={`main-panel${view === "settings" ? " settings-panel" : ""}${toolsOpen ? " runes-panel-host" : ""}`}>
        {view === "accounts" ? (
          <AccountsView controller={controller} fullApp={fullApp} />
        ) : view === "champions" ? (
          <div className="runes-panel runes-desktop">
            <div className="runes-body">
              <ChampionTab
                mode="desktop"
                initialTier={data.runeTier}
                prefillChampion={null}
                prefillPosition=""
                sessionKey={String(runeSession)}
                api={championApi}
                onLoadMatchup={loadMatchup}
              />
            </div>
          </div>
        ) : view === "runes" ? (
          <RunesSurface
            mode="desktop"
            view={runes}
            loading={runesLoading}
            busy={runesBusy}
            error={runesError}
            shopNotice={shopNotice}
            onApply={applyRunes}
            onToggleAutoApply={(enabled) => void setAutoApply(enabled)}
            onToggleSpellsWithRunes={(enabled) => void setApplySpellsWithRunes(enabled)}
            onToggleImportItems={(enabled) => void setImportItemsWithRunes(enabled)}
            onPickSpell={(slot, spellId) => void pickSpell(slot, spellId)}
            onPositionChange={(position) => void loadRunes(position)}
            onImportItems={importItemBuild}
            onLoadProBuilds={loadProBuilds}
            onLoadBuild={loadKeystoneBuild}
            onLoadMatchup={loadMatchup}
            championApi={championApi}
            onTierChange={(tier) => void setRuneTier(tier)}
            sessionKey={String(runeSession)}
          />
        ) : (
          <>
            {fullApp ? <PageHeader title={pageTitle} crumbs={view === "settings" ? undefined : [{ label: "Accounts", onClick: () => navigate("accounts") }]} /> : <div className="panel-heading compact">
              <button className="icon-button back-button" aria-label="Back to accounts" onClick={() => navigate("accounts")}><BackIcon size={17} /></button>
              <div><p className="eyebrow">Swapper</p><h1>{pageTitle}</h1></div>
            </div>}

            <AccountForms controller={controller} />
            {view === "settings" && <SettingsSurface
              state={data}
              busy={busy !== null}
              tab={settingsTab}
              onTabChange={setSettingsTab}
              useDeceive={useDeceive}
              onUseDeceiveChange={(checked) => void applySettings({ useDeceive: checked, riotExe: configuredPath() })}
              startOnStartup={startOnStartup}
              onStartOnStartupChange={(enabled) => void setStartOnStartupSetting(enabled)}
              manualPath={manualPath}
              riotExe={riotExe}
              onRiotExeChange={setRiotExe}
              onPathModeChange={changeRiotPathMode}
              onRiotPathCommit={commitRiotPath}
              onBrowseRiotPath={() => void browseRiotPath()}
              onSetHotkey={applyHotkey}
              onNotificationsChange={(enabled) => void setNotifications(enabled)}
              onReadyCheckChange={(enabled) => void setReadyCheckNotifications(enabled)}
              koreanFont={koreanFont}
              onKoreanFontChange={(enabled) => void setKoreanFontSetting(enabled)}
              ltkInstalled={data.ltkInstalled}
              ltkInstalling={ltkInstalling}
              ltkInstallError={ltkInstallError}
              onInstallLtk={() => void installLtkManager()}
              onAutoApplyChange={(enabled) => void setAutoApply(enabled)}
              onApplySpellsChange={(enabled) => void setApplySpellsWithRunes(enabled)}
              onImportItemsChange={(enabled) => void setImportItemsWithRunes(enabled)}
              onAllowLanFirewall={allowLanFirewall}
              onRuneTierChange={(tier) => void setRuneTier(tier)}
              remoteTransport={remoteTransport}
              onRemoteTransportChange={setRemoteTransport}
              onRemoteToggle={(enabled) => void setRemote(enabled)}
              qrRequest={remoteQrRequest}
              onError={showError}
            />}
          </>
        )}
      </main>
      <AccountMenu controller={controller} />

      {!native && !error && <div className="error-banner" role="status"><InfoIcon size={16} /><span>Browser preview only. Account actions need the Windows tray app.</span></div>}
      {/* The Fullsized App mounts its own, once, around this embedded copy. */}
      {!fullApp && <OnboardingGate />}
      <PairingNotice />
      {error && <div className="error-banner" role="alert"><ErrorIcon size={16} /><span>{error}</span>{repairableId && <Button size="sm" variant="outline" className="repair-banner-action" onClick={() => startRepair(repairableId)}><Wrench size={14} /> Repair account</Button>}<button aria-label="Dismiss error" onClick={() => { setRepairableId(null); setError(null); }}><X size={14} /></button></div>}
      {/* The Fullsized App shows status in its sidebar podium instead. */}
      {!fullApp && <footer className="footer"><span className="footer-status"><span className={footerLoggedIn ? "status-dot good" : "status-dot idle"} /> {footerText}</span><span className="footer-mode">{data.useDeceive ? "DECEIVE" : "RIOT CLIENT"}</span></footer>}
    </div>
  );
}

export default App;
