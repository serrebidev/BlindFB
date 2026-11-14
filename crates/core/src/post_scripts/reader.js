(() => {
  const F = window.__fb;
  const ARTICLE = '[role=article][aria-label]';
  const EDITOR = '[role=textbox][contenteditable=true],textarea';
  const EMOJI = /^[\p{Extended_Pictographic}\p{Emoji_Component}]+$/u;
  const own = (element, article) => element.closest(ARTICLE) === article;
  const label = element => F.clean(element.getAttribute('aria-label') || element.getAttribute('placeholder') || '');
  const rawContents = element => element.value === undefined ? element.innerText || '' : element.value;
  const contents = element => F.clean(rawContents(element));
  // Facebook's words per language: labels.js, injected before this script.
  const W = window.__blindfbLabels;
  const action = W.commentAction;
  const time = F.TIME || W.time;
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
    while (element && !buttonLabeled(element, W.comment).length) element = element.parentElement;
    // Until it shows its author the card is still loading, or is the next reel's.
    return element && [...element.querySelectorAll('a[href]')].some(a => F.clean(a.innerText || '')) ? element : null;
  }

  // A video page on another site (a YouTube clip shared into a post, say) has no Facebook
  // video behind it; the player fetches the page itself. Same sites the player accepts.
  const EXTERNAL_HOSTS = ['youtube.com', 'youtu.be', 'vimeo.com', 'dailymotion.com', 'twitch.tv', 'tiktok.com', 'streamable.com', 'rumble.com'];
  const externalVideo = url => {
    if (url.protocol !== 'https:' || url.username || url.password) return false;
    const host = url.hostname.toLowerCase().replace(/\.$/, '');
    return EXTERNAL_HOSTS.some(domain => host === domain || host.endsWith('.' + domain));
  };
  // Where the post's media lives: a video link inside it, else the post itself when it plays one.
  function videoTarget(scope) {
    if (card) return target;
    const link = [...scope.querySelectorAll('a[href]')].filter(a => !a.closest(ARTICLE)).map(a => a.href).find(href => /facebook\.com\/(?:[^/?#]+\/videos\/|videos\/|reel\/|watch\/?\?v=)/.test(href));
    if (link) return F.canon(link) ? 'https://www.facebook.com' + F.canon(link) : link;
    if (scope.querySelector('video')) return target;
    // A link-preview card or an embedded player, unwrapped like any other link.
    for (const a of scope.querySelectorAll('a[href]')) {
      if (a.closest(ARTICLE)) continue;
      let url;
      try { url = new URL(a.href, location.href); } catch { continue; }
      if (/^l\.facebook\.com$/.test(url.hostname) && url.searchParams.get('u')) {
        try { url = new URL(url.searchParams.get('u')); } catch { continue; }
      }
      if (externalVideo(url)) return url.href;
    }
    for (const frame of scope.querySelectorAll('iframe[src]')) {
      let url;
      try { url = new URL(frame.src || frame.getAttribute('src'), location.href); } catch { continue; }
      if (externalVideo(url)) return url.href;
    }
    return null;
  }

  // Facebook leaves some links as a bare "?__cft__…" until they are focused: a link-preview
  // card then points at its site (https://13x.audio/…), not at the post itself.
  async function reveal(scope) {
    for (const a of scope.querySelectorAll('a[href]')) {
      if (a.closest(ARTICLE) || !F.visible(a) || !/^\?/.test(a.getAttribute('href') || '')) continue;
      a.focus({preventScroll: true});
      await F.waitFor(() => !/^\?/.test(a.getAttribute('href') || ''), 400, 30);
      a.blur();
    }
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
      if (!/^https?:$/.test(url.protocol)) continue;
      const external = !/(^|\.)facebook\.com$/.test(url.hostname);
      // Facebook's hashtag pages are UI; another site's /hashtag/ page is a real link.
      if (!external && /\/hashtag\//.test(url.pathname)) continue;
      const inMessage = (Array.isArray(message) ? message : [message]).some(element => element && element.contains(a));
      if (!external && !inMessage) continue;
      for (const key of [...url.searchParams.keys()]) if (/^(__cft__|__tn__|fbclid)/.test(key)) url.searchParams.delete(key);
      const text = F.clean(a.innerText || '') || label(a) || url.hostname;
      if (!found.has(url.href)) found.set(url.href, {label: text, url: url.href});
    }
    return [...found.values()];
  }

  // The browsable links inside one comment: the post-wide links() skips
  // article internals, so comments collect their own — real targets behind
  // the redirect wrapper, minus the author link, comment permalinks,
  // hashtags and anchors whose target is not revealed yet.
  function commentLinks(article) {
    const found = new Map();
    for (const a of article.querySelectorAll('a[href]')) {
      if (!own(a, article)) continue;
      const raw = (typeof a.getAttribute === 'function' ? a.getAttribute('href') : a.href) || '';
      if (!raw || raw.startsWith('?') || raw.startsWith('#')) continue;
      let url;
      try { url = new URL(a.href, location.href); } catch { continue; }
      if (url.hostname === 'l.facebook.com' && url.searchParams.get('u')) {
        try { url = new URL(url.searchParams.get('u')); } catch { continue; }
      }
      if (!/^https?:$/.test(url.protocol)) continue;
      // Facebook's own UI links only: an external page may use the same words.
      const facebook = /(^|\.)facebook\.com$/.test(url.hostname);
      if (facebook && (/\/hashtag\//.test(url.pathname) || url.searchParams.has('comment_id') || url.searchParams.has('reply_comment_id'))) continue;
      if (/(^|\.)facebook\.com$/.test(url.hostname) && F.profilePath && F.profilePath(url.href)) continue;
      for (const key of [...url.searchParams.keys()]) if (/^(__cft__|__tn__|fbclid)/.test(key)) url.searchParams.delete(key);
      const text = F.clean(a.innerText || '') || url.hostname;
      if (!found.has(url.href)) found.set(url.href, {label: text, url: url.href});
      if (found.size >= 10) break;
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
        // The name links to its profile (a group's link is no profile); the app opens it itself.
        const profile = F.profilePath && F.profilePath(url.href);
        if (profile) {
          const address = 'https://www.facebook.com' + profile;
          if (!found.has(address)) found.set(address, {label: F.clean(a.innerText || '').replace(/\s+/g, ' ') || name, url: address, profile});
          continue;
        }
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
    // Facebook draws emoji as pictures; they stay on the line of the words around them.
    const walker = document.createTreeWalker(article, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    const rows = [];
    let node;
    while ((node = walker.nextNode())) {
      const picture = node.nodeType === 1;
      if (picture && !(node.tagName === 'IMG' && EMOJI.test(node.alt))) continue;
      const element = picture ? node : node.parentElement;
      // aria-hidden skips Facebook's 1x1 screen-reader tooltip with the full date
      // ("Thursday, November 12, 2026 at 3:48 PM"), which led every comment's text.
      if (!own(element, article) || element.closest(EDITOR + ',script,style,[aria-hidden=true]') || !F.visible(element)) continue;
      const block = element.closest('[dir=auto]'), last = rows[rows.length - 1];
      const text = picture ? node.alt : node.nodeValue || '';
      if (last && block && last.block === block && (picture || last.picture)) last.text += text;
      else rows.push({block, text});
      rows[rows.length - 1].picture = picture;
    }
    const lines = rows.map(row => F.clean(row.text)).filter(Boolean);
    const author = lines[0] || article.getAttribute('aria-label') || '';
    const authorRef = F.profilePath ? [...article.querySelectorAll('a[href]')].filter(a => a.closest(ARTICLE) === article).map(a => F.profilePath(a.href !== undefined ? a.href : a.getAttribute('href'))).find(Boolean) || null : null;
    const ti = lines.findIndex((text, index) => index < 7 && time.test(text));
    const body = lines.slice(1).filter((text, index) => index + 1 !== ti && !action.test(text) && !W.commentBadge.test(text));
    // A bare number after the text is the reaction count.
    if (body.length > 1 && /^\d+$/.test(body[body.length - 1])) body.pop();
    const ancestor = article.parentElement.closest(ARTICLE);
    const parent = ancestor && root.contains(ancestor) ? identity(ancestor).id : id.parent;
    return {id: id.id, author, author_ref: authorRef, time: ti >= 0 ? lines[ti] : '', text: body.join('\n'), parent_id: parent, url: id.url, links: commentLinks(article)};
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

  // Facebook hides replies behind "View replies" buttons and later top-level
  // comments behind "View more comments"; neither is in the page until pressed.
  // Press both in bounded rounds (loaded comments can hide replies of their own,
  // and a reply can hide further replies) so read() returns the whole thread,
  // not just the first batch (issues #22 and #30).
  async function expandReplies() {
    const loadsMore = element => {
      const text = F.clean(element.innerText || '') || label(element);
      return W.viewReplies.test(text) || W.viewMoreComments.test(text);
    };
    for (let round = 0; round < 10; round++) {
      const buttons = [...root.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && loadsMore(element));
      if (!buttons.length) return;
      const before = root.querySelectorAll(ARTICLE).length;
      for (const button of buttons) {
        if (button.isConnected) button.click();
      }
      const grew = await F.waitFor(() => root.querySelectorAll(ARTICLE).length > before || buttons.every(button => !button.isConnected || !F.visible(button) || !loadsMore(button)), 5000);
      if (!grew) return;
    }
  }

  async function read() {
    current();
    const watch = /\/videos\/|^\/watch\/?/.test(new URL(target).pathname);
    // A photo page's side panel holds only this photo's caption and comments.
    const photo = root.getAttribute('role') === 'complementary';
    // Comments and replies loaded by expandReplies() arrive collapsed too: expand again after it.
    const expandText = async () => {
      const expand = [...root.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && W.seeMore.test(F.clean(element.innerText || '') || label(element)) && (watch || photo || element.closest(ARTICLE + ',[data-ad-rendering-role=story_message]')));
      for (const button of expand) {
        if (button.isConnected) button.click();
      }
      if (expand.length && !await F.waitFor(() => expand.every(button => !button.isConnected || !F.visible(button) || !W.seeMore.test(F.clean(button.innerText || '') || label(button))), 5000)) throw Error('Facebook has not expanded all text. Refresh the reader to retry.');
    };
    await expandText();
    await expandReplies();
    await expandText();
    const rows = collect();
    records = new Map(rows.map(row => [row.value.id, {...row, key: fingerprint(row.value)}]));
    if (card) {
      return JSON.stringify({url: target, ...reel, time: '', comments: rows.map(row => row.value), video: target});
    }
    await reveal(root);
    const profile = root.querySelector('[data-ad-rendering-role=profile_name]');
    const head = profile ? F.vtext(profile.parentElement.parentElement.parentElement) : [];
    const messages = [...root.querySelectorAll('[data-ad-rendering-role=story_message]')].filter(element => F.visible(element) && !element.closest(ARTICLE));
    // Photo captions carry no story_message marker: the longest plain text block before the reactions.
    if (photo && !messages.length) {
      const like = root.querySelector('[data-ad-rendering-role=like_button]');
      const caption = [...root.querySelectorAll('[dir=auto]')].filter(element => F.visible(element) && !element.closest('a,h2,h3,' + ARTICLE + ',' + EDITOR) && (!like || element.compareDocumentPosition(like) & Node.DOCUMENT_POSITION_FOLLOWING)).sort((a, b) => (b.innerText || '').length - (a.innerText || '').length)[0];
      if (caption) messages.push(caption);
    }
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
    // A shared post's original author heads an unmarked linked heading between the sharer and the reactions.
    const like = root.querySelector('[data-ad-rendering-role=like_button]');
    for (const h of profile ? root.querySelectorAll('h2,h3,h4') : []) {
      const a = h.querySelector('a[href]');
      if (!a || !F.visible(h) || h.closest(ARTICLE + ',[data-ad-rendering-role=profile_name]') || h.querySelector('[data-ad-rendering-role=profile_name]') || !(profile.compareDocumentPosition(h) & Node.DOCUMENT_POSITION_FOLLOWING) || (like && !(h.compareDocumentPosition(like) & Node.DOCUMENT_POSITION_FOLLOWING))) continue;
      const name = F.clean(a.innerText || '').replace(/\s+/g, ' ');
      if (name) names.push(name);
    }
    // A photo page names its poster in the panel's first linked heading.
    const photoAuthor = photo && !profile ? F.clean([...root.querySelectorAll('h2 a[href],h3 a[href]')].find(a => F.visible(a) && !a.closest(ARTICLE))?.innerText || '').replace(/\s+/g, ' ') : '';
    const author = names.length ? [...new Set(names)].join(' / ') : profile ? F.clean(profile.innerText || '').replace(/\s+/g, ' ') : photoAuthor;
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

  // Facebook draws reply boxes beside the comments, one per open thread, outside every article.
  // The top-level box sits outside the comment list, so it alone encloses every other box.
  function composer() {
    const boxes = [...root.querySelectorAll(EDITOR)].filter(element => F.visible(element) && F.enabled(element) && !element.closest(ARTICLE));
    const first = root.querySelectorAll(ARTICLE)[0];
    const scope = box => { let element = first; while (!element.contains(box)) element = element.parentElement; return element; };
    const found = boxes.length > 1 && first ? boxes.filter(box => boxes.every(other => other === box || (scope(box) !== scope(other) && scope(box).contains(scope(other))))) : boxes;
    return found.length === 1 && !W.replyStart.test(label(found[0])) ? found[0] : null;
  }

  // The box Facebook focused for this comment's Reply: it follows the comment, with only the
  // same thread between them. Replies are not nested in the comment they answer.
  function threadBox(article) {
    const box = document.activeElement;
    const allowed = [...root.querySelectorAll(EDITOR)].filter(element => F.visible(element) && F.enabled(element) && [null, article].includes(element.closest(ARTICLE)));
    const follows = (a, b) => a.compareDocumentPosition(b) & 4;
    if (!allowed.includes(box) || box === composer() || !follows(article, box)) return null;
    const threadOf = value => value.parent_id || value.id;
    const thread = threadOf(comment(article));
    return collect().every(({element, value}) => !follows(article, element) || !follows(element, box) || threadOf(value) === thread) ? box : null;
  }

  function editorFor(parent) {
    current();
    return parent === null ? composer() : threadBox(selected(parent));
  }

  const flat = text => F.clean(text).replace(/\s+/g, ' ');
  // The words of a comment. Facebook can turn :) into an emoji, trim a link's address and
  // cut long text behind See more, so confirmation compares words and only their start.
  const words = text => F.clean(text).replace(/https?:\/\/(www\.)?/g, '').replace(/[^\p{L}\p{N}]/gu, '').toLowerCase();

  // Returns the exact text to type at the caret, which is left after any prefilled name.
  async function prepare(parent, text) {
    current();
    pending = null;
    let thread = null;
    if (parent !== null) {
      const article = selected(parent);
      const replies = [...article.querySelectorAll('[role=button],button,a')].filter(element => own(element, article) && F.visible(element) && W.reply.test(label(element) || F.clean(element.innerText || '')));
      if (replies.length !== 1) throw Error('The selected comment has no unique Reply action. Nothing was sent.');
      const value = comment(article);
      thread = value.parent_id || value.id;
      // Blurred first, so the focus afterwards can only come from this Reply.
      document.activeElement?.blur?.();
      replies[0].click();
    }
    const editor = await F.waitFor(() => editorFor(parent), 5000);
    if (!editor) throw Error(parent === null ? 'No unique comment box on this post. Nothing was sent.' : 'No unique editor inside the selected comment thread. Nothing was sent.');
    // Replying inside an open thread prefills the person's name; the box's name says who it is.
    const draft = contents(editor);
    if (draft && (parent === null || !label(editor).includes(draft))) throw Error('The Facebook editor already contains a draft. Nothing was overwritten.');
    editor.scrollIntoView({block: 'center'});
    const name = label(editor);
    if (!name || name.length > 160) throw Error('The comment editor has no reliable accessible name. Nothing was sent.');
    const refs = F.pageSnapshot(name).split('\n').map(line => line.split('\t')).filter(row => row[1] === 'textbox');
    // Two open boxes can share a name, so the one that takes focus is the one to fill.
    const found = refs.some(row => { try { F.pageTarget(row[0], 'fill', false); } catch { return false; } return document.activeElement === editor; });
    if (!found) throw Error('The comment editor is ambiguous. Nothing was sent.');
    if (draft) { const selection = getSelection(); selection.selectAllChildren(editor); selection.collapseToEnd(); }
    const typed = draft && !/[  ]$/.test(rawContents(editor)) ? ' ' + text : text;
    pending = {parent, thread, start: words(text).slice(0, 40), text: flat(rawContents(editor) + typed), editor, before: new Set(collect().map(row => row.value.id))};
    return JSON.stringify(typed);
  }

  function check() {
    current();
    if (!pending || !pending.editor.isConnected || document.activeElement !== pending.editor || flat(contents(pending.editor)) !== pending.text || editorFor(pending.parent) !== pending.editor) throw Error('The selected comment editor changed. Nothing was sent.');
    return true;
  }

  async function confirm() {
    const sent = pending;
    pending = null;
    if (!sent) throw Error('There is no pending submission.');
    const confirmed = await F.waitFor(() => {
      // A group that reviews newcomers ("Participation review") opens its questions instead of
      // posting: a shown dialog outside the post, with answer controls and no comment editor.
      const questions = window.__blindfbQuestions.dialog(root);
      if (questions) return {questions};
      current();
      // A reply's own box can vanish once sent; one that stays must have been emptied.
      if (sent.editor.isConnected && contents(sent.editor)) return false;
      // While posting, a comment's permalink holds a temporary client id, so it is not accepted yet.
      // A reply to a reply joins the thread's top comment, with the name at the start of its text.
      return collect().some(({value}) => value.url && !/^client:/.test(value.id) && !sent.before.has(value.id) && (value.parent_id === sent.parent || value.parent_id === sent.thread) && words(value.text).includes(sent.start));
    }, 12000);
    if (!confirmed) throw Error('Submission was not confirmed in the selected thread. Refresh before trying again; do not resend automatically.');
    if (confirmed.questions) return window.__blindfbQuestions.read(confirmed.questions);
    return read();
  }

  // Like or unlike one exact comment; confirmed by its Like button changing state.
  async function like(id) {
    const article = selected(id);
    const buttons = [...article.querySelectorAll('[role=button],button')].filter(element => own(element, article) && F.visible(element) && W.commentLike.test(label(element) || F.clean(element.innerText || '')));
    if (buttons.length !== 1) throw Error('The selected comment has no unique Like button. Nothing was changed.');
    const button = buttons[0];
    const state = () => [label(button), button.getAttribute('aria-pressed'), F.clean(button.innerText || ''), button.className].join('|');
    const before = state();
    button.click();
    if (!await F.waitFor(() => !button.isConnected || state() !== before, 8000)) throw Error('Facebook did not confirm the Like. Refresh to check.');
    return read();
  }

  let sharing = null;
  // Facebook answers some buttons (Share) only to a full pointer sequence, not a bare click().
  const activate = element => (F.press ? F.press(element) : element.click());
  const shareNowButton = dialog => [...dialog.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && W.shareNow.test(label(element) || F.clean(element.innerText || '')));

  function shareButton() {
    current();
    const buttons = [...(card || root).querySelectorAll('[role=button],button')].filter(element => !element.closest(ARTICLE) && F.visible(element) && W.share.test(label(element)));
    if (buttons.length !== 1) throw Error('This post has no unique Share button. Nothing was shared.');
    return buttons[0];
  }

  // Facebook ignores script clicks on Share inside a post dialog, so the backend clicks it for
  // real (`page click`). This is the post's Share button as a page-snapshot reference, or null.
  // Buttons with the same name (the same post behind the dialog) are told apart by their order,
  // which is the snapshot's order.
  function shareTarget() {
    const button = shareButton();
    const name = label(button);
    if (!F.pageSnapshot || !name) return JSON.stringify(null);
    const same = [...document.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && label(element) === name);
    const rows = F.pageSnapshot(name).split('\n').map(line => line.split('\t')).filter(row => row[1] === 'button' && row[2] === name);
    const index = same.indexOf(button);
    return JSON.stringify(index >= 0 && rows.length === same.length ? rows[index][0] : null);
  }

  // Open Facebook's own share dialog for this post and report its audience; nothing is shared yet.
  // `clicked`: the backend already clicked Share (see shareTarget); otherwise press it here.
  async function shareOpen(clicked = false) {
    const button = shareButton();
    if (!clicked) activate(button);
    let toFeed = null;
    const dialog = await F.waitFor(() => {
      const dialogs = [...document.querySelectorAll('[role=dialog]')].filter(element => element !== root && !element.contains(root) && F.visible(element));
      const found = dialogs.filter(element => shareNowButton(element).length === 1);
      if (found.length === 1) return found[0];
      // French Facebook asks where to share first ("Options de partage"); the feed opens the editor.
      const options = dialogs.find(element => W.shareOptions.test(label(element)));
      const feed = options ? [...options.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && W.shareToFeed.test(F.clean(element.innerText || '') || label(element))) : [];
      if (feed.length === 1 && toFeed !== feed[0]) {
        toFeed = feed[0];
        activate(toFeed);
      }
      return null;
    }, 10000);
    if (!dialog) throw Error('Facebook did not open its share dialog. Nothing was shared.');
    const privacy = [...dialog.querySelectorAll('[role=button],button')].map(label).find(text => W.editPrivacy.test(text)) || '';
    sharing = {dialog, text: ''};
    return JSON.stringify({audience: (privacy.match(W.sharingWith) || [])[1] || 'Facebook default'});
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

  // Close the unsent share; when Facebook asks about the draft, drop it (nothing was shared).
  async function shareClose() {
    const dialog = sharing && sharing.dialog;
    sharing = null;
    const close = dialog && dialog.isConnected && [...dialog.querySelectorAll('[role=button],button')].find(element => W.close.test(label(element)));
    if (!close) return 'closed';
    close.click();
    const discard = await F.waitFor(() => [...document.querySelectorAll('[role=button],button')].find(element => F.visible(element) && W.discardDraft.test(F.clean(element.innerText || '') || label(element))), 2000);
    if (discard) discard.click();
    return 'closed';
  }

  // React to a reel from its card (the backend's react command only knows post layouts).
  // The Like button reads "Like" with no reaction, "Remove <reaction>" otherwise; in French
  // "J’aime, 28 réactions". W.reelReaction reads either into 'none' or a reaction keyword.
  async function reelReact(kind) {
    current();
    if (!['like', 'love', 'care', 'haha', 'wow', 'sad', 'angry', 'none'].includes(kind)) throw Error('Unknown reaction. Nothing was changed.');
    const scope = card || root;
    const reaction = element => W.reelReaction(label(element), element.getAttribute('aria-pressed'));
    const like = () => {
      const found = [...scope.querySelectorAll('[role=button][aria-label]')].filter(element => F.visible(element) && !element.closest(ARTICLE) && reaction(element) !== null);
      return found.length === 1 ? found[0] : null;
    };
    const button = like();
    if (!button) throw Error('This reel has no Like button. Nothing was changed.');
    if (reaction(button) === kind) return read();
    if (kind === 'none') button.click();
    else {
      const open = buttonLabeled(scope, W.react).filter(element => !element.closest(ARTICLE));
      if (open.length !== 1) throw Error('This reel has no unique React button. Nothing was changed.');
      open[0].click();
      const choice = await F.waitFor(() => {
        const dialog = [...document.querySelectorAll('[role=dialog]')].find(element => F.visible(element) && W.reactionsDialog.test(label(element)));
        const found = dialog && buttonLabeled(dialog, W.reactionName(kind));
        return found && found.length === 1 ? found[0] : null;
      }, 5000);
      if (!choice) throw Error('Facebook did not show its reactions. Nothing was changed.');
      choice.click();
    }
    if (!await F.waitFor(() => { const now = like(); return now && reaction(now) === kind; }, 8000)) throw Error('Facebook did not confirm the reaction. Refresh to check.');
    return read();
  }

  window.__blindfbPost = {
    setTitle(title) { current(); if (card && !reel.text) reel.text = title; return read(); },
    like, shareTarget, shareOpen, shareFill, shareNow, shareClose, reelReact,
    async initialize(url) {
      target = url;
      if (/^\/reel\//.test(new URL(url).pathname)) {
        card = await F.waitFor(reelCard, 15000);
        if (!card) throw Error('The reel could not be read.');
        // BlindFB plays the reel itself; a playing page would move on to the next reel.
        for (const video of document.querySelectorAll('video')) { video.muted = true; video.pause(); }
        const expand = [...card.querySelectorAll('[role=button],button')].filter(element => F.visible(element) && W.seeMore.test(F.clean(element.innerText || '') || label(element)));
        for (const button of expand) if (button.isConnected) button.click();
        if (expand.length && !await F.waitFor(() => expand.every(button => !button.isConnected || !F.visible(button) || !W.seeMore.test(F.clean(button.innerText || '') || label(button))), 5000)) throw Error('Facebook has not expanded the reel caption. Refresh the reader to retry.');
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
        const caption = lines.filter(text => text !== author && !W.captionNoise.test(text));
        await reveal(card);
        reel = {author, text: caption.join('\n'), links: links(card, card)};
        const comment = buttonLabeled(card, W.comment);
        if (comment.length !== 1) throw Error('This reel has no unique Comment button.');
        // "Commenter, 2 commentaires": the button says whether there are comments to wait for.
        const counted = /,\s*(?!0\b)\d/.test(label(comment[0]));
        comment[0].click();
        root = await F.waitFor(() => [...document.querySelectorAll('[role=complementary]')].find(element => F.visible(element) && element.querySelector(EDITOR + ',' + ARTICLE)), 10000);
        if (!root) throw Error("The reel's comments could not be opened.");
        // The panel shows its comment box before its comments.
        if (counted) await F.waitFor(() => root.querySelector(ARTICLE), 5000);
        address = location.href;
        return read();
      }
      // Watch only exposes the full post after opening its comments dialog.
      if (/\/videos\/|^\/watch\/?/.test(new URL(url).pathname)) {
        const button = await F.waitFor(() => {
          const found = buttonLabeled(document, W.leaveComment);
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
        // A photo page (/photo/?fbid=) shows the caption and comments in a side panel, unmarked.
        const photo = /^\/photo(\.php)?\/?$/.test(new URL(url).pathname) ? [...document.querySelectorAll('[role=complementary]')].find(element => F.visible(element) && element.querySelector('[data-ad-rendering-role=like_button]')) : null;
        return candidates[0] || document.querySelector('[role=main] [data-ad-rendering-role=profile_name]')?.closest('[role=main]') || photo;
      }, 15000);
      address = location.href;
      if (!root) throw Error('The post could not be read.');
      for (const video of document.querySelectorAll('video')) { video.muted = true; video.pause(); }
      return read();
    }, read, prepare, check, confirm
  };
})();
