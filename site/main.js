// Progressive extras only: the page is complete without this file.

// Theme: dark unless the visitor picked light with the footer toggle.
const root = document.documentElement;
const toggle = document.querySelector('.theme');

function setTheme(theme) {
  if (theme === 'light') root.dataset.theme = 'light';
  else delete root.dataset.theme;
  if (toggle) toggle.textContent = theme === 'light' ? 'Dark theme' : 'Light theme';
}

let saved = null;
try {
  saved = localStorage.getItem('theme');
} catch {
  // Storage can be blocked (private windows, file:// in some browsers).
}
setTheme(saved);

if (toggle) {
  toggle.hidden = false;
  toggle.addEventListener('click', () => {
    const next = root.dataset.theme === 'light' ? 'dark' : 'light';
    setTheme(next);
    try {
      localStorage.setItem('theme', next);
    } catch {
      // Not saved; the choice still holds for this visit.
    }
  });
}

// Copy buttons: added to every .cmd.copyable block, copying its data-copy text
// (or the block's own text when there is none).
for (const block of document.querySelectorAll('.cmd.copyable')) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'copy';
  button.textContent = 'Copy';
  button.addEventListener('click', async () => {
    const text = block.dataset.copy ?? block.querySelector('pre').textContent.trim();
    try {
      await navigator.clipboard.writeText(text);
      button.textContent = 'Copied';
    } catch {
      // Clipboard refused: select the command so it can be copied by hand.
      const range = document.createRange();
      range.selectNodeContents(block.querySelector('pre'));
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      button.textContent = 'Selected';
    }
    setTimeout(() => (button.textContent = 'Copy'), 1500);
  });
  block.append(button);
}
