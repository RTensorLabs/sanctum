/* Shared policy for particles, page choreography and the local film. */
(() => {
  'use strict';
  const root = document.documentElement;
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const connection = navigator.connection;
  const toggle = document.querySelector('.motion-toggle');
  let userPaused = false, effects = false, frame = 0;
  const hero = document.querySelector('.hero-stage');
  const film = document.querySelector('.film-frame');
  const video = document.querySelector('#app-film');
  const filmToggle = document.querySelector('.film-toggle');
  const expand = document.querySelector('.film-expand');
  let filmVisible = false, filmPaused = false, manualPlayback = false, loaded = false;

  function loadFilm() {
    if (loaded) return;
    video.querySelectorAll('source').forEach(source => { source.src = source.dataset.src; });
    loaded = true;
    video.load();
  }
  function updateFilmButton() {
    const playing = !video.paused;
    film.dataset.playing = String(playing);
    filmToggle.querySelector('span').textContent = playing ? 'Pause film' : 'Play film';
    filmToggle.querySelector('use').setAttribute('href', playing ? '#i-pause' : '#i-play');
  }
  function syncFilm() {
    const shouldPlay = filmVisible && !document.hidden && !filmPaused && (!userPaused || manualPlayback) && (effects || manualPlayback);
    if (shouldPlay) {
      loadFilm();
      // Autoplay may be denied. The visible Play button still works.
      video.play().catch(updateFilmButton);
    } else video.pause();
  }
  filmToggle.hidden = false;
  expand.hidden = !video.requestFullscreen && !video.webkitEnterFullscreen;
  video.addEventListener('play', updateFilmButton);
  video.addEventListener('pause', updateFilmButton);
  video.addEventListener('error', () => {
    video.pause();
    filmToggle.querySelector('span').textContent = 'Retry film';
    loaded = false;
  });
  filmToggle.addEventListener('click', () => {
    if (!video.paused) { filmPaused = true; manualPlayback = false; }
    else { filmPaused = false; manualPlayback = true; }
    syncFilm();
  });
  expand.addEventListener('click', () => {
    if (video.requestFullscreen) video.requestFullscreen().catch(() => {});
    else if (video.webkitEnterFullscreen) video.webkitEnterFullscreen();
  });
  if ('IntersectionObserver' in window) {
    new IntersectionObserver(entries => { filmVisible = entries[0].isIntersecting; syncFilm(); }, { threshold: 0.2 }).observe(video);
  } else filmVisible = true;

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
    const rectFilm = film.getBoundingClientRect();
    if (rectFilm.bottom > 0 && rectFilm.top < innerHeight) {
      const entry = Math.min(1, Math.max(0, (innerHeight - rectFilm.top) / (innerHeight * .8)));
      film.style.setProperty('--film-scale', String(.94 + entry * .06));
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
    syncFilm(); queueScroll();
  }
  toggle.hidden = false;
  toggle.addEventListener('click', () => { userPaused = !userPaused; manualPlayback = false; applyPolicy(); });
  reduced.addEventListener('change', () => { manualPlayback = false; applyPolicy(); });
  connection?.addEventListener('change', () => { manualPlayback = false; applyPolicy(); });
  document.addEventListener('visibilitychange', syncFilm);
  applyPolicy();
})();
