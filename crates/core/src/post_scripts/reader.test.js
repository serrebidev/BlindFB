// Dependency-free DOM fixtures; never connects to Facebook or submits anything.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
// labels.js is injected before reader.js, as the Rust side does.
const source = ['labels.js', 'questions.js', 'reader.js'].map(file => fs.readFileSync(__dirname + '/' + file, 'utf8')).join('\n');
const W = require('./labels.js');
const ARTICLE = '[role=article][aria-label]';
const EDITOR = '[role=textbox][contenteditable=true],textarea';

function fixture(mode = false) {
  const watch = mode === true || mode === 'watch-fr', isReel = mode === 'reel' || mode === 'reel-count' || mode === 'reel-fr' || mode === 'untitled';
  const french = mode === 'reel-fr' || mode === 'watch-fr';
  let comments = [], focus, editor;
  const boxes = [];
  let opened = !watch, paused = false;
  let expanded = false;
  let expanding = false;
  const video = {muted: false, querySelectorAll: () => [], pause() { paused = true; }, getBoundingClientRect: () => ({height: 100, top: 0})};
  const expand = {getAttribute: () => french ? expanded ? 'Voir moins' : 'En voir plus' : expanded ? 'See less' : 'See more', isConnected: true,
    closest: () => mode === 'slow' ? {} : null,
    click() {if (mode === 'slow') expanding = true; else expanded = true;}};
  const author = {innerText: 'Reel author', href: 'https://www.facebook.com/author', closest: () => null, getAttribute: () => ''};
  const reelCard = {isConnected: true, closest: () => null, contains: () => true, querySelectorAll: selector => selector === 'a[href]' ? [author] : selector === '[role=button][aria-label]' ? [openComments] : selector === '[role=button],button' ? [expand] : []};
  video.parentElement = isReel ? reelCard : null;
  const openComments = {closest: () => null, getAttribute: () => mode === 'reel-fr' ? 'Commenter, 1,9 K commentaires' : mode === 'watch-fr' ? 'Laissez un commentaire' : mode === 'reel-count' ? 'Comment, 6 comments' : isReel ? 'Comment' : 'Leave a comment', click() {opened = true;}};
  const cards = [{contains: () => false}, {contains: () => false}];
  // A YouTube clip shared into a post: Facebook wraps it in l.facebook.com/l.php?u=.
  const sharedVideo = {href: 'https://l.facebook.com/l.php?u=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3Dabc&h=AT0', getAttribute: () => '', closest: () => null, innerText: 'A video'};
  // A link-preview card: Facebook gives it a bare "?__cft__" href until it is focused.
  let cardHref = '?__cft__[0]=x';
  const linkCard = {get href() { return new URL(cardHref, 'https://www.facebook.com/a/posts/1').href; }, innerText: 'LOFT – Free Vintage Reverb Plugin',
    getAttribute: name => name === 'href' ? cardHref : '', closest: () => null,
    focus() { cardHref = 'https://13x.audio/products/loft?fbclid=abc'; }, blur() {}};
  const messages = mode === 'shared' || mode === 'shared-source' || mode === 'ambiguous' ? ['Poster text', 'Shared original'].map(text => ({text, closest: () => null, contains: () => false})) : [{closest: () => null, contains: () => false}];
  if (mode === 'shared') messages.push({text: 'Hidden template', visible: false, closest: () => null, contains: () => false});
  let repliesShown = false;
  const viewReplies = {innerText: '', isConnected: true, closest: () => null,
    getAttribute: () => repliesShown ? 'Hide replies' : 'View 1 reply',
    click() { if (!repliesShown) { repliesShown = true; const reply = article('2', comments[0]); reply.lines[2] = 'A hidden reply'; } }};
  let moreShown = false;
  const viewMore = {innerText: '', isConnected: true, closest: () => null,
    getAttribute: () => moreShown ? '' : 'Zobrazit další komentáře',
    click() { if (!moreShown) { moreShown = true; const extra = article('2'); extra.lines[2] = 'A later comment'; } }};
  const reactionButton = {closest: () => null, getAttribute: name => name === 'aria-label' ? "Remove Love from Post author's post" : name === 'aria-pressed' ? 'true' : ''};
  const root = {isConnected: true, getAttribute: () => mode === 'ambiguous' ? 'main' : 'dialog', contains: e => e === root || e === list || comments.includes(e) || cards.includes(e) || boxes.includes(e),    querySelector: selector => selector.includes('profile_name') ? profile : isReel && selector.includes('contenteditable') ? {} : null,
    querySelectorAll: selector => selector === '[data-ad-rendering-role=like_button]' ? [reactionButton] : selector === ARTICLE ? comments : selector === 'a[href]' && mode === 'external-video' ? [sharedVideo] : selector === 'a[href]' && mode === 'card' ? [linkCard] : selector === '[data-ad-rendering-role=profile_name]' && mode === 'shared-source' ? profiles : selector === '[role=button],button' && mode === 'slow' ? [expand] : selector === '[role=button],button' && mode === 'replies' ? [viewReplies] : selector === '[role=button],button' && mode === 'more' ? [viewMore] : selector.includes('story_message') ? watch ? [] : messages : selector.includes('text-align') && watch ? [{closest: () => null, contains: () => false}] : selector === EDITOR ? boxes : []};
  const profile = {innerText: 'Post author', closest: () => null, parentElement: {parentElement: {parentElement: {querySelectorAll: () => []}}}};
  let revealed = false;
  const sourceLink = {get href() {return revealed ? 'https://www.facebook.com/bistro/posts/2?__cft__[0]=tracking' : 'https://www.facebook.com/a/posts/1?__cft__[0]=tracking';},
    getAttribute: () => revealed ? '/bistro/posts/2' : '?__cft__[0]=tracking', closest: () => null,
    focus() {revealed = true;}, blur() {}};
  const headerLink = (href, comment = false) => ({href, getAttribute: () => href, closest: () => comment ? {} : null});
  profile.parentElement.parentElement.parentElement.querySelectorAll = () => [headerLink('https://www.facebook.com/a/posts/1')];
  const profiles = [profile, {innerText: 'Big Feast Bistro + Retail', closest: () => null,
    parentElement: {parentElement: {parentElement: {querySelectorAll: () => [sourceLink,
      headerLink('https://www.facebook.com/bistro'),
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
        const line = lines[at];
        // A line is plain text, or {text|alt, block} when it shares a text block with its neighbours.
        if (line.alt) return {nodeType: 1, tagName: 'IMG', alt: line.alt, closest: selector => selector === '[dir=auto]' ? line.block : article.closest(selector), getAttribute: () => ''};
        if (line.text !== undefined) return {nodeValue: line.text, parentElement: {closest: selector => selector === '[dir=auto]' ? line.block || null : line.hidden && selector.includes('[aria-hidden=true]') ? {} : article.closest(selector)}};
        return {nodeValue: line, parentElement: article === reelCard && at >= 2 ? {closest: () => true} : article};
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
    profilePath: href => {const m = /^\/([A-Za-z0-9.]{3,})\/?$/.exec(new URL(href).pathname); return m ? '/' + m[1] : null;},
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
  vm.runInNewContext(source, {window, document, location: {href: 'https://www.facebook.com/a/posts/1'}, NodeFilter: {SHOW_TEXT: 4, SHOW_ELEMENT: 1}, URL, innerHeight: 600,
    getSelection: () => ({selectAllChildren() {}, collapseToEnd() { collapsed = true; }})});
  return {api: window.__blindfbPost, article, makeEditor, replyBox, replyAction, remove: e => {e.isConnected = false;}, snapshot: value => { named = value; }, get focused() {return focus;}, get collapsed() {return collapsed;}, set comments(value) {comments = value;}, get paused() {return paused;}};
}

(async () => {
  const ownState = JSON.parse(await fixture().api.initialize('https://www.facebook.com/a/posts/1'));
  assert.equal(ownState.reaction_label, "Remove Love from Post author's post");
  assert.equal(ownState.reaction_pressed, true);
  const slow = fixture('slow');
  assert.equal(JSON.parse(await slow.api.initialize('https://www.facebook.com/a/posts/1')).text, 'Entire post text');
  const shared = fixture('shared');
  assert.equal(JSON.parse(await shared.api.initialize('https://www.facebook.com/a/posts/1')).text, 'Poster text\nShared original');
  const sourced = JSON.parse(await fixture('shared-source').api.initialize('https://www.facebook.com/a/posts/1'));
  // The source's name link also rides along, marked as a profile for the app to open.
  assert.deepEqual(sourced.links, [{label: 'Shared post by Big Feast Bistro + Retail', url: 'https://www.facebook.com/bistro/posts/2'},
    {label: 'Big Feast Bistro + Retail', url: 'https://www.facebook.com/bistro', profile: '/bistro'}]);
  // A comment's own links ride along with it: the redirect unwraps, the
  // author link and the comment permalink stay out.
  const linkedFixture = fixture();
  const linkedComment = linkedFixture.article('9');
  linkedComment.querySelectorAll = selector => selector === 'a[href]' ? [
    {closest: () => linkedComment, href: 'https://www.facebook.com/comment.author', getAttribute: () => '/comment.author', innerText: 'Comment author'},
    {closest: () => linkedComment, href: 'https://www.facebook.com/a/posts/1?comment_id=9', getAttribute: () => '/a/posts/1?comment_id=9', innerText: '1h'},
    {closest: () => linkedComment, href: 'https://l.facebook.com/l.php?u=https%3A%2F%2Fexample.com%2Fstory%3Fa%3D1&h=xyz', getAttribute: () => 'https://l.facebook.com/l.php?u=x', innerText: 'example.com/story'},
    {closest: () => linkedComment, href: 'https://othersite.org/page?fbclid=zz', getAttribute: () => 'https://othersite.org/page?fbclid=zz', innerText: 'Other Site'},
    // Another site's pages may use Facebook's UI shapes: they are real links.
    {closest: () => linkedComment, href: 'https://www.tiktok.com/hashtag/cats', getAttribute: () => 'https://www.tiktok.com/hashtag/cats', innerText: '#cats'},
    {closest: () => linkedComment, href: 'https://example.org/story?comment_id=42', getAttribute: () => 'https://example.org/story?comment_id=42', innerText: 'story'},
    {closest: () => linkedComment, href: 'https://www.facebook.com/hashtag/news', getAttribute: () => '/hashtag/news', innerText: '#news'},
  ] : [];
  linkedFixture.article('10');
  const linkedDetail = JSON.parse(await linkedFixture.api.initialize('https://www.facebook.com/a/posts/1'));
  assert.deepEqual(linkedDetail.comments.find(c => c.id === '9').links, [
    {label: 'example.com/story', url: 'https://example.com/story?a=1'},
    {label: 'Other Site', url: 'https://othersite.org/page'},
    {label: '#cats', url: 'https://www.tiktok.com/hashtag/cats'},
    {label: 'story', url: 'https://example.org/story?comment_id=42'},
  ]);
  assert.deepEqual(linkedDetail.comments.find(c => c.id === '10').links, []);
  const ambiguous = fixture('ambiguous');
  await assert.rejects(ambiguous.api.initialize('https://www.facebook.com/a/posts/1'), /More than one post/);
  const reel = fixture('reel');
  const reeled = JSON.parse(await reel.api.initialize('https://www.facebook.com/reel/123/'));
  assert.equal(reeled.text, 'Entire reel caption');
  assert.equal(reeled.author, 'Reel author');
  assert.equal(reeled.video, 'https://www.facebook.com/reel/123/');
  const counted = JSON.parse(await fixture('reel-count').api.initialize('https://www.facebook.com/reel/123/'));
  assert.equal(counted.text, 'Entire reel caption');
  // A French Facebook account: "Commenter, 1,9 K commentaires" and "En voir plus".
  const french = JSON.parse(await fixture('reel-fr').api.initialize('https://www.facebook.com/reel/123/'));
  assert.equal(french.text, 'Entire reel caption');
  assert.equal(french.author, 'Reel author');
  const watchedFr = JSON.parse(await fixture('watch-fr').api.initialize('https://www.facebook.com/page/videos/123/'));
  assert.equal(watchedFr.text, 'Entire post text');
  const untitled = fixture('untitled');
  assert.equal(JSON.parse(await untitled.api.initialize('https://www.facebook.com/reel/123/')).text, '');
  assert.equal(JSON.parse(await untitled.api.setTitle('Dedicated To Bart Simpson')).text, 'Dedicated To Bart Simpson');
  assert.equal(JSON.parse(await untitled.api.read()).text, 'Dedicated To Bart Simpson');
  assert.equal(JSON.parse(await reel.api.setTitle('Other title')).text, 'Entire reel caption');
  const sharedVideo = JSON.parse(await fixture('external-video').api.initialize('https://www.facebook.com/a/posts/1'));
  assert.equal(sharedVideo.video, 'https://www.youtube.com/watch?v=abc');
  assert.deepEqual(sharedVideo.links, [{label: 'A video', url: 'https://www.youtube.com/watch?v=abc'}]);
  const carded = JSON.parse(await fixture('card').api.initialize('https://www.facebook.com/a/posts/1'));
  assert.deepEqual(carded.links, [{label: 'LOFT – Free Vintage Reverb Plugin', url: 'https://13x.audio/products/loft'}]);
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

  // Recorded live: Facebook draws emoji as pictures, so they are read back from the alt text.
  const block = {};
  const emojiLines = [{text: 'Comment author'}, {text: ' · '}, {text: '1h'}, {text: 'Nice work ', block}, {alt: '\u{1F44D}', block}, {text: ' thanks', block}, {text: 'Like'}, {text: 'Reply'}];
  f = fixture(); parent = f.article('1');
  const emojiBox = f.replyBox('Reply as Me');
  f.makeEditor(null, 'Comment as Me');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  parent.actions = [f.replyAction(parent, emojiBox)];
  await f.api.prepare('1', 'Nice work \u{1F44D} thanks');
  const emojiReply = f.article('6', parent); emojiReply.lines = emojiLines;
  assert.equal(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '6').text, 'Nice work \u{1F44D} thanks');
  // Only an emoji: the comment still has text.
  const onlyEmoji = f.article('7'); onlyEmoji.lines = [{text: 'Comment author'}, {text: '1h'}, {alt: '\u{1F44D}', block}, {text: 'Like'}];
  // A picture that is not an emoji stays out.
  const pictured = f.article('8'); pictured.lines = [{text: 'Comment author'}, {text: '1h'}, {text: 'Look', block}, {alt: 'May be an image of a dog', block}];
  // Facebook's hidden full-date tooltip stays out of the text.
  const dated = f.article('10'); dated.lines = [{text: 'Comment author'}, {text: 'Thursday, November 12, 2026 at 3:48 PM', hidden: true}, {text: '1h'}, {text: 'Body', block}];
  const read = JSON.parse(await f.api.read()).comments;
  assert.equal(read.find(c => c.id === '10').text, 'Body');
  assert.equal(read.find(c => c.id === '10').time, '1h');
  assert.equal(read.find(c => c.id === '7').text, '\u{1F44D}');
  assert.equal(read.find(c => c.id === '8').text, 'Look');

  // Facebook can reshape what was typed; punctuation and emoji are not compared.
  f = fixture(); parent = f.article('1');
  const shaped = f.replyBox('Reply as Me');
  f.makeEditor(null, 'Comment as Me');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  parent.actions = [f.replyAction(parent, shaped)];
  await f.api.prepare('1', 'hi :) see https://example.com/a');
  const converted = f.article('9', parent); converted.lines = [{text: 'Comment author'}, {text: '1h'}, {text: 'hi ', block}, {alt: '\u{1F642}', block}, {text: ' see example.com/a', block}];
  assert.equal(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '9').parent_id, '1');
  // A long comment is cut off behind See more; its start is enough.
  await f.api.read();
  const long = 'a long reply '.repeat(20);
  await f.api.prepare('1', long);
  const cut = f.article('10', parent); cut.lines[2] = 'a long reply a long reply a long reply a long reply… See more';
  assert.ok(JSON.parse(await f.api.confirm()).comments.find(c => c.id === '10'));
  // Different words are still not this comment.
  await f.api.read();
  await f.api.prepare('1', 'hello');
  const other = f.article('11', parent); other.lines[2] = 'somebody else';
  await assert.rejects(f.api.confirm(), /not confirmed in the selected thread/);

  // Recorded live: while "Posting..." the permalink holds a temporary client id, not yet accepted.
  f = fixture();
  f.makeEditor(null, 'Comment as Me');
  await f.api.initialize('https://www.facebook.com/a/posts/1');
  await f.api.prepare(null, 'hello');
  const posting = f.article('client:483'); posting.lines = ['Comment author', 'hello', 'Posting...'];
  await assert.rejects(f.api.confirm(), /not confirmed in the selected thread/);

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
  // French words from docs/fr-labels.md.
  assert.equal(W.reelReaction('J’aime, 28 réactions', null), 'none');
  assert.equal(W.reelReaction('J’aime, 28 réactions', 'true'), 'like');
  assert.equal(W.reelReaction('Retirer J’adore, 29 réactions', null), 'love');
  assert.equal(W.reelReaction('J’adore, 29 réactions', null), 'love');
  assert.equal(W.reelReaction('Like', null), 'none');
  assert.equal(W.reelReaction('Remove Haha', null), 'haha');
  assert.equal(W.reelReaction('Réagir', null), null);
  assert.equal(W.reelReaction('Commenter, 2 commentaires', null), null);
  for (const text of ['Commenter', 'Commenter, 2 commentaires', 'Commenter, 1,9 K commentaires', 'Comment, 1.2K comments']) assert.ok(W.comment.test(text), text);
  for (const text of ['Partager la publication de X', 'Partager, 4 partages', "Share X's post, 18 shares"]) assert.ok(W.share.test(text), text);
  for (const text of ['1,9 K', '40,1 K', '4 890', '4\u202f890', 'Compte vérifié', 'Vous suivez JECK', 'Suivre', '·']) assert.ok(W.captionNoise.test(text), text);
  for (const text of ['il y a 10 heures', '8 h', '1 j', '1 sem.', '2 ans', 'À l’instant', 'Hier à 14:05']) assert.ok(W.time.test(text), text);
  for (const text of ['J’aime', "J'aime", 'Répondre', 'Modifié', 'Voir la traduction']) assert.ok(W.commentAction.test(text), text);
  assert.ok(W.nextReel.test('Carte suivante'));
  assert.ok(W.seeMore.test('En voir plus'));
  // "See more" in a language labels.js does not know: the button after text cut with "…".
  assert.ok(W.cutShort({parentElement: {innerText: 'Я проверил информацию… Ещё'}}, 'Ещё'));
  assert.ok(!W.cutShort({parentElement: {innerText: 'Done. Ещё'}}, 'Ещё'));
  {
    // One line split over inline spans (a translated phrase, a mention) reads as one line;
    // a new block or a <br> starts the next. Emoji pictures stay with their words.
    const el = (display, parent = null) => ({display, parentElement: parent, closest: () => null});
    const scope = el('block'), p1 = el('block', scope), span = el('inline', p1), p2 = el('block', scope);
    const nodes = [{nodeValue: 'dates (with', parentElement: p1}, {nodeValue: 'out the wat', parentElement: span}, {nodeValue: 'erfall):', parentElement: p1},
      {nodeType: 1, tagName: 'IMG', alt: '😀', ...el('inline', p1)}, {nodeType: 1, tagName: 'BR'}, {nodeValue: 'next line', parentElement: p1}, {nodeValue: 'Second', parentElement: p2}];
    const context = {document: {createTreeWalker: () => ({nextNode: () => nodes.shift() || null})}, NodeFilter: {SHOW_TEXT: 4, SHOW_ELEMENT: 1}, getComputedStyle: e => ({display: e.display}), window: {}};
    vm.runInNewContext(fs.readFileSync(__dirname + '/labels.js', 'utf8'), context);
    assert.deepEqual([...context.window.__blindfbLabels.textLines(scope, () => true)], ['dates (without the waterfall):😀', 'next line', 'Second']);
  }
  assert.ok(W.manyReels.test('JECK a publié 3 nouveaux reels.'));
  assert.equal(('Partagé avec : Public'.match(W.sharingWith) || [])[1], 'Public');
  // Issue #22: replies hidden behind "View replies" load automatically on read.
  const collapsed = fixture('replies');
  collapsed.article('1');
  const collapsedDetail = JSON.parse(await collapsed.api.initialize('https://www.facebook.com/a/posts/1'));
  assert.equal(collapsedDetail.comments.length, 2);
  assert.equal(collapsedDetail.comments.find(c => c.id === '2').parent_id, '1');
  assert.equal(collapsedDetail.comments.find(c => c.id === '2').text, 'A hidden reply');
  for (const text of ['View replies', 'View 1 reply', 'View 12 replies', 'View more replies', 'Afficher les réponses', 'Voir 2 réponses']) assert.ok(W.viewReplies.test(text), text);
  for (const text of ['Hide replies', 'View comment', 'See more']) assert.ok(!W.viewReplies.test(text), text);
  // Issue #30: Michal's Czech/Slovak Facebook never matched viewReplies, so replies
  // stayed hidden for him even after issue #22. Every count declension must match,
  // and the Hide forms must not (they would collapse replies again).
  for (const text of ['Zobrazit odpovědi', 'Zobrazit 1 odpověď', 'Zobrazit 3 odpovědi', 'Zobrazit 12 odpovědí', 'Zobrazit další odpovědi', 'Zobrazit více odpovědí', 'Zobrazit předchozí odpovědi', 'Zobraziť odpovede', 'Zobraziť 1 odpoveď', 'Zobraziť 4 odpovede', 'Zobraziť 12 odpovedí', 'Zobraziť ďalšie odpovede', 'Zobraziť viac odpovedí', 'Zobraziť predchádzajúce odpovede']) assert.ok(W.viewReplies.test(text), text);
  for (const text of ['Skrýt odpovědi', 'Skryť odpovede', 'Zobrazit komentáře', 'Odpovědět', 'Odpovedať']) assert.ok(!W.viewReplies.test(text), text);
  for (const text of ['View more comments', 'View previous comments', 'View 10 comments', 'View 5 more comments', 'Voir plus de commentaires', 'Afficher plus de commentaires', 'Xem thêm bình luận', 'Zobrazit další komentáře', 'Zobrazit 7 komentářů', 'Zobrazit 1 komentář', 'Zobraziť ďalšie komentáre', 'Zobraziť 7 komentárov']) assert.ok(W.viewMoreComments.test(text), text);
  for (const text of ['Hide comments', 'Most relevant', 'Newest comments', 'View replies', 'Zobrazit odpovědi']) assert.ok(!W.viewMoreComments.test(text), text);
  // Issue #30: later top-level comments load on read too, in Michal's language.
  const more = fixture('more');
  more.article('1');
  const moreDetail = JSON.parse(await more.api.initialize('https://www.facebook.com/a/posts/1'));
  assert.equal(moreDetail.comments.length, 2);
  assert.equal(moreDetail.comments.find(c => c.id === '2').text, 'A later comment');
  assert.equal(moreDetail.comments.find(c => c.id === '2').parent_id, null);
  console.log('French Facebook fixtures passed: reel and video comments, reactions, counts, times, badges.');
  console.log('Reader safety fixtures passed: missing, stale, changed, ambiguous, no root fallback, exact-thread confirmation, reply boxes beside comments, prefilled names.');
})().catch(error => { console.error(error); process.exitCode = 1; });
