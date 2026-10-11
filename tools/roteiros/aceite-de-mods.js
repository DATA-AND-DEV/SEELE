// A tela de aceite de MODs, apertada de verdade — ADR 0045.
//
// O que este roteiro prova, e que nenhum guarda de texto-fonte alcança: que uma
// recusa por MODs **abre o diálogo** em vez de virar linha vermelha, que a
// lista desenhada é a que veio no anúncio, e que ACEITAR grava o sim com a
// identidade certa e tenta entrar de novo, levando a credencial da tentativa.
//
// **E ele reprova quando não prova** (lote W3 da F5). Até então só `relatar`:
// as duas linhas do fim diziam `alvo=undefined` e `tentou entrar de novo:
// false` e a carga saía limpa, porque nada era exigido. O duble respondia
// `null` a `mods_instalados`, e o aceite, que desde a `ea75ac5d` confere o que
// está instalado antes de entrar (e desde o lote W só grava o sim depois disso),
// estourava em `.some` de `null` e parava ali — num `console.warn`, com o botão
// apagado. O produto estava certo; o roteiro é que não sabia mais responder à
// primeira pergunta do aceite. Cada `exigir` abaixo tem a reversão que o faz
// reprovar, no relatório do lote W3
// (`docs/superpowers/sdd/2026-10-08-0.16/F5/lote-w3-report.md`).
//
// Era o caminho inteiro que não existia: antes desta tela, o que a pessoa lia
// era uma frase terminando em «a tela para ler e aceitar esta lista ainda não
// existe neste app», e o servidor ficava inentrável. **Hoje** a recusa da
// tentativa mais nova abre o diálogo, e é isso que este roteiro exige; a frase
// continua em `frases.js` para o ramo que um dia chegar a ela, mas já diz
// «entre de novo neste servidor», e a recusa de uma tentativa que outra
// substituiu vai ao `seele.log` (lote W2).
//
// **O que ele não cobre**: a obtenção de um pacote que falta
// (`instalar_mod_do_catalogo`) e a desistência no meio dela. Aqui os pacotes do
// anúncio já estão na máquina, e quem prova o resto é a `ajustes-v013.cjs`. O
// aceite que rejeita por dentro é de `aceite-de-mods-que-falha.js`.

const ALVO = "203.0.113.9:8383";
const CONVITE = "convite-do-roteiro";

const ANUNCIO = {
  ModsNaoAceitos: {
    conjunto: "c0ffee1234567890",
    mods: [
      {
        id: "seele/placar",
        version: "1.4.0",
        hash: "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900",
        repo: "https://example.invalid/placar",
        reach: ["dom", "mensagens do servidor"],
        no_servidor: false,
      },
      {
        id: "ponte/retransmissor",
        version: "0.8.2",
        hash: "bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900aa11",
        repo: "https://example.invalid/ponte",
        reach: ["rede de saída"],
        no_servidor: true,
      },
    ],
  },
};
const { conjunto: CONJUNTO, mods: ANUNCIADOS } = ANUNCIO.ModsNaoAceitos;
// Uma lista sem aspas duplas: o aparelho lê o relato até a primeira, e uma frase
// de reprovação com `"` dentro sai cortada ali.
const lista = (textos) => `[${textos.join(" · ")}]`;

// O servidor recusa a entrada com a lista na mão.
SEELE_RESPOSTAS.connect = { __recusa: ANUNCIO };
// E esta máquina já disse sim a **outra** lista deste servidor.
SEELE_RESPOSTAS.aceite_de_mods = "uma-lista-diferente";
// **E já tem no disco os pacotes exatos do anúncio** — o que o aceite confere
// antes de gravar o sim (`obterOConjunto`, em `camada-mods.js`). Sem esta
// resposta o duble devolvia `null`, que o produto nunca devolve, e o aceite
// parava em `.some`. Só `id` e `hash` importam a essa conferência; com os dois
// iguais aos do anúncio nada é obtido, e o aceite segue direto.
SEELE_RESPOSTAS.mods_instalados = ANUNCIADOS.map(({ id, hash }) => ({ id, hash }));

ultimoAlvo = ALVO;
await conectar(ALVO, "eu", CONVITE);
await espera(300);

const primeira = window.__SEELE_ARGS.connect ?? {};
exigir(primeira.joinSecret === CONVITE,
  "a primeira tentativa não levou o convite do roteiro, e a volta que aceita não tem o que devolver: o roteiro não mede nada");

relatar(`diálogo: ${visivel("mods-aceite")}`);
exigir(visivel("mods-aceite") === "VISIVEL",
  `a recusa por MODs não abriu o diálogo de aceite (diálogo=${visivel("mods-aceite")}, ` +
    `linha vermelha=${visivel("boot-erro")}: «${document.getElementById("boot-erro").textContent.split("\n")[0]}»)`);

relatar(`linha vermelha da entrada: ${visivel("boot-erro")}`);
exigir(visivel("boot-erro") === "escondida",
  "a recusa por MODs abriu o diálogo e também escreveu a linha vermelha da entrada: " +
    `«${document.getElementById("boot-erro").textContent.split("\n")[0]}»`);

const itens = [...document.querySelectorAll("#mods-lista .mods-item")];
relatar(`itens desenhados: ${itens.length}`);
exigir(itens.length === ANUNCIADOS.length,
  `os itens desenhados não são os do anúncio: o diálogo desenhou ${itens.length}, e o anúncio trouxe ${ANUNCIADOS.length}`);
// **Item por item, na ordem do anúncio, e tudo o que a pessoa lê para decidir**
// (lote W3, rodada 1): quem é, de onde veio, o que alcança, e se roda na máquina
// de quem hospeda. Uma contagem não diz em qual item cada coisa está — a linha
// de «roda na máquina de quem hospeda» no MOD errado tem a mesma contagem.
ANUNCIADOS.forEach((mod, i) => {
  const item = itens[i];
  const nome = `o item ${i + 1} (${mod.id})`;
  const desenhado = {
    id: item.querySelector(".mods-id")?.textContent,
    versao: item.querySelector(".mods-versao")?.textContent,
    hash: item.querySelector(".mods-hash")?.title,
  };
  exigir(desenhado.id === mod.id && desenhado.versao === `versão ${mod.version}` && desenhado.hash === mod.hash,
    `o item ${i + 1} do diálogo é «${desenhado.id} · ${desenhado.versao} · ${desenhado.hash}», ` +
      `e o anúncio trouxe «${mod.id} · versão ${mod.version} · ${mod.hash}»`);

  // O repositório: o segundo `span` de `.mods-repo` (o primeiro é o rótulo).
  const repo = item.querySelector(".mods-repo span:not(.rotulo)")?.textContent;
  relatar(`${nome}: repositório ${repo}`);
  exigir(repo === mod.repo,
    `${nome} mostra o repositório «${repo}», e o anúncio trouxe «${mod.repo}»`);

  // O alcance, que o ADR 0045 manda a pessoa ler antes de aceitar: etiqueta por
  // etiqueta, na ordem do anúncio. Sem alcance a tela escreve «nada declarado»,
  // que parece uma resposta — e é por isso que ele é lido aqui.
  const alcance = [...item.querySelectorAll(".mods-alcances .mods-etiqueta")].map((el) => el.textContent);
  relatar(`${nome}: alcance ${lista(alcance)}`);
  exigir(JSON.stringify(alcance) === JSON.stringify(mod.reach),
    `${nome} mostra o alcance ${lista(alcance)}, e o anúncio trouxe ${lista(mod.reach)}`);

  // «Roda na máquina de quem hospeda», **no item certo**: só no que tem
  // `no_servidor`, e não nos outros.
  const naMaquina = item.querySelectorAll(".mods-servidor").length;
  relatar(`${nome}: linhas de «roda na máquina de quem hospeda» ${naMaquina}`);
  exigir(naMaquina === (mod.no_servidor ? 1 : 0),
    `${nome} ${naMaquina > 0 ? "traz" : "não traz"} a linha «roda na máquina de quem hospeda», ` +
      `e o anúncio ${mod.no_servidor ? "marca" : "não marca"} no_servidor nele`);
});
const contagem = document.getElementById("mods-contagem").textContent;
relatar(`contagem: ${contagem}`);
exigir(contagem === `${ANUNCIADOS.length} MODS`,
  `a contagem do diálogo diz «${contagem}», e o anúncio trouxe ${ANUNCIADOS.length}`);
relatar(`aviso de lista mudada: ${visivel("mods-mudou")}`);
exigir(visivel("mods-mudou") === "VISIVEL",
  "esta máquina já tinha dito sim a outra lista deste servidor, e o diálogo não avisou que a lista mudou");

// Aceitar grava e tenta entrar de novo. Desta vez o servidor deixa.
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS.length = 0;
delete window.__SEELE_ARGS.aceitar_mods;
document.getElementById("mods-aceitar").click();
// Até a volta tentar entrar, e não um tempo fixo: quem para no meio não pode
// parecer quem só demorou.
await ate(() => window.__SEELE_CHAMADAS.includes("connect"), 1500);
await espera(100);

// A sequência que o roteiro viu, que é o que diz **onde** o fluxo parou: até o
// `connect` da volta, e não o resto que a sessão chama depois de entrar.
const chamadas = window.__SEELE_CHAMADAS;
const corte = chamadas.indexOf("connect");
const sequencia = (corte < 0 ? chamadas : chamadas.slice(0, corte + 1)).join(" → ") || "nenhuma";
const gravado = window.__SEELE_ARGS.aceitar_mods;
relatar(`a sequência depois de ACEITAR: ${sequencia}`);
relatar(`gravou o sim: alvo=${gravado?.alvo} conjunto=${gravado?.conjunto}`);
exigir(gravado !== undefined,
  `ACEITAR não gravou o sim: aceitar_mods nunca foi chamado (a sequência foi: ${sequencia})`);
exigir(gravado.alvo === ALVO,
  `ACEITAR gravou o sim sem o alvo da tentativa: alvo=${gravado.alvo}, e a tentativa era a ${ALVO}`);
exigir(gravado.conjunto === CONJUNTO,
  `ACEITAR gravou o sim de outra lista: conjunto=${gravado.conjunto}, e o anúncio trouxe ${CONJUNTO}`);

const voltou = window.__SEELE_CHAMADAS.includes("connect");
relatar(`tentou entrar de novo: ${voltou}`);
exigir(voltou,
  `ACEITAR gravou o sim e não tentou entrar de novo: connect não foi chamado (a sequência foi: ${sequencia})`);
// **O sim já está em disco quando a volta tenta entrar.** A volta passa pelo
// mesmo ponto que recusou a tentativa anterior, e só passa por ele se o aceite
// já foi gravado: um `connect` antes do `aceitar_mods` bate de novo na pergunta.
const iGravou = chamadas.indexOf("aceitar_mods");
relatar(`o sim antes da volta: aceitar_mods na posição ${iGravou}, connect na ${corte}`);
exigir(iGravou >= 0 && iGravou < corte,
  "a volta tentou entrar antes de o sim ser gravado, e sem o aceite em disco ela bate de novo no ponto " +
    `que recusou a tentativa anterior (a sequência foi: ${sequencia})`);
const volta = window.__SEELE_ARGS.connect;
relatar(`a volta: servidor=${volta.server} convite=${volta.joinSecret ?? "nenhum"}`);
exigir(volta.server === ALVO,
  `a volta foi a ${volta.server}, e a tentativa que o servidor recusou era a ${ALVO}: ` +
    "aceitar os MODs de um servidor e entrar em outro");
exigir(volta.joinSecret === CONVITE,
  `a volta que aceita saiu sem a credencial da tentativa (joinSecret=${String(volta.joinSecret ?? null)}): ` +
    "num servidor fechado por convite isso é CREDENCIAL RECUSADA sobre um convite que a recusa dos MODs não gastou");

relatar(`diálogo depois: ${visivel("mods-aceite")}`);
exigir(visivel("mods-aceite") === "escondida",
  `o diálogo de aceite continuou aberto depois de ACEITAR (diálogo=${visivel("mods-aceite")})`);
