import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as browseForFile } from "@tauri-apps/plugin-dialog";
import { Download, ExternalLink, Heart, Info, LayoutGrid, List, Loader2, Plus, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { NearViewport } from "./NearViewport";
import { ModGalleryDialog } from "./ModGalleryDialog";
import { Tooltip } from "./ui/Tooltip";
import { ChampionPicker, Picker } from "./Picker";
import { ChampionIcon } from "../runes/ChampionIcon";
import type { ChampionOption } from "../runes/types";
import { PageHeader } from "./ui/PageHeader";
import { EmptyState, Notice, Skeleton } from "./ui/States";
import { SearchIcon } from "./ui/icons";
import { fmtCompact, fmtCount } from "./ui/format";
import {
  RF_CATEGORIES,
  browseArgs,
  changeNotice,
  clashIds,
  filterLibrary,
  formatModDate,
  installedMod,
  libraryCategoryLabel,
  libraryCounts,
  libraryOptions,
  modInitial,
  modHealth,
  modMeta,
  needsEnableConfirm,
  nextNotice,
  notWorking,
  visibleMods,
  type BrowseSort,
  type LibraryMod,
  type LibraryTab,
  type ModsView,
  type RfList,
  type RfMod,
} from "./customModsModel";
import "./custom-mods.css";

type Props = {
  ltkInstalled: boolean;
  ltkOutdated: boolean;
  ltkNeedsSetup: boolean;
  ltkInstalling: boolean;
  onInstallLtk: () => void;
  koreanFont: boolean;
  onKoreanFont: (enabled: boolean) => void;
};

type Layout = "grid" | "list";

const UNREACHABLE = "RuneForge isn't reachable right now.";
const LAYOUT_KEY = "swapper.customMods.layout";
const LIBRARY_TABS: [LibraryTab, string][] = [["all", "All"], ["on", "On"], ["off", "Off"], ["clashes", "Clashes"], ["problems", "Problems"]];

function readLayout(): Layout {
  try { return localStorage.getItem(LAYOUT_KEY) === "list" ? "list" : "grid"; } catch { return "grid"; }
}

/** Loads a data-URL thumbnail once mounted (inside `NearViewport`, so only near the viewport). */
function ThumbArt({ load }: { load: () => Promise<string | null> }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    load().then((url) => { if (live) setSrc(url); }).catch(() => undefined);
    return () => { live = false; };
    // The loader is rebuilt every render; the card's key changes when its mod does.
  }, []);
  return src ? <img src={src} alt="" decoding="async" /> : null;
}

function Badge({ tone, title, children }: { tone?: "brand" | "warn" | "danger"; title?: string; children: ReactNode }) {
  return <span className={`cm-badge${tone ? ` is-${tone}` : ""}`} title={title}>{children}</span>;
}

/** One mod as a card (grid) or a row (list): thumbnail with corner badges, title, author, actions pinned to the bottom. */
function ModCard({ name, author, meta, note, ask, thumb, badges, actions, aside, dim, onOpen }: {
  name: string;
  /** Makes the whole card open something: the title becomes the button and stretches over the card. */
  onOpen?: () => void;
  author: string;
  meta?: string;
  /** A short hint under the author (e.g. how to repair it). */
  note?: string;
  /** A question that replaces the actions until answered. */
  ask?: ReactNode;
  thumb: (() => Promise<string | null>) | null;
  badges?: ReactNode;
  actions: ReactNode;
  aside: ReactNode;
  dim?: boolean;
}) {
  return (
    <li className={`cm-card${dim ? " is-off" : ""}${onOpen ? " is-openable" : ""}`}
      // The title button is the keyboard path; a click anywhere else on the card, except its own controls, opens it too.
      onClick={onOpen && ((e) => { if (!(e.target as Element).closest("button, a, input, [role=switch]")) onOpen(); })}>
      <div className="cm-thumb">
        <span className="cm-initial" aria-hidden>{modInitial(name)}</span>
        {thumb && isTauri() && <NearViewport className="cm-thumb-art"><ThumbArt load={thumb} /></NearViewport>}
        {badges && <span className="cm-badges">{badges}</span>}
      </div>
      <div className="cm-body">
        <strong className="cm-title" title={name}>
          {onOpen ? <button type="button" className="cm-open" aria-label={`View previews of ${name}`} onClick={onOpen}>{name}</button> : name}
        </strong>
        <small className="cm-by">By {author || "Unknown author"}</small>
        {meta && <small className="cm-meta">{meta}</small>}
        {note && <small className="cm-note">{note}</small>}
        {ask ?? (
          <div className="cm-foot">
            <span className="cm-actions">{actions}</span>
            <span className="cm-aside">{aside}</span>
          </div>
        )}
      </div>
    </li>
  );
}

function ModSwitch({ mod, onToggle }: { mod: LibraryMod; onToggle: (mod: LibraryMod, enabled: boolean) => void }) {
  return <Switch checked={mod.enabled} aria-label={`Turn ${mod.name} ${mod.enabled ? "off" : "on"}`} onCheckedChange={(checked) => onToggle(mod, checked)} />;
}

function SearchField({ value, label, onChange }: { value: string; label: string; onChange: (value: string) => void }) {
  return (
    <label className="cm-search">
      <SearchIcon size={16} />
      <input type="text" value={value} placeholder={label} aria-label={label} onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Escape" && value) { e.preventDefault(); onChange(""); } }} />
      {value && (
        <button type="button" className="cm-search-clear" aria-label="Clear search" onClick={() => onChange("")}>
          <X size={14} aria-hidden />
        </button>
      )}
    </label>
  );
}

/** A compact switch for a setting built into LTK Manager. */
function Builtin({ name, hint, info, checked, onChange }: {
  name: string; hint: string; info: string; checked: boolean; onChange: (on: boolean) => void;
}) {
  return (
    <div className={`cm-builtin${checked ? " is-on" : ""}`}>
      <span className="cm-builtin-copy">
        <span className="cm-builtin-name">{name}
          <Tooltip label={info}><button type="button" className="cm-builtin-info" aria-label={`About ${name}`}><Info aria-hidden /></button></Tooltip>
        </span>
        <small>{hint}</small>
      </span>
      <Switch checked={checked} aria-label={name} onCheckedChange={onChange} />
    </div>
  );
}

export function CustomModsPage({ ltkInstalled, ltkOutdated, ltkNeedsSetup, ltkInstalling, onInstallLtk, koreanFont, onKoreanFont }: Props) {
  const [tab, setTab] = useState<"browse" | "library">("library");
  const [view, setView] = useState<ModsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);
  const [pendingEnable, setPendingEnable] = useState<string | null>(null);
  const [previewId, setPreviewId] = useState<string | null>(null);
  const native = isTauri();

  const [libSearch, setLibSearch] = useState("");
  const [libCategory, setLibCategory] = useState("");
  const [libChampion, setLibChampion] = useState("");
  const [libTab, setLibTab] = useState<LibraryTab>("all");
  const [layout, setLayout] = useState<Layout>(readLayout);

  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [championId, setChampionId] = useState<number | null>(null);
  const [allChampions, setAllChampions] = useState<ChampionOption[]>([]);
  useEffect(() => {
    if (native) void invoke<ChampionOption[]>("get_champion_list").then(setAllChampions).catch(() => {});
  }, [native]);
  const [sort, setSort] = useState<BrowseSort>("trending");
  const [workingOnly, setWorkingOnly] = useState(true);
  const [browse, setBrowse] = useState<RfList | null>(null);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const requestSeq = useRef(0);

  const load = useCallback(() => {
    if (!native) return;
    invoke<ModsView>("mods_list").then(setView).catch((e) => setError(String(e)));
  }, [native]);

  useEffect(() => {
    load();
    if (!native) return;
    const un = listen<string>("mods_conflict", ({ payload }) => {
      setNotice(`Another mod in the same slot was turned off (${payload}).`);
    });
    // A queued change applied (after a game, or once LTK Manager closed).
    const changed = listen("mods_changed", () => {
      invoke<ModsView>("mods_list").then((next) => { setView(next); setNotice((prev) => nextNotice(prev, next)); }).catch(() => undefined);
    });
    return () => { void un.then((f) => f()); void changed.then((f) => f()); };
  }, [load, native]);

  const fetchBrowse = useCallback((page: number, append: boolean) => {
    if (!native) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    invoke<RfList>("runeforge_list", browseArgs({ page, search, category, championId, sort }))
      .then((next) => {
        if (seq !== requestSeq.current) return;
        setBrowseError(null);
        setBrowse((prev) => (append && prev ? { ...next, mods: [...prev.mods, ...next.mods] } : next));
      })
      .catch(() => { if (seq === requestSeq.current) setBrowseError(UNREACHABLE); })
      .finally(() => { if (seq === requestSeq.current) setLoading(false); });
  }, [native, search, category, championId, sort]);

  useEffect(() => {
    if (tab === "browse") fetchBrowse(1, false);
  }, [tab, fetchBrowse]);

  const chooseLayout = (next: Layout) => {
    setLayout(next);
    try { localStorage.setItem(LAYOUT_KEY, next); } catch { /* per-viewer convenience only */ }
  };

  const apply = (next: ModsView) => {
    setView(next);
    setNotice(changeNotice(next));
  };
  const run = (cmd: string, args?: Record<string, unknown>) => {
    setError(null);
    invoke<ModsView>(cmd, args).then(apply).catch((e) => setError(String(e)));
  };
  const setEnabled = (mod: LibraryMod, enabled: boolean) => run("mods_set_enabled", { id: mod.id, enabled });
  const toggle = (mod: LibraryMod, enabled: boolean) => {
    if (enabled && needsEnableConfirm(mod)) setPendingEnable(mod.id);
    else setEnabled(mod, enabled);
  };
  /** "Turn on anyway" for a mod LTK found fatal errors in. */
  const enableAsk = (mod: LibraryMod) => pendingEnable === mod.id && (
    <div className="cm-ask" role="group" aria-label={`Turn ${mod.name} on?`}>
      <p>LTK found errors in this mod for the current patch. It may crash your game.</p>
      <span>
        <Button variant="ghost" size="sm" onClick={() => setPendingEnable(null)}>Cancel</Button>
        <Button variant="outline" size="sm" autoFocus onClick={() => { setPendingEnable(null); setEnabled(mod, true); }}>Turn on anyway</Button>
      </span>
    </div>
  );
  const healthBadges = (mod: LibraryMod) => {
    const health = modHealth(mod);
    return health && <>
      {health.broken && <Badge tone="danger" title={`LTK Manager's check found ${fmtCount(mod.fatals ?? 0)} fatal errors for the current patch.`}>Broken on this patch</Badge>}
      {health.repairable && <Badge tone="warn">Can be repaired</Badge>}
    </>;
  };
  const healthNote = (mod: LibraryMod) => (modHealth(mod)?.repairable ? "Open LTK Manager to repair it." : undefined);

  const addFromFile = async () => {
    if (!native) return;
    const picked = await browseForFile({
      multiple: false,
      filters: [{ name: "Mod package", extensions: ["fantome", "modpkg"] }],
    }).catch((e) => { setError(String(e)); return null; });
    if (typeof picked === "string" && picked) run("mods_add", { path: picked });
  };

  const openLink = (url: string) => {
    if (!native) return;
    invoke("runeforge_open", { url }).catch((e) => setError(String(e)));
  };

  const install = (mod: RfMod) => {
    setError(null);
    setInstallingId(mod.id);
    invoke<ModsView>("runeforge_install", { modId: mod.id, slug: mod.slug, releaseId: null })
      .then(apply)
      .catch((e) => setError(String(e)))
      .finally(() => setInstallingId(null));
  };

  const library = view?.mods ?? [];
  const counts = libraryCounts(library);
  const clashes = clashIds(library);
  const options = libraryOptions(library);
  const championIds = new Map(allChampions.map((c) => [c.name.toLowerCase(), c.id]));
  const libShown = filterLibrary(library, { search: libSearch, category: libCategory, champion: libChampion, tab: libTab });
  const libFiltered = libSearch.trim() !== "" || libCategory !== "" || libChampion !== "" || libTab !== "all";

  const shown = browse ? visibleMods(browse.mods, workingOnly) : [];
  const hasMore = browse !== null && browse.mods.length < browse.total;

  const libraryCard = (mod: LibraryMod) => (
    <ModCard
      key={mod.id}
      name={mod.name}
      author={mod.author}
      meta={layout === "list" ? modMeta(mod) : undefined}
      thumb={() => invoke<string | null>("mods_thumbnail", { id: mod.id })}
      dim={!mod.enabled}
      note={healthNote(mod)}
      ask={enableAsk(mod) || undefined}
      badges={(clashes.has(mod.id) || modHealth(mod)) && <>
        {healthBadges(mod)}
        {clashes.has(mod.id) && <Badge tone="warn" title={`Shares the ${mod.slotLabel} slot with another mod. Only one can be on.`}>Clash</Badge>}
      </>}
      actions={pendingRemove === mod.id ? (
        <span className="cm-confirm" role="group" aria-label={`Remove ${mod.name}?`}>
          <Button variant="ghost" size="sm" onClick={() => setPendingRemove(null)}>Cancel</Button>
          <Button variant="destructive" size="sm" autoFocus onClick={() => { setPendingRemove(null); run("mods_remove", { id: mod.id }); }}>Remove</Button>
        </span>
      ) : (
        <>
          {mod.pageUrl && (
            <Button variant="ghost" size="icon-sm" aria-label={`Open ${mod.name} on RuneForge`} title="Open on RuneForge" onClick={() => openLink(mod.pageUrl!)}>
              <ExternalLink aria-hidden />
            </Button>
          )}
          <Button variant="ghost" size="icon-sm" aria-label={`Remove ${mod.name}`} title="Remove" onClick={() => setPendingRemove(mod.id)}>
            <Trash2 aria-hidden />
          </Button>
        </>
      )}
      aside={<ModSwitch mod={mod} onToggle={toggle} />}
    />
  );

  const installButton = (mod: RfMod, owned: LibraryMod | undefined, size: "icon-sm" | "default") => {
    const installing = installingId === mod.id;
    return owned ? <ModSwitch mod={owned} onToggle={toggle} /> : (
      <Button variant="outline" size={size} disabled={installing || installingId !== null} aria-label={installing ? `Installing ${mod.name}` : `Install ${mod.name}`} title="Install" onClick={() => install(mod)}>
        {installing ? <Loader2 className="cm-spin" aria-hidden /> : <Plus aria-hidden />}{size === "default" && (installing ? "Installing…" : "Install")}
      </Button>
    );
  };

  const browseCard = (mod: RfMod) => {
    const owned = view ? installedMod(mod, view.mods) : undefined;
    // The last update tells you more than the first post: map mods get re-released every patch.
    const updated = mod.updatedAt ?? mod.publishedAt;
    const date = formatModDate(updated);
    const published = formatModDate(mod.publishedAt);
    return (
      <ModCard
        key={mod.id}
        name={mod.name}
        author={mod.author}
        onOpen={() => setPreviewId(mod.id)}
        thumb={mod.thumbnailUrl ? () => invoke<string>("runeforge_thumbnail", { url: mod.thumbnailUrl }) : null}
        note={owned && healthNote(owned)}
        ask={(owned && enableAsk(owned)) || undefined}
        badges={(owned || notWorking(mod)) && <>
          {owned && <Badge tone="brand">Installed</Badge>}
          {owned && healthBadges(owned)}
          {notWorking(mod) && <Badge tone="warn">Not working on the current patch</Badge>}
        </>}
        actions={
          <span className="cm-stats">
            <span title={`${fmtCount(mod.downloadCount)} downloads`}><Download size={14} aria-hidden /><span className="cm-sr">Downloads: </span>{fmtCompact(mod.downloadCount)}</span>
            {mod.likeCount > 0 && <span title={mod.likeCount === 1 ? "1 like" : `${fmtCount(mod.likeCount)} likes`}><Heart size={14} aria-hidden /><span className="cm-sr">Likes: </span>{fmtCompact(mod.likeCount)}</span>}
            {date && <span title={published && published !== date ? `Updated ${date} · first posted ${published}` : `Updated ${date}`}><span className="cm-sr">Updated </span>{date}</span>}
          </span>
        }
        aside={<>
          <Button variant="ghost" size="icon-sm" aria-label={`Open ${mod.name} on RuneForge`} title="Open on RuneForge" onClick={() => openLink(mod.pageUrl)}>
            <ExternalLink aria-hidden />
          </Button>
          {installButton(mod, owned, "icon-sm")}
        </>}
      />
    );
  };

  const previewMod = previewId && browse ? browse.mods.find((m) => m.id === previewId) : undefined;

  const blocked = Boolean(view?.regionBlocked);
  const tabs = !blocked && (
    <nav className="cm-tabs" role="tablist" aria-label="Custom mods">
      {([["browse", "Browse", browse?.total], ["library", "Your library", view?.mods.length]] as const).map(([id, label, n]) => (
        <button key={id} type="button" role="tab" aria-selected={tab === id} className={tab === id ? "on" : ""} onClick={() => setTab(id)}>
          {label}{n !== undefined && <em>{fmtCompact(n)}</em>}
        </button>
      ))}
    </nav>
  );

  return (
    <div className="cm">
      {previewMod && (
        <ModGalleryDialog mod={previewMod} onClose={() => setPreviewId(null)} footer={<>
          <Button variant="outline" onClick={() => openLink(previewMod.pageUrl)}><ExternalLink aria-hidden />Open on RuneForge</Button>
          {installButton(previewMod, view ? installedMod(previewMod, view.mods) : undefined, "default")}
        </>} />
      )}
      <PageHeader title="Custom mods" actions={tabs} />
      {!ltkInstalled && (
        <Notice tone="warn" action={<Button variant="outline" disabled={ltkInstalling} onClick={onInstallLtk}>{ltkInstalling ? "Installing…" : "Install"}</Button>}>
          Custom mods need LTK Manager.
        </Notice>
      )}
      {ltkInstalled && ltkOutdated && (
        <Notice tone="warn" action={<Button variant="outline" disabled={ltkInstalling} onClick={onInstallLtk}>{ltkInstalling ? "Updating…" : "Update LTK Manager"}</Button>}>
          LTK Manager is out of date, so mods won&apos;t load.
        </Notice>
      )}
      {ltkInstalled && !ltkOutdated && ltkNeedsSetup && (
        <Notice tone="warn" action={<Button variant="outline" disabled={ltkInstalling} onClick={onInstallLtk}>{ltkInstalling ? "Setting up…" : "Set up LTK Manager"}</Button>}>
          Swapper sets LTK Manager to run in the background and apply your mods every game.
        </Notice>
      )}
      {blocked ? (
        <Notice tone="warn">Custom mods aren&apos;t available in this region.</Notice>
      ) : (
        <>
          {error && <Notice tone="error">{error}</Notice>}
          {notice && <Notice>{notice}</Notice>}
          {tab === "browse" ? (
            <section aria-label="Browse RuneForge">
              <div className="cm-filters">
                <SearchField value={search} label="Search RuneForge" onChange={setSearch} />
                <Picker label="Category" hideLabel value={category} onChange={setCategory}
                  options={RF_CATEGORIES.map((c) => ({ value: c.value, label: c.value ? c.label : "All categories" }))} />
                <ChampionPicker label="All champions" value={championId ?? 0} champions={browse?.champions ?? []}
                  onChange={(id) => setChampionId(id || null)} />
                <Picker label="Sort" hideLabel value={sort} onChange={(v) => setSort(v as BrowseSort)}
                  options={[{ value: "trending", label: "Trending" }, { value: "downloads", label: "Most downloaded" }, { value: "newest", label: "Newest" }, { value: "updated", label: "Recently updated" }]} />
                <label className="cm-working">
                  <Switch checked={workingOnly} onCheckedChange={setWorkingOnly} />
                  Working on current patch only
                </label>
              </div>
              {browseError && (
                <Notice tone="error" action={<Button variant="outline" onClick={() => openLink("https://runeforge.dev/mods")}>Open RuneForge</Button>}>
                  {browseError}
                </Notice>
              )}
              {!browseError && !browse && loading && <Skeleton label="Loading RuneForge mods" rows={3} height={96} />}
              {!browseError && browse && shown.length === 0 && !loading && (
                <EmptyState title="No mods found">Try a different search or filter.</EmptyState>
              )}
              <ul className="cm-grid">{shown.map(browseCard)}</ul>
              {loading && browse && <p className="cm-loading" role="status">Loading…</p>}
              {hasMore && !loading && (
                <div className="cm-more">
                  <Button variant="outline" onClick={() => browse && fetchBrowse(browse.page + 1, true)}>Load more</Button>
                </div>
              )}
            </section>
          ) : (
            <section aria-labelledby="cm-library-title">
              <div className="cm-head">
                <h2 id="cm-library-title">Your library<span className="cm-chip">{counts.all === 1 ? "1 mod" : `${fmtCount(counts.all)} mods`}</span></h2>
                <div className="cm-head-actions">
                  <Button variant="outline" disabled={counts.on === 0} onClick={() => run("mods_disable_all")}>Turn off all mods</Button>
                  <Button variant="outline" onClick={() => void addFromFile()}><Plus aria-hidden />Add from file</Button>
                </div>
              </div>
              {ltkInstalled && (
                <div className="cm-builtins" role="group" aria-label="Built into LTK Manager">
                  <Builtin name="Korean font" hint="Korean text in game" info="Loads a font with Korean characters through LTK Manager, so Korean names and chat show up in game."
                    checked={koreanFont} onChange={onKoreanFont} />
                </div>
              )}
              {native && !view && !error && <Skeleton label="Loading your library" rows={3} height={64} />}
              {view && library.length === 0 && (
                <EmptyState title="No mods yet">Add a .fantome or .modpkg file, or install one from Browse.</EmptyState>
              )}
              {library.length > 0 && <>
                <div className="cm-filters">
                  <SearchField value={libSearch} label="Search your library" onChange={setLibSearch} />
                  <Picker label="Category" hideLabel value={libCategory} onChange={setLibCategory}
                    options={[{ value: "", label: "All categories" }, ...options.categories.map((c) => ({ value: c, label: libraryCategoryLabel(c) }))]} />
                  <Picker label="Champion" hideLabel searchable value={libChampion} onChange={setLibChampion}
                    options={[{ value: "", label: "All champions" }, ...options.champions.map((name) => {
                      const id = championIds.get(name.toLowerCase());
                      return { value: name, label: name, icon: id ? <ChampionIcon id={id} name={name} mode="desktop" /> : undefined };
                    })]} />
                </div>
                <div className="cm-bar">
                  <div className="cm-seg" role="group" aria-label="Show">
                    {LIBRARY_TABS.map(([id, label]) => (
                      <button key={id} type="button" aria-pressed={libTab === id} className={libTab === id ? "on" : ""} onClick={() => setLibTab(id)}>
                        {label}<em>{counts[id]}</em>
                      </button>
                    ))}
                  </div>
                  <div className="cm-seg is-icons" role="group" aria-label="Layout">
                    <button type="button" aria-pressed={layout === "grid"} aria-label="Grid view" title="Grid view" className={layout === "grid" ? "on" : ""} onClick={() => chooseLayout("grid")}><LayoutGrid size={16} aria-hidden /></button>
                    <button type="button" aria-pressed={layout === "list"} aria-label="List view" title="List view" className={layout === "list" ? "on" : ""} onClick={() => chooseLayout("list")}><List size={16} aria-hidden /></button>
                  </div>
                </div>
                {libShown.length === 0 ? (
                  <EmptyState title={!libSearch && !libCategory && !libChampion && libTab === "clashes" ? "No clashes" : !libSearch && !libCategory && !libChampion && libTab === "problems" ? "No problems found" : "No mods match"}>
                    {libFiltered && "Try a different search or filter."}
                  </EmptyState>
                ) : (
                  <ul className={layout === "list" ? "cm-list" : "cm-grid"}>{libShown.map(libraryCard)}</ul>
                )}
              </>}
            </section>
          )}
        </>
      )}
    </div>
  );
}
