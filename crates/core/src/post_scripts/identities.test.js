// Dependency-free identity fixtures. No Facebook connection or publishing.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

class Element {
  constructor(tag = 'div', attributes = {}, text = '') {
    this.tagName = tag.toUpperCase(); this.attributes = attributes; this.text = text;
    this.children = []; this.isConnected = true; this.value = ''; this.checked = false;
    this.type = attributes.type || ''; this.name = attributes.name || '';
  }
  get innerText() { return [this.text, ...this.children.map(e => e.innerText)].filter(Boolean).join('\n'); }
  get textContent() { return this.innerText; }
  get labels() { return []; }
  getAttribute(name) { return this.attributes[name] ?? null; }
  hasAttribute(name) { return name in this.attributes; }
  matches(selector) {
    return selector.split(',').some(part => {
      part = part.trim();
      const tag = part.match(/^[a-z]+/i)?.[0];
      if (tag && tag.toUpperCase() !== this.tagName) return false;
      return [...part.matchAll(/\[([^\]=\s]+)(?:=['"]?([^\]'"\s]+)['"]?)?\]/g)].every(([, name, value]) => value === undefined ? this.hasAttribute(name) : this.getAttribute(name) === value);
    });
  }
  append(...elements) { for (const e of elements) { e.parentElement = this; this.children.push(e); } }
  contains(element) { return this === element || this.children.some(e => e.contains(element)); }
  querySelectorAll(selector) { return this.children.flatMap(e => [...(e.matches(selector) ? [e] : []), ...e.querySelectorAll(selector)]); }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
  closest(selector) { return this.matches(selector) ? this : this.parentElement?.closest(selector) || null; }
  click() { this.onClick?.(); }
  focus() { this.document.activeElement = this; }
  dispatchEvent(event) { this.onEvent?.(event); return true; }
  remove() { this.isConnected = false; if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(e => e !== this); }
}

function fixture(group = true, page = {}) {
  const body = new Element(), root = new Element('div', {role: 'dialog'});
  const shell = new Element(), editor = new Element('div', {role: 'textbox', contenteditable: 'true', 'aria-label': 'Comment as Test Person'});
  const button = new Element('div', {role: 'button', 'aria-haspopup': 'menu', 'aria-label': 'Comment as Test Person'});
  button.append(new Element('img', {alt: 'Test Person'}));
  shell.append(button, editor); root.append(shell); body.append(root);
  const document = {body, activeElement: editor, querySelectorAll: s => body.querySelectorAll(s), getElementById: () => null};
  const attach = element => { element.document = document; element.children.forEach(attach); };
  attach(body);
  let menu, selected = 'Test Person', clicks = 0, readCount = 0;
  const visible = e => e && e.isConnected && !e.getAttribute('hidden') && (!e.parentElement || visible(e.parentElement));
  const F = {clean: s => String(s || '').trim(), visible, enabled: e => e.getAttribute('aria-disabled') !== 'true' && !e.hasAttribute('disabled'),
    waitFor: async fn => { for (let i = 0; i < 20; i++) { const value = fn(); if (value) return value; await Promise.resolve(); } return null; }, press: e => e.click()};
  const makeMenu = () => {
    menu = new Element('div', {role: 'menu'});
    for (const name of ['Test Person', 'Anonymous participant', 'Blue Owl']) {
      const option = new Element('div', {role: 'menuitemradio', 'aria-checked': String(name === selected)}, name);
      option.onClick = () => { clicks++; selected = name; button.attributes['aria-label'] = 'Comment as ' + name; editor.attributes['aria-label'] = 'Comment as ' + name; button.children[0].attributes.alt = name; menu.remove(); };
      menu.append(option);
    }
    body.append(menu); attach(menu); return menu;
  };
  button.onClick = () => { if (menu && visible(menu)) menu.remove(); else makeMenu(); };
  const context = {...page, window: {__fb: F}, document, URL, Event: class {constructor(type) {this.type = type;}}, KeyboardEvent: class {}, HTMLInputElement: Element, HTMLTextAreaElement: Element};
  vm.runInNewContext(fs.readFileSync(__dirname + '/identities.js', 'utf8'), context);
  const api = context.window.__blindfbIdentities.create({current: () => root, editorFor: () => editor,
    target: () => 'https://www.facebook.com/' + (group ? 'groups/123/posts/456/' : 'person/posts/456/'),
    read: async () => { readCount++; return JSON.stringify({url: 'post', comments: [], comment_identity: api.state(editor)}); }});
  return {api, body, root, shell, editor, button, document, visible, makeMenu, get menu() {return menu;}, get clicks() {return clicks;}, get readCount() {return readCount;}, Element, attach};
}

(async () => {
  const f = fixture();
  assert.equal(f.api.state(f.editor).label, 'Test Person');
  assert.equal(f.api.state(f.editor).can_change, true);
  const panel = JSON.parse(await f.api.open(null)).identity;
  assert.equal(panel.options.length, 3);
  assert.equal(panel.options.find(o => o.selected).label, 'Test Person');
  assert.equal(f.clicks, 0, 'reading choices never changes identity');
  const anonymous = panel.options.find(o => o.label === 'Anonymous participant');
  const updated = JSON.parse(await f.api.act(panel.token, anonymous.key, []));
  assert.equal(updated.comment_identity.label, 'Anonymous participant');
  assert.equal(f.clicks, 1);
  await assert.rejects(f.api.act(panel.token, anonymous.key, []), /changed|expired|stale/i);

  const snapshot = f.api.capture(f.editor, 'Anonymous participant');
  assert.equal(f.api.check(f.editor, snapshot), true);
  f.button.attributes['aria-label'] = 'Comment as Test Person';
  f.editor.attributes['aria-label'] = 'Comment as Test Person';
  f.button.children[0].attributes.alt = 'Test Person';
  assert.throws(() => f.api.check(f.editor, snapshot), /identity.*changed/i);
  assert.throws(() => f.api.capture(f.editor, 'Blue Owl'), /identity.*changed/i);
  f.editor.attributes['aria-label'] = 'Reply as Test Person';
  assert.equal(f.api.capture(f.editor, 'Test Person').label, 'Test Person', 'reply and top-level labels describe the same identity');

  const cancel = fixture(); cancel.editor.value = 'unsent draft';
  const opened = JSON.parse(await cancel.api.open(null)).identity;
  await cancel.api.cancel(opened.token);
  assert.equal(cancel.clicks, 0); assert.equal(cancel.editor.value, 'unsent draft');
  assert.equal(cancel.visible(cancel.menu), false);

  const stale = fixture(), stalePanel = JSON.parse(await stale.api.open(null)).identity;
  stale.menu.children[1].text = 'Different option';
  await assert.rejects(stale.api.act(stalePanel.token, stalePanel.options[1].key, []), /changed|expired|stale/i);
  assert.equal(stale.clicks, 0);

  const disabled = fixture();
  disabled.button.attributes['aria-disabled'] = 'true';
  assert.equal(disabled.api.state(disabled.editor).can_change, false);
  await assert.rejects(disabled.api.open(null), /available|allow|locked/i);
  assert.equal(fixture(false).api.state(), null, 'non-group readers retain normal behavior');

  const live = fixture();
  live.button.attributes['aria-label'] = 'Comment as, choose profile';
  assert.equal(live.api.state().label, 'Test Person', 'current desktop selector has a constant button label; identity comes from the editor');
  live.button.attributes['aria-label'] = 'Switch to a different profile or Page';
  assert.equal(live.api.state().can_change, false, 'global profiles menu is not the group identity selector');

  // Translated switcher: no known wording, no fiber key on the node. Found from the React root
  // through Facebook's anonymous actor menu entrypoint (live DOM shape, 2026-11-17).
  const entry = {getPreloadProps() {}}, mount = {};
  const translated = fixture(true, {require: name => { if (name === 'GroupsCometUFIAnonActorSwitcherMenu.entrypoint') return entry; throw Error('no module'); }});
  translated.button.attributes['aria-label'] = 'Profilauswahl';
  translated.editor.attributes['aria-label'] = 'Schreib einen Kommentar';
  const roots = translated.document.querySelectorAll;
  translated.document.querySelectorAll = s => s === '[id^="mount_"]' ? [mount] : roots(s);
  assert.equal(translated.api.state(), null, 'no switcher, unknown wording: no identity row');
  mount.__reactContainer$x = {child: {memoizedProps: {popoverEntryPoint: entry}, child: {tag: 5, stateNode: {matches: () => false}, sibling: {tag: 5, stateNode: translated.button}}}};
  assert.deepEqual({...translated.api.state()}, {label: 'Schreib einen Kommentar', can_change: true, reason: ''});
  const translatedPanel = JSON.parse(await translated.api.open(null)).identity;
  assert.equal(translatedPanel.options.length, 3, 'translated switcher opens Facebook choices');
  const owl = translatedPanel.options.find(o => o.label === 'Blue Owl');
  const pick = translated.menu.children[2].onClick;
  translated.menu.children[2].onClick = () => { pick(); translated.editor.attributes['aria-label'] = 'Als Blue Owl kommentieren'; };
  const switched = JSON.parse(await translated.api.act(translatedPanel.token, owl.key, []));
  assert.equal(switched.comment_identity.label, 'Als Blue Owl kommentieren', 'untranslated label still confirms the chosen identity');

  // Facebook's own data says why no switcher is on the page.
  const reasonFor = feedback => {
    const page = fixture();
    page.button.attributes['aria-label'] = 'Profilauswahl';
    page.editor.attributes['aria-label'] = 'Comment as Test Person';
    const roots = page.document.querySelectorAll;
    const mount = {__reactContainer$x: {child: {memoizedProps: {isComposer: true, feedback}}}};
    page.document.querySelectorAll = s => s === '[id^="mount_"]' ? [mount] : roots(s);
    const state = page.api.state();
    assert.equal(state.can_change, false);
    return state.reason;
  };
  const allowed = extra => ({if_viewer_can_comment_anonymously: {associated_group: {if_viewer_can_use_nickname_on_comet: {}}, actor_provider: extra}});
  assert.match(reasonFor({if_viewer_can_comment_anonymously: null}), /^Facebook does not offer/);
  assert.match(reasonFor(allowed({has_multiple_eligible_actors: false})), /^This group allows anonymous comments/);
  assert.match(reasonFor(allowed({has_multiple_eligible_actors: true})), /could not find it/);

  const duplicate = fixture(); duplicate.shell.append(duplicate.button);
  assert.equal(duplicate.api.state(duplicate.editor)?.can_change || false, false, 'ambiguous controls must never be pressed');

  const nickname = fixture();
  const normalMenu = nickname.button.onClick;
  let form, input, created = 0;
  nickname.button.onClick = () => {
    normalMenu();
    const create = new Element('div', {role: 'menuitem'}, 'Create a nickname');
    create.onClick = () => {
      nickname.menu.remove();
      form = new Element('div', {role: 'dialog', 'aria-label': 'Participate with a nickname'});
      input = new Element('input', {type: 'text', 'aria-label': 'Your nickname'}); input.value = 'Suggested Owl'; input.maxLength = 26;
      const save = new Element('button', {'aria-label': 'Create', disabled: ''}, 'Create');
      const close = new Element('button', {'aria-label': 'Close'});
      input.onEvent = () => { delete save.attributes.disabled; };
      close.onClick = () => form.remove();
      save.onClick = () => { created++; nickname.editor.attributes['aria-label'] = 'Comment as ' + input.value; form.remove(); };
      form.append(input, save, close); nickname.body.append(form); nickname.attach(form);
    };
    nickname.menu.append(create); nickname.attach(create);
  };
  const choices = JSON.parse(await nickname.api.open(null)).identity;
  const setup = JSON.parse(await nickname.api.act(choices.token, choices.options.find(o => o.label === 'Create a nickname').key, [])).identity;
  assert.equal(setup.fields[0].prompt, 'Your nickname');
  assert.equal(setup.answers[0], 'Suggested Owl', 'Facebook suggestions are preserved');
  const save = setup.options.find(o => o.label === 'Create');
  const invalid = JSON.parse(await nickname.api.act(setup.token, save.key, ['x'.repeat(27)])).identity;
  assert.match(invalid.error, /Your nickname/); assert.equal(created, 0); assert.equal(input.value, 'Suggested Owl');
  const ready = JSON.parse(await nickname.api.act(invalid.token, save.key, ['Northern Owl']));
  assert.equal(ready.comment_identity.label, 'Northern Owl'); assert.equal(created, 1);

  const avatar = fixture();
  avatar.button.onClick = () => {
    const dialog = new Element('div', {role: 'dialog'});
    for (const group of ['Color options', 'Avatar options']) {
      const radios = new Element('div', {role: 'radiogroup', 'aria-label': group});
      for (let index = 0; index < 3; index++) {
        const radio = new Element('div', {role: 'radio', 'aria-label': group + ' ' + (index + 1), 'aria-checked': String(index === 0)});
        radio.onClick = () => { radios.children.forEach(e => e.attributes['aria-checked'] = String(e === radio)); };
        radios.append(radio);
      }
      dialog.append(radios);
    }
    const save = new Element('button', {}, 'Save'); save.onClick = () => dialog.remove();
    dialog.append(save); avatar.body.append(dialog); avatar.attach(dialog);
  };
  const avatarPanel = JSON.parse(await avatar.api.open(null)).identity;
  assert.equal(avatarPanel.fields.length, 2, 'avatar and color radios become native choice fields');
  assert.equal(avatarPanel.options.length, 1, 'only Save is an action; avatar radios are answers');
  assert.deepEqual(avatarPanel.answers, [0, 0]);
  await avatar.api.act(avatarPanel.token, avatarPanel.options[0].key, [1, 2]);
  console.log('Group identity fixtures passed: choices, selection, no side effects on read, cancellation, stale tokens, locked state and pre-send identity drift.');
})().catch(error => { console.error(error); process.exitCode = 1; });
