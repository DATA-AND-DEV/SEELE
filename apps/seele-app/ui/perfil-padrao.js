// SEELE — o perfil padrão (0.19, decisão do dono «Padrão + ajuste por servidor»).
//
// O «Seu perfil» da tela inicial edita o perfil inteiro — nome exibido,
// pronomes, status, bio, cor, efeito, foto e capa —, e o que se salva ali é o
// **padrão desta máquina** (desktop: `perfil_padrao.rs`, na pasta das
// preferências; navegador: o IndexedDB da origem, `ponte-web.js`). Este arquivo
// decide **quando** o padrão vai a um servidor, e é o mesmo nas duas cascas:
//
// 1. **Ao entrar num servidor 12 onde você não tem perfil**, ele é aplicado:
//    `GravarPerfil` com revisão zero e depois a foto e a capa. «Não tenho
//    perfil» é a **ausência** da pessoa na lista `Perfis` do servidor (quem tem
//    perfil vazio tem linha, e é tratado como quem tem perfil). Revisão zero é
//    também o guarda do servidor: se a lista estava velha e há uma linha, ele
//    responde `RevisaoVelha`, e nada é sobrescrito.
// 2. **Quem foi criado assim segue o padrão** (o registro por servidor, pela
//    impressão dele: `segue`, a revisão do padrão aplicada e a assinatura do
//    que foi aplicado). Quando o padrão muda, cada servidor que o segue o
//    recebe na próxima vez que estiver na vista — na hora, para o que está.
// 3. **Mexer no perfil dentro de um servidor vale só ali** e desliga o
//    `segue` (`deixarDeSeguirOPadrao`, chamado pelo editor). Um perfil que
//    mudou por outro caminho (outro aparelho) não bate com a assinatura, e
//    também deixa de seguir: um perfil personalizado nunca é sobrescrito.
// 4. **USAR MEU PERFIL PADRÃO** (no editor, dentro do servidor) aplica o
//    padrão ali e volta a seguir.
// 5. **Num servidor 0.18, nada**: `perfisOficiaisDisponiveis()` é falso, e o
//    caminho velho do MOD fica intocado.
// 6. **A falha de aplicar é dita** — no editor daquele servidor, no aviso da
//    coluna e no registro (`registrarNoAnfitriao`) —, e tentada de novo na
//    próxima conexão, nunca em laço na mesma.
//
// Por que aqui, e não na sessão do Rust: o navegador tem de fazer o mesmo, e a
// lista `Perfis` que distingue «sem perfil» de «perfil vazio» é a que esta
// casca já lê (`perfis.js`).

"use strict";

/** O que esta janela sabe do padrão e do servidor da vista. */
const estadoDoPadrao = {
  /** O `perfil_padrao` lido por último, ou `null`. */
  atual: null,
  /** O registro do servidor da vista: `{ chave, servidor, segue, revisao_aplicada, assinatura }`. */
  seguimento: null,
  /** Uma aplicação por vez; a conferência que chega no meio roda depois. */
  aplicando: false,
  conferirDepois: false,
  /** `sessao:revisao` já tentados nesta janela: uma falha não vira laço. */
  tentados: new Set(),
  /** A falha de aplicar, por sessão: `sessao` → frase. */
  falhas: new Map(),
  /** Quem espera um desfecho (o `PerfilMudou` ou o `PerfilRecusado`). */
  esperando: [],
};

/** O padrão lido por último (`null` antes da primeira leitura). */
function perfilPadraoAtual() {
  return estadoDoPadrao.atual;
}

/**
 * **Lê o padrão desta máquina.** `null` quando a leitura falha — dito no
 * console, e o «Seu perfil» da tela inicial abre no modo de sempre.
 */
async function lerOPerfilPadrao() {
  try {
    estadoDoPadrao.atual = await invoke("perfil_padrao");
  } catch (falha) {
    console.warn("perfil_padrao:", falha);
    return null;
  }
  return estadoDoPadrao.atual;
}

/** Os textos do padrão no formato do `gravar_perfil`. */
function entradaDoPadrao(padrao) {
  return {
    nome_exibido: padrao?.nome_exibido ?? "",
    pronomes: padrao?.pronomes ?? "",
    status: padrao?.status ?? "",
    bio: padrao?.bio ?? "",
    cor: (padrao?.cor ?? COR_PADRAO_DO_PERFIL).toLowerCase(),
    efeito: padrao?.efeito ?? "Nenhum",
  };
}

/**
 * **A assinatura de um perfil**: os textos (aparados, como o servidor grava),
 * a cor, o efeito e os hashes das imagens. Dois perfis com a mesma assinatura
 * mostram o mesmo; é por ela que se sabe se o perfil de um servidor que segue
 * o padrão ainda é o que foi aplicado.
 */
function assinaturaDoPerfil(perfil) {
  return JSON.stringify([
    ...["nome_exibido", "pronomes", "status", "bio"].map((k) => (perfil?.[k] ?? "").trim()),
    (perfil?.cor ?? COR_PADRAO_DO_PERFIL).toLowerCase(),
    perfil?.efeito ?? "Nenhum",
    perfil?.foto?.hash ?? "",
    perfil?.capa?.hash ?? "",
  ]);
}

/**
 * **A decisão, pura** — o que fazer com o padrão no servidor da vista:
 *
 * - `nada`: servidor 0.18, padrão nunca salvo, servidor sem impressão, um
 *   servidor personalizado, ou um que segue e já está em dia;
 * - `primeira`: você não tem perfil aqui (a ausência na lista);
 * - `seguir`: o servidor segue o padrão e o padrão mudou;
 * - `soltar`: o servidor seguia, mas o perfil de lá não é mais o aplicado —
 *   mudou por outro caminho, e deixa de seguir sem ser tocado.
 */
function decidirOPadrao({ disponivel, meu, padrao, seguimento }) {
  if (!disponivel) return { acao: "nada", porque: "servidor-antigo" };
  if (!padrao || !(Number(padrao.revisao) > 0)) return { acao: "nada", porque: "sem-padrao" };
  if (!seguimento?.servidor) return { acao: "nada", porque: "sem-impressao" };
  if (!meu) return { acao: "primeira" };
  if (!seguimento.segue) return { acao: "nada", porque: "personalizado" };
  if (seguimento.assinatura && assinaturaDoPerfil(meu) !== seguimento.assinatura) {
    return { acao: "soltar", porque: "mudou-por-fora" };
  }
  if (Number(seguimento.revisao_aplicada) === Number(padrao.revisao)) return { acao: "nada", porque: "em-dia" };
  return { acao: "seguir" };
}

/** O carimbo da vista agora: os pedidos de uma aplicação vão todos à mesma sessão. */
function carimboDoPadrao() {
  if (typeof vistaDaJanela !== "object" || vistaDaJanela.sessao === null) return null;
  return { sessao: vistaDaJanela.sessao, geracao: vistaDaJanela.geracao };
}

/** O registro do servidor da vista, lido uma vez por sessão e pessoa. */
async function seguimentoDaVista(carimbo) {
  const chave = `${carimbo.sessao}:${perfisOficiais.me}`;
  if (estadoDoPadrao.seguimento?.chave === chave) return estadoDoPadrao.seguimento;
  const lido = await invoke("seguimento_do_perfil_padrao", { carimbo });
  estadoDoPadrao.seguimento = { ...lido, chave };
  return estadoDoPadrao.seguimento;
}

/**
 * Grava o registro do servidor da sessão `carimbo`. **A cópia desta janela
 * muda antes do disco**, e não depois: uma conferência que começa enquanto o
 * disco grava (a lista nova que o SALVAR do editor provoca) já lê «não
 * segue», e não aplica o padrão por cima do que se acabou de personalizar.
 */
async function guardarOSeguimento(carimbo, { segue, revisao_aplicada, assinatura }) {
  const chave = `${carimbo.sessao}:${perfisOficiais.me}`;
  if (estadoDoPadrao.seguimento?.chave === chave) {
    estadoDoPadrao.seguimento = { ...estadoDoPadrao.seguimento, segue, revisao_aplicada, assinatura: assinatura ?? null };
  }
  await invoke("guardar_seguimento_do_perfil_padrao", {
    carimbo,
    segue,
    revisaoAplicada: revisao_aplicada,
    assinatura: assinatura ?? null,
  });
}

/** Acorda quem espera um desfecho (chamado por `avisarQuemEsperaOPerfil`, em `perfis.js`). */
function avisarQuemAplicaOPadrao(evento = null) {
  for (const esperando of [...estadoDoPadrao.esperando]) esperando(evento);
}

/**
 * Espera até `pronto()` valer (conferido a cada lista nova e já agora), uma
 * recusa do servidor, ou o prazo. `{ ok }`, `{ recusa }` ou `{ semResposta }`.
 */
function esperarNoPadrao(pronto, prazoMs = 10_000) {
  return new Promise((resolver) => {
    let feito = false;
    const acabar = (valor) => {
      if (feito) return;
      feito = true;
      clearTimeout(prazo);
      estadoDoPadrao.esperando = estadoDoPadrao.esperando.filter((f) => f !== olhar);
      resolver(valor);
    };
    const olhar = (evento) => {
      if (evento?.PerfilRecusado) return acabar({ recusa: evento.PerfilRecusado });
      if (pronto()) acabar({ ok: true });
    };
    const prazo = setTimeout(() => acabar({ semResposta: true }), prazoMs);
    estadoDoPadrao.esperando.push(olhar);
    olhar(null);
  });
}

/** Uma recusa do caminho de aplicar, com a frase que a pessoa lê. */
class FalhaDoPadrao extends Error {
  constructor(frase, causa) {
    super(frase);
    this.frase = frase;
    this.causa = causa;
  }
}

/** `GravarPerfil` com os textos do padrão, e o desfecho. */
async function gravarOPadraoNoServidor(carimbo, revisao, entrada) {
  const me = perfisOficiais.me;
  const desfecho = esperarNoPadrao(() => {
    const meu = perfilOficial(me);
    return Boolean(meu && meu.revisao > revisao && mesmosTextos(textosDoPerfil(meu), entrada));
  });
  await invoke("gravar_perfil", { revisao, entrada, carimbo });
  return desfecho;
}

/**
 * **Aplica o padrão no servidor da vista.** `primeira`: revisão zero, e o
 * servidor que já tem uma linha recusa (a lista estava velha) — então não é
 * a primeira, e o servidor não passa a seguir. `seguir`: a revisão de agora.
 * Depois dos textos, a foto e a capa — só as que o servidor ainda não tem
 * (pelo hash) — e, num servidor que segue, tira o que o padrão não tem mais.
 *
 * O registro é escrito **antes** (`segue`, revisão zero): uma aplicação
 * cortada no meio (a vista trocou, a conexão caiu) é retomada na próxima
 * conferência, e não esquecida num servidor que ficaria sem seguir.
 */
async function aplicarOPadrao(acao, carimbo, { forcar = false, visto = null } = {}) {
  const padrao = estadoDoPadrao.atual;
  const me = perfisOficiais.me;
  const entrada = entradaDoPadrao(padrao);
  const esperada = assinaturaDoPerfil({ ...entrada, foto: padrao.foto, capa: padrao.capa });
  await guardarOSeguimento(carimbo, { segue: true, revisao_aplicada: 0, assinatura: null });

  // **A revisão é a do perfil que a decisão viu**, e não a da lista que chegou
  // durante a espera: se o perfil mudou por fora nesse meio-tempo, o servidor
  // responde `RevisaoVelha`, e ele não é sobrescrito.
  // **Um nome só** (0.19): o nome do padrão vai também ao registro do
  // servidor, e antes do perfil — recusado ali, o perfil não é gravado com um
  // nome exibido que o registro não tem.
  const alinhado = await alinharONomeNoServidor(carimbo, entrada.nome_exibido);
  if (alinhado.frase) throw new FalhaDoPadrao(alinhado.frase, "apelido");

  let meu = visto;
  if (acao === "primeira" || !meu || !mesmosTextos(textosDoPerfil(meu), entrada)) {
    let revisao = acao === "primeira" ? 0 : (meu?.revisao ?? 0);
    let desfecho = await gravarOPadraoNoServidor(carimbo, revisao, entrada);
    if (desfecho.recusa?.motivo === "RevisaoVelha" && acao === "primeira") {
      // Havia perfil: a lista é que estava velha. Ele não é tocado, e o
      // servidor não segue o padrão.
      await guardarOSeguimento(carimbo, { segue: false, revisao_aplicada: 0, assinatura: null });
      return { resultado: "ja-tinha" };
    }
    if (desfecho.recusa?.motivo === "RevisaoVelha" && !forcar) {
      // O perfil de lá mudou depois da lista que esta janela leu: outro
      // aparelho, outra janela. Ele não é sobrescrito, e o servidor deixa de
      // seguir o padrão.
      await guardarOSeguimento(carimbo, { segue: false, revisao_aplicada: 0, assinatura: null });
      return { resultado: "mudou-por-fora" };
    }
    if (desfecho.recusa?.motivo === "RevisaoVelha") {
      // USAR MEU PERFIL PADRÃO é pedido de quem está olhando: espera a lista
      // alcançar o servidor, e tenta mais uma vez com a revisão de agora.
      await esperarNoPadrao(() => (perfilOficial(me)?.revisao ?? 0) !== revisao, 3000);
      revisao = perfilOficial(me)?.revisao ?? revisao;
      desfecho = await gravarOPadraoNoServidor(carimbo, revisao, entrada);
    }
    if (desfecho.semResposta) throw new FalhaDoPadrao(FRASES_DOS_PERFIS.padrao.semResposta, "sem-resposta");
    if (desfecho.recusa) {
      const { motivo } = desfecho.recusa;
      throw new FalhaDoPadrao(fraseDaImagemDoPerfil({ Recusada: { motivo } }), desfecho.recusa);
    }
  }

  meu = perfilOficial(me);
  for (const lado of ["Foto", "Capa"]) {
    const quer = lado === "Capa" ? padrao.capa : padrao.foto;
    const tem = lado === "Capa" ? meu?.capa : meu?.foto;
    if (quer && tem?.hash !== quer.hash) {
      await invoke("enviar_imagem_do_perfil_padrao", { lado, hash: quer.hash, carimbo });
    } else if (!quer && tem) {
      await invoke("tirar_imagem_do_perfil", { lado, carimbo });
    }
  }

  // O registro só diz «em dia» quando o servidor mostra o que foi aplicado; sem
  // isso, a assinatura fica em branco e a próxima conferência não confunde a
  // demora do servidor com uma mudança por fora.
  const chegou = await esperarNoPadrao(() => assinaturaDoPerfil(perfilOficial(me)) === esperada);
  await guardarOSeguimento(carimbo, {
    segue: true,
    revisao_aplicada: padrao.revisao,
    assinatura: chegou.ok ? esperada : null,
  });
  return { resultado: "aplicado" };
}

/** Os nomes já mandados ao registro nesta janela (`sessao:nome`): uma vez cada. */
const nomesAlinhados = new Set();

/**
 * **O registro do servidor com o nome do perfil.** O `Hello` de cada conexão
 * leva o apelido desta máquina, e o servidor o grava: um nome trocado dentro
 * do servidor (o `nome_exibido` e o `SetNickname`, juntos) voltaria ao apelido
 * de antes na conexão seguinte. Aqui o registro segue o nome do perfil — uma
 * vez por sessão e nome, e só quando o servidor já disse quem a pessoa é.
 *
 * `{}` quando não há o que fazer (ou o nome não cabe no registro, o que é dito
 * no console), `{ ok }` quando foi, `{ frase }` quando o servidor recusou.
 */
async function alinharONomeNoServidor(carimbo, nome) {
  const me = perfisOficiais.me;
  nome = (nome ?? "").trim();
  if (!nome || !identidadeConhecida(me) || apelidoETagDe(me).apelido.trim() === nome) return {};
  const chave = `${carimbo.sessao}:${nome}`;
  if (nomesAlinhados.has(chave)) return {};
  nomesAlinhados.add(chave);
  const recusa = await conferirONome(nome);
  if (recusa) {
    console.warn(`perfil: o nome do perfil não cabe no registro do servidor, e o apelido ficou: ${recusa}`);
    return {};
  }
  const desfecho = await mandarONomeAoServidor(nome, { carimbo });
  if (!desfecho.ok) console.warn(`perfil: o servidor não alinhou o apelido ao nome do perfil: ${desfecho.frase}`);
  return desfecho;
}

/**
 * **A pergunta do nome semeia o padrão** (`guardarOApelido`, `camada-nomear.js`):
 * o nome com que se entra passa a ser o nome do perfil padrão quando ele ainda
 * não tem um — o nome é um só, e o padrão que nunca foi escrito não fica
 * mudo sobre ele. Uma falha é dita no console, e o apelido continua gravado.
 */
async function semearONomeDoPadrao(nome) {
  const padrao = await lerOPerfilPadrao();
  if (!padrao || (padrao.nome_exibido ?? "").trim() !== "") return;
  const entrada = { ...entradaDoPadrao(padrao), nome_exibido: nome };
  try {
    estadoDoPadrao.atual = await invoke("gravar_perfil_padrao", { entrada });
  } catch (falha) {
    console.warn("perfil padrão: o nome não foi semeado:", falha);
  }
}

/** A falha de aplicar o padrão na sessão da vista, ou `null` (o aviso da coluna lê). */
function falhaDoPadraoNaVista() {
  const sessao = typeof vistaDaJanela === "object" ? vistaDaJanela.sessao : null;
  return sessao === null ? null : estadoDoPadrao.falhas.get(sessao) ?? null;
}

/** Diz a falha: no editor e no aviso da coluna (pelo redesenho), e no registro. */
function dizerAFalhaDoPadrao(carimbo, falha) {
  const frase = falha instanceof FalhaDoPadrao ? falha.frase : fraseDaImagemDoPerfil(falha);
  estadoDoPadrao.falhas.set(carimbo.sessao, frase);
  const detalhe = falha instanceof FalhaDoPadrao ? JSON.stringify(falha.causa ?? null) : JSON.stringify(falha);
  console.warn(`perfil padrão: não foi aplicado neste servidor: ${frase} (${detalhe})`);
  if (typeof registrarNoAnfitriao === "function") {
    registrarNoAnfitriao("perfil-padrao", `o perfil padrão não foi aplicado: ${detalhe}`);
  }
  if (typeof agendarRedesenhoDosPerfis === "function") agendarRedesenhoDosPerfis();
  desenharFaixaDoPadrao();
}

/** A vista trocou no meio: não é falha, e a próxima conferência retoma. */
function foraDaVista(falha) {
  return Boolean(falha?.ForaDaVista || falha?.VistaSuperada);
}

/**
 * **A conferência**, chamada a cada lista `Perfis` nova (`perfis.js`) e
 * quando o padrão muda. Uma por vez; a que chega no meio roda depois.
 * `forcar` é o USAR MEU PERFIL PADRÃO: aplica num servidor que não segue.
 */
async function conferirOPadrao({ forcar = false } = {}) {
  if (estadoDoPadrao.aplicando) {
    estadoDoPadrao.conferirDepois = true;
    return null;
  }
  if (!perfisOficiais.disponivel || perfisOficiais.me === null) return null;
  const carimbo = carimboDoPadrao();
  if (!carimbo) return null;
  estadoDoPadrao.aplicando = true;
  let acao = null;
  try {
    const padrao = estadoDoPadrao.atual ?? (await lerOPerfilPadrao());
    const seguimento = await seguimentoDaVista(carimbo);
    const meu = perfilOficial(perfisOficiais.me);
    let decisao = decidirOPadrao({ disponivel: perfisOficiais.disponivel, meu, padrao, seguimento });
    if (forcar && padrao?.revisao > 0 && seguimento?.servidor) decisao = { acao: meu ? "seguir" : "primeira" };
    acao = decisao.acao;
    // Quem não vai receber o padrão agora, e tem perfil aqui: o registro segue
    // o nome dele (`alinharONomeNoServidor`).
    if (meu && acao !== "primeira" && acao !== "seguir") await alinharONomeNoServidor(carimbo, meu.nome_exibido);
    if (acao === "soltar") {
      await guardarOSeguimento(carimbo, { segue: false, revisao_aplicada: seguimento.revisao_aplicada, assinatura: null });
      console.info("perfil padrão: o perfil deste servidor mudou por outro caminho, e ele deixou de seguir o padrão");
      return decisao;
    }
    if (acao !== "primeira" && acao !== "seguir") return decisao;
    // Uma tentativa automática por sessão e revisão do padrão: a falha é dita
    // uma vez, e a próxima conexão tenta de novo.
    const tentativa = `${carimbo.sessao}:${padrao.revisao}`;
    if (!forcar && estadoDoPadrao.tentados.has(tentativa)) return { acao: "nada", porque: "ja-tentado" };
    estadoDoPadrao.tentados.add(tentativa);
    estadoDoPadrao.falhas.delete(carimbo.sessao);
    desenharFaixaDoPadrao();
    const feito = await aplicarOPadrao(acao, carimbo, { forcar, visto: meu });
    // O USAR MEU PERFIL PADRÃO com o editor aberto: os campos passam a mostrar o
    // padrão. Uma falha ao redesenhar não é falha de aplicar.
    if (feito.resultado === "aplicado" && forcar) {
      anunciar(FRASES_DOS_PERFIS.padrao.aplicado);
      try {
        if (typeof recarregarOEditorDoPerfil === "function") recarregarOEditorDoPerfil();
      } catch (falha) {
        console.warn("perfil padrão: o editor não foi redesenhado depois de aplicar:", falha);
      }
    }
    return { ...decisao, ...feito };
  } catch (falha) {
    if (foraDaVista(falha)) {
      // A tentativa volta a valer quando esta sessão voltar à vista.
      estadoDoPadrao.tentados.clear();
      return null;
    }
    dizerAFalhaDoPadrao(carimbo, falha);
    return { acao, falha };
  } finally {
    estadoDoPadrao.aplicando = false;
    if (typeof agendarRedesenhoDosPerfis === "function") agendarRedesenhoDosPerfis();
    desenharFaixaDoPadrao();
    if (estadoDoPadrao.conferirDepois) {
      estadoDoPadrao.conferirDepois = false;
      setTimeout(() => conferirOPadrao(), 0);
    }
  }
}

/**
 * **O padrão foi salvo na tela inicial**: guardado aqui, e conferido no
 * servidor da vista (os que seguem recebem; os outros, quando voltarem à vista).
 */
function padraoMudou(salvo) {
  if (salvo) estadoDoPadrao.atual = salvo;
  conferirOPadrao().catch((falha) => console.warn("perfil padrão:", falha));
}

/**
 * **Personalizar aqui** (o editor salvou os textos, ou trocou ou tirou uma
 * imagem, dentro do servidor): este servidor deixa de seguir o padrão.
 */
function deixarDeSeguirOPadrao() {
  const carimbo = carimboDoPadrao();
  const seguimento = estadoDoPadrao.seguimento;
  if (!carimbo || !seguimento?.servidor || !seguimento.segue) return;
  guardarOSeguimento(carimbo, { segue: false, revisao_aplicada: seguimento.revisao_aplicada ?? 0, assinatura: null })
    .then(() => desenharFaixaDoPadrao())
    .catch((falha) => console.warn("perfil padrão: não anotei que este servidor tem perfil próprio:", falha));
}

/** A sessão acabou (`esquecerOsPerfis`): o registro da próxima é relido. */
function esquecerOPadraoDaSessao() {
  estadoDoPadrao.seguimento = null;
  estadoDoPadrao.esperando = [];
}

/**
 * **A faixa do padrão no editor.** Na tela inicial: a linha que diz que o que
 * se edita é o padrão. Num servidor 12: «segue o seu perfil padrão», ou a
 * falha de aplicar, e USAR MEU PERFIL PADRÃO quando ele não segue.
 */
function desenharFaixaDoPadrao() {
  const faixa = $("perfil-padrao-faixa");
  if (!faixa) return;
  const linha = $("perfil-padrao-linha");
  const usar = $("perfil-padrao-usar");
  const f = FRASES_DOS_PERFIS.padrao;
  const modo = typeof editorDoPerfil === "object" && editorDoPerfil.aberto ? editorDoPerfil.modo : "nativo";
  let texto = "";
  let tom = "";
  let mostrarUsar = false;
  if (modo === "padrao") {
    texto = f.naInicial;
  } else if (modo === "oficial" && Number(estadoDoPadrao.atual?.revisao) > 0) {
    const falha = falhaDoPadraoNaVista();
    const seguimento = estadoDoPadrao.seguimento;
    if (estadoDoPadrao.aplicando) {
      texto = f.aplicando;
    } else if (falha) {
      texto = f.falhou(falha);
      tom = "erro";
      mostrarUsar = true;
    } else if (seguimento?.segue) {
      texto = f.segue;
    } else if (seguimento?.servidor) {
      mostrarUsar = true;
    }
  }
  linha.textContent = texto;
  linha.hidden = texto === "";
  linha.dataset.tom = tom;
  usar.hidden = !mostrarUsar;
  usar.disabled = estadoDoPadrao.aplicando;
  faixa.hidden = texto === "" && !mostrarUsar;
}

(() => {
  // O padrão é lido uma vez na carga: a conferência da primeira lista já o
  // tem, e a pílula da tela inicial mostra a foto dele.
  lerOPerfilPadrao().then(() => {
    if (typeof desenharPerfilDaEntrada === "function") {
      desenharPerfilDaEntrada().catch((falha) => console.warn("perfil da entrada:", falha));
    }
  });
  $("perfil-padrao-usar")?.addEventListener("click", () => {
    conferirOPadrao({ forcar: true }).catch((falha) => console.warn("perfil padrão:", falha));
  });
})();
