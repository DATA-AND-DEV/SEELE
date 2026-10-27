// SEELE — cargos e permissões (`#cargos`), camada sobre a operação. E6 da 0.17.
//
// O pedido foi este: «permissões de MODs podem ser configuradas através dos
// cargos ... uma pessoa pode ter cargo pra fazer tal coisa, mas não pra outra
// ... precisa ficar bem esclarecido». Três abas respondem às três perguntas
// de quem administra:
//
// - CARGOS: quais existem, em que ordem (a hierarquia), e o que cada um
//   concede ou restringe, com as permissões de cada MOD sob o nome dele;
// - PESSOAS: quem tem qual cargo, e dar ou tirar um;
// - QUEM PODE O QUÊ: para uma pessoa, cada permissão com PODE ou NÃO PODE e a
//   regra que decidiu — qual cargo concedeu, qual restringiu, ou que ela é a
//   proprietária.
//
// O modelo é o de `cargos-decisoes.md` (cargos-backend): concede · neutro ·
// restringe, a soma das concessões, a restrição vencendo até o Administrador,
// o @everyone na posição 0 e o proprietário acima de tudo.
//
// ---- o que este arquivo não decide ----
//
// Quem pode. O que desliga um controle vem do servidor com um motivo nomeado
// (`gerivel`, `motivo`, `concediveis`), a regra de uma permissão vem pronta em
// `explicar`, e o que uma mudança daria a cada pessoa vem de `prever_cargo`,
// calculado pela mesma função que o servidor usa para recusar. Desligar é
// complemento: quem recusa é o servidor (contrato E6), e a recusa aparece com a
// frase do motivo e, embaixo, o que ele escreveu.
//
// Quando o quadro não traz `gerivel`, a tela deduz pela posição só para
// DESENHAR o desligado com um motivo; o pedido sai do mesmo jeito, e o servidor
// decide.
//
// O que não se pode administrar aparece **desligado com o motivo escrito**, e
// nunca some: um cargo acima do seu está na lista, no lugar dele.
//
// ---- o ganho efetivo (revisão do Codex) ----
//
// Tirar uma restrição, tirar de alguém um cargo que restringe ou apagar um
// cargo restritivo DEVOLVE permissões, e isso nunca pode parecer inofensivo.
// Antes de salvar, de dar ou de tirar um cargo e de apagar, a tela mostra o
// que cada pessoa atingida passa a poder e deixa de poder. Quando o servidor
// não calcula a prévia, a tela ainda diz o que a mudança devolve, pela
// diferença do próprio rascunho.
//
// ---- a camada é da sessão (E3 e E3b) ----
//
// O estado desta camada — aberta ou não, a aba, o cargo escolhido, o rascunho
// ainda não salvo, a pessoa escolhida, a atribuição pendente — mora em
// `cargosPorSessao`, pelo `SessaoId`:
//
// - toda leitura guarda o estado de onde partiu e é aplicada a ELE; ela só é
//   desenhada se aquela sessão ainda for a da tela;
// - trocar de A para B esconde a camada de A sem perder nada
//   (`cargosAoSairDaVista`), e voltar a A a reabre como estava
//   (`cargosAoVoltarAVista`);
// - todo comando sai pelo `invoke` da casca, com o carimbo da vista do
//   instante do clique (base.js), e todo ato confere antes que o estado é o da
//   vista: um clique nunca age sobre a sessão da frente com dados da de trás.

"use strict";

/** O estado da camada em cada sessão, pelo `SessaoId` (0 sem vista conhecida). */
const cargosPorSessao = new Map();

/** O estado da sessão que a camada mostra agora, ou `null` com ela fechada. */
let cargosNaTela = null;

/** Para onde o foco volta ao fechar. */
let cargosFocoDeVolta = null;

/** O vigia da revisão dos cargos, enquanto a camada está aberta. */
let cargosVigia = null;

/** A espera curta antes de pedir a prévia de um rascunho que ainda muda. */
let cargosEsperaDaPrevia = null;

/** As cores que um cargo pode ter: nomes da palheta, nunca valores livres. */
const CARGOS_CORES = Object.freeze(["osso", "laranja", "ambar", "fosforo"]);

/** A ordem dos grupos de permissões, como a tela os mostra. */
const CARGOS_GRUPOS = Object.freeze(["conversa", "voz", "moderacao", "servidor", "mod", "outras"]);

/** As abas, na ordem das setas do teclado. */
const CARGOS_ABAS = Object.freeze(["cargos", "pessoas", "explicar"]);

/**
 * Os únicos nomes de permissão que esta tela lê, e só para DESENHAR: o
 * `SuasPermissoes` do servidor diz o que quem pede tem, e o quadro não diz se
 * ele gere cargos nem se tem tudo. `GERIR CARGOS` acende os controles de
 * gestão; o `ADMINISTRADOR` implica todas (cargos-decisoes §5), então nada se
 * desliga por «você não tem». O servidor confere de novo em todo pedido.
 * `tests/cargos_ui.rs` recusa qualquer outro nome de permissão neste arquivo.
 */
const CARGOS_GERIR = Object.freeze({ gerirCargos: "ManageRoles", administrador: "Administrator" });

/**
 * Os comandos de cargos, um `invoke` literal cada — é assim que
 * `tests/frontend.rs` liga cada nome a um comando registrado, nos dois
 * sentidos. Só `cargosPedir` os chama (`tests/cargos_ui.rs`).
 */
const CARGOS_COMANDOS = Object.freeze({
  pedir_cargos: (args) => invoke("pedir_cargos", args),
  explicar_permissoes: (args) => invoke("explicar_permissoes", args),
  prever_cargo: (args) => invoke("prever_cargo", args),
  criar_cargo: (args) => invoke("criar_cargo", args),
  editar_cargo: (args) => invoke("editar_cargo", args),
  mover_cargo: (args) => invoke("mover_cargo", args),
  apagar_cargo: (args) => invoke("apagar_cargo", args),
  atribuir_cargo: (args) => invoke("atribuir_cargo", args),
  revogar_cargo: (args) => invoke("revogar_cargo", args),
});

// ------------------------------------------------------------------ o estado

/** A sessão que a janela mostra; 0 numa casca que ainda não carimba. */
function cargosSessaoDaVista() {
  return vistaDaJanela.sessao ?? 0;
}

/** O estado guardado de uma sessão, criado na primeira vez. */
function cargosEstadoDe(sessao) {
  let estado = cargosPorSessao.get(sessao);
  if (!estado) {
    estado = {
      sessao,
      aberta: false,
      aba: "cargos",
      cargo: null,
      pessoa: null,
      explicarQuem: null,
      detalhe: false,
      rascunho: null,
      rascunhosGuardados: new Map(),
      previa: null,
      pendente: null,
      quadro: null,
      pessoas: null,
      explicacao: null,
      marcaVista: null,
      erro: null,
    };
    cargosPorSessao.set(sessao, estado);
  }
  return estado;
}

/** Se este estado é o que está na tela, e a sessão dele ainda é a vista. */
function cargosEstaNaTela(estado) {
  return cargosNaTela === estado && estado.sessao === cargosSessaoDaVista() && !$("cargos").hidden;
}

/**
 * Pede ao servidor, e aplica a resposta ao estado da sessão de ORIGEM.
 *
 * `aplicar` roda sobre o estado que pediu, mesmo que a vista tenha mudado: o
 * que chegou é daquela sessão. Desenhar é que só acontece se ela continua na
 * tela. Uma `ForaDaVista` (base.js) é a casca dizendo o mesmo; o estado já é
 * da sessão certa, e ela sai calada daqui.
 */
async function cargosPedir(estado, comando, args, aplicar) {
  // **O carimbo é o da sessão do estado, e não o da vista do instante.** Um
  // pedido que sai depois de a vista mudar — a releitura de quem espera o
  // desfecho de um verbo — tem de ir à sessão que mandou o verbo; sem isto, a
  // casca poria o carimbo da vista nova, e o quadro de B entraria no estado
  // de A.
  const carimbo =
    estado.sessao && typeof estado.sessao === "number"
      ? { sessao: estado.sessao, geracao: geracoesDasSessoes.get(estado.sessao) ?? vistaDaJanela.geracao }
      : undefined;
  try {
    const valor = await CARGOS_COMANDOS[comando](carimbo ? { ...args, carimbo } : args);
    aplicar(valor);
    return true;
  } catch (falha) {
    if (falha && falha.ForaDaVista) return false;
    throw falha;
  }
}

// ------------------------------------------------------ a forma dos dados

/**
 * A chave de uma permissão, como a tela a guarda: o nome da variante para as
 * do produto (`Kick`), e `mod:<autor/nome>#<acao>` para as de MOD — a mesma
 * forma em que o servidor as grava (`Permissao::gravada`, seele-proto).
 *
 * Do fio ela chega como `seele_proto::cargos::Permissao` em serde:
 * `{"Produto":"Kick"}` ou `{"DeMod":{"mod_id":…,"acao":…}}`; uma
 * `ChaveDeMod` solta chega como `{"mod_id":…,"acao":…}`.
 */
function cargosChave(bruta) {
  if (bruta && typeof bruta === "object") {
    const mod = bruta.DeMod ?? (bruta.mod_id ? bruta : null);
    if (mod) return `mod:${mod.mod_id}#${mod.acao}`;
    if (bruta.Produto) return String(bruta.Produto);
  }
  return String(bruta);
}

/** O MOD e a ação de uma chave de MOD. */
function cargosPartesDeMod(chave) {
  const resto = String(chave).slice(4);
  const corte = resto.lastIndexOf("#");
  return { mod_id: resto.slice(0, corte), acao: resto.slice(corte + 1) };
}

/** A chave de volta para o fio, como `Permissao`: a de MOD é estruturada, nunca texto livre. */
function cargosChaveParaOFio(chave) {
  if (!cargosEDeMod(chave)) return { Produto: chave };
  return { DeMod: cargosPartesDeMod(chave) };
}

/** Se a chave é de MOD. */
function cargosEDeMod(chave) {
  return String(chave).startsWith("mod:");
}

/** A altura de quem pede, em número: `Proprietario` fica acima de tudo. */
function cargosAltura(altura) {
  if (altura === "Proprietario") return Infinity;
  if (altura && typeof altura === "object" && typeof altura.Cargo === "number") return altura.Cargo;
  return typeof altura === "number" ? altura : null;
}

/**
 * Os nomes das pessoas que esta janela conhece: o que o quadro trouxer
 * (`apelidos`), e quem o retrato mostra — presentes e quem está nas salas.
 * O quadro do servidor dá as atribuições por id; o nome é da tela.
 */
function cargosApelidos(bruto) {
  const nomes = new Map();
  for (const [id, apelido] of Object.entries(bruto?.apelidos ?? {})) nomes.set(Number(id), apelido);
  const pessoasDoRetrato = [
    ...(desenhado?.presentes ?? []),
    ...(desenhado?.voice_rooms ?? []).flatMap((sala) => sala.people ?? []),
  ];
  for (const p of pessoasDoRetrato) if (!nomes.has(p.id)) nomes.set(p.id, p.nickname);
  return nomes;
}

/**
 * O quadro na forma da tela, venha ele como vier: as concessões e restrições
 * em listas (`concede`/`restringe`, a forma do fio) ou num mapa
 * (`permissoes`), e os cargos do mais alto ao mais baixo.
 */
function cargosNormalizar(resposta) {
  if (!resposta || typeof resposta !== "object") return null;
  // `CargosNaTela { quadro, suas, apelidos, ultima_recusa }` (cargos-api Δ v2),
  // ou o quadro sozinho. Um `quadro` nulo é «antes do primeiro quadro»: a tela
  // diz que está lendo, e relê.
  if ("quadro" in resposta && !resposta.quadro) return null;
  const bruto = resposta.quadro ?? resposta;
  const suas = resposta.suas ?? null;
  const me = desenhado?.me ?? null;
  const proprietario =
    suas?.proprietario === true || bruto.eu?.proprietario === true || (bruto.proprietario != null && bruto.proprietario === me);
  // As efetivas de quem pede: o que ele pode conceder. O Administrador implica
  // todas (cargos-decisoes §5), e o proprietário também.
  const efetivas = suas ? new Set([...(suas.permissoes ?? []).map(cargosChave), ...(suas.de_mods ?? []).map(cargosChave)]) : null;
  const eu = {
    pessoa: bruto.eu?.pessoa ?? me,
    proprietario,
    gere_cargos:
      proprietario ||
      bruto.eu?.gere_cargos === true ||
      Boolean(efetivas && (efetivas.has(CARGOS_GERIR.gerirCargos) || efetivas.has(CARGOS_GERIR.administrador))),
    posicao: proprietario ? Infinity : cargosAltura(suas?.altura) ?? bruto.eu?.posicao_mais_alta ?? bruto.eu?.posicao ?? null,
  };
  // Quem tem qual cargo (só quem tem algum além do @everyone).
  const atribuicoes = new Map();
  for (const a of bruto.atribuicoes ?? []) atribuicoes.set(a.pessoa, a.cargos ?? []);
  const temCor = (bruto.cargos ?? []).some((c) => "cor" in c);
  const cargos = (bruto.cargos ?? []).map((c, i, todosOsCargos) => {
    const permissoes = {};
    if (c.permissoes && !Array.isArray(c.permissoes)) {
      for (const [chave, efeito] of Object.entries(c.permissoes)) {
        permissoes[cargosChave(chave)] = efeito === "nega" ? "restringe" : efeito;
      }
    }
    for (const chave of c.concede ?? []) permissoes[cargosChave(chave)] = "concede";
    for (const chave of c.restringe ?? []) permissoes[cargosChave(chave)] = "restringe";
    const posicao = typeof c.posicao === "number" ? c.posicao : todosOsCargos.length - 1 - i;
    const todos = c.todos === true || c.id === 0 || posicao === 0;
    const pessoas = c.pessoas ?? [...atribuicoes.values()].filter((lista) => lista.includes(c.id)).length;
    const cargo = {
      id: c.id, nome: c.nome, cor: c.cor ?? null, posicao, todos, permissoes, pessoas,
      revisao: c.revisao ?? null,
    };
    if (typeof c.gerivel === "boolean") {
      cargo.gerivel = c.gerivel;
      cargo.motivo = c.gerivel ? null : c.motivo ?? "desconhecido";
    } else {
      cargo.motivo = cargosMotivoPelaPosicao(eu, posicao);
      cargo.gerivel = cargo.motivo === null;
    }
    return cargo;
  });
  cargos.sort((a, b) => b.posicao - a.posicao);
  // O catálogo: as permissões do produto que esta versão conhece (a tabela de
  // frases, que `tests/cargos_ui.rs` prende ao `Permission` de verdade), e as
  // que os MODs ligados declararam, cada uma sob o MOD dela.
  const catalogo = bruto.catalogo
    ? bruto.catalogo.map((p) => ({ ...p, chave: cargosChave(p.chave) }))
    : [
        ...Object.keys(FRASES_DOS_CARGOS.permissoes).map((chave) => ({ chave, grupo: FRASES_DOS_CARGOS.grupoDe[chave] })),
        ...(bruto.de_mods ?? []).map((d) => ({
          chave: cargosChave(d.chave),
          grupo: "mod",
          nome: d.nome,
          mod_: { id: d.chave.mod_id, titulo: d.titulo ?? d.chave.mod_id },
        })),
      ];
  const concediveis = bruto.concediveis
    ? new Set(bruto.concediveis.map(cargosChave))
    : efetivas && !efetivas.has(CARGOS_GERIR.administrador)
      ? efetivas
      : null;
  return {
    revisao: bruto.revisao ?? null,
    combinacao: bruto.combinacao ?? "restringe_vence",
    donoDeclarado: bruto.dono_declarado !== false,
    proprietario: bruto.proprietario ?? null,
    eu,
    cargos,
    catalogo,
    concediveis,
    temCor,
    atribuicoes,
    apelidos: cargosApelidos({ apelidos: { ...(bruto.apelidos ?? {}), ...(resposta.apelidos ?? {}) } }),
    // A última recusa que o servidor mandou a esta sessão: é por ela que a
    // tela sabe que um verbo mandado foi recusado (`cargosMandar`).
    ultimaRecusa: cargosRecusaNumerada(resposta.ultima_recusa),
    // A marca da sessão (`cargos_revision` do retrato) no instante da leitura.
    marca: typeof resposta.marca === "number" ? resposta.marca : null,
    pessoasDoServidor: Array.isArray(bruto.pessoas) ? bruto.pessoas : null,
  };
}

/**
 * A `ultima_recusa` na forma da tela: `{ seq, motivo }` (cargos-api, a recusa
 * numerada: o `seq` sobe a cada recusa, inclusive a mesma de novo), ou a
 * recusa sozinha de um servidor sem número (`seq: null`).
 */
function cargosRecusaNumerada(bruta) {
  if (bruta === null || bruta === undefined) return null;
  if (typeof bruta === "object" && "seq" in bruta && "motivo" in bruta) return { seq: bruta.seq, motivo: bruta.motivo };
  return { seq: null, motivo: bruta };
}

/**
 * O que distingue uma recusa da anterior: o `seq`. Sem número, o motivo inteiro
 * — e aí a mesma recusa duas vezes seguidas não se distingue, que é o caso que
 * o número existe para resolver.
 */
function cargosMarcaDaRecusa(recusa) {
  if (!recusa) return null;
  return recusa.seq !== null ? `seq:${recusa.seq}` : JSON.stringify(recusa.motivo);
}

/** A marca dos cargos no retrato da vista (`Snapshot.cargos_revision`), ou `null`. */
function cargosMarcaDoRetrato() {
  const marca = desenhado?.cargos_revision;
  return typeof marca === "number" ? marca : null;
}

/** O motivo de não gerir um cargo, pela posição: só para desenhar. */
function cargosMotivoPelaPosicao(eu, posicao) {
  if (eu.proprietario) return null;
  if (!eu.gere_cargos) return "sem_gerir_cargos";
  if (typeof eu.posicao !== "number") return null;
  if (posicao === eu.posicao) return "igual_ao_seu";
  if (posicao > eu.posicao) return "acima_do_seu";
  return null;
}

/**
 * As pessoas, do quadro: quem tem cargo, o proprietário, quem pede e quem o
 * retrato mostra — com o motivo de não mexer nos cargos de alguém.
 */
function cargosPessoasDoQuadro(quadro) {
  if (!quadro) return null;
  const ids = new Set([...quadro.atribuicoes.keys(), ...quadro.apelidos.keys()]);
  if (quadro.eu.pessoa !== null) ids.add(quadro.eu.pessoa);
  if (quadro.proprietario !== null) ids.add(quadro.proprietario);
  const lista = quadro.pessoasDoServidor ?? [...ids].map((id) => ({
    pessoa: id,
    apelido: quadro.apelidos.get(id) ?? FRASES_DOS_CARGOS.pessoaSemNome(id),
    proprietario: id === quadro.proprietario,
    cargos: quadro.atribuicoes.get(id) ?? [],
  }));
  return cargosNormalizarPessoas(lista, quadro);
}

/** As pessoas na forma da tela, com o motivo de não mexer nos cargos de alguém. */
function cargosNormalizarPessoas(lista, quadro) {
  return lista.map((p) => {
    const pessoa = {
      pessoa: p.pessoa ?? p.id,
      apelido: p.apelido ?? p.nickname ?? String(p.pessoa ?? p.id),
      proprietario: p.proprietario === true,
      cargos: p.cargos ?? [],
    };
    if (typeof p.gerivel === "boolean") {
      pessoa.gerivel = p.gerivel;
      pessoa.motivo = p.gerivel ? null : p.motivo ?? "pessoa_acima";
    } else {
      const eu = quadro?.eu;
      const mais = cargosMaisAlto(quadro, pessoa);
      const acima =
        pessoa.proprietario ||
        (eu && !eu.proprietario && typeof eu.posicao === "number" && pessoa.pessoa !== eu.pessoa && (mais?.posicao ?? 0) >= eu.posicao);
      pessoa.gerivel = !acima;
      pessoa.motivo = pessoa.proprietario ? "do_proprietario" : acima ? "pessoa_acima" : null;
    }
    return pessoa;
  });
}

/**
 * Um `seele_proto::cargos::Motivo` (serde) na regra desta tela, com os cargos
 * que decidiram. `"Proprietario"`, `"Banida"`, `"Desconhecida"` e
 * `"NinguemConcede"` chegam como texto; os outros, com `por`.
 */
function cargosRegraDoMotivo(motivo) {
  if (typeof motivo === "string") {
    return { regra: { Proprietario: "proprietario", Banida: "banida", Desconhecida: "desconhecida" }[motivo] ?? "nenhum_concede", por: [] };
  }
  const [nome] = Object.keys(motivo ?? {});
  const por = [motivo?.[nome]?.por ?? []].flat();
  const regra = { Restringida: "restringida_por", Concedida: "concedida_por", PeloAdministrador: "administrador" }[nome] ?? "nenhum_concede";
  return { regra, por };
}

/**
 * A explicação de uma pessoa, na forma da tela: uma linha por permissão, com
 * se tem, a regra e os cargos dela que decidiram. Numa restrição, os cargos
 * que concediam e perderam aparecem também: é a metade que explica por que
 * «ter o cargo» não bastou.
 */
function cargosNormalizarExplicacao(bruto, pessoa, quadro) {
  if (!bruto) return null;
  const lista = Array.isArray(bruto) ? bruto : bruto.permissoes ?? [];
  const deles = new Set([0, ...(quadro?.atribuicoes.get(pessoa) ?? [])]);
  return {
    pessoa: bruto.pessoa ?? pessoa,
    proprietario: bruto.proprietario === true || pessoa === quadro?.proprietario,
    permissoes: lista.map((e) => {
      const chave = cargosChave(e.permissao ?? e.chave);
      if ("regra" in e) {
        return {
          chave, pode: e.pode === true, regra: e.regra === "negada_por" ? "restringida_por" : e.regra,
          cargos: (e.cargos ?? []).map((c) => ({ cargo: c.cargo, efeito: c.efeito === "nega" ? "restringe" : c.efeito })),
        };
      }
      const explicacao = e.explicacao ?? e;
      const { regra, por } = cargosRegraDoMotivo(explicacao.motivo);
      const efeito = regra === "restringida_por" ? "restringe" : "concede";
      const cargos = por.map((cargo) => ({ cargo, efeito }));
      if (regra === "restringida_por") {
        for (const c of quadro?.cargos ?? []) {
          if (deles.has(c.id) && c.permissoes[chave] === "concede") cargos.push({ cargo: c.id, efeito: "concede" });
        }
      }
      return { chave, pode: explicacao.tem === true, regra, cargos };
    }),
  };
}

/**
 * A prévia de uma mudança, na forma da tela: as pessoas atingidas, ou a
 * recusa que o servidor já sabe que vai dar (`decidir`, cargos-api §1d).
 */
function cargosNormalizarPrevia(bruto) {
  if (bruto && typeof bruto === "object" && !Array.isArray(bruto) && (bruto.Err || bruto.recusa)) {
    return { recusa: bruto.Err ?? bruto.recusa, pessoas: [] };
  }
  const lista = Array.isArray(bruto) ? bruto : bruto?.pessoas ?? [];
  return {
    recusa: null,
    pessoas: lista.map((p) => ({
      pessoa: p.pessoa,
      apelido: p.apelido ?? null,
      ganha: (p.ganha ?? []).map(cargosChave),
      perde: (p.perde ?? []).map(cargosChave),
      alem: (p.alem_do_autor ?? p.alem ?? []).map(cargosChave),
    })),
  };
}

// --------------------------------------------------------------- as frases

/** O nome e a descrição de uma permissão, pela chave (e o catálogo, para as de MOD). */
function cargosDescrever(chave, doCatalogo) {
  if (cargosEDeMod(chave)) {
    const { acao } = cargosPartesDeMod(chave);
    return [doCatalogo?.nome || acao, doCatalogo?.descricao || FRASES_DOS_CARGOS.semDescricaoDoMod];
  }
  return FRASES_DOS_CARGOS.permissoes[chave] ?? FRASES_DOS_CARGOS.desconhecida(chave);
}

/** Se esta versão do app conhece a permissão. Uma desconhecida nunca se concede daqui. */
function cargosConhece(chave) {
  return cargosEDeMod(chave) || Object.hasOwn(FRASES_DOS_CARGOS.permissoes, chave);
}

/** O nome curto de uma permissão, para as frases da prévia e das recusas. */
function cargosNomeDaPermissao(estado, chave) {
  const p = estado.quadro?.catalogo?.find((c) => c.chave === chave);
  return cargosDescrever(chave, p)[0];
}

/** Várias permissões numa frase. */
function cargosNomesDasPermissoes(estado, chaves) {
  return FRASES_DOS_CARGOS.nomes(chaves.map((c) => cargosNomeDaPermissao(estado, c)));
}

/** O grupo de uma permissão: o que o servidor disse, ou o desta versão. */
function cargosGrupoDe(p) {
  if (!cargosConhece(p.chave)) return "outras";
  if (cargosEDeMod(p.chave)) return "mod";
  if (CARGOS_GRUPOS.includes(p.grupo)) return p.grupo;
  return FRASES_DOS_CARGOS.grupoDe[p.chave] ?? "outras";
}

/**
 * O catálogo em grupos, na ordem da tela, e as permissões de MOD separadas
 * por MOD, cada bloco com o nome dele.
 */
function cargosAgrupar(catalogo) {
  const blocos = [];
  for (const grupo of CARGOS_GRUPOS) {
    const doGrupo = (catalogo ?? []).filter((p) => cargosGrupoDe(p) === grupo);
    if (doGrupo.length === 0) continue;
    if (grupo !== "mod") {
      blocos.push({ titulo: FRASES_DOS_CARGOS.grupos[grupo], permissoes: doGrupo });
      continue;
    }
    const porMod = new Map();
    for (const p of doGrupo) {
      const id = p.mod_?.id ?? cargosPartesDeMod(p.chave).mod_id;
      if (!porMod.has(id)) {
        porMod.set(id, { titulo: FRASES_DOS_CARGOS.doMod(p.mod_?.titulo ?? id, id), permissoes: [] });
      }
      porMod.get(id).permissoes.push(p);
    }
    blocos.push(...porMod.values());
  }
  return blocos;
}

/** O motivo nomeado, em frase. */
function cargosFraseDoMotivo(motivo) {
  return FRASES_DOS_CARGOS.motivo[motivo] ?? FRASES_DOS_CARGOS.motivo.desconhecido;
}

/**
 * A recusa do servidor, como a pessoa a lê: a frase do motivo e, embaixo, o
 * texto que o servidor mandou, como veio. A do ganho efetivo leva o nome da
 * pessoa e as permissões que ela ganharia.
 */
/**
 * O `RecusaDeCargo` dentro do que a casca rejeitou: `{ CargoRecusado: { motivo } }`
 * (o `ServerMessage`), `{ RecusaDeCargo: … }`, ou a recusa sozinha.
 */
function cargosRecusaDe(falha) {
  const envelope = falha?.CargoRecusado ?? falha?.RecusaDeCargo ?? falha;
  return envelope?.motivo ?? envelope;
}

function cargosNomeDaRecusa(falha) {
  const recusa = cargosRecusaDe(falha);
  return typeof recusa === "string" ? recusa : Object.keys(recusa ?? {})[0];
}

/** O apelido de uma pessoa pelo id, do quadro da sessão. */
function cargosApelidoDe(estado, id) {
  return estado.quadro?.apelidos.get(id) ?? FRASES_DOS_CARGOS.pessoaSemNome(id);
}

function cargosFraseDaRecusa(estado, falha) {
  if (falha?.SemConfirmacao) return FRASES_DOS_CARGOS.semConfirmacao;
  // `FalhaDeCargo::Conexao(ConnectionError)` (cargos-api Δ v2): a frase da
  // conexão, que é a de sempre.
  if (falha && typeof falha === "object" && "Conexao" in falha) return fraseDeErro(falha.Conexao);
  const recusa = cargosRecusaDe(falha);
  const nome = cargosNomeDaRecusa(falha);
  const dentro = recusa && typeof recusa === "object" ? recusa[nome] : null;
  const texto =
    typeof dentro === "string" ? dentro : dentro?.mensagem ?? dentro?.texto ?? dentro?.message ?? null;
  const doServidor = texto ? `\n${FRASES_DOS_CARGOS.doServidor(texto)}` : "";
  const frase = FRASES_DOS_CARGOS.recusa[nome];
  if (typeof frase === "function") {
    // As que trazem dado: a permissão e a pessoa, com o nome que a tela usa.
    const dados = dentro && typeof dentro === "object" ? dentro : {};
    const permissao = dados.permissao !== undefined ? cargosNomeDaPermissao(estado, cargosChave(dados.permissao)) : "";
    const pessoa = dados.pessoa !== undefined ? dados.apelido ?? cargosApelidoDe(estado, dados.pessoa) : "";
    return frase({ permissao, pessoa }) + doServidor;
  }
  if (frase) return frase + doServidor;
  return (fraseDeErro(falha) || FRASES_DOS_CARGOS.recusaSemNome) + doServidor;
}

/** Se o que a casca rejeitou é uma recusa de cargo que esta tela sabe dizer. */
function cargosERecusaDeCargo(falha) {
  return Object.hasOwn(FRASES_DOS_CARGOS.recusa, cargosNomeDaRecusa(falha) ?? "");
}

/** Escreve uma recusa num `role="alert"`: revelado antes, para ser lido. */
function cargosMostrarErro(id, frase) {
  const alvo = $(id);
  alvo.hidden = !frase;
  alvo.textContent = frase ?? "";
}

/** O nome de um cargo pelo id, no quadro da sessão. */
function cargosNomeDe(estado, id) {
  return estado.quadro?.cargos?.find((c) => c.id === id)?.nome ?? String(id);
}

/** O cargo mais alto de uma pessoa: dá a cor dela, como no modelo do Discord. */
function cargosMaisAlto(quadro, pessoa) {
  const tem = new Set(pessoa.cargos ?? []);
  return (quadro?.cargos ?? []).find((c) => tem.has(c.id) && !c.todos) ?? null;
}

/**
 * A amostra de cor de um cargo: só nomes da palheta, e escondida do leitor de
 * tela. `null` quando o modelo do servidor não tem cor (o da 0.17 não tem):
 * um quadrado vazio ao lado de cada cargo seria um enfeite sem dado.
 */
function cargosAmostra(estado, cor) {
  if (!estado.quadro?.temCor) return null;
  const amostra = elemento(
    "span",
    CARGOS_CORES.includes(cor) ? `cargos-cor-amostra cargos-cor-${cor}` : "cargos-cor-amostra",
  );
  amostra.setAttribute("aria-hidden", "true");
  return amostra;
}

// ------------------------------------------------------------- ler e reler

/** Relê o quadro e as pessoas da sessão deste estado. */
async function cargosReler(estado) {
  estado.erro = null;
  if (cargosEstaNaTela(estado) && !estado.quadro) $("cargos-estado").textContent = FRASES_DOS_CARGOS.lendo;
  try {
    await cargosPedir(estado, "pedir_cargos", {}, (bruto) => cargosAplicarCargos(estado, bruto));
    if (estado.aba === "explicar" && estado.quadro) await cargosLerExplicacao(estado);
  } catch (falha) {
    estado.erro = falha === "NotConnected" ? FRASES_DOS_CARGOS.semSessao : cargosFraseDaLeitura(estado, falha);
  }
  estado.marcaVista = estado.quadro?.marca ?? cargosMarcaDoRetrato();
  if (cargosEstaNaTela(estado)) cargosDesenhar(estado);
  // Antes do primeiro quadro (ele chega logo depois da entrada): relê em
  // seguida, enquanto a camada estiver aberta nesta sessão.
  if (!estado.quadro && !estado.erro && cargosEstaNaTela(estado)) setTimeout(() => cargosEstaNaTela(estado) && cargosReler(estado), 500);
}

/** Relê em silêncio, e só aplica e redesenha se o quadro de fato mudou. */
async function cargosRelerSeMudou(estado) {
  let bruto = null;
  try {
    await cargosPedir(estado, "pedir_cargos", {}, (resposta) => {
      bruto = resposta;
    });
  } catch (falha) {
    console.warn("cargos: releitura", falha);
    return;
  }
  const lido = cargosNormalizar(bruto);
  if (!lido || estado.mandando) return;
  const igual =
    lido.marca !== null && estado.quadro?.marca != null
      ? lido.marca === estado.quadro.marca
      : lido.revisao === estado.quadro?.revisao &&
        cargosMarcaDaRecusa(lido.ultimaRecusa) === cargosMarcaDaRecusa(estado.quadro?.ultimaRecusa);
  if (igual) return;
  cargosAplicarCargos(estado, bruto);
  if (estado.aba === "explicar") await cargosLerExplicacao(estado).catch(() => {});
  if (cargosEstaNaTela(estado)) cargosDesenhar(estado);
}

/** Aplica uma leitura de `pedir_cargos` ao estado da sessão que pediu. */
function cargosAplicarCargos(estado, bruto) {
  const quadro = cargosNormalizar(bruto);
  if (!quadro) return;
  // Um verbo que ficou esperando quando a vista saiu: esta leitura diz o que
  // aconteceu com ele, como a espera teria dito.
  const emVoo = estado.emVoo;
  if (emVoo) {
    const recusou = cargosMarcaDaRecusa(quadro.ultimaRecusa) !== emVoo.antes.recusa;
    if (recusou || cargosVerboEntrou(emVoo.antes, quadro)) {
      estado.emVoo = null;
      if (recusou) {
        estado.erro = cargosFraseDaRecusa(estado, { CargoRecusado: { motivo: quadro.ultimaRecusa.motivo } });
        // Uma atribuição pendente recusada sai: a caixa volta ao que o servidor tem.
        estado.pendente = null;
      } else {
        emVoo.aplicar(null);
      }
    }
  }
  estado.quadro = quadro;
  if (quadro.marca !== null) estado.marcaVista = quadro.marca;
  estado.pessoas = cargosPessoasDoQuadro(quadro);
  cargosConferirRascunhos(estado);
}

/**
 * O tempo que a tela espera a resposta de um verbo, e o passo de uma leitura
 * LOCAL sem evento (o `pedir_cargos` só fala com o servidor enquanto não há
 * quadro; com quadro, ele lê o estado da sessão).
 */
const CARGOS_ESPERA = Object.freeze({ total: 6000, passo: 1000 });

/** Quem espera um evento de cargos, por sessão: acorda na hora em que ele chega. */
const cargosQuemEspera = new Map();

/** Espera o próximo `CargosChanged` desta sessão, ou `ms`, o que vier primeiro. */
function cargosEsperarSinal(estado, ms) {
  return new Promise((pronto) => {
    const prazo = setTimeout(() => {
      cargosQuemEspera.delete(estado.sessao);
      pronto();
    }, ms);
    cargosQuemEspera.set(estado.sessao, () => {
      clearTimeout(prazo);
      cargosQuemEspera.delete(estado.sessao);
      pronto();
    });
  });
}

/**
 * Manda um verbo de gestão e espera o desfecho dele.
 *
 * O comando volta assim que o pedido sai (cargos-api Δ v2): a resposta do
 * servidor — o quadro novo, ou `CargoRecusado` — chega depois, no estado da
 * sessão. A tela relê até ver uma das duas coisas: a `ultima_recusa` mudou
 * (recusado: lança a recusa, como um comando que tivesse falhado), ou a
 * revisão subiu sem recusa nova (aceito). Nada chegou no prazo: lança
 * `SemConfirmacao`, e a tela diz isso em vez de fingir que deu certo.
 */
async function cargosMandar(estado, comando, args, aplicar = () => {}, efeito = null) {
  // `efeito(quadro)`: se o quadro novo mostra o que ESTE verbo pediu. É o que
  // separa «o meu verbo entrou» de «a revisão subiu por outro gestor» — sem
  // ele, a revisão nova de outra pessoa passava por aceite, e a recusa por
  // revisão velha, que vem logo depois, chegava tarde (medido pela E2E).
  const antes = { revisao: estado.quadro?.revisao ?? null, recusa: cargosMarcaDaRecusa(estado.quadro?.ultimaRecusa), efeito };
  estado.mandando = true;
  try {
    if (cargosEstaNaTela(estado)) $("cargos-estado").textContent = FRASES_DOS_CARGOS.confirmando;
    await cargosPedir(estado, comando, args, () => {});
    await cargosEsperarDesfecho(estado, antes, aplicar);
  } finally {
    estado.mandando = false;
    if (cargosEstaNaTela(estado)) $("cargos-estado").textContent = "";
  }
}

/** Se dois mapas de permissões (chave → concede/restringe) são o mesmo. */
function cargosMesmasPermissoes(a, b) {
  const chaves = new Set([...Object.keys(a ?? {}), ...Object.keys(b ?? {})]);
  return [...chaves].every((k) => (a?.[k] ?? "neutro") === (b?.[k] ?? "neutro"));
}

/**
 * Se o verbo entrou: o efeito dele está no quadro (quando o verbo diz qual), ou
 * — sem efeito declarado — a revisão subiu.
 */
function cargosVerboEntrou(antes, quadro) {
  return antes.efeito ? antes.efeito(quadro) === true : quadro.revisao !== antes.revisao;
}

/** Relê até a recusa mudar (recusado) ou o efeito do verbo aparecer (aceito), no prazo. */
async function cargosEsperarDesfecho(estado, antes, aplicar) {
  const fim = Date.now() + CARGOS_ESPERA.total;
  let primeira = true;
  while (Date.now() < fim) {
    // A primeira leitura sai logo (a resposta pode já ter chegado); as outras,
    // no evento da sessão — ou no passo, se ele não vier.
    if (!primeira) await cargosEsperarSinal(estado, Math.min(CARGOS_ESPERA.passo, Math.max(0, fim - Date.now())));
    primeira = false;
    // A vista saiu desta sessão no meio da espera: o desfecho fica anotado
    // com ela, e a próxima leitura dela o resolve (`cargosAplicarCargos`).
    // Nada é aplicado à vista nova, e nada se lê dela.
    if (estado.sessao !== cargosSessaoDaVista()) {
      estado.emVoo = { antes, aplicar };
      throw { ForaDaVista: { sessao: estado.sessao } };
    }
    let bruto = null;
    await cargosPedir(estado, "pedir_cargos", {}, (resposta) => {
      bruto = resposta;
    });
    const lido = cargosNormalizar(bruto);
    if (!lido) continue;
    // Recusado: o número da recusa andou — a mesma recusa de novo também anda.
    if (cargosMarcaDaRecusa(lido.ultimaRecusa) !== antes.recusa) {
      cargosAplicarCargos(estado, bruto);
      throw { CargoRecusado: { motivo: lido.ultimaRecusa.motivo } };
    }
    if (cargosVerboEntrou(antes, lido)) {
      cargosAplicarCargos(estado, bruto);
      aplicar(null);
      return;
    }
  }
  throw { SemConfirmacao: true };
}

/** Uma leitura que falhou: o servidor sem cargos é um caso dito à parte. */
function cargosFraseDaLeitura(estado, falha) {
  const nome = typeof falha === "string" ? falha : Object.keys(falha ?? {})[0];
  if (nome === "Unsupported" || nome === "NaoSuportado" || nome === "SemCargos") return FRASES_DOS_CARGOS.semCargos;
  // Antes do primeiro quadro: passa sozinho, e a releitura vem em seguida.
  if (nome === "SemQuadro") return FRASES_DOS_CARGOS.lendo;
  return `${FRASES_DOS_CARGOS.naoLeu}\n${cargosFraseDaRecusa(estado, falha)}`;
}

/** Lê a explicação da pessoa escolhida na aba QUEM PODE O QUÊ. */
async function cargosLerExplicacao(estado) {
  const quem = estado.explicarQuem ?? estado.quadro?.eu?.pessoa ?? null;
  if (quem === null) return;
  estado.explicarQuem = quem;
  await cargosPedir(estado, "explicar_permissoes", { pessoa: quem }, (bruto) => {
    estado.explicacao = cargosNormalizarExplicacao(bruto, quem, estado.quadro);
  });
}

/**
 * A prévia de um pedido (`PedidoDeCargo`, em serde: `{ Editar: {…} }`): o que
 * cada pessoa atingida passa a poder e deixa de poder, ou a recusa que o
 * servidor já sabe que vai dar. `null` quando o servidor não a calculou; a
 * tela então avisa pela diferença local.
 */
async function cargosPrever(estado, pedido) {
  let previa = null;
  try {
    await cargosPedir(estado, "prever_cargo", { pedido, revisao: estado.quadro?.revisao ?? null }, (bruto) => {
      previa = cargosNormalizarPrevia(bruto);
    });
  } catch (falha) {
    // Uma recusa de cargo é a prévia dizendo «vai ser recusado»; o resto é o
    // servidor sem prévia, e a tela segue com o aviso local.
    if (cargosERecusaDeCargo(falha)) return { recusa: falha, pessoas: [] };
    console.warn("prever_cargo:", falha);
    return null;
  }
  return previa;
}

/**
 * As linhas de uma prévia, e se ela já diz que o servidor vai recusar (alguém
 * ganharia além de quem pede, ou outra recusa que `decidir` antecipou).
 */
function cargosLinhasDaPrevia(estado, previa, avisoLocal) {
  const linhas = [];
  let alem = false;
  if (previa === null) {
    if (avisoLocal) linhas.push(["cargos-previa-item cargos-previa-ganha", avisoLocal]);
    linhas.push(["cargos-previa-item", FRASES_DOS_CARGOS.previa.semPrevia]);
    return { linhas, alem };
  }
  if (previa.recusa) {
    alem = true;
    linhas.push(["cargos-previa-item cargos-previa-alem", FRASES_DOS_CARGOS.previa.vaiRecusar(cargosFraseDaRecusa(estado, previa.recusa))]);
  }
  for (const p of previa.pessoas) {
    const apelido = p.apelido ?? cargosApelidoDe(estado, p.pessoa);
    if (p.alem.length > 0) {
      alem = true;
      linhas.push(["cargos-previa-item cargos-previa-alem", FRASES_DOS_CARGOS.previa.alem(apelido, cargosNomesDasPermissoes(estado, p.alem))]);
    }
    const ganhaSoDentro = p.ganha.filter((c) => !p.alem.includes(c));
    if (ganhaSoDentro.length > 0) {
      linhas.push(["cargos-previa-item cargos-previa-ganha", FRASES_DOS_CARGOS.previa.ganha(apelido, cargosNomesDasPermissoes(estado, ganhaSoDentro))]);
    }
    if (p.perde.length > 0) {
      linhas.push(["cargos-previa-item cargos-previa-perde", FRASES_DOS_CARGOS.previa.perde(apelido, cargosNomesDasPermissoes(estado, p.perde))]);
    }
  }
  if (linhas.length === 0) linhas.push(["cargos-previa-item", FRASES_DOS_CARGOS.previa.nada]);
  linhas.push(["cargos-previa-item", FRASES_DOS_CARGOS.previa.informativa]);
  return { linhas, alem };
}

/** Escreve as linhas de uma prévia numa lista. */
function cargosEscreverPrevia(lista, linhas) {
  repovoar(lista, linhas.map(([classe, texto]) => elemento("li", classe, texto)));
}

// ---------------------------------------------------------------- o desenho

/** Desenha a camada inteira a partir do estado da sessão. */
function cargosDesenhar(estado) {
  const quadro = estado.quadro;
  $("cargos-servidor").textContent = FRASES_DOS_CARGOS.deOnde(desenhado?.server);
  $("cargos-regra-hierarquia").textContent = FRASES_DOS_CARGOS.hierarquia;
  $("cargos-regra-combinacao").textContent =
    FRASES_DOS_CARGOS.combinacao[quadro?.combinacao] ?? FRASES_DOS_CARGOS.combinacao.restringe_vence;
  // Sem proprietário declarado, a regra do proprietário não vale para ninguém
  // aqui, e a tela diz isso no lugar dela (cargos-api §6).
  $("cargos-regra-proprietario").textContent =
    quadro && !quadro.donoDeclarado ? FRASES_DOS_CARGOS.semDono : FRASES_DOS_CARGOS.proprietario;
  $("cargos-estado").textContent = quadro || estado.erro ? "" : FRASES_DOS_CARGOS.lendo;
  cargosMostrarErro("cargos-erro", estado.erro);

  for (const aba of CARGOS_ABAS) {
    const botao = $(`cargos-aba-${aba}`);
    const esta = aba === estado.aba;
    botao.textContent = FRASES_DOS_CARGOS.abas[aba];
    botao.setAttribute("aria-selected", String(esta));
    botao.tabIndex = esta ? 0 : -1;
    $(`cargos-painel-${aba}`).hidden = !esta;
  }

  if (estado.aba === "cargos") cargosDesenharCargos(estado);
  else if (estado.aba === "pessoas") cargosDesenharPessoas(estado);
  else cargosDesenharExplicacao(estado);
}

// ----- a aba CARGOS

/** A lista (a hierarquia) e o editor do cargo escolhido. */
function cargosDesenharCargos(estado) {
  const quadro = estado.quadro;
  const cargos = quadro?.cargos ?? [];
  $("cargos-painel-cargos").dataset.detalhe = estado.detalhe && estado.cargo !== null ? "sim" : "nao";
  repovoar($("cargos-lista"), cargos.map((cargo, i) => cargosItemDaLista(estado, cargos, cargo, i)));

  // NOVO CARGO pede GERIR CARGOS; sem ela, desligado e com o motivo.
  const gere = quadro?.eu?.gere_cargos === true;
  $("cargos-novo").textContent = FRASES_DOS_CARGOS.novo;
  $("cargos-novo").disabled = !quadro || !gere;
  $("cargos-novo").title = !quadro || gere ? "" : FRASES_DOS_CARGOS.motivo.sem_gerir_cargos;
  $("cargos-novo-motivo").hidden = !quadro || gere;
  $("cargos-novo-motivo").textContent = !quadro || gere ? "" : FRASES_DOS_CARGOS.motivo.sem_gerir_cargos;

  cargosDesenharEditor(estado);
}

/** Uma linha da hierarquia: a posição, a cor, o nome, quem tem, e subir/descer. */
function cargosItemDaLista(estado, cargos, cargo, i) {
  const item = elemento("li", "cargos-item");
  const botao = elemento("button", "cargos-item-botao");
  botao.type = "button";
  botao.dataset.cargo = String(cargo.id);
  botao.setAttribute("aria-current", String(cargo.id === estado.cargo));
  const pos = elemento("span", "cargos-item-pos", FRASES_DOS_CARGOS.posicao(i + 1));
  const nome = elemento("span", "cargos-item-nome", cargo.todos ? `${FRASES_DOS_CARGOS.todos} · ${cargo.nome}` : cargo.nome);
  const meta = elemento(
    "span",
    "cargos-item-meta",
    [
      cargo.todos ? FRASES_DOS_CARGOS.todosNota : FRASES_DOS_CARGOS.pessoasNoCargo(cargo.pessoas),
      cargo.gerivel ? FRASES_DOS_CARGOS.geres : FRASES_DOS_CARGOS.naoGeres,
    ].join(" · "),
  );
  botao.append(...[pos, cargosAmostra(estado, cargo.cor), nome, meta].filter(Boolean));
  // O nome acessível diz tudo que a linha mostra, e o motivo quando não se gere.
  botao.setAttribute(
    "aria-label",
    [pos.textContent, nome.textContent, meta.textContent, cargo.gerivel ? null : cargosFraseDoMotivo(cargo.motivo)]
      .filter(Boolean)
      .join(", "),
  );
  item.append(botao);

  // Subir e descer. O de todos é a base e não se move, nada desce abaixo dele,
  // e um cargo não sobe para cima de um que a pessoa não gere.
  const ordem = elemento("span", "cargos-ordem");
  const acima = cargos[i - 1];
  const abaixo = cargos[i + 1];
  const naoMove = !cargo.gerivel
    ? cargosFraseDoMotivo(cargo.motivo)
    : cargo.todos
      ? FRASES_DOS_CARGOS.motivo.cargo_de_todos
      : null;
  // O triângulo é o `aberto` de glifos.js, o mesmo da casa inteira: o de
  // subir é ele girado (`.cargos-ordem-subir`). O nome está no botão.
  const subir = elemento("button", "cargos-ordem-botao cargos-ordem-subir");
  subir.append(glifo("aberto"));
  subir.type = "button";
  subir.dataset.subir = String(cargo.id);
  subir.setAttribute("aria-label", FRASES_DOS_CARGOS.subir(cargo.nome));
  subir.disabled = Boolean(naoMove) || !acima || !acima.gerivel;
  subir.title = naoMove ?? (acima && !acima.gerivel ? FRASES_DOS_CARGOS.motivo.acima_do_seu : "");
  const descer = elemento("button", "cargos-ordem-botao");
  descer.append(glifo("aberto"));
  descer.type = "button";
  descer.dataset.descer = String(cargo.id);
  descer.setAttribute("aria-label", FRASES_DOS_CARGOS.descer(cargo.nome));
  descer.disabled = Boolean(naoMove) || !abaixo || abaixo.todos;
  descer.title = naoMove ?? (abaixo?.todos ? FRASES_DOS_CARGOS.motivo.cargo_de_todos : "");
  ordem.append(subir, descer);
  item.append(ordem);
  return item;
}

/** Como o cargo estava quando o rascunho começou: a revisão esperada e o de antes. */
function cargosBaseDe(cargo, revisaoDoQuadro) {
  return {
    revisao: cargo.revisao, revisaoDoQuadro: revisaoDoQuadro ?? null,
    nome: cargo.nome, cor: cargo.cor ?? "", permissoes: { ...cargo.permissoes },
  };
}

/** O rascunho do cargo escolhido: o que está nos campos, ainda não salvo. */
function cargosRascunhoDe(estado, cargo) {
  if (estado.rascunho?.cargo === cargo.id) return estado.rascunho;
  return {
    cargo: cargo.id,
    nome: cargo.nome,
    cor: cargo.cor ?? "",
    permissoes: { ...cargo.permissoes },
    base: cargosBaseDe(cargo, estado.quadro?.revisao),
    sujo: false,
    desatualizado: false,
    mudouFora: [],
  };
}

/**
 * Depois de toda releitura: um rascunho cujo cargo mudou no servidor (outro
 * gestor salvou antes) fica DESATUALIZADO, com o que mudou anotado. As
 * mudanças da pessoa ficam no rascunho, sem salvar, e nada é reaplicado por
 * cima sozinho: ela vê o que mudou e escolhe REFAZER SOBRE O ATUAL ou
 * DESCARTAR.
 */
function cargosConferirRascunhos(estado) {
  const rascunhos = [estado.rascunho, ...estado.rascunhosGuardados.values()].filter(Boolean);
  for (const rascunho of rascunhos) {
    const atual = estado.quadro?.cargos?.find((c) => c.id === rascunho.cargo);
    if (!atual || !rascunho.sujo) continue;
    const base = rascunho.base;
    const mudouRevisao = atual.revisao !== null && base.revisao !== null && atual.revisao !== base.revisao;
    const chaves = new Set([...Object.keys(base.permissoes), ...Object.keys(atual.permissoes)]);
    const mudou = [...chaves].filter((k) => (base.permissoes[k] ?? "neutro") !== (atual.permissoes[k] ?? "neutro"));
    const nomeMudou = base.nome !== atual.nome;
    if (mudouRevisao || mudou.length > 0 || nomeMudou) {
      rascunho.desatualizado = true;
      rascunho.mudouFora = mudou.map((k) => ({ chave: k, antes: base.permissoes[k] ?? "neutro", agora: atual.permissoes[k] ?? "neutro" }));
      rascunho.nomeFora = nomeMudou ? { antes: base.nome, agora: atual.nome } : null;
    }
  }
}

/** REFAZER SOBRE O ATUAL: as mudanças da pessoa passam a partir do cargo como está. */
function cargosRefazer(estado) {
  const rascunho = estado.rascunho;
  const atual = estado.quadro?.cargos?.find((c) => c.id === rascunho?.cargo);
  if (!rascunho || !atual) return;
  // Só o que a PESSOA mudou volta por cima do atual; o resto é o do servidor.
  const minhas = {};
  const chaves = new Set([...Object.keys(rascunho.base.permissoes), ...Object.keys(rascunho.permissoes)]);
  for (const k of chaves) {
    const antes = rascunho.base.permissoes[k] ?? "neutro";
    const meu = rascunho.permissoes[k] ?? "neutro";
    if (antes !== meu) minhas[k] = meu;
  }
  const permissoes = { ...atual.permissoes };
  for (const [k, efeito] of Object.entries(minhas)) {
    if (efeito === "neutro") delete permissoes[k];
    else permissoes[k] = efeito;
  }
  rascunho.permissoes = permissoes;
  if (rascunho.nome === rascunho.base.nome) rascunho.nome = atual.nome;
  rascunho.base = cargosBaseDe(atual, estado.quadro?.revisao);
  rascunho.desatualizado = false;
  rascunho.mudouFora = [];
  rascunho.nomeFora = null;
  estado.previa = null;
}

/** As restrições que o rascunho tira do cargo: a diferença que devolve permissões. */
function cargosRestricoesTiradas(cargo, rascunho) {
  return Object.entries(cargo.permissoes)
    .filter(([chave, efeito]) => efeito === "restringe" && rascunho.permissoes[chave] !== "restringe")
    .map(([chave]) => chave);
}

/** O editor: nome, cor e as permissões agrupadas, cada uma com três estados. */
function cargosDesenharEditor(estado) {
  const quadro = estado.quadro;
  const cargo = quadro?.cargos?.find((c) => c.id === estado.cargo) ?? null;
  $("cargos-voltar").textContent = FRASES_DOS_CARGOS.voltarALista;
  $("cargos-editor-vazio").textContent = quadro && !cargo ? FRASES_DOS_CARGOS.escolhaUmCargo : "";
  $("cargos-form").hidden = !cargo;
  if (!cargo) return;

  const rascunho = cargosRascunhoDe(estado, cargo);
  const gere = cargo.gerivel === true;
  const comRestricao = quadro.combinacao !== "concede_vence";

  $("cargos-editor-titulo").textContent = cargo.todos ? `${FRASES_DOS_CARGOS.todos} · ${cargo.nome}` : cargo.nome;
  $("cargos-editor-motivo").hidden = gere;
  $("cargos-editor-motivo").textContent = gere ? "" : cargosFraseDoMotivo(cargo.motivo);

  // Os campos só são reescritos quando não são eles que se edita: o vigia
  // relê o quadro, e reescrever um campo com o foco apagaria o que se digita.
  const nome = $("cargos-nome");
  if (document.activeElement !== nome) nome.value = rascunho.nome;
  nome.disabled = !gere || cargo.todos;
  nome.title = cargo.todos ? FRASES_DOS_CARGOS.motivo.cargo_de_todos : gere ? "" : cargosFraseDoMotivo(cargo.motivo);
  const cor = $("cargos-cor");
  repovoar(cor, [
    Object.assign(elemento("option", null, FRASES_DOS_CARGOS.semCor), { value: "" }),
    ...CARGOS_CORES.map((c) => Object.assign(elemento("option", null, FRASES_DOS_CARGOS.cores[c]), { value: c })),
  ]);
  cor.value = rascunho.cor ?? "";
  cor.disabled = !gere;
  // Sem cor no modelo do servidor, o campo não aparece: não há o que escolher.
  cor.closest("label").hidden = !quadro.temCor;

  $("cargos-estado-nota").textContent = comRestricao ? FRASES_DOS_CARGOS.estadoNota : "";

  const mudouFora = new Map((rascunho.desatualizado ? rascunho.mudouFora : []).map((m) => [m.chave, m]));
  const blocos = cargosAgrupar(quadro.catalogo).map((bloco, b) => {
    const grupo = elemento("fieldset", "cargos-grupo");
    grupo.append(elemento("legend", "cargos-grupo-titulo", bloco.titulo));
    bloco.permissoes.forEach((p, i) => {
      const caixa = cargosPermissaoNoEditor(quadro, p, `${b}-${i}`, rascunho, gere, comRestricao);
      const fora = mudouFora.get(p.chave);
      if (fora) {
        // O que outro gestor mudou: destacado, com o antes e o depois escritos.
        caixa.classList.add("cargos-permissao-mudou");
        const frase = elemento(
          "p",
          "cargos-mudou",
          FRASES_DOS_CARGOS.mudouFora(FRASES_DOS_CARGOS.estado[fora.antes], FRASES_DOS_CARGOS.estado[fora.agora]),
        );
        frase.id = `cargos-mudou-${b}-${i}`;
        caixa.append(frase);
        caixa.setAttribute("aria-describedby", `${caixa.getAttribute("aria-describedby")} ${frase.id}`);
      }
      grupo.append(caixa);
    });
    return grupo;
  });
  repovoar($("cargos-permissoes"), blocos);

  $("cargos-rascunho").hidden = !rascunho.sujo && !rascunho.desatualizado;
  $("cargos-rascunho").textContent = rascunho.desatualizado
    ? [
        FRASES_DOS_CARGOS.desatualizado,
        rascunho.nomeFora ? FRASES_DOS_CARGOS.nomeMudouFora(rascunho.nomeFora.antes, rascunho.nomeFora.agora) : null,
      ].filter(Boolean).join("\n")
    : rascunho.sujo
      ? FRASES_DOS_CARGOS.rascunho
      : "";
  $("cargos-refazer").hidden = !rascunho.desatualizado;
  $("cargos-refazer").textContent = FRASES_DOS_CARGOS.refazer;

  // A prévia do que muda: só com mudança pendente.
  // Desatualizado, a prévia era do cargo de antes: ela some até REFAZER.
  const previa = estado.previa?.cargo === cargo.id && rascunho.sujo ? estado.previa : null;
  $("cargos-previa").hidden = !rascunho.sujo || rascunho.desatualizado;
  let passaDoAutor = false;
  const lendoPrevia = rascunho.sujo && !rascunho.desatualizado && (!previa || previa.lendo);
  if (rascunho.sujo && !rascunho.desatualizado) {
    const tiradas = cargosRestricoesTiradas(cargo, rascunho);
    const avisoLocal = tiradas.length > 0 ? FRASES_DOS_CARGOS.previa.devolve(cargosNomesDasPermissoes(estado, tiradas)) : null;
    if (!previa || previa.lendo) {
      cargosEscreverPrevia($("cargos-previa-lista"), [["cargos-previa-item", FRASES_DOS_CARGOS.previa.lendo]]);
    } else {
      const { linhas, alem } = cargosLinhasDaPrevia(estado, previa.resultado, avisoLocal);
      passaDoAutor = alem;
      cargosEscreverPrevia($("cargos-previa-lista"), linhas);
    }
  }

  $("cargos-salvar").textContent = FRASES_DOS_CARGOS.salvar;
  // SALVAR desligado quando a prévia do servidor já diz que ele vai recusar; o
  // porquê está escrito na prévia, logo acima. Complemento: o servidor confere.
  // E desligado com o rascunho desatualizado: salvar por cima do que outro
  // gestor acabou de gravar só depois de a pessoa ver o que mudou e refazer.
  // E enquanto a prévia não chega: salvar antes de ler o que muda é o que a
  // prévia existe para impedir.
  $("cargos-salvar").disabled = !gere || !rascunho.sujo || passaDoAutor || rascunho.desatualizado || lendoPrevia;
  $("cargos-salvar").title = !gere
    ? cargosFraseDoMotivo(cargo.motivo)
    : rascunho.desatualizado
      ? FRASES_DOS_CARGOS.refazer
      : passaDoAutor || lendoPrevia
        ? FRASES_DOS_CARGOS.previa.titulo
        : "";
  $("cargos-descartar").textContent = FRASES_DOS_CARGOS.descartar;
  $("cargos-descartar").disabled = !rascunho.sujo;
  $("cargos-apagar").textContent = FRASES_DOS_CARGOS.apagar;
  const naoApaga = cargo.todos ? FRASES_DOS_CARGOS.motivo.cargo_de_todos : gere ? null : cargosFraseDoMotivo(cargo.motivo);
  $("cargos-apagar").disabled = Boolean(naoApaga);
  $("cargos-apagar").title = naoApaga ?? "";
}

/**
 * Uma permissão no editor: nome, o que ela permite, e os três estados como um
 * grupo de rádio. Desligada, ela continua mostrando o estado que tem, e diz
 * por que não muda.
 */
function cargosPermissaoNoEditor(quadro, p, sufixo, rascunho, gere, comRestricao) {
  const [nome, descricao] = cargosDescrever(p.chave, p);
  const conhecida = cargosConhece(p.chave);
  const concedivel = quadro.concediveis === null || quadro.concediveis.has(p.chave) || quadro.eu.proprietario;
  const atual = rascunho.permissoes[p.chave] ?? "neutro";
  // Tirar uma concessão ou mexer numa restrição só diminui um cargo que já está
  // abaixo de quem edita (cargos-decisoes §3); acrescentar uma concessão pede
  // tê-la. Por isso só CONCEDE fica desligado sem a permissão.
  const motivo = !gere
    ? null // o motivo do cargo inteiro já está no alto do editor
    : !conhecida
      ? FRASES_DOS_CARGOS.desconhecida(p.chave)[1]
      : !concedivel
        ? FRASES_DOS_CARGOS.motivo.nao_concedivel
        : null;

  const caixa = elemento("fieldset", "cargos-permissao");
  caixa.append(elemento("legend", "cargos-permissao-nome", nome));
  const desc = elemento("p", "cargos-permissao-desc", descricao);
  desc.id = `cargos-desc-${sufixo}`;
  caixa.append(desc);
  const descritores = [desc.id];

  const tres = elemento("span", "cargos-tres");
  for (const valor of comRestricao ? ["concede", "neutro", "restringe"] : ["concede", "neutro"]) {
    const rotulo = elemento("label", `cargos-opcao cargos-opcao-${valor}`);
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = `cargos-p-${sufixo}`;
    radio.value = valor;
    radio.checked = atual === valor;
    radio.disabled = !gere || !conhecida || (valor === "concede" && !concedivel && atual !== "concede");
    radio.dataset.permissao = p.chave;
    rotulo.append(radio, document.createTextNode(FRASES_DOS_CARGOS.estado[valor]));
    tres.append(rotulo);
  }
  caixa.append(tres);
  if (motivo) {
    const porQue = elemento("p", "cargos-motivo", motivo);
    porQue.id = `cargos-motivo-${sufixo}`;
    caixa.append(porQue);
    descritores.push(porQue.id);
  }
  caixa.setAttribute("aria-describedby", descritores.join(" "));
  return caixa;
}

/** Pede, com uma espera curta, a prévia do rascunho que está na tela. */
function cargosPreverORascunho(estado) {
  clearTimeout(cargosEsperaDaPrevia);
  const rascunho = estado.rascunho;
  if (!rascunho?.sujo) return;
  // A vez desta prévia: o rascunho muda no mesmo objeto, e uma resposta de uma
  // versão anterior dele que chegue depois não pode valer pela de agora.
  const vez = (estado.vezDaPrevia = (estado.vezDaPrevia ?? 0) + 1);
  estado.previa = { cargo: rascunho.cargo, lendo: true, resultado: null };
  cargosEsperaDaPrevia = setTimeout(async () => {
    if (estado.rascunho !== rascunho || estado.vezDaPrevia !== vez) return;
    const resultado = await cargosPrever(estado, {
      Editar: { cargo: rascunho.cargo, nome: rascunho.nome.trim(), ...cargosListasParaOFio(rascunho.permissoes) },
    });
    if (estado.rascunho !== rascunho || estado.vezDaPrevia !== vez) return;
    const velha = resultado?.recusa && cargosNomeDaRecusa(resultado.recusa) === "RevisaoVelha";
    // Uma prévia que já se sabe velha não é desenhada: a tela segue em
    // «calculando» enquanto relê.
    if (!velha) estado.previa = { cargo: rascunho.cargo, lendo: false, resultado };
    // A prévia antecipou que outro gestor mudou os cargos: relê, e o rascunho
    // fica desatualizado com o que mudou destacado — o mesmo caminho da
    // recusa no SALVAR, só que antes de a pessoa apertar.
    if (resultado?.recusa && cargosNomeDaRecusa(resultado.recusa) === "RevisaoVelha") {
      await cargosReler(estado);
      cargosConferirRascunhos(estado);
      // Se ESTE cargo não mudou (só outros), não há o que destacar nem o que
      // refazer: o rascunho passa a partir da revisão nova, e a prévia é
      // pedida de novo — agora a do estado de verdade. Nada é gravado.
      if (!rascunho.desatualizado && estado.rascunho === rascunho) {
        rascunho.base.revisaoDoQuadro = estado.quadro?.revisao ?? rascunho.base.revisaoDoQuadro;
        cargosPreverORascunho(estado);
        return;
      }
      rascunho.desatualizado = true;
      estado.previa = { cargo: rascunho.cargo, lendo: false, resultado };
    }
    if (cargosEstaNaTela(estado) && estado.aba === "cargos") cargosDesenharEditor(estado);
  }, 250);
}

/** O mapa de permissões nas duas listas do fio. */
function cargosListasParaOFio(permissoes) {
  const concede = [];
  const restringe = [];
  for (const [chave, efeito] of Object.entries(permissoes)) {
    if (efeito === "concede") concede.push(cargosChaveParaOFio(chave));
    else if (efeito === "restringe") restringe.push(cargosChaveParaOFio(chave));
  }
  return { concede, restringe };
}

// ----- a aba PESSOAS

/** A lista de pessoas e os cargos da escolhida. */
function cargosDesenharPessoas(estado) {
  const pessoas = estado.pessoas ?? [];
  const eu = estado.quadro?.eu?.pessoa;
  $("cargos-painel-pessoas").dataset.detalhe = estado.detalhe && estado.pessoa !== null ? "sim" : "nao";
  const itens = pessoas.map((pessoa) => {
    const item = elemento("li", "cargos-item");
    const botao = elemento("button", "cargos-item-botao");
    botao.type = "button";
    botao.dataset.pessoa = String(pessoa.pessoa);
    botao.setAttribute("aria-current", String(pessoa.pessoa === estado.pessoa));
    const nome = elemento(
      "span",
      "cargos-item-nome",
      pessoa.pessoa === eu ? `${pessoa.apelido} (${FRASES_DOS_CARGOS.voce})` : pessoa.apelido,
    );
    const nomes = pessoa.cargos.map((id) => cargosNomeDe(estado, id));
    const meta = elemento(
      "span",
      "cargos-item-meta",
      [pessoa.proprietario ? FRASES_DOS_CARGOS.proprietarioCurto : null, nomes.join(", ") || FRASES_DOS_CARGOS.todos]
        .filter(Boolean)
        .join(" · "),
    );
    botao.append(
      ...[elemento("span", "cargos-item-pos", ""), cargosAmostra(estado, cargosMaisAlto(estado.quadro, pessoa)?.cor), nome, meta].filter(Boolean),
    );
    item.append(botao);
    return item;
  });
  repovoar($("cargos-pessoas"), itens);
  $("cargos-pessoas-voltar").textContent = FRASES_DOS_CARGOS.voltarAsPessoas;
  cargosDesenharPessoa(estado);
}

/** Os cargos da pessoa escolhida: um por linha, marcado se ela o tem. */
function cargosDesenharPessoa(estado) {
  const pessoa = (estado.pessoas ?? []).find((p) => p.pessoa === estado.pessoa) ?? null;
  $("cargos-pessoa-vazio").textContent =
    estado.pessoas && estado.pessoas.length === 0
      ? FRASES_DOS_CARGOS.semPessoas
      : pessoa
        ? ""
        : FRASES_DOS_CARGOS.escolhaUmaPessoa;
  $("cargos-pessoa").hidden = !pessoa;
  if (!pessoa) return;
  $("cargos-pessoa-titulo").textContent = FRASES_DOS_CARGOS.cargosDe(pessoa.apelido);
  const motivoDaPessoa = pessoa.gerivel ? null : cargosFraseDoMotivo(pessoa.motivo);
  $("cargos-pessoa-motivo").hidden = !motivoDaPessoa;
  $("cargos-pessoa-motivo").textContent = motivoDaPessoa ?? "";
  $("cargos-pessoa-explicar").textContent = FRASES_DOS_CARGOS.verPorQue;

  const pendente = estado.pendente?.pessoa === pessoa.pessoa ? estado.pendente : null;
  const tem = new Set(pessoa.cargos);
  const linhas = (estado.quadro?.cargos ?? [])
    .filter((c) => !c.todos)
    .map((cargo, i) => {
      const linha = elemento("li", null);
      const rotulo = elemento("label", "cargos-atribuicao");
      const caixa = document.createElement("input");
      caixa.type = "checkbox";
      // A caixa mostra o que o servidor tem, e a pendente mostra o pedido.
      caixa.checked = pendente?.cargo === cargo.id ? pendente.dar : tem.has(cargo.id);
      caixa.dataset.atribuir = String(cargo.id);
      const motivo = motivoDaPessoa ? null : cargo.gerivel ? null : cargosFraseDoMotivo(cargo.motivo);
      caixa.disabled = Boolean(motivoDaPessoa) || !cargo.gerivel || (pendente !== null && pendente.cargo !== cargo.id);
      const nome = elemento("span", "cargos-atribuicao-nome", `${FRASES_DOS_CARGOS.posicao(i + 1)} ${cargo.nome}`);
      rotulo.append(...[caixa, cargosAmostra(estado, cargo.cor), nome].filter(Boolean));
      if (motivo) {
        const porQue = elemento("span", "cargos-motivo", motivo);
        porQue.id = `cargos-atribuir-motivo-${cargo.id}`;
        caixa.setAttribute("aria-describedby", porQue.id);
        rotulo.append(porQue);
      }
      linha.append(rotulo);
      return linha;
    });
  repovoar($("cargos-da-pessoa"), linhas);
  cargosDesenharPendente(estado, pessoa, pendente);
}

/** A atribuição pendente: o que muda, e CONFIRMAR ou CANCELAR. */
function cargosDesenharPendente(estado, pessoa, pendente) {
  $("cargos-pendente").hidden = !pendente;
  if (!pendente) return;
  const nomeDoCargo = cargosNomeDe(estado, pendente.cargo);
  $("cargos-pendente-titulo").textContent = pendente.dar
    ? FRASES_DOS_CARGOS.pendenteDar(nomeDoCargo, pessoa.apelido)
    : FRASES_DOS_CARGOS.pendenteTirar(nomeDoCargo, pessoa.apelido);
  let alem = false;
  if (pendente.lendo) {
    cargosEscreverPrevia($("cargos-pendente-lista"), [["cargos-previa-item", FRASES_DOS_CARGOS.previa.lendo]]);
  } else {
    const cargo = estado.quadro?.cargos?.find((c) => c.id === pendente.cargo);
    const restringe = Object.entries(cargo?.permissoes ?? {}).filter(([, e]) => e === "restringe").map(([c]) => c);
    const avisoLocal =
      !pendente.dar && restringe.length > 0
        ? FRASES_DOS_CARGOS.previa.revogarRestritivo(nomeDoCargo, pessoa.apelido, cargosNomesDasPermissoes(estado, restringe))
        : null;
    const resultado = cargosLinhasDaPrevia(estado, pendente.resultado, avisoLocal);
    alem = resultado.alem;
    cargosEscreverPrevia($("cargos-pendente-lista"), resultado.linhas);
  }
  $("cargos-pendente-cancelar").textContent = FRASES_DOS_CARGOS.cancelar;
  $("cargos-pendente-confirmar").textContent = pendente.dar ? FRASES_DOS_CARGOS.confirmarDar : FRASES_DOS_CARGOS.confirmarTirar;
  $("cargos-pendente-confirmar").disabled = Boolean(pendente.lendo) || alem;
}

// ----- a aba QUEM PODE O QUÊ

/** As permissões de uma pessoa, cada uma com PODE ou NÃO PODE e a regra. */
function cargosDesenharExplicacao(estado) {
  const pessoas = estado.pessoas ?? [];
  const eu = estado.quadro?.eu?.pessoa;
  const quem = $("cargos-explicar-quem");
  repovoar(
    quem,
    pessoas.map((p) =>
      Object.assign(elemento("option", null, p.pessoa === eu ? `${p.apelido} (${FRASES_DOS_CARGOS.voce})` : p.apelido), {
        value: String(p.pessoa),
      }),
    ),
  );
  if (estado.explicarQuem !== null) quem.value = String(estado.explicarQuem);

  const explicacao = estado.explicacao;
  const valeAgora = explicacao && explicacao.pessoa === estado.explicarQuem;
  $("cargos-explicar-resumo").textContent = valeAgora && explicacao.proprietario ? FRASES_DOS_CARGOS.regra.proprietario() : "";
  if (!valeAgora) {
    repovoar($("cargos-explicacao"), []);
    return;
  }
  const porChave = new Map(explicacao.permissoes.map((e) => [e.chave, e]));
  // O catálogo dá a ordem e os grupos; uma permissão explicada que o catálogo
  // não tem entra em OUTRAS, e nunca some.
  const catalogo = [...(estado.quadro?.catalogo ?? [])];
  for (const e of explicacao.permissoes) {
    if (!catalogo.some((p) => p.chave === e.chave)) catalogo.push({ chave: e.chave, grupo: "outras" });
  }
  const blocos = cargosAgrupar(catalogo).map((bloco) => {
    const secao = elemento("section", "cargos-grupo");
    secao.append(elemento("h4", "cargos-grupo-titulo", bloco.titulo));
    for (const p of bloco.permissoes) {
      const e = porChave.get(p.chave) ?? { chave: p.chave, pode: false, regra: "nenhum_concede", cargos: [] };
      secao.append(cargosLinhaExplicada(estado, p, e));
    }
    return secao;
  });
  repovoar($("cargos-explicacao"), blocos);
}

/** Uma permissão explicada: o veredito em palavra, o nome e a regra. */
function cargosLinhaExplicada(estado, p, e) {
  const [nome] = cargosDescrever(p.chave, p);
  const conhecida = cargosConhece(p.chave);
  // Uma permissão que este app não conhece nunca aparece como PODE: a regra do
  // contrato é que desconhecida não concede, e a tela não promete o contrário.
  const pode = e.pode && conhecida && e.regra !== "desconhecida";
  const linha = elemento("div", "cargos-explicado");
  linha.setAttribute("role", "group");
  const veredito = elemento(
    "span",
    pode ? "cargos-veredito cargos-veredito-pode" : "cargos-veredito cargos-veredito-nao",
    pode ? FRASES_DOS_CARGOS.pode : FRASES_DOS_CARGOS.naoPode,
  );
  const titulo = elemento("span", "cargos-permissao-nome", nome);
  const regra = elemento("p", "cargos-explicado-regra", cargosFraseDaRegra(estado, e, conhecida));
  linha.append(veredito, titulo, regra);
  linha.setAttribute("aria-label", `${nome}: ${veredito.textContent}. ${regra.textContent}`);
  return linha;
}

/** A regra que decidiu, com os nomes dos cargos dentro. */
function cargosFraseDaRegra(estado, e, conhecida) {
  const regras = FRASES_DOS_CARGOS.regra;
  if (!conhecida || e.regra === "desconhecida") return regras.desconhecida();
  const nomesCom = (efeito) =>
    FRASES_DOS_CARGOS.nomes(e.cargos.filter((c) => c.efeito === efeito).map((c) => cargosNomeDe(estado, c.cargo)));
  if (e.regra === "proprietario") return regras.proprietario();
  if (e.regra === "banida") return regras.banida();
  if (e.regra === "restringida_por") return regras.restringida_por(nomesCom("restringe"), nomesCom("concede"));
  if (e.regra === "administrador") return regras.administrador(nomesCom("concede"));
  if (e.regra === "concedida_por") return regras.concedida_por(nomesCom("concede"));
  return regras.nenhum_concede();
}

// ---------------------------------------------------------------- navegação

/**
 * Abre a camada na sessão da tela. `pessoa` pré-escolhe alguém (a porta da
 * moderação), `aba` escolhe por onde começar.
 */
function abrirCargos({ pessoa = null, aba = null } = {}) {
  const estado = cargosEstadoDe(cargosSessaoDaVista());
  if (pessoa !== null && pessoa !== undefined) {
    estado.pessoa = Number(pessoa);
    estado.explicarQuem = Number(pessoa);
    estado.aba = aba ?? "pessoas";
    estado.detalhe = true;
  } else if (aba && CARGOS_ABAS.includes(aba)) {
    estado.aba = aba;
  }
  cargosFocoDeVolta = document.activeElement;
  cargosMostrar(estado);
  $(`cargos-aba-${estado.aba}`).focus();
  anunciar(`${FRASES_DOS_CARGOS.titulo}. ${FRASES_DOS_CARGOS.hierarquia}`);
}

/** Põe o estado de uma sessão na tela e começa a vigiar a revisão. */
function cargosMostrar(estado) {
  estado.aberta = true;
  cargosNaTela = estado;
  $("cargos").hidden = false;
  cargosDesenhar(estado);
  cargosReler(estado);
}

/**
 * Se há uma tela em que a camada faz sentido: a sessão, ou os ajustes abertos
 * dela — e nenhuma tela de chegada ou de saída à vista. Entrar noutro servidor
 * põe a `tela-auth` sobre a sessão; a camada por cima dela cobria o ENTRAR, e
 * a pessoa ficava sem entrar (medido pela E2E). Ela fica guardada, e volta.
 */
function cargosHaOndeMostrar() {
  const daSessao = !$("tela-sessao").hidden || !$("tela-server").hidden;
  const chegandoOuSaindo = ["tela-auth", "tela-boot", "tela-fim", "tela-antes-de-hospedar"].some((id) => !$(id).hidden);
  return daSessao && !chegandoOuSaindo;
}

/**
 * O vigia, a cada 250 ms e sem falar com o servidor:
 *
 * 1. **a vista mudou de sessão** sem os ganchos da troca (`cargosAoSairDaVista`)
 *    terem sido chamados — por um caminho novo da casca, ou uma casca que ainda
 *    não os chama: a camada de A some, guardada, e a de B só volta se estava
 *    aberta em B. Os ganchos são o caminho certo, no mesmo tique do clique;
 *    isto é a rede embaixo deles, para que os cargos de A nunca fiquem na
 *    frente de B;
 * 2. **os cargos mudaram**: a marca `cargos_revision` do retrato andou, e a
 *    camada relê o estado da sessão. Nenhum pedido periódico: sem a marca
 *    andar (nem o `CargosChanged`, em `cargosAoEvento`), nada é lido.
 */
function cargosVigiar() {
  const vista = cargosSessaoDaVista();
  if (cargosNaTela && (cargosNaTela.sessao !== vista || !cargosHaOndeMostrar())) {
    cargosAoSairDaVista(cargosNaTela.sessao);
  }
  if (!cargosNaTela) {
    const guardado = cargosPorSessao.get(vista);
    if (guardado?.aberta && cargosHaOndeMostrar()) cargosMostrar(guardado);
    return;
  }
  const estado = cargosNaTela;
  if (!cargosEstaNaTela(estado)) return;
  const marca = cargosMarcaDoRetrato();
  if (marca === null || marca === estado.marcaVista || estado.mandando) return;
  estado.marcaVista = marca;
  // Só redesenha se o quadro de fato mudou: redesenhar à toa tiraria o foco
  // de quem está no teclado.
  cargosRelerSeMudou(estado);
}

/** Fecha e devolve o teclado. O estado da sessão fica: reabrir volta onde estava. */
function fecharCargos() {
  if (cargosNaTela) {
    cargosGuardarCampos(cargosNaTela);
    cargosNaTela.aberta = false;
  }
  cargosEsconder();
  if (focavel(cargosFocoDeVolta)) cargosFocoDeVolta.focus();
  else if (focavel($("tela-sessao"))) $("tela-sessao").focus();
  cargosFocoDeVolta = null;
}

/** Some sem tocar no foco nem no estado guardado. */
function cargosEsconder() {
  $("cargos").hidden = true;
  cargosNaTela = null;
  clearTimeout(cargosEsperaDaPrevia);
}

/**
 * A vista sai desta sessão (E3b): a camada some, e o que era dela — aberta, a
 * aba, o cargo, o rascunho, a atribuição pendente — fica guardado com ela.
 * Nada vai à sessão nova.
 */
function cargosAoSairDaVista(sessao) {
  const estado = cargosNaTela;
  if (!estado) return;
  if (sessao !== undefined && sessao !== null && estado.sessao !== sessao) return;
  cargosGuardarCampos(estado);
  estado.aberta = true;
  cargosEsconder();
  cargosFocoDeVolta = null;
}

/** A vista volta a esta sessão: a camada reabre como estava, se estava aberta. */
function cargosAoVoltarAVista(sessao) {
  const estado = cargosPorSessao.get(sessao ?? cargosSessaoDaVista());
  if (!estado || !estado.aberta) return;
  cargosMostrar(estado);
}

/** A sessão acabou: o estado dela vai embora junto, sem ir a mais ninguém. */
function cargosEsquecerSessao(sessao) {
  if (cargosNaTela?.sessao === sessao) cargosEsconder();
  cargosPorSessao.delete(sessao);
}

/** Copia o que está nos campos para o rascunho do estado. */
function cargosGuardarCampos(estado) {
  if (!estado.rascunho || $("cargos-form").hidden || estado.aba !== "cargos") return;
  if (!$("cargos-nome").disabled) estado.rascunho.nome = $("cargos-nome").value;
  if (!$("cargos-cor").disabled) estado.rascunho.cor = $("cargos-cor").value;
}

/** Troca de aba, com o teclado indo junto. */
function cargosEscolherAba(estado, aba) {
  cargosGuardarCampos(estado);
  estado.aba = aba;
  estado.detalhe = false;
  cargosDesenhar(estado);
  if (aba === "explicar") cargosLerEDesenharExplicacao(estado);
}

/** Lê a explicação e desenha, se a sessão ainda for a da tela. */
function cargosLerEDesenharExplicacao(estado) {
  cargosLerExplicacao(estado)
    .then(() => {
      if (cargosEstaNaTela(estado)) cargosDesenhar(estado);
    })
    .catch((falha) => {
      estado.erro = cargosFraseDaRecusa(estado, falha);
      if (cargosEstaNaTela(estado)) cargosDesenhar(estado);
    });
}

/** O estado na tela, para os ouvintes: `null` se a camada não é desta sessão. */
function cargosDaTela() {
  const estado = cargosNaTela;
  return estado && cargosEstaNaTela(estado) ? estado : null;
}

// ------------------------------------------------------------------ os atos

/** Um rascunho que começa agora, a partir do cargo como está. */
function cargosComecarRascunho(estado) {
  const cargo = estado.quadro?.cargos?.find((c) => c.id === estado.cargo);
  if (!cargo) return null;
  if (estado.rascunho?.cargo !== cargo.id) estado.rascunho = cargosRascunhoDe(estado, cargo);
  return estado.rascunho;
}

async function cargosSalvar(estado) {
  cargosGuardarCampos(estado);
  const rascunho = estado.rascunho;
  if (!rascunho || !rascunho.sujo) return;
  cargosMostrarErro("cargos-form-erro", null);
  const nome = rascunho.nome.trim();
  try {
    await cargosMandar(
      estado,
      "editar_cargo",
      {
        cargo: rascunho.cargo,
        nome,
        ...(estado.quadro?.temCor ? { cor: rascunho.cor === "" ? null : rascunho.cor } : {}),
        ...cargosListasParaOFio(rascunho.permissoes),
        // A revisão como estava quando o rascunho começou — a do cargo, se o
        // servidor a der, ou a do conjunto (cargos-api §1b): o servidor recusa
        // se outro gestor gravou no meio, e nada desta pessoa vai por cima.
        revisao: rascunho.base.revisao ?? rascunho.base.revisaoDoQuadro ?? estado.quadro?.revisao ?? null,
      },
      () => {
        if (estado.rascunho === rascunho) estado.rascunho = null;
        estado.previa = null;
      },
      // Entrou quando o cargo, no quadro, é o que a pessoa salvou.
      (quadro) => {
        const c = quadro.cargos.find((x) => x.id === rascunho.cargo);
        return Boolean(c) && (c.todos || c.nome === nome) && cargosMesmasPermissoes(c.permissoes, rascunho.permissoes);
      },
    );
  } catch (falha) {
    // A vista saiu no meio: o desfecho fica com a sessão (`emVoo`).
    if (falha?.ForaDaVista) return;
    const frase = cargosFraseDaRecusa(estado, falha);
    if (cargosNomeDaRecusa(falha) === "RevisaoVelha") {
      // Outro gestor gravou antes: relê, destaca o que mudou, e deixa as
      // mudanças desta pessoa no rascunho, sem reaplicá-las por cima.
      await cargosReler(estado);
      rascunho.desatualizado = true;
      cargosConferirRascunhos(estado);
      if (cargosEstaNaTela(estado)) cargosDesenharEditor(estado);
    }
    if (cargosEstaNaTela(estado)) cargosMostrarErro("cargos-form-erro", frase);
    return;
  }
  await cargosReler(estado);
  if (cargosEstaNaTela(estado)) anunciar(FRASES_DOS_CARGOS.salvo(nome || cargosNomeDe(estado, rascunho.cargo)));
}

async function cargosCriar(estado) {
  cargosMostrarErro("cargos-erro", null);
  const antes = new Set((estado.quadro?.cargos ?? []).map((c) => c.id));
  let criado = null;
  try {
    const pedido = { nome: FRASES_DOS_CARGOS.nomeDoNovo, concede: [], restringe: [], revisao: estado.quadro?.revisao ?? null };
    // O comando não devolve o id (cargos-api Δ v2): o cargo novo é o que
    // aparece na lista depois e não estava antes.
    await cargosMandar(estado, "criar_cargo", pedido, () => {}, (quadro) =>
      quadro.cargos.some((c) => !antes.has(c.id) && c.nome === pedido.nome),
    );
  } catch (falha) {
    if (falha?.ForaDaVista) return;
    if (cargosEstaNaTela(estado)) cargosMostrarErro("cargos-erro", cargosFraseDaRecusa(estado, falha));
    return;
  }
  await cargosReler(estado);
  const novo = criado ?? (estado.quadro?.cargos ?? []).find((c) => !antes.has(c.id))?.id ?? null;
  if (novo !== null) {
    estado.cargo = novo;
    estado.rascunho = null;
    estado.detalhe = true;
  }
  if (!cargosEstaNaTela(estado)) return;
  cargosDesenhar(estado);
  $("cargos-nome").focus();
  $("cargos-nome").select();
}

/** Sobe (-1) ou desce (+1) um cargo: ele toma a posição do vizinho. */
async function cargosMover(estado, id, passo) {
  const cargos = estado.quadro?.cargos ?? [];
  const i = cargos.findIndex((c) => c.id === id);
  const vizinho = cargos[i + passo];
  if (i < 0 || !vizinho || vizinho.todos) return;
  cargosMostrarErro("cargos-erro", null);
  try {
    await cargosMandar(
      estado,
      "mover_cargo",
      { cargo: id, posicao: vizinho.posicao, revisao: cargos[i].revisao ?? estado.quadro?.revisao ?? null },
      () => {},
      (quadro) => quadro.cargos.find((c) => c.id === id)?.posicao === vizinho.posicao,
    );
  } catch (falha) {
    if (falha?.ForaDaVista) return;
    if (cargosEstaNaTela(estado)) cargosMostrarErro("cargos-erro", cargosFraseDaRecusa(estado, falha));
    return;
  }
  await cargosReler(estado);
  if (!cargosEstaNaTela(estado)) return;
  const agora = (estado.quadro?.cargos ?? []).findIndex((c) => c.id === id);
  anunciar(FRASES_DOS_CARGOS.ordenado(cargosNomeDe(estado, id), agora + 1));
  // O foco segue o botão apertado, que o redesenho refez.
  const alvo = $("cargos-lista").querySelector(passo < 0 ? `[data-subir="${id}"]` : `[data-descer="${id}"]`);
  if (focavel(alvo)) alvo.focus();
  else $("cargos-lista").querySelector(`[data-cargo="${id}"]`)?.focus();
}

/**
 * APAGAR: a prévia primeiro, e a confirmação depois, com o que muda dentro da
 * frase. Apagar um cargo que restringe devolve permissões, e a caixa diz isso.
 */
async function cargosApagar(estado) {
  const cargo = estado.quadro?.cargos?.find((c) => c.id === estado.cargo);
  if (!cargo || cargo.todos) return;
  const revisao = cargo.revisao ?? estado.quadro?.revisao ?? null;
  const resultado = await cargosPrever(estado, { Apagar: { cargo: cargo.id } });
  if (!cargosEstaNaTela(estado)) return;
  const restringe = Object.entries(cargo.permissoes).filter(([, e]) => e === "restringe").map(([c]) => c);
  const avisoLocal = restringe.length > 0 ? FRASES_DOS_CARGOS.previa.devolve(cargosNomesDasPermissoes(estado, restringe)) : null;
  const { linhas, alem } = cargosLinhasDaPrevia(estado, resultado, avisoLocal);
  const consequencia = [
    FRASES_DOS_CARGOS.apagarConsequencia(cargo.nome, cargo.pessoas),
    ...linhas.map(([, texto]) => texto),
  ].join("\n");
  if (alem) {
    // O servidor vai recusar: a caixa diz por quê, sem nada para confirmar.
    abrirRecusa(`${FRASES_DOS_CARGOS.apagar} ${cargo.nome}`, consequencia);
    return;
  }
  // A confirmação é a da moderação: dizer a consequência é a confirmação, e a
  // caixa dela fica acima desta (80 contra 75).
  abrirConfirmacao(`${FRASES_DOS_CARGOS.apagar} ${cargo.nome}`, consequencia, `${FRASES_DOS_CARGOS.apagar} ${cargo.nome}`, async () => {
    try {
      await cargosMandar(
        estado,
        "apagar_cargo",
        { cargo: cargo.id, revisao },
        () => {
          if (estado.cargo === cargo.id) estado.cargo = null;
          estado.rascunho = null;
          estado.detalhe = false;
        },
        (quadro) => !quadro.cargos.some((c) => c.id === cargo.id),
      );
    } catch (falha) {
      if (falha?.ForaDaVista) return;
      // A caixa da moderação mostra o erro com `fraseDeErro`; a frase de
      // cargos diz mais, e é ela que vai.
      throw { FrasePronta: cargosFraseDaRecusa(estado, falha) };
    }
    await cargosReler(estado);
  });
}

/** Marca a atribuição pendente, e pede a prévia dela. */
async function cargosPedirAtribuicao(estado, pessoaId, cargoId, dar) {
  // A revisão em que a prévia foi feita é a que vai no CONFIRMAR: se outro
  // gestor mudou os cargos no meio, o servidor recusa, e a prévia não mente.
  const pendente = { pessoa: pessoaId, cargo: cargoId, dar, lendo: true, resultado: null, revisao: estado.quadro?.revisao ?? null };
  estado.pendente = pendente;
  cargosMostrarErro("cargos-pessoa-erro", null);
  cargosDesenharPessoa(estado);
  const pedido = { pessoa: pessoaId, cargo: cargoId };
  let resultado = await cargosPrever(estado, dar ? { Atribuir: pedido } : { Revogar: pedido });
  // A prévia com a revisão velha: relê, e pede de novo UMA vez com a de agora,
  // que é a que o CONFIRMAR vai mandar.
  if (resultado?.recusa && cargosNomeDaRecusa(resultado.recusa) === "RevisaoVelha" && estado.pendente === pendente) {
    await cargosReler(estado);
    pendente.revisao = estado.quadro?.revisao ?? pendente.revisao;
    resultado = await cargosPrever(estado, dar ? { Atribuir: pedido } : { Revogar: pedido });
  }
  if (estado.pendente !== pendente) return;
  pendente.lendo = false;
  pendente.resultado = resultado;
  if (cargosEstaNaTela(estado) && estado.aba === "pessoas") {
    cargosDesenharPessoa(estado);
    $("cargos-pendente-confirmar").disabled ? $("cargos-pendente-cancelar").focus() : $("cargos-pendente-confirmar").focus();
  }
}

/** CONFIRMAR a atribuição pendente: só agora o pedido vai ao servidor. */
async function cargosConfirmarAtribuicao(estado) {
  const pendente = estado.pendente;
  if (!pendente || pendente.lendo) return;
  const pessoa = (estado.pessoas ?? []).find((p) => p.pessoa === pendente.pessoa);
  const nomeDoCargo = cargosNomeDe(estado, pendente.cargo);
  $("cargos-pendente-confirmar").disabled = true;
  try {
    const args = { pessoa: pendente.pessoa, cargo: pendente.cargo, revisao: pendente.revisao };
    await cargosMandar(
      estado,
      pendente.dar ? "atribuir_cargo" : "revogar_cargo",
      args,
      () => {
        if (estado.pendente === pendente) estado.pendente = null;
      },
      (quadro) => (quadro.atribuicoes.get(pendente.pessoa) ?? []).includes(pendente.cargo) === pendente.dar,
    );
  } catch (falha) {
    if (falha?.ForaDaVista) return;
    // Nada mudou lá: a pendente sai, e a caixa volta ao que o servidor tem.
    if (estado.pendente === pendente) estado.pendente = null;
    if (cargosEstaNaTela(estado)) {
      cargosDesenharPessoa(estado);
      cargosMostrarErro("cargos-pessoa-erro", cargosFraseDaRecusa(estado, falha));
    }
    return;
  }
  await cargosReler(estado);
  if (cargosEstaNaTela(estado) && pessoa) {
    anunciar(pendente.dar ? FRASES_DOS_CARGOS.atribuido(nomeDoCargo, pessoa.apelido) : FRASES_DOS_CARGOS.revogado(nomeDoCargo, pessoa.apelido));
    $("cargos-da-pessoa").querySelector(`[data-atribuir="${pendente.cargo}"]`)?.focus();
  }
}

// ------------------------------------------------------------------- ligação

$("cargos-fechar").addEventListener("click", fecharCargos);
fecharAoClicarFora("cargos", fecharCargos);
cargosVigia = setInterval(cargosVigiar, 250);

/**
 * Os eventos de cargos (cargos-api §6), que o `listen` da casca só entrega da
 * sessão da vista (base.js): um quadro novo ou permissões novas fazem a sessão
 * reler; uma recusa que chega por evento — o servidor responde ao verbo depois
 * de o comando ter voltado — aparece com a frase dela, onde a pessoa está.
 */
function cargosAoEvento(mensagem) {
  const evento = mensagem?.payload?.evento ?? mensagem?.payload;
  // `Event::CargosChanged { marca }` (seele-ffi): a sessão da vista recebeu
  // uma mensagem de cargo. Acorda quem espera o desfecho de um verbo, e a
  // camada aberta relê — só isso: a frase de uma recusa vem da leitura.
  if (evento && typeof evento === "object" && evento.CargosChanged) {
    const sessao = cargosSessaoDaVista();
    cargosQuemEspera.get(sessao)?.();
    const estado = cargosPorSessao.get(sessao);
    const marca = evento.CargosChanged.marca;
    if (estado && cargosEstaNaTela(estado) && !estado.mandando && marca !== estado.marcaVista) {
      estado.marcaVista = marca;
      cargosRelerSeMudou(estado);
    }
    return;
  }
  const tipo = evento?.tipo;
  if (tipo !== "cargos" && tipo !== "suas_permissoes" && tipo !== "cargo_recusado") return;
  const estado = cargosPorSessao.get(cargosSessaoDaVista());
  if (!estado) return;
  if (tipo === "cargo_recusado") {
    const frase = cargosFraseDaRecusa(estado, { CargoRecusado: { motivo: evento.motivo } });
    if (cargosEstaNaTela(estado)) {
      const onde = estado.aba === "pessoas" ? "cargos-pessoa-erro" : !$("cargos-form").hidden ? "cargos-form-erro" : "cargos-erro";
      cargosMostrarErro(onde, frase);
    }
  }
  if (estado.quadro) cargosReler(estado);
}
Promise.resolve(listen("seele://event", cargosAoEvento)).catch((falha) => console.warn("cargos: ouvir os eventos", falha));

// As abas: setas, Home e End, como um tablist de verdade.
$("cargos").addEventListener("keydown", (evento) => {
  const estado = cargosDaTela();
  if (!estado || !evento.target.closest?.(".cargos-aba")) return;
  const i = CARGOS_ABAS.indexOf(estado.aba);
  const vai = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: CARGOS_ABAS.length - 1 }[evento.key];
  if (vai === undefined) return;
  evento.preventDefault();
  const nova = CARGOS_ABAS[(vai + CARGOS_ABAS.length) % CARGOS_ABAS.length];
  cargosEscolherAba(estado, nova);
  $(`cargos-aba-${nova}`).focus();
});

// Escape fecha, só com esta camada por cima: com a confirmação da moderação
// aberta acima dela, o Escape é daquela caixa.
window.addEventListener("keydown", (evento) => {
  if (evento.key !== "Escape" || $("cargos").hidden || !$("moderar").hidden) return;
  evento.preventDefault();
  fecharCargos();
});

for (const aba of CARGOS_ABAS) {
  $(`cargos-aba-${aba}`).addEventListener("click", () => {
    const estado = cargosDaTela();
    if (estado) cargosEscolherAba(estado, aba);
  });
}

$("cargos-lista").addEventListener("click", (evento) => {
  const estado = cargosDaTela();
  if (!estado) return;
  const subir = evento.target.closest("button[data-subir]");
  if (subir) return void cargosMover(estado, Number(subir.dataset.subir), -1);
  const descer = evento.target.closest("button[data-descer]");
  if (descer) return void cargosMover(estado, Number(descer.dataset.descer), +1);
  const escolher = evento.target.closest("button[data-cargo]");
  if (!escolher) return;
  const id = Number(escolher.dataset.cargo);
  cargosGuardarCampos(estado);
  // Trocar de cargo com mudanças pendentes não as perde: o rascunho é de um
  // cargo, e voltar a ele o traz de volta enquanto a sessão durar.
  if (estado.rascunho && estado.rascunho.cargo !== id && estado.rascunho.sujo) {
    estado.rascunhosGuardados.set(estado.rascunho.cargo, estado.rascunho);
  }
  if (estado.rascunho?.cargo !== id) {
    estado.rascunho = estado.rascunhosGuardados.get(id) ?? null;
    estado.rascunhosGuardados.delete(id);
  }
  estado.cargo = id;
  estado.detalhe = true;
  estado.previa = null;
  cargosDesenhar(estado);
  if (estado.rascunho?.sujo) cargosPreverORascunho(estado);
  $("cargos-editor-titulo").focus();
});

$("cargos-voltar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (!estado) return;
  cargosGuardarCampos(estado);
  estado.detalhe = false;
  cargosDesenhar(estado);
  $("cargos-lista").querySelector(`[data-cargo="${estado.cargo}"]`)?.focus();
});

$("cargos-nome").addEventListener("input", () => {
  const estado = cargosDaTela();
  const rascunho = estado && cargosComecarRascunho(estado);
  if (!rascunho) return;
  const eraLimpo = !rascunho.sujo;
  rascunho.nome = $("cargos-nome").value;
  rascunho.sujo = true;
  // Redesenhar o editor inteiro a cada tecla apagaria a seleção do campo; só
  // na primeira, que acende os botões e a prévia.
  if (eraLimpo) {
    cargosDesenharEditor(estado);
    cargosPreverORascunho(estado);
  }
});

$("cargos-cor").addEventListener("change", () => {
  const estado = cargosDaTela();
  const rascunho = estado && cargosComecarRascunho(estado);
  if (!rascunho) return;
  rascunho.cor = $("cargos-cor").value;
  rascunho.sujo = true;
  cargosDesenharEditor(estado);
  cargosPreverORascunho(estado);
});

$("cargos-permissoes").addEventListener("change", (evento) => {
  const radio = evento.target.closest("input[data-permissao]");
  const estado = cargosDaTela();
  if (!radio || !estado) return;
  const rascunho = cargosComecarRascunho(estado);
  if (!rascunho) return;
  cargosGuardarCampos(estado);
  if (radio.value === "neutro") delete rascunho.permissoes[radio.dataset.permissao];
  else rascunho.permissoes[radio.dataset.permissao] = radio.value;
  rascunho.sujo = true;
  cargosPreverORascunho(estado);
  cargosDesenharEditor(estado);
  // O rádio redesenhado volta a ter o foco, pelo nome e pelo valor.
  $("cargos-permissoes").querySelector(`input[name="${radio.name}"][value="${radio.value}"]`)?.focus();
});

$("cargos-form").addEventListener("submit", (evento) => {
  evento.preventDefault();
  const estado = cargosDaTela();
  if (estado) cargosSalvar(estado);
});

$("cargos-descartar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (!estado) return;
  estado.rascunho = null;
  estado.previa = null;
  cargosMostrarErro("cargos-form-erro", null);
  cargosDesenharEditor(estado);
  $("cargos-editor-titulo").focus();
});

$("cargos-refazer").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (!estado) return;
  cargosRefazer(estado);
  cargosMostrarErro("cargos-form-erro", null);
  cargosDesenharEditor(estado);
  cargosPreverORascunho(estado);
  anunciar(FRASES_DOS_CARGOS.refeito);
  $("cargos-editor-titulo").focus();
});

$("cargos-apagar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (estado) cargosApagar(estado);
});

$("cargos-novo").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (estado) cargosCriar(estado);
});

$("cargos-pessoas").addEventListener("click", (evento) => {
  const estado = cargosDaTela();
  const botao = evento.target.closest("button[data-pessoa]");
  if (!estado || !botao) return;
  const pessoa = Number(botao.dataset.pessoa);
  if (estado.pendente && estado.pendente.pessoa !== pessoa) estado.pendente = null;
  estado.pessoa = pessoa;
  estado.detalhe = true;
  cargosMostrarErro("cargos-pessoa-erro", null);
  cargosDesenhar(estado);
  $("cargos-pessoa-titulo").focus();
});

$("cargos-pessoas-voltar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (!estado) return;
  estado.detalhe = false;
  cargosDesenhar(estado);
  $("cargos-pessoas").querySelector(`[data-pessoa="${estado.pessoa}"]`)?.focus();
});

$("cargos-da-pessoa").addEventListener("change", (evento) => {
  const caixa = evento.target.closest("input[data-atribuir]");
  const estado = cargosDaTela();
  if (!caixa || !estado || estado.pessoa === null) return;
  const cargo = Number(caixa.dataset.atribuir);
  // Desmarcar a própria pendente é desistir dela.
  if (estado.pendente?.cargo === cargo) {
    estado.pendente = null;
    cargosDesenharPessoa(estado);
    return;
  }
  cargosPedirAtribuicao(estado, estado.pessoa, cargo, caixa.checked);
});

$("cargos-pendente-cancelar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (!estado || !estado.pendente) return;
  const cargo = estado.pendente.cargo;
  estado.pendente = null;
  cargosDesenharPessoa(estado);
  $("cargos-da-pessoa").querySelector(`[data-atribuir="${cargo}"]`)?.focus();
});

$("cargos-pendente-confirmar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (estado) cargosConfirmarAtribuicao(estado);
});

$("cargos-pessoa-explicar").addEventListener("click", () => {
  const estado = cargosDaTela();
  if (!estado || estado.pessoa === null) return;
  estado.explicarQuem = estado.pessoa;
  cargosEscolherAba(estado, "explicar");
  $("cargos-aba-explicar").focus();
});

$("cargos-explicar-quem").addEventListener("change", () => {
  const estado = cargosDaTela();
  if (!estado) return;
  estado.explicarQuem = Number($("cargos-explicar-quem").value);
  estado.explicacao = null;
  cargosDesenhar(estado);
  cargosLerEDesenharExplicacao(estado);
});

// ---------------------------------------------------- o foco preso nos cargos
//
// CARGOS E PERMISSÕES é `aria-modal` e ocupa a janela inteira: o Tab não sai dele para a tela de trás. Tab no último controle visível volta ao
// primeiro, Shift+Tab no primeiro vai ao último, e um foco que já está fora é trazido de volta. (O Esc e a volta do foco a quem abriu já existem.)

function focaveisDosCargos() {
  const seletor = 'button, [href], input, select, textarea, summary, [tabindex]:not([tabindex="-1"])';
  return [...$("cargos").querySelectorAll(seletor)]
    .filter((e) => !e.disabled && !e.closest("[hidden]") && e.getClientRects().length > 0);
}

function prenderOFocoDosCargos(evento) {
  if (evento.key !== "Tab" || $("cargos").hidden) return;
  const raiz = $("cargos");
  const lista = focaveisDosCargos();
  if (lista.length === 0) {
    evento.preventDefault();
    raiz.focus();
    return;
  }
  const primeiro = lista[0];
  const ultimo = lista[lista.length - 1];
  const ativo = document.activeElement;
  if (!raiz.contains(ativo)) {
    evento.preventDefault();
    (evento.shiftKey ? ultimo : primeiro).focus();
  } else if (evento.shiftKey && (ativo === primeiro || ativo === raiz)) {
    evento.preventDefault();
    ultimo.focus();
  } else if (!evento.shiftKey && ativo === ultimo) {
    evento.preventDefault();
    primeiro.focus();
  }
}

window.addEventListener("keydown", prenderOFocoDosCargos);
