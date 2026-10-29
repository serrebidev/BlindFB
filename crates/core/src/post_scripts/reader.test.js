// Dependency-free DOM fixtures; never connects to Facebook or submits anything.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/reader.js', 'utf8');
const ARTICLE = '[role=article][aria-label]';
const EDITOR = '[role=textbox][contenteditable=true],textarea';

function fixture(mode = false) {
  const watch = mode === true, isReel = mode === 'reel';
  let comments = [], focus, editor;
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
  const openComments = {getAttribute: () => isReel ? 'Comment' : 'Leave a comment', click() {opened = true;}};
  const cards = [{contains: () => false}, {contains: () => false}];
  const messages = mode === 'shared' || mode === 'ambiguous' ? ['Poster text', 'Shared original'].map(text => ({text, closest: () => null, contains: () => false})) : [{closest: () => null, contains: () => false}];
  if (mode === 'shared') messages.push({text: 'Hidden template', visible: false, closest: () => null, contains: () => false});
  const root = {isConnected: true, getAttribute: () => mode === 'ambiguous' ? 'main' : 'dialog', contains: e => e === root || comments.includes(e) || cards.includes(e), querySelector: selector => selector.includes('profile_name') ? profile : isReel && selector.includes('contenteditable') ? {} : null,
    querySelectorAll: selector => selector === ARTICLE ? comments : selector === '[role=button],button' && mode === 'slow' ? [expand] : selector.includes('story_message') ? watch ? [] : messages : selector.includes('text-align') && watch ? [{closest: () => null, contains: () => false}] : selector === EDITOR && editor ? [editor] : []};
  const profile = {innerText: 'Post author', parentElement: {parentElement: {parentElement: {}}}};
  const document = {
    querySelectorAll: selector => selector === 'video' ? [video] : selector === '[role=button][aria-label]' ? [openComments] : (selector === '[role=dialog]' && watch || selector === '[role=complementary]' && isReel) && opened ? [root] : [],
    querySelector: () => opened ? {closest: () => root} : null,
    createTreeWalker(article) {
      let index = 0;
      const lines = article === reelCard ? ['Reel author', expanded ? 'Entire reel caption' : 'Short caption…', 'See less', '::-webkit-scrollbar {width: 7px;}'] : article.lines;
      return {nextNode() {
        if (index >= lines.length) return null;
        const at = index++;
        return {nodeValue: lines[at], parentElement: article === reelCard && at >= 2 ? {closest: () => true} : article};
      }};
    },
    get activeElement() { return focus; }
  };
  function article(id, parent = null) {
    const element = {
      lines: ['Comment author', '1h', 'Complete comment body'], isConnected: true,
      getAttribute: () => 'Comment author', closest: selector => selector === ARTICLE ? element : null,
      parentElement: {closest: () => parent},
      querySelectorAll: selector => selector === 'a[href]' ? [{closest: () => element, href: `https://www.facebook.com/a/posts/1?comment_id=${parent ? parent.id : id}${parent ? '&reply_comment_id=' + id : ''}`}]
        : selector === EDITOR ? editor && editor.parent === element ? [editor] : [] : element.actions || [], id
    };
    comments.push(element);
    return element;
  }
  function makeEditor(parent) {
    editor = {parent, innerText: '', isConnected: true, closest: () => parent, scrollIntoView() {}, getAttribute: () => 'Write a comment'};
    return editor;
  }
  const F = {
    clean: text => text.trim(), TIME: /^\d+h$/, visible: element => element.visible !== false, enabled: () => true,
    vtext: scope => scope === reelCard ? ['Reel author', expanded ? 'Entire reel caption' : 'Short caption…', 'See more', 'Following', '::-webkit-scrollbar {width: 7px;}'] : ['Post author', '1h'],
    postText: element => element === reelCard ? ['Reel author', expanded ? 'Entire reel caption' : 'Short caption…', 'See more', 'Following'].join('\n') : mode === 'slow' && !expanded ? 'Short caption… See more' : element.text || 'Entire post text',
    postCards: () => mode === 'ambiguous' ? cards : [root], waitFor: async predicate => {
      const found = predicate();
      if (!found && expanding) {expanded = true; expanding = false; return predicate();}
      return found;
    }, sleep: async () => {},
    pageSnapshot: () => 'ref\ttextbox\tWrite a comment\t[redacted]', pageTarget: () => { focus = editor; }
  };
  const window = {__fb: F};
  vm.runInNewContext(source, {window, document, location: {href: 'https://www.facebook.com/a/posts/1'}, NodeFilter: {SHOW_TEXT: 4}, URL, innerHeight: 600});
  return {api: window.__blindfbPost, article, makeEditor, remove: e => {e.isConnected = false;}, set comments(value) {comments = value;}, get paused() {return paused;}};
}

(async () => {
  const slow = fixture('slow');
  assert.equal(JSON.parse(await slow.api.initialize('https://www.facebook.com/a/posts/1')).text, 'Entire post text');
  const shared = fixture('shared');
  assert.equal(JSON.parse(await shared.api.initialize('https://www.facebook.com/a/posts/1')).text, 'Poster text\nShared original');
  const ambiguous = fixture('ambiguous');
  await assert.rejects(ambiguous.api.initialize('https://www.facebook.com/a/posts/1'), /More than one post/);
  const reel = fixture('reel');
  const reeled = JSON.parse(await reel.api.initialize('https://www.facebook.com/reel/123/'));
  assert.equal(reeled.text, 'Entire reel caption');
  assert.equal(reeled.author, 'Reel author');
  assert.equal(reeled.video, 'https://www.facebook.com/reel/123/');
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
  parent.actions = [{closest: () => parent, getAttribute: () => 'Reply', click() {}}];
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  assert.equal(await f.api.prepare('1', 'hello'), '"ref"');
  box.innerText = 'hello';
  assert.equal(f.api.check(), true);
  box.innerText = '';
  const wrong = f.article('2'); wrong.lines[2] = 'hello';
  await assert.rejects(f.api.confirm(), /not confirmed in the selected thread/);

  await f.api.read();
  await f.api.prepare('1', 'hello'); box.innerText = 'hello'; f.api.check(); box.innerText = '';
  const right = f.article('3', parent); right.lines[2] = 'hello';
  assert.equal(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '3').parent_id, '1');
  console.log('Reader safety fixtures passed: missing, stale, changed, ambiguous, no root fallback, exact-thread confirmation.');
})().catch(error => { console.error(error); process.exitCode = 1; });
