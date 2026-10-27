// SEELE — ANTES DE HOSPEDAR (E2 da 0.17).
//
// # O caso que trouxe esta tela
//
// Quem atualizou para a 0.16 e mandou hospedar o servidor que a 0.15 deixou
// nesta máquina leu, na tela de ENTRADA, uma parede vermelha («ESTE SERVIDOR
// TEM MOD QUE ESTE APP NÃO RODA…») e, embaixo, um botão por saída por MOD —
// ATUALIZAR E HOSPEDAR, HOSPEDAR PELO SERVIDOR 0.15, DESLIGAR E HOSPEDAR —,
// repetidos a cada MOD e passando da janela. Três perguntas diferentes estavam
// misturadas numa lista de MODs:
//
// 1. **os dados**: hospedar por esta versão converte o banco do servidor, e
//    isso não era dito em lugar nenhum;
// 2. **a versão do servidor**: quem consegue entrar (o Servidor 0.15, o 1.0 e o
//    2.0 não conversam entre si), uma escolha do servidor inteiro, e não de um
//    MOD;
// 3. **os MODs**: o que este app roda.
//
// Esta tela as separa, uma por bloco, cada uma com o estado escrito e as
// ações dela. Nenhuma ação de MOD hospeda sozinha; HOSPEDAR é um botão só, no
// fim. O texto de cada bloco é estado, consequência e ação; o caminho da
// cópia, as APIs e os esquemas ficam em «mais» fechados (revisão do Codex).
//
// # A cópia de segurança: só a comprovação
//
// «Cópia do estado atual» só aparece com a comprovação do backend: a cópia que
// o `preparar_copia` acabou de criar e verificar, NESTA tela. Toda cópia que já
// estava no disco é histórico — a linha e a data, sem prometer que é atual —,
// qualquer que seja o `do_estado_atual` que a lista traga: o Codex mediu que
// inferir pelo nome e pela data de modificação erra com o WAL e com uma cópia
// corrompida, e toda conversão do `hospedar` cria e verifica uma cópia nova.
//
// - Convertendo: o HOSPEDAR é CRIAR CÓPIA, CONVERTER E HOSPEDAR — o `hospedar`
//   cria a cópia, a verifica e só então converte; falha fechado.
// - A cópia falhou (o `hospedar` voltou com `CopiaDeSeguranca`, ou o
//   `preparar_copia` desta tela): bloqueado, com a etapa e TENTAR DE NOVO.
// - A leitura não verificou os dados (`verificado: false`, um erro do comando,
//   ou o `hospedar` voltou com `DadosNaoVerificados`): NÃO FOI POSSÍVEL
//   VERIFICAR, bloqueado, com VERIFICAR DE NOVO. Nunca vira «nada a fazer».
//
// # De onde vem o que ela diz
//
// - `antes_de_hospedar(servidor)` (só leitura) e `preparar_copia(servidor)`
//   (cria e verifica), comandos do hosting-lifecycle (`src/antes_de_hospedar.rs`).
//   Sem o primeiro (uma versão do app que não o tem), a tela segue com a recusa
//   do `hospedar` e a versão que criou o servidor, e diz menos, sem inventar;
// - `linhas_para_hospedar`: as versões do servidor que esta máquina hospeda;
// - `catalogo_de_mods`: as versões dos MODs para a API deste app.
//
// Nada aqui apaga. Desligar um MOD deixa os dados dele (ADR 0045), e a
// conversão é do `hospedar`, que também confere a cópia (falha fechada).

"use strict";

/** O que a tela está mostrando, para as ações e para a corrida de desenhos. */
const antesDeHospedar = {
  geracao: 0,
  servidor: null,
  nome: "",
  info: null,
  /** `ok`, `falhou` (a leitura deu erro) ou `sem-comando` (a ponte não o tem). */
  leitura: "sem-comando",
  falhaDaLeitura: null,
  recusa: null,
  mods: [],
  /** Os MODs que ainda impedem de subir, pelo id. */
  pendentes: new Set(),
  /**
   * Os MODs com uma escolha estagiada (pelo id): o backend as guarda, e o
   * `hospedar` as aplica depois da cópia; nenhuma vai ao banco antes do CTA.
   */
  escolhas: new Set(),
  futuro: false,
  converte: false,
  /** `valida`, `ausente`, `falhou` ou `nao-se-aplica`. */
  copia: "nao-se-aplica",
  /** A cópia que o `preparar_copia` criou e verificou nesta tela: a única prova. */
  comprovacao: null,
  /** Por que a cópia não ficou pronta (`FalhaNaCopia`), desta tentativa. */
  falhaDaCopia: null,
  /** A linha dos dados, «1» ou «0.15», quando a pré-conferência disse. */
  linhaDosDados: null,
  /** Para onde o VOLTAR leva, e o que reabrir em VERIFICAR DE NOVO. */
  volta: "tela-boot",
  origem: "tela-boot",
};

/** O nome com que a tela chama um servidor guardado. */
function nomeDoServidorGuardado(guardado, id) {
  if (guardado?.nome) return guardado.nome;
  if (guardado && guardado.caminho === null) return "O SERVIDOR QUE JÁ ESTAVA AQUI";
  return id ?? "O SEU SERVIDOR";
}

/** Escreve o estado de um passo: o atributo (o filete) e a palavra. */
function estadoDoPasso(passo, estado) {
  $(passo).dataset.estado = estado;
  $(`${passo}-estado`).textContent = FRASES_ANTES_DE_HOSPEDAR.estado[estado];
}

/** A ponte não conhece o comando: o app de antes, e não uma falha da leitura. */
function comandoQueNaoExiste(falha) {
  if (falha?.ComandoDesconhecido || falha?.RecusaDaPonte) return true;
  return typeof falha === "string" && /not found|não existe/i.test(falha);
}

/**
 * Abre a tela para um servidor guardado (`servidor`, ou `null` para o de
 * sempre), com a recusa do `hospedar`, quando foi ela que trouxe a pessoa, com
 * a pré-conferência já lida (`info`), ou com a falha dela (`falhaDaLeitura`).
 */
async function abrirAntesDeHospedar({ servidor = null, recusa = null, info, falhaDaLeitura, origem = "tela-boot" } = {}) {
  const geracao = ++antesDeHospedar.geracao;
  Object.assign(antesDeHospedar, {
    servidor,
    recusa,
    info: null,
    leitura: "sem-comando",
    falhaDaLeitura: null,
    mods: [],
    pendentes: new Set(),
    escolhas: new Set(),
    futuro: false,
    converte: false,
    copia: "nao-se-aplica",
    comprovacao: null,
    // O `hospedar` voltou porque a cópia não ficou pronta: isso é desta
    // tentativa, e a tela abre dizendo onde parou.
    falhaDaCopia: recusa?.CopiaDeSeguranca?.falha ?? (recusa?.CopiaDeSeguranca ? {} : null),
    linhaDosDados: null,
    volta: origem === "tela-servidores" ? "tela-servidores" : "tela-boot",
    origem,
  });

  // A tela aparece antes das respostas: quem apertou HOSPEDAR vê para onde foi,
  // e os blocos se preenchem.
  if (!["tela-antes-de-hospedar"].includes(origem)) guardarFoco(origem);
  for (const tela of ["tela-boot", "tela-servidores", "tela-preparar"]) $(tela).hidden = true;
  $("tela-antes-de-hospedar").hidden = false;
  // Uma escolha de MOD que não se aplicou no `hospedar` (P1): dito no alto,
  // com o motivo, e a tela reaberta mostra a escolha que continua guardada.
  $("antes-erro").hidden = !recusa?.EscolhaDeModNaoAplicou;
  $("antes-erro").textContent = recusa?.EscolhaDeModNaoAplicou ? fraseDeErro(recusa) : "";
  $("antes-etapa").hidden = true;
  marcarOHospedar({ bloqueado: true, motivo: null, rotulo: FRASES_ANTES_DE_HOSPEDAR.hospedar() });
  $("antes-resumo").textContent = "lendo o servidor…";
  $("antes-pendencias").hidden = true;
  abrirTela("tela-antes-de-hospedar");

  const lerAPreconferencia = async () => {
    if (info !== undefined) return { estado: "ok", info };
    if (falhaDaLeitura !== undefined) return { estado: "falhou", falha: falhaDaLeitura };
    try {
      const lida = await invoke("antes_de_hospedar", { servidor });
      return lida ? { estado: "ok", info: lida } : { estado: "sem-comando" };
    } catch (falha) {
      console.warn("antes_de_hospedar:", falha);
      return comandoQueNaoExiste(falha) ? { estado: "sem-comando" } : { estado: "falhou", falha };
    }
  };
  const [guardados, leitura, linhas] = await Promise.all([
    invoke("servidores_guardados").catch(() => []),
    lerAPreconferencia(),
    invoke("linhas_para_hospedar").catch(() => []),
  ]);
  if (geracao !== antesDeHospedar.geracao) return;

  const lida = leitura.info ?? null;
  antesDeHospedar.info = lida;
  antesDeHospedar.leitura = leitura.estado;
  antesDeHospedar.falhaDaLeitura = leitura.falha ?? null;
  // **Lido não é verificado**: o comando responde, e diz que o banco não se
  // lê (`verificado: false`, com o motivo). E o `hospedar` que voltou com
  // `DadosNaoVerificados` diz o mesmo, mais recente que qualquer leitura.
  if (lida && lida.verificado === false) {
    antesDeHospedar.leitura = "falhou";
    antesDeHospedar.falhaDaLeitura = lida.motivo ?? FRASES_ANTES_DE_HOSPEDAR.dadosNaoVerificados;
  }
  if (recusa?.DadosNaoVerificados) {
    antesDeHospedar.leitura = "falhou";
    antesDeHospedar.falhaDaLeitura = recusa.DadosNaoVerificados.motivo ?? FRASES_ANTES_DE_HOSPEDAR.dadosNaoVerificados;
  }
  const guardado = (guardados ?? []).find((s) => s.id === servidor) ??
    (servidor === null ? (guardados ?? []).find((s) => s.caminho === null) : undefined);
  antesDeHospedar.nome = nomeDoServidorGuardado(guardado, servidor);
  const criadoNa = lida?.criado_na ?? guardado?.versao ?? null;
  $("antes-servidor").textContent = FRASES_ANTES_DE_HOSPEDAR.servidor(antesDeHospedar.nome, criadoNa);

  // A pré-conferência primeiro: ela lista os MODs como estão no banco, com os
  // já escolhidos (que a recusa do `hospedar` não lista mais), e as escolhas
  // guardadas vêm marcadas na linha deles.
  // A união das duas, pelo id: a recusa é a mais recente, e nada que uma
  // delas diga impedir fica de fora.
  const dosMods = new Map();
  for (const par of [...(lida?.mods_de_api_antiga ?? []), ...(recusa?.ModsDeApiAntiga?.mods ?? [])]) {
    if (!dosMods.has(par[0])) dosMods.set(par[0], par);
  }
  antesDeHospedar.mods = [...dosMods.values()];
  const desta = (linhas ?? []).find((l) => l.esta) ?? null;

  antesDeHospedar.pendentes = new Set(antesDeHospedar.mods.map(([id]) => id));
  antesDeHospedar.criadoNa = criadoNa;
  antesDeHospedar.desta = desta;
  antesDeHospedar.redesenharOsDados = () => desenharOsDados(lida, recusa, criadoNa, desta);
  antesDeHospedar.redesenharOsDados();
  antesDeHospedar.redesenharOsDetalhes = () => desenharOsDetalhes(servidor, criadoNa, lida, recusa);
  antesDeHospedar.redesenharOsDetalhes();
  antesDeHospedar.redesenharAVersao = () => desenharAVersao(linhas ?? [], desta, guardado, lida);
  antesDeHospedar.redesenharAVersao();
  await desenharOsMods(geracao);
  if (geracao !== antesDeHospedar.geracao) return;
  atualizarOHospedar();
}

/**
 * **DETALHES TÉCNICOS**, fechado: o que serve a quem dá suporte e não decide
 * nada para quem hospeda — os esquemas, a linha dos dados, as cópias com o
 * estado, a data e o caminho, e o nome da recusa. Sem a pré-conferência, a
 * linha dela diz que ela não respondeu, ou por quê, em vez de sumir.
 */
function desenharOsDetalhes(servidor, criadoNa, info, recusa) {
  const d = FRASES_ANTES_DE_HOSPEDAR.detalhe;
  const pares = [[d.servidor, servidor ?? "—"], [d.criadoNa, criadoNa ?? "—"]];
  if (info) {
    pares.push(
      [d.banco, info.banco ?? "—"],
      [d.linhaDosDados, info.linha_dos_dados ?? "—"],
      [d.esquemaGravado, textoDoEsquema(info.esquema)],
      [d.esquemaDesta, info.esquema_desta_versao ?? "—"],
      [d.converte, info.converte ? "sim" : "não"],
      ...(info.verificado === false ? [[d.semPreconferencia, `não verificado: ${info.motivo ?? "—"}`]] : []),
      // As que estavam no disco: histórico, sem «do estado atual».
      ...(info.copias ?? []).map((c) => [
        d.copia(servidorDaLinha(c.linha)),
        ["histórico", c.tipo, dataDaCopia(c.data), c.caminho].filter(Boolean).join(" · "),
      ]),
    );
  } else {
    const falha = antesDeHospedar.falhaDaLeitura;
    pares.push([d.semPreconferencia, falha ? `falhou: ${typeof falha === "string" ? falha : JSON.stringify(falha)}` : "não respondeu"]);
  }
  const prova = antesDeHospedar.comprovacao;
  if (prova) pares.push([d.copiaComprovada, [dataDaCopia(prova.data), prova.caminho].filter(Boolean).join(" · ")]);
  const parou = antesDeHospedar.falhaDaCopia;
  if (parou) pares.push([d.copiaFalhou, [parou.etapa, parou.motivo].filter(Boolean).join(": ") || "—"]);
  if (recusa) {
    const [nome, valor] = Object.entries(recusa)[0] ?? ["—", null];
    pares.push([d.recusa, valor && typeof valor === "object" && !Array.isArray(valor) && !valor.mods
      ? `${nome} ${JSON.stringify(valor)}` : nome]);
  }
  $("antes-detalhes-titulo").textContent = FRASES_ANTES_DE_HOSPEDAR.detalhes;
  $("antes-detalhes").open = false;
  $("antes-detalhes-lista").replaceChildren(...pares.flatMap(([termo, valor]) => [
    elemento("dt", null, termo),
    elemento("dd", null, String(valor)),
  ]));
}

/** Um botão de ação dentro de um bloco. */
function acaoDoBloco(rotulo, fazer) {
  const botao = elemento("button", "botao-fantasma", rotulo);
  botao.type = "button";
  botao.addEventListener("click", () => {
    Promise.resolve(fazer()).catch((falha) => console.warn(rotulo, falha));
  });
  return botao;
}

/** O `Esquema` do backend em texto, para «Detalhes técnicos». */
function textoDoEsquema(esquema) {
  if (esquema === undefined || esquema === null) return "—";
  if (typeof esquema === "number") return String(esquema);
  if (esquema === "Ausente") return "nenhum (banco novo)";
  if (esquema === "NaoESqlite") return "o arquivo não é um banco SQLite";
  if (esquema.Gravado !== undefined) return String(esquema.Gravado);
  if (esquema.Ilegivel !== undefined) return `não se lê: ${esquema.Ilegivel}`;
  return JSON.stringify(esquema);
}

/**
 * **A cópia do estado atual, só com a comprovação desta tela** (revisão do
 * Codex): a que o `preparar_copia` acabou de criar e verificar, da mesma linha
 * dos dados. Nenhuma cópia que já estava no disco vale como prova — nem com
 * `do_estado_atual` —; elas são histórico.
 */
function estadoDaCopia(info) {
  const prova = antesDeHospedar.comprovacao;
  if (prova && prova.do_estado_atual === true && prova.linha === info.linha_dos_dados) return "valida";
  if (antesDeHospedar.falhaDaCopia) return "falhou";
  return "ausente";
}

/** O passo 1: o que hospedar faz com os dados, e a cópia de segurança. */
function desenharOsDados(info, recusa, criadoNa, desta) {
  const frase = $("antes-dados-frase");
  const aviso = $("antes-dados-aviso");
  const copia = $("antes-dados-copia");
  const mais = $("antes-dados-mais");
  const acoes = $("antes-dados-acoes");
  acoes.replaceChildren();
  aviso.hidden = true;
  mais.hidden = true;
  // O que um desenho anterior escreveu sai: um texto escondido continua sendo
  // texto, e uma cópia de outro servidor não pode ficar aqui dentro.
  copia.textContent = "";
  aviso.textContent = "";
  delete aviso.dataset.tom;
  antesDeHospedar.futuro = false;
  antesDeHospedar.converte = false;
  antesDeHospedar.copia = "nao-se-aplica";

  const doFuturo = recusa?.BancoDoFuturo ?? (info?.futuro ? {} : null);
  if (doFuturo) {
    antesDeHospedar.futuro = true;
    estadoDoPasso("antes-dados", "resolva");
    frase.textContent = FRASES_ANTES_DE_HOSPEDAR.dadosDoFuturo();
    acoes.append(acaoDoBloco(FRASES_ANTES_DE_HOSPEDAR.procurarAtualizacao, () =>
      abrirServer("tela-antes-de-hospedar").then(() => abrirSecao("secao-atualizacao"))));
    return;
  }

  // **Uma leitura que falhou não é «nada a fazer»** (revisão do Codex).
  if (antesDeHospedar.leitura === "falhou") {
    estadoDoPasso("antes-dados", "resolva");
    frase.textContent = FRASES_ANTES_DE_HOSPEDAR.dadosNaoVerificados;
    const falha = antesDeHospedar.falhaDaLeitura;
    aviso.textContent = typeof falha === "string" ? falha : fraseDeErro(falha);
    aviso.hidden = false;
    acoes.append(acaoDoBloco(FRASES_ANTES_DE_HOSPEDAR.verificarDeNovo, () =>
      abrirAntesDeHospedar({ servidor: antesDeHospedar.servidor, recusa: antesDeHospedar.recusa, origem: antesDeHospedar.origem })));
    return;
  }

  const servidorDesta = desta?.servidor ?? "desta versão";
  if (info) {
    antesDeHospedar.converte = Boolean(info.converte);
    antesDeHospedar.linhaDosDados = info.linha_dos_dados ?? null;
    if (!info.converte) {
      estadoDoPasso("antes-dados", "ok");
      frase.textContent = FRASES_ANTES_DE_HOSPEDAR.dadosNoFormato;
      return;
    }
    const de = servidorDaLinha(info.linha_dos_dados ?? criadoNa ?? "anterior");
    frase.textContent = FRASES_ANTES_DE_HOSPEDAR.dadosConverte(de, servidorDesta);
    const estado = estadoDaCopia(info);
    antesDeHospedar.copia = estado;
    // **Toda cópia do disco é histórico**: a linha e a data, e nada sobre o
    // estado atual — o `do_estado_atual` da lista não é lido aqui.
    const historico = (info.copias ?? []).map((c) =>
      FRASES_ANTES_DE_HOSPEDAR.copiaAnterior(servidorDaLinha(c.linha), dataDaCopia(c.data)));
    const comHistorico = (linhas) => {
      const todas = historico.length ? [...linhas, FRASES_ANTES_DE_HOSPEDAR.historicoDeCopias, ...historico] : linhas;
      copia.textContent = todas.join("\n");
      mais.hidden = todas.length === 0;
    };
    if (estado === "valida") {
      const prova = antesDeHospedar.comprovacao;
      estadoDoPasso("antes-dados", "leia");
      // A prova curta à vista; o caminho no «mais».
      aviso.textContent = FRASES_ANTES_DE_HOSPEDAR.copiaDoEstadoAtual(de, dataDaCopia(prova.data));
      aviso.dataset.tom = "ok";
      aviso.hidden = false;
      comHistorico([FRASES_ANTES_DE_HOSPEDAR.ondeFica(prova.caminho)]);
    } else if (estado === "falhou") {
      const parou = antesDeHospedar.falhaDaCopia;
      estadoDoPasso("antes-dados", "resolva");
      aviso.textContent = `${FRASES_ANTES_DE_HOSPEDAR.copiaNaoDeu(de)}\n${fraseDaCopia(parou)}`;
      aviso.dataset.tom = "material";
      aviso.hidden = false;
      acoes.append(acaoDoBloco(FRASES_ANTES_DE_HOSPEDAR.tentarACopiaDeNovo, () => criarACopia()));
      comHistorico([]);
    } else {
      // Convertendo: o `hospedar` cria uma cópia nova, a verifica e só então
      // converte (falha fechado). Não bloqueia; o aviso fica à vista.
      estadoDoPasso("antes-dados", "leia");
      aviso.textContent = FRASES_ANTES_DE_HOSPEDAR.semCopiaAinda(de);
      aviso.dataset.tom = "material";
      aviso.hidden = false;
      comHistorico([]);
    }
    return;
  }

  // Sem a pré-conferência (a ponte não a tem): o que a lista sabe, e só isso.
  if (criadoNa) {
    estadoDoPasso("antes-dados", "leia");
    frase.textContent = FRASES_ANTES_DE_HOSPEDAR.dadosConverteSemDetalhe(criadoNa);
  } else {
    estadoDoPasso("antes-dados", "ok");
    frase.textContent = FRASES_ANTES_DE_HOSPEDAR.dadosSemVersao;
  }
}

/**
 * TENTAR A CÓPIA DE NOVO: o `preparar_copia` cria uma cópia do estado atual e a
 * verifica, sem converter nada. A cópia que ele devolve é a única que esta
 * tela chama de «do estado atual»; a falha diz a etapa e bloqueia.
 */
async function criarACopia() {
  const geracao = antesDeHospedar.geracao;
  const etapa = $("antes-etapa");
  etapa.hidden = false;
  etapa.textContent = FRASES_ANTES_DE_HOSPEDAR.criandoCopia;
  marcarOHospedar({ bloqueado: true, motivo: FRASES_ANTES_DE_HOSPEDAR.criandoCopia, rotulo: $("antes-hospedar").textContent });
  let feita = null;
  let falhou = null;
  try {
    feita = await invoke("preparar_copia", { servidor: antesDeHospedar.servidor });
  } catch (falha) {
    falhou = falha;
  }
  if (geracao !== antesDeHospedar.geracao) return;
  if (falhou || !feita || feita.do_estado_atual !== true) {
    etapa.hidden = true;
    antesDeHospedar.comprovacao = null;
    antesDeHospedar.falhaDaCopia = falhou && typeof falhou === "object" ? falhou : { motivo: falhou ? String(falhou) : null };
  } else {
    antesDeHospedar.comprovacao = feita;
    antesDeHospedar.falhaDaCopia = null;
    etapa.textContent = FRASES_ANTES_DE_HOSPEDAR.copiaCriada(feita.caminho, dataDaCopia(feita.data));
  }
  antesDeHospedar.redesenharOsDados();
  antesDeHospedar.redesenharOsDetalhes?.();
  atualizarOHospedar();
  $("antes-dados").focus();
}

/** O que dizer de uma cópia que não ficou pronta, pela etapa (`FalhaNaCopia`). */
function fraseDaCopia(falha) {
  return FRASES_ANTES_DE_HOSPEDAR.copiaParou[falha?.etapa] ?? FRASES_ANTES_DE_HOSPEDAR.copiaParouSemEtapa;
}

/**
 * O passo 2: quem consegue entrar. Esta janela sobe o servidor da versão dela;
 * as anteriores, quando esta máquina as tem e este servidor tem dados para
 * elas, abrem noutra janela — num «mais», aberto quando um MOD só roda lá.
 */
function desenharAVersao(linhas, desta, guardado, info) {
  const lista = $("antes-versao-lista");
  const servidorDesta = desta?.servidor ?? "desta versão";
  // As anteriores valem para o servidor que tem cópia guardada para elas (são
  // os dados que elas abririam); sem a pré-conferência, para o adotado.
  // Só a cópia da linha legada (a da primeira abertura) é o que o launcher
  // abre por uma versão anterior; a cópia de segurança deste banco não.
  const comCopia = new Set((info?.copias ?? []).filter((c) => c.tipo === "linha-legada").map((c) => c.linha));
  const adotado = guardado?.caminho === null;
  const anteriores = linhas.filter((l) => !l.esta && (info ? comCopia.has(l.linha) : adotado));

  // Um MOD que só uma versão anterior roda é o que faz esta escolha importar.
  // Só os que ainda impedem: um MOD já resolvido não prende a versão nenhuma.
  const soLa = (l) => antesDeHospedar.mods
    .filter(([id, api]) => antesDeHospedar.pendentes.has(id) && api && l.apis.includes(api) && !(desta?.apis ?? []).includes(api))
    .map(([id]) => id);

  let frase = FRASES_ANTES_DE_HOSPEDAR.versaoDesta(servidorDesta);
  if (anteriores.length === 0) frase += `\n${FRASES_ANTES_DE_HOSPEDAR.versaoSoEsta}`;
  const forcadas = anteriores.filter((l) => soLa(l).length > 0);
  for (const l of forcadas) {
    frase += `\n${FRASES_ANTES_DE_HOSPEDAR.versaoForcada(l.servidor, soLa(l).join(", "))}`;
  }
  $("antes-versao-frase").textContent = frase;
  estadoDoPasso("antes-versao", forcadas.length > 0 ? "leia" : "ok");
  const mais = $("antes-versao-mais");
  mais.hidden = anteriores.length === 0;
  $("antes-versao-mais-titulo").textContent = FRASES_ANTES_DE_HOSPEDAR.versaoAnteriores(anteriores.length);
  // Aberto quando é a saída de um MOD que impede: aí a escolha é do bloco.
  if (forcadas.length > 0) mais.open = true;

  lista.replaceChildren(...anteriores.map((l) => {
    const item = elemento("li", "antes-linha");
    item.dataset.linha = l.linha;
    const texto = elemento("span", "antes-linha-texto");
    texto.append(elemento("span", "antes-linha-nome", `SERVIDOR ${l.servidor}`));
    const disponivel = l.instalada ? "" : ` · ${l.tem_pacote ? FRASES_ANTES_DE_HOSPEDAR.versaoBaixar : FRASES_ANTES_DE_HOSPEDAR.versaoSemPacote}`;
    texto.append(elemento("span", "antes-linha-sub", FRASES_ANTES_DE_HOSPEDAR.versaoOutra(l.servidor, l.app) + disponivel));
    item.append(texto);

    const acoes = elemento("span", "antes-linha-acoes");
    const ir = elemento("button", "botao-fantasma", FRASES_ANTES_DE_HOSPEDAR.hospedarPor(l.servidor));
    ir.type = "button";
    ir.dataset.saida = `servidor-${l.servidor}`;
    ir.disabled = !l.instalada && !l.tem_pacote;
    ir.addEventListener("click", () => {
      const motivo = soLa(l);
      $("tela-antes-de-hospedar").hidden = true;
      abrirPreparar(antesDeHospedar.servidor)
        .then(() => escolherVersaoParaHospedar(l, motivo.length > 0 ? `${motivo.join(", ")} roda${motivo.length > 1 ? "m" : ""} no Servidor ${l.servidor}. ` : ""))
        .catch((falha) => console.warn("hospedar por outra versão:", falha));
    });
    acoes.append(ir);
    item.append(acoes);
    return item;
  }));
}

/** O passo 3: os MODs que este app não roda, cada um com as saídas dele. */
async function desenharOsMods(geracao) {
  const mods = antesDeHospedar.mods;
  const mais = $("antes-mods-mais");
  if (mods.length === 0) {
    estadoDoPasso("antes-mods", "ok");
    $("antes-mods-frase").textContent = FRASES_ANTES_DE_HOSPEDAR.modsNenhum;
    mais.hidden = true;
    esconderSaidasDeQuemHospeda();
    return;
  }
  estadoDoPasso("antes-mods", "resolva");
  $("antes-mods-frase").textContent = FRASES_ANTES_DE_HOSPEDAR.modsPendentes;
  // As APIs, para quem quer saber por quê: no «mais», e não na linha.
  repovoar($("antes-mods-apis"), mods.map(([id, api]) => elemento("li", null,
    FRASES_ANTES_DE_HOSPEDAR.modApi(id, api, [...APIS_QUE_ESTE_APLICATIVO_ACEITA].sort().join(" e "), APIS_DO_SERVIDOR_0_15.includes(api) ? APIS_DO_SERVIDOR_0_15_DITAS : null))));
  mais.hidden = false;
  // **As escolhas ficam na tela até o botão do fim** (P1 do Codex): nada vai ao
  // banco agora. O estado do bloco diz «escolhido, a aplicar», e não «feito».
  const redesenharOsMods = () => {
    const n = antesDeHospedar.pendentes.size;
    if (n === 0) {
      estadoDoPasso("antes-mods", "leia");
      $("antes-mods-frase").textContent = FRASES_ANTES_DE_HOSPEDAR.modsEscolhidos;
    } else {
      estadoDoPasso("antes-mods", "resolva");
      $("antes-mods-frase").textContent = FRASES_ANTES_DE_HOSPEDAR.modsPendentes;
    }
    antesDeHospedar.redesenharAVersao?.();
    atualizarOHospedar();
  };
  await desenharSaidasDeQuemHospeda(mods, antesDeHospedar.servidor, {
    escolhidas: antesDeHospedar.info?.escolhas_pendentes ?? [],
    aoResolver: (id) => {
      if (geracao !== antesDeHospedar.geracao) return;
      antesDeHospedar.pendentes.delete(id);
      antesDeHospedar.escolhas.add(id);
      redesenharOsMods();
    },
    aoDesfazer: (id) => {
      if (geracao !== antesDeHospedar.geracao) return;
      antesDeHospedar.escolhas.delete(id);
      antesDeHospedar.pendentes.add(id);
      redesenharOsMods();
    },
  });
}

/**
 * O estado do HOSPEDAR: **apagado de verdade** quando bloqueado — sem
 * preenchimento, com `aria-disabled` e o motivo ao lado, ligado pelo
 * `aria-describedby` (revisão do Codex). Ele continua focável, para que o
 * leitor de tela leia o porquê; o clique não faz nada.
 */
function marcarOHospedar({ bloqueado, motivo, rotulo }) {
  const botao = $("antes-hospedar");
  botao.textContent = rotulo;
  botao.setAttribute("aria-disabled", bloqueado ? "true" : "false");
  botao.dataset.bloqueado = bloqueado ? "sim" : "nao";
  const linha = $("antes-hospedar-motivo");
  linha.textContent = motivo ?? "";
  linha.hidden = !motivo;
}

/** O HOSPEDAR do fim, o resumo do alto e os links para o que pede atenção. */
function atualizarOHospedar() {
  const f = FRASES_ANTES_DE_HOSPEDAR;
  const n = antesDeHospedar.pendentes.size;
  const naoVerificado = antesDeHospedar.leitura === "falhou";
  const copiaFalhou = antesDeHospedar.converte && antesDeHospedar.copia === "falhou";
  const precisaDeCopia = antesDeHospedar.converte && antesDeHospedar.copia === "ausente";
  const motivo = antesDeHospedar.futuro ? f.motivo.futuro
    : naoVerificado ? f.motivo.naoVerificado
      : copiaFalhou ? f.motivo.semCopia
        : n > 0 ? f.motivo.mods(n)
          : precisaDeCopia ? f.motivo.criaCopia
            : null;
  const bloqueado = antesDeHospedar.futuro || naoVerificado || copiaFalhou || n > 0;
  // Toda conversão cria e verifica a sua cópia, mesmo depois de uma feita
  // aqui: o rótulo diz isso sempre que converte.
  const escolhidos = antesDeHospedar.escolhas.size;
  const rotulo = antesDeHospedar.converte ? f.criarCopiaConverterEHospedar()
    : escolhidos > 0 ? f.aplicarEHospedar()
      : f.hospedar();
  // As escolhas, ao lado do botão que as aplica: é ele que muda o banco.
  const doBotao = [motivo, escolhidos > 0 ? f.motivo.escolhas(escolhidos) : null].filter(Boolean).join(" ") || null;
  marcarOHospedar({ bloqueado, motivo: bloqueado ? motivo : doBotao, rotulo });

  // **Nenhuma frase diz «nada mudou»** (P1 do Codex): esta tela não tem como
  // provar isso. Ela diz o que está escolhido, e o que o botão do fim aplica.
  const leia = ["antes-dados", "antes-versao", "antes-mods"].some((p) => $(p).dataset.estado === "leia");
  $("antes-resumo").textContent = bloqueado ? f.resumoBloqueia
    : escolhidos > 0 ? f.resumoEscolhas(escolhidos)
      : leia ? f.resumoLeia : f.resumoOk;

  // O que pede atenção, no alto, cada um levando ao bloco dele.
  const itens = [];
  if (antesDeHospedar.futuro || copiaFalhou) itens.push(["antes-dados", f.pendencias.dados]);
  else if (naoVerificado) itens.push(["antes-dados", f.pendencias.dadosNaoVerificados]);
  else if (antesDeHospedar.converte) itens.push(["antes-dados", f.pendencias.converte]);
  if (n > 0) itens.push(["antes-mods", f.pendencias.mods(n)]);
  repovoar($("antes-pendencias"), itens.map(([bloco, texto]) => {
    const item = elemento("li");
    const ir = elemento("button", "antes-ir", texto);
    ir.type = "button";
    ir.dataset.bloco = bloco;
    ir.addEventListener("click", () => {
      // Uma pendência leva ao bloco, e o bloco recolhido abre para ela.
      if ($(bloco).dataset.recolhido === "sim") { $(bloco).dataset.escolhaDaPessoa = "sim"; $(bloco).dataset.recolhido = "nao"; organizarParaCelular(); }
      $(bloco).scrollIntoView({ block: "start" });
      $(bloco).focus({ preventScroll: true });
    });
    item.append(ir);
    return item;
  }));
  $("antes-pendencias").hidden = itens.length === 0;
  organizarParaCelular();
}

// ---------------------------------------------------------- o celular

const ANTES_CELULAR = typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(max-width: 700px)") : null;
const PASSOS_DO_ANTES = ["antes-dados", "antes-versao", "antes-mods"];
const PESO_DO_ESTADO = { resolva: 0, leia: 1 };

/**
 * No celular (< 700px) o que pede ação vem primeiro e o que não pede vem recolhido, com um botão que abre e fecha. A ordem do DOM é a ordem
 * que se vê, para o foco do teclado e o leitor de tela andarem na mesma sequência. No desktop, a ordem é a numerada, e nada vem recolhido.
 */
function organizarParaCelular() {
  const lista = document.querySelector("#tela-antes-de-hospedar .antes-passos");
  if (!lista) return;
  const passos = PASSOS_DO_ANTES.map($);
  const estreito = Boolean(ANTES_CELULAR && ANTES_CELULAR.matches);
  const ordem = estreito
    ? [...passos].sort((a, b) => (PESO_DO_ESTADO[a.dataset.estado] ?? 2) - (PESO_DO_ESTADO[b.dataset.estado] ?? 2) || passos.indexOf(a) - passos.indexOf(b))
    : passos;
  // Só mexe no DOM se a ordem mudou: reinserir um nó com o foco o perderia.
  if (ordem.some((p, i) => lista.children[i] !== p)) for (const p of ordem) lista.appendChild(p);
  const f = FRASES_ANTES_DE_HOSPEDAR.recolher;
  for (const passo of passos) {
    let abre = passo.querySelector(":scope > .antes-abre");
    if (!abre) {
      abre = elemento("button", "antes-abre botao-fantasma", f.abrir);
      abre.type = "button";
      abre.setAttribute("aria-controls", passo.id);
      abre.addEventListener("click", () => {
        passo.dataset.recolhido = passo.dataset.recolhido === "sim" ? "nao" : "sim";
        passo.dataset.escolhaDaPessoa = "sim";
        organizarParaCelular();
      });
      passo.append(abre);
    }
    // Recolhe o que não pede nada, e só enquanto a pessoa não decidiu abrir ou fechar.
    if (passo.dataset.escolhaDaPessoa !== "sim") passo.dataset.recolhido = estreito && passo.dataset.estado === "ok" ? "sim" : "nao";
    const recolhido = passo.dataset.recolhido === "sim";
    abre.textContent = recolhido ? f.abrir : f.fechar;
    abre.setAttribute("aria-expanded", String(!recolhido));
  }
}

if (ANTES_CELULAR && ANTES_CELULAR.addEventListener) ANTES_CELULAR.addEventListener("change", organizarParaCelular);

/** Sai da tela sem hospedar, para de onde a pessoa veio. */
function fecharAntesDeHospedar() {
  antesDeHospedar.geracao += 1;
  $("tela-antes-de-hospedar").hidden = true;
  esconderSaidasDeQuemHospeda();
  const destino = antesDeHospedar.volta;
  $(destino).hidden = false;
  voltarParaTela(destino);
}

/** Hospeda o mesmo servidor; uma recusa nova volta para cá (`hospedar`). */
function hospedarDeVerdade() {
  const servidor = antesDeHospedar.servidor;
  antesDeHospedar.geracao += 1;
  $("tela-antes-de-hospedar").hidden = true;
  $("tela-boot").hidden = false;
  voltarParaTela("tela-boot");
  servidorEscolhido = servidor;
  hospedar().catch((falha) => console.warn("hospedar:", falha));
}

$("antes-voltar").addEventListener("click", fecharAntesDeHospedar);

$("antes-hospedar").addEventListener("click", () => {
  if ($("antes-hospedar").getAttribute("aria-disabled") === "true") {
    // Apagado: leva ao motivo, e não faz nada.
    $("antes-hospedar-motivo").scrollIntoView({ block: "nearest" });
    return;
  }
  // Convertendo, a cópia nova é do `hospedar` (o portão da conversão): ele a
  // cria, a verifica e só então converte; se ela não fica pronta, ele volta
  // com `CopiaDeSeguranca` e esta tela reabre dizendo a etapa.
  hospedarDeVerdade();
});
