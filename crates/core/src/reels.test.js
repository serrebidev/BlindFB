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
