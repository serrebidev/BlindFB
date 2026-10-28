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
  const identities = new WeakMap();

  function current() {
    if (!root || !root.isConnected || location.href !== address) throw Error('Post changed or reader expired. Refresh or reopen the post.');
    return root;
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
    const ti = lines.findIndex((text, index) => index < 7 && time.test(text));
    const body = lines.slice(1).filter((text, index) => index + 1 !== ti && !action.test(text) && !/^((by )?author|Top fan|Follow|·)$/i.test(text));
    // A bare number after the text is the reaction count.
    if (body.length > 1 && /^\d+$/.test(body[body.length - 1])) body.pop();
    const ancestor = article.parentElement.closest(ARTICLE);
    const parent = ancestor && root.contains(ancestor) ? identity(ancestor).id : id.parent;
    return {id: id.id, author, time: ti >= 0 ? lines[ti] : '', text: body.join('\n'), parent_id: parent, url: id.url};
  }

  const fingerprint = value => JSON.stringify([value.id, value.author, value.text, value.parent_id, value.url]);
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
    const expand = [...root.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && /^(See more|Xem thêm|Voir plus)$/i.test(label(element) || F.clean(element.innerText || '')) && element.closest(ARTICLE + ',[data-ad-rendering-role=story_message]'));
    for (const button of expand) {
      if (button.isConnected) button.click();
    }
    if (expand.length) await F.sleep(300);
    const rows = collect();
    records = new Map(rows.map(row => [row.value.id, {...row, key: fingerprint(row.value)}]));
    const profile = root.querySelector('[data-ad-rendering-role=profile_name]');
    const head = profile ? F.vtext(profile.parentElement.parentElement.parentElement) : [];
    const messages = [...root.querySelectorAll('[data-ad-rendering-role=story_message]')].filter(element => !element.closest(ARTICLE));
    if (messages.length > 1) throw Error('More than one post is visible. Open the individual post permalink.');
    // Group posts name the group, then the poster; comments carry their own names.
    const names = [...root.querySelectorAll('[data-ad-rendering-role=profile_name]')].filter(e => !e.closest(ARTICLE)).map(e => F.clean(e.innerText || '').replace(/\s+/g, ' ')).filter(Boolean);
    const author = names.length ? [...new Set(names)].join(' / ') : profile ? F.clean(profile.innerText || '').replace(/\s+/g, ' ') : '';
    return JSON.stringify({url: target, author, time: head.find(text => time.test(text)) || '', text: messages.length ? F.postText(messages[0]) : '', comments: rows.map(row => row.value)});
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
    const buttons = [...root.querySelectorAll('[role=button],button')].filter(element => !element.closest(ARTICLE) && F.visible(element) && /^Share\b/i.test(label(element)));
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

  window.__blindfbPost = {
    like, shareOpen, shareFill, shareNow, shareClose,
    async initialize(url) {
      target = url;
      root = await F.waitFor(() => {
        const visible = [...document.querySelectorAll('[role=dialog]')].filter(element => F.visible(element) && element.querySelector('[data-ad-rendering-role=profile_name]'));
        // Facebook nests one post dialog inside another wrapper dialog; keep the innermost.
        const candidates = visible.filter(element => !visible.some(other => other !== element && element.contains(other)));
        if (candidates.length > 1) throw Error('More than one post dialog is open.');
        return candidates[0] || document.querySelector('[role=main] [data-ad-rendering-role=profile_name]')?.closest('[role=main]');
      }, 15000);
      address = location.href;
      if (!root) throw Error('The post could not be read.');
      return read();
    }, read, prepare, check, confirm
  };
})();
