// **A área útil das telas de hospedar e configurar**, medida com o código de verdade (0.17).
//
// Requisito do dono: nada de informação aglomerada numa coluna estreita no centro; a área útil inteira, no desktop e no navegador do celular,
// com o parágrafo na medida de leitura DENTRO da grade. Esta bancada mede isso nas três telas do escopo — `#tela-preparar`,
// `#tela-antes-de-hospedar` e `#tela-server` — com as MESMAS cenas e o mesmo duble do Tauri da `navegacao-0-17.cjs` (a UI real, nenhum
// mockup desligado). Ela não prova comportamento do produto: diz se a tela usa a área, se cabe e se tudo é alcançável.
//
//   PLAYWRIGHT=<caminho> node apps/seele-app/bancada/area-util.cjs <pasta> [--antes] [--webkit] [--so=<trecho>]
//
// Escreve `<pasta>/<tela>-<L>x<A>[-zoom200][-antes][-webkit].png` e `<pasta>/medidas[-antes].json`. Por tela × tamanho × motor:
//   - sobrefluxo horizontal (scrollWidth > clientWidth) da página, da tela e de cada rolagem interna;
//   - a parcela da área ocupada (a caixa de conteúdo contra a janela, em largura e em área);
//   - a ordem do foco (a sequência do Tab) e os controles visíveis que o Tab NÃO alcançou;
//   - cada controle alcançável: `elementFromPoint` no centro devolve o controle (ou um filho dele), e não a barra ou outra coisa;
//   - a barra de ações não cobre o último conteúdo quando a rolagem vai ao fim;
//   - com `prefers-reduced-motion: reduce` emulado, nenhuma animação nem transição em curso na tela.
// Sai com código 1 se alguma medida reprova a regra (a regra mora em `veredito`, abaixo).

const fs = require("node:fs");
const path = require("node:path");
const { servir } = require("./telas.cjs");
const { CENAS } = require("./navegacao-0-17.cjs");

const TAMANHOS = [
  { w: 1440, h: 900 }, { w: 1024, h: 768 }, { w: 390, h: 844, toque: true }, { w: 320, h: 568, toque: true },
  { w: 844, h: 390, toque: true }, { w: 820, h: 1180, toque: true, tablet: true }, { w: 1024, h: 768, toque: true, tablet: true, nome: "1024x768-tablet" },
  { w: 720, h: 450, zoom: 2, nome: "1440x900-zoom200" }, { w: 512, h: 384, zoom: 2, nome: "1024x768-zoom200" },
];

// Cada tela: a cena da `navegacao-0-17.cjs` que a abre, o seletor da raiz, o da barra de ações e o da caixa de conteúdo.
const TELAS = [
  { tela: "servidores-lista", cena: "app-hospedar-lista", raiz: "#tela-servidores", barra: "#servidores-novo", caixa: "#tela-servidores .boot-caixa" },
  { tela: "preparar-novo", cena: "app-hospedar-preparar-novo", raiz: "#tela-preparar", barra: "#preparar-confirmar", caixa: "#tela-preparar .boot-caixa" },
  { tela: "preparar-adotado", cena: "app-hospedar-preparar-adotado", raiz: "#tela-preparar", barra: "#preparar-confirmar", caixa: "#tela-preparar .boot-caixa" },
  { tela: "antes-de-hospedar", cena: "app-hospedar-legado-mods", raiz: "#tela-antes-de-hospedar", barra: "#antes-hospedar", caixa: "#tela-antes-de-hospedar .antes-caixa" },
  { tela: "antes-de-hospedar-resolvidos", cena: "app-hospedar-legado-mods-resolvidos", raiz: "#tela-antes-de-hospedar", barra: "#antes-hospedar", caixa: "#tela-antes-de-hospedar .antes-caixa" },
  { tela: "config-audio", cena: "app-config-audio", raiz: "#tela-server", barra: null, caixa: "#tela-server .server" },
  { tela: "config-mods", cena: "app-config-mods", raiz: "#tela-server", barra: null, caixa: "#tela-server .server" },
  // O ÍNDICE das seções (a janela estreita do app abre aqui) e a seção aberta inteira com o botão SEÇÕES.
  { tela: "config-indice", raiz: "#tela-server", barra: null, caixa: "#tela-server .server", so_estreito: true,
    cenaPropria: { nome: "config-indice", casca: "app", passos: `abrirServer("tela-boot")` } },
  // As telas do CELULAR de verdade (a casca `celular` e a `web` com a ponte web), e não o app com a janela estreita: é o que o PWA mostra.
  { tela: "celular-entrada", raiz: ".cel-entrada", barra: null, caixa: ".cel-entrada", so_celular: true,
    cenaPropria: { nome: "celular-entrada", casca: "web" } },
  // A entrada do celular com servidores JÁ guardados (a lista de «conhecidos» da identidade da página, por um stub dela).
  { tela: "celular-entrada-servidores", raiz: ".cel-entrada", barra: null, caixa: ".cel-entrada", so_celular: true,
    cenaPropria: { nome: "celular-entrada-servidores", casca: "web", espera: 900,
      passos: `await CELULAR.ajudante("entrada").usar({ ler: () => null, abrir: async () => null },
        { conhecidos: async () => [
          { endereco: "lia.exemplo.br:8383", nome: "Oficina da Lia", apelido: "Alex", impressao: "ab".repeat(32), versaoDoServidor: "2.0", visto: Date.now() - 86400000, podeEntrar: true },
          { endereco: "203.0.113.9:8383", nome: "Biblioteca Norte", apelido: "alex", impressao: "cd".repeat(32), versaoDoServidor: "2.0", visto: Date.now() - 600000, podeEntrar: true },
          { endereco: "198.51.100.4:8383", nome: "", apelido: "alex", impressao: "ef".repeat(32), versaoDoServidor: "1.0", visto: Date.now() - 9000000, podeEntrar: false },
        ], entrarNoConhecido: async () => {} });` } },
  { tela: "celular-sessao", raiz: "#tela-sessao", barra: null, caixa: "#tela-sessao", so_celular: true,
    cenaPropria: { nome: "celular-sessao", casca: "celular", sessao: true } },
  { tela: "celular-ajustes", raiz: "#tela-sessao", barra: null, caixa: "#tela-sessao", so_celular: true,
    cenaPropria: { nome: "celular-ajustes", casca: "celular", sessao: true, passos: `document.getElementById("tela-sessao").dataset.aba = "ajustes";`, espera: 800 } },
  // Os diálogos e camadas que ocupam a janela, ou que têm de continuar legíveis como diálogo (cenas ESCRITAS; a primeira corrida da bancada as confere).
  { tela: "perfil-com-agenda", raiz: "#perfil", barra: null, caixa: "#perfil .perfil-caixa", largo: true,
    cenaPropria: { nome: "perfil-com-agenda", casca: "app", espera: 900,
      resposta: {
        meu_codigo: "ALEX-EXMP-9999-ZZZZ", estado_da_sala: { estado: "NoAr", registro: { registro: "Registrado" }, firewall: null },
        contatos_da_agenda: Array.from({ length: 12 }, (_, i) => ({ codigo: `AGE${String(i).padStart(2, "0")}-EXMP-00${i}1-AAAA`, nome: i % 3 ? `Contato ${i}` : null, apelido: i % 3 ? null : `amigo${i}`,
          visto_em: Math.floor(Date.now() / 1000) - 3600 * (i + 1), direcao: i % 2 ? "recebi" : "liguei", bloqueado: i === 5, amigo: i % 4 === 0 })),
      },
      passos: `abrirPerfil()` } },
  { tela: "cargos", raiz: "#cargos", barra: null, caixa: "#cargos .cargos-caixa", largo: true,
    cenaPropria: { nome: "cargos", casca: "app", celularNoToque: true, sessao: true, espera: 1200,
      resposta: { pedir_cargos: { quadro: { revisao: 1, combinacao: "restringe_vence", proprietario: 1, eu: { pessoa: 1, proprietario: true },
        cargos: [
          { id: 0, nome: "@everyone", posicao: 0, todos: true, concede: ["send_message"] },
          { id: 1, nome: "Convidado", posicao: 1 }, { id: 2, nome: "Membro", posicao: 2, concede: ["send_message", "speak"] },
          { id: 3, nome: "Moderador", posicao: 3, cor: "azul", concede: ["kick"] }, { id: 4, nome: "Administrador", posicao: 4, cor: "laranja" },
          { id: 5, nome: "Narrador", posicao: 2, cor: "verde" }, { id: 6, nome: "Observador", posicao: 1, restringe: ["send_message"] },
        ],
        atribuicoes: [{ pessoa: 1, cargos: [4] }, { pessoa: 2, cargos: [3, 5] }], apelidos: [] } } },
      passos: `abrirCargos()` } },
  { tela: "cargos-selecionado", raiz: "#cargos", barra: null, caixa: "#cargos .cargos-caixa", largo: true,
    cenaPropria: { nome: "cargos-selecionado", casca: "app", celularNoToque: true, sessao: true, espera: 1200,
      resposta: { pedir_cargos: { quadro: { revisao: 1, combinacao: "restringe_vence", proprietario: 1, eu: { pessoa: 1, proprietario: true },
        cargos: [
          { id: 0, nome: "@everyone", posicao: 0, todos: true, concede: ["send_message"] },
          { id: 1, nome: "Convidado", posicao: 1 }, { id: 2, nome: "Membro", posicao: 2, concede: ["send_message", "speak"] },
          { id: 3, nome: "Moderador", posicao: 3, cor: "azul", concede: ["kick"] }, { id: 4, nome: "Administrador", posicao: 4, cor: "laranja" },
          { id: 5, nome: "Narrador", posicao: 2, cor: "verde" }, { id: 6, nome: "Observador", posicao: 1, restringe: ["send_message"] },
        ],
        atribuicoes: [{ pessoa: 1, cargos: [4] }, { pessoa: 2, cargos: [3, 5] }], apelidos: [] } } },
      passos: `abrirCargos(); await new Promise((r) => setTimeout(r, 900)); document.querySelector('#cargos-lista [data-cargo="3"]').click(); await new Promise((r) => setTimeout(r, 700));` } },
  // A HOME (a entrada do app) na largura total: hoje há um override, e ela não era medida.
  { tela: "home-desktop", raiz: "#tela-boot", barra: null, caixa: "#tela-boot .boot-caixa", largo: true,
    cenaPropria: { nome: "home-desktop", casca: "app" } },
  // O aceite de MODs com TRÊS itens: fica diálogo, e tem de continuar legível, sem corte (a caixa cabe na janela e o corpo rola).
  { tela: "aceite-de-mods-3-itens", raiz: "#mods-aceite", barra: null, caixa: "#mods-aceite .mods-caixa", dialogo: true,
    cenaPropria: { nome: "aceite-de-mods-3-itens", casca: "app", espera: 900,
      passos: `abrirAceiteDeMods("lia.exemplo.br:8383", [
        { id: "lia/mesa", version: "1.2.0", hash: "h1a2b3c4d5e6", repo: "https://github.com/lia/mesa", api: 6 },
        { id: "lia/dados", version: "0.4.1", hash: "h2f6e5d4c3b2", repo: null, api: 7 },
        { id: "lia/estilo-do-servidor-com-um-nome-bem-longo", version: "3.0.0", hash: "h3a4b5c6d7e8", repo: "https://github.com/lia/estilo", api: 7 }], "c1", null)` } },
  // Os avisos do pé da sessão no celular: o primeiro contato, uma falha de áudio e a identidade juntos (a captura do PWA no simulador: ~40% da altura).
  { tela: "celular-avisos", raiz: "#tela-sessao", barra: null, caixa: "#tela-sessao", so_celular: true,
    cenaPropria: {
      nome: "celular-avisos", casca: "celular", sessao: true,
      passos: `mostrarVeredito({ FirstContactVerified: { fingerprint: "0a7311e23a170100f10f39a54da69431b106146a008868bb3674c8150fce5c87" } });
        document.getElementById("aviso-audio").hidden = false;
        document.getElementById("aviso-audio-texto").textContent = "Microfone bloqueado. Revise a permissão.";
        const p = document.getElementById("cel-aviso-persistencia");
        p.dataset.resposta = "negada"; p.hidden = false;
        p.replaceChildren(Object.assign(document.createElement("span"), { className: "veredito-texto", textContent: "Não foi possível guardar a identidade neste aparelho." }),
          Object.assign(document.createElement("button"), { className: "botao-fantasma", type: "button", textContent: "dispensar" }));` } },
];

/** O que roda NA PÁGINA: mede a tela já aberta. Devolve um objeto serializável. */
function medirNaPagina({ raiz, barra, caixa }) {
  const q = (s) => document.querySelector(s);
  const visivel = (e) => {
    const r = e.getBoundingClientRect();
    const cs = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && cs.visibility !== "hidden" && cs.display !== "none" && !e.closest("[hidden]") && !e.closest("details:not([open]) > :not(summary)");
  };
  const tela = q(raiz);
  const vw = innerWidth, vh = innerHeight;
  const medida = { janela: `${vw}x${vh}`, tela_visivel: Boolean(tela) && !tela.hidden && visivel(tela) };
  if (!medida.tela_visivel) return medida;
  // sobrefluxo horizontal
  const de = document.documentElement;
  const rolagens = [tela, ...tela.querySelectorAll("*")].filter((e) => {
    const o = getComputedStyle(e).overflowX;
    return (o === "auto" || o === "scroll" || o === "hidden") && e.scrollWidth > e.clientWidth + 1;
  });
  medida.sobrefluxo_horizontal = {
    pagina: de.scrollWidth > de.clientWidth + 1,
    tela: tela.scrollWidth > tela.clientWidth + 1,
    internos: rolagens.slice(0, 6).map((e) => `${e.id ? "#" + e.id : "." + String(e.className).split(" ")[0]} ${e.scrollWidth}>${e.clientWidth}`),
    saindo_da_janela: [...tela.querySelectorAll("button, input, summary, h1, h2, p, li")].filter(visivel)
      .filter((e) => e.getBoundingClientRect().right > vw + 1 || e.getBoundingClientRect().left < -1)
      .slice(0, 6).map((e) => e.id || e.tagName.toLowerCase() + ":" + (e.textContent || "").trim().slice(0, 24)),
  };
  // a área ocupada
  const cx = q(caixa);
  if (cx) {
    const r = cx.getBoundingClientRect();
    medida.area = {
      caixa: `${Math.round(r.width)}x${Math.round(r.height)}`,
      largura_pct: Math.round((100 * r.width) / vw),
      altura_pct: Math.round((100 * Math.min(r.height, vh)) / vh),
      area_pct: Math.round((100 * r.width * Math.min(r.height, vh)) / (vw * vh)),
    };
  }
  // a medida de leitura: o parágrafo mais largo, em caracteres de "0"
  const medidaCh = (p) => {
    const cs = getComputedStyle(p);
    const c = document.createElement("span");
    c.style.cssText = `position:absolute;visibility:hidden;white-space:pre;font:${cs.font};letter-spacing:${cs.letterSpacing}`;
    c.textContent = "0".repeat(10);
    document.body.appendChild(c);
    const um = c.getBoundingClientRect().width / 10;
    c.remove();
    return um > 0 ? Math.round(p.getBoundingClientRect().width / um) : null;
  };
  const paragrafos = [...tela.querySelectorAll("p, .server-frase, .antes-passo-frase")].filter(visivel).filter((p) => (p.textContent || "").trim().length > 80);
  const larguras = paragrafos.map((p) => [medidaCh(p), p]);
  const mais = larguras.sort((a, b) => b[0] - a[0])[0];
  medida.leitura_ch = { maior: mais ? mais[0] : 0, paragrafos: paragrafos.length, o_mais_largo: mais ? `${mais[1].className} «${mais[1].textContent.trim().slice(0, 40)}»` : null };
  // A GRADE de cartões: quando há menos cartões na primeira fileira do que colunas possíveis, eles dividem a largura (soma das larguras + vãos ≥ 90% da
  // grade). Pega a coluna vazia que sobra ao lado dos cartões, que a medida de «100% da raiz» não vê.
  const grade = tela.querySelector(".area-grade");
  if (grade && visivel(grade)) {
    const gr = grade.getBoundingClientRect();
    const gap = parseFloat(getComputedStyle(grade).columnGap) || 0;
    const cartoes = [...grade.querySelectorAll(".area-cartao")].filter(visivel).map((c) => c.getBoundingClientRect());
    const primeiro = cartoes.length ? Math.round(cartoes[0].top) : null;
    const fila = cartoes.filter((c) => Math.round(c.top) === primeiro);
    const possiveis = Math.max(1, Math.floor((gr.width + gap) / (360 + gap)));
    const soma = fila.reduce((t, c) => t + c.width, 0) + gap * Math.max(0, fila.length - 1);
    medida.grade = { cartoes_na_primeira_fila: fila.length, colunas_possiveis: possiveis, largura_da_grade: Math.round(gr.width), soma_pct: Math.round((100 * soma) / gr.width) };
  }
  // a barra
  const botaoBarra = barra && q(barra);
  let barraEl = null;
  if (botaoBarra) {
    barraEl = botaoBarra.closest(".antes-rodape, .preparar-barra, .area-barra") ?? botaoBarra.parentElement;
  }
  // os controles
  const controles = [...tela.querySelectorAll("button, input:not([type=hidden]), select, textarea, summary, a[href], [tabindex='0']")]
    .filter(visivel).filter((e) => !e.disabled);
  // a rolagem vertical que existe: a maior com sobra
  const roladores = [tela, ...tela.querySelectorAll("*")].filter((e) => {
    const o = getComputedStyle(e).overflowY;
    return (o === "auto" || o === "scroll") && e.scrollHeight > e.clientHeight + 1;
  });
  const rolador = roladores.sort((a, b) => b.clientHeight - a.clientHeight)[0] ?? null;
  const alcance = [];
  for (const c of controles) {
    c.scrollIntoView({ block: "nearest", inline: "nearest" });
    const r = c.getBoundingClientRect();
    const x = r.left + r.width / 2, y = r.top + r.height / 2;
    const dentro = x >= 0 && y >= 0 && x <= vw && y <= vh;
    const topo = dentro ? document.elementFromPoint(x, y) : null;
    const ok = Boolean(topo) && (topo === c || c.contains(topo) || topo.contains(c) && topo !== tela && !topo.matches("section, .server, .boot-caixa"));
    if (!ok) alcance.push({ controle: c.id || (c.textContent || "").trim().slice(0, 30), tampado_por: topo ? (topo.id || topo.className || topo.tagName).toString().slice(0, 40) : "fora da janela" });
  }
  medida.alcance = { controles: controles.length, nao_alcancaveis: alcance.slice(0, 8) };
  medida.alvos_pequenos = controles.filter((e) => e.matches("button, summary")).filter((e) => e.getBoundingClientRect().height < 36)
    .slice(0, 6).map((e) => (e.id || (e.textContent || "").trim().slice(0, 24)) + ":" + Math.round(e.getBoundingClientRect().height));
  // a barra não cobre o último conteúdo ao fim da rolagem
  if (rolador) rolador.scrollTop = rolador.scrollHeight;
  tela.scrollTop = tela.scrollHeight;
  if (barraEl) {
    const b = barraEl.getBoundingClientRect();
    const pos = getComputedStyle(barraEl).position;
    const conteudo = [...tela.querySelectorAll("li, p, h2, summary, button, dl, details")].filter(visivel).filter((e) => !barraEl.contains(e) && e !== barraEl);
    const ultimo = conteudo.sort((a, bb) => bb.getBoundingClientRect().bottom - a.getBoundingClientRect().bottom)[0];
    const ub = ultimo ? ultimo.getBoundingClientRect() : null;
    medida.barra = {
      posicao: pos, topo: Math.round(b.top), altura: Math.round(b.height), do_pe: Math.round(vh - b.bottom),
      ultimo_conteudo_fim: ub ? Math.round(ub.bottom) : null,
      cobre_o_ultimo: ub ? ub.bottom > b.top + 1 && ub.top < b.bottom : false,
      ocupa_da_janela_pct: Math.round((100 * b.height) / vh),
    };
    // a barra alcançável também ao fim
    const alvo = botaoBarra.getBoundingClientRect();
    const topo2 = document.elementFromPoint(alvo.left + alvo.width / 2, alvo.top + alvo.height / 2);
    medida.barra.botao_principal_livre = Boolean(topo2) && (topo2 === botaoBarra || botaoBarra.contains(topo2));
  }
  // a LARGURA ÚTIL REAL da sessão: o elemento e os filhos de conteúdo contra a janela menos os recuos do aparelho (env(safe-area-inset-*)).
  if (tela.id === "tela-sessao") {
    const sonda = document.createElement("div");
    sonda.style.cssText = "position:fixed;visibility:hidden;padding-left:env(safe-area-inset-left,0px);padding-right:env(safe-area-inset-right,0px)";
    document.body.appendChild(sonda);
    const insets = parseFloat(getComputedStyle(sonda).paddingLeft) + parseFloat(getComputedStyle(sonda).paddingRight);
    sonda.remove();
    const r = tela.getBoundingClientRect();
    const util = vw - insets;
    const filhos = [...tela.children].filter(visivel).map((e) => {
      const b = e.getBoundingClientRect();
      return { filho: e.id || String(e.className).split(" ")[0] || e.tagName.toLowerCase(), largura: Math.round(b.width), pct_do_util: Math.round((100 * b.width) / util) };
    });
    medida.largura_da_sessao = { elemento: Math.round(r.width), esquerda: Math.round(r.left), janela_menos_insets: Math.round(util), pct: Math.round((100 * r.width) / util),
      filhos_estreitos: filhos.filter((c) => c.pct_do_util < 90) };
  }
  // a sessão no celular: a altura dos avisos e a faixa de voz livre
  if (tela.id === "tela-sessao") {
    const avisos = ["veredito", "aviso-fone", "aviso-audio", "cel-aviso-persistencia", "cel-avisos-faixa"].map((i) => document.getElementById(i)).filter((e) => e && visivel(e));
    const soma = avisos.reduce((t, e) => t + e.getBoundingClientRect().height, 0);
    const voz = [...document.querySelectorAll(".voz-celular button, .voz-celular [role=switch]")].filter(visivel);
    const cobertos = voz.filter((b) => {
      const r = b.getBoundingClientRect();
      const topo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return !(topo === b || b.contains(topo));
    }).map((b) => b.getAttribute("aria-label") || (b.textContent || "").trim().slice(0, 24));
    medida.avisos = { visiveis: avisos.map((e) => e.id), altura_pct: Math.round((100 * soma) / vh), botoes_de_voz: voz.length, botoes_de_voz_cobertos: cobertos };
  }
  const origemNova = tela.querySelector(".cel-entrada-origem-nova");
  if (origemNova) {
    const r = origemNova.getBoundingClientRect();
    medida.aviso_origem_nova = { altura: Math.round(r.height), altura_pct: Math.round((100 * r.height) / vh), detalhes_fechados: tela.querySelector("#cel-entrada-origem-detalhes")?.hidden ?? null };
  }
  // Um diálogo que se mantém diálogo: a caixa cabe na janela, e tudo o que existe nela é alcançável por rolagem (nada cortado sem rolagem).
  if (caixa && q(caixa) && tela.id === "mods-aceite") {
    const cx = q(caixa);
    const r = cx.getBoundingClientRect();
    const rolador2 = [cx, ...cx.querySelectorAll("*")].find((e) => { const o = getComputedStyle(e).overflowY; return (o === "auto" || o === "scroll") && e.scrollHeight > e.clientHeight; });
    medida.dialogo_sem_corte = { ok: r.top >= -1 && r.bottom <= vh + 1 && r.left >= -1 && r.right <= vw + 1, caixa: `${Math.round(r.width)}x${Math.round(r.height)}`, rola: Boolean(rolador2) };
  }
  // SALVAR e CANCELAR dos cargos: visíveis ou alcançáveis rolando, dentro da área útil, sem cobrir conteúdo, sem reduzir fonte nem alvo.
  const salvar = tela.querySelector("#cargos-salvar");
  if (salvar && visivel(salvar)) {
    salvar.scrollIntoView({ block: "nearest" });
    const cancelar = [...tela.querySelectorAll("#cargos-form button")].find((b) => /CANCELAR|DESCARTAR/i.test(b.textContent));
    const info = (b) => { const r = b.getBoundingClientRect(); const topo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return { top: Math.round(r.top), bottom: Math.round(r.bottom), altura: Math.round(r.height), dentro_da_janela: r.top >= 0 && r.bottom <= vh, livre: topo === b || b.contains(topo) }; };
    medida.cargos_acoes = { salvar: info(salvar), cancelar: cancelar ? info(cancelar) : null, fonte_px: getComputedStyle(salvar).fontSize };
  }
  // A agenda do perfil: a altura do cartão e o último contato alcançável.
  const agenda = tela.querySelector("#perfil-agenda");
  if (agenda && visivel(agenda)) {
    const cartao = agenda.closest(".perfil-cartao-agenda");
    const linhas = [...agenda.children];
    const ultimo = linhas.at(-1);
    let cobertura = null;
    if (ultimo) {
      ultimo.scrollIntoView({ block: "nearest" });
      for (let e = ultimo.parentElement; e && e !== document.body; e = e.parentElement) { const o = getComputedStyle(e).overflowY; if ((o === "auto" || o === "scroll") && e.scrollHeight > e.clientHeight) e.scrollTop = e.scrollHeight; }
      ultimo.scrollIntoView({ block: "nearest" });
      const r = ultimo.getBoundingClientRect();
      const topo = document.elementFromPoint(r.left + r.width / 2, r.top + Math.min(r.height / 2, 12));
      cobertura = { visivel: r.top >= 0 && r.bottom <= vh && r.height > 0, hittable: Boolean(topo) && (topo === ultimo || ultimo.contains(topo) || topo.contains(ultimo)) };
    }
    const cr = cartao ? cartao.getBoundingClientRect() : null;
    medida.agenda = { contatos: linhas.length, altura_do_cartao: cr ? Math.round(cr.height) : null, altura_pct: cr ? Math.round((100 * cr.height) / vh) : null, ultimo: cobertura };
  }
  // movimento
  const animando = document.getAnimations().filter((a) => a.effect && tela.contains(a.effect.target) && a.playState === "running");
  medida.animacoes_em_curso = animando.length;
  medida.reduzido = matchMedia("(prefers-reduced-motion: reduce)").matches;
  return medida;
}

/** A sequência do Tab, até `n` passos, ou até voltar ao primeiro. */
async function ordemDoFoco(aba, raiz, n = 40) {
  await aba.evaluate((r) => { const t = document.querySelector(r); t.focus(); }, raiz);
  const vistos = [];
  for (let i = 0; i < n; i++) {
    await aba.keyboard.press("Tab");
    const id = await aba.evaluate((r) => {
      const a = document.activeElement;
      if (!a || a === document.body) return null;
      const dentro = document.querySelector(r).contains(a);
      const cs = getComputedStyle(a);
      const anel = cs.outlineStyle === "auto" || (cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0) || (cs.boxShadow && cs.boxShadow !== "none");
      return { nome: (dentro ? "" : "FORA:") + (a.id || (a.textContent || a.getAttribute("aria-label") || a.tagName).trim().slice(0, 28)), anel: Boolean(anel) };
    }, raiz);
    if (!id) continue;
    if (vistos.length && id.nome === vistos[0].nome) break;
    vistos.push(id);
  }
  return vistos;
}

/** O teclado virtual simulado: o `visualViewport` encolhe com um campo em foco (a barra de ações sai), e volta no blur e no resize. */
async function simularTeclado(aba, t) {
  return aba.evaluate(async ({ raiz, barra }) => {
    const tela = document.querySelector(raiz);
    const botao = document.querySelector(barra);
    const vazio = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(e.closest(".area-barra") ?? e).display !== "none"; };
    const campo = [...tela.querySelectorAll("input[type=text], input:not([type]), textarea")].find((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; });
    const normal = vazio(botao);
    if (!campo) return { ok: normal, sem_campo: true, visivel_sem_teclado: normal };
    const vista = window.visualViewport;
    const alturaReal = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(vista), "height");
    const espera = (ms) => new Promise((r) => setTimeout(r, ms));
    Object.defineProperty(vista, "height", { configurable: true, get: () => window.innerHeight - Math.max(200, Math.round(window.innerHeight * 0.5)) });
    campo.focus();
    vista.dispatchEvent(new Event("resize"));
    await espera(80);
    const comTeclado = vazio(botao);
    // o teclado fecha: o blur e o resize devolvem a barra
    campo.blur();
    delete vista.height;
    Object.defineProperty(vista, "height", alturaReal ? { configurable: true, get: () => alturaReal.get.call(vista) } : { configurable: true, get: () => window.innerHeight });
    vista.dispatchEvent(new Event("resize"));
    await espera(150);
    const depois = vazio(botao);
    const r = botao.getBoundingClientRect();
    const topo = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return { ok: normal && !comTeclado && depois && (topo === botao || botao.contains(topo)),
      visivel_sem_teclado: normal, escondida_com_teclado: !comTeclado, de_volta_depois: depois, alcancavel_depois: topo === botao || botao.contains(topo) };
  }, { raiz: t.raiz, barra: t.barra });
}

/** O teclado simulado sobre um campo de uma seção: depois de rolar até ele, o campo fica acima do teclado (a rolagem do painel resolve). */
async function simularTecladoNoCampo(aba, t) {
  return aba.evaluate(async ({ raiz }) => {
    const tela = document.querySelector(raiz);
    const visivel = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && !e.closest("[hidden]"); };
    // REGRA CORRIGIDA (motivo): o teclado do sistema só abre sobre campos de TEXTO. Um `range` (a sensibilidade da voz), um `checkbox`, um `radio` e
    // um `select` não abrem o teclado, então a cena do teclado simulado não pode escolhê-los: ela media «o campo fica sob o teclado» para um controle
    // que nunca o abre. Valem input sem tipo, text, email, search, tel, url, number, password e textarea.
    const campo = [...tela.querySelectorAll('input:not([type]), input[type="text"], input[type="email"], input[type="search"], input[type="tel"], input[type="url"], input[type="number"], input[type="password"], textarea')].filter(visivel)[0];
    if (!campo) return { ok: true, sem_campo: true };
    const vista = window.visualViewport;
    const teclado = Math.max(200, Math.round(window.innerHeight * 0.5));
    const topoDoTeclado = window.innerHeight - teclado;
    Object.defineProperty(vista, "height", { configurable: true, get: () => topoDoTeclado });
    campo.focus();
    vista.dispatchEvent(new Event("resize"));
    // O que o navegador faz com o teclado aberto: rola o que rola para o campo ficar na parte visível (acima do teclado). A pergunta da bancada é se a
    // tela PERMITE isso: sem folga embaixo do conteúdo, o painel não rola o bastante e o campo fica sob o teclado.
    campo.scrollIntoView({ block: "nearest" });
    for (let e = campo.parentElement; e; e = e.parentElement) {
      const o = getComputedStyle(e).overflowY;
      if ((o === "auto" || o === "scroll") && e.scrollHeight > e.clientHeight) {
        const falta = campo.getBoundingClientRect().bottom - (topoDoTeclado - 16);
        if (falta > 0) e.scrollTop += falta;
      }
    }
    await new Promise((r) => setTimeout(r, 100));
    const r = campo.getBoundingClientRect();
    const ok = r.bottom <= topoDoTeclado + 1 && r.top >= 0;
    // O PERFIL não tem barra fixa: SALVAR e CANCELAR estão no fluxo do formulário. A exigência é que continuem ALCANÇÁVEIS rolando e que nada os cubra
    // de verdade (com o teclado simulado), e não que sumam com o teclado.
    const acoes = ["perfil-salvar", "perfil-cancelar", "cargos-salvar", "cargos-descartar"].map((id) => document.getElementById(id)).filter((b) => b && visivel(b));
    const acoesLivres = acoes.every((b) => {
      b.scrollIntoView({ block: "nearest" });
      const rb = b.getBoundingClientRect();
      const topo = document.elementFromPoint(rb.left + rb.width / 2, rb.top + rb.height / 2);
      return topo === b || b.contains(topo);
    });
    delete vista.height;
    return { ok: ok && acoesLivres, campo: campo.id || campo.tagName, fim_do_campo: Math.round(r.bottom), topo_do_teclado: topoDoTeclado, acoes_alcancaveis: acoesLivres, acoes: acoes.map((b) => b.id) };
  }, { raiz: t.raiz });
}

/** A regra que reprova (o que o dono pediu), por medida. */
function veredito(m, t) {
  const f = [];
  if (!m.tela_visivel) return ["a tela não ficou visível"];
  const s = m.sobrefluxo_horizontal;
  if (s.pagina || s.tela) f.push("sobrefluxo horizontal da página/tela");
  if (s.saindo_da_janela.length) f.push("elemento fora da janela: " + s.saindo_da_janela.join(", "));
  if (m.alcance.nao_alcancaveis.length) f.push("controle coberto/fora: " + m.alcance.nao_alcancaveis.map((c) => c.controle).join(", "));
  if (m.barra && (m.barra.cobre_o_ultimo || !m.barra.botao_principal_livre)) f.push("a barra cobre conteúdo");
  const larg = Number(m.janela.split("x")[0]);
  if (m.area && larg >= 1000 && m.area.largura_pct < 85 && !t.dialogo) f.push(`coluna estreita: ${m.area.largura_pct}% da largura`);
  if (m.leitura_ch && m.leitura_ch.maior > 90) f.push(`parágrafo largo demais: ${m.leitura_ch.maior}ch`);
  if (m.acessibilidade && m.acessibilidade.aria.problemas.length) f.push("aria: " + m.acessibilidade.aria.problemas.join("; "));
  if (m.acessibilidade && m.acessibilidade.sem_anel_de_foco.length) f.push("sem anel de foco visível: " + m.acessibilidade.sem_anel_de_foco.join(", "));
  if (m.largura_da_sessao && Number(m.janela.split("x")[0]) >= 600 && m.largura_da_sessao.elemento < m.largura_da_sessao.janela_menos_insets - 8) {
    f.push(`a sessão ocupa ${m.largura_da_sessao.elemento}px de ${m.largura_da_sessao.janela_menos_insets}px úteis (coluna estreita)`);
  }
  if (m.dialogo_sem_corte && !m.dialogo_sem_corte.ok) f.push("o diálogo corta conteúdo: " + JSON.stringify(m.dialogo_sem_corte));
  if (m.grade && m.grade.cartoes_na_primeira_fila > 0 && m.grade.cartoes_na_primeira_fila < m.grade.colunas_possiveis && m.grade.soma_pct < 90) {
    f.push(`os cartões ocupam só ${m.grade.soma_pct}% da grade (${m.grade.cartoes_na_primeira_fila} cartões, ${m.grade.colunas_possiveis} colunas possíveis): coluna vazia`);
  }
  if (m.agenda && m.agenda.ultimo && !(m.agenda.ultimo.visivel && m.agenda.ultimo.hittable)) f.push("o último contato da agenda não é alcançável: " + JSON.stringify(m.agenda.ultimo));
  if (m.agenda && Number(m.janela.split("x")[0]) >= 1100 && m.agenda.altura_pct < 60) f.push(`o cartão da agenda tem só ${m.agenda.altura_pct}% da altura no desktop`);
  if (m.cargos_acoes && !(m.cargos_acoes.salvar.livre && m.cargos_acoes.salvar.dentro_da_janela)) f.push("SALVAR não está à vista nem alcançável: " + JSON.stringify(m.cargos_acoes.salvar));
  if (m.cargos_acoes && m.cargos_acoes.cancelar && !m.cargos_acoes.cancelar.livre) f.push("CANCELAR coberto: " + JSON.stringify(m.cargos_acoes.cancelar));
  if (m.teclado_no_campo && !m.teclado_no_campo.ok) f.push("o campo ficou sob o teclado: " + JSON.stringify(m.teclado_no_campo));
  if (m.aviso_origem_nova && m.aviso_origem_nova.altura_pct > 30) f.push(`o aviso do endereço novo toma ${m.aviso_origem_nova.altura_pct}% da altura`);
  if (m.mutacoes_em_1s > 200) f.push(`laço de mutações: ${m.mutacoes_em_1s} em 1 s`);
  if (m.avisos && m.avisos.altura_pct > 25) f.push(`os avisos tomam ${m.avisos.altura_pct}% da altura`);
  if (m.avisos && m.avisos.botoes_de_voz_cobertos.length) f.push("botão de voz coberto: " + m.avisos.botoes_de_voz_cobertos.join(", "));
  if (m.teclado && !m.teclado.ok) f.push("a barra de ações não voltou/sumiu direito com o teclado: " + JSON.stringify(m.teclado));
  if (m.reduzido && m.animacoes_em_curso > 0) f.push("animação com reduced-motion");
  return f;
}

// Regressão do AJUSTES-4 no PWA: dois atributos de navegação deixavam
// coluna E corpo ocultos. Reutiliza servidor, dados e cascas desta bancada.
// --volta-ajustes roda ambos os motores, 390/320, celular e app estreito;
// remove só a regra corretiva na resposta CSS para provar o mutante.
async function provarVoltaDosAjustes(pasta) {
  const assert = require("node:assert/strict");
  const pw = require(process.env.PLAYWRIGHT || "playwright");
  const regra = ':root[data-casca="celular"] #tela-server[data-cel-pagina="lista"] .server-coluna {\n  display: flex;\n}';
  const css = fs.readFileSync(path.join(__dirname, "../ui/celular-sessao.css"), "utf8");
  assert.ok(css.includes(regra), "o mutante não encontrou a regra: teste cego");
  fs.mkdirSync(pasta, { recursive: true });
  const servidor = servir();
  await new Promise((r) => servidor.listen(0, "127.0.0.1", r));
  const resultados = [];
  try {
    for (const motor of ["chromium", "webkit"]) {
      const browser = await pw[motor].launch({ headless: true });
      try {
        for (const largura of [390, 320]) for (const casca of ["celular", "app"]) for (const mutante of [false, true]) {
          const context = await browser.newContext({ viewport: { width: largura, height: largura === 390 ? 844 : 568 }, hasTouch: casca === "celular", isMobile: casca === "celular" });
          try {
            const page = await context.newPage();
            const erros = []; page.on("pageerror", (e) => erros.push(e.message));
            if (mutante) await page.route("**/celular-sessao.css", (route) => route.fulfill({ contentType: "text/css", body: css.replace(regra, "") }));
            await page.addInitScript(({ tabela, foto, casca }) => {
              window.__retrato = foto;
              if (casca === "celular") new MutationObserver((_, o) => {
                if (!document.documentElement) return;
                document.documentElement.dataset.casca = "celular"; o.disconnect();
              }).observe(document, { childList: true });
              window.__TAURI__ = { core: { invoke: async (cmd) => Object.hasOwn(tabela, cmd) ? structuredClone(tabela[cmd]) : null },
                event: { listen: async () => () => {} }, window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) } };
            }, { ...(await montarTabela({})), casca });
            await page.goto(`http://127.0.0.1:${servidor.address().port}/`, { waitUntil: "domcontentloaded" });
            await page.waitForTimeout(700);
            if (casca === "celular") await page.evaluate(() => {
              for (const s of document.querySelectorAll("section.tela")) s.hidden = true;
              document.getElementById("tela-sessao").hidden = false;
              window.desenhar(window.__retrato);
              document.getElementById("tela-sessao").dataset.aba = "ajustes";
            });
            else await page.evaluate(() => abrirServer("tela-boot"));
            await page.locator("#secao-audio").waitFor({ state: "visible" });
            const medidas = [];
            for (let ciclo = 0; ciclo < 2; ciclo++) {
              await page.locator("#secao-audio").click();
              const voltar = page.locator(casca === "celular" ? ".cel-ajustes-voltar" : "#server-ao-indice");
              await voltar.click();
              await page.waitForTimeout(300);
              const medida = await page.evaluate(() => {
                const tela = document.getElementById("tela-server"), coluna = tela.querySelector(".server-coluna"), corpo = tela.querySelector(".server-corpo");
                const botao = document.getElementById("secao-audio"), r = botao.getBoundingClientRect();
                const atingido = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
                return { vista: tela.dataset.vista, pagina: tela.dataset.celPagina,
                  coluna: getComputedStyle(coluna).display, corpo: getComputedStyle(corpo).display,
                  visivel: r.width > 0 && r.height > 0 && Boolean(atingido && botao.contains(atingido)) };
              });
              medidas.push(medida);
              if (mutante && casca === "celular") {
                assert.equal(medida.coluna, "none"); assert.equal(medida.corpo, "none");
                assert.equal(medida.visivel, false, "o mutante precisava reproduzir o índice vazio");
                break;
              }
              assert.equal(medida.visivel, true, `${motor}/${casca}/${largura}: índice não voltou`);
              if (casca === "celular") { assert.equal(medida.coluna, "flex"); assert.equal(medida.corpo, "none"); }
              else assert.equal(medida.vista, "indice", "o desktop estreito perdeu seu próprio índice");
            }
            const foco = [];
            if (!mutante) {
              assert.equal(await page.locator("#tela-server").getAttribute("aria-modal"), casca === "celular" ? "false" : "true");
              // Tab real no último controle: o desktop prende o foco, a aba
              // móvel deixa alcançar a navegação externa. O mutante repõe
              // a declaração modal que ocultava essa navegação no Safari AX.
              for (const modalMutante of casca === "celular" ? [false, true] : [false]) {
                await page.evaluate((mutar) => {
                  if (mutar) document.getElementById("tela-server").setAttribute("aria-modal", "true");
                  focaveisDaConfiguracao().at(-1).focus();
                }, modalMutante);
                await page.keyboard.press("Tab");
                const dentro = await page.evaluate(() => document.getElementById("tela-server").contains(document.activeElement));
                assert.equal(dentro, casca === "app" || modalMutante, "Tab não respeitou a modalidade da casca");
                foco.push({ modalMutante, dentro });
              }
              if (casca === "celular") {
                await page.locator("#tela-server").evaluate((e) => e.setAttribute("aria-modal", "false"));
                await page.getByRole("button", { name: "SERVIDOR", exact: true }).click();
                assert.equal(await page.locator("#tela-server").evaluate((e) => e.hidden), true, "SERVIDOR não fechou AJUSTES");
              }
            }
            assert.deepEqual(erros, [], "erro de página durante a navegação");
            await page.screenshot({ path: path.join(pasta, `${motor}-${casca}-${largura}-${mutante ? "mutante" : "real"}.png`) });
            resultados.push({ motor, casca, largura, mutante, medidas, foco });
            fs.writeFileSync(path.join(pasta, "volta-ajustes.json"), JSON.stringify(resultados, null, 2));
            console.log(`${motor} ${casca} ${largura} ${mutante ? "mutante" : "real"}: ok`);
          } finally { await context.close(); }
        }
      } finally { await browser.close(); }
    }
  } finally { await new Promise((r) => servidor.close(r)); }
}

// O mouse atravessa um snapshot real: WebKit perde o click se o texto
// do botão for substituído entre down e up. O mutante restaura esse defeito.
async function provarSaidaDuranteRedesenho(pasta) {
  const assert = require("node:assert/strict");
  const pw = require(process.env.PLAYWRIGHT || "playwright");
  const fonte = fs.readFileSync(path.join(__dirname, "../ui/tela-chamada.js"), "utf8");
  const guarda = "if (sair.textContent !== rotuloDaSaida) sair.textContent = rotuloDaSaida;";
  const guardaCompartilhar = 'if (outro.textContent !== "COMPARTILHAR") outro.textContent = "COMPARTILHAR";';
  assert.ok(fonte.includes(guarda) && fonte.includes(guardaCompartilhar), "mutante não encontrou as guardas");
  fs.mkdirSync(pasta, { recursive: true });
  const servidor = servir();
  await new Promise((r) => servidor.listen(0, "127.0.0.1", r));
  const resultados = [];
  try {
    for (const motor of ["chromium", "webkit"]) {
      const browser = await pw[motor].launch({ headless: true });
      try {
        for (const mutante of [false, true]) {
          const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
          try {
            const erros = []; page.on("pageerror", (e) => erros.push(e.message));
            if (mutante) await page.route("**/tela-chamada.js", (r) => r.fulfill({ contentType: "text/javascript", body: fonte.replace(guarda, "sair.textContent = rotuloDaSaida;").replace(guardaCompartilhar, 'outro.textContent = "COMPARTILHAR";') }));
            await page.addInitScript(({ tabela }) => {
              window.__saidas = 0;
              window.__TAURI__ = { core: { invoke: async (cmd) => {
                if (cmd === "leave_voice_room") window.__saidas++;
                return Object.hasOwn(tabela, cmd) ? structuredClone(tabela[cmd]) : null;
              } }, event: { listen: async () => () => {} }, window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) } };
            }, await montarTabela({}));
            await page.goto(`http://127.0.0.1:${servidor.address().port}/`);
            await page.evaluate(() => {
              for (const e of document.querySelectorAll("section.tela")) e.hidden = true;
              document.getElementById("tela-sessao").hidden = false;
              // Observar a chamada do handler real, sem iniciar uma captura.
              abrirCompartilhar = async () => { window.__compartilhar++; };
            });
            for (const botao of ["operador-sair", "operador-vista"]) for (const redesenho of [false, true]) {
              await page.evaluate(() => { window.__saidas = 0; window.__compartilhar = 0; desenharORodapeDoOperador({ name: "SALA 1" }); });
              const caixa = await page.locator("#" + botao).boundingBox();
              assert.ok(caixa && caixa.width > 0 && caixa.height > 0);
              await page.mouse.move(caixa.x + caixa.width / 2, caixa.y + caixa.height / 2);
              await page.mouse.down();
              if (redesenho) await page.evaluate(() => desenharORodapeDoOperador({ name: "SALA 1" }));
              await page.mouse.up();
              await page.waitForTimeout(100);
              const chamadas = await page.evaluate((id) => id === "operador-sair" ? window.__saidas : window.__compartilhar, botao);
              const esperado = motor === "webkit" && mutante && redesenho ? 0 : 1;
              assert.equal(chamadas, esperado, `${motor} ${botao} mutante=${mutante} redesenho=${redesenho}`);
              resultados.push({ motor, versao: browser.version(), botao, mutante, redesenho, chamadas });
            }
            assert.deepEqual(erros, []);
          } finally { await page.close(); }
        }
      } finally { await browser.close(); }
    }
    fs.writeFileSync(path.join(pasta, "saida-redesenho.json"), JSON.stringify(resultados, null, 2));
    console.log(`SAIR durante redesenho: ${resultados.length}/${resultados.length} PASS; mutante WebKit reproduz clique perdido`);
  } finally { await new Promise((r) => servidor.close(r)); }
}

async function principal() {
  const args = process.argv.slice(2);
  const [pasta] = args.filter((a) => !a.startsWith("--"));
  if (!pasta) throw new Error("uso: area-util.cjs <pasta> [--antes] [--webkit] [--so=<trecho>]");
  if (args.includes("--volta-ajustes")) return provarVoltaDosAjustes(pasta);
  if (args.includes("--saida-redesenho")) return provarSaidaDuranteRedesenho(pasta);
  const antes = args.includes("--antes");
  const so = args.find((a) => a.startsWith("--so="))?.slice(5) ?? "";
  // `--repetir=N` roda cada cena N vezes (para dizer se uma falha de execução é intermitente de startup, e quantas caíram).
  const repeticoes = Math.max(1, Number(args.find((a) => a.startsWith("--repetir="))?.slice(10)) || 1);
  fs.mkdirSync(pasta, { recursive: true });
  const escolhido = process.env.PLAYWRIGHT;
  const pw = escolhido ? require(escolhido) : require("playwright");
  const soMotor = args.find((a) => a.startsWith("--so-motor="))?.slice(11);
  const motores = (args.includes("--webkit") ? ["chromium", "webkit"] : ["chromium"]).filter((m) => !soMotor || m === soMotor);
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${servidor.address().port}/`;
  const saida = [];
  const parcial = path.join(pasta, `medidas${antes ? "-antes" : ""}.parcial.json`);
  // Cada cena medida (ou que falhou) vai ao disco na hora: uma queda não apaga o que já foi medido.
  const registrar = (m) => {
    saida.push(m);
    fs.writeFileSync(parcial, JSON.stringify(saida));
    console.log(`${m.motor} ${m.tela} ${m.tamanho}${m.reduzido_pedido ? " (reduced-motion)" : ""}${m.repeticao > 1 ? ` #${m.repeticao}` : ""}: ${m.reprovacoes.length ? "REPROVA — " + m.reprovacoes.join("; ") : "ok"}`);
  };
  try {
    for (const motor of motores) {
      const navegador = await pw[motor].launch({ headless: true });
      for (const t of TELAS.filter((x) => x.tela.includes(so))) {
        const cena = t.cenaPropria ?? CENAS.find((c) => c.nome === t.cena);
        if (!cena) throw new Error(`a cena ${t.cena} sumiu da navegacao-0-17.cjs`);
        for (const tam of TAMANHOS) {
          if ((t.so_celular || t.so_estreito) && !tam.toque) continue;
          if (tam.tablet && !t.so_celular) continue;
          for (const [reduzido, repeticao] of [false, true].flatMap((r) => Array.from({ length: repeticoes }, (_, i) => [r, i + 1]))) {
            if (reduzido && !(tam.w === 1440 || tam.w === 390)) continue;
            const nome = tam.nome ?? `${tam.w}x${tam.h}`;
            // REGRA CORRIGIDA (cena mal aplicada): CARGOS existe no PWA do celular (a casca `celular`), e não no app em janela de 320px. Medi-lo na casca
            // do app a 320/390px media o app inteiro de lado (a telemetria da sessão atrás dele alargava a página) e reprovava os cargos por um defeito
            // que não é deles. Com `celularNoToque`, os tamanhos de toque usam a casca do celular; os de desktop seguem na do app.
            const casca = cena.celularNoToque && tam.toque ? "celular" : cena.casca;
            let contexto = null;
            const diagnostico = { erros_do_console: [], requisicoes_que_falharam: [], pagina_caiu: false };
            try {
            contexto = await navegador.newContext({
              viewport: { width: tam.w, height: tam.h }, deviceScaleFactor: tam.zoom ?? 1,
              hasTouch: Boolean(tam.toque), isMobile: Boolean(tam.toque) && motor !== "firefox",
              reducedMotion: reduzido ? "reduce" : "no-preference",
            });
            const aba = await contexto.newPage();
            const erros = [];
            aba.on("pageerror", (e) => erros.push(String(e.message).slice(0, 160)));
            // O DIAGNÓSTICO da cena: o que a página disse enquanto montava, para uma falha de execução vir com o motivo.
            aba.on("console", (c) => { if (c.type() === "error" && diagnostico.erros_do_console.length < 8) diagnostico.erros_do_console.push(c.text().slice(0, 160)); });
            aba.on("requestfailed", (r) => { if (diagnostico.requisicoes_que_falharam.length < 8) diagnostico.requisicoes_que_falharam.push(`${r.method()} ${new URL(r.url()).pathname}: ${r.failure()?.errorText ?? "?"}`); });
            aba.on("crash", () => { diagnostico.pagina_caiu = true; });
            if (casca !== "web") await aba.addInitScript(({ tabela, falhas, foto, casca }) => {
              window.__retrato = foto; window.__falhas = falhas;
              if (casca === "celular") {
                new MutationObserver((_, o) => { if (!document.documentElement) return; document.documentElement.dataset.casca = "celular"; o.disconnect(); })
                  .observe(document, { childList: true });
              }
              window.__TAURI__ = { core: { invoke: async (cmd, a) => {
                if (Object.hasOwn(window.__falhas, cmd)) throw structuredClone(window.__falhas[cmd]);
                return Object.hasOwn(tabela, cmd) ? structuredClone(tabela[cmd]) : null;
              } }, event: { listen: async () => () => {} },
              window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) } };
            }, { ...(await montarTabela(cena)), casca });
            // O GUARDA do laço de observador: a página tem de chegar ao DOMContentLoaded em 10 s (uma faixa que se reescreve a si mesma o impede).
            await aba.goto(url, { waitUntil: "domcontentloaded", timeout: 10000 }).catch((e) => erros.push("DOMContentLoaded não chegou em 10 s: " + e.message.slice(0, 80)));
            await aba.waitForTimeout(700);
            if (cena.sessao) {
              await aba.evaluate(`for (const s of document.querySelectorAll("section.tela")) s.hidden = true;
                document.getElementById("tela-sessao").hidden = false; window.desenhar(window.__retrato);
                if (typeof sincronizarMensagens === "function") sincronizarMensagens(99);`).catch((e) => erros.push("sessão: " + e.message));
              await aba.waitForTimeout(400);
            }
            if (cena.passos) await aba.evaluate(`(async () => { ${cena.passos.includes(";") ? cena.passos : "await " + cena.passos} })()`).catch((e) => erros.push("passos: " + e.message));
            await aba.waitForTimeout(cena.espera ?? 600);
            const sufixo = `${antes ? "-antes" : ""}${motor === "webkit" ? "-webkit" : ""}`;
            if (!reduzido) await aba.screenshot({ path: path.join(pasta, `${t.tela}-${nome}${sufixo}.png`), caret: "initial" });
            const m = await aba.evaluate(medirNaPagina, t);
            if (cena.sessao) {
              // Depois de um aviso ligar e desligar, o que o DOM faz em 1 s: um laço seriam milhares de mutações.
              m.mutacoes_em_1s = await aba.evaluate(async () => {
                let n = 0;
                const o = new MutationObserver((r) => { n += r.length; });
                o.observe(document.getElementById("tela-sessao"), { subtree: true, childList: true, attributes: true, characterData: true });
                const f = document.getElementById("aviso-audio");
                f.hidden = !f.hidden; await new Promise((r) => setTimeout(r, 30)); f.hidden = !f.hidden;
                await new Promise((r) => setTimeout(r, 1000));
                o.disconnect();
                return n;
              });
            }
            if (!reduzido && m.tela_visivel) {
              const passos = (await ordemDoFoco(aba, t.raiz)).filter(Boolean);
              m.foco = passos.map((p) => p.nome);
              m.acessibilidade = {
                passos_do_tab: passos.length,
                sem_anel_de_foco: passos.filter((p) => !p.anel).map((p) => p.nome).slice(0, 8),
                aria: await aba.evaluate((raiz) => {
                  const tela = document.querySelector(raiz);
                  const problemas = [];
                  for (const e of tela.querySelectorAll("[aria-expanded]")) {
                    for (const id of (e.getAttribute("aria-controls") || "").split(/\s+/).filter(Boolean)) {
                      if (!document.getElementById(id)) problemas.push(`${e.id || e.textContent.trim().slice(0, 20)}: aria-controls ${id} não existe`);
                    }
                    if (!["true", "false"].includes(e.getAttribute("aria-expanded"))) problemas.push(`${e.id}: aria-expanded inválido`);
                  }
                  const faixa = tela.querySelector("#cel-avisos-faixa");
                  const indice = tela.querySelectorAll(".server-secao[aria-current]");
                  return { expansores: tela.querySelectorAll("[aria-expanded]").length, secoes_com_aria_current: indice.length, faixa_de_avisos: faixa ? { rotulo: faixa.textContent, expandido: faixa.getAttribute("aria-expanded") } : null, problemas };
                }, t.raiz),
              };
            }
            if (!reduzido && tam.toque && t.barra) m.teclado = await simularTeclado(aba, t);
            if (!reduzido && tam.toque && (t.tela === "config-audio" || t.tela === "perfil-com-agenda")) m.teclado_no_campo = await simularTecladoNoCampo(aba, t);
            if (erros.some((e) => e.startsWith("DOMContentLoaded"))) m.erros_de_pagina = erros;
            m.tela = t.tela; m.motor = motor; m.tamanho = nome; m.reduzido_pedido = reduzido; m.erros_de_pagina = erros; m.repeticao = repeticao;
            m.reprovacoes = veredito(m, t);
            if (!m.tela_visivel) m.diagnostico = diagnostico;
            registrar(m);
            } catch (erro) {
              // FALHA DE EXECUÇÃO: a cena não pôde ser medida (a página não montou, o motor caiu, o seletor sumiu). Não é um passe, e não derruba o resto:
              // vira uma linha da tabela com o motivo e o diagnóstico da página.
              registrar({ tela: t.tela, motor, tamanho: nome, reduzido_pedido: reduzido, repeticao, falha_de_execucao: String(erro && erro.message || erro).slice(0, 300),
                reprovacoes: ["FALHA DE EXECUÇÃO: " + String(erro && erro.message || erro).slice(0, 200)], diagnostico });
            } finally {
              if (contexto) await contexto.close().catch(() => {});
            }
          }
        }
      }
      await navegador.close();
    }
  } finally {
    servidor.close();
  }
  const arquivo = path.join(pasta, `medidas${antes ? "-antes" : ""}.json`);
  fs.writeFileSync(arquivo, JSON.stringify(saida, null, 1));
  const reprovadas = saida.filter((m) => m.reprovacoes.length);
  console.log(`\n${saida.length} medidas, ${reprovadas.length} reprovadas → ${arquivo}`);
  if (!antes && reprovadas.length) process.exitCode = 1;
}

/** A tabela de respostas do duble, pela mesma montagem da `navegacao-0-17.cjs` para a cena. */
async function montarTabela(cena) {
  const { respostas, retrato } = require("./telas.cjs");
  const tabela = respostas({
    microfones: [{ id: "mic", name: "Microfone interno", is_default: true }],
    saidas: [{ id: "saida", name: "Saída padrão", is_default: true }],
    ...(cena.resposta ?? {}),
  });
  const foto = retrato(cena.retrato ?? {});
  tabela.snapshot = foto;
  return { tabela, falhas: cena.falhas ?? {}, foto };
}

module.exports = { medirNaPagina, veredito, TELAS, TAMANHOS };

if (require.main === module) principal().catch((e) => { console.error(e); process.exitCode = 1; });
