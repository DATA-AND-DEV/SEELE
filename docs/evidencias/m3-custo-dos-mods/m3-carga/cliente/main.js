// M3 da 0.16 — o MOD de carga sintético da API 6 (tarefa 11 do plano da F3).
//
// Não é um MOD de produto, e não vai ao indexador: a preparação
// (`gerar-carga.py`) escreve uma variante por cenário, com `CENARIO` abaixo, e
// a põe no cache por hash da medição. Cada cenário exercita os pontos da API 6
// **no teto declarado** — o orçamento de cada perfil (o cartão, 64 nós; a
// superfície, 4096), o teto de folhas (3), o do aviso (1 KiB, 4 por segundo) —,
// para a M3 medir o custo que a 6 promete, e não o de um MOD comportado.
//
// | cenário   | o que fica de pé                                                     |
// |-----------|----------------------------------------------------------------------|
// | regioes   | as oito regiões ocupadas ao mesmo tempo, cada uma no teto do cartão  |
// | capas     | 7 capas animadas (a chamada e a barra) e 20 paradas na lista         |
// | folhas    | três folhas empilhadas, cada uma perto do teto da superfície (3980 nós, em partes) |
// | avisos    | um pedido a cada 250 ms, cada um com um aviso de 1 KiB               |
// | dialogo   | um diálogo modal alto e largo (o jitter é lido do `seele.log`)       |
// | pagina    | uma página imersiva (idem)                                           |
// | palco     | o `chamada.palco` com uma `tela` de 1024 × 768 (o lado máximo) e as peças que cabem numa mensagem |
// | tudo      | regioes + capas + folhas + avisos + palco                            |
//
// O que cada cenário montou vai ao `seele.log` pela recusa e pelo aviso do
// produto; o roteiro confere o desenho pela acessibilidade.

const CENARIO = "__CENARIO__";
const EU = "m3/carga";
const SEM_CANAL = 0;

/** Linhas de texto: cada uma custa dois nós (o elemento e o texto). */
const linhas = (n, prefixo) =>
  Array.from({ length: n }, (_, i) => ({ forma: "texto", chave: `l${i}`, dentro: `${prefixo} · linha ${i + 1}` }));

/** O teto do cartão: 64 nós, 32 linhas. */
const NO_TETO_DO_CARTAO = (prefixo) => linhas(32, prefixo);

const quer = (nome) => CENARIO === nome || (CENARIO === "tudo" && ["regioes", "capas", "folhas", "avisos", "palco"].includes(nome));

async function tentar(o_que, fazer) {
  try {
    return await fazer();
  } catch (erro) {
    console.warn(`m3/carga: ${o_que} — ${erro?.erro ?? erro?.message ?? erro}`);
    return null;
  }
}

// ------------------------------------------------------------- as regiões
async function ocuparAsRegioes(retrato) {
  const canal = retrato?.canais?.find((c) => c.aberto)?.id ?? retrato?.canais?.[0]?.id ?? null;
  const semAlvo = ["redes", "operador.barra", "compositor", "status.barra", "pessoas.lista"];
  for (const ponto of semAlvo) {
    await tentar(`região ${ponto}`, () => SeeleUI.contribuicoes.registrar({
      ponto, modo: "adicionar", prioridade: 0, conteudo: NO_TETO_DO_CARTAO(ponto),
    }));
  }
  if (canal !== null) {
    await tentar("região canal.cabecalho", () => SeeleUI.contribuicoes.registrar({
      ponto: "canal.cabecalho", modo: "adicionar", alvo: String(canal), prioridade: 0,
      conteudo: NO_TETO_DO_CARTAO("canal.cabecalho"),
    }));
  }
  // Uma entrada na navegação, que é um botão do produto.
  await tentar("região servidor.navegacao", () => SeeleUI.contribuicoes.registrar({
    ponto: "servidor.navegacao", modo: "adicionar", prioridade: 0,
    rotulo: "Carga M3", nomeAcessivel: "Carga M3", acaoPrincipal: "abrir",
    conteudo: [{ forma: "texto", chave: "c", dentro: "Carga M3" }],
  }));
  // Sem alvo, o acréscimo vale para **toda** mensagem: com a conversa cheia
  // (o roteiro posta 30), são 30 montagens no teto do cartão.
  await tentar("região conversa.mensagem", () => SeeleUI.contribuicoes.registrar({
    ponto: "conversa.mensagem", modo: "adicionar", prioridade: 0,
    conteudo: NO_TETO_DO_CARTAO("conversa.mensagem"),
  }));
}

// -------------------------------------------------------------- as capas
const capasRegistradas = new Set();
async function vestirAsCapas() {
  const retrato = await tentar("retrato", () => SeeleUI.retrato());
  if (!retrato) return;
  const minha = retrato.salas?.find((s) => s.minha);
  // As animadas: quem está na sala (até 6) e quem usa (a barra do operador).
  const animadas = new Set([retrato.eu, ...(minha?.pessoas ?? [])].filter((id) => id !== null).slice(0, 7));
  const paradas = (retrato.pessoas ?? []).map((p) => p.id).filter((id) => !animadas.has(id)).slice(0, 20);
  for (const [ids, capa] of [[animadas, "animada"], [paradas, "parada"]]) {
    for (const id of ids) {
      if (capasRegistradas.has(id)) continue;
      capasRegistradas.add(id);
      await tentar(`capa de ${id}`, () => SeeleUI.contribuicoes.registrar({
        ponto: "pessoa.capa", modo: "substituir", alvo: String(id), prioridade: 10,
        conteudo: { doServidor: { canal: SEM_CANAL, pedido: { capa }, campo: "bytes" } },
      }));
      // Na lista, a capa aparece parada quando o cartão a cita (PERFIS R1).
      if (capa === "parada") {
        await tentar(`cartão de ${id}`, () => SeeleUI.contribuicoes.registrar({
          ponto: "pessoa.cartao", modo: "substituir", alvo: String(id), prioridade: 10,
          conteudo: [{ forma: "caixa", chave: "cartao", fundoDeMidia: { daPessoa: "capa" },
            dentro: [{ forma: "texto", chave: "n", dentro: `pessoa ${id}` }] }],
        }));
      }
    }
  }
}

/**
 * **A montagem por partes** (rascunho §12): uma mensagem tem 12 KiB, e uma
 * superfície perto do teto (4096 nós) não cabe numa. Um esqueleto de caixas,
 * e cada caixa trocada por `atualizar` com 99 linhas (~6 KB, 199 nós).
 */
async function montarEmPartes(superficie, prefixo, partes) {
  await tentar(`esqueleto de ${prefixo}`, () => superficie.montar(
    Array.from({ length: partes }, (_, i) => ({ forma: "caixa", chave: `b${i}`, dentro: [] })),
  ));
  for (let i = 0; i < partes; i += 1) {
    await tentar(`parte ${i} de ${prefixo}`, () => superficie.atualizar(`b${i}`,
      { forma: "caixa", chave: `b${i}`, dentro: linhas(99, `${prefixo} · parte ${i}`) }));
  }
}

// -------------------------------------------------------------- as folhas
async function empilharAsFolhas() {
  for (let n = 1; n <= 3; n += 1) {
    const folha = await tentar(`folha ${n}`, () => SeeleUI.superficies.criar({
      id: `folha-${n}`, tipo: "folha", titulo: `Carga M3 · folha ${n}`,
    }));
    if (!folha) continue;
    // Perto do teto da superfície (4096 nós): 20 partes de 199 nós, 3980.
    await montarEmPartes(folha, `folha ${n}`, 20);
    await tentar(`mostrar a folha ${n}`, () => folha.mostrar());
  }
}

// -------------------------------------------------------------- os avisos
let avisos = 0;
function avisarNoTeto() {
  // 4 por segundo, o teto por MOD; cada um com 1 KiB de carga.
  setInterval(() => {
    avisos += 1;
    SeeleMods.request(EU, SEM_CANAL, { aviso: avisos }).catch(() => {});
  }, 250);
}

// ------------------------------------------------- o diálogo e a página
async function abrirODialogo() {
  const dialogo = await tentar("diálogo", () => SeeleUI.superficies.criar({
    id: "dialogo", tipo: "dialogo", modal: true, titulo: "Carga M3 · diálogo",
  }));
  if (!dialogo) return;
  await montarEmPartes(dialogo, "diálogo", 4);
  await tentar("mostrar o diálogo", () => dialogo.mostrar());
}

async function abrirAPagina() {
  const pagina = await tentar("página", () => SeeleUI.superficies.criar({
    id: "pagina", tipo: "pagina", imersiva: true, titulo: "Carga M3 · página",
  }));
  if (!pagina) return;
  await montarEmPartes(pagina, "página", 4);
  await tentar("mostrar a página", () => pagina.mostrar());
}

// O diálogo modal e a página imersiva deixam inerte o resto da janela, e o
// roteiro ainda precisa do ENTRAR NA SALA depois de o MOD carregar (os dois
// cenários são com voz). Abertos na carga, o roteiro não achava o botão
// (medido em 10/10, «não entrou na sala»). Por isso abrem quando quem usa já
// está numa sala, pelo retrato e pelo evento `sala`.
async function quandoNaSala(abrir) {
  let estado = "esperando";
  let deNovo = false;
  const olhar = async () => {
    if (estado === "aberto") return;
    if (estado === "olhando") { deNovo = true; return; }
    estado = "olhando";
    const retrato = await tentar("retrato", () => SeeleUI.retrato());
    if (retrato?.salas?.some((s) => s.minha)) {
      estado = "aberto";
      await abrir();
      return;
    }
    estado = "esperando";
    if (deNovo) { deNovo = false; await olhar(); }
  };
  SeeleUI.aoEvento((evento) => { if (evento?.nome === "sala") olhar(); });
  await olhar();
}

// --------------------------------------------------------------- o palco
async function montarOPalco() {
  // **O teto é o da mensagem, e não o das figuras.** O teto de figuras é 256,
  // e 200 figuras completas (com chave e cor) já passam dos 12 KiB de uma
  // mensagem — medido pela conferência. As peças que cabem: começa nas 256 e
  // tira até a declaração caber, com folga para o envelope do pedido.
  const pecas = Array.from({ length: 256 }, (_, i) => ({
    tipo: "circulo", chave: `p${i}`, x: 25 + (i % 20) * 50, y: 40 + Math.floor(i / 20) * 56, raio: 20, cor: "#c86b1f",
  }));
  const pedido = () => ({
    ponto: "chamada.palco", modo: "substituir", prioridade: 0,
    conteudo: [{ forma: "tela", chave: "tabuleiro", largura: 1024, altura: 768, figuras: pecas }],
  });
  while (pecas.length && JSON.stringify(pedido()).length > 11800) pecas.pop();
  console.log(`m3: o palco leva ${pecas.length} figuras`);
  await tentar("palco", () => SeeleUI.contribuicoes.registrar(pedido()));
}

// ------------------------------------------------------------- a subida
(async () => {
  const retrato = await tentar("retrato", () => SeeleUI.retrato());
  if (quer("regioes")) await ocuparAsRegioes(retrato);
  if (quer("capas")) {
    await vestirAsCapas();
    // Quem entra depois também ganha capa: o roteiro abre os participantes
    // antes do app, mas a ordem da presença não é garantida.
    SeeleUI.aoEvento((evento) => {
      if (evento?.nome === "pessoas" || evento?.nome === "sala") vestirAsCapas();
    });
  }
  if (quer("folhas")) await empilharAsFolhas();
  if (quer("avisos")) avisarNoTeto();
  if (quer("palco")) await montarOPalco();
  if (CENARIO === "dialogo") await quandoNaSala(abrirODialogo);
  if (CENARIO === "pagina") await quandoNaSala(abrirAPagina);
})();
