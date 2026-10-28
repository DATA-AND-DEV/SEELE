// A identidade do navegador, o link e o convite (Tarefa 3.4 da F4): o que a
// tela «ENTRAR EM <servidor>?» lê e as ações dela.
//
// # O que a tela vê, e o que ela não vê
//
// O comportamento é daqui, e o desenho é da F8 (a interface aprovada pela
// orquestradora em 09/10). A tela lê um objeto (`ler()`) e chama ações; ela
// não toca em chave, em hash nem no fragmento, e **não vê o token**.
//
// # O segredo da tentativa
//
// O token do convite (e o link inteiro, que o carrega) vive só em `segredos`,
// na memória da tentativa. Ele vai ao `conectar` e a nenhum outro lugar: nem ao
// armazenamento da origem, nem ao console, nem à tela. É esquecido ao entrar,
// no `agoraNao()` e em todo desfecho que encerra a tentativa. O
// `apelido-tomado`, o `mods-a-aceitar` e o `sem-rede` não a encerram: o convite
// não foi gasto, e a volta precisa dele. É o contrato do lote R da F5
// (`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`, em
// `tests/frontend.rs`), e cada linha daqui que o toca está na lista dele.
//
// # O fragmento
//
// Lido e apagado com `history.replaceState` **na carga**, antes de qualquer
// gesto: o link não fica na barra, no histórico nem num favorito. Até a página
// pedir, ele fica em `linkDaCarga`, que o entrega uma vez.
//
// # Quem conecta, e quem cria
//
// A sessão pelo navegador (Tarefa 2.6), pelo `connect` da ponte
// (`conectarPelaPonte`). Quem cria a identidade passa o `conectar(alvo,
// apelido, credencial)`, que resolve como o `connect` do desktop e recusa com o
// `ConnectionError` dele; a bancada passa um servidor de mentira.
//
// **A página tem uma identidade só**, a de `identidadeDaPagina()` (decidido
// pela orquestradora em 10/10). Ela mesma abre o link da carga, e a casca nunca
// toca no token do fragmento: a casca lê o `ler()`, chama as ações e usa o
// `abrir(texto)` no COLAR UM LINK. Depois do `{ ok: true, resposta }`, quem
// desenha a sessão é a tela, pelo `depoisDeConectar` do `tela-boot.js`: a
// identidade não chama a casca, e roda sem ela.
//
// # Uma tentativa por vez
//
// Cada ação que espera (o banco, o `persist()`, a chave, o servidor) guarda a
// `vez` em que começou e confere na volta. Outro link aberto no meio é outra
// tentativa, e a continuação da velha não toca nela: nem na chave, nem no
// token, nem no aceite de MODs, e resolve `sem-tentativa`. A entrada velha que
// o servidor aceitou fica guardada com a chave dela (o servidor já a conhece) e
// é solta pelo `soltar`, como o `largarAEntrada` do desktop (o E54 da F5).
//
// # No desktop, nada
//
// Carregado nas duas cascas, no desktop só declara: o fragmento não é lido, e
// ninguém cria a identidade.

/** O banco desta origem: um servidor por impressão, com a chave dela. */
const BANCO_DA_IDENTIDADE = Object.freeze({ nome: "seele-identidade", versao: 1, servidores: "servidores" });

/** O sim aos MODs de cada servidor, pela impressão (`aceitarOsMods`). */
const CHAVE_DOS_ACEITES_DE_MODS = "seele.aceites-de-mods";

/** Os erros de rede: a tentativa segue, porque o convite não foi gasto. */
const SEM_REDE = Object.freeze(["Unreachable", "UnresolvableHost", "HandshakeTimeout", "SemResposta", "LinkLost"]);

let linkDaCarga = lerEApagarOFragmento();

// Uma navegação só pelo fragmento reutiliza o documento. Apagamos a
// credencial antes de qualquer espera, tal como na carga, e só a identidade
// recebe o link. O evento para as cascas não carrega o fragmento nem conecta.
let vezDoFragmento = 0;
if (PONTE_NO_LUGAR_DO_TAURI) window.addEventListener("hashchange", async () => {
  const link = lerEApagarOFragmento();
  if (!link) return;
  const minha = ++vezDoFragmento;
  linkDaCarga = null;
  try {
    const identidade = await identidadeDaPagina();
    if (minha !== vezDoFragmento) return;
    await identidade.abrir(link);
    if (minha !== vezDoFragmento) return;
    document.dispatchEvent(new Event("seele:link-do-navegador"));
  } catch (falha) {
    console.warn("identidade: não foi possível abrir o link recebido:", falha);
  }
});

/** O pedido de instalação que o navegador oferece (Android), guardado para o `instalar()`. */
let pedidoDeInstalar = null;
if (PONTE_NO_LUGAR_DO_TAURI) {
  window.addEventListener("beforeinstallprompt", (evento) => {
    evento.preventDefault();
    pedidoDeInstalar = evento;
  });
}

function lerEApagarOFragmento() {
  if (!PONTE_NO_LUGAR_DO_TAURI) return null;
  const { hash, href } = window.location;
  if (hash.length <= 1) return null;
  // Convites pessoais têm identidade e aceite próprios. O consumidor da
  // chamada apaga este fragmento antes de esperar a rede; não é um servidor.
  if (hash.startsWith("#chamada/")) return null;
  history.replaceState(history.state, "", href.slice(0, href.length - hash.length));
  return href;
}

/** O link que a página trouxe, entregue uma vez e esquecido aqui. */
function tomarOLinkDaCarga() {
  const link = linkDaCarga;
  linkDaCarga = null;
  return link;
}

/**
 * A entrada pela sessão do navegador: o `connect` da ponte, com o alvo inteiro
 * (o endereço, a impressão, o bilhete e a chave). Até a 2.6, a ponte responde
 * `AindaNaoNoNavegador`, e o `entrar` resolve `recusado` com essa razão.
 */
function conectarPelaPonte(alvo, apelido, credencial, preparacao) {
  return invocarNaPonte("connect", { alvo, nickname: apelido, joinSecret: credencial, preparacao });
}

/**
 * Solta a sessão de uma entrada que outra tentativa substituiu: o
 * `soltar_a_entrada` da ponte, pela geração que o `connect` devolveu, como o
 * `largarAEntrada` do `tela-boot.js`.
 */
function soltarPelaPonte(resposta) {
  return invocarNaPonte("soltar_a_entrada", { geracao: resposta?.geracao });
}

let identidadePrometida = null;

/**
 * A identidade desta página, uma só, com o link da carga já aberto. Uma carga
 * que falhou não fica guardada: o pedido seguinte tenta de novo, e o link da
 * carga só é tomado depois de o `.wasm` chegar.
 */
function identidadeDaPagina() {
  if (!identidadePrometida) {
    identidadePrometida = carregarOWasmDaPagina().then(async ({ instancia }) => {
      const identidade = criarIdentidadeDoNavegador({
        analisar: analisadorDoWasm(instancia.exports),
        conectar: conectarPelaPonte,
      });
      const daCarga = tomarOLinkDaCarga();
      if (daCarga !== null) await identidade.abrir(daCarga);
      return identidade;
    });
    identidadePrometida.catch(() => {
      identidadePrometida = null;
    });
  }
  return identidadePrometida;
}

/** O link lido pelo `.wasm` com as regras da página (`analisar_link`). */
function analisadorDoWasm(w) {
  return (texto) => {
    const bytes = new TextEncoder().encode(texto);
    const p = w.alocar(bytes.length);
    new Uint8Array(w.memory.buffer).set(bytes, p);
    try {
      w.analisar_link(p, bytes.length);
    } finally {
      w.liberar(p, bytes.length);
    }
    return JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.saida_ptr(), w.saida_len()).slice()));
  };
}

// ------------------------------------------------------------- o banco

const PRAZO_DO_BANCO_DA_IDENTIDADE_MS = 3_000;

/** O prazo inclui abrir e concluir a transação. Abortamos a escrita e fechamos
 * também um banco que só entregue onsuccess depois de cancelar. */
function noBanco(modo, fazer, { sinal } = {}) {
  return new Promise((resolve, reject) => {
    let banco = null;
    let transacao = null;
    let terminou = false;
    let relogio;
    const terminar = (falha, valor) => {
      if (terminou) return;
      terminou = true;
      clearTimeout(relogio);
      sinal?.removeEventListener("abort", cancelar);
      if (falha) {
        try { transacao?.abort(); } catch { /* já concluída */ }
      }
      banco?.close();
      if (falha) reject(valor); else resolve(valor);
    };
    const cancelar = () => terminar(true, sinal.reason ?? "Cancelada");
    if (sinal?.aborted) return cancelar();
    sinal?.addEventListener("abort", cancelar, { once: true });
    relogio = setTimeout(() => terminar(true, "ArmazenamentoIndisponivel"), PRAZO_DO_BANCO_DA_IDENTIDADE_MS);
    let abertura;
    try {
      abertura = indexedDB.open(BANCO_DA_IDENTIDADE.nome, BANCO_DA_IDENTIDADE.versao);
    } catch (falha) {
      terminar(true, falha);
      return;
    }
    abertura.onupgradeneeded = () => {
      if (terminou) { abertura.transaction.abort(); return; }
      abertura.result.createObjectStore(BANCO_DA_IDENTIDADE.servidores, { keyPath: "impressao" });
    };
    abertura.onerror = () => terminar(true, abertura.error);
    abertura.onsuccess = () => {
      banco = abertura.result;
      if (terminou) { banco.close(); return; }
      try {
        transacao = banco.transaction(BANCO_DA_IDENTIDADE.servidores, modo);
        const pedido = fazer(transacao.objectStore(BANCO_DA_IDENTIDADE.servidores));
        transacao.oncomplete = () => terminar(false, pedido.result);
        transacao.onerror = () => terminar(true, transacao.error);
        transacao.onabort = () => terminar(true, transacao.error);
      } catch (falha) {
        terminar(true, falha);
      }
    };
  });
}

/** O servidor que esta origem já conhece por esta impressão, com a chave dele. */
const servidorConhecido = (impressao, opcoes) => noBanco("readonly", (servidores) => servidores.get(impressao), opcoes);

const guardarOServidor = (registro, opcoes) => noBanco("readwrite", (servidores) => servidores.put(registro), opcoes);

/** Uma entrada aceita depois de substituída preserva sua chave só se nenhuma
 * outra já foi guardada. A leitura e o put pertencem à mesma transação. */
function guardarOServidorSeAusente(registro) {
  return noBanco("readwrite", (servidores) => {
    const pedido = servidores.get(registro.impressao);
    pedido.onsuccess = () => {
      if (!pedido.result) servidores.put(registro);
    };
    return pedido;
  });
}

/**
 * Os aceites de MODs desta origem, pela impressão: `{conjunto, quando}`, com
 * `quando` em segundos, como o `Aceite` do desktop. Um aceite guardado antes da
 * data (só o conjunto) vale, sem data.
 */
function aceitesDeModsGuardados() {
  let todos = {};
  try {
    todos = JSON.parse(localStorage.getItem(CHAVE_DOS_ACEITES_DE_MODS) ?? "{}") ?? {};
  } catch {
    todos = {};
  }
  return Object.entries(todos)
    .map(([impressao, aceite]) =>
      typeof aceite === "string" ? { impressao, conjunto: aceite, quando: 0 } : { impressao, conjunto: aceite?.conjunto, quando: aceite?.quando ?? 0 },
    )
    .filter((aceite) => typeof aceite.conjunto === "string");
}

/** O conjunto de MODs que esta origem aceitou para o servidor desta impressão. */
function aceiteDeModsGuardado(impressao) {
  return aceitesDeModsGuardados().find((aceite) => aceite.impressao === impressao)?.conjunto ?? null;
}

/** Grava (ou, com `null`, esquece) o aceite de um servidor; diz se gravou. */
function guardarOAceiteDeMods(impressao, conjunto) {
  try {
    const todos = JSON.parse(localStorage.getItem(CHAVE_DOS_ACEITES_DE_MODS) ?? "{}") ?? {};
    if (conjunto === null) delete todos[impressao];
    else todos[impressao] = { conjunto, quando: Math.floor(Date.now() / 1000) };
    localStorage.setItem(CHAVE_DOS_ACEITES_DE_MODS, JSON.stringify(todos));
    return true;
  } catch {
    return false;
  }
}

function relogioDaPagina() {
  return Date.now();
}

function cascaDaPagina() {
  // Pela casca do celular de pé, e não pela marca: o `CELULAR` só existe quando
  // o `celular-casca.js` achou a marca dele (o G2 de `tests/celular.rs`, e a
  // mesma regra do `cascaDoRetrato` em `base.js`).
  return typeof CELULAR === "object" && CELULAR !== null ? "celular" : "larga";
}

function estaInstalado() {
  return window.matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
}

/** Pedido **antes** de a chave nascer: uma chave num armazenamento que o navegador apaga é uma identidade perdida. */
async function pedirPersistencia() {
  if (!navigator.storage || typeof navigator.storage.persist !== "function") return "sem-suporte";
  try {
    return (await navigator.storage.persist()) ? "concedida" : "negada";
  } catch {
    return "negada";
  }
}

/** A chave desta pessoa para um servidor: Ed25519, e a privada nunca sai. */
function nascerAChave() {
  return crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
}

// ---------------------------------------------------------- a identidade

/**
 * A identidade para um link.
 *
 * - `analisar(texto)`: o link lido (`analisadorDoWasm`);
 * - `conectar(alvo, apelido, credencial)`: a entrada, que recusa com o
 *   `ConnectionError` do desktop;
 * - `soltar(resposta)`: solta a entrada que outra tentativa substituiu;
 * - `agora()`, `instalado()` e `casca()`: o relógio e o lugar, trocados na
 *   bancada.
 */
function criarIdentidadeDoNavegador({ analisar, conectar, soltar = soltarPelaPonte, agora = relogioDaPagina, instalado = estaInstalado, casca = cascaDaPagina }) {
  /** O que é segredo nesta tentativa, e só aqui: o token e o link que o carrega. */
  const segredos = { credencial: null, link: null };
  /** A tentativa: o link sem o token, o servidor já conhecido e a chave. */
  let tentativa = null;
  /** Quantas vezes a tentativa mudou: quem espera confere se ainda é a vez dele. */
  let vez = 0;
  let operacaoAtual = null;

  function esquecer() {
    segredos.credencial = null;
    segredos.link = null;
  }

  function encerrar() {
    operacaoAtual?.abort("Cancelada");
    operacaoAtual = null;
    esquecer();
    tentativa = null;
    vez += 1;
  }

  const venceu = (lido) => lido.ate != null && lido.ate * 1000 <= agora();

  function estadoDaTentativa() {
    const { lido, conhecido } = tentativa;
    if (tentativa.bancoIndisponivel) return "armazenamento-indisponivel";
    if (venceu(lido)) return "vencido";
    if (conhecido) return "ja-conhecido";
    if (casca() === "celular" && !instalado()) return "numa-aba";
    return "pronto";
  }

  /** O que a tela lê. Sem nome de servidor: antes de conectar só há o link. */
  function ler() {
    if (!tentativa) return null;
    const lido = tentativa.lido;
    return {
      estado: tentativa.estado,
      ...(tentativa.motivo ? { motivo: tentativa.motivo } : {}),
      endereco: lido?.alvo ?? null,
      impressao: lido?.impressao ?? null,
      versaoDoServidor: lido?.versao ?? null,
      valeAte: lido?.ate ?? null,
      podeInstalar: pedidoDeInstalar !== null,
      persistencia: tentativa.persistencia,
    };
  }

  /** Abre a tentativa de um link: o da carga, ou um colado. Nada conecta aqui. */
  async function abrir(texto) {
    encerrar();
    const minha = vez;
    const resposta = analisar(texto);
    if (resposta.erro) {
      const estado =
        resposta.erro === "OutraOrigem" ? "outra-origem" : resposta.erro === "OutraLinha" ? "outra-linha" : "incompleto";
      // Um link da 0.16 (Servidor 1) ou anterior diz a versão dele, quando a
      // traz: a tela diz «abra a 0.16», e não «link incompleto».
      const lido = estado === "outra-linha" ? { versao: resposta.versao ?? null } : null;
      tentativa = { estado, motivo: estado === "incompleto" ? resposta.erro : null, lido, persistencia: null };
      return ler();
    }
    const { convite, ...lido } = resposta.link;
    let conhecido = null;
    let bancoIndisponivel = false;
    try {
      conhecido = (await servidorConhecido(lido.impressao)) ?? null;
    } catch {
      bancoIndisponivel = true;
    }
    // Outro link aberto enquanto o banco respondia: a tentativa é a dele, e o
    // token deste não vai a ela.
    if (vez !== minha) return ler();
    segredos.credencial = convite ?? null;
    segredos.link = texto;
    tentativa = { lido, conhecido, bancoIndisponivel, chave: null, persistencia: null, apelido: null, motivo: null };
    tentativa.estado = estadoDaTentativa();
    return ler();
  }

  /** Repete só a leitura que falhou. Não cria chave e não abre conexão. */
  async function reler() {
    if (!tentativa?.bancoIndisponivel) return ler();
    const esta = tentativa;
    const minha = vez;
    operacaoAtual?.abort("Cancelada");
    const controle = new AbortController();
    operacaoAtual = controle;
    try {
      const conhecido = await servidorConhecido(esta.lido.impressao, { sinal: controle.signal });
      if (vez !== minha || operacaoAtual !== controle) return ler();
      esta.conhecido = conhecido ?? null;
      esta.bancoIndisponivel = false;
      esta.estado = estadoDaTentativa();
    } catch {
      // Falha ao ler não é ausência de identidade: a tentativa continua bloqueada.
    } finally {
      if (operacaoAtual === controle) operacaoAtual = null;
    }
    return ler();
  }

  /**
   * O bilhete do encontro que vale: o do link, ou o guardado com o servidor.
   * Na 0.17 o `fp` é a âncora inteira (o `a=fingerprint` que a página escreve,
   * contrato da 0.17, D3), e não há hashes nem janelas a escolher.
   */
  function bilheteQueVale() {
    const { lido, conhecido } = tentativa;
    return lido.bilhete ?? conhecido?.bilhete ?? null;
  }

  /** O `ConnectionError` do desktop no vocabulário da tela. */
  function desfecho(erro) {
    if (erro && erro.ModsNaoAceitos) {
      return { erro: "mods-a-aceitar", mods: erro.ModsNaoAceitos.mods ?? [], conjunto: erro.ModsNaoAceitos.conjunto ?? "", segue: true };
    }
    // A falha do enlace WebRTC (`rtc-web.js`, D8): a conexão segura que não
    // se verificou encerra a tentativa (o servidor pode não ser o do link, e
    // insistir não muda isso); o resto é rede, e o convite não foi gasto. A
    // frase de cada motivo está no bloco do transporte da `frases.js`.
    if (erro && erro.FalhaDoEnlace) {
      const { motivo } = erro.FalhaDoEnlace;
      if (motivo === "VerificacaoDaConexaoSegura") return { erro: "verificacao-falhou", falha: motivo };
      return { erro: "sem-rede", falha: motivo, segue: true };
    }
    if (erro === "PreparacaoExpirada") return { erro: "preparacao-expirada", segue: true };
    if (erro === "ArmazenamentoIndisponivel") return { erro: "armazenamento-indisponivel", segue: true };
    const razao = erro && erro.Refused ? erro.Refused.reason : erro;
    if (razao === "NicknameTaken") return { erro: "apelido-tomado", segue: true };
    // **A portaria segura a entrada** (ADR 0030): a batida ficou na fila de quem
    // hospeda, e o convite não foi gasto. A tentativa segue (a espera bate de
    // novo pela mesma chave), e quem hospeda ainda pode recusar.
    if (razao === "AdmissionPending") return { erro: "espera-de-admissao", segue: true, guardar: true };
    if (razao === "AdmissionDenied") return { erro: "admissao-recusada" };
    if (SEM_REDE.includes(razao)) return { erro: "sem-rede", segue: true };
    if (razao === "CredentialRejected") {
      // O servidor responde igual a todo convite que não vale (spec 08): o
      // que se sabe daqui é a validade do link.
      return { erro: venceu(tentativa.lido) ? "convite-vencido" : "convite-gasto" };
    }
    return { erro: "recusado", razao: typeof razao === "string" ? razao : JSON.stringify(razao) };
  }

  /** Entra no servidor do link com `apelido`. A aba móvel é permitida, com aviso na UI. */
  async function entrar(apelido) {
    if (!tentativa || !tentativa.lido) return { erro: "sem-tentativa" };
    if (tentativa.bancoIndisponivel) return { erro: "armazenamento-indisponivel" };
    if (venceu(tentativa.lido)) {
      encerrar();
      return { erro: "convite-vencido" };
    }
    if (!["pronto", "ja-conhecido", "numa-aba"].includes(tentativa.estado)) {
      return { erro: "recusado", razao: tentativa.estado };
    }
    operacaoAtual?.abort("Cancelada");
    const controle = new AbortController();
    operacaoAtual = controle;
    const minha = vez;
    const substituida = () => vez !== minha || operacaoAtual !== controle;
    const esta = tentativa;
    const { lido } = esta;
    const preparo = { sinal: controle.signal, ate: Date.now() + PRAZO_DA_PREPARACAO_DA_ENTRADA_MS };
    let resposta = null;
    let soltou = false;
    const soltarEsta = () => {
      if (!resposta || soltou) return;
      soltou = true;
      soltarASubstituida(lido, resposta);
    };
    controle.signal.addEventListener("abort", soltarEsta);
    const guardar = (registro) => esperarEtapaDaEntrada(
      () => guardarOServidor(registro, { sinal: controle.signal }),
      { sinal: controle.signal, ate: Date.now() + PRAZO_DO_BANCO_DA_IDENTIDADE_MS, motivo: "ArmazenamentoIndisponivel" },
    );
    try {
      esta.apelido = apelido;
      try {
        if (esta.persistencia === null) {
          const persistencia = await esperarEtapaDaEntrada(() => pedirPersistencia(), preparo);
          if (substituida()) return { erro: "sem-tentativa" };
          esta.persistencia = persistencia;
        }
        if (!esta.chave) {
          const chave = esta.conhecido ? esta.conhecido.chave : await esperarEtapaDaEntrada(() => nascerAChave(), preparo);
          if (substituida()) return { erro: "sem-tentativa" };
          esta.chave = chave;
        }
      } catch (falha) {
        if (substituida()) return { erro: "sem-tentativa" };
        if (falha === "PreparacaoExpirada") return { erro: "preparacao-expirada" };
        encerrar();
        return { erro: "recusado", razao: `a chave não nasceu: ${falha && falha.message ? falha.message : falha}` };
      }
      const alvo = {
        endereco: lido.alvo,
        alternativos: lido.alternativos ?? [],
        impressao: lido.impressao,
        bilhete: bilheteQueVale(),
        chave: esta.chave,
        conhecido: Boolean(esta.conhecido),
      };
      const credencial = esta.conhecido ? null : segredos.credencial;
      const registro = {
        impressao: lido.impressao,
        endereco: lido.alvo,
        alternativos: lido.alternativos ?? [],
        bilhete: alvo.bilhete,
        versao: lido.versao ?? null,
        chave: esta.chave,
        nome: esta.conhecido?.nome ?? null,
        icone: esta.conhecido?.icone ?? null,
        apelido: esta.apelido || esta.conhecido?.apelido || null,
        visto: agora(),
      };
      try {
        // O retorno tardio pertence à operação antiga: fecha mesmo se o
        // cancelamento já fez a espera da tela terminar.
        const ligando = Promise.resolve(conectar(alvo, apelido, credencial, preparo)).then((entrada) => {
          resposta = entrada;
          if (controle.signal.aborted || substituida()) {
            soltarEsta();
            guardarOServidorSeAusente(registro).catch((falha) => console.warn("identidade: a chave da entrada tardia não ficou guardada:", falha));
          }
          return entrada;
        });
        // O cancelamento pode vencer antes de a espera começar a observar.
        ligando.catch(() => {});
        await esperarEtapaDaEntrada(() => ligando, { sinal: controle.signal });
      } catch (erro) {
        if (substituida()) return { erro: "sem-tentativa" };
        const { segue, guardar: deveGuardar, ...resultado } = desfecho(erro);
        if (deveGuardar) {
          try {
            await guardar({ ...registro, naPortaria: true });
          } catch {
            if (substituida()) return { erro: "sem-tentativa" };
            return { erro: "armazenamento-indisponivel" };
          }
          if (substituida()) return { erro: "sem-tentativa" };
        }
        if (!segue) encerrar();
        else if (resultado.erro === "mods-a-aceitar") tentativa.aguardaMods = true;
        return { ...resultado, motivo: erro };
      }
      // A sessão já existe: cancelar a espera do banco a solta imediatamente.
      try {
        await guardar(registro);
      } catch {
        soltarEsta();
        if (substituida()) return { erro: "sem-tentativa" };
        // O servidor já reconheceu esta chave. A repetição usa a mesma chave
        // em memória e não reapresenta o convite que ele pode ter consumido.
        esta.conhecido = registro;
        esta.estado = "ja-conhecido";
        return { erro: "armazenamento-indisponivel" };
      }
      if (substituida()) {
        soltarEsta();
        return { erro: "sem-tentativa" };
      }
      const { persistencia } = esta;
      controle.signal.removeEventListener("abort", soltarEsta);
      operacaoAtual = null;
      encerrar();
      return { ok: true, resposta, persistencia };
    } finally {
      // Aborta também uma transação cujo resultado chegou tarde. Uma operação
      // substituída nunca cancela nem apaga a tentativa que tomou seu lugar.
      controle.abort("Cancelada");
      controle.signal.removeEventListener("abort", soltarEsta);
      if (operacaoAtual === controle) operacaoAtual = null;
    }
  }

  /** A entrada que o servidor aceitou depois de outro link abrir: dita e solta. */
  function soltarASubstituida(lido, resposta) {
    console.warn(`identidade: a entrada em ${lido.alvo} foi encerrada antes de concluir a entrada, e a sessão dela (geração ${resposta?.geracao}) é solta`);
    Promise.resolve()
      .then(() => soltar(resposta))
      .catch((falha) => console.warn("identidade, soltar a entrada substituída:", falha));
  }

  /**
   * Se um servidor guardado ainda se alcança pela página: na 0.17 sempre, pelo
   * `fp` (a âncora não vence). Um registro da 0.16 (com `hashes` e sem
   * `versao` da linha 2) é de um Servidor 1, que a página da 0.17 não alcança.
   */
  const daLinhaDesta = (registro) =>
    !Array.isArray(registro.hashes) || (typeof registro.versao === "string" && !registro.versao.startsWith("1."));

  /** Os SERVIDORES DESTE APARELHO, do mais recente ao mais antigo. */
  async function conhecidos() {
    let registros = [];
    try {
      registros = await noBanco("readonly", (servidores) => servidores.getAll());
    } catch {
      registros = [];
    }
    return registros
      .map((registro) => ({
        impressao: registro.impressao,
        endereco: registro.endereco,
        nome: registro.nome ?? null,
        icone: registro.icone ?? null,
        apelido: registro.apelido ?? null,
        versaoDoServidor: registro.versao ?? null,
        visto: registro.visto ?? null,
        podeEntrar: daLinhaDesta(registro),
      }))
      .sort((a, b) => (b.visto ?? 0) - (a.visto ?? 0));
  }

  /**
   * Entra num servidor deste aparelho sem link: pela chave guardada, e sem
   * convite. Resolve como o `entrar`, e com `outra-linha` quando o servidor
   * guardado é de um Servidor 1 (a 0.16): a tela diz «abra a 0.16».
   */
  async function entrarNoConhecido(impressao, apelido) {
    const minha = vez;
    let registro = null;
    try {
      registro = (await servidorConhecido(impressao)) ?? null;
    } catch {
      if (vez !== minha) return { erro: "sem-tentativa" };
      return { erro: "armazenamento-indisponivel" };
    }
    // Outro link aberto enquanto o banco respondia: a vez é dele.
    if (vez !== minha) return { erro: "sem-tentativa" };
    if (!registro) return { erro: "recusado", razao: "servidor-desconhecido" };
    if (!daLinhaDesta(registro)) return { erro: "outra-linha", versaoDoServidor: registro.versao ?? null };
    encerrar();
    tentativa = {
      lido: {
        alvo: registro.endereco,
        alternativos: registro.alternativos ?? [],
        impressao: registro.impressao,
        bilhete: registro.bilhete ?? null,
        versao: registro.versao ?? null,
      },
      conhecido: registro,
      chave: null,
      persistencia: null,
      apelido: null,
      motivo: null,
      estado: "ja-conhecido",
    };
    // **O apelido nunca vai vazio ao servidor**: ele aplica o apelido pedido à
    // conta que volta (`registrar_ou_achar`), e vazio apagaria o nome de quem foi
    // admitido. O digitado vale; sem ele, o que este servidor guardou. Sem nenhum
    // dos dois a tentativa fica aberta, e a tela pede o nome no campo da «ENTRAR EM».
    const nome = (apelido ?? "").trim() || (registro.apelido ?? "").trim();
    if (!nome) return { erro: "pede-apelido" };
    return entrar(nome);
  }

  /** O nome do servidor, que chega no primeiro retrato depois de entrar. */
  async function lembrarONome(impressao, nome) {
    return lembrarAAparencia(impressao, { nome });
  }

  /**
   * O nome e o ícone do servidor, como o `lembrar_aparencia_do_servidor` do
   * desktop os anota nos conhecidos: só num servidor que este aparelho já
   * guarda. O ícone são os bytes do PNG que o servidor mandou.
   */
  async function lembrarAAparencia(impressao, aparencia) {
    let registro = null;
    try {
      registro = (await servidorConhecido(impressao)) ?? null;
    } catch {
      registro = null;
    }
    if (!registro) return false;
    await guardarOServidor({ ...registro, ...aparencia });
    return true;
  }

  /**
   * Esquece um servidor deste aparelho, e com ele a chave que esta origem tinha
   * para ele: voltar pede um link novo, e o servidor vê uma chave nova.
   */
  async function esquecerOServidor(impressao) {
    if (tentativa && tentativa.lido && tentativa.lido.impressao === impressao) encerrar();
    try {
      await noBanco("readwrite", (servidores) => servidores.delete(impressao));
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Os hashes dos certificados curtos da 0.16 (`HashesDoNavegador`) não servem
   * à 0.17: o `fp` é a âncora inteira. Fica para a ponte que ainda chama, e não
   * guarda nada.
   */
  async function lembrarOsHashes() {
    return false;
  }

  /**
   * Grava o sim aos MODs pela impressão do servidor e entra de novo, **com o
   * mesmo token**: o servidor recusou antes de gastar o convite (`d0b98b9c`, da
   * F1). Resolve como o `entrar`.
   */
  async function aceitarOsMods(conjunto) {
    if (!tentativa || !tentativa.aguardaMods) return { erro: "sem-tentativa" };
    if (!guardarOAceiteDeMods(tentativa.lido.impressao, conjunto)) return { erro: "recusado", razao: "NaoGravei" };
    tentativa.aguardaMods = false;
    return entrar(tentativa.apelido);
  }

  /** Descarta o link e encerra a tentativa. */
  function agoraNao() {
    encerrar();
  }

  /** No iOS o link não atravessa a instalação: a pessoa o leva copiado. */
  async function copiarLink() {
    if (!segredos.link) return false;
    await navigator.clipboard.writeText(segredos.link);
    return true;
  }

  /** O pedido de instalação do navegador, quando ele oferece um. */
  async function instalar() {
    const pedido = pedidoDeInstalar;
    if (!pedido) return false;
    pedidoDeInstalar = null;
    await pedido.prompt();
    const { outcome } = await pedido.userChoice;
    return outcome === "accepted";
  }

  return {
    ler,
    abrir,
    reler,
    entrar,
    aceitarOsMods,
    agoraNao,
    copiarLink,
    instalar,
    conhecidos,
    entrarNoConhecido,
    lembrarONome,
    lembrarAAparencia,
    lembrarOsHashes,
    esquecerOServidor,
  };
}

// Identidade pessoal da origem, separada das chaves por servidor da versão 1.
// CryptoKey é clonada pelo IndexedDB; a chave privada continua não exportável.
// Nomes nunca indexam contatos: códigos e chaves completas governam a agenda.
const BANCO_PESSOAL_WEB = "seele-pessoal";
let identidadePessoalPrometida = null;
function noBancoPessoalWeb(loja, modo, fazer) {
  return new Promise((resolve, reject) => {
    let banco, transacao, terminou = false;
    const concluir = (erro, valor) => {
      if (terminou) return;
      terminou = true; clearTimeout(prazo);
      if (erro) { try { transacao?.abort(); } catch {} }
      banco?.close(); erro ? reject(valor) : resolve(valor);
    };
    const prazo = setTimeout(() => concluir(true, "ArmazenamentoIndisponivel"), 3000);
    let pedido;
    try { pedido = indexedDB.open(BANCO_PESSOAL_WEB, 1); }
    catch (erro) { concluir(true, erro); return; }
    pedido.onupgradeneeded = () => {
      if (terminou) { pedido.transaction.abort(); return; }
      pedido.result.createObjectStore("identidade", { keyPath: "id" });
      pedido.result.createObjectStore("contatos", { keyPath: "codigo" });
    };
    pedido.onerror = () => concluir(true, pedido.error);
    pedido.onsuccess = () => {
      banco = pedido.result;
      if (terminou) { banco.close(); return; }
      banco.onversionchange = () => banco.close();
      try {
        transacao = banco.transaction(loja, modo);
        const resultado = fazer(transacao.objectStore(loja));
        transacao.oncomplete = () => concluir(false, resultado.result);
        transacao.onerror = () => concluir(true, transacao.error);
        transacao.onabort = () => concluir(true, transacao.error || "NaoGravou");
      } catch (erro) { concluir(true, erro); }
    };
  });
}
async function identidadePessoalDoNavegador() {
  if (!identidadePessoalPrometida) {
    identidadePessoalPrometida = (async () => {
      let registro = await noBancoPessoalWeb("identidade", "readonly", loja => loja.get("pessoal"));
      if (!registro) {
        const chave = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
        const publica = Array.from(new Uint8Array(await crypto.subtle.exportKey("raw", chave.publicKey)));
        const candidato = { id: "pessoal", chave, publica, criadoEm: Date.now() };
        // A leitura e a decisão de inserir são atômicas, inclusive entre abas.
        await noBancoPessoalWeb("identidade", "readwrite", loja => {
          const pedido = loja.get("pessoal");
          pedido.onsuccess = () => { if (!pedido.result) loja.put(candidato); };
          return pedido;
        });
        registro = await noBancoPessoalWeb("identidade", "readonly", loja => loja.get("pessoal"));
      }
      if (!registro?.chave?.privateKey || registro.chave.privateKey.extractable
        || registro.chave.privateKey.algorithm.name !== "Ed25519") throw new Error("IdentidadePessoalInvalida");
      return registro;
    })();
    identidadePessoalPrometida.catch(() => { identidadePessoalPrometida = null; });
  }
  return identidadePessoalPrometida;
}
let wasmDoCodigoPessoal = null;
async function operarCodigoPessoalWeb(intencao) {
  if (!wasmDoCodigoPessoal) wasmDoCodigoPessoal = instanciarOWasmDaSessao().catch(erro => { wasmDoCodigoPessoal = null; throw erro; });
  const wasm = await wasmDoCodigoPessoal;
  const bytes = new TextEncoder().encode(JSON.stringify(intencao));
  const p = wasm.alocar(bytes.length);
  try {
    new Uint8Array(wasm.memory.buffer).set(bytes, p); wasm.chamada_operar(p, bytes.length);
    const valor = JSON.parse(new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, wasm.saida_ptr(), wasm.saida_len()).slice()));
    if (valor.erro) throw valor.erro;
    return valor.codigo;
  } finally { wasm.liberar(p, bytes.length); }
}
async function meuCodigoPessoalWeb() {
  return operarCodigoPessoalWeb({ acao: "codigo_da_chave", chave: (await identidadePessoalDoNavegador()).publica });
}
async function codigoPessoalWebLido(codigo) {
  return operarCodigoPessoalWeb({ acao: "codigo_ler", codigo: String(codigo || "").replace(/^@/, "") });
}
async function alterarContatoPessoalWeb(codigo, mudar) {
  codigo = await codigoPessoalWebLido(codigo);
  if (codigo === await meuCodigoPessoalWeb()) throw "SeuCodigo";
  return noBancoPessoalWeb("contatos", "readwrite", loja => {
    const pedido = loja.get(codigo);
    pedido.onsuccess = () => {
      try {
        const contato = pedido.result || { codigo, nome: null, apelido: null, visto_em: null,
          direcao: null, bloqueado: false, amigo: false, alvo: `@${codigo}` };
        loja.put(mudar(contato));
      } catch (erro) { pedido.transaction?.abort(); throw erro; }
    };
    return pedido;
  });
}
async function contatosPessoaisWeb(soAmigos = false) {
  const contatos = await noBancoPessoalWeb("contatos", "readonly", loja => loja.getAll());
  // O pino de chave não atravessa a projeção da agenda.
  return contatos.filter(c => !soAmigos || c.amigo).map(({ chavePublica, ...contato }) => contato);
}
async function conferirParPessoalWeb(codigo, publica, apelido = null, direcao = null) {
  codigo = await codigoPessoalWebLido(codigo);
  const texto = publica.map(b => b.toString(16).padStart(2, "0")).join("");
  if (await operarCodigoPessoalWeb({ acao: "codigo_da_chave", chave: publica }) !== codigo) throw new Error("CodigoNaoConfere");
  let falha = null;
  await alterarContatoPessoalWeb(codigo, contato => {
    if (contato.bloqueado) { falha = "Bloqueado"; return contato; }
    if (contato.chavePublica && contato.chavePublica !== texto) { falha = "ChaveMudou"; return contato; }
    return { ...contato, chavePublica: texto, apelido: apelido || contato.apelido,
      visto_em: Math.floor(Date.now() / 1000), direcao: direcao || contato.direcao };
  });
  if (falha) throw new Error(falha);
  return codigo;
}

async function preferenciaDeReceberPessoalWeb(receber) {
  const identidade = await identidadePessoalDoNavegador();
  if (receber === undefined) return identidade.receber !== false;
  await noBancoPessoalWeb("identidade", "readwrite", loja => {
    const pedido = loja.get("pessoal");
    pedido.onsuccess = () => loja.put({ ...pedido.result, receber: Boolean(receber) });
    return pedido;
  });
  identidade.receber = Boolean(receber);
  return identidade.receber;
}
