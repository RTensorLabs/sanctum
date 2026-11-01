/* Constellations fold into a flowing sigil. One bounded scheduler, capped
   pixels and particles, and no animation while offscreen or hidden. */
(() => {
  'use strict';
  const tau = Math.PI * 2;
  const scenes = [];
  let active = document.documentElement.dataset.motion === 'active';
  let raf = 0, last = 0, time = 0, gold = '#e8be62', light = false;
  const random = (min, max) => min + Math.random() * (max - min);
  function readTheme() {
    gold = getComputedStyle(document.documentElement).getPropertyValue('--gold').trim();
    light = document.documentElement.dataset.theme === 'light';
    scenes.forEach(scene => scene.draw(0));
  }
  function schedule() {
    if (!raf && active && !document.hidden && scenes.some(scene => scene.visible)) raf = requestAnimationFrame(tick);
  }
  function tick(now) {
    raf = 0;
    if (!active || document.hidden) { last = 0; return; }
    const dt = last ? Math.min((now - last) / 1000, .04) : 0;
    last = now; time += dt;
    scenes.forEach(scene => { if (scene.visible) scene.draw(dt); });
    schedule();
  }
  document.querySelectorAll('[data-particles]').forEach(canvas => {
    const context = canvas.getContext('2d', { alpha: true });
    if (!context) return;
    const host = canvas.parentElement;
    const orbit = canvas.dataset.particles === 'orbit';
    const pointer = { x: -9999, y: -9999, on: false };
    const waves = [];
    let width = 0, height = 0, particles = [];
    const scene = { visible: false, draw, resize };
    function resize() {
      const rect = canvas.getBoundingClientRect();
      width = rect.width; height = rect.height;
      const ratio = Math.min(devicePixelRatio || 1, width < 701 ? 1.4 : 1.7);
      canvas.width = Math.round(width * ratio); canvas.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      const count = Math.min(width < 701 ? 62 : 130, Math.round(width * height / 7800));
      particles = Array.from({ length: count }, (_, index) => ({
        phase: random(0, tau), ring: index % 5, drift: random(.25, 1),
        radius: random(.7, 1.8), x: 0, y: 0,
      }));
      draw(0);
    }
    function draw(dt) {
      context.clearRect(0, 0, width, height);
      const mobile = width < 701;
      const centerX = width * (orbit ? .5 : mobile ? .59 : .77);
      const centerY = height * (orbit ? .6 : mobile ? .64 : .46);
      const radius = Math.min(width * (mobile ? .63 : .3), height * .46);
      const phase = time * .13;
      context.strokeStyle = gold;
      for (let ring = 0; ring < 3; ring++) {
        context.globalAlpha = light ? .11 : .17; context.lineWidth = .65;
        context.beginPath();
        for (let step = 0; step <= 110; step++) {
          const angle = step / 110 * tau;
          const twist = Math.sin(angle * 3 + phase + ring) * .09;
          const scale = radius * (.64 + ring * .2 + twist);
          const x = centerX + Math.cos(angle + phase + ring * .6) * scale;
          const y = centerY + Math.sin(angle + phase + ring * .6) * scale * (.58 + ring * .08);
          if (!step) context.moveTo(x, y); else context.lineTo(x, y);
        }
        context.closePath(); context.stroke();
      }
      for (const p of particles) {
        const angle = p.phase + phase * p.drift * (p.ring % 2 ? -1 : 1);
        const radiusHere = radius * (.37 + p.ring * .17 + Math.sin(p.phase * 3 + time * .3) * .07);
        let x = centerX + Math.cos(angle) * radiusHere;
        let y = centerY + Math.sin(angle * 2 + p.ring * .7) * radiusHere * .6;
        if (pointer.on && active) {
          const dx = x - pointer.x, dy = y - pointer.y, distance = Math.hypot(dx, dy) || 1;
          const force = Math.max(0, 1 - distance / 155) * 48;
          x += dx / distance * force; y += dy / distance * force;
        }
        for (const wave of waves) {
          const dx = x - wave.x, dy = y - wave.y, distance = Math.hypot(dx, dy) || 1;
          const push = Math.max(0, 1 - Math.abs(distance - wave.age * 220) / 65) * (1 - wave.age / 1.6) * 30;
          x += dx / distance * push; y += dy / distance * push;
        }
        if (!dt) { p.x = x; p.y = y; }
        else { const ease = Math.min(1, dt * 5); p.x += (x - p.x) * ease; p.y += (y - p.y) * ease; }
      }
      const neighbors = mobile ? 2 : 4;
      context.lineWidth = .55;
      for (let i = 0; i < particles.length; i++) {
        const p = particles[i];
        for (let step = 1; step <= neighbors; step++) {
          const next = particles[(i + step * 7) % particles.length];
          const distance = Math.hypot(p.x - next.x, p.y - next.y);
          if (distance > 105) continue;
          context.globalAlpha = (1 - distance / 105) * (light ? .24 : .3);
          context.beginPath(); context.moveTo(p.x, p.y); context.lineTo(next.x, next.y); context.stroke();
        }
        const pulse = .45 + .4 * Math.sin(p.phase + time * .7);
        context.fillStyle = p.ring === 2 && !light ? '#8fd3dc' : gold;
        context.globalAlpha = pulse;
        context.beginPath(); context.arc(p.x, p.y, p.radius, 0, tau); context.fill();
        context.globalAlpha = pulse * .13;
        context.beginPath(); context.arc(p.x, p.y, p.radius * 4, 0, tau); context.fill();
      }
      for (let i = waves.length - 1; i >= 0; i--) {
        const wave = waves[i]; wave.age += dt;
        if (wave.age > 1.6) { waves.splice(i, 1); continue; }
        context.strokeStyle = gold; context.globalAlpha = (1 - wave.age / 1.6) * .35;
        context.beginPath(); context.arc(wave.x, wave.y, wave.age * 220, 0, tau); context.stroke();
      }
      context.globalAlpha = 1;
    }
    host.addEventListener('pointermove', event => {
      if (!active) return;
      const rect = canvas.getBoundingClientRect();
      pointer.x = event.clientX - rect.left; pointer.y = event.clientY - rect.top; pointer.on = true;
    }, { passive: true });
    host.addEventListener('pointerleave', () => { pointer.on = false; });
    host.addEventListener('pointerup', event => {
      if (!active || event.target.closest('a,button,summary')) return;
      const rect = canvas.getBoundingClientRect();
      waves.push({ x: event.clientX - rect.left, y: event.clientY - rect.top, age: 0 });
      if (waves.length > 4) waves.shift();
    }, { passive: true });
    scenes.push(scene);
    if ('ResizeObserver' in window) new ResizeObserver(resize).observe(host);
    else addEventListener('resize', resize, { passive: true });
    if ('IntersectionObserver' in window) new IntersectionObserver(entries => {
      scene.visible = entries[0].isIntersecting; last = 0; schedule();
    }).observe(canvas);
    else scene.visible = true;
    resize();
  });
  new MutationObserver(readTheme).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  document.addEventListener('sanctum-motion', event => {
    active = event.detail.active;
    if (raf) cancelAnimationFrame(raf);
    raf = 0; last = 0;
    scenes.forEach(scene => scene.draw(0)); schedule();
  });
  document.addEventListener('visibilitychange', () => {
    if (raf) cancelAnimationFrame(raf);
    raf = 0; last = 0; schedule();
  });
  readTheme(); schedule();
})();
