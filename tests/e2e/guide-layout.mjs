import assert from 'node:assert/strict';

export async function guideLayoutChecks({ check, navigate, fullApp, page, nav, cdp }) {
  await check('Guide layout keeps credits, unused references, overview and timeline readable', async () => {
    await navigate(`${fullApp}?draft-guide`, 'nav[aria-label="Swapper pages"]');
    await page.evaluate(`(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const sources = ['First author', 'Second author'].map((author, i) => ({ id: 's' + i, author, kind: 'web', rankClaim: 'Guide contributor', url: 'https://example.com/' + i }));
      const tips = [
        { scope: 'champion', text: 'Buy Black Cleaver.', itemIds: [3071, 6692], sourceIds: ['s0'] },
        { scope: 'champion', heading: 'Keep your escape', text: 'Use Charm ({cd:103:E}) carefully.', abilityKeys: [{ champion: 103, key: 'E' }, { champion: 103, key: 'R' }], sourceIds: ['s1'] },
        { scope: 'champion', text: 'Stay safe.', sourceIds: ['s0', 's1'] },
      ];
      const guide = { champion: 103, lane: 'mid', patch: '26.20', reviewed: true, keyTips: tips, itemization: [], sources,
        sections: Object.fromEntries(['overview', 'early-lane', 'mid-lane', 'win-condition'].map(id => [id, { tips: [{ scope: 'champion', text: id === 'overview' ? 'Unique authored overview advice.' : 'Section advice.' }] }])) };
      guide.sections['mid-lane'].timeline = [{ at: 'Level 6', champion: 'Your level-six setup.', enemy: 'Opponent level-six threat.' }];
      window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => cmd === 'get_matchup_guide'
        ? Promise.resolve({ guide, record: null }) : invoke(cmd, args, options);
    })()`);
    await nav('Draft', '.cs-matchup-guide');
    await page.click('.cs-matchup-guide > summary');
    await page.click('.cs-guide-footer button', 'Open full guide');
    await page.visible('.mg-deck');
    await page.wait("!!document.querySelector('.mg-deck .mg-summary .mg-tips > li:first-child .mg-inline-reference')", 'inline item loaded');
    const tips = await page.evaluate(`(() => {
      const items = [...document.querySelectorAll('.mg-deck .mg-summary .mg-tips > li')];
      return items.map(el => ({
        credits: [...el.querySelectorAll('.mg-tip-credits .mg-author-dot')].map(dot => ({ label: dot.getAttribute('aria-label'), parent: dot.parentElement.parentElement.tagName })),
        chips: el.querySelectorAll('.mg-references > *').length,
        abilityChips: [...el.querySelectorAll('.mg-references .fa-ability-trigger')].map(chip => chip.textContent),
        emptyRows: [...el.querySelectorAll('.mg-references')].filter(row => !row.children.length).length,
      }));
    })()`);
    assert.deepEqual(tips.map(t => t.chips), [1, 1, 0], 'only references absent from the text remain as chips');
    assert.ok(tips[1].abilityChips[0].startsWith('R'), 'the unmentioned escape stays available');
    assert.deepEqual(tips.map(t => t.credits), [
      [{ label: 'First author', parent: 'P' }],
      [{ label: 'Second author', parent: 'H3' }],
      [{ label: 'First author', parent: 'P' }, { label: 'Second author', parent: 'P' }],
    ]);
    assert.ok(tips.every(t => !t.emptyRows));
    await page.text('.mg-slide:not([hidden])', 'Unique authored overview advice.');
    for (const width of [1100, 1600]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', { width, height: 1000, deviceScaleFactor: 1, mobile: false });
      await page.settle();
      const layout = await page.evaluate(`(() => {
        const summary = document.querySelector('.mg-deck .mg-summary');
        const item = summary.querySelector('.mg-tips');
        return { columns: getComputedStyle(summary).gridTemplateColumns.split(' ').length,
          width: item.getBoundingClientRect().width / summary.getBoundingClientRect().width,
          overflow: [...document.querySelectorAll('.mg-slide:not([hidden]), .mg-tip-content')].some(el => el.checkVisibility() && el.scrollWidth > el.clientWidth + 1) };
      })()`);
      assert.equal(layout.columns, 1, 'general guide key tips fill a single column');
      assert.ok(layout.width > .95, `${width}px: unused summary column`);
      assert.equal(layout.overflow, false);
    }
    await page.click('.mg-tabs [role="tab"]', 'Mid lane');
    await page.text('.mg-slide:not([hidden]) table', 'Your level-six setup.');
    await page.text('.mg-slide:not([hidden]) table', 'Opponent level-six threat.');
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
  });
}
