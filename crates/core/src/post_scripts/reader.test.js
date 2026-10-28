// Dependency-free DOM fixtures; never connects to Facebook or submits anything.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/reader.js', 'utf8');
const ARTICLE = '[role=article][aria-label]';
const EDITOR = '[role=textbox][contenteditable=true],textarea';

function fixture() {
  let comments = [], focus, editor;
  const root = {isConnected: true, contains: e => comments.includes(e), querySelector: () => profile,
    querySelectorAll: selector => selector === ARTICLE ? comments : selector.includes('story_message') ? [{closest: () => null}] : selector === EDITOR && editor ? [editor] : []};
  const profile = {innerText: 'Post author', parentElement: {parentElement: {parentElement: {}}}};
  const document = {
    querySelectorAll: () => [], querySelector: () => ({closest: () => root}),
    createTreeWalker(article) { let index = 0; return {nextNode: () => index < article.lines.length ? {nodeValue: article.lines[index++], parentElement: article} : null}; },
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
    clean: text => text.trim(), TIME: /^\d+h$/, visible: () => true, enabled: () => true,
    vtext: () => ['Post author', '1h'], postText: () => 'Entire post text', waitFor: async predicate => predicate(),
    pageSnapshot: () => 'ref\ttextbox\tWrite a comment\t[redacted]', pageTarget: () => { focus = editor; }
  };
  const window = {__fb: F};
  vm.runInNewContext(source, {window, document, location: {href: 'https://www.facebook.com/a/posts/1'}, NodeFilter: {SHOW_TEXT: 4}, URL});
  return {api: window.__blindfbPost, article, makeEditor, remove: e => {e.isConnected = false;}, set comments(value) {comments = value;}};
}

(async () => {
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
