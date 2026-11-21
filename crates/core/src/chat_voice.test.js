const assert = require('node:assert/strict');
const {Tail} = require('./chat_voice.js');

const row = (id, voice = false, mine = false) => ({id, voice, mine});
const tail = new Tail();
assert.deepEqual(tail.update([row('old', true), row('anchor')]), {added: [], play: []});
assert.deepEqual(tail.update([row('history', true), row('old', true), row('anchor')]), {added: [], play: []});
assert.deepEqual(tail.update([row('anchor'), row('new'), row('own', true, true), row('newer', true)]), {
    added: [row('new'), row('own', true, true), row('newer', true)], play: ['newer'],
});
assert.deepEqual(tail.update([row('new', true), row('own', true, true), row('newer', true)]), {
    added: [row('new', true)], play: ['new'],
});
assert.deepEqual(tail.update([row('new', true), row('own', true, true), row('newer', true)]), {added: [], play: []});
assert.deepEqual(tail.update([row('unrelated', true)]), {added: [], play: []});
assert.deepEqual(tail.update([row('unrelated', true), row('after-gap', true)]), {
    added: [row('after-gap', true)], play: ['after-gap'],
});
