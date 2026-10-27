// MESA 4.0.0: a entrada «Mesa», a página (sem mesa → criar → cinco abas), o
// cabeçalho do canal (`canal.cabecalho`), e um gesto que vai ao servidor e volta
// (rolar os dados, que o servidor decide, e o Registro que ele guarda).
"use strict";

module.exports = async function roteiro(ctx) {
  const { aba, confere, esperar } = ctx;
  const pagina = () => aba.evaluate(() => {
    const p = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    return p ? p.innerText : null;
  });

  // 1. A superfície monta: a entrada na navegação.
  const entrada = await esperar(aba, () => {
    const item = [...document.querySelectorAll("#lista-mods-navegacao > *")].find((n) => /Mesa/.test(n.textContent));
    return { pronto: Boolean(item) && window.__visivel(item) };
  }, null, 15_000);
  confere("mesa", entrada.pronto, "a entrada «Mesa» não montou na navegação");
  await ctx.captura("1-entrada-na-navegacao");

  // 2. O gesto: abrir. Sem mesa neste canal, a página diz como criar.
  await aba.locator("#lista-mods-navegacao").getByText("Mesa").first().click();
  const vazia = await esperar(aba, async () => ({ pronto: /Nenhuma mesa neste canal/.test([...document.querySelectorAll(".pagina-de-mod")].filter(window.__visivel).map((n) => n.innerText).join(" ")) }), null, 15_000);
  confere("mesa", vazia.pronto, "a página da mesa não montou com «Nenhuma mesa neste canal»");
  await ctx.captura("2-sem-mesa");

  // 3. Criar a mesa: o formulário do MOD, o servidor guarda a campanha.
  await aba.getByText("CRIAR MESA", { exact: true }).first().click();
  const dialogo = aba.locator(".dialogo-de-mod").filter({ hasText: "CRIAR A MESA" }).first();
  await dialogo.waitFor({ state: "visible", timeout: 15_000 });
  await aba.waitForTimeout(600);
  await dialogo.getByLabel("NOME DA MESA").click();
  await aba.keyboard.type("Mesa de Teste", { delay: 15 });
  await ctx.captura("3-criar-a-mesa");
  await dialogo.getByText("CRIAR MESA", { exact: true }).click();
  const criada = await esperar(aba, () => {
    const p = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    const t = p?.innerText ?? "";
    return { pronto: /MESA DE TESTE/.test(t) && /TABULEIRO/.test(t) && /REGISTRO/.test(t), texto: t.slice(0, 300) };
  }, null, 20_000);
  confere("mesa", criada.pronto, `a mesa criada não voltou do servidor com as abas: ${JSON.stringify(criada.texto)}`);
  confere("mesa", /Mestre dona/.test(criada.texto ?? ""), `a página não diz quem mestra: ${criada.texto}`);
  const aviso = await aba.evaluate(() => {
    const r = document.getElementById("avisos-de-mod").getBoundingClientRect();
    return { largura: Math.round(r.width), altura: Math.round(r.height) };
  });
  ctx.defeito("mesa", aviso.largura === 0 || (aviso.largura >= 80 && aviso.altura <= 64),
    `o aviso «Mesa criada.» (#avisos-de-mod) encolhe para ${aviso.largura}×${aviso.altura} px, uma letra por linha (ui/mods-superficies.css; o mesmo do PERFIS)`);
  await ctx.captura("4-mesa-criada");

  // 4. Rolar os dados: o servidor rola, e o Registro (outra aba) guarda e mostra.
  await aba.getByText("ROLAR", { exact: true }).first().click();
  await aba.waitForTimeout(1500);
  await aba.locator(".pagina-de-mod").getByText("REGISTRO", { exact: true }).first().click();
  const registro = await esperar(aba, () => {
    const p = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    const t = p?.innerText ?? "";
    return { pronto: /1d20/i.test(t.split("REGISTRO").slice(-1)[0] ?? "") || /rol/i.test(t.split("REGISTRO").slice(-1)[0] ?? ""), texto: t.slice(-300) };
  }, null, 15_000);
  confere("mesa", registro.pronto, `o Registro não mostra a rolagem que o servidor decidiu: ${JSON.stringify(registro.texto)}`);
  await ctx.captura("5-registro-com-a-rolagem");

  // 5. O cabeçalho do canal (`canal.cabecalho`): fechada a página, o canal
  //    mostra a mesa e o gesto de abrir de novo.
  await aba.locator(".pagina-de-mod").getByText("VOLTAR", { exact: true }).first().click();
  await aba.waitForTimeout(800);
  const cabecalho = await esperar(aba, () => {
    const barra = document.querySelector("#vista-conversa .linha-barra");
    return { pronto: Boolean(barra) && window.__visivel(barra) && /MESA · Mesa de Teste/.test(barra.innerText), texto: barra?.innerText ?? null };
  }, null, 15_000);
  confere("mesa", cabecalho.pronto, `o cabeçalho do canal não traz a mesa: ${JSON.stringify(cabecalho.texto)}`);
  await ctx.captura("6-cabecalho-do-canal");
  await aba.locator("#vista-conversa .linha-barra").getByText("ABRIR A MESA").first().click();
  const reaberta = await esperar(aba, () => {
    const p = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    return { pronto: /MESA DE TESTE/.test(p?.innerText ?? ""), texto: p?.innerText?.slice(0, 120) ?? null };
  }, null, 15_000);
  confere("mesa", reaberta.pronto, `ABRIR A MESA, no cabeçalho, não reabriu a página: ${JSON.stringify(reaberta.texto)}`);
  void pagina;
};
