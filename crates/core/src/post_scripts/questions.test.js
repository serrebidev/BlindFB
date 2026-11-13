// Dependency-free fixtures for the captured Participation review DOM; no Facebook writes.
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(__dirname + '/questions.js', 'utf8');

function fixture() {
  let submitted = 0, cancelled = 0, enabled = true, closes = true;
  const node = (attrs = {}, extra = {}) => ({
    isConnected: true, visible: true, innerText: '', labels: [],
    getAttribute: key => attrs[key] || '', closest: () => null, ...extra,
  });
  const root = node({}, {contains: () => false});
  const d = node({}, {innerText: 'Participation review\nDo you live in Hope?\nGroup rules: Be respectful.'});
  const section = node({}, {parentElement: d, lines: ['Do you live in Hope?'], contains: e => radios.includes(e)});
  const radios = ['yes', 'no'].map(value => node({'aria-labelledby': value}, {
    type: 'radio', name: 'radio', checked: false, parentElement: section,
    closest: () => section, click() { radios.forEach(e => e.checked = e === this); },
  }));
  const text = node({'aria-label': 'Is your content about Hope?'}, {
    type: 'textarea', tagName: 'TEXTAREA', parentElement: d, value: '',
    dispatchEvent() {},
  });
  const check = node({'aria-label': 'I agree to the group rules'}, {
    type: 'checkbox', checked: false, parentElement: d, click() {this.checked = !this.checked;},
  });
  const submit = node({'aria-label': 'Submit'}, {click() {submitted++; if (closes) d.isConnected = false;}});
  const cancel = node({'aria-label': 'Close'}, {click() {cancelled++; d.isConnected = false;}});
  const fields = [text, ...radios, check]; // Deliberately put a text question before a choice question.
  d.contains = e => fields.includes(e);
  d.querySelector = selector => selector.includes('input[type=radio]') ? text : null;
  d.querySelectorAll = selector => selector === '[role=button],button' ? [submit, cancel] : fields;
  const document = {
    querySelectorAll: () => [root, d],
    getElementById: id => ({textContent: id}),
    createTreeWalker: p => {
      const nodes = (p.lines || []).map(value => ({nodeValue: value, parentElement: node()}));
      return {nextNode: () => nodes.shift()};
    },
  };
  const F = {clean: s => s.trim(), visible: e => e.visible && e.isConnected, enabled: () => enabled, waitFor: async fn => fn()};
  class Input {set value(value) {this.valueWritten = value;}}
  const window = {__fb: F};
  vm.runInNewContext(source, {window, document, NodeFilter: {SHOW_TEXT: 4}, HTMLInputElement: Input, HTMLTextAreaElement: Input, Event: class {}});
  return {api: window.__blindfbQuestions, root, d, radios, text, check, fields,
    get submitted() {return submitted;}, get cancelled() {return cancelled;},
    disable() {enabled = false;}, keepOpen() {closes = false;}};
}

(async () => {
  let f = fixture();
  assert.equal(f.api.dialog(f.root), f.d);
  let form = JSON.parse(f.api.read(f.d)).participation;
  assert.deepEqual(form.fields.map(x => x.kind), ['text', 'choice', 'check']);
  assert.equal(form.fields[1].prompt, 'Do you live in Hope?');
  assert.deepEqual(form.fields[1].choices, ['yes', 'no']);
  assert.match(form.text, /Be respectful/);
  assert.equal(f.check.checked, false);
  await assert.rejects(f.api.submit(form.token, ['Hope', 99, true]), /Answer every/);
  assert.equal(f.text.valueWritten, undefined); // Whole request validates before any edits.
  assert.equal(f.submitted, 0);
  await assert.rejects(f.api.submit(form.token, [' ', 0, true]), /Answer every/);
  await assert.rejects(f.api.submit('stale', ['Hope', 0, true]), /changed/);
  assert.equal(await f.api.submit(form.token, ['I am from Agassiz and will discuss Hope.', 1, true]), 'submitted');
  assert.equal(f.text.valueWritten, 'I am from Agassiz and will discuss Hope.');
  assert.equal(f.radios[1].checked, true);
  assert.equal(f.check.checked, true);
  assert.equal(f.submitted, 1);
  await assert.rejects(f.api.submit(form.token, ['Hope', 0, true]), /changed/);
  assert.equal(f.submitted, 1);

  f = fixture();
  form = JSON.parse(f.api.read(f.d)).participation;
  assert.equal(f.api.cancel(form.token), 'cancelled');
  assert.equal(f.cancelled, 1);
  assert.equal(f.submitted, 0);
  assert.equal(f.text.valueWritten, undefined);

  f = fixture();
  // Facebook labels the enclosing span, while the textarea's wrapping label only says
  // "Write an answer...". Read the real question instead of that placeholder.
  f.text.getAttribute = () => '';
  f.text.labels = [{innerText: 'Write an answer...'}];
  f.text.parentElement = {getAttribute: key => key === 'aria-label' ? 'Specific to Hope?' : '', parentElement: f.d};
  assert.equal(JSON.parse(f.api.read(f.d)).participation.fields[0].prompt, 'Specific to Hope?');

  f = fixture();
  form = JSON.parse(f.api.read(f.d)).participation;
  f.text.isConnected = false;
  await assert.rejects(f.api.submit(form.token, ['Hope', 0, true]), /changed/);
  assert.equal(f.submitted, 0);

  f = fixture();
  form = JSON.parse(f.api.read(f.d)).participation;
  f.radios[0].getAttribute = () => 'Changed answer';
  await assert.rejects(f.api.submit(form.token, ['Hope', 0, true]), /changed/);
  assert.equal(f.submitted, 0);

  f = fixture();
  form = JSON.parse(f.api.read(f.d)).participation;
  f.disable();
  const retry = JSON.parse(await f.api.submit(form.token, ['Hope', 0, false])).participation;
  assert.match(retry.error, /not enabled Submit/);
  assert.equal(retry.token, form.token);
  assert.deepEqual(retry.answers, ['Hope', 0, false]);
  assert.equal(f.submitted, 0);
  assert.equal(f.check.checked, false); // Never agrees on the user's behalf.

  f = fixture();
  form = JSON.parse(f.api.read(f.d)).participation;
  f.keepOpen();
  await assert.rejects(f.api.submit(form.token, ['Hope', 0, true]), /did not confirm/);
  await assert.rejects(f.api.submit(form.token, ['Hope', 0, true]), /changed/);
  assert.equal(f.submitted, 1);
  console.log('Participation fixtures passed: DOM order, native choices, rules, validation, stale forms, cancel, single submission.');
})().catch(error => {console.error(error); process.exitCode = 1;});
