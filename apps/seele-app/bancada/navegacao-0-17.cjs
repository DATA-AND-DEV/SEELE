// **A navegação da 0.17, tela por tela** (E2 do contrato da 0.17, ui-ux).
//
// # O que ela é
//
// Uma volta pela casca inteira num navegador de verdade (o Chromium e o WebKit
// do Playwright), com retrato de cada tela, camada e estado que o inventário
// (`docs/evidencias/ui-0.17/inventario.md`) lista. Dois modos:
//
// - **app**: a casca do desktop com a ponte do Tauri simulada, como a
//   `telas.cjs` faz. O que se vê é o `ui/` de verdade; o que responde é esta
//   bancada. Não há Rust do outro lado — ela diz se a tela cabe, se o controle
//   está ao alcance e se o texto se lê, e **não** que o produto faz o que
//   promete;
// - **web**: a casca do navegador, com a `ponte-web.js` de verdade e nenhum
//   duble, na casca larga (1920×1080) e na de celular (390×844, toque). Sem
//   servidor do outro lado: a entrada, os ajustes e as recusas da ponte são
//   reais; a sessão no navegador é da `sessao-no-navegador.cjs`, com `seeled`.
//
// # Rodar
//
//   node apps/seele-app/bancada/navegacao-0-17.cjs <pasta-de-saida> [filtro] [--webkit]
//
// O Playwright vem de `playwright.cjs` (a variável `PLAYWRIGHT`, ou o
// `node_modules` do repositório). `filtro` é um trecho do nome da cena. Cada
// cena escreve `<pasta>/<nome>.png`, e o fim imprime uma linha por cena com os
// erros de página e os comandos sem resposta combinada — um retrato com erro de
// página não é um retrato da tela, é um retrato do defeito.

const fs = require("node:fs");
const path = require("node:path");
const { servir, respostas, retrato } = require("./telas.cjs");

const FHD = { width: 1920, height: 1080 };
const CELULAR = { width: 390, height: 844 };
/** O mínimo da janela do app (`tauri.conf.json`: minWidth 720, minHeight 480). */
const MINIMA = { width: 720, height: 480 };

// ------------------------------------------------------------------ os dados

const agora = Math.floor(Date.now() / 1000);

/** Um servidor guardado, com os nomes de `servidores::Servidor`. */
const guardado = (id, nome, versao, caminho = `servidores/${id}`) => ({
  id, nome, versao, caminho, ultimo_uso: agora - 3600,
});

/** As linhas de `versoes::linhas_para_hospedar` na 0.17. */
const LINHAS_017 = [
  { linha: "2", servidor: "2.0", apis: [7, 6], app: null, esta: true, instalada: true, tem_pacote: true, perde_conversas: false },
  { linha: "1", servidor: "1.0", apis: [6], app: "0.16.0", esta: false, instalada: true, tem_pacote: true, perde_conversas: false },
  { linha: "0.15", servidor: "0.15", apis: [5, 4, 3], app: "0.15.2", esta: false, instalada: false, tem_pacote: true, perde_conversas: false },
];

const versaoDeMod = (versao, api, hash) => ({ versao, api, hash, nivel: "oficial", notas: [], alcanca: [], arquivos: [] });
const CATALOGO = {
  esquema: 1,
  gerado_em: agora,
  mods: [
    { id: "seele/estilo", titulo: "Estilo", resumo: "", repo: "", nivel: "oficial",
      versoes: [versaoDeMod("3.1.0", 4, "h31"), versaoDeMod("4.0.0", 6, "h40")] },
    { id: "seele/mesa", titulo: "Mesa", resumo: "", repo: "", nivel: "oficial",
      versoes: [versaoDeMod("3.1.0", 4, "m31")] },
  ],
};

/** A recusa do caso concreto: um servidor da 0.15 com MODs da API 4. */
const RECUSA_MODS = { ModsDeApiAntiga: { mods: [["seele/estilo", 4], ["seele/mesa", 4], ["x/sumido", null]] } };

/** O servidor da 0.15, adotado: sem nome, sem caminho, as conversas de antes. */
const ADOTADO = guardado("principal", "", "0.15.1", null);

/**
 * **O esquema desta versão, da build de verdade** (revisão do Codex: um retrato
 * com o número de um modelo velho não é evidência). É o que o
 * `seele_server::persistence::esquema_conhecido()` devolve — a maior
 * `version:` das `MIGRATIONS` —, lido do mesmo arquivo que o compila.
 */
const ESQUEMA_DESTA_VERSAO = (() => {
  const schema = path.join(__dirname, "..", "..", "..", "crates", "seele-server", "src", "persistence", "schema.rs");
  const versoes = [...fs.readFileSync(schema, "utf8").matchAll(/^\s*version:\s*(\d+),/gm)].map((m) => Number(m[1]));
  if (versoes.length === 0) throw new Error(`nenhuma migração em ${schema}: a bancada não inventa o esquema`);
  return Math.max(...versoes);
})();

/**
 * O que `antes_de_hospedar` devolve, na forma do backend
 * (`src/antes_de_hospedar.rs`, `AntesDeHospedar` + `mods_de_api_antiga`): o
 * esquema gravado, o desta versão, se hospedar converte, e as cópias que estão
 * no disco — histórico, nunca prova do estado atual. Uma resposta nula é o
 * comando que ainda não existe, e a tela cai no que sabe sem ele.
 */
const ANTES_DE_HOSPEDAR_015 = {
  servidor: "principal",
  banco: "seele.db",
  esquema: { Gravado: 13 },
  esquema_desta_versao: ESQUEMA_DESTA_VERSAO,
  linha_dos_dados: "0.15",
  verificado: true,
  motivo: null,
  acoes: [],
  converte: true,
  futuro: false,
  vai_copiar: true,
  copias: [{ tipo: "linha-legada", linha: "0.15", origem: "seele.db", esquema: null, data: agora - 86400 * 3,
    caminho: "lancador/dados/servidor-0.15", do_estado_atual: false }],
};

/** Abre os DETALHES TÉCNICOS e leva a rolagem até eles. */
const ABRIR_DETALHES = `const d = document.getElementById("antes-detalhes"); d.open = true; d.scrollIntoView({ block: "center" });`;

/** As sessões da multi-sessão (E3), na forma pedida em `pedidos.md`. */
/**
 * As sessões da multi-sessão (E3), na forma de `SessaoNaTela` (fronteiras.md
 * v2, §6): hospedar (Casa do Lago, no ar nesta máquina, com uma conversa nele)
 * e mais TRÊS conversas cliente, uma voltando em segundo plano. `voz` diz qual
 * tem o foco de voz; `vista`, qual a janela mostra.
 */
function sessoes({ vista = 2, voz = 2 } = {}) {
  const s = (sessao, nome, endereco, extra = {}) => ({
    carimbo: { sessao, geracao: 1 }, endereco, nome, na_vista: sessao === vista, foco_de_voz: sessao === voz,
    na_sala_de_voz: sessao === voz ? "Sala principal" : null, onde: "Outro", enlace: "online", encerrada: null,
    nao_lidas: 0, mudou: false, ...extra,
  });
  return [
    s(1, "Casa do Lago", "127.0.0.1:8383", { onde: "Guardado" }),
    s(2, "Oficina da Lia", "lia.exemplo.br:8383", { nao_lidas: vista === 2 ? 0 : 7 }),
    s(3, "Biblioteca Norte", "203.0.113.9:8383", { enlace: "reconectando", nao_lidas: 2, carimbo: { sessao: 3, geracao: 2 } }),
    s(4, "Turno da Noite", "198.51.100.4:8383", { mudou: vista !== 4 }),
  ];
}
const SESSOES = sessoes();
/**
 * O retrato de uma sessão SEM o foco de voz: fora de qualquer sala (quem perde
 * o foco sai da sala, fronteiras §5), e a abertura do áudio `em_outra_sessao`.
 */
function semVoz(server) {
  return {
    server,
    voice_rooms: [{
      id: 1, name: "Sala principal", limit: 8, password_required: false, occupied_by_us: false, channel: 1, sync: null,
      people: [{ id: 2, nickname: "Lia", speaking: false, muted: false, total_isolation: false, signal: 88, sync_band: "Nominal", is_self: false }],
    }],
    abertura_do_audio: "em_outra_sessao",
    audio_available: false,
  };
}
const MEU_SERVIDOR = { nome: "Casa do Lago", alvo: "127.0.0.1:8383" };
/** As respostas da multi-sessão, com o retrato de cada uma pelo nome dela. */
function respostasDeVariasSessoes(opcoes = {}) {
  const lista = sessoes(opcoes);
  return {
    sessoes: lista,
    meu_servidor: MEU_SERVIDOR,
    ver_sessao: { __porArg: "sessao", valores: Object.fromEntries(lista.map((s) => [s.carimbo.sessao, {
      carimbo: s.carimbo, vista: 10 + s.carimbo.sessao,
      snapshot: retrato({ server: s.nome, carimbo: s.carimbo, link: s.enlace === "reconectando" ? { InternalBattery: { remaining_seconds: 240, attempts: 2 } } : "Online" }),
    }])) },
    focar_voz: { antes: 2, agora: opcoes.vista ?? 2, saiu_da_sala: "Sala principal" },
  };
}

/** O alerta antes de atualizar (E4), na forma pedida ao compat em `pedidos.md`. */
const ATUALIZACAO_QUE_CRUZA = {
  instalada: "0.16.0",
  versao: "0.17.0",
  notas: "A conexão passa a ser WebRTC. A 0.16 continua disponível pelo launcher.",
  fronteira: { tipo: "cruza", de: "1", para: "2" },
  pede_aviso: true,
  adiada: false,
  conversas_abertas: 3,
};

// ------------------------------------------------------------------ as cenas

/** Abre a sessão desenhada, como a `telas.cjs` faz. */
const SESSAO = `
  for (const s of document.querySelectorAll("section.tela")) s.hidden = true;
  document.getElementById("tela-sessao").hidden = false;
  window.desenhar(window.__retrato);
  if (typeof sincronizarMensagens === "function") sincronizarMensagens(99);
`;

const CENAS = [
  // --- a entrada
  { nome: "app-entrada", casca: "app" },
  { nome: "app-entrada-hospedando", casca: "app",
    resposta: { meu_servidor: { nome: "CASA DO LAGO", alvo: "127.0.0.1:8383" } } },
  { nome: "app-entrada-primeira-abertura", casca: "app",
    resposta: { estado_da_primeira_abertura: { dados: { Guardados: null } } } },
  { nome: "app-entrada-detalhes", casca: "app",
    passos: `document.querySelector(".boot-detalhes").open = true; document.getElementById("boot-nome-publico").open = true;` },
  { nome: "app-conectar", casca: "app",
    resposta: { conhecidos: [
      { alvo: "lia.exemplo.br:8383", apelido: "Alex", visto_em: agora - 86400, nome: "Oficina da Lia" },
      { alvo: "203.0.113.9:8383", apelido: "alex", visto_em: agora - 600000, nome: "Biblioteca Norte" },
    ] },
    passos: `abrirServidores()` },
  // --- hospedar
  { nome: "app-hospedar-lista", casca: "app",
    resposta: { servidores_guardados: [ADOTADO, guardado("srv-2", "Oficina", "0.16.0"), guardado("srv-3", "Turno da Noite", "0.17.0")] },
    passos: `abrirHospedagem()` },
  { nome: "app-hospedar-preparar-novo", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, regras_do_icone_do_server: { limite_bytes: 262144, lado: 512 } },
    passos: `abrirPreparar()` },
  { nome: "app-hospedar-preparar-adotado", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO] },
    passos: `abrirPreparar("principal")` },
  // **O caso concreto**: o servidor da 0.15 escolhido na lista, e hospedar
  // recusado pelos MODs da API 4.
  { nome: "app-hospedar-legado-mods", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO], catalogo_de_mods: CATALOGO,
      antes_de_hospedar: ANTES_DE_HOSPEDAR_015 },
    falhas: { hospedar: RECUSA_MODS },
    passos: `servidorEscolhido = "principal"; hospedar()`, espera: 1200 },
  // O mesmo, depois de resolver os três MODs: as linhas dizem o que foi
  // feito, e o HOSPEDAR do fim se acende.
  { nome: "app-hospedar-legado-mods-resolvidos", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO], catalogo_de_mods: CATALOGO,
      antes_de_hospedar: ANTES_DE_HOSPEDAR_015 },
    falhas: { hospedar: RECUSA_MODS },
    passos: `servidorEscolhido = "principal"; await hospedar(); await new Promise((r) => setTimeout(r, 500));
      document.querySelector('#boot-saidas [data-mod="seele/estilo"] [data-saida="atualizar"]').click();
      await new Promise((r) => setTimeout(r, 200));
      for (const id of ["seele/mesa", "x/sumido"]) { document.querySelector('#boot-saidas [data-mod="' + id + '"] [data-saida="desligar"]').click(); await new Promise((r) => setTimeout(r, 200)); }
      document.getElementById("tela-antes-de-hospedar").scrollTop = 9999;`,
    espera: 900 },
  { nome: "app-hospedar-legado-mods-detalhes", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO], catalogo_de_mods: CATALOGO,
      antes_de_hospedar: ANTES_DE_HOSPEDAR_015 },
    falhas: { hospedar: RECUSA_MODS },
    passos: `servidorEscolhido = "principal"; await hospedar(); await new Promise((r) => setTimeout(r, 500)); ${ABRIR_DETALHES}`,
    espera: 700 },
  { nome: "app-minima-hospedar-legado-mods-detalhes", casca: "app", viewport: MINIMA,
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO], catalogo_de_mods: CATALOGO,
      antes_de_hospedar: ANTES_DE_HOSPEDAR_015 },
    falhas: { hospedar: RECUSA_MODS },
    passos: `servidorEscolhido = "principal"; await hospedar(); await new Promise((r) => setTimeout(r, 500)); ${ABRIR_DETALHES}`,
    espera: 700 },
  { nome: "app-hospedar-legado-mods-sem-catalogo", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO] },
    falhas: { hospedar: RECUSA_MODS, catalogo_de_mods: { NaoRespondeu: "tempo esgotado" } },
    passos: `servidorEscolhido = "principal"; hospedar()`, espera: 1200 },
  { nome: "app-hospedar-banco-do-futuro", casca: "app",
    resposta: { servidores_guardados: [guardado("srv-9", "Oficina", "0.18.0")] },
    falhas: { hospedar: { BancoDoFuturo: { gravado: ESQUEMA_DESTA_VERSAO + 2, conhecido: ESQUEMA_DESTA_VERSAO } } },
    passos: `servidorEscolhido = "srv-9"; hospedar()`, espera: 900 },
  // A pré-conferência de um servidor da 0.16 (só a conversão dos dados e a
  // versão do servidor, sem MOD nenhum) — a cena só tem o que mostrar com o
  // comando `antes_de_hospedar` do pedido.
  { nome: "app-hospedar-legado-016", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [guardado("srv-2", "Oficina", "0.16.0")],
      antes_de_hospedar: { ...ANTES_DE_HOSPEDAR_015, servidor: "srv-2", banco: "servidores/srv-2/seele.db",
        esquema: { Gravado: 17 }, linha_dos_dados: "1", copias: [] } },
    passos: `hospedarDaLista({ id: "srv-2", nome: "Oficina", versao: "0.16.0", caminho: "servidores/srv-2" })`, espera: 1000 },
  // O portão da conversão do `hospedar` fechou: a cópia não ficou pronta.
  { nome: "app-hospedar-copia-falhou", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [guardado("srv-2", "Oficina", "0.16.0")],
      antes_de_hospedar: { ...ANTES_DE_HOSPEDAR_015, servidor: "srv-2", esquema: { Gravado: 17 }, linha_dos_dados: "1", copias: [] } },
    falhas: { hospedar: { CopiaDeSeguranca: { falha: { etapa: "Copiar", motivo: "No space left on device (os error 28)" } } } },
    passos: `servidorEscolhido = "srv-2"; hospedar()`, espera: 1000 },
  // O banco não se lê: «não verificado», e nunca «pronto».
  { nome: "app-hospedar-nao-verificado", casca: "app",
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [guardado("srv-2", "Oficina", "0.16.0")],
      antes_de_hospedar: { ...ANTES_DE_HOSPEDAR_015, servidor: "srv-2", esquema: { Ilegivel: "permission denied (os error 13)" },
        linha_dos_dados: null, verificado: false, motivo: "permission denied (os error 13)", acoes: ["tentar-de-novo", "abrir-a-pasta"],
        converte: false, vai_copiar: false, copias: [] } },
    passos: `hospedarDaLista({ id: "srv-2", nome: "Oficina", versao: "0.16.0", caminho: "servidores/srv-2" })`, espera: 1000 },
  { nome: "app-hospedar-porta-ocupada", casca: "app",
    falhas: { hospedar: "PortaOcupada" }, passos: `hospedar()`, espera: 700 },
  // --- a sessão
  { nome: "app-sessao", casca: "app", sessao: true },
  // E3: hospedando a Casa do Lago, e três conversas cliente; a do Clube de
  // Leitura voltando em segundo plano.
  { nome: "app-sessao-varias", casca: "app", sessao: true,
    resposta: respostasDeVariasSessoes({ vista: 2, voz: 2 }), retrato: { server: "Oficina da Lia" }, espera: 900 },
  { nome: "app-sessao-varias-voz-noutro", casca: "app", sessao: true,
    resposta: respostasDeVariasSessoes({ vista: 4, voz: 2 }), retrato: semVoz("Turno da Noite"), espera: 900 },
  { nome: "app-sessao-varias-voz-trazida", casca: "app", sessao: true,
    resposta: respostasDeVariasSessoes({ vista: 4, voz: 4 }), retrato: { server: "Turno da Noite" }, espera: 900 },
  { nome: "app-sessao-varias-reconectando-na-vista", casca: "app", sessao: true,
    resposta: respostasDeVariasSessoes({ vista: 3, voz: 2 }),
    retrato: { ...semVoz("Biblioteca Norte"), link: { InternalBattery: { remaining_seconds: 240, attempts: 2 } } }, espera: 900 },
  { nome: "app-entrada-varias", casca: "app",
    resposta: respostasDeVariasSessoes({ vista: 2, voz: 2 }), espera: 900,
    passos: `await recarregarSessoes(); abrirMaisUmaSessao(null);` },
  // A conexão segura que não se verificou, sem prova de quem atendeu.
  { nome: "app-entrada-conexao-nao-verificada", casca: "app",
    falhas: { connect: "ConexaoSeguraNaoVerificada" }, passos: `conectar("lia.exemplo.br:8383", "Alex")`, espera: 800 },
  // No celular a falha da conexão segura é dita pela entrada da identidade (`identidade-web.js`, do
  // browser-signaling), que esta bancada não simula; o app de mesa no mínimo da janela, sim.
  { nome: "app-minima-conexao-nao-verificada", casca: "app", viewport: MINIMA,
    falhas: { connect: "ConexaoSeguraNaoVerificada" }, passos: `conectar("lia.exemplo.br:8383", "Alex")`, espera: 800 },
  { nome: "app-sessao-queda", casca: "app", sessao: true,
    retrato: { link: { InternalBattery: { remaining_seconds: 214, attempts: 3 } } } },
  { nome: "app-sessao-fim", casca: "app", sessao: true, retrato: { ended: "Kicked" } },
  { nome: "app-sessao-portaria", casca: "app", sessao: true,
    resposta: {
      estado_da_porta: { hospedando: true, aberto: true, tem_senha: true, aceita_convites: true, portaria_ligada: true, pendentes: 1, alcance: "RedeLocal" },
      pedidos_da_portaria: [{ impressao: "9f2c8a1140de77b31c05e6f92a84db30", apelido: "quem-bate", segredo: "senha", observacao: "Sou a Iria.", bateu_em: agora - 45, batidas: 2 }],
    },
    passos: `abrirPortaria()` },
  { nome: "app-sessao-moderar", casca: "app", sessao: true, passos: `abrirModeracao("2")` },
  { nome: "app-sessao-ajuda", casca: "app", sessao: true, passos: `abrirAjuda()` },
  { nome: "app-sessao-pessoas-gaveta", casca: "app", sessao: true, passos: `alternarPessoas(true)` },
  // --- autenticação, porta, perfil, palcos e consentimentos (revisão do Codex:
  //     todas as telas e camadas, também no celular)
  //     O celular é a casca do navegador (`data-casca="celular"`, com toque):
  //     o app de mesa não abre abaixo de 720×480 (`tauri.conf.json`), e um
  //     retrato dele a 390 px seria de uma tela que não existe. As cenas que
  //     são só do app (hospedar, a portaria, atualizar) ficam no FHD e no
  //     mínimo da janela, 720×480.
  ...[["app", FHD, "app"], ["web-celular", CELULAR, "celular"]].flatMap(([prefixo, viewport, casca]) => [
    { nome: `${prefixo}-auth-aperto`, casca, viewport, toque: casca === "celular",
      passos: `for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
        entrarNaAutenticacao(window.__retrato, { FirstContactVerified: { fingerprint: "9f2c8a1140de77b31c05e6f92a84db309f2c8a1140de77b31c05e6f92a84db30" } }, "lia.exemplo.br:8383");` },
    { nome: `${prefixo}-auth-espera`, casca, viewport, toque: casca === "celular",
      passos: `levarParaAEspera({ Refused: { reason: "AdmissionPending" } }, "lia.exemplo.br:8383", 0);` },
    { nome: `${prefixo}-porta`, casca, viewport, toque: casca === "celular", sessao: true,
      resposta: { estado_da_porta: { hospedando: true, aberto: false, tem_senha: true, aceita_convites: true, portaria_ligada: true, pendentes: 0, alcance: "Internet" } },
      passos: `guardarOLinkDaPorta("seele://203.0.113.9:8383/?fp=9f2c8a1140de77b31c05e6f92a84db309f2c8a1140de77b31c05e6f92a84db30&v=2.0"); abrirPorta("Alcança de qualquer rede.");` },
    { nome: `${prefixo}-perfil`, casca, viewport, toque: casca === "celular", passos: `abrirPerfil()` },
    { nome: `${prefixo}-nome-na-entrada`, casca, viewport, toque: casca === "celular", passos: `perguntarONome();` },
    { nome: `${prefixo}-compartilhar`, casca, viewport, toque: casca === "celular", sessao: true,
      resposta: { fontes_de_tela: [{ id: 1, nome: "Monitor principal", monitor: true, largura: 1920, altura: 1080 }, { id: 2, nome: "Janela: mapa.pdf", monitor: false, largura: 1280, altura: 800 }], permissao_de_tela: "Concedida" },
      passos: `abrirCompartilhar()` },
    { nome: `${prefixo}-chamada`, casca, viewport, toque: casca === "celular", sessao: true, passos: `abrirChamada()` },
    { nome: `${prefixo}-consentimento-mods`, casca, viewport, toque: casca === "celular",
      passos: `abrirAceiteDeMods("lia.exemplo.br:8383", [{ id: "lia/mesa", version: "1.2.0", hash: "h1a2b3c4d5e6", repo: "https://github.com/lia/mesa", api: 6 }, { id: "lia/dados", version: "0.4.1", hash: "h2f6e5d4c3b2", repo: null, api: 7 }], "c1", null)` },
    { nome: `${prefixo}-microfone-negado`, casca, viewport, toque: casca === "celular", resposta: { permissao_de_microfone: "NegadaParaAreaDeTrabalho", plataforma: "windows" },
      passos: `abrirServer("tela-boot").then(() => abrirSecao("secao-audio"))` },
    { nome: `${prefixo}-portaria-da-entrada`, casca, viewport, toque: casca === "celular",
      resposta: {
        meu_servidor: { nome: "Casa do Lago", alvo: "127.0.0.1:8383" },
        estado_da_porta: { hospedando: true, aberto: false, tem_senha: true, aceita_convites: false, portaria_ligada: true, pendentes: 1, alcance: "RedeLocal", hospedagem: { servidor: "principal", geracao: 1, nome: "Casa do Lago" } },
        pedidos_da_portaria: [{ impressao: "9f2c8a1140de77b31c05e6f92a84db30", apelido: "quem-bate", segredo: "senha", observacao: "Sou a Iria.", bateu_em: agora - 45, batidas: 2, decidido_em: null, admitido: null }],
      },
      passos: `await atualizarPorta(); document.getElementById("portaria-batendo-ver").click();`, espera: 900 },
  ].filter((c) => !(casca === "celular" && /-porta$|portaria|compartilhar|microfone-negado/.test(c.nome)))),
  // --- configurações
  ...["secao-audio", "secao-atalhos", "secao-aparencia", "secao-identidade", "secao-mods", "secao-versoes", "secao-malha", "secao-atualizacao"]
    .map((secao) => ({ nome: `app-config-${secao.slice(6)}`, casca: "app", passos: `abrirServer("tela-boot").then(() => abrirSecao("${secao}"))` })),
  { nome: "app-config-atualizacao-achada", casca: "app",
    resposta: { procurar_atualizacao: { instalada: "0.17.0", versao: "0.17.1", notas: "Correções na sala de voz.",
      fronteira: { tipo: "mesma", linha: "2" }, pede_aviso: false, adiada: false } },
    passos: `abrirServer("tela-boot").then(() => { abrirSecao("secao-atualizacao"); return procurarAtualizacao(); })` },
  // E4: a atualização que cruza a quebra, antes de instalar.
  { nome: "app-config-atualizacao-quebra", casca: "app",
    resposta: { procurar_atualizacao: ATUALIZACAO_QUE_CRUZA },
    passos: `abrirServer("tela-boot").then(() => { abrirSecao("secao-atualizacao"); return procurarAtualizacao(); }).then(() => document.getElementById("atualizacao-instalar").click())`,
    espera: 900 },
  { nome: "app-config-atualizacao-quebra-adiada", casca: "app",
    resposta: { procurar_atualizacao: { ...ATUALIZACAO_QUE_CRUZA, adiada: true } },
    passos: `abrirServer("tela-boot").then(() => { abrirSecao("secao-atualizacao"); return procurarAtualizacao(); })` },
  { nome: "app-minima-atualizacao-quebra", casca: "app", viewport: MINIMA,
    resposta: { procurar_atualizacao: ATUALIZACAO_QUE_CRUZA },
    passos: `abrirServer("tela-boot").then(() => { abrirSecao("secao-atualizacao"); return procurarAtualizacao(); }).then(() => document.getElementById("atualizacao-instalar").click())`,
    espera: 900 },
  // --- a web
  { nome: "web-larga-entrada", casca: "web", viewport: FHD },
  { nome: "web-larga-config", casca: "web", viewport: FHD, passos: `abrirServer("tela-boot")` },
  { nome: "web-celular-entrada", casca: "web", viewport: CELULAR, toque: true },
  { nome: "web-celular-sessao", casca: "celular", viewport: CELULAR, toque: true, sessao: true },
  { nome: "web-celular-sessao-varias", casca: "celular", viewport: CELULAR, toque: true, sessao: true,
    resposta: respostasDeVariasSessoes({ vista: 4, voz: 2 }), retrato: semVoz("Turno da Noite"), espera: 900 },
  { nome: "web-celular-sessao-varias-servidor", casca: "celular", viewport: CELULAR, toque: true, sessao: true,
    resposta: respostasDeVariasSessoes({ vista: 4, voz: 2 }), retrato: semVoz("Turno da Noite"), espera: 900,
    passos: `CELULAR.ajudante("abrirAba")("servidor")` },
  { nome: "app-minima-hospedar-legado-mods", casca: "app", viewport: MINIMA,
    resposta: { linhas_para_hospedar: LINHAS_017, servidores_guardados: [ADOTADO], catalogo_de_mods: CATALOGO, antes_de_hospedar: ANTES_DE_HOSPEDAR_015 },
    falhas: { hospedar: RECUSA_MODS },
    passos: `servidorEscolhido = "principal"; hospedar()`, espera: 1200 },
];

// ------------------------------------------------------------------ a corrida

function engine(nome) {
  // Também pela linha de comando (`--playwright=<caminho>`), como a `playwright.cjs`.
  const daLinha = process.argv.find((argumento) => argumento.startsWith("--playwright="))?.slice("--playwright=".length);
  const escolhido = daLinha || process.env.PLAYWRIGHT;
  const pw = escolhido ? require(escolhido) : require("playwright");
  return pw[nome];
}

async function rodarCena(navegador, url, cena, pasta) {
  const viewport = cena.viewport ?? FHD;
  const contexto = await navegador.newContext({
    viewport,
    hasTouch: Boolean(cena.toque),
    isMobile: Boolean(cena.toque) && navegador.browserType().name() !== "firefox",
    deviceScaleFactor: 1,
  });
  const aba = await contexto.newPage();
  const erros = [];
  let retratando = false;
  aba.on("pageerror", (e) => erros.push(String(e.message).slice(0, 200)));
  aba.on("console", (m) => {
    if (retratando) return;
    if (m.type() === "error" && !m.text().startsWith("Failed to load resource")) erros.push(m.text().slice(0, 200));
  });
  // O 404 com o endereço, e não a frase genérica do console.
  aba.on("response", (r) => {
    if (r.status() >= 400) erros.push(`${r.status()} ${new URL(r.url()).pathname}`);
  });

  if (cena.casca !== "web") {
    // Os aparelhos e o resto que a configuração lê na abertura: sem eles, a
    // lista de microfones lança `null.map` e a seção não desenha — um defeito
    // da bancada, e não da tela.
    const tabela = respostas({
      microfones: [{ id: "mic", name: "Microfone interno", is_default: true }],
      saidas: [{ id: "saida", name: "Saída padrão", is_default: true }],
      ...(cena.resposta ?? {}),
    });
    const foto = retrato(cena.retrato ?? {});
    tabela.snapshot = foto;
    await aba.addInitScript(({ tabela, falhas, foto, casca }) => {
      window.__retrato = foto;
      window.__falhas = falhas;
      // A marca do celular antes do primeiro script da página, como a
      // `bancada-do-celular.py` faz: o duble a põe, e a ponte não a troca.
      // O `documentElement` ainda não existe quando isto roda, e o observador
      // a escreve no instante em que ele nasce.
      if (casca === "celular") {
        new MutationObserver((_, observador) => {
          if (!document.documentElement) return;
          document.documentElement.dataset.casca = "celular";
          observador.disconnect();
        }).observe(document, { childList: true });
      }
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            window.__chamadas = window.__chamadas ?? [];
            window.__chamadas.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
            if (Object.hasOwn(window.__falhas, cmd)) throw structuredClone(window.__falhas[cmd]);
            const r = tabela[cmd];
            // Uma resposta por argumento (`ver_sessao` de cada sessão).
            if (r && typeof r === "object" && r.__porArg) return structuredClone(r.valores[args?.[r.__porArg]] ?? null);
            if (Object.hasOwn(tabela, cmd)) return structuredClone(r);
            window.__semResposta = window.__semResposta ?? [];
            if (!window.__semResposta.includes(cmd)) window.__semResposta.push(cmd);
            return null;
          },
        },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ onCloseRequested: () => {}, isMaximized: async () => false }) },
      };
    }, { tabela, falhas: cena.falhas ?? {}, foto, casca: cena.casca });
  }

  await aba.goto(url);
  await aba.waitForTimeout(700);
  if (cena.sessao) {
    await aba.evaluate(SESSAO).catch((e) => erros.push("sessão: " + e.message));
    await aba.waitForTimeout(400);
  }
  if (cena.passos) {
    await aba.evaluate(`(async () => { ${cena.passos.includes(";") ? cena.passos : "await " + cena.passos} })()`)
      .catch((e) => erros.push("passos: " + e.message));
  }
  await aba.waitForTimeout(cena.espera ?? 600);
  const arquivo = path.join(pasta, `${cena.nome}.png`);
  // `caret: "initial"`: o padrão do Playwright esconde o cursor com uma folha
  // `<style>` injetada, que a CSP do produto recusa — e o WebKit escrevia a
  // recusa no console de toda cena, como se fosse da casca.
  //
  // E o WebKit do Playwright injeta uma folha no próprio retrato, mesmo assim.
  // Medido em 24/10 (Playwright 1.62.1, WebKit dele): com a página servida por
  // `servir()`, sem duble e sem script nenhum nosso, o console fica limpo até o
  // `screenshot`, e a recusa «Refused to apply a stylesheet…» aparece entre o
  // antes e o depois dele, com `caret: "initial"`. O que o console diz durante
  // o retrato é do Playwright, e não entra no relatório.
  retratando = true;
  await aba.screenshot({ path: arquivo, fullPage: false, caret: "initial" });
  const semResposta = await aba.evaluate(() => window.__semResposta ?? []).catch(() => []);
  const marca = await aba.evaluate(() => document.documentElement.dataset.casca ?? "app").catch(() => "?");
  await contexto.close();
  return { cena: cena.nome, arquivo, viewport: `${viewport.width}x${viewport.height}`, casca: marca, erros: [...new Set(erros)], semResposta };
}

async function principal() {
  const args = process.argv.slice(2);
  const comWebkit = args.includes("--webkit");
  const [pasta = path.join(require("node:os").tmpdir(), "seele-navegacao"), filtro = ""] = args.filter((a) => !a.startsWith("--"));
  fs.mkdirSync(pasta, { recursive: true });
  const servidor = servir();
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const url = `http://127.0.0.1:${servidor.address().port}/`;
  const motores = comWebkit ? ["chromium", "webkit"] : ["chromium"];
  const relatorio = [];
  try {
    for (const motor of motores) {
      const navegador = await engine(motor).launch({ headless: true });
      const destino = motores.length > 1 ? path.join(pasta, motor) : pasta;
      fs.mkdirSync(destino, { recursive: true });
      for (const cena of CENAS.filter((c) => c.nome.includes(filtro))) {
        const r = await rodarCena(navegador, url, cena, destino);
        r.motor = motor;
        relatorio.push(r);
        console.log(`${motor} ${r.cena} ${r.viewport} casca=${r.casca}` +
          (r.erros.length ? `  ERROS: ${r.erros.join(" · ")}` : ""));
      }
      await navegador.close();
    }
  } finally {
    servidor.close();
  }
  fs.writeFileSync(path.join(pasta, "relatorio.json"), JSON.stringify(relatorio, null, 2));
  // **Um retrato com erro de página reprova a volta**: ele é o retrato do
  // defeito, e não da tela. O 404 do `.wasm` da página não conta: a casca web
  // sem o `cargo xtask web` não o tem, e a entrada desenha sem ele.
  const comErro = relatorio.filter((r) => r.erros.some((e) => !/^404 \/seele_web\.wasm$/.test(e)));
  if (comErro.length > 0) {
    console.error(`cenas com erro de página: ${comErro.map((r) => `${r.motor}/${r.cena}`).join(", ")}`);
    process.exitCode = 1;
  }
}

module.exports = { CENAS, principal };

if (require.main === module) principal().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});
