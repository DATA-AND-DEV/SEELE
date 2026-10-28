// Vista real, ações simuladas: não homologa sinalização, voz ou vídeo.
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const { servir, respostas } = require('./telas.cjs');
const engines = require('./playwright.cjs');
(async () => {
  const server = servir();
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  try {
    for (const engine of ['chromium', 'webkit']) {
      const browser = await engines[engine]().launch({ headless: true });
      try {
        for (const width of [1280, 390, 320]) {
          const page = await browser.newPage({ viewport: { width, height: width > 600 ? 800 : 700 } });
          await page.addInitScript(table => {
            window.__TAURI__ = { core: { invoke: async cmd => table[cmd] ?? null },
              event: { listen: async () => () => {} },
              window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) } };
          }, respostas({ amigos: [], contatos_da_agenda: [], conhecidos: [], microfones: [], saidas: [], pacotes_no_cache: [], aceites_de_mods: [] }));
          await page.goto(`http://127.0.0.1:${server.address().port}`);
          await page.evaluate(() => {
            window.acciones = [];
            window.vistaPessoal = criarVistaDaConversaPessoal({ acoes: {
              voltar: () => vistaPessoal.fechar(), criar: nome => acciones.push(['criar', nome]),
              chamar: nome => { if(window.erroDaOrigem){const erro=new Error('Este endereço foi aposentado. Peça um novo convite.');erro.origemDoConviteInvalida=true;throw erro;} acciones.push(['chamar', nome]); }, copiar: link => acciones.push(['copiar', link]),
              atender: () => acciones.push(['atender']), recusar: () => acciones.push(['recusar']),
              encerrar: () => { acciones.push(['encerrar']); vistaPessoal.estado({ estado: 'encerrada' }); },
              enviarTexto: texto => { acciones.push(['texto', texto]); vistaPessoal.texto({ id: 1, de: 'eu', texto }); },
              microfone: () => {}, compartilhar: () => {}, podeCompartilhar: () => false,
            } });
            vistaPessoal.elemento.id = 'vista-pessoal-bancada';
            document.body.append(vistaPessoal.elemento);
            vistaPessoal.abrir({ porConvite: true, apelido: 'Alex' });
          });
          const root = page.locator('#vista-pessoal-bancada');
          await root.getByRole('button', { name: 'ENTRAR NA CONVERSA' }).click();
          assert.deepEqual(await page.evaluate(() => acciones[0]), ['chamar', 'Alex']);
          await page.evaluate(()=>window.erroDaOrigem=true);
          await root.getByRole('button', { name: 'ENTRAR NA CONVERSA' }).click();
          assert.ok((await root.innerText()).includes('Este endereço foi aposentado. Peça um novo convite.'));
          await page.evaluate(()=>{window.erroDaOrigem=false;vistaPessoal.limpar();});
          await page.evaluate(() => vistaPessoal.estado({ estado: 'tocando', apelido: '<img src=x onerror=alert(1)>' }));
          assert.equal(await root.locator('img:not(.pessoal-web-marca),[onerror]').count(), 0);
          await root.getByRole('button', { name: 'ATENDER', exact: true }).click();
          await page.evaluate(() => vistaPessoal.estado({ estado: 'dentro', apelido: 'Lia' }));
          assert.ok(await root.getByRole('button', { name: 'COMPARTILHAR TELA' }).isDisabled());
          const message = root.getByRole('textbox', { name: 'Mensagem', exact: true });
          await message.fill('Primeira linha\nSegunda linha');
          await root.getByRole('button', { name: 'ENVIAR', exact: true }).click();
          assert.equal(await message.inputValue(), '');
          assert.equal(await root.locator('.pessoal-web-conteudo').innerText(), 'Primeira linha\nSegunda linha');
          await page.evaluate(() => vistaPessoal.texto({ id: 1, de: 'eu', texto: 'duplicado' }));
          assert.equal(await root.locator('.pessoal-web-mensagem').count(), 1);
          await page.evaluate(() => vistaPessoal.texto({ id: 1, de: 'par', texto: 'A primeira mensagem da outra pessoa' }));
          assert.equal(await root.locator('.pessoal-web-mensagem').count(), 2);
          const layout = await root.evaluate(el => {
            const controls = [...el.querySelectorAll('button,textarea')].filter(e => e.getClientRects().length);
            return { overflow: el.scrollWidth > innerWidth, fora: controls.some(e => {
              const r = e.getBoundingClientRect(); return r.left < 0 || r.right > innerWidth + 1 || r.top < 0 || r.bottom > innerHeight + 1;
            }) };
          });
          assert.deepEqual(layout, { overflow: false, fora: false });
          assert.deepEqual(await root.locator('.pessoal-web-participante-nome').allTextContents(), ['Alex','Lia']);
          // Reabrir após um evento de estado sem identidade local não pode
          // apagar o próprio nome nem trocá-lo pelo nome remoto.
          await page.evaluate(() => vistaPessoal.abrir());
          assert.deepEqual(await root.locator('.pessoal-web-participante-nome').allTextContents(), ['Alex','Lia']);
          const targets=await root.locator('button:visible').evaluateAll(buttons=>buttons.map(b=>({w:b.getBoundingClientRect().width,h:b.getBoundingClientRect().height,icon:!!b.querySelector('svg')})));
          assert.ok(targets.every(b=>b.w>=44&&b.h>=44&&b.icon),JSON.stringify(targets));
          await page.evaluate(()=>vistaPessoal.microfone(true));
          assert.equal(await root.getByRole('button',{name:'DESLIGAR MICROFONE',exact:true}).getAttribute('aria-pressed'),'true');
          await page.evaluate(()=>vistaPessoal.microfone(false));
          if (process.env.SEELE_PREVIEW_DIR && width !== 320) {
            fs.mkdirSync(process.env.SEELE_PREVIEW_DIR, { recursive: true });
            await page.screenshot({ path: path.join(process.env.SEELE_PREVIEW_DIR, `${engine}-pessoal-${width}.png`) });
          }
          const leave=root.getByRole('button', { name: 'SAIR DA CONVERSA' });
          const box=await leave.boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();
          await page.evaluate(()=>vistaPessoal.estado({estado:'dentro',apelido:'Lia'}));
          await page.mouse.up();
          assert.ok(await message.isDisabled());
          await page.close();
        }
        console.log(`${engine}: vista pessoal 1280/390/320, convite, aceite, chat multiline, duplicação, saída e controles PASS`);
      } finally { await browser.close(); }
    }
  } finally { server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
