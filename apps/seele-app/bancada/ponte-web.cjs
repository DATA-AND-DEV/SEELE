// A ponte do navegador (`ui/ponte-web.js`), contra o código de verdade.
//
// ADR 0057 e D-web-3. Ela carrega a ponte num contexto de VM com uma janela de
// mentira (o armazenamento, o ponteiro, a largura e o console), e confere o que
// um guarda de texto não alcança: **o que ela responde**. `tests/web.rs` segura
// as listas contra a casca e contra o `main.rs`; esta bancada segura que cada
// lista faz o que diz.
//
//     node apps/seele-app/bancada/ponte-web.cjs
//
// Roda em Node puro, pela lista de `cargo xtask check-runtime`. Sai com 1 no
// primeiro caso que falhar, dizendo qual.

const fs = require("node:fs");
const path = require("node:path");
const util = require("node:util");
const vm = require("node:vm");

const raiz = path.resolve(__dirname, "..");
const fonte = fs.readFileSync(path.join(raiz, "ui/ponte-web.js"), "utf8");

let terminou = false;
process.on("beforeExit", () => {
  if (!terminou) {
    console.error("ponte-web: não chegou ao fim — uma promessa ficou pendurada");
    process.exitCode = 1;
  }
});

const falhas = [];
function confere(caso, condicao, detalhe) {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
}
// Pelo JSON dos dois lados: um objeto feito dentro da VM tem o `Object.prototype`
// de outro reino, e a comparação estrita o acusaria por isso.
const emJson = (valor) => (valor === undefined ? undefined : JSON.parse(JSON.stringify(valor)));
const igual = (a, b) => util.isDeepStrictEqual(emJson(a), emJson(b));

/**
 * Uma página de mentira com a ponte carregada.
 *
 * `guardado` é o que o armazenamento tem em `seele.casca` (`undefined` para
 * nada, `Error` para um armazenamento que lança). `tauri` põe o
 * `__TAURI_INTERNALS__`; `duble` põe um `__TAURI__` antes da ponte.
 */
function pagina({
  guardado,
  toque = false,
  largura = 1280,
  tauri = false,
  duble = null,
  hash = "",
  gravacaoFalha = false,
  seletor = () => null,
  aparelhos = [],
  temSetSinkId = true,
  telaCheiaFalha = false,
  permissao = "prompt",
} = {}) {
  const telaCheia = { pedidos: 0, saidas: 0, elemento: null };
  const ouvidos = [];
  // Quem a ponte pôs a ouvir no documento (o arraste), à parte dos da janela.
  const doDocumento = new Map();
  // O seletor de arquivos: cada `<input type=file>` que a ponte criou, se ele
  // foi clicado **dentro** da chamada (sem `await` antes) e se ainda está na
  // página.
  const seletores = [];
  let dentroDaChamada = false;
  const avisos = [];
  const linhas = [];
  const abertos = [];
  // O resto da origem, que o lote B grava. O `seele.casca` continua vindo de
  // `guardado`.
  const origem = new Map();
  const janela = {
    structuredClone,
    TextEncoder,
    Blob,
    File,
    atob,
    innerWidth: largura,
    location: { hash, href: `https://seele.invalid/${hash}` },
    navigator: {
      mediaDevices: { enumerateDevices: async () => aparelhos.map((a) => ({ ...a })) },
      permissions:
        permissao === null
          ? undefined
          : {
              query: async ({ name }) => {
                if (name !== "microphone") throw new TypeError(name);
                if (permissao instanceof Error) throw permissao;
                return { state: permissao };
              },
            },
    },
    HTMLMediaElement: temSetSinkId ? class { setSinkId() {} } : class {},
    document: {
      get fullscreenElement() {
        return telaCheia.elemento;
      },
      exitFullscreen: async () => {
        telaCheia.saidas += 1;
        telaCheia.elemento = null;
      },
      documentElement: {
        dataset: {},
        requestFullscreen: async () => {
          telaCheia.pedidos += 1;
          if (telaCheiaFalha) throw new Error("sem gesto");
          telaCheia.elemento = janela.document.documentElement;
        },
      },
      body: {
        append: (no) => {
          no.naPagina = true;
        },
      },
      addEventListener: (evento, ouvinte) => doDocumento.set(evento, ouvinte),
      createElement(tag) {
        const ouvintes = new Map();
        const no = {
          tag,
          files: null,
          naPagina: false,
          addEventListener: (evento, ouvinte) => ouvintes.set(evento, ouvinte),
          remove() {
            no.naPagina = false;
          },
          click() {
            seletores.push({ no, dentroDaChamada });
            // A pessoa escolhe (ou desiste) depois, como no navegador.
            setImmediate(() => {
              const escolhido = seletor();
              if (escolhido) {
                no.files = [escolhido];
                ouvintes.get("change")?.({});
              } else {
                ouvintes.get("cancel")?.({});
              }
            });
          },
        };
        return no;
      },
    },
    localStorage: {
      getItem(chave) {
        if (guardado instanceof Error) throw guardado;
        if (chave === "seele.casca") return guardado !== undefined ? guardado : null;
        return origem.has(chave) ? origem.get(chave) : null;
      },
      setItem(chave, valor) {
        if (gravacaoFalha) throw new Error("QuotaExceededError");
        origem.set(chave, String(valor));
      },
      removeItem(chave) {
        if (gravacaoFalha) throw new Error("QuotaExceededError");
        origem.delete(chave);
      },
    },
    open: (...argumentos) => {
      abertos.push(argumentos);
      return null;
    },
    matchMedia(consulta) {
      return {
        matches: consulta === "(pointer: coarse)" && toque,
        addEventListener: (evento) => ouvidos.push(`matchMedia:${evento}`),
        addListener: () => ouvidos.push("matchMedia:addListener"),
      };
    },
    addEventListener: (evento) => ouvidos.push(evento),
    console: {
      warn: (...partes) => avisos.push(partes),
      debug: (...partes) => linhas.push(["debug", ...partes]),
      info: (...partes) => linhas.push(["info", ...partes]),
      error: (...partes) => linhas.push(["error", ...partes]),
      log: (...partes) => linhas.push(["log", ...partes]),
    },
  };
  janela.window = janela;
  if (tauri) janela.__TAURI_INTERNALS__ = {};
  if (duble) janela.__TAURI__ = duble;
  const contexto = vm.createContext(janela);
  vm.runInContext(fonte, contexto, { filename: "ponte-web.js" });
  const ler = (expressao) => vm.runInContext(expressao, contexto);
  return {
    janela,
    ler,
    ouvidos,
    avisos,
    linhas,
    abertos,
    origem,
    doDocumento,
    seletores,
    telaCheia,
    invoke(comando, args) {
      dentroDaChamada = true;
      try {
        return janela.__TAURI__.core.invoke(comando, args);
      } finally {
        dentroDaChamada = false;
      }
    },
  };
}

/** O desfecho de um `invoke`: `{ ok }` ou `{ erro }`. */
async function desfecho(promessa) {
  try {
    return { ok: await promessa };
  } catch (erro) {
    return { erro };
  }
}

// ---------------------------------------------------------------------------

/// **O lugar.** No Tauri e sob um duble de bancada, a ponte não faz nada: nem
/// marca, nem `__TAURI__`. Só no navegador ela se põe no lugar.
async function oLugar() {
  const caso = "o lugar";
  const tauri = pagina({ tauri: true });
  confere(caso, tauri.ler("PONTE_NO_LUGAR_DO_TAURI") === false, "no Tauri a ponte se pôs no lugar");
  confere(caso, tauri.janela.document.documentElement.dataset.casca === undefined, "no Tauri a ponte pôs a marca");
  confere(caso, tauri.janela.__TAURI__ === undefined, "no Tauri a ponte escreveu o `__TAURI__`");

  const duble = { core: { invoke: async () => "do duble" }, event: { listen: async () => () => {} } };
  const comDuble = pagina({ duble, toque: true });
  confere(caso, comDuble.ler("PONTE_NO_LUGAR_DO_TAURI") === false, "sob um duble a ponte se pôs no lugar");
  confere(caso, comDuble.janela.__TAURI__ === duble, "a ponte trocou o duble da bancada");
  confere(caso, comDuble.janela.document.documentElement.dataset.casca === undefined,
    "sob um duble a ponte pôs a marca, que é de quem pôs o duble");

  const navegador = pagina();
  confere(caso, navegador.ler("PONTE_NO_LUGAR_DO_TAURI") === true, "no navegador a ponte não se pôs no lugar");
  confere(caso, typeof navegador.janela.__TAURI__?.core?.invoke === "function", "no navegador não há `invoke`");
  confere(caso, typeof navegador.janela.__TAURI__?.event?.listen === "function", "no navegador não há `listen`");
  const plataforma = await desfecho(navegador.invoke("plataforma"));
  confere(caso, plataforma.ok === "web", `o \`plataforma\` respondeu ${util.inspect(plataforma)}, e não "web"`);
}

/// **A marca**, as quatro regras do §9.2 do spec da F8: a escolha guardada
/// primeiro; sem ela, toque ou menos de 900 px; lida uma vez.
async function aMarca() {
  const caso = "a marca";
  const cenarios = [
    [{ guardado: "larga", toque: true, largura: 390 }, "larga", "a escolha guardada vale contra o toque"],
    [{ guardado: "celular", largura: 1440 }, "celular", "a escolha guardada vale contra a largura"],
    [{ guardado: "outra", largura: 1440 }, "larga", "um valor guardado estranho cai no critério"],
    [{ guardado: new Error("SecurityError"), toque: true, largura: 1440 }, "celular",
      "sem armazenamento, o toque decide"],
    [{ guardado: new Error("SecurityError"), largura: 1440 }, "larga", "sem armazenamento e sem toque, a largura"],
    [{ largura: 899 }, "celular", "abaixo de 900 px é o celular"],
    [{ largura: 900 }, "larga", "900 px é a casca larga"],
    [{ toque: true, largura: 1280 }, "celular", "o toque é o celular, mesmo largo"],
  ];
  for (const [opcoes, esperada, porque] of cenarios) {
    const p = pagina(opcoes);
    const marca = p.janela.document.documentElement.dataset.casca;
    confere(caso, marca === esperada, `${porque}: saiu ${marca}, e não ${esperada}`);
    confere(caso, p.ler("CASCA_DA_PONTE") === marca, "a casca da ponte e a marca discordam");
  }
  // Lida uma vez: a ponte não ouve nada que a faria trocar.
  const p = pagina({ largura: 1280 });
  p.janela.innerWidth = 400;
  confere(caso, p.janela.document.documentElement.dataset.casca === "larga", "a marca mudou depois da carga");
  confere(caso, p.ouvidos.length === 0, `a ponte ouve ${p.ouvidos.join(", ")}, e a casca troca no meio da sessão`);
}

/// **Cada atendido tem corpo ou está a fazer, e nunca os dois.** Um atendido
/// sem nenhum dos dois voltaria `ComandoDesconhecido`; um recusado com corpo é
/// um corpo que nunca roda.
async function asListasEOsCorpos() {
  const caso = "as listas e os corpos";
  const p = pagina();
  const nomes = (lista) => [...p.ler(lista).keys()];
  const atendidos = [
    ...p.ler("COMANDOS_DA_PONTE"),
    ...nomes("SO_NA_CASCA_LARGA"),
  ];
  const corpos = Object.keys(p.ler("ATENDIMENTOS_DA_PONTE"));
  const aFazer = new Set(nomes("AINDA_A_FAZER"));
  for (const comando of atendidos) {
    const temCorpo = corpos.includes(comando);
    confere(caso, temCorpo !== aFazer.has(comando),
      temCorpo ? `\`${comando}\` tem corpo e está a fazer` : `\`${comando}\` não tem corpo nem está a fazer`);
  }
  for (const comando of corpos) {
    confere(caso, atendidos.includes(comando), `\`${comando}\` tem corpo e não é atendido em lista nenhuma`);
  }
  for (const [comando, tarefa] of p.ler("AINDA_A_FAZER")) {
    confere(caso, /^\d+\.\d+/.test(tarefa), `\`${comando}\` está a fazer sem a tarefa do plano: ${tarefa}`);
  }
  // As constantes têm corpo, e o corpo as devolve como estão.
  for (const [comando, valor] of Object.entries(p.ler("CONSTANTES_DO_DESKTOP"))) {
    const r = await desfecho(p.invoke(comando));
    confere(caso, igual(r.ok, valor), `\`${comando}\` respondeu ${util.inspect(r)}, e não a constante`);
  }
}

/// **As recusas, por casca.** No celular, os do PC voltam recusados com a
/// frase do celular; no PC, não. Os recusados voltam recusados nas duas, com a
/// frase deles. E a primeira recusa de cada comando é dita no console.
async function asRecusas() {
  const caso = "as recusas";
  for (const [casca, opcoes] of [["celular", { toque: true }], ["larga", { largura: 1280 }]]) {
    const p = pagina(opcoes);
    for (const [comando, frase] of p.ler("RECUSAS_NOMEADAS")) {
      const r = await desfecho(p.invoke(comando));
      confere(caso, igual(r.erro, { RecusaDaPonte: { comando, frase } }),
        `${casca}: \`${comando}\` voltou ${util.inspect(r)}, e não a recusa com a frase`);
    }
    for (const [comando, frase] of p.ler("SO_NA_CASCA_LARGA")) {
      const r = await desfecho(p.invoke(comando));
      if (casca === "celular") {
        confere(caso, igual(r.erro, { RecusaDaPonte: { comando, frase } }),
          `celular: \`${comando}\` voltou ${util.inspect(r)}, e não a recusa do celular`);
      } else {
        confere(caso, !r.erro?.RecusaDaPonte, `larga: \`${comando}\`, que o PC atende, voltou recusado`);
      }
    }
    const recusados = p.ler("RECUSAS_NOMEADAS").size + (casca === "celular" ? p.ler("SO_NA_CASCA_LARGA").size : 0);
    const ditos = p.avisos.filter(([prefixo, valor]) => String(prefixo).startsWith("ponte:") && valor?.RecusaDaPonte);
    confere(caso, ditos.length === recusados, `${casca}: ${ditos.length} recusas ditas no console, e não ${recusados}`);
    // A segunda vez conta e não repete o aviso.
    const [comando] = p.ler("RECUSAS_NOMEADAS").keys();
    const avisosAntes = p.avisos.length;
    await desfecho(p.invoke(comando));
    confere(caso, p.ler("CONTAS_DA_PONTE").get(comando)?.recusas === 2, "a segunda recusa não foi contada");
    confere(caso, p.avisos.length === avisosAntes, "a segunda recusa do mesmo comando foi dita de novo");
  }
  const p = pagina();
  const [[comando, tarefa]] = p.ler("AINDA_A_FAZER");
  const r = await desfecho(p.invoke(comando));
  confere(caso, igual(r.erro, { AindaNaoNoNavegador: { comando, tarefa } }),
    `\`${comando}\`, a fazer, voltou ${util.inspect(r)}`);
  const d = await desfecho(p.invoke("comando_que_nao_existe"));
  confere(caso, igual(d.erro, { ComandoDesconhecido: { comando: "comando_que_nao_existe" } }),
    `um comando desconhecido voltou ${util.inspect(d)}`);
}

/// **Os neutros respondem o que a tabela diz** (`F4/ponte-no-pc.md`), e sempre
/// uma cópia. Trocar a resposta de um deles por um atendimento de verdade
/// reprova aqui.
async function osNeutros() {
  const caso = "os neutros";
  const tabela = {
    estado_da_porta: {
      hospedando: false,
      aberto: false,
      tem_senha: false,
      aceita_convites: false,
      portaria_ligada: false,
      pendentes: 0,
      alcance: "SoRedeLocal",
    },
    estou_hospedando: false,
    conjunto_exigido_agora: "",
    estado_do_servidor: { hospedando: false, mensagens_desconhecidas: 0, sessoes_com_desconhecidas: 0 },
    // A sala viva da F7 (décima segunda integração): o navegador não tem sala.
    estado_da_sala: null,
    toques_agora: [],
    // As tarefas 19 e 20 da F7: o navegador não hospeda.
    meu_servidor: null,
  };
  for (const casca of [{ toque: true }, { largura: 1280 }]) {
    const p = pagina(casca);
    confere(caso, igual([...p.ler("NEUTROS_DE_QUEM_NAO_HOSPEDA").keys()].sort(), Object.keys(tabela).sort()),
      "os neutros da ponte não são os da tabela");
    for (const [comando, esperada] of Object.entries(tabela)) {
      const r = await desfecho(p.invoke(comando));
      confere(caso, igual(r.ok, esperada), `\`${comando}\` respondeu ${util.inspect(r)}`);
      if (r.ok && typeof r.ok === "object") r.ok.hospedando = true;
      const de_novo = await desfecho(p.invoke(comando));
      confere(caso, igual(de_novo.ok, esperada), `mexer na resposta de \`${comando}\` mudou a seguinte`);
    }
  }
}

/// **Sair da sessão sai.** O `disconnect` é um atendido comum desde a tarefa 19
/// da F7 (no desktop ele não derruba mais a hospedagem), e tem de encerrar a
/// sessão da ponte.
async function sairSai() {
  const caso = "sair sai";
  for (const comando of ["disconnect"]) {
    const p = pagina();
    let saiu = 0;
    p.ler("sessaoDaPonte").atual = { encerrar: async () => { saiu += 1; } };
    const r = await desfecho(p.invoke(comando));
    confere(caso, r.ok === null, `\`${comando}\` voltou ${util.inspect(r)}`);
    confere(caso, saiu === 1, `\`${comando}\` não encerrou a sessão`);
    confere(caso, p.ler("sessaoDaPonte").atual === null, `\`${comando}\` deixou a sessão de pé`);
  }
}

/// **Sem sessão, o que o desktop responde sem conexão.**
async function semSessao() {
  const caso = "sem sessão";
  const p = pagina();
  const retrato = await desfecho(p.invoke("snapshot"));
  confere(caso, retrato.erro === "NotConnected", `o \`snapshot\` voltou ${util.inspect(retrato)}`);
  const volumes = await desfecho(p.invoke("volumes_locais"));
  confere(caso, igual(volumes.ok, {}), `o \`volumes_locais\` voltou ${util.inspect(volumes)}`);
  const apelido = await desfecho(p.invoke("apelido_local"));
  confere(caso, apelido.ok === null, `o \`apelido_local\` sem nada guardado voltou ${util.inspect(apelido)}`);
}

/// **A abertura nunca entra** (ADR 0057): com um link no fragmento, o
/// `abertura` não traz `entrar`, ou `cumprirAAbertura` conectaria sem toque.
async function aAberturaNuncaEntra() {
  const caso = "a abertura";
  for (const hash of ["", "#seele://servidor.invalid:4433?fp=abc", "#https://seele.invalid/entrar#fp=abc"]) {
    const p = pagina({ hash });
    const r = await desfecho(p.invoke("abertura"));
    confere(caso, r.ok?.entrar === null && r.ok?.hospedar === false,
      `com o fragmento «${hash}», o \`abertura\` voltou ${util.inspect(r)}`);
  }
}

/// **O `listen` devolve o desligar, como o do Tauri**, e o registro da janela
/// vai ao console com o nível dele.
async function oListenEORegistro() {
  const caso = "o listen e o registro";
  const p = pagina();
  const desligar = await p.janela.__TAURI__.event.listen("seele://event", () => {});
  confere(caso, p.ler("OUVINTES_DA_PONTE").get("seele://event")?.size === 1, "o ouvinte não ficou registrado");
  desligar();
  confere(caso, p.ler("OUVINTES_DA_PONTE").get("seele://event")?.size === 0, "o desligar não desligou");
  await p.invoke("registrar_da_janela", { nivel: "warn", onde: "base.js", oQue: "algo", modId: "autor/mod" });
  const linha = p.avisos.find(([texto]) => String(texto).includes("[base.js] MOD autor/mod: algo"));
  confere(caso, linha !== undefined, `o registro da janela não chegou ao console: ${util.inspect(p.avisos)}`);
}

/// **O apelido de entrada mora na origem**, com as regras do desktop
/// (`escolher_apelido_local`, `Preferences::set_nickname`): aparado, sem as
/// quebras que o formato do arquivo não aceita, e vazio apaga. Não conseguir
/// gravar volta `NaoGravei`, e não um silêncio.
async function oApelidoLocal() {
  const caso = "o apelido local";
  const p = pagina();
  for (const [escrito, guardado] of [
    ["  Ana  ", "Ana"],
    ["Bea\tTriz\n", "BeaTriz"],
    ["", null],
    ["   ", null],
  ]) {
    const r = await desfecho(p.invoke("escolher_apelido_local", { apelido: escrito }));
    confere(caso, r.ok === null, `escolher «${escrito}» voltou ${util.inspect(r)}`);
    const lido = await desfecho(p.invoke("apelido_local"));
    confere(caso, lido.ok === guardado, `depois de «${escrito}», o apelido lido é ${util.inspect(lido)}, e não ${guardado}`);
  }
  const sem = pagina({ gravacaoFalha: true });
  const r = await desfecho(sem.invoke("escolher_apelido_local", { apelido: "Ana" }));
  confere(caso, r.erro === "NaoGravei", `sem gravar, o apelido voltou ${util.inspect(r)}`);
}

/// **As prévias consentidas** (ADR 0053), com as regras do desktop
/// (`Preferences::consentir_previa`): o domínio em minúsculas, uma palavra só,
/// e um subdomínio de um liberado já está liberado.
async function asPreviasConsentidas() {
  const caso = "as prévias consentidas";
  const p = pagina();
  const lista = async () => (await desfecho(p.invoke("previas_consentidas"))).ok;
  confere(caso, igual(await lista(), []), "sem nada consentido, a lista não é vazia");
  await p.invoke("consentir_previa", { dominio: "Exemplo.COM" });
  await p.invoke("consentir_previa", { dominio: "fotos.exemplo.com" });
  await p.invoke("consentir_previa", { dominio: "dois dominios" });
  await p.invoke("consentir_previa", { dominio: "" });
  await p.invoke("consentir_previa", { dominio: "outro.net" });
  confere(caso, igual(await lista(), ["exemplo.com", "outro.net"]),
    `a lista saiu ${util.inspect(await lista())}`);
  await p.invoke("esquecer_previa", { dominio: "EXEMPLO.com" });
  confere(caso, igual(await lista(), ["outro.net"]), `esquecer deixou ${util.inspect(await lista())}`);
  const sem = pagina({ gravacaoFalha: true });
  const r = await desfecho(sem.invoke("consentir_previa", { dominio: "exemplo.com" }));
  confere(caso, typeof r.erro === "string" && r.erro.includes("não consegui gravar"),
    `sem gravar, consentir voltou ${util.inspect(r)}`);
}

/// **Abrir um endereço é abrir uma aba nova, e só `http` e `https`**, com a
/// conferência do desktop (`endereco_que_pode_sair`). A aba nova não leva a
/// página junto (`noopener`, `noreferrer`).
async function abrirNoNavegador() {
  const caso = "abrir no navegador";
  const p = pagina();
  const r = await desfecho(p.invoke("abrir_no_navegador", { url: "  https://exemplo.com/a?b=c  " }));
  confere(caso, r.ok === null, `um endereço bom voltou ${util.inspect(r)}`);
  confere(caso, igual(p.abertos, [["https://exemplo.com/a?b=c", "_blank", "noopener,noreferrer"]]),
    `a aba aberta foi ${util.inspect(p.abertos)}`);
  for (const ruim of [
    "javascript:alert(1)",
    "data:text/html,oi",
    "file:///etc/passwd",
    "seele://x",
    "https://exemplo.com/a b",
    "https://exemplo.com/\u0007",
    "http://",
    `https://exemplo.com/${"a".repeat(2048)}`,
    "",
  ]) {
    const q = pagina();
    const s = await desfecho(q.invoke("abrir_no_navegador", { url: ruim }));
    confere(caso, typeof s.erro === "string" && q.abertos.length === 0,
      `«${ruim.slice(0, 40)}» voltou ${util.inspect(s)} e abriu ${q.abertos.length}`);
  }
}

/// **O arquivo por alça** (`F4/ponte-no-pc.md`, as quatro condições):
/// 1. o mapa tem teto e solta o que guarda (no teto e no fim da sessão);
/// 2. uma alça desconhecida, ou já solta, é recusada;
/// 3. o seletor abre sem `await` antes do `click()`, dentro do gesto;
/// 4. desistir devolve `null`, e não deixa a promessa pendurada.
async function oArquivoPorAlca() {
  const caso = "o arquivo por alça";
  let proximo = null;
  const p = pagina({ seletor: () => proximo });

  proximo = new File(["abc"], "Foto.JPG");
  const escolhido = await desfecho(p.invoke("escolher_arquivo"));
  confere(caso, p.seletores.at(-1)?.dentroDaChamada === true,
    "o seletor não abriu dentro da chamada: um `await` antes do `click()` gasta o gesto");
  confere(caso, p.seletores.at(-1)?.no.naPagina === false, "o `<input>` do seletor ficou na página");
  const alca = escolhido.ok?.caminho;
  confere(caso, typeof alca === "string" && alca.startsWith("web-arquivo:"), `a alça saiu ${util.inspect(escolhido)}`);
  confere(caso, igual(escolhido.ok, { caminho: alca, nome: "Foto.JPG", tamanho: 3, tipo: "image/jpeg" }),
    `o escolhido saiu ${util.inspect(escolhido)}; o tipo é o da extensão, e não o do navegador`);

  proximo = null;
  const desistiu = await desfecho(p.invoke("escolher_arquivo"));
  confere(caso, desistiu.ok === null, `desistir do seletor voltou ${util.inspect(desistiu)}`);

  const descrito = await desfecho(p.invoke("descrever_arquivo", { caminho: alca }));
  confere(caso, igual(descrito.ok, escolhido.ok), `descrever a alça voltou ${util.inspect(descrito)}`);
  for (const estranha of ["web-arquivo:999", "/Users/alguem/segredo.txt", ""]) {
    const r = await desfecho(p.invoke("descrever_arquivo", { caminho: estranha }));
    confere(caso, r.erro === "NotConnected", `a alça «${estranha}» voltou ${util.inspect(r)}`);
  }

  // O teto: as mais velhas saem, e a mais nova fica.
  const teto = p.ler("TETO_DE_ALCAS");
  const alcas = [];
  for (let i = 0; i < teto + 2; i++) {
    proximo = new File([`${i}`], `a${i}.txt`);
    alcas.push((await p.invoke("escolher_arquivo")).caminho);
  }
  confere(caso, p.ler("ALCAS_DA_PONTE").size === teto, `o mapa guarda ${p.ler("ALCAS_DA_PONTE").size}, e o teto é ${teto}`);
  const velha = await desfecho(p.invoke("descrever_arquivo", { caminho: alcas[0] }));
  confere(caso, velha.erro === "NotConnected", "a alça mais velha não saiu no teto");
  const nova = await desfecho(p.invoke("descrever_arquivo", { caminho: alcas.at(-1) }));
  confere(caso, nova.ok?.nome === `a${teto + 1}.txt`, `a alça mais nova saiu ${util.inspect(nova)}`);

  // O fim da sessão solta tudo.
  await p.invoke("disconnect");
  confere(caso, p.ler("ALCAS_DA_PONTE").size === 0, "sair da sessão não soltou as alças");
  const depois = await desfecho(p.invoke("descrever_arquivo", { caminho: alcas.at(-1) }));
  confere(caso, depois.erro === "NotConnected", "uma alça solta ainda descreve");
}

/// **A imagem colada** vira uma alça com as regras do `guardar_colado` do
/// desktop: o teto, a recusa do que não é base64 ou está vazio, e o nome com a
/// extensão do tipo.
async function aImagemColada() {
  const caso = "a imagem colada";
  const p = pagina();
  const r = await desfecho(p.invoke("guardar_colado", { tipo: "image/png", base64: Buffer.from("png!").toString("base64") }));
  confere(caso, /^colado-\d+\.png$/.test(r.ok?.nome ?? "") && r.ok?.tamanho === 4 && r.ok?.tipo === "image/png",
    `colar voltou ${util.inspect(r)}`);
  const sem = await desfecho(p.invoke("guardar_colado", { tipo: "image/x", base64: Buffer.from("x").toString("base64") }));
  confere(caso, /^colado-\d+$/.test(sem.ok?.nome ?? "") && sem.ok?.tipo === "application/octet-stream",
    `um tipo sem extensão voltou ${util.inspect(sem)}`);
  for (const [base64, falha] of [["", "NaoEImagem"], ["não é base64!", "NaoEImagem"]]) {
    const q = await desfecho(p.invoke("guardar_colado", { tipo: "image/png", base64 }));
    confere(caso, q.erro === falha, `«${base64}» voltou ${util.inspect(q)}, e não ${falha}`);
  }
  // Das coladas, que estão todas na memória, só ficam as mais novas.
  for (let i = 0; i < 4; i++) await p.invoke("guardar_colado", { tipo: "image/png", base64: "AAAA" });
  const coladas = [...p.ler("ALCAS_DA_PONTE").values()].filter((guardado) => guardado.colado).length;
  confere(caso, coladas === p.ler("TETO_DE_COLADOS"), `ficaram ${coladas} imagens coladas na memória`);
  const teto = p.ler("REGRAS_DO_ARQUIVO").maior_colagem_bytes;
  const grande = await desfecho(p.invoke("guardar_colado", { tipo: "image/png", base64: "A".repeat(Math.ceil(teto / 3) * 4 + 4) }));
  confere(caso, grande.erro === "GrandeDemais", `uma colagem acima do teto voltou ${util.inspect(grande)}`);
}

/// **O arraste, só no PC**: a ponte ouve o `drop` do documento, guarda cada
/// arquivo sob uma alça e emite o mesmo `tauri://drag-drop` que a casca já
/// ouve. No celular não há arraste, e ela não ouve nada.
async function oArraste() {
  const caso = "o arraste";
  const celular = pagina({ toque: true });
  confere(caso, celular.doDocumento.size === 0, `no celular a ponte ouve ${[...celular.doDocumento.keys()]}`);

  const p = pagina({ largura: 1280 });
  const recebidos = [];
  await p.janela.__TAURI__.event.listen("tauri://drag-drop", (evento) => recebidos.push(evento.payload));
  let permitiu = false;
  p.doDocumento.get("dragover")?.({ dataTransfer: { types: ["Files"] }, preventDefault: () => { permitiu = true; } });
  confere(caso, permitiu, "o `dragover` com arquivos não foi permitido, e o navegador não deixaria soltar");
  let soltou = false;
  p.doDocumento.get("drop")?.({
    dataTransfer: { types: ["Files"], files: [new File(["1"], "um.pdf"), new File(["22"], "dois.txt")] },
    preventDefault: () => { soltou = true; },
  });
  confere(caso, soltou, "o `drop` não foi segurado, e o navegador abriria o arquivo no lugar da página");
  confere(caso, recebidos.length === 1 && recebidos[0].paths?.length === 2, `o evento saiu ${util.inspect(recebidos)}`);
  const descrito = await desfecho(p.invoke("descrever_arquivo", { caminho: recebidos[0]?.paths?.[0] }));
  confere(caso, descrito.ok?.nome === "um.pdf" && descrito.ok?.tipo === "application/pdf",
    `o arquivo arrastado descreveu ${util.inspect(descrito)}`);
  let textoSegurado = false;
  p.doDocumento.get("dragover")?.({ dataTransfer: { types: ["text/plain"] }, preventDefault: () => { textoSegurado = true; } });
  confere(caso, !textoSegurado, "arrastar texto foi segurado pela ponte, e deixaria de soltar num campo");
}

/// **A saída de som do PC**: a lista do `enumerateDevices`, sem os
/// pseudo-aparelhos (`default`, `communications`) e sem os ids vazios que o
/// navegador dá antes de o microfone ser permitido; o padrão marcado pelo
/// grupo do `default`. Onde não há `setSinkId`, não há o que escolher, e a
/// lista é vazia. A escolha mora na origem, como no desktop.
async function aSaidaDeSom() {
  const caso = "a saída de som";
  const aparelhos = [
    { kind: "audiooutput", deviceId: "default", groupId: "a", label: "Padrão - Alto-falantes" },
    { kind: "audiooutput", deviceId: "communications", groupId: "b", label: "Comunicações - Fone" },
    { kind: "audiooutput", deviceId: "falante", groupId: "a", label: "Alto-falantes" },
    { kind: "audiooutput", deviceId: "fone", groupId: "b", label: "Fone" },
    { kind: "audioinput", deviceId: "mic", groupId: "b", label: "Microfone" },
  ];
  const p = pagina({ aparelhos });
  const lista = await desfecho(p.invoke("saidas"));
  confere(caso, igual(lista.ok, [
    { id: "falante", name: "Alto-falantes", default: true },
    { id: "fone", name: "Fone", default: false },
  ]), `a lista saiu ${util.inspect(lista)}`);
  const semSink = await desfecho(pagina({ aparelhos, temSetSinkId: false }).invoke("saidas"));
  confere(caso, igual(semSink.ok, []), `sem setSinkId a lista saiu ${util.inspect(semSink)}`);
  const semPermissao = await desfecho(pagina({ aparelhos: [{ kind: "audiooutput", deviceId: "", groupId: "", label: "" }] }).invoke("saidas"));
  confere(caso, igual(semPermissao.ok, []), `sem permissão a lista saiu ${util.inspect(semPermissao)}`);

  confere(caso, (await desfecho(p.invoke("saida_escolhida"))).ok === null, "sem escolha, a saída escolhida não é nula");
  await p.invoke("escolher_saida", { dispositivo: "fone" });
  confere(caso, (await desfecho(p.invoke("saida_escolhida"))).ok === "fone", "a escolha não ficou");
  await p.invoke("escolher_saida", { dispositivo: null });
  confere(caso, (await desfecho(p.invoke("saida_escolhida"))).ok === null, "voltar ao padrão não apagou a escolha");
  const r = await desfecho(pagina({ gravacaoFalha: true }).invoke("escolher_saida", { dispositivo: "fone" }));
  confere(caso, r.erro === "NaoGravei", `sem gravar, escolher voltou ${util.inspect(r)}`);
}

/// **A tela cheia do PC é a do navegador** (a Fullscreen API), no lugar da
/// janela. Pedida duas vezes não pede de novo; sair só sai se estiver nela; e
/// um pedido recusado (sem gesto) é dito, e não lança na casca, como no
/// desktop.
async function aTelaCheia() {
  const caso = "a tela cheia";
  const p = pagina();
  await p.invoke("tela_cheia", { ligada: true });
  await p.invoke("tela_cheia", { ligada: true });
  confere(caso, p.telaCheia.pedidos === 1, `pedir duas vezes pediu ${p.telaCheia.pedidos}`);
  await p.invoke("tela_cheia", { ligada: false });
  await p.invoke("tela_cheia", { ligada: false });
  confere(caso, p.telaCheia.saidas === 1, `sair duas vezes saiu ${p.telaCheia.saidas}`);
  const q = pagina({ telaCheiaFalha: true });
  const r = await desfecho(q.invoke("tela_cheia", { ligada: true }));
  confere(caso, r.ok === null, `um pedido recusado voltou ${util.inspect(r)}`);
  confere(caso, q.avisos.some(([texto]) => String(texto).includes("tela cheia")), "o pedido recusado não foi dito");
}

/// **As preferências da voz moram na origem**, com as regras do desktop
/// (`set_voice_mode`, `escolher_microfone`, `ajustar_sensibilidade_da_voz`,
/// `ajustar_reducao_de_ruido`) e os controles derivados delas pela regra do
/// `controles_da_voz` sem sessão. Sem escolha, o modo é `VoiceActivated` (a
/// matriz da F8). A tecla de falar é só do PC.
async function asPreferenciasDaVoz() {
  const caso = "as preferências da voz";
  const p = pagina({ largura: 1280 });
  const ler = async (comando) => (await desfecho(p.invoke(comando))).ok;
  confere(caso, (await ler("modo_de_voz_escolhido")) === "VoiceActivated", "sem escolha, o modo não é VoiceActivated");
  await p.invoke("set_voice_mode", { mode: "PushToTalk" });
  confere(caso, (await ler("modo_de_voz_escolhido")) === "PushToTalk", "o modo escolhido não ficou");
  const estranho = await desfecho(p.invoke("set_voice_mode", { mode: "Grito" }));
  confere(caso, typeof estranho.erro === "string" && (await ler("modo_de_voz_escolhido")) === "PushToTalk",
    `um modo desconhecido voltou ${util.inspect(estranho)} ou trocou o guardado`);

  confere(caso, (await ler("microfone_escolhido")) === null, "sem escolha, o microfone escolhido não é nulo");
  await p.invoke("escolher_microfone", { dispositivo: "mic-usb" });
  confere(caso, (await ler("microfone_escolhido")) === "mic-usb", "o microfone escolhido não ficou");
  await p.invoke("escolher_microfone", { dispositivo: null });
  confere(caso, (await ler("microfone_escolhido")) === null, "voltar ao padrão não apagou o microfone");

  const padrao = await ler("controles_da_voz");
  await p.invoke("ajustar_sensibilidade_da_voz", { dbfs: -50 });
  const sensivel = await ler("controles_da_voz");
  confere(caso, sensivel.abertura_dbfs === -50 && sensivel.abertura_escolhida === true,
    `a sensibilidade escolhida não chegou aos controles: ${util.inspect(sensivel)}`);
  await p.invoke("ajustar_sensibilidade_da_voz", { dbfs: null });
  confere(caso, igual(await ler("controles_da_voz"), padrao), "voltar ao padrão não devolveu os controles de antes");
  await p.invoke("ajustar_reducao_de_ruido", { forca: 0 });
  const semReducao = await ler("controles_da_voz");
  confere(caso, semReducao.supressao === 0 && semReducao.abertura_dbfs === p.ler("REGRAS_DA_VOZ").abertura_sem_supressao_dbfs,
    `sem redução de ruído, a abertura padrão não é a sem supressão: ${util.inspect(semReducao)}`);
  await p.invoke("ajustar_reducao_de_ruido", { forca: 7 });
  confere(caso, (await ler("controles_da_voz")).supressao === 1, "a força da redução não foi fixada entre 0 e 1");

  confere(caso, (await ler("tecla_de_falar")) === null, "sem escolha, a tecla de falar não é nula");
  await p.invoke("escolher_tecla_de_falar", { tecla: "KeyT" });
  confere(caso, (await ler("tecla_de_falar")) === "KeyT", "a tecla escolhida não ficou");
  // Uma tecla guardada (de uma visita no PC, no mesmo aparelho) não vale no
  // celular: ele não tem tecla de falar.
  const celular = pagina({ toque: true });
  celular.origem.set("seele.tecla-de-falar", "KeyT");
  confere(caso, (await desfecho(celular.invoke("tecla_de_falar"))).ok === null, "no celular a tecla de falar não é nula");

  const sem = pagina({ gravacaoFalha: true });
  for (const [comando, args] of [["set_voice_mode", { mode: "Open" }], ["escolher_microfone", { dispositivo: "x" }]]) {
    const r = await desfecho(sem.invoke(comando, args));
    confere(caso, r.erro === "NaoGravei", `sem gravar, ${comando} voltou ${util.inspect(r)}`);
  }
}

/// **Os microfones do aparelho**, na forma do `CaptureDevice` do desktop: o
/// `enumerateDevices` sem os pseudo-aparelhos e sem os ids vazios de antes da
/// permissão, com o padrão marcado pelo grupo do `default`.
async function osMicrofones() {
  const caso = "os microfones";
  const aparelhos = [
    { kind: "audioinput", deviceId: "default", groupId: "a", label: "Padrão - Interno" },
    { kind: "audioinput", deviceId: "interno", groupId: "a", label: "Microfone interno" },
    { kind: "audioinput", deviceId: "usb", groupId: "b", label: "" },
    { kind: "audiooutput", deviceId: "falante", groupId: "a", label: "Alto-falantes" },
  ];
  const r = await desfecho(pagina({ aparelhos }).invoke("microfones"));
  confere(caso, igual(r.ok, [
    { id: "interno", name: "Microfone interno", default: true },
    { id: "usb", name: "MICROFONE 2", default: false },
  ]), `a lista saiu ${util.inspect(r)}`);
  const semPermissao = await desfecho(pagina({ aparelhos: [{ kind: "audioinput", deviceId: "", groupId: "", label: "" }] }).invoke("microfones"));
  confere(caso, igual(semPermissao.ok, []), `sem permissão a lista saiu ${util.inspect(semPermissao)}`);
}

/// **A permissão do microfone, pela Permissions API**: concedida é
/// `Permitida`, negada é `NegadaPeloNavegador` (o achado 3 da matriz da F8), e o
/// resto, inclusive a API que não pergunta pelo microfone, é `NaoSeSabe`.
async function aPermissaoDoMicrofone() {
  const caso = "a permissão do microfone";
  for (const [estado, esperada] of [
    ["granted", "Permitida"],
    ["denied", "NegadaPeloNavegador"],
    ["prompt", "NaoSeSabe"],
    [new TypeError("microphone"), "NaoSeSabe"],
    [null, "NaoSeSabe"],
  ]) {
    const r = await desfecho(pagina({ permissao: estado }).invoke("permissao_de_microfone"));
    confere(caso, r.ok === esperada, `com «${estado}», voltou ${util.inspect(r)}, e não ${esperada}`);
  }
}

(async () => {
  const provas = [
    oLugar,
    aMarca,
    asListasEOsCorpos,
    asRecusas,
    osNeutros,
    sairSai,
    semSessao,
    aAberturaNuncaEntra,
    oListenEORegistro,
    oApelidoLocal,
    asPreviasConsentidas,
    abrirNoNavegador,
    oArquivoPorAlca,
    aImagemColada,
    oArraste,
    aSaidaDeSom,
    aTelaCheia,
    asPreferenciasDaVoz,
    osMicrofones,
    aPermissaoDoMicrofone,
  ];
  for (const prova of provas) {
    try {
      await prova();
    } catch (erro) {
      falhas.push(`${prova.name}: lançou ${erro?.stack ?? erro}`);
    }
  }
  terminou = true;
  if (falhas.length === 0) {
    console.log(`ponte web: as ${provas.length} provas passam`);
    process.exit(0);
  }
  for (const falha of falhas) console.error(`FALHOU — ${falha}`);
  process.exit(1);
})();
