// Read-only reel discovery. Notification IDs come from Facebook's own matching notification,
// never from unrelated recommendations or a guess at the account's latest three uploads.
(() => {
  function reelUrl(value) {
    try {
      const url = new URL(value);
      const id = /^\/reel\/(\d+)\/?$/.exec(url.pathname)?.[1];
      return url.protocol === 'https:' && /^(www\.|m\.)?facebook\.com$/.test(url.hostname) && id
        ? 'https://www.facebook.com/reel/' + id + '/' : null;
    } catch { return null; }
  }

  function* objects(roots) {
    const pending = [...roots];
    const seen = new WeakSet();
    let visited = 0;
    while (pending.length && ++visited < 300000) {
      const value = pending.pop();
      if (!value || typeof value !== 'object' || seen.has(value)) continue;
      seen.add(value);
      yield value;
      // Runtime props may include accessors or DOM objects; inspect data without invoking getters.
      let properties;
      try { properties = Object.getOwnPropertyDescriptors(value); } catch { continue; }
      for (const property of Object.values(properties)) {
        const child = property.value;
        if (child && typeof child === 'object') pending.push(child);
      }
    }
  }

  function notification(roots, target) {
    if (!reelUrl(target)) return null;
    const url = new URL(target), alert = url.searchParams.get('notif_id');
    if (!alert || !/^\d+$/.test(alert)) return null;
    for (const value of objects(roots)) {
      if (value.notif_type !== 'watch_follower_video' || typeof value.tracking !== 'string') continue;
      let tracking;
      try { tracking = JSON.parse(value.tracking); } catch { continue; }
      if (String(tracking.alert_id) !== alert || String(tracking.content_id) !== url.pathname.split('/')[2]) continue;
      const parts = typeof tracking.dedup_key === 'string' ? tracking.dedup_key.split('|') : [];
      const ids = parts[1]?.split(',') || [];
      if (!ids.length || ids.length > 20 || ids.some(id => !/^\d+$/.test(id)) || !ids.includes(String(tracking.content_id))) continue;
      const owner = String(tracking.context_id || '');
      return {
        urls: [...new Set(ids)].map(id => 'https://www.facebook.com/reel/' + id + '/'),
        account: /^\d+$/.test(owner) ? 'https://www.facebook.com/profile.php?id=' + owner + '&sk=reels_tab' : null,
      };
    }
    return null;
  }

  function stories(roots) {
    const found = new Map();
    for (const value of objects(roots)) {
      if (value.__typename !== 'Story' || !Array.isArray(value.attachments)) continue;
      for (const attachment of value.attachments) {
        const media = attachment.media;
        const url = media && reelUrl(media.shareable_url || media.permalink_url);
        if (!url) continue;
        const row = {url, author: value.actors?.[0]?.name || '', text: value.message?.text || ''};
        if (!found.has(url) || row.text) found.set(url, row);
      }
    }
    return [...found.values()];
  }

  if (typeof module !== 'undefined') module.exports = {notification, stories};
  if (typeof window === 'undefined') return;
  const F = window.__fb;
  const roots = () => [...document.querySelectorAll('script[type="application/json"]')].flatMap(script => {
    try { return [JSON.parse(script.textContent)]; } catch { return []; }
  });
  function notificationRoots(target) {
    const data = roots(), alert = new URL(target).searchParams.get('notif_id');
    for (const row of document.querySelectorAll('[role=listitem]')) {
      const link = row.querySelector('a[href]');
      if (!link || new URL(link.href).searchParams.get('notif_id') !== alert) continue;
      let fiber = row[Object.keys(row).find(key => key.startsWith('__reactFiber$'))];
      // Hydrated/paged notifications may live only in React's current props, not initial JSON.
      for (let depth = 0; fiber && depth < 8; depth++, fiber = fiber.return) {
        if (fiber.memoizedProps) data.push(fiber.memoizedProps);
      }
    }
    return data;
  }
  let mode, key, notice = '', seen = new Set();
  const pause = () => { for (const video of document.querySelectorAll('video')) { video.muted = true; video.pause(); } };
  const label = element => F.clean(element.getAttribute('aria-label') || element.innerText || '');

  function currentCard() {
    const video = [...document.querySelectorAll('video')].find(element => {
      const r = element.getBoundingClientRect();
      return r.height > 0 && r.top >= -10 && r.top < innerHeight / 2;
    });
    let card = video;
    while (card && ![...card.querySelectorAll('[role=button][aria-label]')].some(b => F.visible(b) && /^Comment$/i.test(label(b)))) card = card.parentElement;
    return card && [...card.querySelectorAll('a[href]')].some(a => F.clean(a.innerText || '')) ? card : null;
  }

  async function recommended(count) {
    const rows = [], deadline = Date.now() + 25000;
    for (let turn = 0; turn < count + 8 && Date.now() < deadline && rows.length < count; turn++) {
      const card = await F.waitFor(currentCard, 5000);
      pause();
      if (!card) break;
      const url = reelUrl(location.href);
      if (url && !seen.has(url)) {
        const expand = [...card.querySelectorAll('[role=button],button')].filter(b => F.visible(b) && /^(See more|Xem thêm|Voir plus|Afficher la suite)$/i.test(label(b)));
        for (const button of expand) button.click();
        if (expand.length && !await F.waitFor(() => expand.every(button => !button.isConnected || !F.visible(button) || !/^(See more|Xem thêm|Voir plus|Afficher la suite)$/i.test(label(button))), 5000)) throw Error('Facebook has not expanded the reel caption. Refresh to retry.');
        const author = [...card.querySelectorAll('a[href]')].map(a => F.clean(a.innerText || '')).find(Boolean) || '';
        const walker = document.createTreeWalker(card, NodeFilter.SHOW_TEXT), lines = [];
        let node;
        while ((node = walker.nextNode())) {
          const element = node.parentElement;
          if (element.closest('script,style,button,[role=button],[aria-hidden=true]') || !F.visible(element)) continue;
          const line = F.clean(node.nodeValue || '');
          if (line && line !== author && !/^(·|Follow|Following|Verified account|[\d.,]+[KMB]?)$/i.test(line)) lines.push(line);
        }
        const text = lines.join('\n');
        rows.push({url, author, text});
        seen.add(url);
      }
      if (rows.length >= count) break;
      const before = location.href;
      const next = [...document.querySelectorAll('[role=button][aria-label]')].filter(b => F.visible(b) && /^(Next Card|Next reel)$/i.test(label(b)));
      if (next.length !== 1) break;
      next[0].click();
      if (!await F.waitFor(() => location.href !== before && currentCard(), 6000)) break;
      // Facebook recycles the same card element; wait for its slide and caption hydration.
      await F.sleep(700);
    }
    pause();
    return rows;
  }

  async function account(count) {
    const rows = [];
    for (let turn = 0; turn < 5 && rows.length < count; turn++) {
      const captions = new Map(stories(roots()).map(row => [row.url, row]));
      for (const anchor of document.querySelectorAll('a[href*="/reel/"]')) {
        const url = reelUrl(anchor.href);
        if (!url || seen.has(url)) continue;
        const row = captions.get(url) || {url, author: document.title.replace(/ Reels.*$/, ''), text: ''};
        rows.push(row);
        seen.add(url);
        if (rows.length >= count) break;
      }
      if (rows.length >= count) break;
      scrollBy(0, innerHeight);
      await F.sleep(700);
    }
    pause();
    return rows;
  }

  window.__blindfbReels = {
    notificationNow: target => notification(notificationRoots(target), target),
    notification: async target => JSON.stringify(await F.waitFor(() => notification(notificationRoots(target), target), 5000)),
    initialize(sourceKey, kind, message = '') { key = sourceKey; mode = kind; notice = message; seen = new Set(); },
    async more(sourceKey, count) {
      if (sourceKey !== key) throw Error('This reels list expired. Refresh it to continue.');
      if (mode === 'exact') return JSON.stringify({rows: [], notice: 'All reels from this notification are already listed.'});
      const rows = mode === 'account' ? await account(count) : await recommended(count);
      return JSON.stringify({rows, notice});
    },
  };
})();
