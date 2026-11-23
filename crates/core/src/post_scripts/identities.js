// Facebook owns group identities and their eligibility. This adapter only drives its scoped UI.
(() => {
  const F = window.__fb;
  const BUTTON = '[role=button],button';
  const LAYER = '[role=menu],[role=dialog],[role=listbox]';
  const OPTION = '[role=menuitemradio],[role=menuitem],[role=option],[role=radio],[role=button],button';
  const CANCEL = /^(?:Cancel|Close|Dismiss|Hủy|Đóng|Annuler|Fermer|Abbrechen|Schließen|Отмена|Закрыть|Скасувати|Закрити|Tühista|Sulge)$/i;
  const AS = /^(?:Comment as|Commenting as|Reply as|Answer as|Interact as|Kommentiere als|Commenter en tant que|Répondre en tant que|Комментировать как|Комментируете как|Ответить как|Kommenteeri (?:kui|kasutajana)|Vasta (?:kui|kasutajana)|Bình luận (?:với tư cách|dưới tên))/i;
  const clean = value => F.clean(value || '');
  const name = element => clean(element.getAttribute('aria-label') ||
    (element.getAttribute('aria-labelledby') || '').split(/\s+/).map(id => document.getElementById(id)?.textContent || '').join(' ') || element.innerText || element.getAttribute('title'));
  const visible = element => element && element.isConnected && F.visible(element);
  const enabled = element => visible(element) && F.enabled(element);
  const role = element => element.getAttribute('role') || element.tagName.toLowerCase();
  const picked = element => element.getAttribute('aria-checked') === 'true' || element.getAttribute('aria-selected') === 'true' || element.checked === true;
  const press = element => { if (!enabled(element)) throw Error('Facebook has disabled this identity action.'); element.click(); };

  // Facebook's group actor switcher ("Comment as, choose profile" in English) is translated, and
  // live DOM nodes carry no React fiber keys. Its popover trigger is found from the React root
  // instead: it opens Facebook's anonymous/nickname actor menu, whatever the interface language.
  // Structural signal only, never an action API.
  function eachFiber(visit) {
    for (const mount of document.querySelectorAll('[id^="mount_"]')) {
      const key = Object.keys(mount).find(key => key.startsWith('__reactContainer$'));
      const stack = key ? [mount[key]] : [];
      while (stack.length) {
        const fiber = stack.pop();
        visit(fiber);
        if (fiber.sibling) stack.push(fiber.sibling);
        if (fiber.child) stack.push(fiber.child);
      }
    }
  }

  function actorSwitchers() {
    const found = new Set();
    let entry;
    try { entry = require('GroupsCometUFIAnonActorSwitcherMenu.entrypoint'); } catch { return found; }
    if (!entry) return found;
    eachFiber(fiber => {
      if (fiber.memoizedProps?.popoverEntryPoint !== entry) return;
      for (const inner = [fiber.child]; inner.length;) {
        const node = inner.pop();
        if (!node) continue;
        if (node.tag === 5 && node.stateNode?.matches?.(BUTTON)) { found.add(node.stateNode); break; }
        inner.push(node.sibling, node.child);
      }
    });
    return found;
  }

  // Why no switcher is on the page, from the data Facebook's comment box draws it with
  // (CometUFICommentRow.react). The website needs more than the group allowing anonymity.
  function missingReason() {
    let offer = '';
    eachFiber(fiber => {
      const props = fiber.memoizedProps, feedback = props?.isComposer === true ? props.feedback : null;
      if (!feedback || typeof feedback !== 'object' || !('if_viewer_can_comment_anonymously' in feedback)) return;
      const anonymous = feedback.if_viewer_can_comment_anonymously;
      const shown = (anonymous?.associated_group?.if_viewer_can_use_nickname_on_comet != null &&
        (anonymous.actor_provider?.has_multiple_eligible_actors || feedback.if_viewer_has_locked_actor != null)) ||
        feedback.if_viewer_post_type_blocks_alternate_voice != null;
      if (shown) offer = 'shown'; else if (anonymous && !offer) offer = 'limited';
    });
    return offer === 'shown' ? 'Facebook offers a comment identity choice here, but BlindFB could not find it. Please send a debug report.' :
      offer === 'limited' ? 'This group allows anonymous comments, but Facebook\'s website does not offer your account a choice on this post. Facebook\'s phone app may still offer it.' :
      'Facebook does not offer identity switching on this post.';
  }

  function create({current, editorFor, target, read}) {
    let saved = null, serial = 0;
    const group = () => /^\/groups\/[^/]+\//.test(new URL(target()).pathname) ||
      (typeof location !== 'undefined' && /(^|\.)facebook\.com$/i.test(new URL(location.href).hostname) && /^\/groups\/[^/]+\//.test(new URL(location.href).pathname));
    function control(editor) {
      if (!group()) return null;
      const root = current();
      editor = editor || editorFor(null);
      if (!editor || !root.contains(editor)) return null;
      const switchers = actorSwitchers();
      for (let scope = editor.parentElement; scope && root.contains(scope); scope = scope.parentElement) {
        const candidates = [...scope.querySelectorAll(BUTTON)].filter(element => {
          if (!visible(element) || element.contains(editor) || element.closest('[role=article][aria-label]')) return false;
          const text = name(element);
          // This global selector is also beside the composer, but cannot choose a group nickname.
          if (/^Switch to a different profile or Page$/i.test(text)) return false;
          const prefix = text.split(',')[0].trim();
          const sharedPrefix = prefix.length > 4 && text.includes(',') && name(editor).startsWith(prefix + ' ');
          return switchers.has(element) || AS.test(text) || sharedPrefix || /^(?:Choose (?:how|who)|Change (?:who|identity))/i.test(text);
        });
        if (candidates.length) return candidates.length === 1 ? candidates[0] : null;
        if (scope === root) break;
      }
      return null;
    }

    function state(editor) {
      if (!group()) return null;
      editor = editor || editorFor(null);
      const button = control(editor);
      // The desktop selector says "Comment as, choose profile" for every identity.
      // Its editor, in contrast, says "Comment as Brandon" or the selected nickname.
      const editorName = editor ? name(editor) : '';
      const prefix = button ? name(button).split(',')[0].trim() : '';
      const label = AS.test(editorName) ? editorName.replace(AS, '').trim() :
        prefix.length > 4 && editorName.startsWith(prefix + ' ') ? editorName.slice(prefix.length).trim() :
        // Untranslated wording: the editor's whole label still names, and tracks, the identity.
        button ? editorName : '';
      if (!label) return null;
      return {label, can_change: !!enabled(button), reason: enabled(button) ? '' : missingReason()};
    }

    function capture(editor, expected) {
      if (saved && panel()) throw Error('Finish or cancel the Facebook identity dialog before sending your comment.');
      const value = state(editor);
      if (expected && (!value || value.label !== expected)) throw Error('Your commenting identity changed. Refresh and choose who to comment as before sending.');
      return value ? {label: value.label} : null;
    }

    function check(editor, before) {
      const after = state(editor);
      if ((before?.label || null) !== (after?.label || null)) throw Error('Your commenting identity changed. Nothing was sent. Refresh before trying again.');
      return true;
    }

    const layers = () => [...document.querySelectorAll(LAYER)].filter(visible);
    function panel() {
      const found = layers().filter(element => !saved.before.has(element) && !element.contains(saved.root));
      const inner = found.filter(element => !found.some(other => other !== element && element.contains(other)));
      if (inner.length > 1) throw Error('Facebook opened more than one identity panel. Close them and try again.');
      return inner[0] || null;
    }

    function fieldName(element) {
      return clean(element.getAttribute('aria-label') ||
        (element.getAttribute('aria-labelledby') || '').split(/\s+/).map(id => document.getElementById(id)?.textContent || '').join(' ') ||
        [...(element.labels || [])].map(label => label.innerText || label.textContent).join(' ') || element.getAttribute('placeholder') || element.getAttribute('name'));
    }

    function fields(layer) {
      const controls = [...layer.querySelectorAll('input,textarea,[role=textbox][contenteditable=true],[role=radio],[role=checkbox]')].filter(enabled);
      const seen = new Set(), rows = [];
      for (const element of controls) {
        if (seen.has(element) || ['hidden', 'submit', 'button', 'file'].includes(element.type)) continue;
        if (element.tagName === 'INPUT' && !['', 'text', 'search', 'checkbox', 'radio'].includes(element.type)) throw Error('Facebook requested an unsupported identity field. Nothing was submitted.');
        let members = [element];
        const radio = element.type === 'radio' || role(element) === 'radio';
        if (radio) {
          const section = element.closest('[role=radiogroup],fieldset');
          if (!element.name && !section) throw Error('Facebook did not identify its identity choices.');
          members = controls.filter(other => (other.type === 'radio' || role(other) === 'radio') &&
            (section && section !== layer ? section.contains(other) : other.name === element.name && other.form === element.form));
        }
        members.forEach(other => seen.add(other));
        const kind = radio ? 'choice' : element.type === 'checkbox' || role(element) === 'checkbox' ? 'check' : 'text';
        const prompt = kind === 'choice' ? name(element.closest('[role=radiogroup],fieldset') || element) || fieldName(element) : fieldName(element);
        const choices = kind === 'choice' ? members.map(fieldName) : [];
        if (!prompt || choices.some(value => !value) || new Set(choices).size !== choices.length) throw Error('Facebook did not clearly label its identity form. Nothing was submitted.');
        rows.push({kind, prompt, choices, members});
      }
      if (rows.length > 30) throw Error('Facebook opened an unsupported identity form. Nothing was submitted.');
      return rows;
    }

    function describe(layer) {
      const rows = fields(layer);
      const candidates = [...layer.querySelectorAll(OPTION)].filter(element => visible(element) && name(element) && !CANCEL.test(name(element)) &&
        !element.getAttribute('href') && !rows.some(row => row.members.includes(element)));
      const elements = candidates.filter(element => !candidates.some(other => other !== element && other.contains(element)));
      if (elements.length > 30) throw Error('Facebook exposed too many identity actions. Nothing was submitted.');
      const labels = elements.map(name);
      if (new Set(labels).size !== labels.length) throw Error('Facebook exposed ambiguous identity actions. Nothing was submitted.');
      const options = elements.map((element, index) => ({key: String(index), label: name(element), selected: picked(element),
        enabled: enabled(element) || (rows.length > 0 && role(element) === 'button'), element}));
      const signature = JSON.stringify([options.map(option => [option.label, role(option.element), option.selected]), rows.map(row => [row.kind, row.prompt, row.choices])]);
      return {layer, options, rows, signature, text: clean(layer.innerText)};
    }

    function snapshot(description, answers = null, error = '') {
      saved.description = description;
      saved.token = String(++serial);
      answers = answers || description.rows.map(row => row.kind === 'check' ? picked(row.members[0]) : row.kind === 'choice' ?
        (row.members.some(picked) ? row.members.findIndex(picked) : null) : row.members[0].value || '');
      return JSON.stringify({identity: {token: saved.token, text: clean(description.layer.innerText),
        options: description.options.map(({element, ...option}) => option), fields: description.rows.map(({members, ...row}) => row), answers, error}});
    }

    function currentPanel(token) {
      current();
      if (!saved || saved.token !== token || !visible(saved.description?.layer) || panel() !== saved.description.layer) throw Error('The Facebook identity panel changed or expired. Open Comment as again.');
      const description = describe(saved.description.layer);
      if (description.signature !== saved.description.signature || description.options.some((option, i) => option.element !== saved.description.options[i].element) || description.rows.some((row, i) => row.members.some((element, j) => element !== saved.description.rows[i].members[j]))) throw Error('The Facebook identity panel changed. Open Comment as again.');
      return description;
    }

    async function open(parent) {
      current();
      if (saved && panel()) throw Error('Finish or cancel the open identity panel first.');
      const editor = editorFor(parent) || editorFor(null), button = control(editor);
      if (!button || !enabled(button)) throw Error('Facebook does not offer an available identity selector on this post.');
      saved = {before: new Set(layers()), root: current(), button, editor, token: ''};
      press(button);
      const shown = await F.waitFor(panel, 5000);
      if (!shown) { saved = null; throw Error('Facebook did not open the identity selector. Your comment was not sent.'); }
      return snapshot(describe(shown));
    }

    function validate(rows, answers) {
      if (!Array.isArray(answers) || answers.length !== rows.length) throw Error('Complete the identity form before continuing.');
      rows.forEach((row, index) => {
        const answer = answers[index], element = row.members[0];
        if (row.kind === 'choice' ? !Number.isInteger(answer) || answer < 0 || answer >= row.members.length : row.kind === 'check' ? typeof answer !== 'boolean' : typeof answer !== 'string' || !answer.trim() || answer.length > 10000 || (element.maxLength > 0 && answer.length > element.maxLength)) throw Error('Check your answer for: ' + row.prompt);
        if (row.members.some(element => !enabled(element))) throw Error('Facebook has disabled an identity field. Nothing was submitted.');
      });
    }

    function fill(rows, answers) {
      rows.forEach((row, index) => {
        const element = row.members[0], answer = answers[index];
        if (row.kind === 'choice') { const choice = row.members[answer]; if (!picked(choice)) choice.click(); }
        else if (row.kind === 'check') { if (picked(element) !== answer) element.click(); }
        else if (element.getAttribute('contenteditable') === 'true') throw Error('Facebook requested an unsupported nickname editor. Nothing was submitted.');
        else {
          const prototype = element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
          const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
          if (setter) setter.call(element, answer); else element.value = answer;
          element.dispatchEvent(new Event('input', {bubbles: true}));
          element.dispatchEvent(new Event('change', {bubbles: true}));
        }
      });
    }

    async function act(token, key, answers) {
      const description = currentPanel(token);
      const option = description.options.find(option => option.key === key);
      if (!option || !option.enabled) throw Error('This identity action is unavailable. Nothing was submitted.');
      answers = answers || [];
      try { validate(description.rows, answers); }
      catch (error) { return snapshot(description, answers, error.message); }
      fill(description.rows, answers);
      const action = await F.waitFor(() => {
        const updated = currentPanel(saved.token).options.find(other => other.key === key);
        return updated && enabled(updated.element) ? updated.element : null;
      }, 2000);
      if (!action) return snapshot(describe(description.layer), answers, 'Facebook has not enabled this action. Check the form and try again.');
      const actorName = ['menuitemradio', 'radio'].includes(role(action)) ? option.label :
        description.rows.length === 1 && description.rows[0].kind === 'text' && description.rows[0].members[0].maxLength === 26 ? answers[0] : null;
      press(action);
      // Clicking a choice can close the menu, open nickname setup, or leave validation feedback.
      const changed = await F.waitFor(() => {
        const next = panel();
        if (!next) return {done: true};
        const nextDescription = describe(next);
        return next !== description.layer || nextDescription.signature !== description.signature || nextDescription.text !== description.text ? {next: nextDescription} : null;
      }, 4000);
      const next = panel();
      if (changed?.done || !next) {
        const editor = saved.editor;
        saved = null;
        // An untranslated editor label holds the chosen name inside its own wording.
        if (actorName && !await F.waitFor(() => state(editorFor(null) || editor)?.label.includes(actorName), 10000)) throw Error('Facebook did not confirm the selected commenting identity. Refresh before sending a comment.');
        return read();
      }
      return snapshot(changed?.next || describe(next), null, changed ? '' : 'Facebook has not completed this identity action. Check the information below.');
    }

    async function cancel(token) {
      currentPanel(token);
      for (let count = 0; count < 5; count++) {
        const shown = panel();
        if (!shown) { saved = null; return read(); }
        const close = [...shown.querySelectorAll(BUTTON)].filter(element => enabled(element) && CANCEL.test(name(element)));
        if (close.length === 1) press(close[0]);
        else if (role(shown) === 'menu' || role(shown) === 'listbox' || (!fields(shown).length && saved.description.layer === shown)) {
          if (!enabled(saved.button)) throw Error('Close the Facebook identity menu before continuing.');
          press(saved.button);
        } else throw Error('Facebook did not expose a unique Close action. Close its identity dialog before continuing.');
        await F.waitFor(() => !visible(shown) || panel() !== shown, 2000);
      }
      throw Error('Facebook has not closed its identity dialog. No comment was sent.');
    }

    return {state, capture, check, open, act, cancel};
  }
  window.__blindfbIdentities = {create, eachFiber};
})();
