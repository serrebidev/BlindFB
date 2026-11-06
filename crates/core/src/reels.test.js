const assert = require('node:assert/strict');
const {notification, stories} = require('./reels.js');
const target = 'https://www.facebook.com/reel/111/?notif_id=555';
const note = {
  notif_type: 'watch_follower_video',
  tracking: JSON.stringify({alert_id: 555, content_id: 111,
    context_id: '444', dedup_key: '0|111,222,333|444'}),
  body: {ranges: [{entity: {name: 'The Verge', url: 'https://www.facebook.com/verge'}}]},
};
assert.deepEqual(notification([{nested: [note]}], target), {
  urls: ['111', '222', '333'].map(id => 'https://www.facebook.com/reel/' + id + '/'),
  account: 'https://www.facebook.com/profile.php?id=444&sk=reels_tab',
});
assert.equal(notification([{...note, tracking: note.tracking.replace('555', '556')}], target), null);
assert.equal(notification([{...note, tracking: note.tracking.replace('111,', 'evil,')}], target), null);
assert.equal(notification([note], target.replace('www.facebook.com', 'evil.example')), null);
const story = {__typename: 'Story', actors: [{name: 'The Verge'}], message: {text: 'Complete caption\nSecond paragraph'},
  attachments: [{media: {shareable_url: 'https://www.facebook.com/reel/123/?tracking=1'}}]};
assert.deepEqual(stories([{nested: story}]), [{url: 'https://www.facebook.com/reel/123/', author: 'The Verge', text: 'Complete caption\nSecond paragraph'}]);
const hydrated = {nested: story, get inaccessible() { throw Error('Do not evaluate runtime getters'); }};
assert.equal(stories([hydrated]).length, 1);
assert.deepEqual(stories([{...story, attachments: [{media: {shareable_url: 'https://facebook.com.evil/reel/123/'}}]}]), []);
console.log('Reel fixtures passed: exact notification identity, three IDs, account fallback, full captions, trusted URLs.');

// The live viewer includes its comment count in the button's accessible name.
async function recommendedFixture(commentLabel) {
  const vm = require('node:vm'), fs = require('node:fs');
  const comment = {getAttribute: () => commentLabel};
  const author = {innerText: 'Reel author'};
  const card = {querySelectorAll: selector => selector === 'a[href]' ? [author] : [comment]};
  const video = {parentElement: card, getBoundingClientRect: () => ({height: 100, top: 0}), querySelectorAll: () => [], pause() {}};
  const document = {
    querySelectorAll: selector => selector === 'video' ? [video] : [],
    createTreeWalker: () => ({nextNode: () => null}),
  };
  const window = {__fb: {clean: text => text.trim(), visible: () => true,
    waitFor: async predicate => predicate(), sleep: async () => {}}};
  // labels.js is injected before reels.js, as the Rust side does.
  vm.runInNewContext(fs.readFileSync(__dirname + '/post_scripts/labels.js', 'utf8') + fs.readFileSync(__dirname + '/reels.js', 'utf8'),
    {window, document, location: {href: 'https://www.facebook.com/reel/123/'}, URL, innerHeight: 800, NodeFilter: {SHOW_TEXT: 4}});
  window.__blindfbReels.initialize('fixture', 'recommended');
  const first = JSON.parse(await window.__blindfbReels.more('fixture', 1));
  assert.equal(first.rows.length, 1, commentLabel);
  assert.equal(first.rows[0].url, 'https://www.facebook.com/reel/123/');
  assert.equal(first.rows[0].author, 'Reel author');
  assert.deepEqual(JSON.parse(await window.__blindfbReels.more('fixture', 1)).rows, []);
}
(async () => {
  for (const label of ['Comment', 'Comment, 1 comment', 'Comment, 6 comments', 'Comment, 1.2K comments',
    'Commenter', 'Commenter, 1 commentaire', 'Commenter, 2 commentaires', 'Commenter, 1,9 K commentaires']) {
    await recommendedFixture(label);
  }
  console.log('Recommended reels fixtures passed: plain and counted comment buttons in English and French, no duplicate reels.');
})().catch(error => {console.error(error); process.exitCode = 1;});
