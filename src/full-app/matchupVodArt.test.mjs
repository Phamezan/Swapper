import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { championsInMoment, momentArt, vodPlayerName } from './matchupVodModel.ts';

test('missing, blank and question-mark player names are unlisted, real names are trimmed', () => {
  for (const player of [undefined, '', '   ', '?', ' ? ', '???']) assert.equal(vodPlayerName(player), null);
  assert.equal(vodPlayerName(' Faker '), 'Faker');
  assert.equal(vodPlayerName('Who?'), 'Who?');
});

const roster = ['Ahri', 'Zed', "Kai\'Sa", 'Thresh', 'Miss Fortune', 'Nunu & Willump', "Cho'Gath", 'Vi', 'Lee Sin'].map((name, id) => ({ id, name }));
const names = label => championsInMoment(label, roster).map(champion => champion.name);

test('moment portraits match full display names, in label order and only once', () => {
  assert.deepEqual(names('Ahri takes down Zed, then Ahri recalls'), ['Ahri', 'Zed']);
  assert.deepEqual(names("Kai’Sa picks off Thresh"), ["Kai'Sa", 'Thresh']);
  assert.deepEqual(names("Miss Fortune and Nunu & Willump fight Cho'Gath"), ['Miss Fortune', 'Nunu & Willump', "Cho'Gath"]);
  assert.deepEqual(names("Lee Sin’s gank catches AHRI."), ['Lee Sin', 'Ahri']);
  assert.deepEqual(championsInMoment('Nunu & Willump', [{ id: 1, name: 'Nunu' }, { id: 2, name: 'Nunu & Willump' }]).map(c => c.id), [2]);
});

test('moment portraits never guess partial words or abbreviated names', () => {
  for (const label of ['vision denied', 'Zedlike movement', 'Ahri2', 'éAhri', 'Miss wins', 'Kai wins', 'Lee arrives', 'Nunu ganks', 'Cho Gath', 'Kaisa', '_Vi_']) {
    assert.deepEqual(names(label), [], label);
  }
  assert.deepEqual(championsInMoment('Ahri', []), []);
  assert.deepEqual(championsInMoment('A+B arrives', [{ id: 1, name: 'A+B' }]), [{ id: 1, name: 'A+B' }]);
});

test('moment kinds reuse League objective assets or bundled ping art, with a neutral fallback', () => {
  const expected = { kill: 'kills', trade: 'kills', dragon: 'dragon-100', baron: 'baron-100', herald: 'herald-100', grubs: 'grub', tower: 'tower-100', death: '/vod-icons/dead_red.png', teamfight: '/vod-icons/teamfight.png', roam: '/vod-icons/roam.png' };
  for (const [kind, art] of Object.entries(expected)) {
    assert.equal(momentArt(kind), art);
    if (art.startsWith('/')) assert.equal(readFileSync(new URL(`../../public${art}`, import.meta.url)).subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  }
  for (const kind of [undefined, '', 'other', 'unknown', 'constructor', '__proto__']) assert.equal(momentArt(kind), null);
});
