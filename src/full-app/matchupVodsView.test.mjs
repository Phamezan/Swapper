import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as logic from "./matchupVodModel.ts";
import * as guide from "./matchupGuide.ts";
import * as youtube from './youtubePlayer.ts';
import { ahriVsZedVods } from "./fixtures/ahriVsZedVods.ts";

const photos = {};
function harness(file, component, vods = ahriVsZedVods, championVods = vods.map(vod => ({ ...vod, enemy: 238, lane: "mid" }))) {
  const states = [], effects = [], focus = [], domRefs = new Set(), messages = [], listeners = new Map(), timers = new Map(), fullscreen = [], scrolls = [];
  const contentWindow = { postMessage: (data, origin) => messages.push({ data: JSON.parse(data), origin }) };
  const browser = { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name), setInterval: fn => { timers.set('interval', fn); return 'interval'; }, clearInterval: name => timers.delete(name), setTimeout: fn => { timers.set('timeout', fn); return 'timeout'; }, clearTimeout: name => timers.delete(name) };
  const documentListeners = new Map();
  const fakeDocument = { fullscreenElement: null, addEventListener: (name, fn) => documentListeners.set(name, fn), removeEventListener: name => documentListeners.delete(name), exitFullscreen: async () => { fullscreen.push('element off'); fakeDocument.fullscreenElement = null; } };
  const tauriWindow = { maximized: false, isMaximized: async () => tauriWindow.maximized, toggleMaximize: async () => { tauriWindow.maximized = !tauriWindow.maximized; fullscreen.push(tauriWindow.maximized ? 'maximize' : 'unmaximize'); },
    setFullscreen: async on => { fullscreen.push(`window ${on ? 'on' : 'off'}`); } };
  let cursor = 0, pending = [], props, request, requestKey;
  const react = {
    useId: () => "test-id", useRef(initial) { const i = cursor++; return states[i] ??= { current: initial }; },
    useState(initial) { const i = cursor++; if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial; return [states[i], value => { states[i] = typeof value === 'function' ? value(states[i]) : value; }]; },
    useEffect(fn, deps) { const i = cursor++; if (!effects[i] || deps.some((dep, j) => dep !== effects[i].deps[j])) pending.push(() => { effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() }; }); },
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    "@tauri-apps/api/core": { isTauri: () => true, invoke: async () => [{ id: 103, name: "Ahri" }, { id: 238, name: "Zed" }] },
    "../runes/ChampionIcon": { ChampionIcon: "champion-icon" }, "./VodMomentArt": { VodMomentArt: "event-art" },
    react, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" }, "lucide-react": { Play: "play", ChevronRight: "chevron", ArrowLeft: "back-icon" },
    "./matchupVodModel": logic, "./matchupGuide": guide, "./loadMatchupVods": { loadMatchupVods: async () => vods, loadChampionVods: async () => championVods, loadAllVods: async () => championVods.map(vod => ({ ...vod, champion: vod.champion ?? (vod.enemy === 103 ? 238 : 103) })) },
    "./Picker": { ChampionPicker: "champion-picker", Picker: "picker", ROLES: [["mid", "Mid"], ["top", "Top"]] }, "./NearViewport": { NearEnd: "near-end" },
    './youtubePlayer': youtube, './YoutubeVodPlayer': { YoutubeVodPlayer: 'youtube-player' },
    "./art": { useChampionArt: () => null }, "./GuideSourceButton": { GuideSourceButton: "source-button" },
    "./VodPlayer": { VodPlayer: "player-identity" }, "./PlayerPhoto": { usePlayerPhoto: name => photos[name] ?? null }, "../runes/TeamLogo": { TeamLogo: "team-logo" }, "../runes/RankCrest": { RankCrest: "rank-crest" },
    "./ui/States": { Skeleton: "skeleton" }, "./MomentIcon": { MomentIcon: "moment-icon" }, "@tauri-apps/api/window": { getCurrentWindow: () => tauriWindow }, "./MatchupGuideContent": { GuideSectionContent: "guide-content" },
    "./MatchupGuideSummary": { MatchupGuideSummary: "summary" }, "./TradeWindows": { TradeWindows: "trades" }, "./PowerSpikeTimeline": { PowerSpikeTimeline: "spikes" },
  };
  const source = readFileSync(new URL(file, import.meta.url), "utf8");
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", "window", 'location', 'document', code)(name => modules[name] ?? {}, exports, browser, { origin: 'http://tauri.localhost' }, fakeDocument);
  const render = value => {
    props = value ?? props; cursor = 0; if (component === 'MatchupVods') {
      const key = `${props.champion?.id}-${props.enemy?.id}-${props.lane}`;
      if (key !== requestKey) { requestKey = key; request = { champion: props.champion ?? null, enemy: props.enemy, lane: props.lane }; }
    }
    const tree = exports[component](component === 'MatchupVods' ? { ...props, request } : props);
    for (const ref of domRefs) ref.current = null;
    for (const node of nodes(tree)) if (node.props.ref && typeof node.props.ref === "object") {
      const ref = node.props.ref; domRefs.add(ref); ref.current = { contentWindow, querySelector: () => null, contains: () => false, focus: () => focus.push(node.props["data-vod-id"] ?? node.props.className), children: [], scrollHeight: 0, clientHeight: 0,
        requestFullscreen: async () => { fullscreen.push('element on'); fakeDocument.fullscreenElement = ref.current; } };
    }
    pending.splice(0).forEach(fn => fn()); return tree;
  };
  return { render, tauriWindow, focus, messages, contentWindow, timers, fullscreen, fakeDocument, documentListeners, receive(event) { listeners.get('message')?.(event); }, unmount() { effects.forEach(effect => effect?.cleanup?.()); }, async flush() { for (let i = 0; i < 5; i++) await new Promise(setImmediate); return render(); } };
}
// The player's header is a prop like its children; walk both.
function nodes(tree) { return !tree || typeof tree !== "object" ? [] : Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.header), ...nodes(tree.props?.children)]; }
function text(tree) { return typeof tree === "string" || typeof tree === "number" ? String(tree) : !tree || typeof tree !== "object" ? "" : Array.isArray(tree) ? tree.map(text).join("") : text(tree.props?.children); }
const props = { champion: { id: 103, name: "Ahri" }, enemy: { id: 238, name: "Zed" }, lane: "mid", onGuideSection() {} };

test('moment cards render roster portraits and unnamed replays show a rank crest', async () => {
  const vod = { ...ahriVsZedVods[0], player: undefined, result: undefined, moments: [{ atSec: 30, label: 'Ahri takes down Zed', kind: 'kill' }] };
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [vod]);
  h.render(props);
  nodes(await h.flush()).find(node => node.props.className === 'mv-vod-row').props.onClick();
  const tree = h.render();
  const art = nodes(tree).find(node => node.type === 'event-art');
  const portraits = harness('./VodMomentArt.tsx', 'VodMomentArt').render(art.props);
  assert.deepEqual(nodes(portraits).filter(node => node.type === 'champion-icon').map(node => node.props.id), [103, 238]);
  const identity = harness('./VodPlayer.tsx', 'VodPlayer').render({ vod });
  const avatar = nodes(identity).find(node => node.props.className === 'mv-avatar');
  assert.equal(nodes(avatar).find(node => node.type === 'rank-crest').props.size, 26);
  assert.match(text(identity), /replay/);
});

test('question-mark players read as a rank replay with no player photo', () => {
  const tree = harness('./VodPlayer.tsx', 'VodPlayer').render({ vod: { ...ahriVsZedVods[0], player: ' ? ', team: undefined, playerImage: '/unknown.png' } });
  assert.doesNotMatch(text(tree), /\?/);
  assert.equal(text(tree), 'Challenger replay');
  assert.equal(nodes(tree).some(node => node.type === 'img'), false);
});

test('unnamed events get ordinal art and death events retain their explicit kind', () => {
  const h = harness('./VodMomentArt.tsx', 'VodMomentArt');
  const tree = h.render({ moment: { label: 'Caught at river', kind: 'death' }, champions: [], index: 7 });
  assert.equal(text(nodes(tree).find(node => node.props.className === 'mv-event-number')), '08');
  assert.equal(tree.props['data-kind'], 'death');
  assert.equal(nodes(tree).find(node => node.type === 'moment-icon').props.kind, 'death');
});

test("VODs start as a five-game list and Back restores focus to the opened row", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods");
  h.render(props);
  let tree = await h.flush();
  assert.equal(nodes(tree).filter(node => node.props.className === "mv-vod-row").length, 5);
  assert.equal(nodes(tree).some(node => node.props.className === "mv-frame"), false);
  assert.doesNotMatch(text(tree), /Played as/);
  nodes(tree).filter(node => node.props.className === "mv-vod-row")[2].props.onClick();
  tree = h.render();
  assert.equal(nodes(tree).find(node => node.props.className === "mv-game-info").props.title, ahriVsZedVods[2].title, "the YouTube title is a tooltip, not a second heading");
  assert.equal(h.focus[h.focus.length - 1], "mv-back");
  nodes(tree).find(node => node.props.className === "mv-back").props.onClick();
  tree = h.render();
  assert.equal(nodes(tree).filter(node => node.props.className === "mv-vod-row").length, 5);
  assert.equal(h.focus[h.focus.length - 1], ahriVsZedVods[2].videoId);
});

test("Escape leaves the media view without triggering the parent matchup navigation", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods");
  h.render(props);
  const list = await h.flush();
  nodes(list).find(node => node.props.className === "mv-vod-row").props.onClick();
  const player = h.render();
  let prevented = false, stopped = false;
  player.props.onKeyDown({ key: "Escape", preventDefault() { prevented = true; }, stopPropagation() { stopped = true; } });
  assert.equal(prevented && stopped, true);
  assert.equal(nodes(h.render()).filter(node => node.props.className === "mv-vod-row").length, 5);
  assert.equal(h.focus[h.focus.length - 1], ahriVsZedVods[0].videoId);
});

test("fixture playback stays inert while moment tags navigate to their exact guide section", async () => {
  const sections = [];
  const h = harness("./MatchupVods.tsx", "MatchupVods");
  h.render({ ...props, onGuideSection: section => sections.push(section) });
  const list = await h.flush();
  nodes(list).find(node => node.props.className === "mv-vod-row").props.onClick();
  const tree = h.render();
  assert.ok(nodes(tree).filter(node => node.props.className === 'mv-moment').every(node => node.props.disabled));
  const player = nodes(tree).find(node => node.type === 'youtube-player');
  const preview = harness('./YoutubeVodPlayer.tsx', 'YoutubeVodPlayer').render(player.props);
  assert.match(text(preview), /Preview, no video/);
  assert.equal(nodes(preview).some(node => node.type === 'iframe'), false);
  nodes(tree).find(node => node.props.className === "mv-section-link").props.onClick();
  assert.deepEqual(sections, ["early-lane"]);
});

test('moment clicks seek repeatedly and the mini player can reopen or close', async () => {
  let reopened = 0;
  const real = { ...ahriVsZedVods[0], fixture: undefined, videoId: 'A_b-cD12345' };
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [real]);
  h.render({ ...props, onReopen: () => reopened++ });
  nodes(await h.flush()).find(node => node.props.className === 'mv-vod-row').props.onClick();
  let tree = h.render();
  const moment = nodes(tree).find(node => node.props.className === 'mv-moment');
  moment.props.onClick(); tree = h.render();
  let player = nodes(tree).find(node => node.type === 'youtube-player');
  assert.equal(player.props.seek.moment.atSec, 184);
  assert.equal(player.props.seek.serial, 1);
  nodes(tree).filter(node => node.props.className === 'mv-moment')[1].props.onClick();
  tree = h.render({ ...props, active: false, onReopen: () => reopened++ });
  assert.match(tree.props.className, /is-mini/);
  player = nodes(tree).find(node => node.type === 'youtube-player');
  assert.equal(player.props.seek.serial, 2);
  assert.equal(player.props.mini, true, 'the mini player drops fullscreen and the bigger controls');
  nodes(tree).find(node => node.props['aria-label'] === 'Reopen VOD').props.onClick();
  assert.equal(reopened, 1);
  nodes(tree).find(node => node.props['aria-label'] === 'Close mini player').props.onClick();
  assert.equal(h.render(), null);
});

const timeline = tree => nodes(tree).find(node => node.props.onJump);
const control = (tree, label) => nodes(tree).find(node => node.props.label === label);
const realVod = { ...ahriVsZedVods[0], fixture: undefined, videoId: 'A_b-cD12345' };

test('the playing moment is highlighted in the feed with its kind icon', async () => {
  const vod = { ...realVod, moments: realVod.moments.map((moment, i) => i === 1 ? { ...moment, kind: 'dragon' } : moment) };
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [vod]);
  h.render(props);
  nodes(await h.flush()).find(node => node.props.className === 'mv-vod-row').props.onClick();
  let tree = h.render();
  assert.equal(nodes(tree).some(node => node.props.className === 'is-active'), false);
  nodes(tree).find(node => node.type === 'youtube-player').props.onMomentChange(1);
  tree = h.render();
  const active = nodes(tree).filter(node => node.type === 'li' && node.props.className === 'is-active');
  assert.equal(active.length, 1);
  const row = nodes(active[0]).find(node => node.props.className === 'mv-moment');
  assert.equal(row.props['aria-current'], true);
  assert.match(text(row), new RegExp(vod.moments[1].label));
  assert.equal(nodes(row).find(node => node.type === 'event-art').props.moment.kind, 'dragon');
});

test('keyboard shortcuts drive the player and fullscreen covers the window until Esc', async () => {
  const changes = [];
  const h = harness('./YoutubeVodPlayer.tsx', 'YoutubeVodPlayer');
  let tree = h.render({ vod: realVod, seek: null, onMomentChange: index => changes.push(index) });
  h.receive({ source: h.contentWindow, origin: youtube.YOUTUBE_ORIGIN, data: JSON.stringify({ event: 'infoDelivery', info: { currentTime: 600, duration: 1746, playerState: 2 } }) });
  tree = h.render();
  assert.equal(changes.at(-1), logic.currentMomentIndex(realVod.moments, 600));
  const press = (key, target = { tagName: 'DIV' }) => {
    const result = { prevented: false, stopped: false };
    tree.props.onKeyDown({ key, target, preventDefault() { result.prevented = true; }, stopPropagation() { result.stopped = true; } });
    tree = h.render();
    return result;
  };
  press('k');
  assert.equal(h.messages.pop().data.func, 'playVideo');
  press('j');
  assert.deepEqual(h.messages.pop().data.args, [590, true]);
  press('ArrowRight');
  assert.deepEqual(h.messages.pop().data.args, [595, true]);
  press(']');
  assert.equal(h.messages.pop().data.func, 'playVideo');
  assert.deepEqual(h.messages.pop().data.args, [logic.adjacentMoment(realVod.moments, 595, 1).atSec, true], '] jumps to the next key moment');
  const sent = h.messages.length;
  assert.equal(press('k', { tagName: 'INPUT' }).prevented, false);
  assert.equal(h.messages.length, sent, 'keys typed into a field are left alone');

  press('f');
  await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ['window on', 'element on']);
  assert.match(tree.props.className, /is-fullscreen/);
  assert.equal(control(tree, 'Exit full screen').props.shortcut, 'f');
  assert.deepEqual(press('Escape'), { prevented: true, stopped: true }, 'Esc leaves fullscreen, not the VOD');
  await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ['element off', 'window off']);
  assert.doesNotMatch(tree.props.className, /is-fullscreen/);
  assert.deepEqual(press('Escape'), { prevented: false, stopped: false }, 'a later Esc closes the VOD as before');

  press('f');
  await h.flush();
  h.fullscreen.splice(0);
  h.fakeDocument.fullscreenElement = null;
  h.documentListeners.get('fullscreenchange')();
  await h.flush();
  tree = h.render();
  assert.deepEqual(h.fullscreen.splice(0), ['window off'], 'the browser leaving element fullscreen also restores the window');
  assert.doesNotMatch(tree.props.className, /is-fullscreen/);
  assert.deepEqual(press('Escape'), { prevented: true, stopped: true }, 'the Esc that made the browser leave fullscreen does not also close the VOD');

  press('f');
  await h.flush();
  h.fullscreen.splice(0);
  tree = h.render({ vod: realVod, seek: null, mini: true });
  await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ['element off', 'window off'], 'switching to the mini player leaves fullscreen');
  press('f');
  await h.flush();
  assert.deepEqual(h.fullscreen, [], 'the mini player ignores the fullscreen key');
});

test('element fullscreen waits for the native resize and native errors stay visible', async () => {
  const h = harness('./YoutubeVodPlayer.tsx', 'YoutubeVodPlayer');
  let finishResize;
  h.tauriWindow.setFullscreen = () => new Promise(resolve => { finishResize = resolve; });
  let tree = h.render({ vod: realVod, seek: null });
  const entering = control(tree, 'Full screen').props.onClick();
  await h.flush();
  assert.deepEqual(h.fullscreen, [], 'the element must not capture the old window size');
  finishResize();
  await entering;
  tree = await h.flush();
  assert.deepEqual(h.fullscreen, ['element on']);
  h.tauriWindow.setFullscreen = async () => { throw new Error('permission denied'); };
  control(tree, 'Exit full screen').props.onClick();
  tree = await h.flush();
  assert.match(text(tree), /Could not leave window fullscreen: Error: permission denied/);
  control(tree, 'Full screen').props.onClick();
  tree = await h.flush();
  assert.match(text(tree), /Window fullscreen unavailable: Error: permission denied/);
});

test('player receives trusted progress, jumps to moments and shows a failed embed with a way out', () => {
  const vod = { ...ahriVsZedVods[0], fixture: undefined, videoId: 'A_b-cD12345' };
  const h = harness('./YoutubeVodPlayer.tsx', 'YoutubeVodPlayer');
  let tree = h.render({ vod, seek: null });
  const iframe = nodes(tree).find(node => node.type === 'iframe');
  assert.match(iframe.props.src, /controls=1&fs=0.*origin=http%3A%2F%2Ftauri.localhost/);
  iframe.props.onLoad();
  assert.deepEqual(h.messages.pop(), { data: { event: 'listening', id: 1 }, origin: youtube.YOUTUBE_ORIGIN });
  h.receive({ source: h.contentWindow, origin: 'https://evil.io', data: JSON.stringify({ event: 'infoDelivery', info: { currentTime: 99 } }) });
  assert.ok(timeline(h.render()).props.disabled);
  h.timers.get('timeout')();
  tree = h.render();
  assert.match(text(tree), /Video unavailable or slow to load/);
  assert.equal(nodes(tree).find(node => node.type === 'source-button').props.url, 'https://www.youtube.com/watch?v=A_b-cD12345&t=0', 'the notice offers YouTube itself');
  assert.equal(timeline(tree), undefined, 'no rail without the player connection');
  h.receive({ source: h.contentWindow, origin: youtube.YOUTUBE_ORIGIN, data: JSON.stringify({ event: 'infoDelivery', info: { currentTime: 184, duration: 1746, playerState: 1 } }) });
  tree = h.render();
  assert.doesNotMatch(text(tree), /Video unavailable/);
  assert.deepEqual([timeline(tree).props.disabled, timeline(tree).props.current, timeline(tree).props.duration], [false, 184, 1746]);
  timeline(tree).props.onJump(412);
  assert.equal(h.messages.pop().data.func, 'playVideo');
  assert.deepEqual(h.messages.pop().data.args, [412, true]);
  assert.equal(timeline(h.render()).props.current, 412, 'jumps show on the rail immediately');
  h.render({ vod, seek: { moment: vod.moments[1], serial: 1 } });
  assert.equal(h.messages.pop().data.func, 'playVideo');
  assert.deepEqual(h.messages.pop().data.args, [vod.moments[1].atSec, true]);
  h.receive({ source: h.contentWindow, origin: youtube.YOUTUBE_ORIGIN, data: JSON.stringify({ event: 'onError', info: 150 }) });
  assert.match(text(h.render()), /Video unavailable or slow to load/, 'an embed error shows the notice too');
  h.unmount();
  assert.equal(h.timers.size, 0);
});

test('a VOD without moments gets no rail and no sidebar; one with moments gets both, and fullscreen can hide the sidebar', async () => {
  const sidebar = 'key-moments';
  const h = harness('./YoutubeVodPlayer.tsx', 'YoutubeVodPlayer');
  let tree = h.render({ vod: { ...realVod, moments: [] }, seek: null, header: 'header', children: sidebar });
  assert.equal(timeline(tree), undefined);
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-side'), false, 'the picture takes the full width');
  assert.equal(tree.props.style['--mv-bars'], 1, 'a slim bar under the picture keeps fullscreen bottom right');
  assert.ok(control(tree, 'Full screen'));
  assert.equal(control(tree, 'Show key moments'), undefined, 'no moments, nothing to reopen');
  tree = h.render({ vod: realVod, seek: null, header: 'header', children: sidebar });
  assert.ok(timeline(tree));
  assert.equal(tree.props.style['--mv-bars'], 1);
  assert.equal(text(nodes(tree).find(node => node.props.className === 'mv-side')), sidebar);
  control(tree, 'Full screen').props.onClick();
  tree = await h.flush();
  assert.equal(nodes(tree).some(node => node.type === 'header'), false, 'fullscreen drops the page header');
  assert.match(text(nodes(tree).find(node => node.props.className === 'mv-side-top')), /Esc to return/);
  control(tree, 'Hide key moments').props.onClick();
  tree = h.render();
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-side'), false);
  control(tree, 'Exit full screen').props.onClick();
  tree = await h.flush();
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-side'), false, 'a closed sidebar stays closed outside fullscreen too');
  assert.ok(control(tree, 'Show key moments'), 'the bar under the picture reopens it');
  control(tree, 'Full screen').props.onClick();
  tree = await h.flush();
  assert.ok(control(tree, 'Show key moments'), 'the collapsed sidebar is remembered');
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-side'), false);
  h.render({ vod: realVod, seek: null, mini: true, header: 'header', children: sidebar });
  tree = await h.flush();
  assert.deepEqual([timeline(tree), nodes(tree).some(node => node.type === 'header' || node.props.className === 'mv-side')], [undefined, false], 'the mini player is just the video');
});

test('the player header shows the facts once and the sidebar only exists with moments', async () => {
  const vod = { ...ahriVsZedVods[0], player: 'Peng', team: 'MKOI', rankClaim: 'EUW Challenger', result: 'W', uploaded: '2020-03-04' };
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [vod, { ...ahriVsZedVods[1], moments: [] }]);
  h.render(props);
  let tree = await h.flush();
  rows(tree)[0].props.onClick();
  tree = h.render();
  const facts = nodes(tree).find(node => node.type?.name === 'VodFacts');
  assert.equal(text(facts.type(facts.props)), `EUW ChallengerPatch ${vod.patch}VictoryMar 4, 2020`);
  assert.equal(text(nodes(tree).find(node => node.props.className === 'mv-chapters').props.children[0]), `Key moments${vod.moments.length}`, 'the count appears once, in the sidebar header');
  assert.doesNotMatch(text(tree), /Match room|Match story|Match timeline/);
  nodes(tree).find(node => node.props.className === 'mv-back').props.onClick();
  tree = h.render();
  rows(tree)[1].props.onClick();
  tree = h.render();
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-chapters'), false);
});

test("player identity shows a pro's desktop team logo and an unnamed player's rank crest; a named solo player's rank sits in the facts", () => {
  const h = harness("./VodPlayer.tsx", "VodPlayer");
  let tree = h.render({ vod: { ...ahriVsZedVods[0], team: "T1" } });
  const team = nodes(tree).find(node => node.type === "team-logo");
  assert.equal(team.props.team, "T1");
  assert.equal(team.props.mode, "desktop");
  assert.equal(nodes(tree).some(node => node.type === "rank-crest"), false);
  tree = h.render({ vod: { ...ahriVsZedVods[0], team: undefined, rankClaim: "Grandmaster" } });
  assert.equal(nodes(tree).some(node => node.type === "team-logo" || node.type === "rank-crest"), false);
  tree = h.render({ vod: { ...ahriVsZedVods[0], player: undefined, team: undefined, rankClaim: "Grandmaster" } });
  const crest = nodes(tree).find(node => node.type === "rank-crest");
  assert.equal(crest.props.tier, "grandmaster");
  assert.equal(crest.props.mode, "desktop");
});

test("missing and failed player photos show initials rather than a broken image", () => {
  const h = harness("./VodPlayer.tsx", "VodPlayer");
  const vod = { ...ahriVsZedVods[0], player: "Fixture Pro 1" };
  const avatar = tree => nodes(tree).find(node => node.props.className === "mv-avatar");
  assert.equal(text(avatar(h.render({ vod }))), "FP");
  photos["Fixture Pro 1"] = "data:image/png;base64,AAAA";
  const image = nodes(h.render({ vod })).find(node => node.type === "img");
  delete photos["Fixture Pro 1"];
  assert.equal(image.props.src, "data:image/png;base64,AAAA", "the CDN headshot fills the avatar");
  image.props.onError();
  const failed = h.render();
  assert.equal(nodes(failed).some(node => node.type === "img"), false);
  assert.equal(text(avatar(failed)), "FP");
});

test("an empty champion library asks for another champion without filters", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods", [], []);
  h.render({ ...props, enemy: undefined, lane: undefined });
  const tree = await h.flush();
  assert.match(text(tree), /No Ahri VODs yet. Choose another champion/);
  assert.equal(nodes(tree).some(node => node.props.className === "mv-filters"), false);
});

const rows = tree => nodes(tree).filter(node => node.props.className === "mv-vod-row");
const labelled = (tree, label) => nodes(tree).find(node => node.props?.label === label && node.props.onChange);

test("the library pages a large matchup list and filters every Ahri VOD across opponents", async () => {
  const { ahriChampionVods: every } = await import("./fixtures/ahriChampionVods.ts");
  const zed = every.filter(vod => vod.enemy === 238 && vod.lane === "mid");
  const h = harness("./MatchupVods.tsx", "MatchupVods", zed, every);
  h.render({ ...props, enemy: undefined, lane: undefined });
  let tree = await h.flush();
  assert.equal(rows(tree).length, 60, "a long list renders its first step only");
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-match-count")), `${every.length} VODs`);
  assert.ok(labelled(tree, "Opponent"));
  const patches = rows(tree).map(row => every.find(vod => vod.videoId === row.props["data-vod-id"]).patch);
  assert.ok(patches.every((patch, i) => !i || logic.comparePatch(patches[i - 1], patch) >= 0), "newest patch first by default");
  nodes(tree).find(node => node.type === "near-end").props.onNear();
  tree = h.render();
  assert.equal(rows(tree).length, 120, "scrolling near the end renders the next step");

  assert.match(text(tree), /Browse VODs/);
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-match-count")), `${every.length} VODs`);
  assert.equal(rows(tree).length, 120, "paging retains the next step");
  const opponents = labelled(tree, "Opponent").props.options;
  assert.deepEqual([opponents[1].label, opponents[1].count], ["Zed", zed.length + every.filter(vod => vod.enemy === 238 && vod.lane !== "mid").length], "opponents are ordered by games, with portraits and a separate count");
  assert.equal(opponents[1].icon.props.id, 238);

  labelled(tree, "Opponent").props.onChange("157");
  labelled(tree, "Players").props.onChange(true);
  labelled(tree, "Rank").props.onChange("grandmaster");
  tree = h.render();
  const filters = { ...logic.ANY_VOD, enemy: 157, named: true, tier: "grandmaster" };
  const expected = logic.filterVods(every, filters);
  assert.ok(expected.length > 1);
  assert.deepEqual(rows(tree).map(row => row.props["data-vod-id"]), expected.slice(0, 60).map(vod => vod.videoId));
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-match-count")), `${expected.length} of ${every.length}`);
  assert.deepEqual(labelled(tree, "Players").props.options.map(option => option[2]),
    [false, true].map(named => every.filter(vod => logic.vodMatches(vod, { ...filters, named })).length), "each option counts what choosing it would show");
  assert.ok(rows(tree).every(row => /vs Yasuo/.test(row.props["aria-label"])));

  labelled(tree, "Patch").props.onChange("1.1");
  tree = h.render();
  assert.match(text(tree), /No VODs match these filters/);
  nodes(tree).find(node => node.props.className === "mv-text-button").props.onClick();
  tree = h.render();
  assert.equal(rows(tree).length, 60);
  assert.equal(labelled(tree, "Sort"), undefined, "VODs are always newest patch first");
  assert.equal(labelled(tree, "Opponent").props.value, "0");
});

test("a VOD from another matchup keeps its moments but cannot open this matchup's guide", async () => {
  const other = { ...ahriVsZedVods[0], enemy: 157, lane: "mid" }, same = { ...ahriVsZedVods[1], enemy: 238, lane: "mid" };
  const h = harness("./MatchupVods.tsx", "MatchupVods", [], [other, same]);
  h.render(props);
  let tree = await h.flush();
  labelled(tree, "Champion");
  nodes(tree).find(node => node.props.className === 'mv-matchup-scope').props.children[1].props.onClick();
  tree = h.render();
  const open = id => { rows(tree).find(row => row.props["data-vod-id"] === id).props.onClick(); return h.render(); };
  let player = open(other.videoId);
  assert.ok(nodes(player).some(node => node.props.className === "mv-moment"));
  assert.equal(nodes(player).some(node => node.props.className === "mv-section-link"), false);
  nodes(player).find(node => node.props.className === "mv-back").props.onClick();
  tree = h.render();
  player = open(same.videoId);
  assert.ok(nodes(player).some(node => node.props.className === "mv-section-link"));
});

test("fullscreen playback hides everything but the moment rail when idle and wakes on activity", async () => {
  const h = harness("./YoutubeVodPlayer.tsx", "YoutubeVodPlayer");
  const progress = playerState => h.receive({ source: h.contentWindow, origin: youtube.YOUTUBE_ORIGIN, data: JSON.stringify({ event: "infoDelivery", info: { currentTime: 100, duration: 1746, playerState } }) });
  let tree = h.render({ vod: realVod, seek: null });
  progress(1);
  control(h.render(), "Full screen").props.onClick();
  tree = await h.flush();
  assert.match(tree.props.className, /is-fullscreen/);
  assert.doesNotMatch(tree.props.className, /is-idle/);
  h.timers.get("timeout")();
  tree = h.render();
  assert.match(tree.props.className, /is-idle/);
  assert.ok(timeline(tree), "the rail stays, so a mouse move outside the video can wake the HUD");
  h.documentListeners.get("pointermove")();
  assert.doesNotMatch(h.render().props.className, /is-idle/, "moving the mouse brings the HUD back");
  h.timers.get("timeout")();
  assert.match(h.render().props.className, /is-idle/, "the countdown restarted after the activity");
  h.documentListeners.get("keydown")();
  assert.doesNotMatch(h.render().props.className, /is-idle/, "a key brings the HUD back");
  progress(2);
  tree = h.render();
  await h.flush();
  assert.equal(h.documentListeners.has("pointermove"), false, "paused playback keeps the HUD up");
  assert.equal(h.timers.has("timeout"), false);
  progress(1);
  h.render();
  control(h.render(), "Exit full screen").props.onClick();
  tree = await h.flush();
  assert.equal(h.documentListeners.has("pointermove"), false, "leaving fullscreen stops watching activity");
  assert.doesNotMatch(tree.props.className, /is-idle/);
});

test("a maximized window is restored before fullscreen and maximized again after it", async () => {
  const h = harness("./YoutubeVodPlayer.tsx", "YoutubeVodPlayer");
  h.tauriWindow.maximized = true;
  let tree = h.render({ vod: realVod, seek: null });
  control(tree, "Full screen").props.onClick();
  tree = await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ["unmaximize", "window on", "element on"], "a maximized borderless window would keep its taskbar-high gap");
  control(tree, "Exit full screen").props.onClick();
  tree = await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ["element off", "window off", "maximize"]);
  control(tree, "Full screen").props.onClick();
  tree = await h.flush();
  control(tree, "Exit full screen").props.onClick();
  await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ["unmaximize", "window on", "element on", "element off", "window off", "maximize"]);
  h.tauriWindow.maximized = false;
  control(h.render(), "Full screen").props.onClick();
  await h.flush();
  assert.deepEqual(h.fullscreen.splice(0), ["window on", "element on"], "a normal window is left as it is");
});

test("opening a guide from a VOD starts on the requested slide", () => {
  const h = harness("./MatchupGuideCarousel.tsx", "MatchupGuideCarousel");
  const tree = h.render({ ...props, initialSection: "mid-lane", guide: null, loading: false, abilities: {}, tier: "emerald_plus", matchup: null });
  assert.equal(text(nodes(tree).find(node => node.props.role === "tab" && node.props["aria-selected"])), "Mid lane");
  assert.equal(nodes(tree).filter(node => node.props.role === "tabpanel" && !node.props.hidden).length, 1);
});

test("general fallback is labelled above the existing carousel sections", () => {
  const guide = { champion: 103, lane: "mid", keyTips: [] };
  const h = harness("./MatchupGuideCarousel.tsx", "MatchupGuideCarousel");
  const tree = h.render({ ...props, guide, loading: false, abilities: {}, tier: "emerald_plus", matchup: null });
  assert.match(text(tree), /General Ahri guide \u00b7 no Zed matchup guide yet/);
  assert.equal(nodes(tree).find(n => n.type === "summary").props.guide, guide);
  assert.equal(nodes(tree).filter(n => n.type === "guide-content").length, 3);
});

test("library rows show the player or rank and region, a result badge and moments only when known", async () => {
  const named = { ...ahriVsZedVods[0], videoId: "named-row-1", title: "(MKOI Peng) Ahri Mid vs Zed - EUW Challenger Patch 26.19", player: "Peng", team: "MKOI", result: "W", rankClaim: "Challenger" };
  const unnamed = { ...ahriVsZedVods[0], videoId: "plain-row-1", title: "Ahri Mid vs Zed - KR Master Patch 26.18", player: "?", team: undefined, result: undefined, rankClaim: "Master", patch: "26.18", moments: [] };
  const h = harness("./MatchupVods.tsx", "MatchupVods", [named, unnamed]);
  h.render(props);
  const [first, second] = rows(await h.flush());
  assert.equal(first.props.title, named.title, "the full title stays available as a tooltip");
  assert.equal(first.props["data-result"], "win");
  assert.match(first.props["aria-label"], /^Watch Peng, Ahri vs Zed: /);
  assert.equal(text(nodes(first).find(node => node.type === "strong")), "Peng");
  assert.equal(text(nodes(first).find(node => node.props.className === "mv-team-tag")), "MKOI");
  assert.equal(text(nodes(first).find(node => node.type === "small")), `EUW Challenger · Patch ${named.patch}`);
  assert.equal(text(nodes(first).find(node => node.props.className === "mv-result-badge is-win")), "Win");
  assert.match(text(nodes(first).find(node => node.props.className === "mv-cell-moments")), new RegExp(`^${named.moments.length} moments$`));
  assert.equal(text(nodes(second).find(node => node.type === "strong")), "KR Master", "an unknown player leads with rank and region");
  assert.equal(nodes(second).find(node => node.type === "strong").props.className, "is-unnamed");
  assert.equal(text(nodes(second).find(node => node.type === "small")), "Patch 26.18");
  assert.equal(second.props["data-result"], undefined);
  assert.equal(nodes(second).some(node => String(node.props.className).startsWith("mv-result-badge")), false, "no result shows nothing, not a dash");
  assert.equal(text(nodes(second).find(node => node.props.className === "mv-cell-moments")), "", "zero moments shows nothing");
  assert.doesNotMatch(text(second), /Unlisted|–/);
});


test("filters are hidden under eight VODs and visible at eight", async () => {
  for (const count of [7, 8]) {
    const list = Array.from({ length: count }, (_, i) => ({ ...ahriVsZedVods[0], videoId: `count-${i}`, enemy: 238, lane: "mid" }));
    const h = harness('./MatchupVods.tsx', 'MatchupVods', list, list);
    h.render(props);
    const tree = await h.flush();
    assert.equal(nodes(tree).some(node => node.props.className === 'mv-filters'), count >= 8);
  }
});


test("opponent side is separate, labelled, restricted to the same lane and cannot link the guide", async () => {
  const own = { ...ahriVsZedVods[0], enemy: 238, lane: 'mid' };
  const other = { ...own, videoId: 'reverse-mid', enemy: 103 };
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [], [own, other, { ...other, videoId: 'reverse-top', lane: 'top' }]);
  h.render(props);
  let tree = await h.flush();
  assert.equal(rows(tree).length, 1);
  const perspective = labelled(tree, 'Perspective');
  assert.equal(perspective.props.options[1][1], 'From Zed’s side');
  assert.equal(perspective.props.options[1][2], 1);
  perspective.props.onChange(true);
  tree = h.render();
  assert.deepEqual(rows(tree).map(row => row.props['data-vod-id']), ['reverse-mid']);
  assert.equal(nodes(tree).find(node => node.props.className === 'mv-vod-list').props['aria-label'], 'From Zed’s side');
  rows(tree)[0].props.onClick(); tree = h.render();
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-section-link'), false);
});

test("estimated patch tooltip and upload age are visible on the row", async () => {
  const vod = { ...ahriVsZedVods[0], patchEstimated: true, uploaded: '2020-03-04' };
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [vod]);
  h.render(props);
  const tree = await h.flush();
  assert.match(text(rows(tree)[0]), /Patch ~26.21.*Mar 4, 2020/);
  assert.ok(nodes(tree).some(node => node.props.title === 'Patch estimated from the upload date'));
});

test("champion picker hands the selected champion to the section", async () => {
  const chosen = [];
  const h = harness('./MatchupVods.tsx', 'MatchupVods');
  h.render({ ...props, onChampionChange: champion => chosen.push(champion) });
  const tree = await h.flush();
  labelled(tree, 'Champion').props.onChange('238');
  assert.deepEqual(chosen, [{ id: 238, name: 'Zed' }]);
});

test("matchup entry opens the exact champion, opponent and lane or browses all when empty", async () => {
  for (const vods of [ahriVsZedVods, []]) {
    const opened = [];
    const h = harness('./MatchupVodEntry.tsx', 'MatchupVodEntry', vods);
    h.render({ ...props, onWatch: request => opened.push(request) });
    const tree = await h.flush();
    assert.match(text(tree), vods.length ? /5 VODs for Ahri vs ZedWatch/ : /No VODs for this matchupBrowse all Ahri VODs/);
    nodes(tree).find(node => node.type === 'button').props.onClick();
    assert.deepEqual(opened, [vods.length ? { champion: props.champion, enemy: props.enemy, lane: 'mid' } : { champion: props.champion }]);
  }
});


test("a small matchup hides filters even in a large champion library", async () => {
  const list = Array.from({ length: 10 }, (_, i) => ({ ...ahriVsZedVods[0], videoId: `scope-${i}`, enemy: i < 7 ? 238 : 157, lane: 'mid' }));
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [], list);
  h.render(props);
  let tree = await h.flush();
  assert.equal(rows(tree).length, 7);
  assert.equal(nodes(tree).some(node => node.props.className === 'mv-filters'), false);
  nodes(tree).find(node => node.props.className === 'mv-matchup-scope').props.children[1].props.onClick();
  tree = h.render();
  assert.equal(rows(tree).length, 10);
  assert.ok(nodes(tree).some(node => node.props.className === 'mv-filters'));
});


test("all champions is the neutral default with sorted paging and both champion portraits", async () => {
  const all = Array.from({ length: 65 }, (_, i) => ({ ...ahriVsZedVods[0], videoId: `all-${i}`, champion: i % 2 ? 238 : 103, enemy: i % 2 ? 103 : 238, lane: 'mid', patch: i === 64 ? '26.22' : '26.21' }));
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [], all);
  h.render({ ...props, champion: null, enemy: undefined, lane: undefined });
  let tree = await h.flush();
  assert.equal(labelled(tree, 'Champion').props.value, '0');
  assert.equal(labelled(tree, 'Champion').props.options[0].label, 'All champions');
  assert.equal(rows(tree).length, 60);
  assert.equal(rows(tree)[0].props['data-vod-id'], 'all-64');
  assert.match(text(rows(tree)[0]), /Ahri vs Zed.*Mid/);
  assert.deepEqual(nodes(rows(tree)[0]).filter(n => n.type === 'champion-icon').map(n => n.props.id), [103, 238]);
  assert.equal(labelled(tree, 'Perspective'), undefined);
  nodes(tree).find(n => n.type === 'near-end').props.onNear();
  tree = h.render();
  assert.equal(rows(tree).length, 65);
});

test("clearing Champion requests all VODs and reverse controls require both filters", async () => {
  const changes = [];
  const list = Array.from({ length: 9 }, (_, i) => ({ ...ahriVsZedVods[0], videoId: `clear-${i}`, enemy: 238, lane: 'mid' }));
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [], list);
  h.render({ ...props, onChampionChange: champion => changes.push(champion) });
  let tree = await h.flush();
  assert.ok(labelled(tree, 'Perspective'));
  labelled(tree, 'Champion').props.onChange('0');
  assert.deepEqual(changes, [null]);
  h.render({ ...props, champion: null, enemy: undefined, lane: undefined });
  tree = await h.flush();
  assert.equal(labelled(tree, 'Perspective'), undefined);
  labelled(tree, 'Opponent').props.onChange('238');
  tree = h.render();
  assert.equal(labelled(tree, 'Perspective'), undefined, 'opponent alone does not offer a reverse perspective');
  h.render({ ...props, enemy: undefined, lane: undefined });
  tree = await h.flush();
  assert.equal(labelled(tree, 'Perspective'), undefined, 'champion alone does not offer a reverse perspective');
});


test("returning to all champions preserves playback and the Watch guide context", async () => {
  const vod = { ...ahriVsZedVods[0], fixture: undefined, videoId: 'A_b-cD12345', champion: 103, enemy: 238, lane: 'mid' };
  const opened = [];
  const h = harness('./MatchupVods.tsx', 'MatchupVods', [], [vod]);
  h.render({ ...props, onGuideSection: (section, watch) => opened.push({ section, watch }) });
  let tree = await h.flush();
  rows(tree)[0].props.onClick(); h.render();
  h.render({ ...props, champion: null, enemy: undefined, lane: undefined, onGuideSection: (section, watch) => opened.push({ section, watch }) });
  tree = await h.flush();
  assert.equal(nodes(tree).find(n => n.type === 'youtube-player').props.vod.videoId, vod.videoId);
  nodes(tree).find(n => n.props.className === 'mv-section-link').props.onClick();
  assert.deepEqual(opened[0].watch, { champion: props.champion, enemy: props.enemy, lane: 'mid' });
  nodes(tree).find(n => n.props.className === 'mv-back').props.onClick();
  tree = h.render();
  assert.equal(labelled(tree, 'Champion').props.value, '0');
});
