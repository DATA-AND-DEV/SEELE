// DOM/CSS reais, ações locais simuladas. Não homologa mídia ou teclado nativo:
// a janela reduzida representa o espaço disponível após abrir o teclado.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { servir, respostas } = require('./telas.cjs');
const motores = require('./playwright.cjs');
(async () => {
  const server = servir();
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    for (const motor of ['chromium', 'webkit']) {
      const browser = await motores[motor]().launch({ headless: true });
      try {
        const page = await browser.newPage();
        await page.addInitScript(tabela => {
          window.__TAURI__ = { core: { invoke: async cmd => tabela[cmd] ?? null }, event: { listen: async () => () => {} }, window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) } };
        }, respostas({ amigos: [], contatos_da_agenda: [], conhecidos: [], microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
        await page.goto(`http://127.0.0.1:${server.address().port}`);
        await page.evaluate(() => {
          document.documentElement.dataset.casca = 'celular';
          const noop = () => {};
          window.vista = criarVistaDaConversaPessoal({ acoes: { voltar: noop, criar: noop, chamar: noop, copiar: noop, atender: noop, recusar: noop, encerrar: noop, enviarTexto: noop, microfone: noop, compartilhar: noop, podeCompartilhar: () => true } });
          document.body.append(vista.elemento);
          vista.abrir({ apelido: 'Alex' });
        });
        for (const [width, height] of [[320,844],[390,844],[320,568],[390,568],[320,360],[390,360],[568,320],[844,390]]) {
          await page.setViewportSize({ width, height });
          for (const fase of ['livre','recebendo','tocando','dentro','tela','encerrada']) {
            await page.evaluate(fase => {
              vista.limpar();
              vista.estado({ estado: fase === 'tela' ? 'dentro' : fase, apelido: 'Lia' });
              if (fase === 'recebendo') vista.convite({ link: 'https://exemplo.test/?convite=abc' });
              if (['dentro','tela','encerrada'].includes(fase)) for (let i=0;i<20;i++) vista.texto({ id:i, de:'par', texto:'Mensagem comprida para testar a rolagem do histórico. '.repeat(3) });
              if (fase === 'tela') {
                const canvas = document.createElement('canvas'); canvas.width = 1920; canvas.height = 1080;
                const button = document.createElement('button'); button.textContent = 'AMPLIAR';
                vista.palco.append(canvas, button); vista.palco.hidden = false;
              }
            }, fase);
            if (process.env.SEELE_PREVIEW_DIR && fase === 'dentro' && [[390,844],[390,360],[844,390]].some(([w,h]) => w===width && h===height)) {
              fs.mkdirSync(process.env.SEELE_PREVIEW_DIR, { recursive:true });
              await page.screenshot({ path:path.join(process.env.SEELE_PREVIEW_DIR, `${motor}-pessoal-${width}x${height}.png`) });
            }
            const medidas = await page.evaluate(() => {
              const root = vista.elemento;
              const inside = r => r.left >= -1 && r.top >= -1 && r.right <= innerWidth+1 && r.bottom <= innerHeight+1;
              const controls = [...root.querySelectorAll('button,input,textarea')].filter(el => el.getClientRects().length);
              const clipped = controls.filter(el => {
                const r = el.getBoundingClientRect();
                if (!inside(r) || r.width < 43 || r.height < 43 || (el.tagName === 'BUTTON' && el.scrollWidth > el.clientWidth+1)) return true;
                for(let p=el.parentElement;p&&p!==root;p=p.parentElement) {
                  if (/hidden|auto|scroll/.test(getComputedStyle(p).overflow)) {
                    const b=p.getBoundingClientRect(); if(r.left < b.left-1 || r.right > b.right+1 || r.top < b.top-1 || r.bottom > b.bottom+1) return true;
                  }
                }
                return false;
              }).map(el => ({ min:getComputedStyle(el).minHeight,css:el.style.cssText,name:el.getAttribute('aria-label') || el.tagName, rect:el.getBoundingClientRect().toJSON(), parent:el.parentElement.getBoundingClientRect().toJSON() }));
              const rect = selector => { const r=root.querySelector(selector).getBoundingClientRect(); return {top:r.top,bottom:r.bottom,height:r.height}; };
              return { clipped, overflow: root.scrollWidth > innerWidth+1, body:rect('.pessoal-web-corpo'), stage:rect('.pessoal-web-palco'), chat:rect('.pessoal-web-chat'), foot:rect('.pessoal-web-rodape') };
            });
            const label = `${motor} ${width}x${height} ${fase}`;
            assert.deepEqual(medidas.clipped, [], `${label}: controles cortados ${JSON.stringify(medidas)}`);
            assert.equal(medidas.overflow, false, `${label}: rolagem horizontal`);
            if (['dentro','tela'].includes(fase)) {
              assert.ok(medidas.chat.height >= 96, `${label}: chat espremido`);
              assert.ok(medidas.stage.height > 50, `${label}: palco espremido`);
              assert.ok(medidas.chat.bottom <= medidas.foot.top+1, `${label}: chat sobre controles`);
              if (width < height) assert.ok(medidas.stage.height >= medidas.body.height*.25, `${label}: participantes concentrados no topo`);
            }
          }
        }
        await page.close();
        console.log(`${motor}: layout pessoal 320/390 × 844/568/360, paisagem 568×320/844×390; livre/convite/chamada/chat/tela/encerrada PASS`);
      } finally { await browser.close(); }
    }
  } finally { server.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
