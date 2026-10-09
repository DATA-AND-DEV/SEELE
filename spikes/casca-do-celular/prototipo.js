// O protótipo navegável da tela do celular.
//
// **Descartável.** Ele existe para o desenho de
// `docs/superpowers/specs/2026-10-09-0.16-celular-design.md` poder ser tocado,
// fotografado e revisado antes de a casca ser escrita. Não fala com servidor
// nenhum, e os dados são os de `dados.js`.
//
// Mesmas regras da casca: nenhum `innerHTML` (tudo por `createElement` e
// `textContent`), as cores dos tokens, os glifos de `glifos.js` e as frases do
// `frases.js` quando elas já existem. As que o celular acrescenta estão em
// `FRASES_DO_CELULAR`, e vão para o `frases.js` na Etapa B.
//
// Cada estado tem um endereço (`?tela=sessao&aba=pessoas&tema=papel`…), para a
// mesma foto poder ser tirada de novo; `fotos.py` lista todos.

"use strict";

const UI = "../../apps/seele-app/ui/";

// --------------------------------------------------------------- as frases

const FRASES_DO_CELULAR = {
  abas: { servidor: "SERVIDOR", conversa: "CONVERSA", pessoas: "PESSOAS", ajustes: "AJUSTES" },
  inicio: {
    titulo: "SERVIDORES DESTE APARELHO",
    vazio: "Nenhum servidor ainda. Abra um link de convite, ou cole um aqui.",
    colar: "COLAR UM LINK",
    entrar: "ENTRAR",
    hospedar: "Hospedar um servidor é pelo SEELE no computador.",
    identidade: "Sua identidade mora neste navegador, neste aparelho.",
  },
  entrar: {
    titulo: "ENTRAR EM",
    sobre: "Nada saiu deste aparelho ainda: a conexão só abre quando você tocar em ENTRAR.",
    endereco: "ENDEREÇO",
    impressao: "IMPRESSÃO DO SERVIDOR",
    impressaoNota: "É a mesma que o SEELE de quem hospeda mostra. Quem atender com outra é recusado.",
    versao: "VERSÃO",
    vale: "O LINK VALE ATÉ",
    apelido: "COMO OS OUTROS VÃO TE CHAMAR",
    persistente: "Este navegador guarda a sua identidade neste aparelho.",
    naoPersistente:
      "Este navegador pode apagar a sua identidade se faltar espaço. Instalado na Tela de Início, ele guarda.",
    entrar: "ENTRAR",
    agoraNao: "AGORA NÃO",
  },
  estadosDoConvite: {
    "numa-aba": {
      titulo: "INSTALE ANTES DE ENTRAR",
      corpo: "Você está numa aba do navegador. A identidade nasce onde você entra, e a aba e o ícone da Tela de Início não se enxergam: quem entra pela aba e depois instala vira outra pessoa, já sem convite.",
      passos: ["Toque em Compartilhar.", "Toque em Adicionar à Tela de Início.", "Abra o SEELE pelo ícone e cole este link."],
      acao: "COPIAR O LINK",
      outra: "ENTRAR NESTA ABA MESMO ASSIM",
    },
    vencido: {
      titulo: "ESTE LINK VENCEU EM 22/10",
      corpo: "O navegador só abre a conexão com o certificado que o link anuncia, e ele expirou. Peça um link novo a quem hospeda.",
    },
    "servidor-0.15": {
      titulo: "ESTE LINK É DE UM SERVIDOR 0.15",
      corpo: "O celular entra só no Servidor 1.0. Abra o link no SEELE do computador, ou peça a quem hospeda o Servidor 1.0.",
    },
    incompleto: {
      titulo: "LINK INCOMPLETO",
      corpo: "Falta a impressão do servidor ou o certificado do navegador. Sem os dois não há como conferir quem responde, e o navegador não abre a conexão. Peça o link de novo, copiado inteiro.",
    },
    "outra-origem": {
      titulo: "ESTE LINK É DE OUTRA PÁGINA DO SEELE",
      corpo: "A sua identidade mora nesta página. Entrar pela outra cria outra pessoa, sem o que você tem aqui.",
    },
    "ja-conhecido": {
      titulo: "ESTE APARELHO JÁ ENTRA NESTE SERVIDOR",
      corpo: "Você entra com a identidade que já tem aqui, e o convite não é usado.",
    },
    "apelido-tomado": {
      titulo: "ESTE APELIDO JÁ É DE OUTRA CHAVE NESTE SERVIDOR",
      corpo: "Se for outra pessoa, escolha outro apelido. Se for você, este aparelho perdeu a identidade de antes: quem hospeda libera o nome. O convite continua valendo.",
    },
  },
  conectando: "CONECTANDO",
  aceite: {
    titulo: (n) => `ESTE SERVIDOR USA ${n} MODS`,
    corpo: "Eles rodam neste aparelho, isolados: sem rede e sem acesso à sua identidade.",
    tema: "MUDA A APARÊNCIA DO SERVIDOR",
    oficial: "OFICIAL",
    aceitar: "ACEITAR E ENTRAR",
    recusar: "NÃO ENTRAR",
  },
  servidor: {
    redes: "SERVIDORES",
    outro: "OUTRO SERVIDOR",
    salas: "SALAS DE VOZ",
    canais: "CANAIS",
    dosMods: "DOS MODS",
    aqui: "VOCÊ ESTÁ AQUI",
    cheia: "LOTADA",
    senha: "COM SENHA",
    entrar: "ENTRAR NA SALA",
    entrarMudo: "Entrar mudo na sala",
    mudo: "MUDO",
    abrir: "ABRIR A CHAMADA",
    salaCheia: "SALA CHEIA",
    ninguem: "ninguém aqui",
    foraSalas: "Criar, renomear e apagar salas e canais é pelo SEELE no computador.",
  },
  voz: {
    mic: "MIC", mudo: "MUDO", som: "SOM", surdo: "SURDO", sair: "SAIR",
    segure: "SEGURE", noAr: "NO AR",
    recolher: "RECOLHER",
    falando: (quem) => `${quem} fala`,
    naSala: (n) => `${n} na sala`,
    reconectando: "SEM SOM · RECONECTANDO",
  },
  conversa: {
    escrever: (canal) => `escrever em #${canal}`,
    enviar: "ENVIAR",
    falhou: "NÃO ENVIADA",
    reenviar: "REENVIAR",
    descartar: "DESCARTAR",
  },
  pessoas: {
    noServidor: "NO SERVIDOR, FORA DE SALA",
    foraDoAr: "FORA DO AR",
    estados: { falando: "FALANDO", mudo: "MUDO", surdo: "SEM SOM", ouvindo: "OUVINDO" },
    faixas: { Verde: "BOA", Laranja: "FRACA", Vermelho: "CRÍTICA" },
  },
  folhaDaPessoa: {
    volume: "VOLUME DESTA PESSOA",
    perfil: "VER PERFIL",
    moderar: "MODERAR",
    moderarFora: "Moderar é pelo SEELE no computador.",
    sinal: "SINAL",
  },
  chamada: {
    titulo: "CHAMADA",
    transmitindo: "TRANSMITINDO",
    telaCheia: "TELA CHEIA",
    escolha: "Transmissões desta sala",
    naoVer: "NÃO VER",
    silenciar: "SILENCIAR",
    silenciarNota: "Cala só esta transmissão; as pessoas continuam.",
    volumeDaTela: "VOLUME DA TELA",
    pararDeAssistir: "PARAR DE ASSISTIR",
    naoAbre: "ESTA TELA NÃO ABRE NESTE APARELHO",
    naoAbreNota: "O navegador deste aparelho não decodifica o vídeo que esta transmissão manda. A sala e a conversa seguem normalmente.",
    grade: "GRADE",
    palcoDeMod: "MESA",
    semCompartilhar: "Compartilhar a própria tela é pelo SEELE no computador. A tela de quem compartilhar aparece aqui.",
    telemetria: { atraso: "ATRASO", jitter: "JITTER", perda: "PERDA", codec: "CODEC" },
  },
  ajustes: {
    seuPerfil: "SEU PERFIL",
    editar: "EDITAR PERFIL",
    chave: "CHAVE",
    secoes: [
      ["audio", "audio", "ÁUDIO", "VOZ · microfone do aparelho"],
      ["aparencia", "aparencia", "APARÊNCIA", "segue o sistema"],
      ["servidor", "engrenagem", "ESTE SERVIDOR", "aparência · MODs"],
      ["previas", "linhas", "PRÉVIAS DE LINK", "perguntar antes"],
      ["conexao", "atualizar", "CONEXÃO", "atraso 23 ms · perda 0,2% · caminho direto"],
      ["aparelho", "chave", "ESTE APARELHO", "identidade guardada"],
      ["computador", "teclado", "NO COMPUTADOR", "o que o celular não faz"],
      ["sobre", "atualizar", "SOBRE", "página do Servidor 1.0"],
    ],
    sair: "SAIR DO SERVIDOR",
    voltar: "AJUSTES",
  },
  audio: {
    modo: "MODO DO MICROFONE",
    voz: "VOZ",
    segurar: "SEGURAR PARA FALAR",
    vozNota: "O microfone abre quando você fala. É o único modo que funciona com a tela bloqueada.",
    segurarNota: "O botão grande da faixa fala enquanto o dedo está nele. Com a tela bloqueada, o microfone fica fechado.",
    microfone: "MICROFONE",
    saida: "SAÍDA DE SOM",
    saidaNota: "O sistema escolhe: alto-falante, fone ou Bluetooth, pela Central de Controle.",
    tratamento: "TRATAMENTO DO SOM",
    tratamentoNota: "Eco, ruído e ganho: os do navegador, ligados.",
    teste: "TESTE DO MICROFONE",
  },
  noComputador: [
    ["HOSPEDAR UM SERVIDOR", "O celular entra em servidores e não hospeda. Hospedar é pelo SEELE no computador."],
    ["COMPARTILHAR A TELA", "O navegador do celular não grava a tela. A tela de quem compartilhar aparece na chamada."],
    ["MODERAR", "Expulsar, banir, mover pessoas e remover mensagens de outros é pelo SEELE no computador."],
    ["A PORTARIA", "Aprovar quem bate, a senha e os convites ficam na máquina que hospeda."],
    ["SALAS E CANAIS", "Criar, renomear e apagar é pelo SEELE no computador."],
    ["NOME E IMAGEM DO SERVIDOR", "Pelo SEELE no computador. A aparência do servidor se edita aqui, em ESTE SERVIDOR."],
    ["INSTALAR E LIGAR MODS", "Os MODs deste servidor rodam aqui. Instalar, ligar e desligar MODs no servidor é de quem hospeda."],
    ["LIGAR PELO CÓDIGO", "Não existe no celular nesta versão: o navegador não abre o caminho que a ligação usa."],
    ["VERSÕES LADO A LADO", "O celular abre a página da versão do servidor que o link pede. Guardar versões é do SEELE no computador."],
  ],
  estilo: {
    titulo: "APARÊNCIA DO SERVIDOR",
    de: "DO MOD ESTILO",
    abas: ["CORES", "FORMA", "CONJUNTOS"],
    ver: "VER NA SESSÃO",
    publicar: "PUBLICAR",
    restaurar: "RESTAURAR PADRÃO",
    selo: "PRÉVIA NÃO PUBLICADA · SÓ NESTE APARELHO",
    voltar: "VOLTAR",
  },
  queda: {
    titulo: "CONEXÃO PERDIDA",
    conta: (t) => `${t} para o servidor guardar o seu lugar`,
    tentativa: (n) => `TENTATIVA ${n}`,
    agora: "TENTAR AGORA",
  },
  fim: { reconectar: "RECONECTAR", servidores: "SERVIDORES DESTE APARELHO" },
  fechar: "FECHAR",
};

// ------------------------------------------------------------- o desenho

/** Um elemento, por `createElement`. `filhos` aceita texto, nós e listas. */
function h(tag, props = {}, ...filhos) {
  const el = document.createElement(tag);
  for (const [chave, valor] of Object.entries(props)) {
    if (valor === undefined || valor === null || valor === false) continue;
    if (chave === "class") el.className = valor;
    else if (chave === "text") el.textContent = valor;
    else if (chave.startsWith("on")) el.addEventListener(chave.slice(2), valor);
    else if (chave === "dataset") Object.assign(el.dataset, valor);
    else if (chave === "style") Object.assign(el.style, valor);
    else el.setAttribute(chave, valor === true ? "" : valor);
  }
  for (const filho of filhos.flat(Infinity)) {
    if (filho === null || filho === undefined || filho === false) continue;
    el.append(typeof filho === "string" ? document.createTextNode(filho) : filho);
  }
  return el;
}

const rotulo = (texto, extra = "") => h("span", { class: `rotulo ${extra}`.trim(), text: texto });

// --------------------------------------------------------------- o estado

const parametros = new URLSearchParams(location.search);
const estado = {
  tela: parametros.get("tela") ?? "inicio",
  convite: parametros.get("convite") ?? "pronto",
  aba: parametros.get("aba") ?? "conversa",
  sala: parametros.has("sala") ? Number(parametros.get("sala")) : null,
  chamada: parametros.get("chamada") === "1",
  folha: parametros.get("folha"),
  tema: parametros.get("tema"),
  teclado: parametros.get("teclado") === "1",
  queda: parametros.get("queda") === "1",
  modo: parametros.get("modo") ?? "voz",
  previa: parametros.get("previa") === "1",
  sub: parametros.get("sub"),
  mod: parametros.get("mod"),
  mudo: parametros.get("mudo") === "1",
  surdo: false,
  apertado: false,
  canal: 1,
};

function mudar(novo) {
  Object.assign(estado, novo);
  desenharTudo();
}

// -------------------------------------------------- o texto do sistema
//
// O texto dinâmico do iOS chega à página só pela fonte `-apple-system-body`: o
// tamanho dela é o que a pessoa escolheu em Ajustes › Acessibilidade. A casca
// mede uma vez e escala a escala de tipo do produto inteira pela mesma razão,
// sem trocar de família. Fora do Safari a fonte não existe e a escala fica 1.

function medirTextoDoSistema() {
  const forcado = Number(parametros.get("texto"));
  if (forcado > 0) return forcado;
  const sonda = h("span", { "aria-hidden": "true" }, "x");
  sonda.style.font = "-apple-system-body";
  if (!sonda.style.font) return 1;
  sonda.style.position = "absolute";
  sonda.style.visibility = "hidden";
  document.body.append(sonda);
  const tamanho = parseFloat(getComputedStyle(sonda).fontSize);
  sonda.remove();
  // 17 é o tamanho padrão do corpo no iOS; o produto não encolhe abaixo dele.
  return Number.isFinite(tamanho) ? Math.min(Math.max(tamanho / 17, 1), 2) : 1;
}

// ------------------------------------------------------------- o tema
//
// O mesmo mapa de `base.js` (`TEMA_DA_API`, `NUMEROS_DA_API`, `BANDEIRAS_DA_API`,
// `MEDIDAS_DA_API`), escrito no mesmo contêiner. Copiado aqui porque o protótipo
// não carrega `base.js`; a casca não copia nada, ela é o `base.js`.

const TEMA_DA_API = {
  fundo: "--seele-negro-absoluto", texto: "--seele-osso", acento: "--seele-laranja-nerv",
  borda: "--seele-linha", painel: "--seele-negro-painel", apagado: "--seele-rotulo-painel",
};

// **As cores de estado sob um tema (provisório).** O fósforo e o azul não são
// chaves do tema; o produto as deriva das superfícies, com o mesmo matiz e a
// luminosidade movida até 4,5:1 sobre o fundo e o painel (F3, rascunho da API 6
// §9, tarefa 17e). Esta função ocupa o lugar da da F3 até ela existir; o
// vermelho-alerta não passa por aqui (fica fora de todo caminho de tema).
function luz(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
const razao = (a, b) => { const [x, y] = [luz(a), luz(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
function hsl(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b), min = Math.min(r, g, b), l = (max + min) / 2, d = max - min;
  if (!d) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [(h * 60 + 360) % 360, s, l];
}
function hex([h, s, l]) {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return "#" + [r, g, b].map((v) => Math.round((v + m) * 255).toString(16).padStart(2, "0")).join("");
}
function derivarEstado(cor, fundo, painel) {
  const [h, s, l0] = hsl(cor);
  const escuro = luz(painel) < 0.18;
  for (let passo = 0; passo <= 100; passo++) {
    const l = Math.min(1, Math.max(0, l0 + (escuro ? 1 : -1) * passo * 0.01));
    const tom = hex([h, s, l]);
    if (razao(tom, fundo) >= 4.5 && razao(tom, painel) >= 4.5) return tom;
  }
  return escuro ? "#ffffff" : "#000000";
}

function aplicarTema(sessao) {
  const valores = TEMAS[estado.tema];
  if (!valores) return;
  for (const [nome, token] of Object.entries(TEMA_DA_API)) sessao.style.setProperty(token, valores[nome]);
  sessao.style.setProperty("--seele-raio", `${valores.arredondamento}px`);
  sessao.style.setProperty("--seele-sombra", valores.brilho ? "0 0 12px var(--seele-laranja-nerv)" : "none");
  if (valores.fonte === "sans") sessao.style.setProperty("--seele-mono", "var(--seele-pilha-sans)");
  const densidade = valores.densidade === "compacta"
    ? { "--seele-celula-x": "6px", "--seele-celula-y": "10px" }
    : { "--seele-celula-x": "8px", "--seele-celula-y": "16px" };
  for (const [token, valor] of Object.entries(densidade)) sessao.style.setProperty(token, valor);
  for (const [token, base] of [["--seele-fosforo", "#6BFFB6"], ["--seele-padrao-azul", "#2F8FE0"], ["--seele-sync-nominal", "#6BFFB6"]]) {
    sessao.style.setProperty(token, derivarEstado(base, valores.fundo, valores.painel));
  }
  // O tom fraco do acento (a sala em que se está): o mesmo lugar, provisório,
  // da fórmula da F3 que reproduz o `--seele-laranja-fraco` a partir do padrão.
  sessao.style.setProperty("--seele-laranja-fraco", "color-mix(in srgb, var(--seele-laranja-nerv) 14%, var(--seele-negro-painel))");
}

// ------------------------------------------------------------ a entrada

function marcaAssinatura() {
  return h("img", { src: `${UI}marca-assinatura.svg`, alt: "SEELE" });
}

function telaInicio() {
  const f = FRASES_DO_CELULAR.inicio;
  return h("section", { class: "tela cel-entrada", "aria-labelledby": "inicio-titulo" },
    h("div", { class: "cel-entrada-marca" }, marcaAssinatura()),
    h("div", { class: "cel-entrada-corpo" },
      h("p", { class: "coordenada", text: "A·01 / ESTE APARELHO" }),
      h("h1", { id: "inicio-titulo", class: "cel-titulo cel-titulo-grande", text: f.titulo }),
      h("ul", { class: "cel-lista", style: { marginTop: "16px" } },
        SERVIDORES_DO_APARELHO.map((s) => h("li", {},
          h("button", { class: "cel-linha", type: "button", onclick: () => mudar({ tela: "entrar", convite: "ja-conhecido" }) },
            h("span", { class: "cel-sigla", text: s.sigla }),
            h("span", { class: "cel-linha-texto" },
              h("span", { class: "cel-titulo cel-titulo-medio", text: s.nome }),
              h("span", { class: "cel-linha-sub", text: `${s.endereco} · Servidor ${s.versao} · ${s.ultima}` })),
            h("span", { class: "rotulo", text: f.entrar }))))),
      h("p", { class: "cel-nota", style: { marginTop: "16px" }, text: f.identidade }),
      h("p", { class: "cel-fora" }, rotulo("HOSPEDAR"), f.hospedar)),
    h("div", { class: "cel-entrada-pe" },
      h("button", { class: "botao cel-largo", type: "button", onclick: () => mudar({ tela: "entrar", convite: "pronto" }) }, f.colar)));
}

function impressaoEmGrupos(fp) {
  const grupos = fp.toUpperCase().match(/.{1,4}/g) ?? [];
  return h("span", { class: "cel-impressao", "aria-label": grupos.join(" ") }, grupos.map((g) => h("span", { text: g })));
}

function telaEntrar() {
  const f = FRASES_DO_CELULAR.entrar;
  const especial = FRASES_DO_CELULAR.estadosDoConvite[estado.convite];
  const bloqueia = ["vencido", "servidor-0.15", "incompleto", "outra-origem"].includes(estado.convite);
  const aviso = especial && h("div", {
    class: "cel-aviso", role: "alert",
    "data-tipo": bloqueia || estado.convite === "apelido-tomado" ? "erro" : null,
  },
    h("p", { class: "cel-titulo cel-titulo-medio", text: especial.titulo }),
    h("p", { class: "cel-nota", text: especial.corpo }),
    especial.passos && h("ol", { class: "cel-passos" }, especial.passos.map((p) => h("li", { text: p }))));

  const ficha = !["incompleto"].includes(estado.convite) && h("dl", { class: "cel-ficha" },
    h("dt", {}, rotulo(f.endereco)), h("dd", { text: CONVITE.endereco }),
    h("dt", {}, rotulo(f.impressao)), h("dd", {}, impressaoEmGrupos(CONVITE.impressao)),
    h("dt", {}, rotulo(f.versao)), h("dd", { text: `Servidor ${estado.convite === "servidor-0.15" ? "0.15" : CONVITE.versaoDoServidor}` }),
    h("dt", {}, rotulo(f.vale)), h("dd", { text: CONVITE.valeAte }));

  const podeEntrar = !bloqueia;
  const apelido = podeEntrar && estado.convite !== "ja-conhecido" && h("label", { class: "cel-campo" },
    rotulo(f.apelido),
    h("input", { type: "text", value: estado.convite === "apelido-tomado" ? "" : "rafa",
                 autocomplete: "nickname", autocapitalize: "none", spellcheck: "false",
                 "aria-invalid": estado.convite === "apelido-tomado" ? "true" : null }));

  const acoes = [];
  if (estado.convite === "numa-aba") {
    acoes.push(h("button", { class: "botao cel-largo", type: "button" }, especial.acao));
    acoes.push(h("button", { class: "botao botao-secundario cel-largo", type: "button", onclick: () => mudar({ tela: "conectando" }) }, especial.outra));
  } else if (podeEntrar) {
    acoes.push(h("button", { class: "botao cel-largo", type: "button", onclick: () => mudar({ tela: "conectando" }) }, f.entrar));
    acoes.push(h("button", { class: "botao-fantasma cel-largo", type: "button", onclick: () => mudar({ tela: "inicio" }) }, f.agoraNao));
  } else {
    acoes.push(h("button", { class: "botao-fantasma cel-largo", type: "button", onclick: () => mudar({ tela: "inicio" }) }, FRASES_DO_CELULAR.fim.servidores));
  }

  return h("section", { class: "tela cel-entrada", "aria-labelledby": "entrar-titulo" },
    h("div", { class: "cel-entrada-marca" }, marcaAssinatura()),
    h("div", { class: "cel-entrada-corpo" },
      h("p", { class: "coordenada", text: "A·02 / CONVITE" }),
      h("h1", { id: "entrar-titulo", class: "cel-titulo cel-titulo-grande" }, `${f.titulo} `, h("span", { class: "cel-dado", style: { fontFamily: "var(--seele-mono)", textTransform: "none", display: "block", fontSize: "calc(var(--seele-t-secao) * var(--cel-escala))", marginTop: "4px" }, text: `${CONVITE.endereco}?` })),
      aviso,
      !especial && h("p", { class: "cel-nota", style: { marginTop: "12px" }, text: f.sobre }),
      ficha,
      !bloqueia && h("p", { class: "cel-nota cel-apagado", style: { marginTop: "8px" }, text: f.impressaoNota }),
      apelido,
      podeEntrar && h("p", { class: "cel-nota", style: { marginTop: "12px" }, text: CONVITE.persistente ? f.persistente : f.naoPersistente })),
    h("div", { class: "cel-entrada-pe" }, acoes));
}

let simbolo = null;
async function carregarSimbolo() {
  // O SVG da marca, inteiro, para o enlace poder ser desenhado: lido do mesmo
  // arquivo que a casca serve, e não copiado.
  try {
    const texto = await (await fetch(`${UI}marca-simbolo.svg`)).text();
    const doc = new DOMParser().parseFromString(texto, "image/svg+xml");
    simbolo = document.importNode(doc.documentElement, true);
  } catch {
    simbolo = h("img", { src: `${UI}marca-simbolo.svg`, alt: "SEELE" });
  }
}

function telaConectando() {
  const etapas = ["Parada", "Tentando", "CaminhoAberto"];
  setTimeout(() => {
    if (estado.tela === "conectando" && !parametros.has("tela")) mudar({ tela: "aceite" });
  }, 1400);
  return h("section", { class: "tela cel-entrada", "aria-busy": "true", "aria-labelledby": "conectando-titulo" },
    h("div", {}),
    h("div", { class: "cel-conectando" },
      simbolo ? simbolo.cloneNode(true) : null,
      h("p", { id: "conectando-titulo", class: "cel-titulo cel-titulo-medio", style: { textAlign: "center" } },
        FRASES_DO_CELULAR.conectando,
        h("span", { class: "cel-dado", style: { display: "block", fontFamily: "var(--seele-mono)", textTransform: "none", fontWeight: "400", marginTop: "4px" }, text: CONVITE.endereco })),
      h("ol", { class: "cel-etapas" }, etapas.map((e, i) => h("li", { "data-feita": i < 2 ? "" : null },
        typeof fraseDeEtapa === "function" ? fraseDeEtapa(e) : e)))),
    h("div", {}));
}

function telaAceite() {
  const f = FRASES_DO_CELULAR.aceite;
  return h("section", { class: "tela cel-entrada", "aria-labelledby": "aceite-titulo" },
    h("div", { class: "cel-entrada-marca" }, h("p", { class: "coordenada", text: "A·03 / MODS DESTE SERVIDOR" })),
    h("div", { class: "cel-entrada-corpo" },
      h("h1", { id: "aceite-titulo", class: "cel-titulo cel-titulo-grande", text: f.titulo(MODS_DO_SERVIDOR.length) }),
      h("p", { class: "cel-nota", style: { marginTop: "8px" }, text: f.corpo }),
      h("ul", { class: "cel-lista", style: { marginTop: "16px" } }, MODS_DO_SERVIDOR.map((m) => h("li", { class: "cel-linha", style: { display: "block" } },
        h("span", { class: "cel-sala-cabeca" },
          h("span", { class: "cel-titulo cel-titulo-medio cel-sala-nome", text: m.nome }),
          h("span", { class: "cel-dado cel-apagado", text: m.versao }),
          m.oficial && h("span", { class: "cel-marca cel-marca-aqui", text: f.oficial })),
        h("p", { class: "cel-nota", style: { marginTop: "4px" }, text: m.faz }),
        m.tema && h("p", { class: "cel-marca", style: { display: "inline-block", marginTop: "6px", color: "var(--seele-laranja-nerv)" }, text: f.tema }))))),
    h("div", { class: "cel-entrada-pe" },
      h("button", { class: "botao cel-largo", type: "button", onclick: () => mudar({ tela: "sessao", aba: "conversa" }) }, f.aceitar),
      h("button", { class: "botao-fantasma cel-largo", type: "button", onclick: () => mudar({ tela: "inicio" }) }, f.recusar)));
}

function telaFim() {
  const f = FRASES_DO_CELULAR.fim;
  const motivo = typeof MOTIVOS === "object" ? MOTIVOS.ServerShuttingDown : "O SERVIDOR ESTÁ ENCERRANDO";
  return h("section", { class: "tela cel-entrada", "aria-labelledby": "fim-titulo" },
    h("div", { class: "cel-entrada-marca" }, marcaAssinatura()),
    h("div", { class: "cel-entrada-corpo" },
      h("p", { class: "coordenada", text: "A·04 / SESSÃO ENCERRADA" }),
      h("h1", { id: "fim-titulo", class: "cel-titulo cel-titulo-grande", text: motivo }),
      h("dl", { class: "cel-ficha" },
        h("dt", {}, rotulo("SERVIDOR")), h("dd", { text: `TÓQUIO-3 · ${CONVITE.endereco}` }),
        h("dt", {}, rotulo("DURAÇÃO")), h("dd", { text: "01:12:44" }))),
    h("div", { class: "cel-entrada-pe" },
      h("button", { class: "botao cel-largo", type: "button", onclick: () => mudar({ tela: "conectando" }) }, f.reconectar),
      h("button", { class: "botao-fantasma cel-largo", type: "button", onclick: () => mudar({ tela: "inicio" }) }, f.servidores)));
}

// -------------------------------------------------------------- a sessão

const pessoa = (id) => QUADRO.voice_rooms.flatMap((s) => s.people).concat(QUADRO.presentes_sem_sala).find((p) => p.id === id);
const salaAtual = () => QUADRO.voice_rooms.find((s) => s.id === estado.sala);
const iniciais = (nome) => nome.slice(0, 2).toUpperCase();
// A `blocos()` de `tela-sessao.js:209`, com as mesmas 20 células de 5 % do
// desktop: quem aprendeu a ler a barra lá lê a mesma aqui. Na casca, o celular
// chama aquela função, e não esta cópia.
const barra = (sinal) => {
  const cheios = Math.max(0, Math.min(20, Math.round((sinal / 100) * 20)));
  return "█".repeat(cheios) + "░".repeat(20 - cheios);
};

function avatar(p, extra = {}) {
  return h("span", {
    class: "cel-avatar", "aria-hidden": "true",
    "data-proprio": p.is_self || p.id === QUADRO.me ? "" : null,
    "data-retrato": p.id === 2 ? "" : null, ...extra,
  }, p.id === 2 ? "" : iniciais(p.nickname));
}

/** O nome que aparece, e o verificado ao lado quando um MOD o trocou (D-nucleo-1). */
function nomeComVerificado(p, classe) {
  const deMod = NOMES_DE_MOD[p.id];
  return [
    h("span", { class: classe, text: deMod ?? p.nickname }),
    deMod && " ",
    deMod && h("span", { class: "cel-verificado", text: `@${p.nickname}`, title: null, "aria-label": `apelido verificado ${p.nickname}` }),
  ];
}

function topo(sobre, titulo, acoes = []) {
  const seguro = QUADRO.link_state === "Verified";
  return h("header", { class: "cel-topo" },
    h("div", { class: "cel-topo-texto" },
      h("p", { class: "cel-topo-sobre" },
        `${QUADRO.server} · `,
        h("span", { id: "selo-seguro", class: "cel-selo-seguro", "data-estado": seguro ? "segura" : "sem", text: seguro ? "CONEXÃO SEGURA" : "SEM CONEXÃO SEGURA" }),
        sobre ? ` · ${sobre}` : ""),
      titulo),
    acoes);
}

function vozRecolhida() {
  if (!estado.sala) return null;
  return h("div", { class: "voz-recolhida" },
    h("span", { class: "voz-recolhida-sala", text: salaAtual().name }),
    botaoMudo(true));
}

function painelServidor() {
  const f = FRASES_DO_CELULAR.servidor;
  return h("div", { class: "cel-painel", role: "tabpanel", "aria-label": FRASES_DO_CELULAR.abas.servidor },
    h("nav", { class: "cel-redes", "aria-label": f.redes, "data-rola-de-lado": "" },
      SERVIDORES_DO_APARELHO.map((s, i) => h("button", { class: "cel-rede", type: "button", "aria-current": i === 0 ? "true" : null },
        h("span", { class: "cel-sigla", text: s.sigla }), s.nome)),
      h("button", { class: "cel-rede", type: "button", onclick: () => mudar({ tela: "inicio" }) },
        h("span", { class: "cel-sigla", text: "+" }), f.outro)),
    h("div", { class: "cel-secao-titulo" }, rotulo(f.salas), h("span", { class: "cel-dado cel-apagado", text: `${QUADRO.voice_rooms.length}` })),
    QUADRO.voice_rooms.map((sala) => {
      const nossa = sala.id === estado.sala;
      // LOTADA é dito, e o ENTRAR continua: quem recusa é o servidor, como no
      // desktop (`SALA DE VOZ LOTADA`), e a lista pode estar um quadro atrás.
      const cheia = sala.people.length >= sala.limit && !nossa;
      return h("section", { class: "cel-sala", "data-nossa": nossa ? "" : null, "aria-label": `Sala ${sala.name}` },
        h("div", { class: "cel-sala-cabeca" },
          h("h3", { class: "cel-titulo cel-titulo-medio cel-sala-nome" }, `${sala.name} `, h("span", { class: "cel-dado", text: `${sala.people.length}/${sala.limit}` })),
          nossa && h("span", { class: "cel-marca cel-marca-aqui", text: f.aqui }),
          sala.password_required && h("span", { class: "cel-marca cel-marca-senha" }, glifo("chave"), ` ${f.senha}`),
          cheia && h("span", { class: "cel-marca cel-marca-cheia", text: f.cheia })),
        sala.people.length
          ? h("ul", { class: "cel-sala-pessoas" }, sala.people.map((p) => h("li", { class: p.speaking ? "cel-falando" : null },
              p.speaking ? glifo("falando") : glifo("silencio"), ` ${NOMES_DE_MOD[p.id] ?? p.nickname}${p.is_self ? " (você)" : ""}`)))
          : h("p", { class: "cel-sala-pessoas cel-sala-vazia", text: f.ninguem }),
        nossa
          ? h("button", { class: "botao cel-largo", type: "button", onclick: () => mudar({ chamada: true }) }, f.abrir)
          // ENTRAR MUDO ao lado: fora de sala não há faixa de voz, e o áudio
          // só abre ao entrar (D-52). Sem isto, quem quer chegar calado entra
          // com o microfone aberto e corre para o MUDO.
          : h("div", { class: "cel-sala-entrar" },
              h("button", {
                class: "botao botao-secundario", type: "button",
                onclick: () => (sala.password_required ? mudar({ folha: `senha:${sala.id}` }) : mudar({ sala: sala.id, mudo: false, chamada: true })),
              }, f.entrar),
              h("button", {
                class: "botao-fantasma cel-entrar-mudo", type: "button", "aria-label": `${f.entrarMudo} ${sala.name}`,
                onclick: () => (sala.password_required ? mudar({ folha: `senha:${sala.id}` }) : mudar({ sala: sala.id, mudo: true, chamada: true })),
              }, glifoComEstado("microfone", true), f.mudo)));
    }),
    h("div", { class: "cel-secao-titulo" }, rotulo(f.canais)),
    h("ul", { class: "cel-lista" }, QUADRO.channels.map((c) => h("li", {},
      h("button", { class: "cel-linha", type: "button", "aria-current": c.id === estado.canal ? "true" : null, onclick: () => mudar({ canal: c.id, aba: "conversa" }) },
        h("span", { class: "cel-linha-texto" }, h("span", { style: { color: "var(--seele-laranja-nerv)" }, text: "# " }), c.name),
        c.novas > 0 && h("span", { class: "cel-conta", "aria-label": `${c.novas} novas`, text: String(c.novas) }))))),
    h("div", { class: "cel-secao-titulo" }, rotulo(f.dosMods)),
    h("ul", { class: "cel-lista" },
      h("li", {}, h("button", { class: "cel-linha", type: "button", onclick: () => mudar({ aba: "conversa", mod: "pagina" }) },
        h("span", { class: "cel-linha-texto" }, "Campanha · Ilha Sombria", h("span", { class: "cel-de-mod", style: { display: "block" }, text: "DO MOD MESA" })),
        h("span", { class: "cel-seta" }, glifo("avancar"))))),
    QUADRO.may_manage_voice_rooms && h("p", { class: "cel-fora" }, rotulo("SALAS E CANAIS"), f.foraSalas));
}

function painelConversa() {
  if (estado.mod === "pagina") return paginaDeMod();
  const f = FRASES_DO_CELULAR.conversa;
  const canal = QUADRO.channels.find((c) => c.id === estado.canal);
  const lista = h("ol", { class: "cel-mensagens", "aria-label": `mensagens de #${canal.name}` },
    h("li", { class: "cel-dia", text: "HOJE" }),
    MENSAGENS.map((m) => {
      const autor = pessoa(m.author) ?? { id: m.author, nickname: m.author_nickname };
      return h("li", { class: "cel-msg", "data-propria": m.own ? "" : null, tabindex: "0",
                       onclick: () => mudar({ folha: `mensagem:${m.id}` }) },
        avatar({ ...autor, is_self: m.own }),
        h("div", { class: "cel-msg-autor" }, nomeComVerificado(autor, "cel-msg-nome"), h("span", { class: "cel-msg-hora", text: m.at })),
        h("p", { class: "cel-msg-corpo", text: m.body }),
        m.attachment && h("button", { class: "cel-anexo", type: "button" }, glifo("anexo"),
          h("span", {}, m.attachment.nome, h("span", { class: "cel-linha-sub", text: m.attachment.tamanho }))),
        m.falhou && h("p", { class: "cel-msg-falhou" }, f.falhou, " · ",
          h("button", { class: "botao-fantasma", type: "button" }, f.reenviar)));
    }));
  const compor = h("form", { class: "cel-compor", onsubmit: (e) => e.preventDefault() },
    h("button", { class: "cel-compor-anexar", type: "button", "aria-label": "anexar", onclick: () => mudar({ folha: "anexar" }) }, "+"),
    h("textarea", { rows: "1", placeholder: f.escrever(canal.name), "aria-label": f.escrever(canal.name), enterkeyhint: "enter",
                    onfocus: () => { if (!estado.teclado) mudar({ teclado: true }); } }),
    h("button", { class: "botao", type: "submit" }, f.enviar));
  return h("div", { class: "cel-painel cel-painel-cheio", role: "tabpanel", "aria-label": FRASES_DO_CELULAR.abas.conversa },
    lista, compor, estado.teclado && h("div", { class: "cel-teclado-falso", "aria-hidden": "true", text: "TECLADO DO SISTEMA" }));
}

function linhaPessoa(p) {
  const f = FRASES_DO_CELULAR.pessoas;
  const qual = p.speaking ? "falando" : p.total_isolation ? "surdo" : p.muted ? "mudo" : "ouvindo";
  return h("li", {},
    h("button", { class: "cel-pessoa", type: "button", onclick: () => mudar({ folha: `pessoa:${p.id}` }) },
      avatar(p),
      h("span", {},
        h("span", { class: "cel-pessoa-nome" }, nomeComVerificado(p, null), p.is_self && h("span", { class: "cel-apagado", text: "(você)" })),
        h("span", { class: "cel-pessoa-estado", "data-estado": qual, text: f.estados[qual] })),
      h("span", { class: "cel-sinal", "data-faixa": p.sync_band },
        h("span", { class: "cel-sinal-numero", text: String(p.signal) }),
        h("span", { class: "cel-sinal-barra", "aria-hidden": "true", text: barra(p.signal) }),
        h("span", { class: "cel-sinal-faixa", text: f.faixas[p.sync_band] }))));
}

function painelPessoas() {
  const f = FRASES_DO_CELULAR.pessoas;
  return h("div", { class: "cel-painel", role: "tabpanel", "aria-label": FRASES_DO_CELULAR.abas.pessoas },
    QUADRO.voice_rooms.filter((s) => s.people.length).map((s) => [
      h("div", { class: "cel-secao-titulo" }, rotulo(`${s.name} ${s.people.length}/${s.limit}`),
        s.id === estado.sala && h("span", { class: "cel-marca cel-marca-aqui", text: FRASES_DO_CELULAR.servidor.aqui })),
      h("ul", { class: "cel-lista" }, s.people.map(linhaPessoa)),
    ]),
    h("div", { class: "cel-secao-titulo" }, rotulo(f.noServidor)),
    h("ul", { class: "cel-lista" }, QUADRO.presentes_sem_sala.map(linhaPessoa)),
    h("div", { class: "cel-secao-titulo" }, rotulo(f.foraDoAr), h("span", { class: "cel-dado cel-apagado", text: String(QUADRO.membros_fora_do_ar.length) })),
    h("ul", { class: "cel-fora-do-ar" }, QUADRO.membros_fora_do_ar.map((m) => h("li", {},
      h("span", { class: "cel-avatar", "aria-hidden": "true", text: iniciais(m.nickname) }), m.nickname))));
}

function painelAjustes() {
  const f = FRASES_DO_CELULAR.ajustes;
  if (estado.sub) return subAjustes(estado.sub);
  return h("div", { class: "cel-painel", role: "tabpanel", "aria-label": FRASES_DO_CELULAR.abas.ajustes },
    h("section", { class: "cel-perfil", "aria-label": f.seuPerfil },
      h("div", { class: "cel-capa", "data-capa": CAPAS[1], "aria-hidden": "true" }),
      h("div", { class: "cel-placa" },
        h("span", { class: "cel-retrato", "aria-hidden": "true", text: "RA" }),
        h("span", {},
          rotulo(f.seuPerfil),
          h("span", { class: "cel-titulo cel-titulo-grande", style: { display: "block" }, text: QUADRO.nickname }),
          h("span", { class: "cel-linha-sub", text: `${f.chave} 7F3A·91C0·E2D4` })))),
    h("section", { class: "cel-operador-barra", "aria-label": "Do MOD PERFIS" },
      h("span", { class: "cel-de-mod", text: "DO MOD PERFIS" }),
      h("p", { class: "cel-nota", text: "em campanha · Ilha Sombria · sessão 12" })),
    h("button", { class: "botao-fantasma cel-largo", type: "button", style: { marginTop: "12px" } }, f.editar),
    h("ul", { class: "cel-lista", style: { marginTop: "16px" } }, f.secoes.map(([id, g, nome, resumo]) => h("li", {},
      h("button", { class: "cel-ajuste", type: "button", onclick: () => mudar({ sub: id }) },
        glifo(g), h("span", {}, nome, h("span", { class: "cel-linha-sub", text: resumo })), h("span", { class: "cel-seta" }, glifo("avancar")))))),
    h("button", { class: "botao botao-secundario cel-largo", type: "button", style: { marginTop: "24px" }, onclick: () => mudar({ tela: "fim" }) }, f.sair));
}

function tituloDaSecao(id) {
  return FRASES_DO_CELULAR.ajustes.secoes.find((s) => s[0] === id)?.[2] ?? id.toUpperCase();
}

function subAjustes(id) {
  const f = FRASES_DO_CELULAR;
  const titulo = () => null;
  let corpo = [];
  if (id === "audio") {
    const a = f.audio;
    corpo = [titulo("ÁUDIO"),
      h("div", { class: "cel-campo" }, rotulo(a.modo),
        h("div", { class: "cel-segmento", role: "group", "aria-label": a.modo },
          h("button", { type: "button", "aria-pressed": String(estado.modo === "voz"), onclick: () => mudar({ modo: "voz" }) }, a.voz),
          h("button", { type: "button", "aria-pressed": String(estado.modo === "segurar"), onclick: () => mudar({ modo: "segurar" }) }, a.segurar)),
        h("p", { class: "cel-nota", text: estado.modo === "voz" ? a.vozNota : a.segurarNota })),
      h("label", { class: "cel-campo" }, rotulo(a.microfone), h("select", {}, h("option", {}, "Microfone do iPhone"), h("option", {}, "AirPods de rafa"))),
      h("div", { class: "cel-campo" }, rotulo(a.teste), h("div", { class: "cel-medidor" }, h("span", { style: { width: "42%" } }))),
      h("div", { class: "cel-campo" }, rotulo(a.saida), h("p", { class: "cel-nota", text: a.saidaNota })),
      h("div", { class: "cel-campo" }, rotulo(a.tratamento), h("p", { class: "cel-nota", text: a.tratamentoNota }))];
  } else if (id === "computador") {
    corpo = [titulo("NO COMPUTADOR"),
      h("ul", { class: "cel-no-computador" }, f.noComputador.map(([nome, frase]) => h("li", {}, h("strong", { text: nome }), h("p", { class: "cel-nota", text: frase }))))];
  } else if (id === "servidor") {
    corpo = [titulo("ESTE SERVIDOR"),
      h("ul", { class: "cel-lista" },
        h("li", {}, h("button", { class: "cel-ajuste", type: "button", onclick: () => mudar({ mod: "dialogo" }) },
          glifo("aparencia"), h("span", {}, f.estilo.titulo, h("span", { class: "cel-linha-sub", text: "DO MOD ESTILO · quem administra edita" })), h("span", { class: "cel-seta" }, glifo("avancar")))),
        // A saída do tema é do núcleo: esta linha é o caminho para desligar um
        // MOD e para «o SEELE desenha», e o tema daquele MOD não a alcança.
        h("li", {}, h("button", { id: "linha-mods", class: "cel-ajuste cel-nucleo", type: "button" },
          glifo("engrenagem"), h("span", {}, "MODS DESTE SERVIDOR", h("span", { class: "cel-linha-sub", text: "PERFIS, MESA, ESTILO · quem desenha cada lugar" })), h("span", { class: "cel-seta" }, glifo("avancar"))))),
      h("p", { class: "cel-fora" }, rotulo("NOME E IMAGEM"), f.noComputador[5][1])];
  } else if (id === "conexao") {
    const t = QUADRO.telemetry;
    const linha = (r, v) => [h("dt", {}, rotulo(r)), h("dd", { text: v })];
    corpo = [h("dl", { class: "cel-ficha" },
      linha("ATRASO", `${t.rtt_ms} ms`), linha("JITTER", `${t.jitter_ms} ms`),
      linha("PERDA", `${(t.loss_fraction * 100).toFixed(1).replace(".", ",")}%`), linha("CODEC", "OPUS 48k"),
      linha("CAMINHO", "DIRETO · IPV6"), linha("UPTIME", "01:12:44"), linha("SINAL DA SALA", estado.sala ? String(t.signal) : "fora de sala de voz")),
      h("p", { class: "cel-nota", style: { marginTop: "12px" }, text: "Na sala de voz, as mesmas medidas ficam também na fileira de baixo da chamada." })];
  } else {
    corpo = [];
  }
  return h("div", { class: "cel-painel", role: "tabpanel", "aria-label": tituloDaSecao(id) }, corpo);
}

function paginaDeMod() {
  return h("div", { class: "cel-painel cel-painel-cheio", role: "tabpanel" },
    h("div", { class: "cel-superficie-cabeca" },
      h("div", { class: "cel-superficie-titulo" }, h("span", { class: "cel-de-mod", style: { display: "block" }, text: "DO MOD MESA" }),
        h("span", { class: "cel-titulo cel-titulo-medio", text: "ILHA SOMBRIA" })),
      h("button", { class: "botao-fantasma", type: "button", onclick: () => mudar({ mod: null }) }, FRASES_DO_CELULAR.fechar)),
    h("div", { class: "cel-superficie-corpo" },
      h("p", { class: "cel-nota", text: "Sessão 12 · quinta, 21h. Fichas, mapa e diário, desenhados pela MESA dentro do lugar da conversa. A faixa de voz e as abas continuam por cima de qualquer página." })));
}

// --------------------------------------------------------- a faixa de voz

function botaoMudo(compacto = false) {
  const f = FRASES_DO_CELULAR.voz;
  if (estado.modo === "segurar") {
    const soltar = () => { if (estado.apertado) mudar({ apertado: false }); };
    return h("button", {
      id: compacto ? null : "voz-mudo", class: "voz-nucleo", type: "button", "data-modo": "segurar",
      "aria-pressed": String(estado.apertado), "aria-label": "Segurar para falar",
      onpointerdown: (e) => { e.preventDefault(); mudar({ apertado: true }); },
      onpointerup: soltar, onpointerleave: soltar, onpointercancel: soltar, ontouchcancel: soltar,
    }, glifoComEstado("microfone", !estado.apertado), estado.apertado ? f.noAr : f.segure);
  }
  return h("button", {
    id: compacto ? null : "voz-mudo", class: "voz-nucleo", type: "button",
    "aria-pressed": String(estado.mudo), "aria-label": estado.mudo ? "Microfone fechado. Tocar abre." : "Microfone aberto. Tocar fecha.",
    onclick: () => mudar({ mudo: !estado.mudo }),
  }, glifoComEstado("microfone", estado.mudo), estado.mudo ? f.mudo : f.mic);
}

function controlesDoNucleo() {
  const f = FRASES_DO_CELULAR.voz;
  return h("div", { class: "voz-controles" },
    h("button", { id: "voz-sair", class: "voz-nucleo", type: "button", "aria-label": "Sair da sala de voz",
                  onclick: () => mudar({ sala: null, chamada: false }) }, glifo("sair"), f.sair),
    h("button", { id: "voz-surdo", class: "voz-nucleo", type: "button", "aria-pressed": String(estado.surdo),
                  "aria-label": estado.surdo ? "Som desligado. Tocar liga." : "Som ligado. Tocar desliga.",
                  onclick: () => mudar({ surdo: !estado.surdo }) },
      glifoComEstado("fone", estado.surdo), estado.surdo ? f.surdo : f.som),
    botaoMudo());
}

function faixaDeVoz({ naChamada = false } = {}) {
  const sala = salaAtual();
  if (!sala) return null;
  const f = FRASES_DO_CELULAR.voz;
  const quemFala = sala.people.find((p) => p.speaking && !p.is_self);
  const sub = estado.queda ? f.reconectando
    : quemFala ? [h("span", { class: "cel-falando" }, glifo("falando"), ` ${f.falando(NOMES_DE_MOD[quemFala.id] ?? quemFala.nickname)}`), ` · sinal ${QUADRO.telemetry.signal}`]
    : `${f.naSala(sala.people.length)} · sinal ${QUADRO.telemetry.signal}`;
  return h("section", { id: "voz", class: "voz-celular", "aria-label": `Sala de voz ${sala.name}` },
    h("button", {
      class: "voz-sala", type: "button",
      "aria-label": naChamada ? f.recolher : `${sala.name}: abrir a chamada`,
      onclick: () => mudar({ chamada: !naChamada }),
    },
      h("span", { class: "voz-sala-nome" }, naChamada ? [glifo("aberto"), ` ${f.recolher}`] : [sala.name, " ", h("span", { class: "cel-dado", text: `${sala.people.length}/${sala.limit}` })]),
      h("span", { class: "voz-sala-quem" }, sub)),
    controlesDoNucleo());
}

// ------------------------------------------------------------- a chamada

function chamada() {
  const sala = salaAtual();
  const f = FRASES_DO_CELULAR.chamada;
  const t = QUADRO.telemetry;
  const transmissao = QUADRO.transmissoes[0];
  return h("section", { class: "cel-chamada", "aria-label": `${f.titulo} ${sala.name}` },
    h("header", { class: "cel-topo" },
      h("div", { class: "cel-topo-texto" },
        h("p", { class: "cel-topo-sobre" }, `${f.titulo} · ${QUADRO.server} · `, h("span", { id: "selo-seguro-chamada", class: "cel-selo-seguro", "data-estado": "segura", text: "CONEXÃO SEGURA" })),
        h("h1", { class: "cel-titulo cel-titulo-grande" }, `${sala.name} `, h("span", { class: "cel-dado", text: `${sala.people.length}/${sala.limit}` }))),
      h("div", { class: "cel-segmento", role: "group", "aria-label": "vista da chamada" },
        h("button", { type: "button", "aria-pressed": "true" }, f.grade),
        h("button", { type: "button", "aria-pressed": "false" }, f.palcoDeMod))),
    h("div", { class: "cel-grade-rolagem" },
      transmissao && palco(transmissao),
      h("ol", { class: "cel-grade", "aria-label": "pessoas na sala de voz" }, sala.people.map((p) => h("li", {
        class: "cel-cartao", "data-falando": p.speaking ? "" : null, tabindex: "0",
        onclick: () => mudar({ folha: `pessoa:${p.id}` }),
      },
        CAPAS[p.id] && h("div", { class: "cel-capa", "data-capa": CAPAS[p.id], "aria-hidden": "true" }),
        avatar(p),
        h("div", { class: "cel-cartao-placa" },
          h("div", { class: "cel-cartao-linha" }, h("span", {}, nomeComVerificado(p, "cel-msg-nome")),
            h("span", { class: "cel-sinal-numero", text: String(p.signal) })),
          h("div", { class: "cel-cartao-linha" },
            p.speaking ? h("span", { class: "cel-pastilha", text: "FALANDO" })
              : p.muted ? h("span", { class: "cel-pastilha cel-pastilha-mudo", text: "MUDO" })
              : h("span", { class: "cel-apagado cel-dado", text: "OUVINDO" }),
            h("span", { class: "cel-sinal", "data-faixa": p.sync_band }, h("span", { class: "cel-sinal-barra", "aria-hidden": "true", text: barra(p.signal) }))))))),
      h("p", { class: "cel-fora" }, rotulo("COMPARTILHAR A TELA"), f.semCompartilhar)),
    faixaDeVoz({ naChamada: true }),
    h("div", { class: "cel-telemetria", role: "group", "aria-label": "telemetria da sala" },
      [[f.telemetria.atraso, `${t.rtt_ms}ms`], [f.telemetria.jitter, `${t.jitter_ms}ms`],
       [f.telemetria.perda, `${(t.loss_fraction * 100).toFixed(1).replace(".", ",")}%`], [f.telemetria.codec, "OPUS 48k"]]
        .map(([r, v]) => h("div", {}, h("span", { class: "rotulo", text: r }), h("span", { text: v })))));
}

// ---------------------------------------------------------------- o palco
//
// A tela compartilhada, assistida com som (D-celular-2). A fileira escolhe qual
// transmissão se assiste, uma por vez, cada uma pela identidade dela (ADR 0054);
// SILENCIAR e VOLUME são do som da tela, e não das pessoas.

function palco(transmissao) {
  const f = FRASES_DO_CELULAR.chamada;
  const naoAbre = parametros.get("palco") === "nao-abre";
  return h("section", { class: "cel-palco-bloco", "aria-label": `tela de ${transmissao.nickname}` },
    h("div", { class: "cel-palco-escolha", role: "group", "aria-label": f.escolha, "data-rola-de-lado": "" },
      QUADRO.transmissoes.map((tr, i) => h("button", { type: "button", class: "cel-rede", "aria-pressed": String(i === 0) },
        h("span", { class: "cel-pastilha", text: f.transmitindo }), tr.nickname)),
      h("button", { type: "button", class: "cel-rede", "aria-pressed": "false" }, f.naoVer)),
    naoAbre
      ? h("div", { class: "cel-palco cel-palco-nao-abre", role: "status" },
          h("p", { class: "cel-titulo cel-titulo-medio", text: f.naoAbre }),
          h("p", { class: "cel-nota", text: f.naoAbreNota }))
      : h("figure", { class: "cel-palco", style: { margin: "0" } },
          h("div", { class: "cel-capa", "data-capa": "c", style: { position: "absolute", inset: "0", opacity: "0.35" }, "aria-hidden": "true" }),
          h("figcaption", { class: "cel-palco-legenda" }, h("span", { class: "cel-pastilha", text: f.transmitindo }), h("span", { class: "cel-dado", text: transmissao.nickname })),
          h("button", { class: "botao-fantasma cel-palco-cheia", type: "button" }, f.telaCheia)),
    !naoAbre && h("div", { class: "cel-palco-som" },
      h("button", { class: "botao-fantasma", type: "button", "aria-pressed": "false" }, f.silenciar),
      h("label", { class: "cel-palco-volume" }, h("span", { class: "rotulo", text: `${f.volumeDaTela} · 100%` }),
        h("input", { type: "range", min: "0", max: "200", value: "100" }))),
    !naoAbre && h("p", { class: "cel-nota cel-apagado", text: f.silenciarNota }),
    h("button", { class: "botao-fantasma cel-largo", type: "button", style: { marginTop: "8px" } }, f.pararDeAssistir));
}

// ------------------------------------------------------------- as folhas

function folha(titulo, corpo, { confirmacao = false } = {}) {
  return [
    h("div", { class: "cel-veu", "aria-hidden": "true", onclick: () => mudar({ folha: null }) }),
    h("section", { class: "cel-folha", role: "dialog", "aria-modal": "true", "aria-label": titulo, "data-confirmacao": confirmacao ? "" : null },
      h("header", { class: "cel-folha-cabeca" },
        h("h2", { class: "cel-titulo cel-titulo-medio", style: { flex: "1" }, text: titulo }),
        h("button", { class: "botao-fantasma", type: "button", onclick: () => mudar({ folha: null }) }, FRASES_DO_CELULAR.fechar)),
      h("div", { class: "cel-folha-corpo" }, corpo)),
  ];
}

function folhaAtual() {
  if (!estado.folha) return null;
  const [tipo, arg] = estado.folha.split(":");
  if (tipo === "pessoa") {
    const p = pessoa(Number(arg)) ?? pessoa(2);
    const f = FRASES_DO_CELULAR.folhaDaPessoa;
    return folha(NOMES_DE_MOD[p.id] ?? p.nickname, [
      h("div", { class: "cel-placa", style: { marginBottom: "12px" } }, avatar(p),
        h("span", {}, nomeComVerificado(p, "cel-msg-nome"), h("span", { class: "cel-linha-sub", text: "CHAVE 3C1D·A8F0·77B2" }))),
      h("div", { class: "cel-campo" }, rotulo(f.sinal),
        h("span", { class: "cel-sinal", "data-faixa": p.sync_band, style: { justifyItems: "start" } },
          h("span", { class: "cel-sinal-numero", text: `${p.signal} · ${FRASES_DO_CELULAR.pessoas.faixas[p.sync_band]}` }),
          h("span", { class: "cel-sinal-barra", text: barra(p.signal) }))),
      h("label", { class: "cel-campo" }, h("span", { class: "rotulo" }, `${f.volume} · 100%`), h("input", { type: "range", min: "0", max: "200", value: "100" })),
      h("div", { class: "cel-folha-acoes" },
        h("button", { class: "botao-fantasma cel-largo", type: "button", onclick: () => mudar({ folha: null, mod: "pagina", aba: "conversa" }) }, f.perfil)),
      QUADRO.may_kick && !p.is_self && h("p", { class: "cel-fora" }, rotulo(f.moderar), f.moderarFora),
    ]);
  }
  if (tipo === "mensagem") {
    const m = MENSAGENS.find((x) => x.id === Number(arg)) ?? MENSAGENS[0];
    return folha("MENSAGEM", [
      h("p", { class: "cel-nota", text: `${m.author_nickname} · ${m.at}` }),
      h("p", { class: "cel-msg-corpo", style: { margin: "8px 0" }, text: m.body }),
      h("div", { class: "cel-folha-acoes" },
        h("button", { class: "botao-fantasma cel-largo", type: "button" }, "COPIAR O TEXTO"),
        m.own && h("button", { class: "botao-fantasma cel-largo", type: "button" }, "APAGAR A MINHA MENSAGEM")),
      !m.own && QUADRO.may_kick && h("p", { class: "cel-fora" }, rotulo("REMOVER"), "Remover a mensagem de outra pessoa é pelo SEELE no computador."),
    ]);
  }
  if (tipo === "anexar") {
    return folha("ANEXAR", [
      h("div", { class: "cel-folha-acoes" },
        h("button", { class: "botao cel-largo", type: "button" }, "FOTO OU VÍDEO"),
        h("button", { class: "botao-fantasma cel-largo", type: "button" }, "ARQUIVO")),
      h("div", { class: "cel-secao-titulo" }, rotulo("DOS MODS")),
      h("div", { class: "cel-folha-acoes" },
        h("button", { class: "botao-fantasma cel-largo", type: "button" }, "ROLAR DADOS", h("span", { class: "cel-de-mod", text: " · MESA" }))),
    ]);
  }
  if (tipo === "senha") {
    return folha("SALA COM SENHA", [
      h("p", { class: "cel-nota", text: "A sala COFRE pede a senha que quem administra definiu." }),
      h("label", { class: "cel-campo" }, rotulo("SENHA DA SALA"), h("input", { type: "password", autocomplete: "off" })),
      h("div", { class: "cel-folha-acoes" }, h("button", { class: "botao cel-largo", type: "button", onclick: () => mudar({ folha: null, sala: 3, chamada: true }) }, "ENTRAR NA SALA")),
    ]);
  }
  if (tipo === "perfis") {
    return folha("EDITAR CARTÃO", [
      h("span", { class: "cel-de-mod", text: "DO MOD PERFIS" }),
      h("label", { class: "cel-campo" }, rotulo("FRASE"), h("input", { type: "text", value: "mestre de quinta-feira" })),
      h("div", { class: "cel-folha-acoes" }, h("button", { class: "botao cel-largo", type: "button" }, "GRAVAR")),
    ]);
  }
  if (tipo === "confirmar") {
    return folha("SAIR DESTE SERVIDOR?", [
      h("p", { class: "cel-nota", text: "Você sai da sala PONTE e entra em OFICINA. A conversa daqui continua guardada." }),
      h("div", { class: "cel-folha-acoes" },
        h("button", { class: "botao cel-largo", type: "button" }, "TROCAR DE SERVIDOR"),
        h("button", { class: "botao-fantasma cel-largo", type: "button", onclick: () => mudar({ folha: null }) }, "FICAR")),
    ], { confirmacao: true });
  }
  return null;
}

function dialogoDoEstilo() {
  const f = FRASES_DO_CELULAR.estilo;
  const valores = TEMAS.profundo;
  const cores = [["FUNDO", valores.fundo], ["PAINEL", valores.painel], ["TEXTO", valores.texto],
                 ["TEXTO APAGADO", valores.apagado], ["DESTAQUE", valores.acento], ["BORDA", valores.borda]];
  return h("section", { class: "cel-dialogo", role: "dialog", "aria-modal": "true", "aria-label": f.titulo },
    h("header", { class: "cel-superficie-cabeca" },
      h("div", { class: "cel-superficie-titulo" }, h("span", { class: "cel-de-mod", style: { display: "block" }, text: f.de }),
        h("span", { class: "cel-titulo cel-titulo-medio", text: f.titulo })),
      h("button", { class: "botao-fantasma", type: "button", onclick: () => mudar({ mod: null }) }, FRASES_DO_CELULAR.fechar)),
    h("div", { class: "cel-superficie-corpo", style: { overflowY: "auto" } },
      h("div", { class: "cel-segmento", role: "tablist" }, f.abas.map((a, i) => h("button", { type: "button", role: "tab", "aria-pressed": String(i === 0) }, a))),
      h("p", { class: "cel-nota", style: { margin: "12px 0" }, text: "Seis cores, e o que decide se a conversa se lê é a relação entre elas." }),
      h("div", { class: "cel-cores" }, cores.map(([nome, valor]) => h("label", { class: "cel-cor" },
        // A amostra é o valor escolhido por quem administra: dado, e não cor do produto.
        h("span", { class: "cel-cor-amostra", style: { background: valor } }),
        h("span", {}, rotulo(nome), h("span", { class: "cel-linha-sub", text: valor })),
        h("span", { class: "cel-seta" }, glifo("avancar")))))),
    h("div", { class: "cel-dialogo-acoes" },
      h("button", { class: "botao-fantasma", type: "button", onclick: () => mudar({ mod: null, previa: true, tema: "profundo", sub: null, aba: "conversa" }) }, f.ver),
      h("button", { class: "botao", type: "button" }, f.publicar),
      h("button", { class: "botao-fantasma cel-largo", type: "button" }, f.restaurar)));
}

function seloDaPrevia() {
  const f = FRASES_DO_CELULAR.estilo;
  return h("div", { class: "cel-selo-previa", role: "status" },
    h("span", { class: "cel-dado", style: { flex: "1" }, text: f.selo }),
    h("button", { class: "botao-fantasma", type: "button", onclick: () => mudar({ previa: false, mod: "dialogo" }) }, f.voltar));
}

function faixaDaQueda() {
  const f = FRASES_DO_CELULAR.queda;
  return h("section", { class: "cel-queda", role: "alert" },
    h("p", { class: "cel-titulo cel-titulo-grande", text: f.titulo }),
    h("p", { class: "cel-nota" }, h("span", { class: "cel-queda-conta", text: "4:32" }), ` ${f.conta("").trim()}`),
    h("p", { class: "cel-nota cel-apagado", text: `${f.tentativa(3)} · ${FRASES?.SemResposta?.split("\n")[0] ?? "OS PACOTES SAÍRAM E NADA VOLTOU"}` }),
    h("button", { class: "botao-fantasma", type: "button", style: { marginTop: "8px" } }, f.agora));
}

function avisoDeMod() {
  return h("div", { class: "cel-avisos", style: { bottom: estado.sala ? "calc(var(--cel-faixa) + var(--cel-abas) + var(--cel-seguro-pe) + 8px)" : "calc(var(--cel-abas) + var(--cel-seguro-pe) + 8px)" } },
    h("p", { class: "cel-aviso-de-mod", role: "status" }, h("span", { class: "cel-de-mod", text: "PERFIS · " }), "cartão gravado"));
}

function telaSessao() {
  const f = FRASES_DO_CELULAR.abas;
  const abas = ["servidor", "conversa", "pessoas", "ajustes"];
  const canal = QUADRO.channels.find((c) => c.id === estado.canal);
  const topoDaAba = {
    servidor: () => topo(null, h("h1", { class: "cel-titulo cel-titulo-grande", text: QUADRO.server })),
    // Com uma página de MOD no lugar da conversa, o topo diz a página, e não o
    // canal que ficou por baixo dela.
    conversa: () => estado.mod === "pagina"
      ? topo(null, h("h1", { class: "cel-titulo cel-titulo-grande", text: "CAMPANHA" }))
      : topo(null, h("h1", { class: "cel-topo-canal" }, h("span", { class: "cel-cerquilha", text: "# " }), canal.name),
      estado.teclado ? vozRecolhida() : h("button", { class: "cel-topo-acao", type: "button", "aria-label": "buscar no canal" }, glifo("buscar"))),
    pessoas: () => topo(null, h("h1", { class: "cel-titulo cel-titulo-grande" }, "PESSOAS ",
      h("span", { class: "cel-dado", text: `${QUADRO.voice_rooms.reduce((n, s) => n + s.people.length, 0) + QUADRO.presentes_sem_sala.length}` }))),
    // Numa seção, o topo leva a volta e o nome dela: «AJUSTES» não se repete.
    ajustes: () => estado.sub
      ? topo(null, h("div", {},
          h("button", { class: "cel-voltar", type: "button", onclick: () => mudar({ sub: null }) }, glifo("recuar"), FRASES_DO_CELULAR.ajustes.voltar),
          h("h1", { class: "cel-titulo cel-titulo-grande", text: tituloDaSecao(estado.sub) })))
      : topo(null, h("h1", { class: "cel-titulo cel-titulo-grande", text: "AJUSTES" })),
  }[estado.aba];
  const painel = { servidor: painelServidor, conversa: painelConversa, pessoas: painelPessoas, ajustes: painelAjustes }[estado.aba];

  const sessao = h("section", {
    id: "tela-sessao", class: "tela", "data-aba": estado.aba,
    "data-teclado": estado.teclado ? "" : null, "data-queda": estado.queda ? "" : null,
  },
    topoDaAba(),
    painel(),
    // A faixa de baixo: o selo da prévia (quando há) em cima da faixa de voz, e
    // nunca no lugar dela — a voz continua à mão durante a prévia.
    h("div", { class: "cel-pe-da-sessao" }, estado.previa && seloDaPrevia(), !estado.teclado && faixaDeVoz()),
    h("nav", { class: "cel-abas", "aria-label": "seções" }, abas.map((a) => h("button", {
      class: "cel-aba", type: "button", "aria-current": estado.aba === a ? "page" : null,
      onclick: () => mudar({ aba: a, sub: null, mod: estado.mod === "pagina" ? null : estado.mod, teclado: false }),
      // A aba AJUSTES é o caminho para desligar um MOD (núcleo): ela lê as
      // cores do produto, e o tema não a alcança.
      id: `cel-aba-${a}`, "data-nucleo": a === "ajustes" ? "" : null,
    }, f[a], a === "conversa" && h("span", { class: "cel-aba-conta", "aria-label": "15 novas em outros canais", text: "15" })))),
    estado.chamada && estado.sala && chamada(),
    estado.queda && faixaDaQueda(),
    estado.mod === "aviso" && avisoDeMod(),
    // **O palco dos MODs fica acima da faixa de voz e das abas.** Diálogo,
    // folha e página imersiva de MOD moram aqui, e nenhuma delas cobre o MUDO,
    // o SAIR nem o caminho para AJUSTES (núcleo, §10 do rascunho da API 6).
    h("div", { class: "cel-palco-dos-mods" }, estado.mod === "dialogo" && dialogoDoEstilo()),
    folhaAtual());
  aplicarTema(sessao);
  return sessao;
}

// --------------------------------------------------------------- o todo

function desenharTudo() {
  const telas = { inicio: telaInicio, entrar: telaEntrar, conectando: telaConectando, aceite: telaAceite, sessao: telaSessao, fim: telaFim };
  document.getElementById("app").replaceChildren((telas[estado.tela] ?? telaInicio)());
}

// SEGURAR PARA FALAR fecha sozinho com a página oculta — a exigência da
// orquestradora: o microfone nunca fica preso aberto.
document.addEventListener("visibilitychange", () => {
  if (document.hidden && estado.apertado) mudar({ apertado: false });
});

(async () => {
  document.documentElement.style.setProperty("--cel-escala", String(medirTextoDoSistema()));
  await carregarSimbolo();
  desenharTudo();
  document.documentElement.dataset.pronto = "sim";
})();
