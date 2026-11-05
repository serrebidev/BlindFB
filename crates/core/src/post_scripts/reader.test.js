// Dependency-free DOM fixtures; never connects to Facebook or submits anything.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/reader.js', 'utf8');
const ARTICLE = '[role=article][aria-label]';
const EDITOR = '[role=textbox][contenteditable=true],textarea';

function fixture(mode = false) {
  const watch = mode === true, isReel = mode === 'reel' || mode === 'reel-count' || mode === 'untitled';
  let comments = [], focus, editor;
  const boxes = [];
  let opened = !watch, paused = false;
  let expanded = false;
  let expanding = false;
  const video = {muted: false, querySelectorAll: () => [], pause() { paused = true; }, getBoundingClientRect: () => ({height: 100, top: 0})};
  const expand = {getAttribute: () => expanded ? 'See less' : 'See more', isConnected: true,
    closest: () => mode === 'slow' ? {} : null,
    click() {if (mode === 'slow') expanding = true; else expanded = true;}};
  const author = {innerText: 'Reel author', href: 'https://www.facebook.com/author', closest: () => null, getAttribute: () => ''};
  const reelCard = {isConnected: true, closest: () => null, contains: () => true, querySelectorAll: selector => selector === 'a[href]' ? [author] : selector === '[role=button][aria-label]' ? [openComments] : selector === '[role=button],button' ? [expand] : []};
  video.parentElement = isReel ? reelCard : null;
  const openComments = {getAttribute: () => mode === 'reel-count' ? 'Comment, 6 comments' : isReel ? 'Comment' : 'Leave a comment', click() {opened = true;}};
  const cards = [{contains: () => false}, {contains: () => false}];
  // A YouTube clip shared into a post: Facebook wraps it in l.facebook.com/l.php?u=.
  const sharedVideo = {href: 'https://l.facebook.com/l.php?u=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3Dabc&h=AT0', getAttribute: () => '', closest: () => null, innerText: 'A video'};
  const messages = mode === 'shared' || mode === 'shared-source' || mode === 'ambiguous' ? ['Poster text', 'Shared original'].map(text => ({text, closest: () => null, contains: () => false})) : [{closest: () => null, contains: () => false}];
  if (mode === 'shared') messages.push({text: 'Hidden template', visible: false, closest: () => null, contains: () => false});
  const root = {isConnected: true, getAttribute: () => mode === 'ambiguous' ? 'main' : 'dialog', contains: e => e === root || e === list || comments.includes(e) || cards.includes(e) || boxes.includes(e),    querySelector: selector => selector.includes('profile_name') ? profile : isReel && selector.includes('contenteditable') ? {} : null,
    querySelectorAll: selector => selector === ARTICLE ? comments : selector === 'a[href]' && mode === 'external-video' ? [sharedVideo] : selector === '[data-ad-rendering-role=profile_name]' && mode === 'shared-source' ? profiles : selector === '[role=button],button' && mode === 'slow' ? [expand] : selector.includes('story_message') ? watch ? [] : messages : selector.includes('text-align') && watch ? [{closest: () => null, contains: () => false}] : selector === EDITOR ? boxes : []};
  const profile = {innerText: 'Post author', closest: () => null, parentElement: {parentElement: {parentElement: {querySelectorAll: () => []}}}};
  let revealed = false;
  const sourceLink = {get href() {return revealed ? 'https://www.facebook.com/bistro/posts/2?__cft__[0]=tracking' : 'https://www.facebook.com/a/posts/1?__cft__[0]=tracking';},
    getAttribute: () => revealed ? '/bistro/posts/2' : '?__cft__[0]=tracking', closest: () => null,
    focus() {revealed = true;}, blur() {}};
  const headerLink = (href, comment = false) => ({href, getAttribute: () => href, closest: () => comment ? {} : null});
  profile.parentElement.parentElement.parentElement.querySelectorAll = () => [headerLink('https://www.facebook.com/a/posts/1')];
  const profiles = [profile, {innerText: 'Big Feast Bistro + Retail', closest: () => null,
    parentElement: {parentElement: {parentElement: {querySelectorAll: () => [sourceLink,
      headerLink('https://www.facebook.com.evil/bistro/posts/2'),
      headerLink('https://www.facebook.com/bistro/posts/2?comment_id=3'),
      headerLink('https://www.facebook.com/bistro/posts/3', true)]}}}},
    {innerText: 'Hidden template', visible: false, closest: () => null},
    {innerText: 'Comment author', closest: () => ({}), parentElement: {parentElement: {parentElement: {querySelectorAll() {throw Error('Do not inspect comment headers');}}}}}];
  const document = {
    querySelectorAll: selector => selector === 'video' ? [video] : selector === '[role=button][aria-label]' ? [openComments] : (selector === '[role=dialog]' && watch || selector === '[role=complementary]' && isReel) && opened ? [root] : [],
    querySelector: () => opened ? {closest: () => root} : null,
    createTreeWalker(article) {
      let index = 0;
      const lines = article === reelCard ? mode === 'untitled' ? ['Reel author'] : ['Reel author', expanded ? 'Entire reel caption' : 'Short caption…', 'See less', '::-webkit-scrollbar {width: 7px;}'] : article.lines;
      return {nextNode() {
        if (index >= lines.length) return null;
        const at = index++;
        return {nodeValue: lines[at], parentElement: article === reelCard && at >= 2 ? {closest: () => true} : article};
      }};
    },
    get activeElement() { return focus; }
  };
  // Document order of comments and boxes, for compareDocumentPosition.
  const order = [];
  const place = element => {
    element.compareDocumentPosition = other => order.indexOf(other) > order.indexOf(element) ? 4 : 2;
    order.push(element);
    return element;
  };
  const list = {contains: e => comments.includes(e) || list === e || boxes.includes(e) && e.inList, parentElement: root};
  function article(id, parent = null) {
    const element = {
      lines: ['Comment author', '1h', 'Complete comment body'], isConnected: true,
      getAttribute: () => 'Comment author', closest: selector => selector === ARTICLE ? element : null,
      contains: e => e === element,
      parentElement: {closest: () => parent, contains: e => e === element, parentElement: list},
      querySelectorAll: selector => selector === 'a[href]' ? [{closest: () => element, href: `https://www.facebook.com/a/posts/1?comment_id=${parent ? (parent.parentId || parent.id) : id}${parent ? '&reply_comment_id=' + id : ''}`}]
        : selector === EDITOR ? editor && editor.parent === element ? [editor] : [] : element.actions || [], id
    };
    if (parent) element.parentId = parent.parentId || parent.id;
    comments.push(element);
    return place(element);
  }
  function box(parent, name, inList) {
    const made = place({parent, inList, innerText: '', isConnected: true, closest: () => parent, scrollIntoView() {}, getAttribute: () => name, blur() { if (focus === made) focus = null; }});
    boxes.push(made);
    editor = made;
    return made;
  }
  const makeEditor = (parent, name = 'Write a comment') => box(parent, name, parent !== null);
  const replyBox = name => box(null, name, true);
  // Pressing Reply focuses the given box, and Facebook may prefill it with a name.
  const replyAction = (owner, target, prefill = '') => ({closest: () => owner, getAttribute: () => 'Reply', click() { target.innerText = prefill; focus = target; }});
  let named = null, collapsed = false;
  const F = {
    canon: href => {const url = new URL(href); return url.pathname;},
    clean: text => text.trim(), TIME: /^\d+h$/, visible: element => element.visible !== false, enabled: () => true,
    vtext: scope => scope === reelCard ? ['Reel author', expanded ? 'Entire reel caption' : 'Short caption…', 'See more', 'Following', '::-webkit-scrollbar {width: 7px;}'] : ['Post author', '1h'],
    postText: element => element === reelCard ? ['Reel author', expanded ? 'Entire reel caption' : 'Short caption…', 'See more', 'Following'].join('\n') : mode === 'slow' && !expanded ? 'Short caption… See more' : element.text || 'Entire post text',
    postCards: () => mode === 'ambiguous' ? cards : [root], waitFor: async predicate => {
      const found = predicate();
      if (!found && expanding) {expanded = true; expanding = false; return predicate();}
      return found;
    }, sleep: async () => {},
    // The reader names the box it wants; the fixture gives one ref per matching textbox.
    pageSnapshot: name => {
      const matches = named || boxes.filter(element => element.getAttribute() === name);
      return matches.map((element, index) => `ref${index}\ttextbox\t${name}\t[redacted]`).join('\n');
    },
    pageTarget: ref => {
      const matches = named || boxes.filter(element => element.getAttribute() === F.lastName);
      focus = matches[Number(ref.slice(3))] || editor;
    }
  };
  const snapshot = F.pageSnapshot;
  F.pageSnapshot = name => { F.lastName = name; return snapshot(name); };
  const window = {__fb: F};
  vm.runInNewContext(source, {window, document, location: {href: 'https://www.facebook.com/a/posts/1'}, NodeFilter: {SHOW_TEXT: 4}, URL, innerHeight: 600,
    getSelection: () => ({selectAllChildren() {}, collapseToEnd() { collapsed = true; }})});
  return {api: window.__blindfbPost, article, makeEditor, replyBox, replyAction, remove: e => {e.isConnected = false;}, snapshot: value => { named = value; }, get focused() {return focus;}, get collapsed() {return collapsed;}, set comments(value) {comments = value;}, get paused() {return paused;}};
}

(async () => {
  const slow = fixture('slow');
  assert.equal(JSON.parse(await slow.api.initialize('https://www.facebook.com/a/posts/1')).text, 'Entire post text');
  const shared = fixture('shared');
  assert.equal(JSON.parse(await shared.api.initialize('https://www.facebook.com/a/posts/1')).text, 'Poster text\nShared original');
  const sourced = JSON.parse(await fixture('shared-source').api.initialize('https://www.facebook.com/a/posts/1'));
  assert.deepEqual(sourced.links, [{label: 'Shared post by Big Feast Bistro + Retail', url: 'https://www.facebook.com/bistro/posts/2'}]);
  const ambiguous = fixture('ambiguous');
  await assert.rejects(ambiguous.api.initialize('https://www.facebook.com/a/posts/1'), /More than one post/);
  const reel = fixture('reel');
  const reeled = JSON.parse(await reel.api.initialize('https://www.facebook.com/reel/123/'));
  assert.equal(reeled.text, 'Entire reel caption');
  assert.equal(reeled.author, 'Reel author');
  assert.equal(reeled.video, 'https://www.facebook.com/reel/123/');
  const counted = JSON.parse(await fixture('reel-count').api.initialize('https://www.facebook.com/reel/123/'));
  assert.equal(counted.text, 'Entire reel caption');
  const untitled = fixture('untitled');
  assert.equal(JSON.parse(await untitled.api.initialize('https://www.facebook.com/reel/123/')).text, '');
  assert.equal(JSON.parse(await untitled.api.setTitle('Dedicated To Bart Simpson')).text, 'Dedicated To Bart Simpson');
  assert.equal(JSON.parse(await untitled.api.read()).text, 'Dedicated To Bart Simpson');
  assert.equal(JSON.parse(await reel.api.setTitle('Other title')).text, 'Entire reel caption');
  const sharedVideo = JSON.parse(await fixture('external-video').api.initialize('https://www.facebook.com/a/posts/1'));
  assert.equal(sharedVideo.video, 'https://www.youtube.com/watch?v=abc');
  assert.deepEqual(sharedVideo.links, [{label: 'A video', url: 'https://www.youtube.com/watch?v=abc'}]);
  const watch = fixture(true);
  const watched = JSON.parse(await watch.api.initialize('https://www.facebook.com/page/videos/123/'));
  assert.equal(watched.author, 'Post author');
  assert.equal(watched.text, 'Entire post text');
  assert.equal(watch.paused, true);

  let f = fixture(), parent = f.article('1');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  await assert.rejects(f.api.prepare('missing', 'hello'), /changed or disappeared/);
  f.remove(parent);
  await assert.rejects(f.api.prepare('1', 'hello'), /changed or disappeared/);

  f = fixture(); parent = f.article('1');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  parent.lines[2] = 'Edited after selection';
  await assert.rejects(f.api.prepare('1', 'hello'), /changed or disappeared/);

  f = fixture(); parent = f.article('1');
  parent.actions = [1, 2].map(() => ({closest: () => parent, getAttribute: () => 'Reply'}));
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  await assert.rejects(f.api.prepare('1', 'hello'), /unique Reply action/);

  f = fixture(); parent = f.article('1');
  f.makeEditor(null); // A top-level editor must never satisfy a reply.
  parent.actions = [{closest: () => parent, getAttribute: () => 'Reply', click() {}}];
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  await assert.rejects(f.api.prepare('1', 'hello'), /inside the selected comment thread/);

  f = fixture(); parent = f.article('1');
  const box = f.makeEditor(parent);
  parent.actions = [f.replyAction(parent, box)];
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  assert.equal(await f.api.prepare('1', 'hello'), '"hello"');
  box.innerText = 'hello';
  assert.equal(f.api.check(), true);
  box.innerText = '';
  const wrong = f.article('2'); wrong.lines[2] = 'hello';
  await assert.rejects(f.api.confirm(), /not confirmed in the selected thread/);

  await f.api.read();
  await f.api.prepare('1', 'hello'); box.innerText = 'hello'; f.api.check(); box.innerText = '';
  const right = f.article('3', parent); right.lines[2] = 'hello';
  assert.equal(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '3').parent_id, '1');

  // Live layout: reply boxes sit beside the comments, outside every article, one per open thread.
  f = fixture(); parent = f.article('1');
  const replyBox = f.replyBox('Reply as Me');
  const next = f.article('2');
  const otherBox = f.replyBox('Reply as Me');
  const main = f.makeEditor(null, 'Comment as Me');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  // Reply boxes left open never make the top-level comment ambiguous.
  assert.equal(await f.api.prepare(null, 'top'), '"top"');
  assert.equal(f.focused, main);
  main.innerText = 'top';
  assert.equal(f.api.check(), true);
  main.innerText = '';
  parent.actions = [f.replyAction(parent, replyBox)];
  assert.equal(await f.api.prepare('1', 'hello'), '"hello"');
  assert.equal(f.focused, replyBox);
  replyBox.innerText = 'hello';
  assert.equal(f.api.check(), true);
  replyBox.innerText = '';
  // Facebook may drop a reply's own box once it is sent.
  f.remove(replyBox);
  const sent = f.article('4', parent); sent.lines[2] = 'hello';
  assert.equal(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '4').parent_id, '1');
  replyBox.isConnected = true;

  // Focus inside another thread, or on the top-level box, is refused.
  await f.api.read();
  parent.actions = [f.replyAction(parent, otherBox)];
  await assert.rejects(f.api.prepare('1', 'hello'), /inside the selected comment thread/);
  next.actions = [f.replyAction(next, main)];
  await assert.rejects(f.api.prepare('2', 'hello'), /inside the selected comment thread/);
  // A reply on the last thread still finds the box after it.
  next.actions = [f.replyAction(next, otherBox)];
  assert.equal(await f.api.prepare('2', 'hello'), '"hello"');

  // Only a reply box showing: a top-level comment must not land in it, but the reply may.
  f = fixture(); parent = f.article('1');
  const lone = f.replyBox('Reply as Me');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  await assert.rejects(f.api.prepare(null, 'top'), /No unique comment box/);
  parent.actions = [f.replyAction(parent, lone)];
  assert.equal(await f.api.prepare('1', 'hello'), '"hello"');

  // Replying inside an open thread: Facebook prefills the person's name, which is kept.
  f = fixture(); parent = f.article('1');
  const reply = f.article('2', parent);
  const named = f.replyBox('Reply to Comment author');
  f.makeEditor(null, 'Comment as Me');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  reply.actions = [f.replyAction(reply, named, 'Comment author ')];
  assert.equal(await f.api.prepare('2', 'hello'), '"hello"');
  assert.equal(f.collapsed, true);
  named.innerText = 'Comment author hello';
  assert.equal(f.api.check(), true);
  named.innerText = '';
  // A reply to a reply joins the thread's top comment and shows the name in its text.
  const threaded = f.article('5', parent); threaded.lines[2] = 'Comment author'; threaded.lines[3] = 'hello';
  assert.equal(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '5').parent_id, '1');
  // A name with no space after it gets one.
  await f.api.read();
  reply.actions = [f.replyAction(reply, named, 'Comment author')];
  assert.equal(await f.api.prepare('2', 'hello'), '" hello"');
  // Anything else already in the box is a draft and is never overwritten.
  await f.api.read();
  reply.actions = [f.replyAction(reply, named, 'my unsent words')];
  await assert.rejects(f.api.prepare('2', 'hello'), /already contains a draft/);

  // Two open boxes can share one name; only the focused one is filled.
  f = fixture(); parent = f.article('1');
  const twin = f.replyBox('Reply to Comment author');
  const mine = f.replyBox('Reply to Comment author');
  f.makeEditor(null, 'Comment as Me');
  f.snapshot([twin, mine]);
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  parent.actions = [f.replyAction(parent, mine)];
  assert.equal(await f.api.prepare('1', 'hello'), '"hello"');
  assert.equal(f.focused, mine);
  console.log('Reader safety fixtures passed: missing, stale, changed, ambiguous, no root fallback, exact-thread confirmation, reply boxes beside comments, prefilled names.');
})().catch(error => { console.error(error); process.exitCode = 1; });
