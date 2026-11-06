import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import * as model from './champ-select-model.ts';

async function render(result) {
  const state = [], refs = [], effects = [];
  let slot = 0, ref = 0;
  const jsx = (type, props) => ({ type, props });
  const modules = {
    'react/jsx-runtime': { jsx, jsxs: jsx },
    react: {
      useState: initial => {
        const id = slot++;
        if (!(id in state)) state[id] = initial;
        return [state[id], value => { state[id] = typeof value === 'function' ? value(state[id]) : value; }];
      },
      useRef: value => refs[ref++] ??= { current: value },
      useEffect: effect => { if (!effects.length) effects.push(effect); },
    },
    '@tauri-apps/api/core': { invoke: async command => { assert.equal(command, 'recommended_picks'); return result; } },
    './champ-select-model': model,
    './ui/format': { fmtCount: String },
    '../runes/types': { fmtPct: value => `${value}%` },
  };
  const code = ts.transpileModule(readFileSync(new URL('./ChampionSelectPicks.tsx', import.meta.url), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  new Function('require', 'exports', code)(key => modules[key] ?? {}, exports);
  const props = { role: 'top', tier: 'all', enemyIds: [], taken: new Set(), names: new Map(result.picks.map(p => [p.championId, `Champion ${p.championId}`])),
    hovered: 0, tierList: async () => { throw new Error('Must not use stats as candidates'); }, canLock: false, pickable: new Set() };
  exports.PickSuggestions(props);
  effects[0]();
  await new Promise(resolve => setImmediate(resolve));
  slot = 0; ref = 0;
  return exports.PickSuggestions(props);
}

function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (typeof tree.type === 'function') return nodes(tree.type(tree.props));
  return [tree, ...nodes(tree.props?.children)];
}
function text(tree) {
  if (typeof tree === 'string' || typeof tree === 'number') return String(tree);
  if (!tree || typeof tree !== 'object') return '';
  return Array.isArray(tree) ? tree.map(text).join('') : text(tree.props?.children);
}

test('all ten curated picks retain accessible reasons and show an older-list note', async () => {
  const picks = Array.from({ length: 10 }, (_, i) => ({ championId: i + 1, winRate: i ? 50 : 0, games: i ? 100 : 0, flex: false, reason: `Safe farming ${i + 1}.` }));
  const tree = await render({ picks, curatedPatch: '26.22', currentPatch: '26.23', status: 'ready' });
  const buttons = nodes(tree).filter(n => n.props.className === 'cs-pick-select');
  assert.equal(buttons.length, 10);
  assert.match(buttons[0].props.title, /Safe farming 1/);
  assert.equal(buttons[0].props['aria-description'], 'Safe farming 1.');
  assert.match(text(tree), /List from patch 26.22/);
  assert.doesNotMatch(text(await render({ picks, curatedPatch: '26.22', currentPatch: '26.22', status: 'ready' })), /List from patch/);
});

test('missing curation displays an explicit empty message', async () => {
  const tree = await render({ picks: [], curatedPatch: null, currentPatch: '26.22', status: 'no_curated_list' });
  assert.match(text(tree), /No curated list/);
  assert.equal(nodes(tree).filter(n => n.props.className === 'cs-pick-select').length, 0);
});
