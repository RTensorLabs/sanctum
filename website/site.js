(() => {
  'use strict';
  const root = document.documentElement;
  root.classList.add('js');
  const themeButton = document.querySelector('.theme-toggle');
  const preference = window.matchMedia('(prefers-color-scheme: light)');
  let choice = null;
  try { choice = localStorage.getItem('sanctum-site-theme'); } catch (_) { /* Optional preference only. */ }
  if (choice !== 'dark' && choice !== 'light') choice = null;
  function applyTheme() {
    const theme = choice || (preference.matches ? 'light' : 'dark');
    root.dataset.theme = theme;
    const target = theme === 'dark' ? 'light' : 'dark';
    themeButton.setAttribute('aria-label', `Switch to ${target} theme`);
    themeButton.title = `Switch to ${target} theme`;
    themeButton.querySelector('use').setAttribute('href', theme === 'dark' ? '#i-sun' : '#i-moon');
    document.querySelector('meta[name="theme-color"]').content = theme === 'dark' ? '#0b0c0f' : '#f8f4ec';
    document.querySelectorAll('[data-screenshot]').forEach(image => {
      const source = image.dataset[theme];
      if (source) {
        image.src = source;
        const link = image.closest('a[data-screenshot-link]');
        if (link) link.href = source;
      }
    });
  }
  applyTheme();
  themeButton.addEventListener('click', () => {
    choice = root.dataset.theme === 'dark' ? 'light' : 'dark';
    try { localStorage.setItem('sanctum-site-theme', choice); } catch (_) { /* The switch still works. */ }
    applyTheme();
  });
  preference.addEventListener('change', () => { if (!choice) applyTheme(); });
  const menuButton = document.querySelector('.menu-toggle');
  const menu = document.querySelector('#site-nav');
  function setMenu(open) {
    menu.toggleAttribute('data-open', open);
    menuButton.setAttribute('aria-expanded', String(open));
    menuButton.setAttribute('aria-label', open ? 'Close navigation' : 'Open navigation');
  }
  menuButton.addEventListener('click', () => setMenu(!menu.hasAttribute('data-open')));
  menu.addEventListener('click', event => { if (event.target.closest('a')) setMenu(false); });
  document.addEventListener('click', event => {
    if (menu.hasAttribute('data-open') && !menu.contains(event.target) && !menuButton.contains(event.target)) setMenu(false);
  });
  window.matchMedia('(min-width: 701px)').addEventListener('change', event => { if (event.matches) setMenu(false); });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && menu.hasAttribute('data-open')) {
      setMenu(false);
      menuButton.focus();
    }
  });
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  if ('IntersectionObserver' in window && !motion.matches) {
    const observer = new IntersectionObserver(entries => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          entry.target.dataset.visible = 'true';
          observer.unobserve(entry.target);
        }
      });
    }, { threshold: 0.08 });
    document.querySelectorAll('.reveal').forEach(element => {
      element.dataset.visible = 'false';
      observer.observe(element);
    });
  }
})();
