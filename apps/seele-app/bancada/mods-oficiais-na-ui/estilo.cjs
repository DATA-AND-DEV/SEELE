// ESTILO 4.0.0: a entrada «Aparência do servidor» (quem administra), o diálogo
// do MOD, a PRÉVIA, a publicação e o `tema` aplicado na janela.
"use strict";

const VARIAVEIS = ["--seele-negro-absoluto", "--seele-negro-painel", "--seele-osso"];

module.exports = async function roteiro(ctx) {
  const { aba, confere, esperar } = ctx;
  const cores = () => aba.evaluate((vs) => vs.map((v) => getComputedStyle(document.getElementById("tela-sessao")).getPropertyValue(v).trim()), VARIAVEIS);
  const visivel = (seletor) => aba.evaluate((s) => [...document.querySelectorAll(s)].some(window.__visivel), seletor);
  const dialogo = () => aba.locator(".dialogo-de-mod").filter({ hasText: "PUBLICAR PARA O SERVIDOR" }).first();

  // 1. A superfície monta: a entrada na navegação, visível para quem administra.
  const entrada = await esperar(aba, () => {
    const itens = [...document.querySelectorAll("#lista-mods-navegacao > *")];
    const item = itens.find((n) => /Aparência do servidor/.test(n.textContent));
    return { pronto: Boolean(item) && window.__visivel(item), n: itens.length };
  }, null, 15_000);
  confere("estilo", entrada.pronto, `a entrada «Aparência do servidor» não montou na navegação: ${JSON.stringify(entrada)}`);
  await ctx.captura("1-entrada-na-navegacao");
  const antes = await cores();

  // 2. O gesto: abrir a aparência. O diálogo do MOD monta, com o texto dele.
  await aba.locator("#lista-mods-navegacao").getByText("Aparência do servidor").first().click();
  await dialogo().waitFor({ state: "visible", timeout: 15_000 });
  const montado = await aba.evaluate(() => {
    const d = [...document.querySelectorAll(".dialogo-de-mod")].find((n) => window.__visivel(n) && /PUBLICAR PARA O SERVIDOR/.test(n.innerText));
    const r = d.getBoundingClientRect();
    return { texto: d.innerText, dentro: r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight, mod: d.dataset.mod ?? null };
  });
  confere("estilo", /APARÊNCIA DO SERVIDOR/.test(montado.texto) && /COMO A CONVERSA FICA/.test(montado.texto), `o diálogo não traz a amostra: ${montado.texto.slice(0, 120)}`);
  confere("estilo", montado.dentro, "o diálogo do ESTILO passa da janela");
  await ctx.captura("2-dialogo");

  // 3. Um conjunto pronto: a PRÉVIA aparece (o selo do SEELE) e a janela já veste o tema.
  await dialogo().getByText("CONJUNTOS", { exact: true }).first().click();
  await aba.waitForTimeout(500);
  await dialogo().getByText("USAR ESTE", { exact: true }).nth(1).click(); // PAPEL: claro
  const previa = await esperar(aba, (vs) => ({
    pronto: [...document.querySelectorAll(".selo-da-previa")].some(window.__visivel),
    cores: vs.map((v) => getComputedStyle(document.getElementById("tela-sessao")).getPropertyValue(v).trim()),
  }), VARIAVEIS, 10_000);
  confere("estilo", previa.pronto, "o selo «PRÉVIA NÃO PUBLICADA» não apareceu");
  confere("estilo", JSON.stringify(previa.cores) !== JSON.stringify(antes), `a prévia não mudou as cores da janela: ${JSON.stringify(previa.cores)}`);
  await ctx.captura("3-previa");

  // 4. PUBLICAR: a metade de servidor guarda, e o `tema` do servidor é aplicado
  //    na janela; o selo some, e a resposta do SEELE volta ao diálogo.
  await dialogo().getByText("PUBLICAR PARA O SERVIDOR", { exact: true }).click();
  const publicado = await esperar(aba, (vs) => ({
    pronto: ![...document.querySelectorAll(".selo-da-previa")].some(window.__visivel) &&
      /publicado para todo mundo neste servidor[\s\S]*Desenhado nesta sessão: .*sem serifa/.test([...document.querySelectorAll(".dialogo-de-mod")].map((n) => n.innerText).join(" ")),
    cores: vs.map((v) => getComputedStyle(document.getElementById("tela-sessao")).getPropertyValue(v).trim()),
    texto: [...document.querySelectorAll(".dialogo-de-mod")].map((n) => n.innerText).join(" ").slice(-500),
  }), VARIAVEIS, 15_000);
  confere("estilo", publicado.pronto, `publicar não voltou «aplicado» ao diálogo: ${JSON.stringify(publicado.texto)}`);
  confere("estilo", JSON.stringify(publicado.cores) === JSON.stringify(previa.cores) && JSON.stringify(publicado.cores) !== JSON.stringify(antes), `o tema publicado não ficou na janela: ${JSON.stringify(publicado.cores)}`);
  confere("estilo", /PUBLICADO/.test(publicado.texto), `o botão não virou PUBLICADO: ${publicado.texto}`);
  await ctx.captura("4-publicado");

  // 5. Fecha o diálogo; a janela segue com o tema do servidor.
  await aba.locator(".dialogo-de-mod").getByText("FECHAR", { exact: true }).first().click();
  await aba.waitForTimeout(500);
  confere("estilo", !(await visivel(".dialogo-de-mod")), "o diálogo não fechou");
  confere("estilo", JSON.stringify(await cores()) === JSON.stringify(publicado.cores), "o tema do servidor saiu da janela ao fechar o diálogo");
  await ctx.captura("5-janela-com-o-tema");
};
