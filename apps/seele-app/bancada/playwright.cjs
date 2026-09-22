// De onde o Playwright vem, e **nunca** da pasta pessoal de alguém.
//
// # Por que este arquivo existe
//
// Porque a resposta estava escrita cinco vezes, e nas cinco ela terminava em
// `/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright` — o caminho
// absoluto de um repositório vizinho na máquina de uma pessoa. Num checkout
// limpo, ou na máquina de qualquer outra pessoa, toda bancada morre com um
// `MODULE_NOT_FOUND` apontando para uma pasta que não existe.
//
// A revisão da v15 nomeia isso no R07: «tornar checks e jornadas de navegador
// reproduzíveis numa máquina limpa», «checkout limpo executa as bancadas sem
// depender de diretórios pessoais».
//
// # A ordem, e por que ela é esta
//
//   1. `PLAYWRIGHT`, quando a variável está posta. É o que o CI usa, e é o que
//      deixa quem já tem uma instalação apontá-la sem copiar nada;
//   2. a resolução normal do Node a partir **deste** arquivo — `node_modules`
//      deste repositório, ou de qualquer pai dele. É o que funciona depois de
//      um `npm install` aqui;
//   3. nada. E «nada» é uma frase que diz o que fazer, não um `MODULE_NOT_FOUND`
//      com o caminho de outra pessoa dentro.
//
// O caminho do repositório vizinho **não** entra como quarto recurso. Um recuo
// que funciona numa máquina só é o que fez o defeito durar: nela tudo passa, e a
// bancada parece coberta.

/** O `chromium` do Playwright, ou um erro que diz como consegui-lo. */
function chromium() {
  const escolhido = process.env.PLAYWRIGHT;
  if (escolhido) {
    return require(escolhido).chromium;
  }
  try {
    return require("playwright").chromium;
  } catch (falha) {
    if (falha?.code !== "MODULE_NOT_FOUND") throw falha;
    throw new Error(
      "esta bancada precisa do Playwright e não o encontrou.\n" +
        "  · instale aqui:  npm install --no-save playwright && npx playwright install chromium\n" +
        "  · ou aponte um:  PLAYWRIGHT=/caminho/para/node_modules/playwright node <bancada>\n" +
        "Nenhum caminho de máquina nenhuma é tentado: um recuo que funciona só " +
        "numa é o que faz a bancada parecer coberta quando não está.",
    );
  }
}

module.exports = { chromium };
