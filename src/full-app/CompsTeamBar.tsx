import { useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { lobbyCandidates, playerLabel, toggleActive, ROLES, type Role, type RosterMember, type TeamPlayer } from "./compsModel";
import "./comps-team.css";

type Member = { puuid: string; riotId: string; games?: number };
type Props = {
  me: Member | null;
  roster: RosterMember[];
  lobby: Member[];
  suggested: { puuid: string; riotId: string; games: number }[];
  team: TeamPlayer[];
  active: string[];
  onActiveChange: (next: string[]) => void;
  roleAssign: { assigned: Record<string, Role>; clash: string[]; open: Role[] };
  roleOverrides: Record<string, Role>;
  onRoleOverride: (puuid: string, role: Role | null) => void;
  onRosterChange: (next: RosterMember[]) => void;
  onOpenPools: () => void;
  teamLoading: boolean;
  signedIn: boolean;
};

const profileIcon = (id: number) => `https://cdn.communitydragon.org/latest/profile-icon/${id}`;
const POS_FILE: Record<Role, string> = { top: "top", jungle: "jungle", mid: "middle", bot: "bottom", support: "utility" };
const posIcon = (r: Role) => `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-clash/global/default/assets/images/position-selector/positions/icon-position-${POS_FILE[r]}.png`;
const ROLE_NAME: Record<Role, string> = { top: "Top", jungle: "Jungle", mid: "Mid", bot: "Bot", support: "Support" };
const RANK = ["1st", "2nd", "3rd", "4th", "5th"];

function AvatarImg({ player, label }: { player?: TeamPlayer; label: string }) {
  return player?.profileIconId != null
    ? <img className="cp-av-img" src={profileIcon(player.profileIconId)} alt="" />
    : <span className="cp-av-img cp-av-letter" aria-hidden="true">{label.slice(0, 1).toUpperCase()}</span>;
}

export function CompsTeamBar(p: Props) {
  const [open, setOpen] = useState(false);
  const [menuPuuid, setMenuPuuid] = useState<string | null>(null);
  const [editPuuid, setEditPuuid] = useState<string | null>(null);
  const [nickDraft, setNickDraft] = useState("");
  const [addText, setAddText] = useState("");
  const [addError, setAddError] = useState("");
  const [adding, setAdding] = useState(false);
  const byPuuid = new Map(p.team.map(t => [t.puuid, t]));
  const lobbySet = new Set(p.lobby.map(m => m.puuid));
  const guests = lobbyCandidates(p.lobby, p.roster.map(r => r.puuid), p.me?.puuid ?? null);
  const full = p.active.length >= 5;
  const labelOf = (puuid: string) => { const t = byPuuid.get(puuid); return t ? playerLabel(t) : "Player"; };

  const addToRoster = async (m: { puuid: string; riotId: string }, activate: boolean) => {
    if (p.roster.some(r => r.puuid === m.puuid)) { if (activate) p.onActiveChange(toggleActive(p.active, m.puuid)); return; }
    let roles: Role[] = [];
    if (isTauri()) { try { roles = await invoke<Role[]>("comps_prefill_roles", { puuid: m.puuid }); } catch { roles = []; } }
    p.onRosterChange([...p.roster, { puuid: m.puuid, riotId: m.riotId, nickname: null, roles: roles ?? [] }]);
    if (activate) p.onActiveChange(toggleActive(p.active, m.puuid));
  };
  const addByRiotId = () => {
    const riotId = addText.trim();
    if (!riotId) return;
    setAddError("");
    void invoke<Member>("comps_resolve", { riotId })
      .then(m => { void addToRoster(m, false); setAddText(""); setAdding(false); setNickDraft(""); setEditPuuid(m.puuid); })
      .catch(() => setAddError("No player found for that ID."));
  };
  const clashTip = (role: Role) => {
    const names = p.roleAssign.clash.filter(id => p.roleAssign.assigned[id] === role).map(labelOf);
    const joined = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}` : names[0];
    return `${joined} both want ${ROLE_NAME[role]}.${p.roleAssign.open.length ? ` Nobody is on ${p.roleAssign.open.map(r => ROLE_NAME[r]).join(" or ")}.` : ""} Click the role icon to change it.`;
  };

  const renderAvatar = (puuid: string) => {
    const t = byPuuid.get(puuid);
    const role = p.roleAssign.assigned[puuid] ?? t?.mainRole ?? "mid";
    const clash = p.roleAssign.clash.includes(puuid);
    const manual = Boolean(p.roleOverrides[puuid]);
    const label = t ? playerLabel(t) : "Player";
    const tip = clash ? clashTip(role) : `${t?.riotId ?? ""} · ${ROLE_NAME[role]}${manual ? ", set by you" : ""}`;
    return <span key={puuid} className={`cp-av${clash ? " clash" : ""}`} data-tip={tip}>
      <AvatarImg player={t} label={label} />
      {lobbySet.has(puuid) && <i className="cp-lobbydot" title="In your lobby" />}
      <button type="button" className={`cp-av-role${manual ? " set" : ""}`} aria-label={`${label}: ${ROLE_NAME[role]}. Change role`} onClick={() => setMenuPuuid(menuPuuid === puuid ? null : puuid)}>
        <img src={posIcon(role)} alt="" />
      </button>
      {clash && <i className="cp-av-warn" aria-hidden="true">!</i>}
      <small>{label}</small>
      {menuPuuid === puuid && <span className="cp-menu" role="menu">
        <em>{label} · role this session</em>
        {ROLES.map(r => {
          const prefs = t?.roles ?? [];
          const rank = prefs.indexOf(r);
          const taken = p.active.find(id => id !== puuid && p.roleAssign.assigned[id] === r);
          return <button key={r} type="button" className={role === r ? "on" : ""} onClick={() => { p.onRoleOverride(puuid, r); setMenuPuuid(null); }}>
            <img className="cp-pos" src={posIcon(r)} alt="" />
            <span>{ROLE_NAME[r]}<small>{rank < 0 ? "off-role" : `${RANK[rank]} choice`}</small></span>
            <small>{taken ? labelOf(taken) : "open"}</small>
          </button>;
        })}
        <hr />
        {manual && <button type="button" className="plain" onClick={() => { p.onRoleOverride(puuid, null); setMenuPuuid(null); }}>↺ Back to auto-assign</button>}
        <button type="button" className="plain" onClick={() => { p.onActiveChange(p.active.filter(id => id !== puuid)); setMenuPuuid(null); }}>Sit out this session</button>
      </span>}
    </span>;
  };

  const memberCard = (member: RosterMember | null, selfPlayer?: TeamPlayer) => {
    const puuid = member?.puuid ?? selfPlayer!.puuid;
    const t = byPuuid.get(puuid);
    const on = p.active.includes(puuid);
    const roles = member?.roles ?? t?.roles ?? [];
    const label = t ? playerLabel(t) : "Player";
    return <div key={puuid} role="button" tabIndex={0} className={`cp-mem${on ? " on" : full ? " full" : ""}${editPuuid === puuid ? " editing" : ""}`}
      title={on ? "Active. Click to sit out" : full ? "5 active. Turn someone off first" : "Click to make active"}
      onClick={() => p.onActiveChange(toggleActive(p.active, puuid))}>
      <span className="cp-pf"><AvatarImg player={t} label={label} /></span>
      <span><strong>{label}</strong><small>{member?.riotId ?? selfPlayer?.riotId}</small>
        <span className="rs-roles">{roles.map((r, i) => <img key={r} className={i === 0 ? "pos main" : "pos"} src={posIcon(r)} alt={ROLE_NAME[r]} title={ROLE_NAME[r]} />)}{lobbySet.has(puuid) && <i>in lobby</i>}</span></span>
      {member && <button type="button" className={`cp-edit${editPuuid === puuid ? " on" : ""}`} aria-label={`Edit ${label}`}
        onClick={e => { e.stopPropagation(); setEditPuuid(editPuuid === puuid ? null : puuid); setNickDraft(member.nickname ?? ""); }}>✎</button>}
    </div>;
  };

  const editing = p.roster.find(r => r.puuid === editPuuid) ?? null;
  const suggested = p.suggested.filter(s => !p.roster.some(r => r.puuid === s.puuid));

  return <section className="cp-teambar ui-card">
    <span className="cp-team-label"><b>Your team</b>{p.active.length > 0 && <small>{p.active.length} of 5 playing</small>}</span>
    {p.lobby.length > 0 && <span className="cp-pill lobby"><i className="cp-lobbydot" />Lobby · {p.lobby.length}</span>}
    {p.teamLoading && p.active.length === 0 ? <span className="cp-muted">Loading your team…</span>
      : p.active.length > 0 || guests.length > 0 ? <span className="cp-squad">
          {p.active.map(renderAvatar)}
          {guests.map(g => <button key={g.puuid} type="button" className="cp-av ghost" data-tip="New teammate · click to add to roster" aria-label={`Add ${g.riotId.split("#")[0]} to your roster`} onClick={() => void addToRoster(g, true)}>
            <span className="cp-av-img cp-av-letter" aria-hidden="true">{(g.riotId.split("#")[0] || "P").slice(0, 1).toUpperCase()}</span>
            <i className="cp-av-plus" aria-hidden="true">+</i><small>{g.riotId.split("#")[0] || "Player"}</small>
          </button>)}
        </span>
      : <span className="cp-muted">{p.signedIn ? "Nobody active" : "Sign in to League Client to load your team and champion pools."}</span>}
    {!p.roster.length && !open && p.signedIn && <span className="cp-invite">Add the friends you play with, then pick who's in each session.<button type="button" className="cp-btn primary" onClick={() => setOpen(true)}>+ Add your first friend</button></span>}
    <div className="cp-tools">
      <button type="button" className={`cp-btn${open ? " primary" : ""}`} aria-expanded={open} onClick={() => { setOpen(!open); setEditPuuid(null); setMenuPuuid(null); }}>Roster <em>{p.roster.length}</em></button>
      <button type="button" className="cp-btn" onClick={p.onOpenPools}>Pools</button>
    </div>
    {open && <div className={`cp-roster${guests.length || suggested.length ? "" : " wide"}`}>
      <div>
        <h4>Roster <small>{p.roster.length} friends · the same list on every account</small><b>{p.active.length} of 5 active</b></h4>
        <div className="cp-members">
          {p.me && byPuuid.get(p.me.puuid) ? memberCard(null, byPuuid.get(p.me.puuid)) : null}
          {p.roster.map(m => memberCard(m))}
          {adding ? <div className="cp-mem adding"><input autoFocus aria-label="Riot ID" placeholder="Name#TAG" value={addText} onChange={e => setAddText(e.target.value)} onKeyDown={e => { if (e.key === "Enter") addByRiotId(); if (e.key === "Escape") { setAdding(false); setAddError(""); } }} /><button type="button" className="cp-btn primary" onClick={addByRiotId}>Add</button><button type="button" className="cp-edit" aria-label="Cancel" onClick={() => { setAdding(false); setAddError(""); }}>✕</button></div> : <button type="button" className="cp-mem add" title="Add a friend by Riot ID" aria-label="Add a friend by Riot ID" onClick={() => { setAdding(true); setEditPuuid(null); }}>+</button>}
          {editing && <div className="ed">
            <div className="ed-id"><AvatarImg player={byPuuid.get(editing.puuid)} label={labelOf(editing.puuid)} /><span><strong>{labelOf(editing.puuid)}</strong><small>{editing.riotId}</small></span></div>
            <label className="ed-f"><span className="ed-l">Nickname</span><input aria-label="Nickname" value={nickDraft} placeholder={editing.riotId.split("#")[0]} onChange={e => setNickDraft(e.target.value)} /></label>
            <div className="ed-f"><span className="ed-l">Roles <small>click in the order they play them</small></span>
              <div className="ed-roles">{ROLES.map(r => {
                const i = editing.roles.indexOf(r);
                return <button key={r} type="button" className={`${i < 0 ? "" : "on"}${i === 0 ? " main" : ""}`} aria-pressed={i >= 0} title={i < 0 ? ROLE_NAME[r] : `${ROLE_NAME[r]} \u2014 choice ${i + 1}`} onClick={() => {
                  if (i < 0) p.onRosterChange(p.roster.map(m => m.puuid === editing.puuid ? { ...m, roles: [...m.roles, r] } : m));
                  else if (editing.roles.length > 1) p.onRosterChange(p.roster.map(m => m.puuid === editing.puuid ? { ...m, roles: m.roles.filter(x => x !== r) } : m));
                }}><img className="pos" src={posIcon(r)} alt="" />{i >= 0 && <i>{i + 1}</i>}<small>{ROLE_NAME[r]}</small></button>;
              })}</div>
              <span className="ed-sum">{editing.roles.map((r, i) => <span key={r}>{i ? ", then " : ""}<b>{ROLE_NAME[r]}</b>{i ? "" : " first"}</span>)}. Click a picked role to remove it.</span></div>
            <div className="ed-actions"><button type="button" className="cp-btn primary" onClick={() => { p.onRosterChange(p.roster.map(m => m.puuid === editing.puuid ? { ...m, nickname: nickDraft.trim() || null } : m)); setEditPuuid(null); }}>Done</button><button type="button" className="ed-remove" onClick={() => { p.onRosterChange(p.roster.filter(m => m.puuid !== editing.puuid)); p.onActiveChange(p.active.filter(id => id !== editing.puuid)); setEditPuuid(null); }}>Remove from roster</button></div>
          </div>}
        </div>
        {addError && <small role="alert" className="cp-add-err">{addError}</small>}
        {!p.roster.length && !p.me && <p className="cp-muted">No friends yet. Add someone from your lobby, suggestions, or by Riot ID.</p>}
      </div>
      {(guests.length > 0 || suggested.length > 0) && <div className="cp-side">
        {guests.length > 0 && <><h4>New teammates in your lobby</h4>{guests.map(g => <div key={g.puuid} className="cp-sugg">
          <span className="cp-pf"><span className="cp-av-img cp-av-letter" aria-hidden="true">{(g.riotId.split("#")[0] || "P").slice(0, 1).toUpperCase()}</span></span>
          <span><strong>{g.riotId}</strong><small>In your lobby now</small></span>
          <button type="button" className="cp-btn" aria-label={`Add ${g.riotId}`} onClick={() => void addToRoster(g, true)}>+</button></div>)}</>}
        {suggested.length > 0 && <><h4>Suggested from your games</h4>{suggested.map(s => <div key={s.puuid} className="cp-sugg">
          <span className="cp-pf"><span className="cp-av-img cp-av-letter" aria-hidden="true">{(s.riotId.split("#")[0] || "P").slice(0, 1).toUpperCase()}</span></span>
          <span><strong>{s.riotId}</strong><small>{s.games} games with you</small></span>
          <button type="button" className="cp-btn" aria-label={`Add ${s.riotId}`} onClick={() => void addToRoster(s, false)}>+</button></div>)}</>}
        <small className="cp-muted">Roles are filled in from their recent games. Change them any time.</small>
      </div>}
    </div>}
  </section>;
}
