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
} = {}) {
  const ouvidos = [];
  const avisos = [];
  const linhas = [];
  const abertos = [];
  // O resto da origem, que o lote B grava. O `seele.casca` continua vindo de
  // `guardado`.
  const origem = new Map();
  const janela = {
    structuredClone,
    TextEncoder,
    innerWidth: largura,
    location: { hash, href: `https://seele.invalid/${hash}` },
    document: { documentElement: { dataset: {} } },
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
    invoke: (comando, args) => janela.__TAURI__.core.invoke(comando, args),
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
    ...nomes("ATENDIDOS_APESAR_DA_HOSPEDAGEM"),
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

/// **Sair da sessão sai.** O `disconnect` mexe na hospedagem no desktop, e
/// mesmo assim é atendido aqui: como neutro, ele responderia sem sair.
async function sairSai() {
  const caso = "sair sai";
  for (const comando of ["disconnect", "desmontar_a_sessao"]) {
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
