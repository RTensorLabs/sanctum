/* Shared motion policy for particles and page choreography. */
(() => {
  'use strict';
  const root = document.documentElement;
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const connection = navigator.connection;
  const toggle = document.querySelector('.motion-toggle');
  let userPaused = false, effects = false, frame = 0;
  const hero = document.querySelector('.hero-stage');
  function paintScroll() {
    frame = 0;
    const range = document.documentElement.scrollHeight - innerHeight;
    root.style.setProperty('--reading-progress', String(range > 0 ? scrollY / range : 0));
    if (!effects) return;
    const rect = hero.getBoundingClientRect();
    if (rect.bottom > 0 && rect.top < innerHeight) {
      const progress = Math.min(1, Math.max(0, -rect.top / Math.max(1, rect.height)));
      hero.style.setProperty('--hero-shift', `${progress * -55}px`);
      hero.style.setProperty('--back-rise', `${progress * 85}px`);
      hero.style.setProperty('--back-rotate', `${-8 - progress * 14}deg`);
      hero.style.setProperty('--front-rotate', `${5 + progress * 8}deg`);
      hero.style.setProperty('--orbit-angle', `${-26 + progress * 70}deg`);
    }

  }
  function queueScroll() { if (!frame) frame = requestAnimationFrame(paintScroll); }
  addEventListener('scroll', queueScroll, { passive: true });
  addEventListener('resize', queueScroll, { passive: true });
  document.querySelector('.hero-preview').addEventListener('pointermove', event => {
    if (!effects || event.pointerType !== 'mouse') return;
    const rect = event.currentTarget.getBoundingClientRect();
    hero.style.setProperty('--pointer-x', `${(event.clientX - rect.left - rect.width / 2) * .025}px`);
    hero.style.setProperty('--pointer-y', `${(event.clientY - rect.top - rect.height / 2) * .018}px`);
  }, { passive: true });
  document.querySelector('.hero-preview').addEventListener('pointerleave', () => {
    hero.style.setProperty('--pointer-x', '0px'); hero.style.setProperty('--pointer-y', '0px');
  });
  document.querySelectorAll('.feature-card').forEach(card => {
    card.addEventListener('pointermove', event => {
      if (!effects || event.pointerType !== 'mouse') return;
      const rect = card.getBoundingClientRect();
      card.style.setProperty('--spot-x', `${event.clientX - rect.left}px`);
      card.style.setProperty('--spot-y', `${event.clientY - rect.top}px`);
    }, { passive: true });
  });
  function applyPolicy() {
    effects = !userPaused && !reduced.matches && !connection?.saveData;
    root.dataset.motion = effects ? 'active' : 'still';
    toggle.setAttribute('aria-pressed', String(userPaused));
    toggle.querySelector('span').textContent = effects ? 'Pause effects' : (userPaused ? 'Resume effects' : 'Effects reduced by your device');
    toggle.querySelector('use').setAttribute('href', effects ? '#i-pause' : '#i-play');
    toggle.disabled = !userPaused && (reduced.matches || !!connection?.saveData);
    document.dispatchEvent(new CustomEvent('sanctum-motion', { detail: { active: effects } }));
    queueScroll();
  }
  toggle.hidden = false;
  toggle.addEventListener('click', () => { userPaused = !userPaused; applyPolicy(); });
  reduced.addEventListener('change', () => { applyPolicy(); });
  connection?.addEventListener('change', () => { applyPolicy(); });
  applyPolicy();
})();
