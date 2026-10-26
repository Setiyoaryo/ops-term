const root = document.documentElement;
const navigation = document.querySelector('#navigation');
const menuToggle = document.querySelector('.menu-toggle');
const themeToggle = document.querySelector('.theme-toggle');
const paletteNames = new Set(['original', 'soft', 'warm', 'cool']);

function applyTheme(theme) {
  const next = theme === 'light' ? 'light' : 'dark';
  root.dataset.theme = next;
  themeToggle.setAttribute('aria-label', `Switch to ${next === 'dark' ? 'light' : 'dark'} theme`);
  try { localStorage.setItem('opsterm-site-theme', next); } catch { /* Storage is optional. */ }
}

try {
  const saved = localStorage.getItem('opsterm-site-theme');
  if (saved === 'light' || saved === 'dark') applyTheme(saved);
} catch { /* The site remains usable with storage blocked. */ }
themeToggle.addEventListener('click', () => applyTheme(root.dataset.theme === 'dark' ? 'light' : 'dark'));

function closeMenu() {
  navigation.classList.remove('is-open');
  menuToggle.setAttribute('aria-expanded', 'false');
  menuToggle.setAttribute('aria-label', 'Open navigation');
}
menuToggle.addEventListener('click', () => {
  const open = menuToggle.getAttribute('aria-expanded') !== 'true';
  navigation.classList.toggle('is-open', open);
  menuToggle.setAttribute('aria-expanded', String(open));
  menuToggle.setAttribute('aria-label', `${open ? 'Close' : 'Open'} navigation`);
});
navigation.addEventListener('click', (event) => { if (event.target.closest('a')) closeMenu(); });
document.addEventListener('keydown', (event) => { if (event.key === 'Escape') closeMenu(); });

for (const button of document.querySelectorAll('button[data-palette]')) {
  button.addEventListener('click', () => {
    const palette = button.dataset.palette;
    if (!paletteNames.has(palette)) return;
    document.querySelector('.workspace').dataset.palette = palette;
    for (const option of document.querySelectorAll('button[data-palette]')) {
      option.setAttribute('aria-pressed', String(option === button));
    }
  });
}

function selectTab(buttons, selected, attribute) {
  const value = selected.dataset[attribute];
  for (const button of buttons) {
    const active = button === selected;
    button.setAttribute('aria-selected', String(active));
    button.tabIndex = active ? 0 : -1;
    const panel = document.getElementById(button.getAttribute('aria-controls'));
    panel.hidden = !active;
  }
  return value;
}

function enableKeyboardTabs(buttons, onSelect) {
  buttons.forEach((button, index) => {
    button.tabIndex = button.getAttribute('aria-selected') === 'true' ? 0 : -1;
    button.addEventListener('click', () => onSelect(button));
    button.addEventListener('keydown', (event) => {
      let next;
      if (event.key === 'ArrowRight') next = (index + 1) % buttons.length;
      else if (event.key === 'ArrowLeft') next = (index - 1 + buttons.length) % buttons.length;
      else if (event.key === 'Home') next = 0;
      else if (event.key === 'End') next = buttons.length - 1;
      else return;
      event.preventDefault();
      onSelect(buttons[next]);
      buttons[next].focus();
    });
  });
}
const sessionButtons = [...document.querySelectorAll('button[data-session]')];
enableKeyboardTabs(sessionButtons, button => selectTab(sessionButtons, button, 'session'));

const installButtons = [...document.querySelectorAll('button[data-install]')];
let installMethod = 'download';
enableKeyboardTabs(installButtons, (button) => {
  installMethod = selectTab(installButtons, button, 'install');
  document.querySelector('[data-command-label]').textContent = installMethod === 'download' ? 'verify, then install' : 'clone, build, make it yours';
  document.querySelector('[data-install-note]').textContent = installMethod === 'download'
    ? 'Ubuntu 24.04 · x86_64 · curl + CA certificates for updates.'
    : 'Requires Rust, Node/npm and Linux build dependencies. See requirements below.';
  document.querySelector('.copy-status').textContent = '';
  clearTimeout(copyTimeout);
  copyButton.textContent = 'Copy ⧉';
});

const copyButton = document.querySelector('.copy-button');
let copyTimeout;
copyButton.addEventListener('click', async () => {
  const panel = document.getElementById(`commands-${installMethod}`);
  const status = document.querySelector('.copy-status');
  try {
    if (!navigator.clipboard?.writeText) throw new Error('Clipboard unavailable');
    await navigator.clipboard.writeText(panel.textContent.trim());
    status.textContent = 'Copied to clipboard.';
    copyButton.textContent = 'Copied ✓';
    clearTimeout(copyTimeout);
    copyTimeout = setTimeout(() => { copyButton.textContent = 'Copy ⧉'; }, 2200);
  } catch {
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(panel);
    selection?.removeAllRanges();
    selection?.addRange(range);
    status.textContent = 'Select the commands and copy them manually.';
    panel.focus();
  }
});
