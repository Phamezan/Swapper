import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as logic from "./matchupVodModel.ts";
import * as guide from "./matchupGuide.ts";
import * as youtube from './youtubePlayer.ts';
import { ahriVsZedVods } from "./fixtures/ahriVsZedVods.ts";

function harness(file, component, vods = ahriVsZedVods, championVods = []) {
  const states = [], effects = [], focus = [], domRefs = new Set(), messages = [], listeners = new Map(), timers = new Map(), fullscreen = [], scrolls = [];
  const contentWindow = { postMessage: (data, origin) => messages.push({ data: JSON.parse(data), origin }) };
  const browser = { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name), setInterval: fn => { timers.set('interval', fn); return 'interval'; }, clearInterval: name => timers.delete(name), setTimeout: fn => { timers.set('timeout', fn); return 'timeout'; }, clearTimeout: name => timers.delete(name) };
  const documentListeners = new Map();
  const fakeDocument = { fullscreenElement: null, addEventListener: (name, fn) => documentListeners.set(name, fn), removeEventListener: name => documentListeners.delete(name), exitFullscreen: async () => { fullscreen.push('element off'); fakeDocument.fullscreenElement = null; } };
  const tauriWindow = { maximized: false, isMaximized: async () => tauriWindow.maximized, toggleMaximize: async () => { tauriWindow.maximized = !tauriWindow.maximized; fullscreen.push(tauriWindow.maximized ? 'maximize' : 'unmaximize'); },
    setFullscreen: async on => { fullscreen.push(`window ${on ? 'on' : 'off'}`); } };
  let cursor = 0, pending = [], props;
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
    "./matchupVodModel": logic, "./matchupGuide": guide, "./loadMatchupVods": { loadMatchupVods: async () => vods, loadChampionVods: async () => championVods },
    "./Picker": { Picker: "picker", ROLES: [["mid", "Mid"], ["top", "Top"]] }, "./NearViewport": { NearEnd: "near-end" },
    './youtubePlayer': youtube, './YoutubeVodPlayer': { YoutubeVodPlayer: 'youtube-player' },
    "./art": { useChampionArt: () => null }, "./GuideSourceButton": { GuideSourceButton: "source-button" },
    "./VodPlayer": { VodPlayer: "player-identity" }, "../runes/TeamLogo": { TeamLogo: "team-logo" }, "../runes/RankCrest": { RankCrest: "rank-crest" },
    "./ui/States": { Skeleton: "skeleton" }, "./MomentIcon": { MomentIcon: "moment-icon" }, "@tauri-apps/api/window": { getCurrentWindow: () => tauriWindow }, "./MatchupGuideContent": { GuideSectionContent: "guide-content" },
    "./MatchupGuideSummary": { MatchupGuideSummary: "summary" }, "./TradeWindows": { TradeWindows: "trades" }, "./PowerSpikeTimeline": { PowerSpikeTimeline: "spikes" },
  };
  const source = readFileSync(new URL(file, import.meta.url), "utf8");
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  new Function("require", "exports", "window", 'location', 'document', code)(name => modules[name] ?? {}, exports, browser, { origin: 'http://tauri.localhost' }, fakeDocument);
  const render = value => {
    props = value ?? props; cursor = 0; const tree = exports[component](props);
    for (const ref of domRefs) ref.current = null;
    for (const node of nodes(tree)) if (node.props.ref && typeof node.props.ref === "object") {
      const ref = node.props.ref; domRefs.add(ref); ref.current = { contentWindow, querySelector: () => null, contains: () => false, focus: () => focus.push(node.props["data-vod-id"] ?? node.props.className), children: [], scrollHeight: 0, clientHeight: 0,
        requestFullscreen: async () => { fullscreen.push('element on'); fakeDocument.fullscreenElement = ref.current; } };
    }
    pending.splice(0).forEach(fn => fn()); return tree;
  };
  return { render, tauriWindow, focus, messages, contentWindow, timers, fullscreen, fakeDocument, documentListeners, receive(event) { listeners.get('message')?.(event); }, unmount() { effects.forEach(effect => effect?.cleanup?.()); }, async flush() { for (let i = 0; i < 5; i++) await new Promise(setImmediate); return render(); } };
}
function nodes(tree) { return !tree || typeof tree !== "object" ? [] : Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.children)]; }
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
  assert.equal(nodes(avatar).find(node => node.type === 'rank-crest').props.size, 40);
  assert.match(text(identity), /replay/);
});

test('question-mark players use the unlisted identity and no player photo', () => {
  const tree = harness('./VodPlayer.tsx', 'VodPlayer').render({ vod: { ...ahriVsZedVods[0], player: ' ? ', team: undefined, playerImage: '/unknown.png' } });
  assert.doesNotMatch(text(tree), /\?/);
  assert.match(text(tree), /player unlisted/);
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
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-title")), ahriVsZedVods[2].title);
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

test('player receives trusted progress, sends controls and cleans up its listeners', () => {
  const vod = { ...ahriVsZedVods[0], fixture: undefined, videoId: 'A_b-cD12345' };
  const h = harness('./YoutubeVodPlayer.tsx', 'YoutubeVodPlayer');
  let tree = h.render({ vod, seek: null });
  const iframe = nodes(tree).find(node => node.type === 'iframe');
  assert.match(iframe.props.src, /controls=0.*origin=http%3A%2F%2Ftauri.localhost/);
  iframe.props.onLoad();
  assert.deepEqual(h.messages.pop(), { data: { event: 'listening', id: 1 }, origin: youtube.YOUTUBE_ORIGIN });
  h.receive({ source: h.contentWindow, origin: 'https://evil.io', data: JSON.stringify({ event: 'infoDelivery', info: { currentTime: 99 } }) });
  assert.ok(timeline(h.render()).props.disabled);
  h.timers.get('timeout')();
  assert.match(text(h.render()), /Video unavailable or slow to load/);
  h.receive({ source: h.contentWindow, origin: youtube.YOUTUBE_ORIGIN, data: JSON.stringify({ event: 'infoDelivery', info: { currentTime: 184, duration: 1746, playerState: 1, playbackRate: 1.5, muted: true } }) });
  tree = h.render();
  assert.doesNotMatch(text(tree), /Video unavailable/);
  assert.match(text(tree), /3:04 \/ 29:06/);
  assert.equal(timeline(tree).props.disabled, false);
  control(tree, 'Pause').props.onClick();
  assert.equal(h.messages.pop().data.func, 'pauseVideo');
  control(tree, 'Unmute').props.onClick();
  assert.equal(h.messages.pop().data.func, 'unMute');
  tree = h.render();
  assert.ok(control(tree, 'Mute'), 'mute state updates without waiting for YouTube');
  nodes(tree).find(node => node.props['aria-label'] === 'Volume').props.onChange({ target: { value: '30' } });
  assert.deepEqual(h.messages.pop().data, { event: 'command', func: 'setVolume', args: [30] });
  nodes(tree).find(node => node.props.className === 'mv-speed-button').props.onClick();
  tree = h.render();
  const rates = nodes(tree).filter(node => node.props.role === 'menuitemradio');
  assert.deepEqual(rates.filter(node => node.props['aria-checked']).map(text), ['1.5×']);
  rates.find(node => text(node) === '2×').props.onClick();
  assert.deepEqual(h.messages.pop().data, { event: 'command', func: 'setPlaybackRate', args: [2] });
  assert.equal(nodes(h.render()).some(node => node.props.role === 'menu'), false);
  timeline(tree).props.onSeek(300, true);
  assert.deepEqual(h.messages.pop().data.args, [300, true]);
  assert.match(text(h.render()), /5:00 \/ 29:06/, 'seeks show immediately');
  timeline(tree).props.onJump(184);
  assert.equal(h.messages.pop().data.func, 'playVideo');
  assert.deepEqual(h.messages.pop().data.args, [184, true]);
  h.render({ vod, seek: { moment: vod.moments[1], serial: 1 } });
  assert.equal(h.messages.pop().data.func, 'playVideo');
  assert.deepEqual(h.messages.pop().data.args, [vod.moments[1].atSec, true]);
  h.unmount();
  assert.equal(h.timers.size, 0);
});

test("player identity uses desktop team logos for pros and a rank crest for solo players", () => {
  const h = harness("./VodPlayer.tsx", "VodPlayer");
  let tree = h.render({ vod: { ...ahriVsZedVods[0], team: "T1" } });
  const team = nodes(tree).find(node => node.type === "team-logo");
  assert.equal(team.props.team, "T1");
  assert.equal(team.props.mode, "desktop");
  assert.equal(nodes(tree).some(node => node.type === "rank-crest"), false);
  tree = h.render({ vod: { ...ahriVsZedVods[0], team: undefined, rankClaim: "Grandmaster" } });
  const crest = nodes(tree).find(node => node.type === "rank-crest");
  assert.equal(crest.props.tier, "grandmaster");
  assert.equal(crest.props.mode, "desktop");
});

test("missing and failed player photos show initials rather than a broken image", () => {
  const h = harness("./VodPlayer.tsx", "VodPlayer");
  const vod = { ...ahriVsZedVods[0], player: "Fixture Pro 1" };
  const avatar = tree => nodes(tree).find(node => node.props.className === "mv-avatar");
  assert.equal(text(avatar(h.render({ vod }))), "FP");
  const withPhoto = { ...vod, playerImage: "https://example.invalid/player.png" };
  const image = nodes(h.render({ vod: withPhoto })).find(node => node.type === "img");
  image.props.onError();
  const failed = h.render();
  assert.equal(nodes(failed).some(node => node.type === "img"), false);
  assert.equal(text(avatar(failed)), "FP");
});

test("an empty matchup offers the champion's whole library, which can be empty too", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods", [], []);
  h.render(props);
  let tree = await h.flush();
  assert.match(text(tree), /No VODs for this matchup yet/);
  assert.equal(nodes(tree).some(node => node.props.className === "mv-filters"), false, "no filters over nothing");
  nodes(tree).find(node => node.props.className === "mv-text-button").props.onClick();
  h.render();
  tree = await h.flush();
  assert.match(text(tree), /No Ahri VODs yet/);
});

const rows = tree => nodes(tree).filter(node => node.props.className === "mv-vod-row");
const labelled = (tree, label) => nodes(tree).find(node => node.props?.label === label && node.props.onChange);

test("the library pages a large matchup list and filters every Ahri VOD across opponents", async () => {
  const { ahriChampionVods: every } = await import("./fixtures/ahriChampionVods.ts");
  const zed = every.filter(vod => vod.enemy === 238 && vod.lane === "mid");
  const h = harness("./MatchupVods.tsx", "MatchupVods", zed, every);
  h.render(props);
  let tree = await h.flush();
  assert.equal(rows(tree).length, 60, "a long list renders its first step only");
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-match-count")), `${zed.length} VODs`);
  assert.equal(labelled(tree, "Opponent"), undefined, "one matchup needs no opponent filter");
  const patches = rows(tree).map(row => zed.find(vod => vod.videoId === row.props["data-vod-id"]).patch);
  assert.ok(patches.every((patch, i) => !i || logic.comparePatch(patches[i - 1], patch) >= 0), "newest patch first by default");
  nodes(tree).find(node => node.type === "near-end").props.onNear();
  assert.equal(rows(h.render()).length, 120, "scrolling near the end renders the next step");

  labelled(tree, "VOD scope").props.onChange("champion");
  h.render();
  tree = await h.flush();
  assert.match(text(tree), /Every Ahri VOD/);
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-match-count")), `${every.length} VODs`);
  assert.equal(rows(tree).length, 60, "switching scope starts from the first step again");
  const opponents = labelled(tree, "Opponent").props.options;
  assert.equal(opponents[1].label, `Zed · ${zed.length + every.filter(vod => vod.enemy === 238 && vod.lane !== "mid").length}`, "opponents are ordered by games, with portraits");
  assert.equal(opponents[1].icon.props.id, 238);

  labelled(tree, "Opponent").props.onChange("157");
  labelled(tree, "Players").props.onChange(true);
  labelled(tree, "Result").props.onChange("W");
  labelled(tree, "Rank").props.onChange("grandmaster");
  labelled(tree, "Sort").props.onChange("longest");
  tree = h.render();
  const filters = { ...logic.ANY_VOD, enemy: 157, pros: true, result: "W", tier: "grandmaster", sort: "longest" };
  const expected = logic.filterVods(every, filters);
  assert.ok(expected.length > 1);
  assert.deepEqual(rows(tree).map(row => row.props["data-vod-id"]), expected.slice(0, 60).map(vod => vod.videoId));
  assert.equal(text(nodes(tree).find(node => node.props.className === "mv-match-count")), `${expected.length} of ${every.length}`);
  assert.deepEqual(labelled(tree, "Players").props.options.map(option => option[2]),
    [false, true].map(pros => every.filter(vod => logic.vodMatches(vod, { ...filters, pros })).length), "each option counts what choosing it would show");
  assert.ok(rows(tree).every(row => /vs Yasuo/.test(row.props["aria-label"])));

  labelled(tree, "Patch").props.onChange("1.1");
  tree = h.render();
  assert.match(text(tree), /No VODs match these filters/);
  nodes(tree).find(node => node.props.className === "mv-text-button").props.onClick();
  tree = h.render();
  assert.equal(rows(tree).length, 60);
  assert.equal(labelled(tree, "Sort").props.value, "longest", "clearing filters keeps the chosen order");
  assert.equal(labelled(tree, "Opponent").props.value, "0");
});

test("a VOD from another matchup keeps its moments but cannot open this matchup's guide", async () => {
  const other = { ...ahriVsZedVods[0], enemy: 157, lane: "mid" }, same = { ...ahriVsZedVods[1], enemy: 238, lane: "mid" };
  const h = harness("./MatchupVods.tsx", "MatchupVods", [], [other, same]);
  h.render(props);
  let tree = await h.flush();
  labelled(tree, "VOD scope").props.onChange("champion");
  h.render();
  tree = await h.flush();
  const open = id => { rows(tree).find(row => row.props["data-vod-id"] === id).props.onClick(); return h.render(); };
  let player = open(other.videoId);
  assert.ok(nodes(player).some(node => node.props.className === "mv-moment"));
  assert.equal(nodes(player).some(node => node.props.className === "mv-section-link"), false);
  nodes(player).find(node => node.props.className === "mv-back").props.onClick();
  tree = h.render();
  player = open(same.videoId);
  assert.ok(nodes(player).some(node => node.props.className === "mv-section-link"));
});

test("fullscreen playback hides everything but the timeline when idle and wakes on activity", async () => {
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
  assert.ok(timeline(tree), "the timeline stays");
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
