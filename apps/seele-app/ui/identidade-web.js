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
// # Quem conecta
//
// A sessão pelo navegador (Tarefa 2.6). Até ela existir, quem cria a
// identidade passa o `conectar(alvo, apelido, credencial)`, que resolve como o
// `connect` do desktop e recusa com o `ConnectionError` dele. A bancada passa
// um servidor de mentira.
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
  history.replaceState(history.state, "", href.slice(0, href.length - hash.length));
  return href;
}

/** O link que a página trouxe, entregue uma vez e esquecido aqui. */
function tomarOLinkDaCarga() {
  const link = linkDaCarga;
  linkDaCarga = null;
  return link;
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

function abrirOBanco() {
  return new Promise((resolve, reject) => {
    const pedido = indexedDB.open(BANCO_DA_IDENTIDADE.nome, BANCO_DA_IDENTIDADE.versao);
    pedido.onupgradeneeded = () =>
      pedido.result.createObjectStore(BANCO_DA_IDENTIDADE.servidores, { keyPath: "impressao" });
    pedido.onsuccess = () => resolve(pedido.result);
    pedido.onerror = () => reject(pedido.error);
  });
}

async function noBanco(modo, fazer) {
  const banco = await abrirOBanco();
  try {
    return await new Promise((resolve, reject) => {
      const transacao = banco.transaction(BANCO_DA_IDENTIDADE.servidores, modo);
      const pedido = fazer(transacao.objectStore(BANCO_DA_IDENTIDADE.servidores));
      transacao.oncomplete = () => resolve(pedido.result);
      transacao.onerror = () => reject(transacao.error);
      transacao.onabort = () => reject(transacao.error);
    });
  } finally {
    banco.close();
  }
}

/** O servidor que esta origem já conhece por esta impressão, com a chave dele. */
const servidorConhecido = (impressao) => noBanco("readonly", (servidores) => servidores.get(impressao));

const guardarOServidor = (registro) => noBanco("readwrite", (servidores) => servidores.put(registro));

/** O conjunto de MODs que esta origem aceitou para o servidor desta impressão. */
function aceiteDeModsGuardado(impressao) {
  try {
    return JSON.parse(localStorage.getItem(CHAVE_DOS_ACEITES_DE_MODS) ?? "{}")[impressao] ?? null;
  } catch {
    return null;
  }
}

function guardarOAceiteDeMods(impressao, conjunto) {
  try {
    const todos = JSON.parse(localStorage.getItem(CHAVE_DOS_ACEITES_DE_MODS) ?? "{}");
    todos[impressao] = conjunto;
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
 * - `agora()`, `instalado()` e `casca()`: o relógio e o lugar, trocados na
 *   bancada.
 */
function criarIdentidadeDoNavegador({ analisar, conectar, agora = relogioDaPagina, instalado = estaInstalado, casca = cascaDaPagina }) {
  /** O que é segredo nesta tentativa, e só aqui: o token e o link que o carrega. */
  const segredos = { credencial: null, link: null };
  /** A tentativa: o link sem o token, o servidor já conhecido e a chave. */
  let tentativa = null;

  function esquecer() {
    segredos.credencial = null;
    segredos.link = null;
  }

  function encerrar() {
    esquecer();
    tentativa = null;
  }

  const venceu = (lido) => lido.ate != null && lido.ate * 1000 <= agora();

  function estadoDaTentativa() {
    const { lido, conhecido } = tentativa;
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
      endereco: lido ? lido.alvo : null,
      impressao: lido ? lido.impressao : null,
      versaoDoServidor: lido ? lido.versao ?? null : null,
      valeAte: lido ? lido.ate ?? null : null,
      podeInstalar: pedidoDeInstalar !== null,
      persistencia: tentativa.persistencia,
    };
  }

  /** Abre a tentativa de um link: o da carga, ou um colado. Nada conecta aqui. */
  async function abrir(texto) {
    encerrar();
    const resposta = analisar(texto);
    if (resposta.erro) {
      const estado =
        resposta.erro === "OutraOrigem" ? "outra-origem" : resposta.erro === "SemHashesDoNavegador" ? "servidor-0.15" : "incompleto";
      tentativa = { estado, motivo: estado === "incompleto" ? resposta.erro : null, lido: null, persistencia: null };
      return ler();
    }
    const { convite, ...lido } = resposta.link;
    segredos.credencial = convite ?? null;
    segredos.link = texto;
    let conhecido = null;
    try {
      conhecido = (await servidorConhecido(lido.impressao)) ?? null;
    } catch {
      conhecido = null;
    }
    tentativa = { lido, conhecido, chave: null, persistencia: null, apelido: null, motivo: null };
    tentativa.estado = estadoDaTentativa();
    return ler();
  }

  /** Os hashes que valem: os guardados e ainda válidos têm precedência sobre os do link. */
  function hashesQueValem() {
    const { lido, conhecido } = tentativa;
    if (conhecido && conhecido.ate != null && conhecido.ate * 1000 > agora()) return conhecido.hashes;
    return lido.hashes;
  }

  /** O `ConnectionError` do desktop no vocabulário da tela. */
  function desfecho(erro) {
    if (erro && erro.ModsNaoAceitos) {
      return { erro: "mods-a-aceitar", mods: erro.ModsNaoAceitos.mods ?? [], conjunto: erro.ModsNaoAceitos.conjunto ?? "", segue: true };
    }
    const razao = erro && erro.Refused ? erro.Refused.reason : erro;
    if (razao === "NicknameTaken") return { erro: "apelido-tomado", segue: true };
    if (SEM_REDE.includes(razao)) return { erro: "sem-rede", segue: true };
    if (razao === "CredentialRejected") {
      // O servidor responde igual a todo convite que não vale (spec 08): o
      // que se sabe daqui é a validade do link.
      return { erro: venceu(tentativa.lido) ? "convite-vencido" : "convite-gasto" };
    }
    return { erro: "recusado", razao: typeof razao === "string" ? razao : JSON.stringify(razao) };
  }

  /** Entra no servidor do link com `apelido`. Só no `pronto` e no `ja-conhecido`. */
  async function entrar(apelido) {
    if (!tentativa || !tentativa.lido) return { erro: "sem-tentativa" };
    if (venceu(tentativa.lido)) {
      encerrar();
      return { erro: "convite-vencido" };
    }
    if (tentativa.estado !== "pronto" && tentativa.estado !== "ja-conhecido") {
      return { erro: "recusado", razao: tentativa.estado };
    }
    tentativa.apelido = apelido;
    if (tentativa.persistencia === null) tentativa.persistencia = await pedirPersistencia();
    if (!tentativa.chave) {
      try {
        tentativa.chave = tentativa.conhecido ? tentativa.conhecido.chave : await nascerAChave();
      } catch (falha) {
        encerrar();
        return { erro: "recusado", razao: `a chave não nasceu: ${falha && falha.message ? falha.message : falha}` };
      }
    }
    const { lido } = tentativa;
    const alvo = { endereco: lido.alvo, alternativos: lido.alternativos ?? [], impressao: lido.impressao, hashes: hashesQueValem(), chave: tentativa.chave };
    // Quem já é conhecido entra pela chave, e o convite não é gasto.
    const credencial = tentativa.conhecido ? null : segredos.credencial;
    try {
      await conectar(alvo, apelido, credencial);
    } catch (erro) {
      const { segue, ...resultado } = desfecho(erro);
      if (!segue) encerrar();
      else if (resultado.erro === "mods-a-aceitar") tentativa.aguardaMods = true;
      return resultado;
    }
    if (!tentativa.conhecido) {
      await guardarOServidor({
        impressao: lido.impressao,
        endereco: lido.alvo,
        alternativos: lido.alternativos ?? [],
        hashes: lido.hashes,
        ate: lido.ate ?? null,
        versao: lido.versao ?? null,
        chave: tentativa.chave,
      });
    }
    encerrar();
    return { ok: true };
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

  return { ler, abrir, entrar, aceitarOsMods, agoraNao, copiarLink, instalar };
}
