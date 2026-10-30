(() => {
  const F = window.__fb;
  const ARTICLE = '[role=article][aria-label]';
  const EDITOR = '[role=textbox][contenteditable=true],textarea';
  const own = (element, article) => element.closest(ARTICLE) === article;
  const label = element => F.clean(element.getAttribute('aria-label') || element.getAttribute('placeholder') || '');
  const contents = element => F.clean(element.value === undefined ? element.innerText || '' : element.value);
  const action = /^(Like|Reply|Edited|See translation|Thích|Trả lời|J’aime|Répondre)$/i;
  const time = F.TIME || /^(\d+\s*(s|m|h|d|w|y|min|hr|day|week)s?|Just now)$/i;
  let root, address, target, serial = 0, records = new Map(), pending;
  // A reel has no post dialog: its card holds the caption and actions, a side panel the comments.
  // Opening its comments hides the card's author and caption, so they are read before that.
  let card = null, reel = null;
  const identities = new WeakMap();

  function current() {
    if (!root || !root.isConnected || location.href !== address || (card && !card.isConnected)) throw Error('Post changed or reader expired. Refresh or reopen the post.');
    return root;
  }

  const buttonLabeled = (scope, pattern) => [...scope.querySelectorAll('[role=button][aria-label]')].filter(element => F.visible(element) && pattern.test(label(element)));

  // The reel playing at the top of the viewer, as the card that owns its Comment button.
  function reelCard() {
    const video = [...document.querySelectorAll('video')].find(element => {
      const rect = element.getBoundingClientRect();
      return rect.height > 0 && rect.top >= -10 && rect.top < innerHeight / 2;
    });
    let element = video;
    while (element && !buttonLabeled(element, /^Comment$/).length) element = element.parentElement;
    // Until it shows its author the card is still loading, or is the next reel's.
    return element && [...element.querySelectorAll('a[href]')].some(a => F.clean(a.innerText || '')) ? element : null;
  }

  // Where the post's media lives: a video link inside it, else the post itself when it plays one.
  function videoTarget(scope) {
    if (card) return target;
    const link = [...scope.querySelectorAll('a[href]')].filter(a => !a.closest(ARTICLE)).map(a => a.href).find(href => /facebook\.com\/(?:[^/?#]+\/videos\/|videos\/|reel\/|watch\/?\?v=)/.test(href));
    if (link) return F.canon(link) ? 'https://www.facebook.com' + F.canon(link) : link;
    return scope.querySelector('video') ? target : null;
  }

  // Links a reader would follow: anything in the message, and pages outside Facebook.
  function links(scope, message) {
    const found = new Map();
    for (const a of scope.querySelectorAll('a[href]')) {
      if (a.closest(ARTICLE)) continue;
      let url;
      try { url = new URL(a.href, location.href); } catch { continue; }
      if (/^l\.facebook\.com$/.test(url.hostname) && url.searchParams.get('u')) {
        try { url = new URL(url.searchParams.get('u')); } catch { continue; }
      }
      if (!/^https?:$/.test(url.protocol) || /\/hashtag\//.test(url.pathname)) continue;
      const external = !/(^|\.)facebook\.com$/.test(url.hostname);
      const inMessage = (Array.isArray(message) ? message : [message]).some(element => element && element.contains(a));
      if (!external && !inMessage) continue;
      for (const key of [...url.searchParams.keys()]) if (/^(__cft__|__tn__|fbclid)/.test(key)) url.searchParams.delete(key);
      const text = F.clean(a.innerText || '') || label(a) || url.hostname;
      if (!found.has(url.href)) found.set(url.href, {label: text, url: url.href});
    }
    return [...found.values()];
  }

  async function sharedLinks(scope) {
    const found = new Map();
    for (const profile of scope.querySelectorAll('[data-ad-rendering-role=profile_name]')) {
      if (!F.visible(profile) || profile.closest(ARTICLE)) continue;
      const name = F.clean(profile.innerText || '').replace(/\s+/g, ' ');
      const header = profile.parentElement?.parentElement?.parentElement;
      if (!name || !header) continue;
      for (const a of header.querySelectorAll('a[href]')) {
        if (!F.visible(a) || a.closest(ARTICLE)) continue;
        // Facebook reveals the original post's timestamp URL only after focus.
        if (/^\?/.test(a.getAttribute('href') || '')) {
          a.focus({preventScroll: true});
          await F.waitFor(() => !/^\?/.test(a.getAttribute('href') || ''), 400, 30);
          a.blur();
        }
        let url;
        try { url = new URL(a.href, location.href); } catch { continue; }
        if (!/^https?:$/.test(url.protocol) || !/^(www\.|m\.|web\.)?facebook\.com$/.test(url.hostname) || url.searchParams.has('comment_id') || url.searchParams.has('reply_comment_id')) continue;
        const path = F.canon(url.href);
        if (!/(?:\/posts\/|\/permalink\/|^\/(?:permalink|story)\.php\?|\/reel\/|\/videos\/|^\/watch\/?\?v=)/.test(path) || path === F.canon(target)) continue;
        const address = 'https://www.facebook.com' + path;
        if (!found.has(address)) found.set(address, {label: 'Shared post by ' + name, url: address});
      }
    }
    return [...found.values()];
  }

  // Profile links carry base64 "comment:<post>_<id>"; reduce it to the numeric comment id.
  function plain(value) {
    if (/^\d+$/.test(value)) return value;
    try { return /^comment:\d+_(\d+)$/.exec(atob(value))?.[1] || value; } catch { return value; }
  }

  function identity(article) {
    const links = [...article.querySelectorAll('a[href]')].filter(e => own(e, article));
    const found = links.map(a => {
      try {
        const u = new URL(a.href, location.href);
        if (!/^(www\.|m\.)?facebook\.com$/.test(u.hostname)) return null;
        const raw = u.searchParams.get('comment_id');
        const parent = raw && plain(raw);
        const reply = u.searchParams.get('reply_comment_id');
        // Numeric ids come from the timestamp permalink, which also names the parent of a reply.
        return parent ? {id: reply || parent, parent: reply ? parent : null, url: u.href, exact: /^\d+$/.test(raw)} : null;
      } catch { return null; }
    }).filter(Boolean);
    const unique = [...new Set(found.map(value => value.id))];
    if (unique.length > 1) throw Error('Ambiguous comment permalink; refresh the post.');
    if (found.length) {
      const {id, parent, url} = found.find(value => value.exact) || found[0];
      return {id, parent, url};
    }
    if (!identities.has(article)) identities.set(article, 'dom-' + (++serial));
    return {id: identities.get(article), parent: null, url: null};
  }

  function comment(article) {
    const id = identity(article);
    // Read only this article's text, excluding nested replies and editable drafts.
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT);
    const lines = [];
    let node;
    while ((node = walker.nextNode())) {
      const element = node.parentElement;
      if (!own(element, article) || element.closest(EDITOR + ',script,style') || !F.visible(element)) continue;
      const text = F.clean(node.nodeValue || '');
      if (text) lines.push(text);
    }
    const author = lines[0] || article.getAttribute('aria-label') || '';
    const authorRef = F.profilePath ? [...article.querySelectorAll('a[href]')].filter(a => a.closest(ARTICLE) === article).map(a => F.profilePath(a.href !== undefined ? a.href : a.getAttribute('href'))).find(Boolean) || null : null;
    const ti = lines.findIndex((text, index) => index < 7 && time.test(text));
    const body = lines.slice(1).filter((text, index) => index + 1 !== ti && !action.test(text) && !/^((by )?author|Top fan|Follow|·)$/i.test(text));
    // A bare number after the text is the reaction count.
    if (body.length > 1 && /^\d+$/.test(body[body.length - 1])) body.pop();
    const ancestor = article.parentElement.closest(ARTICLE);
    const parent = ancestor && root.contains(ancestor) ? identity(ancestor).id : id.parent;
    return {id: id.id, author, author_ref: authorRef, time: ti >= 0 ? lines[ti] : '', text: body.join('\n'), parent_id: parent, url: id.url};
  }

  const fingerprint = value => JSON.stringify([value.id, value.author, value.author_ref, value.text, value.parent_id, value.url]);
  function collect() {
    current();
    const result = [...root.querySelectorAll(ARTICLE)].filter(F.visible).map(element => ({element, value: comment(element)}));
    const ids = new Set();
    for (const {value} of result) {
      if (ids.has(value.id)) throw Error('Duplicate comment identity; refresh the post.');
      ids.add(value.id);
    }
    return result;
  }

  async function read() {
    current();
    const watch = /\/videos\/|^\/watch\/?/.test(new URL(target).pathname);
    const expand = [...root.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && /^(See more|Xem thêm|Voir plus|Afficher la suite)$/i.test(F.clean(element.innerText || '') || label(element)) && (watch || element.closest(ARTICLE + ',[data-ad-rendering-role=story_message]')));
    for (const button of expand) {
      if (button.isConnected) button.click();
    }
    if (expand.length && !await F.waitFor(() => expand.every(button => !button.isConnected || !F.visible(button) || !/^(See more|Xem thêm|Voir plus|Afficher la suite)$/i.test(F.clean(button.innerText || '') || label(button))), 5000)) throw Error('Facebook has not expanded all text. Refresh the reader to retry.');
    const rows = collect();
    records = new Map(rows.map(row => [row.value.id, {...row, key: fingerprint(row.value)}]));
    if (card) {
      return JSON.stringify({url: target, ...reel, time: '', comments: rows.map(row => row.value), video: target});
    }
    const profile = root.querySelector('[data-ad-rendering-role=profile_name]');
    const head = profile ? F.vtext(profile.parentElement.parentElement.parentElement) : [];
    const messages = [...root.querySelectorAll('[data-ad-rendering-role=story_message]')].filter(element => F.visible(element) && !element.closest(ARTICLE));
    const watchCaption = !messages.length && watch;
    // Watch captions lack the story_message marker used by ordinary posts.
    if (watchCaption) messages.push(...[...root.querySelectorAll('[dir=auto][style*="text-align"]')].filter(element => F.visible(element) && !element.closest(ARTICLE + ',' + EDITOR)));
    // A shared post can have both the poster's message and the original item's message.
    // A main page must still identify one card; an individual post dialog already scopes it.
    if (messages.length > 1 && !watchCaption && root.getAttribute('role') !== 'dialog') {
      const cards = F.postCards().filter(element => root.contains(element));
      const owners = cards.filter(element => !cards.some(other => other !== element && other.contains(element)));
      if (owners.length !== 1) throw Error('More than one post is visible. Open the individual post permalink.');
    }
    // Group posts name the group, then the poster; comments carry their own names.
    const names = [...root.querySelectorAll('[data-ad-rendering-role=profile_name]')].filter(e => !e.closest(ARTICLE)).map(e => F.clean(e.innerText || '').replace(/\s+/g, ' ')).filter(Boolean);
    const author = names.length ? [...new Set(names)].join(' / ') : profile ? F.clean(profile.innerText || '').replace(/\s+/g, ' ') : '';
    const outerMessages = messages.filter(element => !messages.some(other => other !== element && other.contains(element)));
    const text = [...new Set(outerMessages.map(element => F.postText(element)).filter(Boolean))].join('\n');
    return JSON.stringify({url: target, author, time: head.find(text => time.test(text)) || '', text, comments: rows.map(row => row.value), links: [...await sharedLinks(root), ...links(root, messages)], video: videoTarget(root)});
  }

  function selected(id) {
    current();
    const row = records.get(id);
    if (!row || !row.element.isConnected || !root.contains(row.element) || fingerprint(comment(row.element)) !== row.key) throw Error('Selected comment changed or disappeared. Refresh and select it again.');
    const matches = collect().filter(other => other.value.id === id);
    if (matches.length !== 1 || matches[0].element !== row.element) throw Error('Selected comment is ambiguous or stale.');
    return row.element;
  }

  function editors(parent) {
    current();
    const article = parent === null ? null : selected(parent);
    return [...(article || root).querySelectorAll(EDITOR)].filter(element => F.visible(element) && F.enabled(element) && element.closest(ARTICLE) === article);
  }

  async function prepare(parent, text) {
    current();
    pending = null;
    if (parent !== null) {
      const article = selected(parent);
      const replies = [...article.querySelectorAll('[role=button],button,a')].filter(element => own(element, article) && F.visible(element) && /^(Reply|Trả lời|Répondre)$/i.test(label(element) || F.clean(element.innerText || '')));
      if (replies.length !== 1) throw Error('The selected comment has no unique Reply action. Nothing was sent.');
      replies[0].click();
    }
    const boxes = await F.waitFor(() => { const boxes = editors(parent); return boxes.length === 1 ? boxes : null; }, 5000);
    if (!boxes) throw Error('No unique editor inside the selected comment thread. Nothing was sent.');
    const editor = boxes[0];
    if (contents(editor)) throw Error('The Facebook editor already contains a draft. Nothing was overwritten.');
    editor.scrollIntoView({block: 'center'});
    const name = label(editor);
    if (!name || name.length > 160) throw Error('The comment editor has no reliable accessible name. Nothing was sent.');
    const snapshot = F.pageSnapshot(name);
    const refs = snapshot.split('\n').map(line => line.split('\t')).filter(row => row[1] === 'textbox');
    if (refs.length !== 1) throw Error('The comment editor is ambiguous. Nothing was sent.');
    F.pageTarget(refs[0][0], 'fill', false);
    if (document.activeElement !== editor) throw Error('The selected editor changed. Nothing was sent.');
    pending = {parent, text: F.clean(text), editor, before: new Set(collect().map(row => row.value.id))};
    return JSON.stringify(refs[0][0]);
  }

  function check() {
    current();
    if (!pending || !pending.editor.isConnected || document.activeElement !== pending.editor || contents(pending.editor) !== pending.text || editors(pending.parent).length !== 1 || editors(pending.parent)[0] !== pending.editor) throw Error('The selected comment editor changed. Nothing was sent.');
    return true;
  }

  async function confirm() {
    const sent = pending;
    pending = null;
    if (!sent) throw Error('There is no pending submission.');
    const confirmed = await F.waitFor(() => {
      current();
      if (!sent.editor.isConnected || contents(sent.editor)) return false;
      return collect().some(({value}) => value.url && !sent.before.has(value.id) && value.parent_id === sent.parent && F.clean(value.text) === sent.text);
    }, 12000);
    if (!confirmed) throw Error('Submission was not confirmed in the selected thread. Refresh before trying again; do not resend automatically.');
    return read();
  }

  // Like or unlike one exact comment; confirmed by its Like button changing state.
  async function like(id) {
    const article = selected(id);
    const buttons = [...article.querySelectorAll('[role=button],button')].filter(element => own(element, article) && F.visible(element) && /^(Like|Remove Like|Unlike)$/i.test(label(element) || F.clean(element.innerText || '')));
    if (buttons.length !== 1) throw Error('The selected comment has no unique Like button. Nothing was changed.');
    const button = buttons[0];
    const state = () => [label(button), button.getAttribute('aria-pressed'), F.clean(button.innerText || ''), button.className].join('|');
    const before = state();
    button.click();
    if (!await F.waitFor(() => !button.isConnected || state() !== before, 8000)) throw Error('Facebook did not confirm the Like. Refresh to check.');
    return read();
  }

  let sharing = null;
  const shareNowButton = dialog => [...dialog.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && /^Share now$/i.test(label(element) || F.clean(element.innerText || '')));

  // Open Facebook's own share dialog for this post and report its audience; nothing is shared yet.
  async function shareOpen() {
    current();
    const buttons = [...(card || root).querySelectorAll('[role=button],button')].filter(element => !element.closest(ARTICLE) && F.visible(element) && /^Share\b/i.test(label(element)));
    if (buttons.length !== 1) throw Error('This post has no unique Share button. Nothing was shared.');
    buttons[0].click();
    const dialog = await F.waitFor(() => {
      const found = [...document.querySelectorAll('[role=dialog]')].filter(element => element !== root && !element.contains(root) && F.visible(element) && shareNowButton(element).length === 1);
      return found.length === 1 ? found[0] : null;
    }, 8000);
    if (!dialog) throw Error('Facebook did not open its share dialog. Nothing was shared.');
    const privacy = [...dialog.querySelectorAll('[role=button],button')].map(label).find(text => /^Edit privacy/i.test(text)) || '';
    sharing = {dialog, text: ''};
    return JSON.stringify({audience: (privacy.match(/Sharing with (.+?)\.?$/i) || [])[1] || 'Facebook default'});
  }

  function shareDialog() {
    current();
    if (!sharing || !sharing.dialog.isConnected || !F.visible(sharing.dialog)) throw Error('The share dialog closed. Nothing was shared.');
    return sharing.dialog;
  }

  // Focus the share dialog's text box and return its backend reference for `page fill`.
  function shareFill(text) {
    const dialog = shareDialog();
    const boxes = [...dialog.querySelectorAll(EDITOR)].filter(element => F.visible(element) && F.enabled(element));
    if (boxes.length !== 1) throw Error('The share dialog has no unique text box. Nothing was shared.');
    const editor = boxes[0];
    if (contents(editor)) throw Error('The share text box already contains text. Nothing was shared.');
    const name = label(editor) || F.clean(editor.getAttribute('aria-placeholder') || '');
    const refs = F.pageSnapshot(name).split('\n').map(line => line.split('\t')).filter(row => row[1] === 'textbox');
    if (!name || refs.length !== 1) throw Error('The share text box is ambiguous. Nothing was shared.');
    F.pageTarget(refs[0][0], 'fill', false);
    sharing.editor = editor;
    sharing.text = F.clean(text);
    return JSON.stringify(refs[0][0]);
  }

  // Press Share now once and wait for Facebook to close its dialog; never retried.
  async function shareNow() {
    const dialog = shareDialog();
    if (sharing.editor && contents(sharing.editor) !== sharing.text) throw Error('The share text did not match. Nothing was shared.');
    const buttons = shareNowButton(dialog);
    if (buttons.length !== 1) throw Error('The Share now button is missing. Nothing was shared.');
    sharing = null;
    buttons[0].click();
    if (!await F.waitFor(() => !dialog.isConnected || !F.visible(dialog), 15000)) throw Error('Facebook did not confirm the share. Check your profile before trying again; do not share again automatically.');
    return 'shared';
  }

  function shareClose() {
    const dialog = sharing && sharing.dialog;
    sharing = null;
    const close = dialog && dialog.isConnected && [...dialog.querySelectorAll('[role=button],button')].find(element => /^Close$/i.test(label(element)));
    if (close) close.click();
    return 'closed';
  }

  // React to a reel from its card (the backend's react command only knows post layouts).
  // The Like button reads "Like" with no reaction, "Remove <reaction>" otherwise.
  async function reelReact(kind) {
    current();
    if (!['like', 'love', 'care', 'haha', 'wow', 'sad', 'angry', 'none'].includes(kind)) throw Error('Unknown reaction. Nothing was changed.');
    const scope = card || root;
    const like = () => {
      const found = buttonLabeled(scope, /^(Like|Remove \w+)$/i).filter(element => !element.closest(ARTICLE));
      return found.length === 1 ? found[0] : null;
    };
    const button = like();
    if (!button) throw Error('This reel has no Like button. Nothing was changed.');
    const before = label(button);
    const want = kind === 'none' ? 'Like' : 'Remove ' + kind[0].toUpperCase() + kind.slice(1);
    if (before.toLowerCase() === want.toLowerCase()) return read();
    if (kind === 'none') button.click();
    else {
      const open = buttonLabeled(scope, /^React$/).filter(element => !element.closest(ARTICLE));
      if (open.length !== 1) throw Error('This reel has no unique React button. Nothing was changed.');
      open[0].click();
      const choice = await F.waitFor(() => {
        const dialog = [...document.querySelectorAll('[role=dialog]')].find(element => F.visible(element) && /^Reactions$/i.test(label(element)));
        const found = dialog && buttonLabeled(dialog, new RegExp('^' + kind + '$', 'i'));
        return found && found.length === 1 ? found[0] : null;
      }, 5000);
      if (!choice) throw Error('Facebook did not show its reactions. Nothing was changed.');
      choice.click();
    }
    if (!await F.waitFor(() => { const now = like(); return now && label(now).toLowerCase() === want.toLowerCase(); }, 8000)) throw Error('Facebook did not confirm the reaction. Refresh to check.');
    return read();
  }

  window.__blindfbPost = {
    setTitle(title) { current(); if (card && !reel.text) reel.text = title; return read(); },
    like, shareOpen, shareFill, shareNow, shareClose, reelReact,
    async initialize(url) {
      target = url;
      if (/^\/reel\//.test(new URL(url).pathname)) {
        card = await F.waitFor(reelCard, 15000);
        if (!card) throw Error('The reel could not be read.');
        // BlindFB plays the reel itself; a playing page would move on to the next reel.
        for (const video of document.querySelectorAll('video')) { video.muted = true; video.pause(); }
        const expand = [...card.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && /^(See more|Xem thêm|Voir plus|Afficher la suite)$/i.test(F.clean(element.innerText || '') || label(element)));
        for (const button of expand) if (button.isConnected) button.click();
        if (expand.length && !await F.waitFor(() => expand.every(button => !button.isConnected || !F.visible(button) || !/^(See more|Xem thêm|Voir plus|Afficher la suite)$/i.test(F.clean(button.innerText || '') || label(button))), 5000)) throw Error('Facebook has not expanded the reel caption. Refresh the reader to retry.');
        const author = [...card.querySelectorAll('a[href]')].map(a => F.clean(a.innerText || '')).find(Boolean) || '';
        // Caption lines, without the author, badges, Follow and the reaction, comment and share counts.
        const walker = document.createTreeWalker(card, NodeFilter.SHOW_TEXT), lines = [];
        let node;
        while ((node = walker.nextNode())) {
          const element = node.parentElement;
          if (element.closest('script,style,button,[role=button],[aria-hidden=true]') || !F.visible(element)) continue;
          const text = F.clean(node.nodeValue || '');
          if (text) lines.push(text);
        }
        const caption = lines.filter(text => text !== author && !/^(·|Follow|Following|Verified account|[\d.,]+[KMB]?)$/i.test(text));
        reel = {author, text: caption.join('\n'), links: links(card, card)};
        const comment = buttonLabeled(card, /^Comment$/);
        if (comment.length !== 1) throw Error('This reel has no unique Comment button.');
        comment[0].click();
        root = await F.waitFor(() => [...document.querySelectorAll('[role=complementary]')].find(element => F.visible(element) && element.querySelector(EDITOR + ',' + ARTICLE)), 10000);
        if (!root) throw Error("The reel's comments could not be opened.");
        address = location.href;
        return read();
      }
      // Watch only exposes the full post after opening its comments dialog.
      if (/\/videos\/|^\/watch\/?/.test(new URL(url).pathname)) {
        const button = await F.waitFor(() => {
          const found = buttonLabeled(document, /^Leave a comment$/);
          return found.length === 1 ? found[0] : null;
        }, 15000);
        if (!button) throw Error('The video comments could not be opened.');
        button.click();
      }
      root = await F.waitFor(() => {
        const visible = [...document.querySelectorAll('[role=dialog]')].filter(element => F.visible(element) && element.querySelector('[data-ad-rendering-role=profile_name]'));
        // Facebook nests one post dialog inside another wrapper dialog; keep the innermost.
        const candidates = visible.filter(element => !visible.some(other => other !== element && element.contains(other)));
        if (candidates.length > 1) throw Error('More than one post dialog is open.');
        return candidates[0] || document.querySelector('[role=main] [data-ad-rendering-role=profile_name]')?.closest('[role=main]');
      }, 15000);
      address = location.href;
      if (!root) throw Error('The post could not be read.');
      for (const video of document.querySelectorAll('video')) { video.muted = true; video.pause(); }
      return read();
    }, read, prepare, check, confirm
  };
})();
