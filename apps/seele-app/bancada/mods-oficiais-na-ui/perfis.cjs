// PERFIS 4.0.0: a entrada «Perfis», a página do diretório, o editor do próprio
// perfil, e o cartão da pessoa (`pessoa.cartao`) na lista de pessoas.
"use strict";

module.exports = async function roteiro(ctx) {
  const { aba, confere, defeito, esperar } = ctx;

  // 1. A superfície monta: a entrada na navegação.
  const entrada = await esperar(aba, () => {
    const item = [...document.querySelectorAll("#lista-mods-navegacao > *")].find((n) => /Perfis/.test(n.textContent));
    return { pronto: Boolean(item) && window.__visivel(item) };
  }, null, 15_000);
  confere("perfis", entrada.pronto, "a entrada «Perfis» não montou na navegação");
  await ctx.captura("1-entrada-na-navegacao");

  // 2. O gesto: abrir o diretório. A página do MOD monta, com as pessoas que o
  //    servidor tem (a metade de servidor respondeu: 1 pessoa, eu).
  await aba.locator("#lista-mods-navegacao").getByText("Perfis").first().click();
  const diretorio = await esperar(aba, () => {
    const pagina = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    return { pronto: Boolean(pagina) && /1 pessoa\(s\) neste servidor/.test(pagina.innerText), texto: pagina?.innerText?.slice(0, 200) ?? null };
  }, null, 15_000);
  confere("perfis", diretorio.pronto, `o diretório não montou com a resposta do servidor: ${JSON.stringify(diretorio.texto)}`);
  confere("perfis", /PERFIS DESTE SERVIDOR/.test(diretorio.texto ?? "") && /dona · você/.test(diretorio.texto ?? ""), `o diretório não lista a pessoa: ${diretorio.texto}`);
  await ctx.captura("2-diretorio");

  // 3. O editor: um diálogo com a prévia do cartão. O nome é escrito tecla a
  //    tecla, e GRAVAR grava no servidor (o formulário manda os valores do DOM).
  await aba.getByText("EDITAR MEU PERFIL", { exact: true }).first().click();
  const dialogo = aba.locator(".dialogo-de-mod").filter({ hasText: "EDITAR MEU PERFIL" }).first();
  await dialogo.waitFor({ state: "visible", timeout: 15_000 });
  await aba.waitForTimeout(800);
  await dialogo.getByLabel("NOME EXIBIDO").click();
  await aba.keyboard.type("Dona de Teste", { delay: 15 });
  await aba.waitForTimeout(500);
  confere("perfis", /Dona de Teste/.test(await dialogo.innerText()), "a prévia do cartão não mostra o nome escrito");
  await ctx.captura("3-editor");
  await dialogo.getByText("GRAVAR", { exact: true }).click();

  // 4. A resposta volta à tela: o aviso do MOD, o diretório com o perfil
  //    gravado e o cartão da lista de pessoas com o nome ao lado do apelido.
  const gravado = await esperar(aba, () => {
    const pagina = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    const roster = document.getElementById("lista-roster").innerText;
    return {
      pronto: Boolean(pagina) && /Dona de Teste/.test(pagina.innerText) && /Dona de Teste/.test(roster) && /Perfil gravado/.test(document.body.innerText),
      pagina: pagina?.innerText?.slice(0, 300) ?? null, roster: roster.slice(0, 200),
    };
  }, null, 20_000);
  confere("perfis", gravado.pronto, `a gravação não voltou à tela: ${JSON.stringify(gravado)}`);
  confere("perfis", /dona/.test(gravado.roster ?? ""), `o apelido do servidor sumiu do cartão: ${gravado.roster}`);
  // O aviso do MOD cabe numa linha legível (não numa coluna de uma letra).
  const aviso = await aba.evaluate(() => {
    const r = document.getElementById("avisos-de-mod").getBoundingClientRect();
    return { largura: Math.round(r.width), altura: Math.round(r.height) };
  });
  defeito("perfis", aviso.largura === 0 || (aviso.largura >= 80 && aviso.altura <= 64),
    `o aviso «Perfil gravado.» (#avisos-de-mod, \`position: fixed\` sem largura, com \`overflow-wrap: anywhere\` no aviso) encolhe para ${aviso.largura}×${aviso.altura} px, uma letra por linha (ui/mods-superficies.css)`);
  await ctx.captura("4-gravado");

  // 5. O perfil persiste no servidor: fecha o editor (limpo: sem pergunta), e a
  //    página lê de novo (VOLTAR e abrir de novo).
  await dialogo.getByText("FECHAR", { exact: true }).first().click();
  await dialogo.waitFor({ state: "hidden", timeout: 10_000 });
  await aba.locator(".pagina-de-mod").getByText("VOLTAR", { exact: true }).first().click();
  await aba.waitForTimeout(500);
  await aba.locator("#lista-mods-navegacao").getByText("Perfis").first().click();
  const lido = await esperar(aba, () => {
    const pagina = [...document.querySelectorAll(".pagina-de-mod")].find(window.__visivel);
    return { pronto: Boolean(pagina) && /Dona de Teste/.test(pagina.innerText), texto: pagina?.innerText?.slice(0, 300) ?? null };
  }, null, 15_000);
  confere("perfis", lido.pronto, `o perfil gravado não voltou do servidor: ${JSON.stringify(lido.texto)}`);
  await ctx.captura("5-lido-do-servidor");

  // 6. A SONDA do defeito: dois campos de texto na mesma edição. O editor manda
  //    a prévia (`atualizar` por chave) sem reenviar o formulário, e a UI
  //    reaplica o `valor` antigo nos campos sem foco, apagando o que a pessoa
  //    acabou de escrever no campo anterior (ui/mods-regiao.js, `atualizarCampo`).
  await aba.getByText("EDITAR MEU PERFIL", { exact: true }).first().click();
  await dialogo.waitFor({ state: "visible", timeout: 15_000 });
  await aba.waitForTimeout(800);
  await dialogo.getByLabel("PRONOMES").click();
  await aba.keyboard.type("ela/dela", { delay: 15 });
  await dialogo.getByLabel("STATUS").click();
  await aba.keyboard.type("na bancada", { delay: 15 });
  await aba.waitForTimeout(800);
  const campos = await aba.evaluate(() => [...document.querySelectorAll(".dialogo-de-mod input[type=text]")].map((i) => i.value));
  defeito("perfis", campos.includes("ela/dela") && campos.includes("na bancada"),
    `escrever num segundo campo apaga o primeiro no editor do PERFIS (campos: ${JSON.stringify(campos)}); GRAVAR então manda o formulário sem eles (ui/mods-regiao.js, \`atualizarCampo\` reescreve o \`valor\` do campo sem foco)`);
  await ctx.captura("6-dois-campos-editados");

  // 7. O guarda de regressão: GRAVAR com os dois campos, reabrir o editor, e os
  //    DOIS (e o nome gravado antes) voltam do servidor.
  await dialogo.getByText("GRAVAR", { exact: true }).click();
  // Limpo de verdade: a prévia volta a dizer «como está gravado» (o servidor avisou e o MOD releu).
  const limpo = await esperar(aba, () => ({ pronto: /como está gravado/.test([...document.querySelectorAll(".dialogo-de-mod")].map((n) => n.innerText).join(" ")) }), null, 15_000);
  confere("perfis", limpo.pronto, "depois de GRAVAR os dois campos o editor não voltou a «como está gravado»");
  await ctx.captura("7a-depois-de-gravar-os-dois");
  await dialogo.getByText("FECHAR", { exact: true }).first().click();
  await aba.waitForTimeout(800);
  await ctx.captura("7b-depois-de-fechar");
  await dialogo.waitFor({ state: "hidden", timeout: 10_000 });
  await aba.getByText("EDITAR MEU PERFIL", { exact: true }).first().click();
  await dialogo.waitFor({ state: "visible", timeout: 15_000 });
  await aba.waitForTimeout(1000);
  const reaberto = await aba.evaluate(() => [...document.querySelectorAll(".dialogo-de-mod input[type=text]")].map((i) => i.value));
  confere("perfis", reaberto[0] === "Dona de Teste" && reaberto[1] === "ela/dela" && reaberto[2] === "na bancada",
    `reabrir o editor não devolve os três valores do servidor: ${JSON.stringify(reaberto)}`);
  await ctx.captura("7-editor-reaberto-com-os-valores-do-servidor");
};
