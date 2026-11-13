// Participation review stays on the reader's page. No navigation or comment retry.
(() => {
  const F = window.__fb;
  const CONTROL = 'input[type=radio],input[type=checkbox],input[type=text],textarea,[role=textbox][contenteditable=true]';
  const text = e => F.clean(e.innerText || '');
  const name = e => F.clean(e.getAttribute('aria-label') || (e.getAttribute('aria-labelledby') || '').split(/\s+/).map(id => document.getElementById(id)?.textContent || '').join(' ') || [...(e.labels || [])].map(text).join(' '));
  let saved = null, serial = 0;
  const snapshot = (form, answers = [], error = '') => JSON.stringify({participation: {
    token: form.token, text: text(form.d), answers, error,
    fields: form.rows.map(({controls, ...row}) => row),
  }});

  function dialog(root) {
    const found = [...document.querySelectorAll('[role=dialog]')].filter(d => F.visible(d) && d !== root && !d.contains(root) &&
      d.querySelector(CONTROL) && !d.querySelector('[role=textbox][contenteditable=true]'));
    // Ignore wrapper dialogs around the form.
    const inner = found.filter(d => !found.some(other => other !== d && d.contains(other)));
    return inner.length === 1 ? inner[0] : null;
  }

  function prompt(field, d) {
    if (field.type !== 'radio' && (field.type === 'checkbox' || field.getAttribute('aria-label') || field.getAttribute('aria-labelledby')) && name(field)) return name(field);
    for (let p = field.parentElement; p && p !== d; p = p.parentElement) {
      if (field.type !== 'radio' && p.getAttribute('aria-label')) return name(p);
      const walker = document.createTreeWalker(p, NodeFilter.SHOW_TEXT), lines = [];
      let node;
      while ((node = walker.nextNode())) {
        if (node.parentElement.closest('label,button,[role=button],textarea,input') || !F.visible(node.parentElement)) continue;
        const value = F.clean(node.nodeValue || '');
        if (value) lines.push(value);
      }
      if (lines.length) return lines.join('\n');
    }
    throw Error('Facebook did not label a participation question. Open the group on Facebook to answer it.');
  }

  function read(d) {
    const fields = [...d.querySelectorAll(CONTROL)].filter(F.visible), seen = new Set(), rows = [];
    for (const field of fields) {
      if (seen.has(field)) continue;
      let controls = [field];
      if (field.type === 'radio') {
        if (!field.name) throw Error('Facebook did not identify a question’s choices.');
        controls = fields.filter(e => e.type === 'radio' && e.name === field.name && e.form === field.form);
        // Facebook sometimes reuses name=radio in separate question containers.
        const section = field.closest('[role=radiogroup],fieldset,[aria-disabled]');
        if (section && section !== d) controls = controls.filter(e => section.contains(e));
      }
      controls.forEach(e => seen.add(e));
      const kind = field.type === 'radio' ? 'choice' : field.type === 'checkbox' ? 'check' : 'text';
      const choices = kind === 'choice' ? controls.map(name) : [];
      if (choices.some(x => !x) || new Set(choices).size !== choices.length) throw Error('Facebook’s answer choices are ambiguous.');
      rows.push({kind, prompt: prompt(field, d), choices, controls});
    }
    if (!rows.length) throw Error('Facebook did not expose its participation questions.');
    saved = {d, rows, token: String(++serial)};
    return snapshot(saved);
  }

  function current(token) {
    if (!saved || saved.token !== token || !saved.d.isConnected || !F.visible(saved.d) || saved.rows.some(r => r.controls.some(e => !e.isConnected || !saved.d.contains(e)))) throw Error('The participation questions changed. Refresh before trying again.');
    if (saved.rows.some(r => prompt(r.controls[0], saved.d) !== r.prompt || r.kind === 'choice' && r.controls.some((e, i) => name(e) !== r.choices[i]))) throw Error('The participation questions changed. Refresh before trying again.');
    return saved;
  }

  async function submit(token, answers) {
    const form = current(token);
    if (!Array.isArray(answers) || answers.length !== form.rows.length) throw Error('Answer every participation question.');
    // Validate the complete request before changing any Facebook field.
    form.rows.forEach((row, i) => {
      const answer = answers[i];
      if (row.kind === 'choice' ? !Number.isInteger(answer) || answer < 0 || answer >= row.controls.length : row.kind === 'check' ? typeof answer !== 'boolean' : typeof answer !== 'string' || !answer.trim()) throw Error('Answer every participation question.');
    });
    form.rows.forEach((row, i) => {
      const e = row.controls[0], answer = answers[i];
      if (row.kind === 'choice') { const option = row.controls[answer]; if (!option.checked) option.click(); }
      else if (row.kind === 'check') { if (e.checked !== answer) e.click(); }
      else {
        const proto = e.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(proto, 'value').set.call(e, answer);
        e.dispatchEvent(new Event('input', {bubbles: true}));
        e.dispatchEvent(new Event('change', {bubbles: true}));
      }
    });
    let button;
    try {
      button = await F.waitFor(() => {
        current(token);
        const buttons = [...form.d.querySelectorAll('[role=button],button')].filter(e => F.visible(e) && F.enabled(e) && /^(Submit|Submit answers|Send|Gửi|Envoyer|Soumettre|Отправить|Saada)$/i.test(name(e) || text(e)));
        return buttons.length === 1 ? buttons[0] : null;
      }, 3000);
      if (!button) throw Error('Facebook has not enabled Submit. Check your answers and agreement to the group rules.');
    } catch (error) {
      current(token);
      // Safe to correct the form: Submit has not been clicked.
      return snapshot(form, answers, error.message);
    }
    saved = null;
    button.click();
    if (!await F.waitFor(() => !form.d.isConnected || !F.visible(form.d), 10000)) throw Error('Facebook did not confirm your answers. Check the group before submitting again.');
    return 'submitted';
  }

  function cancel(token) {
    const {d} = current(token);
    const buttons = [...d.querySelectorAll('[role=button],button')].filter(e => F.visible(e) && /^(Cancel|Close|Hủy|Đóng|Annuler|Fermer|Отмена|Закрыть|Tühista|Sulge)$/i.test(name(e) || text(e)));
    if (buttons.length !== 1) throw Error('Close the participation review on Facebook before trying again.');
    saved = null;
    buttons[0].click();
    return 'cancelled';
  }
  window.__blindfbQuestions = {dialog, read, submit, cancel};
})();
