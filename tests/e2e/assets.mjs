// Small offline catalog for the rune IDs used by the checked-in previews.
const trees = [
  [8000, "Precision", [[8005, 8008, 8021, 8010], [9101, 9111, 8009], [9104, 9105, 9103], [8014, 8017, 8299]]],
  [8100, "Domination", [[8112, 8124, 9923], [8126, 8139, 8143], [8136, 8120, 8138], [8135, 8134, 8106]]],
  [8200, "Sorcery", [[8214, 8229, 8230], [8224, 8226, 8275], [8210, 8234, 8233], [8237, 8232, 8236]]],
  [8400, "Resolve", [[8437, 8439, 8465], [8446, 8463, 8401], [8429, 8444, 8473], [8451, 8453, 8242]]],
  [8300, "Inspiration", [[8351, 8360, 8369], [8306, 8304, 8313], [8321, 8316, 8345], [8347, 8410, 8352]]],
];
const names = { 8010: "Conqueror", 8437: "Grasp of the Undying", 9111: "Triumph", 9104: "Legend: Alacrity", 9105: "Legend: Haste", 8299: "Last Stand", 8304: "Magical Footwear", 8347: "Cosmic Insight", 8444: "Second Wind", 8242: "Unflinching" };
export const runes = trees.map(([id, name, rows]) => ({ id, name, key: name, icon: `e2e/${id}.svg`,
  slots: rows.map(row => ({ runes: row.map(id => ({ id, name: names[id] ?? `Rune ${id}`, key: `Rune${id}`, icon: `e2e/${id}.svg` })) })) }));

export const placeholder = '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><rect width="80" height="80" fill="#292d2b"/><path d="M18 55 40 18 62 55Z" fill="#c5e39e"/></svg>';
