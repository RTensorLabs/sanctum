/* Recreate the local 12-second film from the sample app screenshots.
 * Requires Playwright, Chromium and ffmpeg. Start the website on port 4173.
 * Usage: node website/tools/render-film.cjs [http://127.0.0.1:4173]
 * No remote assets, real conversation data, sound or tracking. */
const { chromium } = require('playwright');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const fs = require('node:fs');
const path = require('node:path');
const base = process.argv[2] || 'http://127.0.0.1:4173';
const output = path.resolve(__dirname, '../media');
const fps = 24;
(async () => {
  fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || '/usr/bin/chromium', args: ['--no-sandbox'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 });
  await page.setContent(`<!doctype html><html><head><style>
    @font-face{font-family:Figtree;src:url('${base}/fonts/Figtree-400.woff2')}
    @font-face{font-family:Figtree;src:url('${base}/fonts/Figtree-600.woff2');font-weight:600}
    @font-face{font-family:Serif;src:url('${base}/fonts/InstrumentSerif-400-italic.woff2');font-style:italic}
    *{box-sizing:border-box}body{margin:0;width:1280px;height:720px;overflow:hidden;background:#0b0c0f;color:#f4f0e8;font-family:Figtree,sans-serif}
    .aura{position:absolute;width:840px;height:840px;left:580px;top:-90px;border-radius:50%;background:radial-gradient(circle,#a9772629,transparent 65%)}
    .ring{position:absolute;width:690px;height:460px;left:600px;top:130px;border:1px solid #d6b46835;border-radius:50%}
    .ring.two{width:790px;height:550px;left:550px;top:80px;border-style:dashed;border-color:#d6b46820}
    .brand{position:absolute;left:64px;top:46px;display:flex;gap:10px;align-items:center;font-size:23px;font-weight:600;letter-spacing:-.04em}
    .brand img{width:32px;height:32px}.brand span{font-size:10px;color:#a6a4a0;letter-spacing:.16em;text-transform:uppercase;margin-left:12px;font-weight:400}
    .copy{position:absolute;left:72px;top:254px;width:550px}
    .eyebrow{color:#e8be62;font-size:11px;letter-spacing:.2em;text-transform:uppercase;margin-bottom:24px}
    h1{font-size:72px;font-weight:600;line-height:1.04;letter-spacing:-.05em;margin:0 0 24px}
    em{font-family:Serif,Georgia,serif;color:#e8be62;font-size:1.12em;font-weight:400}
    .sub{font-size:17px;line-height:1.6;color:#b3b3bd;max-width:400px}
    .phone{position:absolute;width:255px;height:553px;left:860px;top:100px;background:#151519;border:6px solid #343438;border-radius:32px;overflow:hidden;box-shadow:0 24px 70px #0008}
    .phone img{display:block;width:100%;height:100%;object-fit:cover}
    .phone.back{left:667px;top:160px;opacity:.65;width:222px;height:481px}
    .phone.front{z-index:3}.phone.detail{z-index:4}
    .label{position:absolute;left:72px;bottom:54px;font-size:10px;color:#8e8f99;letter-spacing:.03em}
    .steps{position:absolute;right:66px;bottom:45px;display:flex;gap:6px}.steps i{width:30px;height:2px;background:#333338}.steps i.on{background:#e8be62}
  </style></head><body>
    <div class="aura"></div><div class="ring"></div><div class="ring two"></div>
    <div class="brand"><img src="${base}/img/icon.svg">Sanctum<span>by RTensorLabs</span></div>
    <div class="copy"><div class="eyebrow"></div><h1></h1><div class="sub"></div></div>
    <div class="phone back"><img src="${base}/img/app-home-dark.webp"></div>
    <div class="phone front"><img src="${base}/img/app-chat-dark.webp"></div>
    <div class="phone detail"><img src="${base}/img/app-privacy-dark.webp"></div>
    <div class="label">Real app screens · Sample profiles and conversations</div>
    <div class="steps"><i></i><i></i><i></i></div>
  </body></html>`);
  await page.evaluate(async () => { await document.fonts.ready; await Promise.all([...document.images].map(image => image.decode())); });
  const update = t => {
    const chapter = Math.min(2, Math.floor(t / 4));
    const local = t % 4;
    const ease = Math.min(1, local / .8); const e = 1 - (1 - ease) ** 3;
    const endFade = local > 3.5 ? (4 - local) * 2 : 1;
    const headlines = ['Your circle.<br><em>One space.</em>', 'Every word.<br><em>Between you.</em>', 'Your privacy.<br><em>Your choice.</em>'];
    const subtitles = ['A home for your people, made personal with sigils.', 'Messages, calls and little everyday plans. End-to-end encrypted.', 'Clear identity checks, connection choices and message expiry.'];
    document.querySelector('h1').innerHTML = headlines[chapter];
    document.querySelector('.sub').textContent = subtitles[chapter];
    document.querySelector('.eyebrow').textContent = ['01 / Your people', '02 / Your conversations', '03 / Your peace of mind'][chapter];
    const copy = document.querySelector('.copy');
    copy.style.opacity = e * endFade; copy.style.transform = `translateY(${(1-e)*25}px)`;
    document.querySelector('.aura').style.transform = `translate(${Math.sin(t*.5)*40}px,${Math.cos(t*.5)*25}px) scale(${1+Math.sin(t*.4)*.1})`;
    document.querySelector('.ring').style.transform = `rotate(${-14+t*2}deg)`;
    document.querySelector('.two').style.transform = `rotate(${12-t*1.5}deg)`;
    const back=document.querySelector('.back'), front=document.querySelector('.front'), detail=document.querySelector('.detail');
    back.style.transform=`translate(${chapter===0?-25:chapter===1?-10:-35}px,${Math.sin(t*.7)*9}px) rotate(${-10+Math.sin(t*.4)*2}deg)`;
    front.style.transform=`translate(${chapter===0?0:chapter===1?-50:-160}px,${Math.cos(t*.6)*8}px) rotate(${chapter===2?-9:5}deg)`;
    front.style.opacity=chapter===2?.35:1;
    detail.style.opacity=chapter===2?e:0;
    detail.style.transform=`translate(${chapter===2?(1-e)*90:150}px,${Math.sin(t*.6)*7}px) rotate(4deg)`;
    document.querySelectorAll('.steps i').forEach((step,i)=>step.classList.toggle('on',chapter===i));
  };
  const encoder=spawn('ffmpeg',['-y','-hide_banner','-loglevel','error','-f','image2pipe','-vcodec','png','-framerate',String(fps),'-i','pipe:0','-an','-c:v','libx264','-preset','medium','-crf','24','-pix_fmt','yuv420p','-movflags','+faststart','-threads','2',path.join(output,'sanctum-film.mp4')],{stdio:['pipe','ignore','inherit']});
  const exit=once(encoder,'close');
  for(let frame=0;frame<fps*12;frame++){
    await page.evaluate(update,frame/fps);
    if(frame===48) await page.screenshot({path:path.join(output,'sanctum-film-poster.png')});
    const buffer=await page.screenshot({type:'png'});
    if(!encoder.stdin.write(buffer))await once(encoder.stdin,'drain');
    if(frame%96===0)console.log(`Rendered ${frame}/${fps*12} frames`);
  }
  encoder.stdin.end();
  const [code]=await exit;
  await browser.close();
  if(code!==0)throw new Error(`ffmpeg exited ${code}`);
  async function convert(args) {
    const process = spawn('ffmpeg', ['-y','-hide_banner','-loglevel','error', ...args], { stdio: 'inherit' });
    const [status] = await once(process, 'close');
    if (status !== 0) throw new Error(`ffmpeg conversion exited ${status}`);
  }
  await convert(['-i',path.join(output,'sanctum-film.mp4'),'-an','-c:v','libvpx-vp9','-b:v','0','-crf','36','-cpu-used','4','-row-mt','1','-threads','2',path.join(output,'sanctum-film.webm')]);
  await convert(['-i',path.join(output,'sanctum-film-poster.png'),'-c:v','libwebp','-quality','82',path.join(output,'sanctum-film-poster.webp')]);
  fs.unlinkSync(path.join(output,'sanctum-film-poster.png'));
  console.log('Film rendered.');
})().catch(error=>{console.error(error);process.exitCode=1;});
