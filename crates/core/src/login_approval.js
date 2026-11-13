// Facebook's "Someone's trying to log in" request (/afad/): read the dialog,
// or press one of its answers. `choice` null = read only. Facebook's own
// button labels are returned and matched, so any language works.
(async (choice) => {
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
  const shown = (e) => e.getClientRects().length > 0 && !e.closest('[hidden],[aria-hidden="true"]');
  const label = (b) => b.innerText.trim();
  // The answers are the dialog's one-line text buttons; the login details
  // button spans several lines and Close / map buttons have no text.
  const choicesOf = (d) =>
    [...d.querySelectorAll('[role=button],button')].filter((b) => shown(b) && label(b) && !label(b).includes('\n'));
  const find = () =>
    [...document.querySelectorAll('[role=dialog]')].filter((d) => shown(d) && choicesOf(d).length >= 2).pop();
  const linesOf = (root, skip) => {
    // The dialog's label is screen-reader text ("Header of ... dialog").
    const header = document.getElementById(root.getAttribute('aria-labelledby') || '');
    const hidden = header && header.getBoundingClientRect().width <= 1 ? header.innerText.trim() : '';
    return root.innerText
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line && line !== hidden && !skip.includes(line));
  };
  const page = () => {
    const dialog = [...document.querySelectorAll('[role=dialog]')].filter(shown).pop();
    const root = dialog || document.querySelector('[role=main]') || document.body;
    return linesOf(root, []).slice(0, 20);
  };
  let dialog;
  for (let waited = 0; !(dialog = find()) && waited < 15000; waited += 250) await sleep(250);
  if (!dialog) return JSON.stringify({ lines: page(), choices: [] });
  const choices = choicesOf(dialog);
  if (choice === null) return JSON.stringify({ lines: linesOf(dialog, choices.map(label)), choices: choices.map(label) });
  const button = choices.find((b) => label(b) === choice);
  if (!button) return JSON.stringify({ err: 'answer not found', lines: [], choices: [] });
  button.click();
  for (let waited = 0; button.isConnected && shown(button) && waited < 10000; waited += 250) await sleep(250);
  await sleep(1000);
  return JSON.stringify({ lines: page(), choices: [], answered: !(button.isConnected && shown(button)) });
})
