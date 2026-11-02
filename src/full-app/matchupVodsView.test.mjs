import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import * as logic from "./matchupVodModel.ts";
import * as guide from "./matchupGuide.ts";
import * as youtube from './youtubePlayer.ts';
import { ahriVsZedVods } from "./fixtures/ahriVsZedVods.ts";

function harness(file, component, vods = ahriVsZedVods) {
  const states = [], effects = [], focus = [], domRefs = new Set(), messages = [], listeners = new Map(), timers = new Map(), fullscreen = [], scrolls = [];
  const contentWindow = { postMessage: (data, origin) => messages.push({ data: JSON.parse(data), origin }) };
  const browser = { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name), setInterval: fn => { timers.set('interval', fn); return 'interval'; }, clearInterval: name => timers.delete(name), setTimeout: fn => { timers.set('timeout', fn); return 'timeout'; }, clearTimeout: name => timers.delete(name) };
  const documentListeners = new Map();
  const fakeDocument = { fullscreenElement: null, addEventListener: (name, fn) => documentListeners.set(name, fn), removeEventListener: name => documentListeners.delete(name), exitFullscreen: async () => { fullscreen.push('element off'); fakeDocument.fullscreenElement = null; } };
  const tauriWindow = { setFullscreen: async on => { fullscreen.push(`window ${on ? 'on' : 'off'}`); } };
  let cursor = 0, pending = [], props;
  const react = {
    useId: () => "test-id", useRef(initial) { const i = cursor++; return states[i] ??= { current: initial }; },
    useState(initial) { const i = cursor++; if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial; return [states[i], value => { states[i] = typeof value === 'function' ? value(states[i]) : value; }]; },
    useEffect(fn, deps) { const i = cursor++; if (!effects[i] || deps.some((dep, j) => dep !== effects[i].deps[j])) pending.push(() => { effects[i]?.cleanup?.(); effects[i] = { deps, cleanup: fn() }; }); },
  };
  const jsx = (type, props) => ({ type, props });
  const modules = {
    react, "react/jsx-runtime": { jsx, jsxs: jsx, Fragment: "fragment" }, "lucide-react": { Play: "play", ChevronRight: "chevron", ArrowLeft: "back-icon" },
    "./matchupVodModel": logic, "./matchupGuide": guide, "./loadMatchupVods": { loadMatchupVods: async () => vods },
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
      const ref = node.props.ref; domRefs.add(ref); ref.current = { contentWindow, querySelector: () => null, focus: () => focus.push(node.props["data-vod-id"] ?? node.props.className), children: [], scrollHeight: 0, clientHeight: 0,
        requestFullscreen: async () => { fullscreen.push('element on'); fakeDocument.fullscreenElement = ref.current; } };
    }
    pending.splice(0).forEach(fn => fn()); return tree;
  };
  return { render, focus, messages, contentWindow, timers, fullscreen, fakeDocument, documentListeners, receive(event) { listeners.get('message')?.(event); }, unmount() { effects.forEach(effect => effect?.cleanup?.()); }, async flush() { for (let i = 0; i < 5; i++) await new Promise(setImmediate); return render(); } };
}
function nodes(tree) { return !tree || typeof tree !== "object" ? [] : Array.isArray(tree) ? tree.flatMap(nodes) : [tree, ...nodes(tree.props?.children)]; }
function text(tree) { return typeof tree === "string" || typeof tree === "number" ? String(tree) : !tree || typeof tree !== "object" ? "" : Array.isArray(tree) ? tree.map(text).join("") : text(tree.props?.children); }
const props = { champion: { id: 103, name: "Ahri" }, enemy: { id: 238, name: "Zed" }, lane: "mid", onGuideSection() {} };

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
  nodes(tree).find(node => text(node) === 'Reopen VOD' && node.type === 'button').props.onClick();
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
  assert.equal(nodes(row).find(node => node.type === 'moment-icon').props.kind, 'dragon');
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
  assert.deepEqual(h.fullscreen.splice(0), ['element on', 'window on']);
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

test("an empty VOD response shows a compact empty message", async () => {
  const h = harness("./MatchupVods.tsx", "MatchupVods", []);
  h.render(props);
  assert.equal(text(await h.flush()), "No VODs for this matchup yet");
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
