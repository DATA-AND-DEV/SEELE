// **Cargos e permissões, na tela** (E6 da 0.17, cargos-ui).
//
// Com o HTML e os scripts reais de `ui/`, num navegador de verdade (Chromium,
// e WebKit com `--webkit`). Do outro lado da ponte não há Rust: há dois
// servidores de mentira, A e B, com cargos diferentes, que aplicam as regras
// de `cargos-decisoes.md` (a soma das concessões, a restrição vencendo, a
// hierarquia por posição, «só concede o que tem» e o ganho efetivo). Eles
// respondem como a casca vai responder: `NaSessao { carimbo, valor }`, e a
// recusa nomeada `CargoRecusado`.
//
// O que ela prova é a TELA: que a hierarquia aparece em ordem, que o que não
// se administra fica desligado com o motivo escrito, que a prévia diz quem
// ganha o quê antes de salvar, que a recusa do servidor aparece com a frase
// dele, que a explicação diz qual cargo decidiu, que os cargos de A nunca
// aparecem nem agem em B, e que tudo cabe e se usa a 1920x1080 e a 390x844.
// Ela NÃO prova que o servidor recusa: isso é dos testes do cargos-backend,
// contra o servidor real.
//
// # Cada guarda é provado por reversão
//
// `--provar-reversoes` roda cada cena com um trecho do `cargos.js` trocado pelo
// servidor de arquivos desta bancada (o conserto desfeito, em texto), e exige
// que a cena REPROVE. Um guarda que continua verde com o conserto desfeito não
// guarda nada.
//
// # Rodar
//
//   node apps/seele-app/bancada/cargos.cjs [filtro] [--webkit] [--provar-reversoes] [--sem-capturas]
//
// O Playwright vem de `playwright.cjs` (a variável `PLAYWRIGHT`, ou o
// `node_modules` do repositório). As capturas vão para
// `docs/evidencias/ui-0.17/cargos/<motor>/`.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { respostas, retrato } = require("./telas.cjs");
const { chromium } = require("./playwright.cjs");
const { cabecalhosDaOrigem } = require("./cabecalhos-da-origem.cjs");

const RAIZ = path.resolve(__dirname, "../ui");
const EVIDENCIAS = path.resolve(__dirname, "../../../docs/evidencias/ui-0.17/cargos");
const FHD = { width: 1920, height: 1080 };
const CELULAR = { width: 390, height: 844 };

const argumentos = process.argv.slice(2);
const WEBKIT = argumentos.includes("--webkit");
const PROVAR = argumentos.includes("--provar-reversoes");
const SEM_CAPTURAS = argumentos.includes("--sem-capturas") || PROVAR;
const FILTRO = argumentos.find((a) => !a.startsWith("--")) ?? "";
const MOTOR = WEBKIT ? "webkit" : "chromium";

// ------------------------------------------------------------- as reversões

/**
 * O conserto desfeito, em texto: `de` some do `cargos.js` servido e `para`
 * entra. Se `de` não estiver no arquivo, a reversão é inválida e a bancada
 * diz isso — uma reversão que não casa não reverte nada.
 */
const REVERSOES = {
  "motivo-do-cargo": {
    cena: "hierarquia",
    de: '$("cargos-editor-motivo").hidden = gere;',
    para: '$("cargos-editor-motivo").hidden = true;',
  },
  "concede-so-o-que-tem": {
    cena: "permissoes",
    de: '(valor === "concede" && !concedivel && atual !== "concede")',
    para: "false",
  },
  "salvar-com-ganho-alem": {
    cena: "ganho-alem",
    de: '$("cargos-salvar").disabled = !gere || !rascunho.sujo || passaDoAutor || rascunho.desatualizado || lendoPrevia;',
    para: '$("cargos-salvar").disabled = !gere || !rascunho.sujo || rascunho.desatualizado || lendoPrevia;',
  },
  "recusa-nomeada": {
    cena: "recusa",
    de: "return frase({ permissao, pessoa }) + doServidor;",
    para: 'return frase({ permissao: "", pessoa: "" }) + doServidor;',
  },
  "atribuir-sem-confirmar": {
    cena: "pessoas",
    de: "cargosPedirAtribuicao(estado, estado.pessoa, cargo, caixa.checked);",
    para: "cargosPedirAtribuicao(estado, estado.pessoa, cargo, caixa.checked).then(() => cargosConfirmarAtribuicao(estado));",
  },
  "desconhecida-concede": {
    cena: "explicar",
    de: 'const pode = e.pode && conhecida && e.regra !== "desconhecida";',
    para: "const pode = e.pode;",
  },
  // As duas proteções juntas: o gancho da troca de vista (que o ui-ux chama
  // em camada-sessoes.js) e a rede do vigia. Desfazer só uma deixa a outra
  // segurando, e a reversão não provaria nada.
  "isolamento-da-vista": {
    cena: "isolamento",
    trocas: [
      ["if (cargosNaTela && (cargosNaTela.sessao !== vista || !cargosHaOndeMostrar())) {", "if (false) {"],
      ["function cargosAoSairDaVista(sessao) {\n  const estado = cargosNaTela;", "function cargosAoSairDaVista(sessao) {\n  const estado = null;"],
    ],
  },
  "reaplicar-por-cima": {
    cena: "concorrencia",
    de: 'if (cargosNomeDaRecusa(falha) === "RevisaoVelha") {',
    para: 'if (cargosNomeDaRecusa(falha) === "RevisaoVelha") { await cargosReler(estado); cargosRefazer(estado); estado.rascunho = rascunho; return cargosSalvar(estado); } if (false) {',
  },
  "destaque-do-que-mudou": {
    cena: "concorrencia",
    de: "      if (fora) {",
    para: "      if (false) {",
  },
  "verbo-sem-esperar-o-desfecho": {
    cena: "recusa",
    de: "await cargosEsperarDesfecho(estado, antes, aplicar);",
    para: "aplicar(null);",
  },
  "verbo-em-voo-esquecido": {
    cena: "isolamento",
    de: "  if (emVoo) {",
    para: "  if (false) {",
  },
  "previa-velha-sem-reler": {
    cena: "previa-velha",
    de: 'if (resultado?.recusa && cargosNomeDaRecusa(resultado.recusa) === "RevisaoVelha") {',
    para: "if (false) {",
  },
  "evento-ignorado": {
    cena: "evento",
    de: 'if (evento && typeof evento === "object" && evento.CargosChanged) {',
    para: "if (false) {",
  },
  "retrato-ignorado": {
    cena: "revisao-do-retrato",
    de: "const marca = desenhado?.cargos_revision;",
    para: "const marca = undefined;",
  },
  "sondagem-de-volta": {
    cena: "sem-sondagem",
    de: "if (marca === null || marca === estado.marcaVista || estado.mandando) return;",
    para: "if (estado.mandando) return;",
  },
  "recusa-sem-numero": {
    cena: "recusa-repetida",
    de: "return recusa.seq !== null ? `seq:${recusa.seq}` : JSON.stringify(recusa.motivo);",
    para: "return JSON.stringify(recusa.motivo);",
  },
  "aceite-pela-revisao": {
    cena: "aceite-alheio",
    de: "return antes.efeito ? antes.efeito(quadro) === true : quadro.revisao !== antes.revisao;",
    para: "return quadro.revisao !== antes.revisao;",
  },
  "camada-sobre-a-chegada": {
    cena: "chegada",
    de: "return daSessao && !chegandoOuSaindo;",
    para: "return daSessao;",
  },
  "restricao-tirada-parece-inofensiva": {
    cena: "recusa",
    de: "const avisoLocal = tiradas.length > 0 ? FRASES_DOS_CARGOS.previa.devolve(cargosNomesDasPermissoes(estado, tiradas)) : null;",
    para: "const avisoLocal = null;",
  },
};

/** As trocas de uma reversão: uma só (`de`/`para`) ou várias (`trocas`). */
function trocasDe(reversao) {
  return reversao.trocas ?? [[reversao.de, reversao.para]];
}

/** O servidor de arquivos de `ui/`, com a CSP do produto e, se pedida, uma reversão. */
function servir(reversao) {
  const tipos = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".woff2": "font/woff2", ".png": "image/png", ".svg": "image/svg+xml" };
  return http.createServer((req, res) => {
    const pedido = decodeURIComponent(req.url.split("?")[0]);
    const alvo = path.join(RAIZ, pedido === "/" ? "index.html" : pedido);
    if (!alvo.startsWith(RAIZ) || !fs.existsSync(alvo) || fs.statSync(alvo).isDirectory()) {
      res.statusCode = 404;
      res.end("nao encontrado");
      return;
    }
    // Os cabeçalhos da origem de verdade, lidos de `web/_headers` (a fonte
    // única, `tests/web.rs`): a camada roda sob a mesma política da página.
    for (const [nome, valor] of Object.entries(cabecalhosDaOrigem(pedido))) res.setHeader(nome, valor);
    res.setHeader("Content-Type", tipos[path.extname(alvo)] ?? "application/octet-stream");
    let corpo = fs.readFileSync(alvo);
    if (reversao && path.basename(alvo) === "cargos.js") {
      let texto = corpo.toString("utf8");
      for (const [de, para] of trocasDe(reversao)) texto = texto.replace(de, para);
      corpo = Buffer.from(texto, "utf8");
    }
    res.end(corpo);
  });
}

// ------------------------------------------------------- os dois servidores

const P = (nome, grupo) => ({ chave: nome, grupo });
const CATALOGO_BASE = [
  P("ReadChannel", "conversa"), P("WriteChannel", "conversa"), P("AttachFile", "conversa"),
  P("ViewVoiceRoom", "voz"), P("EnterVoiceRoom", "voz"), P("Speak", "voz"), P("ShareScreen", "voz"),
  P("RemoveMessage", "moderacao"), P("MovePerson", "moderacao"), P("Kick", "moderacao"), P("Ban", "moderacao"),
  P("ManageVoiceRooms", "servidor"), P("ManageRoles", "servidor"), P("AdministerServer", "servidor"), P("Administrator", "servidor"),
];
const MESA = { id: "seele/mesa", titulo: "Mesa" };
const CATALOGO_A = [
  ...CATALOGO_BASE,
  { chave: { mod_id: "seele/mesa", acao: "fechar-mesa" }, grupo: "mod", mod_: MESA, nome: "Fechar a mesa", descricao: "Encerrar a partida para todos na sala." },
  { chave: { mod_id: "seele/mesa", acao: "mover-peca" }, grupo: "mod", mod_: MESA, nome: "Mover peças", descricao: "Mexer nas peças do tabuleiro." },
];
const TODAS_A = CATALOGO_A.map((p) => (typeof p.chave === "string" ? p.chave : `mod:${p.chave.mod_id}#${p.chave.acao}`));

/**
 * O servidor A: Alex é Moderação (posição 5), e não é o proprietário.
 * Acima dele, o Comandante; abaixo, Guarda (concede BANIR, que Alex não tem),
 * Membro, Silenciados (restringe FALAR e a tela) e Quarentena (restringe
 * BANIR e EXPULSAR). Bia tem Guarda e Quarentena: tirar a restrição dela daria
 * BANIR a Bia, que Alex não tem — o caso do ganho efetivo.
 */
const SERVIDOR_A = {
  nome: "Oficina da Lia",
  revisao: 7,
  eu: { pessoa: 1, proprietario: false, posicao_mais_alta: 5, gere_cargos: true },
  todas: TODAS_A,
  catalogo: CATALOGO_A,
  cargos: [
    { id: 4, nome: "Comandante", posicao: 6, concede: ["Administrator"], restringe: [] },
    { id: 3, nome: "Moderação", posicao: 5, concede: ["Kick", "MovePerson", "RemoveMessage", "ManageRoles", "mod:seele/mesa#mover-peca"], restringe: [] },
    { id: 6, nome: "Guarda", posicao: 4, concede: ["Ban", "Kick"], restringe: [] },
    { id: 2, nome: "Membro", posicao: 3, concede: ["ReadChannel", "WriteChannel", "AttachFile", "ViewVoiceRoom", "EnterVoiceRoom", "Speak", "ShareScreen"], restringe: [] },
    { id: 1, nome: "Silenciados", posicao: 2, concede: [], restringe: ["Speak", "ShareScreen"] },
    { id: 5, nome: "Quarentena", posicao: 1, concede: [], restringe: ["Ban", "Kick"] },
    { id: 0, nome: "@everyone", posicao: 0, concede: ["ViewVoiceRoom", "ReadChannel"], restringe: [] },
  ],
  pessoas: [
    { pessoa: 2, apelido: "Lia", proprietario: true, cargos: [4] },
    { pessoa: 1, apelido: "Alex", proprietario: false, cargos: [3, 2] },
    { pessoa: 6, apelido: "Teo", proprietario: false, cargos: [4] },
    { pessoa: 3, apelido: "Rafa", proprietario: false, cargos: [2, 1] },
    { pessoa: 4, apelido: "Bia", proprietario: false, cargos: [2, 6, 5] },
  ],
};

/** O servidor B: Alex é o proprietário. Outros cargos, outras pessoas. */
const SERVIDOR_B = {
  nome: "Casa do Lago",
  revisao: 2,
  eu: { pessoa: 1, proprietario: true, posicao_mais_alta: 99, gere_cargos: true },
  todas: CATALOGO_BASE.map((p) => p.chave),
  catalogo: CATALOGO_BASE,
  cargos: [
    { id: 11, nome: "Banda", posicao: 2, concede: ["Speak", "ShareScreen", "EnterVoiceRoom"], restringe: [] },
    { id: 12, nome: "Plateia", posicao: 1, concede: ["EnterVoiceRoom"], restringe: ["Speak"] },
    { id: 0, nome: "@everyone", posicao: 0, concede: ["ViewVoiceRoom", "ReadChannel", "WriteChannel"], restringe: [] },
  ],
  pessoas: [
    { pessoa: 1, apelido: "Alex", proprietario: true, cargos: [] },
    { pessoa: 7, apelido: "Gui", proprietario: false, cargos: [11] },
  ],
};

/**
 * Os servidores de mentira: as regras de `cargos-decisoes.md` sobre o estado
 * de cada um. Roda na página (um servidor por sessão daquela página) ou no
 * Node (um servidor A só para duas páginas: dois gestores, duas sessões).
 * Tudo que usa vem pelos argumentos, porque a função é levada como texto.
 *
 * `responder(cmd, args, eu)`: `eu` é quem pede (o gestor daquela sessão).
 */
function motorDosServidores(servidores, opcoes) {
  // `Permissao` em serde, dos dois lados: `{"Produto":"Kick"}` e
  // `{"DeMod":{"mod_id":…,"acao":…}}`. Por dentro, a forma gravada.
  const chave = (c) => {
    if (c?.DeMod) return `mod:${c.DeMod.mod_id}#${c.DeMod.acao}`;
    if (c?.Produto) return c.Produto;
    return String(c);
  };
  const deMod = (c) => {
    const resto = String(c).slice(4);
    const corte = resto.lastIndexOf("#");
    return { mod_id: resto.slice(0, corte), acao: resto.slice(corte + 1) };
  };
  const paraOFio = (c) => (String(c).startsWith("mod:") ? { DeMod: deMod(c) } : { Produto: c });
  // A recusa como a casca a rejeita: `CargoRecusado { motivo: RecusaDeCargo }`,
  // a variante sem campo como texto (serde).
  const recusa = (variante, dentro) => ({ CargoRecusado: { motivo: dentro === undefined ? variante : { [variante]: dentro } } });

  function efetivas(srv, pessoaId, cargos = srv.cargos) {
    const p = srv.pessoas.find((x) => x.pessoa === pessoaId);
    if (!p) return new Set();
    if (p.proprietario) return new Set(srv.todas);
    const meus = cargos.filter((c) => c.posicao === 0 || p.cargos.includes(c.id));
    const C = new Set(meus.flatMap((c) => c.concede));
    const R = new Set(meus.flatMap((c) => c.restringe));
    const todas = C.has("Administrator") && !R.has("Administrator") ? new Set(srv.todas) : C;
    return new Set([...todas].filter((k) => !R.has(k)));
  }
  function posicaoMaisAlta(srv, pessoaId) {
    const p = srv.pessoas.find((x) => x.pessoa === pessoaId);
    if (p?.proprietario) return Infinity;
    return Math.max(0, ...srv.cargos.filter((c) => p?.cargos.includes(c.id)).map((c) => c.posicao));
  }
  function gerivel(srv, cargo) {
    if (srv.eu.proprietario) return [true, null];
    if (!srv.eu.gere_cargos) return [false, "sem_gerir_cargos"];
    if (cargo.posicao === srv.eu.posicao_mais_alta) return [false, "igual_ao_seu"];
    if (cargo.posicao > srv.eu.posicao_mais_alta) return [false, "acima_do_seu"];
    return [true, null];
  }
  /**
   * O que `pedir_cargos` devolve: `CargosDaSessao { quadro, suas }`, com o
   * `QuadroDeCargos` e o `SuasPermissoes` do seele-proto — sem `gerivel` nem
   * cor, que o modelo não tem. Os `apelidos` são o acréscimo pedido ao F.
   */
  function pedirCargos(srv) {
    const minhas = efetivas(srv, srv.eu.pessoa);
    return {
      quadro: {
        cargos: [...srv.cargos].sort((a, b) => b.posicao - a.posicao).map((c) => ({
          id: c.id, nome: c.nome, posicao: c.posicao,
          concede: c.concede.map(paraOFio), restringe: c.restringe.map(paraOFio),
        })),
        atribuicoes: srv.pessoas.filter((p) => p.cargos.length > 0).map((p) => ({ pessoa: p.pessoa, cargos: [...p.cargos] })),
        proprietario: srv.pessoas.find((p) => p.proprietario)?.pessoa ?? null,
        de_mods: srv.catalogo.filter((p) => p.grupo === "mod").map((p) => ({ chave: p.chave, nome: p.nome })),
        revisao: srv.revisao,
        dono_declarado: srv.donoDeclarado !== false,
      },
      // `CargosNaTela` (cargos-api Δ v2): os apelidos e a última recusa ao
      // lado do quadro, e não dentro dele.
      apelidos: Object.fromEntries(srv.pessoas.map((p) => [p.pessoa, p.apelido])),
      // A recusa numerada: o `seq` sobe a cada recusa, inclusive a mesma de novo.
      ultima_recusa: srv.ultimaRecusa ? { seq: srv.recusaSeq, motivo: srv.ultimaRecusa } : null,
      // A marca da sessão (o `cargos_revision` do retrato) nesta leitura.
      marca: srv.marca ?? 0,
      suas: {
        permissoes: [...minhas].filter((k) => !k.startsWith("mod:")),
        de_mods: [...minhas].filter((k) => k.startsWith("mod:")).map(deMod),
        proprietario: srv.eu.proprietario === true,
        altura: srv.eu.proprietario ? "Proprietario" : { Cargo: posicaoMaisAlta(srv, srv.eu.pessoa) },
      },
    };
  }
  /** O que cada pessoa ganha e perde com um `PedidoDeCargo`: a função que também recusa. */
  function prever(srv, pedido) {
    const cargos = structuredClone(srv.cargos);
    const pessoas = structuredClone(srv.pessoas);
    const depois = { ...srv, cargos, pessoas };
    if (pedido.Editar) {
      const c = cargos.find((x) => x.id === pedido.Editar.cargo);
      c.concede = pedido.Editar.concede.map(chave);
      c.restringe = pedido.Editar.restringe.map(chave);
    } else if (pedido.Apagar) {
      depois.cargos = cargos.filter((x) => x.id !== pedido.Apagar.cargo);
    } else if (pedido.Atribuir) {
      pessoas.find((p) => p.pessoa === pedido.Atribuir.pessoa).cargos.push(pedido.Atribuir.cargo);
    } else if (pedido.Revogar) {
      const p = pessoas.find((x) => x.pessoa === pedido.Revogar.pessoa);
      p.cargos = p.cargos.filter((id) => id !== pedido.Revogar.cargo);
    }
    const autor = efetivas(srv, srv.eu.pessoa);
    const saida = [];
    for (const p of srv.pessoas) {
      if (p.proprietario) continue;
      const antes = efetivas(srv, p.pessoa);
      const agora = efetivas(depois, p.pessoa, depois.cargos);
      const ganha = [...agora].filter((k) => !antes.has(k));
      const perde = [...antes].filter((k) => !agora.has(k));
      if (ganha.length || perde.length) {
        saida.push({
          pessoa: p.pessoa, apelido: p.apelido,
          ganha: ganha.map(paraOFio), perde: perde.map(paraOFio),
          alem_do_autor: srv.eu.proprietario ? [] : ganha.filter((k) => !autor.has(k)).map(paraOFio),
        });
      }
    }
    return { pessoas: saida, depois };
  }
  function conferirGanho(srv, pedido) {
    const { pessoas, depois } = prever(srv, pedido);
    const alem = pessoas.find((p) => p.alem_do_autor.length > 0);
    if (alem) throw recusa("GanhoAlemDaSuaAutoridade", { pessoa: alem.pessoa, permissao: alem.alem_do_autor[0] });
    return depois;
  }
  /** A transação: toda escrita confere a revisão do conjunto e GERIR CARGOS de agora. */
  function conferirEscrita(srv, a) {
    if (!srv.eu.gere_cargos) throw recusa("SemPermissao");
    if (a.revisao !== srv.revisao) throw recusa("RevisaoVelha", { atual: srv.revisao });
  }
  /** `explicar_permissoes`: uma `Explicacao { tem, motivo }` por permissão (seele-proto). */
  function explicar(srv, pessoaId) {
    const p = srv.pessoas.find((x) => x.pessoa === pessoaId);
    const meus = srv.cargos.filter((c) => c.posicao === 0 || p.cargos.includes(c.id));
    const linhas = srv.todas.map((k) => {
      const permissao = paraOFio(k);
      if (p.proprietario) return { permissao, tem: true, motivo: "Proprietario" };
      const restringem = meus.filter((c) => c.restringe.includes(k)).map((c) => c.id);
      const concedem = meus.filter((c) => c.concede.includes(k)).map((c) => c.id);
      const viaAdmin = meus.filter((c) => c.concede.includes("Administrator")).map((c) => c.id);
      if (restringem.length) return { permissao, tem: false, motivo: { Restringida: { por: restringem } } };
      if (concedem.length) return { permissao, tem: true, motivo: { Concedida: { por: concedem } } };
      if (viaAdmin.length) return { permissao, tem: true, motivo: { PeloAdministrador: { por: viaAdmin } } };
      return { permissao, tem: false, motivo: "NinguemConcede" };
    });
    // Uma permissão que esta versão do app não conhece, que o servidor diz
    // conceder: a tela tem de mostrá-la como NÃO PODE.
    linhas.push({ permissao: { Produto: "Teletransportar" }, tem: true, motivo: { Concedida: { por: [2] } } });
    return linhas;
  }

  const comandos = {
    pedir_cargos: (srv) => pedirCargos(srv),
    explicar_permissoes: (srv, a) => explicar(srv, a.pessoa),
    // A prévia é o `decidir` do servidor e depois a conta do ganho: a recusa
    // que o verbo teria (inclusive o ganho além) volta como ERRO
    // `{ CargoRecusado: { motivo } }`, e só sem recusa vem a lista.
    prever_cargo: (srv, a) => {
      if (opcoes.semPrevia) throw "Unsupported";
      const [verbo, campos] = Object.entries(a.pedido)[0];
      comandos[DO_PEDIDO[verbo]](structuredClone(srv), { ...campos, revisao: a.revisao });
      return { pessoas: prever(srv, a.pedido).pessoas };
    },
    editar_cargo: (srv, a) => {
      const c = srv.cargos.find((x) => x.id === a.cargo);
      if (!c) throw recusa("CargoNaoExiste");
      conferirEscrita(srv, a);
      if (!gerivel(srv, c)[0]) throw recusa("CargoAcimaDeVoce");
      const autor = efetivas(srv, srv.eu.pessoa);
      const nova = a.concede.map(chave).find((k) => !c.concede.includes(k) && !autor.has(k));
      if (!srv.eu.proprietario && nova) throw recusa("NaoPodeConceder", { permissao: paraOFio(nova) });
      conferirGanho(srv, { Editar: a });
      c.nome = a.nome;
      c.concede = a.concede.map(chave);
      c.restringe = a.restringe.map(chave);
      srv.revisao += 1;
      return null;
    },
    criar_cargo: (srv, a) => {
      conferirEscrita(srv, a);
      const id = Math.max(...srv.cargos.map((c) => c.id)) + 1;
      for (const c of srv.cargos) if (c.posicao >= 1) c.posicao += 1;
      srv.cargos.push({ id, nome: a.nome, posicao: 1, concede: [], restringe: [] });
      srv.revisao += 1;
      return id;
    },
    mover_cargo: (srv, a) => {
      conferirEscrita(srv, a);
      const c = srv.cargos.find((x) => x.id === a.cargo);
      const outro = srv.cargos.find((x) => x.posicao === a.posicao);
      if (!outro || a.posicao === 0) throw recusa("PosicaoInvalida");
      if (!gerivel(srv, c)[0] || !gerivel(srv, outro)[0]) throw recusa("CargoAcimaDeVoce");
      [outro.posicao, c.posicao] = [c.posicao, outro.posicao];
      srv.revisao += 1;
      return null;
    },
    apagar_cargo: (srv, a) => {
      const c = srv.cargos.find((x) => x.id === a.cargo);
      if (c.posicao === 0) throw recusa("CargoProtegido");
      conferirEscrita(srv, a);
      if (!gerivel(srv, c)[0]) throw recusa("CargoAcimaDeVoce");
      const depois = conferirGanho(srv, { Apagar: a });
      srv.cargos = depois.cargos;
      for (const p of srv.pessoas) p.cargos = p.cargos.filter((id) => id !== a.cargo);
      srv.revisao += 1;
      return null;
    },
    atribuir_cargo: (srv, a) => {
      conferirEscrita(srv, a);
      conferirGanho(srv, { Atribuir: a });
      srv.pessoas.find((p) => p.pessoa === a.pessoa).cargos.push(a.cargo);
      srv.revisao += 1;
      return null;
    },
    revogar_cargo: (srv, a) => {
      conferirEscrita(srv, a);
      conferirGanho(srv, { Revogar: a });
      const p = srv.pessoas.find((x) => x.pessoa === a.pessoa);
      p.cargos = p.cargos.filter((id) => id !== a.cargo);
      srv.revisao += 1;
      return null;
    },
  };

  /** O verbo de cada `PedidoDeCargo`, para a prévia rodá-lo numa cópia. */
  const DO_PEDIDO = { Criar: "criar_cargo", Editar: "editar_cargo", Mover: "mover_cargo", Apagar: "apagar_cargo", Atribuir: "atribuir_cargo", Revogar: "revogar_cargo" };
  /** Os verbos que voltam assim que o pedido sai (cargos-api Δ v2). */
  const VERBOS = Object.values(DO_PEDIDO);

  return {
    servidores,
    comandos: Object.keys(comandos),
    responder(cmd, args, eu) {
      const srv = servidores[args?.carimbo?.sessao];
      if (!srv) throw "NotConnected";
      // Quem pede é o gestor daquela sessão; a posição e GERIR CARGOS dele
      // saem do estado de agora, para uma autoridade tirada no meio valer.
      if (eu) srv.eu = { ...eu };
      srv.eu.posicao_mais_alta = srv.eu.proprietario ? 99 : posicaoMaisAlta(srv, srv.eu.pessoa);
      srv.eu.gere_cargos = srv.eu.proprietario || efetivas(srv, srv.eu.pessoa).has("ManageRoles");
      if (!VERBOS.includes(cmd)) return comandos[cmd](srv, args);
      // Um verbo: o comando volta já, e a recusa fica na `ultima_recusa` da
      // sessão, como o `CargoRecusado` que o servidor manda depois. Toda
      // mensagem de cargo (o quadro novo, ou a recusa) faz a marca andar.
      const recusar = (erro) => {
        srv.ultimaRecusa = erro.CargoRecusado.motivo;
        srv.recusaSeq = (srv.recusaSeq ?? 0) + 1;
        srv.marca = (srv.marca ?? 0) + 1;
      };
      try {
        comandos[cmd](srv, args);
        srv.marca = (srv.marca ?? 0) + 1;
      } catch (erro) {
        if (!erro?.CargoRecusado) throw erro;
        // Como o servidor real: a recusa chega DEPOIS (o `CargoRecusado` vem
        // pelo fio, um instante depois de o comando ter voltado).
        if (opcoes.atrasoDaRecusa) setTimeout(() => recusar(erro), opcoes.atrasoDaRecusa);
        else recusar(erro);
      }
      return null;
    },
    /** Se o comando é um verbo (que faz a marca andar). */
    eVerbo: (cmd) => VERBOS.includes(cmd),
  };
}

/**
 * A ponte de mentira, posta antes de qualquer script da página. Com
 * `opcoes.comum`, os comandos de cargos vão ao servidor do Node
 * (`servidorComum`), que outra página também usa.
 */
function ponteDeMentira({ base, servidores, opcoes }) {
  window.recebidos = [];
  window.opcoes = opcoes;
  const tabela = { ...base };
  window.tabela = tabela;
  const motor = motorDosServidores(servidores, opcoes);
  window.servidores = motor.servidores;
  // Os ouvintes de `seele://event` que a casca registrou, e o aviso que o
  // servidor de mentira dá: o `CargosChanged { marca }` da sessão, no envelope
  // `{ sessao, geracao, vista, evento }` (fronteiras.md §4).
  const ouvintes = [];
  window.__avisarCargos = (sessao, marca) => {
    if (opcoes.semAvisos) return;
    const vista = tabela.sessoes.find((s) => s.na_vista)?.carimbo.sessao;
    for (const ouvir of ouvintes) {
      ouvir({ event: "seele://event", payload: { sessao, geracao: 1, vista: sessao === vista, evento: { CargosChanged: { marca } } } });
    }
  };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        recebidos.push({ cmd, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
        const espera = opcoes.atraso?.[cmd];
        if (espera) await new Promise((ok) => setTimeout(ok, espera));
        if (cmd === "ver_sessao") {
          tabela.sessoes = tabela.sessoes.map((s) => ({ ...s, na_vista: s.carimbo.sessao === args.sessao }));
          const srv = motor.servidores[args.sessao];
          tabela.snapshot = { ...tabela.snapshot, server: srv.nome };
          return { carimbo: { sessao: args.sessao, geracao: 1 }, vista: 100 + args.sessao, snapshot: tabela.snapshot };
        }
        if (motor.comandos.includes(cmd)) {
          const sessao = args?.carimbo?.sessao;
          let valor;
          if (opcoes.comum && sessao === 1) {
            const resposta = await window.servidorComum(cmd, args, opcoes.eu ?? null);
            if ("erro" in resposta) throw resposta.erro;
            valor = resposta.valor;
          } else {
            valor = motor.responder(cmd, args, opcoes.eu ?? null);
            // O servidor responde ao verbo depois: o aviso sai no tique seguinte.
            if (motor.eVerbo(cmd)) {
              const marca = motor.servidores[sessao].marca;
              setTimeout(() => window.__avisarCargos(sessao, marca), 0);
            }
          }
          return { carimbo: { sessao, geracao: 1 }, valor };
        }
        return structuredClone(tabela[cmd] ?? null);
      },
    },
    event: {
      listen: async (nome, ouvir) => {
        if (nome === "seele://event") ouvintes.push(ouvir);
        return () => {};
      },
    },
    window: { getCurrentWindow: () => ({ onCloseRequested() {}, isMaximized: async () => false }) },
  };
}

const sessao = (id, nome) => ({
  carimbo: { sessao: id, geracao: 1 }, endereco: `${nome.toLowerCase().replace(/ /g, "-")}.exemplo:8383`, nome,
  na_vista: id === 1, foco_de_voz: id === 1, na_sala_de_voz: null, onde: "Outro", enlace: "online",
  encerrada: null, nao_lidas: 0, mudou: false,
});

/** Uma página com a sessão A na tela e a camada de cargos fechada. */
async function abrir(browser, viewport, servidor, opcoes = {}) {
  const contexto = await browser.newContext({ viewport, hasTouch: viewport.width < 500, isMobile: viewport.width < 500 && !WEBKIT });
  const page = await contexto.newPage();
  const erros = [];
  page.on("pageerror", (e) => erros.push(e.message));
  const dados = {
    base: {
      ...respostas({ microfones: [], saidas: [], meu_servidor: null }),
      // Sem `cargos_revisao`: o retrato do app de agora não o traz, e a tela
      // tem de saber das mudanças pelas leituras.
      snapshot: retrato({ server: SERVIDOR_A.nome, me: opcoes.eu?.pessoa ?? 1 }),
      sessoes: [sessao(1, SERVIDOR_A.nome), sessao(2, SERVIDOR_B.nome)],
    },
    servidores: { 1: structuredClone(SERVIDOR_A), 2: structuredClone(SERVIDOR_B) },
    opcoes,
  };
  if (opcoes.comum) {
    // O servidor A é um só para as duas páginas: o do Node.
    // Cada página no servidor comum, para o aviso de um verbo chegar às duas.
    opcoes.comum.paginas = opcoes.comum.paginas ?? [];
    opcoes.comum.paginas.push(page);
    await page.exposeFunction("servidorComum", (cmd, args, eu) => {
      try {
        const valor = opcoes.comum.responder(cmd, args, eu);
        if (opcoes.comum.eVerbo(cmd)) {
          const marca = opcoes.comum.servidores[1].marca;
          for (const outra of opcoes.comum.paginas) {
            outra.evaluate((m) => window.__avisarCargos?.(1, m), marca).catch(() => {});
          }
        }
        return { valor };
      } catch (erro) {
        return { erro };
      }
    });
    dados.opcoes = { ...opcoes, comum: true };
  }
  await page.addInitScript({
    content: `${motorDosServidores.toString()}\n(${ponteDeMentira.toString()})(${JSON.stringify(dados)});`,
  });
  await page.goto(`http://127.0.0.1:${servidor.address().port}`);
  await page.evaluate(async () => {
    for (const t of document.querySelectorAll("section.tela")) t.hidden = true;
    $("tela-sessao").hidden = false;
    await verSessao(1);
  });
  await page.waitForFunction(() => vistaDaJanela.sessao === 1);
  return { page, contexto, erros };
}

const texto = (page, seletor) => page.$eval(seletor, (e) => e.textContent.trim());
const nomesDaLista = (page) => page.$$eval("#cargos-lista .cargos-item-nome", (es) => es.map((e) => e.textContent));
const esperarLista = (page, primeiro) =>
  page.waitForFunction((n) => document.querySelector("#cargos-lista .cargos-item-nome")?.textContent === n, primeiro);
const clicarCargo = (page, nome) => page.click(`#cargos-lista .cargos-item-botao:has(.cargos-item-nome:text-is("${nome}"))`);
/** No celular, a lista volta pelo VOLTAR: lista e detalhe não aparecem juntos. */
async function voltarSeCelular(page, rotulo, botao) {
  if (rotulo === "celular") await page.click(botao);
}
/** O rádio de uma permissão no editor, pelo nome dela e pelo estado. */
const radio = (nome, estado) => `#cargos-permissoes fieldset.cargos-permissao:has(> legend:text-is("${nome}")) input[value="${estado}"]`;

async function capturar(page, nome) {
  if (SEM_CAPTURAS) return;
  const pasta = path.join(EVIDENCIAS, MOTOR);
  fs.mkdirSync(pasta, { recursive: true });
  await page.screenshot({ path: path.join(pasta, `${nome}.png`) });
}

/** Cabe na largura: nada da caixa passa para o lado. */
async function cabeNaLargura(page) {
  const medida = await page.$eval(".cargos-caixa", (c) => ({ cheio: c.scrollWidth, visto: c.clientWidth, janela: window.innerWidth, direita: c.getBoundingClientRect().right }));
  assert.ok(medida.cheio <= medida.visto + 1 && medida.direita <= medida.janela + 1, `a camada passa da largura: ${JSON.stringify(medida)}`);
}

// ------------------------------------------------------------------ as cenas

const CENAS = {
  /** A hierarquia em ordem, o de todos embaixo, e o que não se gere com o motivo. */
  async hierarquia(page, viewport) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    assert.deepEqual(await nomesDaLista(page), ["Comandante", "Moderação", "Guarda", "Membro", "Silenciados", "Quarentena", "TODOS · @everyone"]);
    assert.match(await texto(page, "#cargos-regra-hierarquia"), /o primeiro cargo é o mais alto/);
    assert.match(await texto(page, "#cargos-regra-combinacao"), /RESTRIÇÃO em qualquer cargo dela vence/);
    assert.match(await texto(page, "#cargos-regra-proprietario"), /proprietário do servidor pode tudo/);
    const rotulos = await page.$$eval("#cargos-lista .cargos-item-botao", (bs) => bs.map((b) => b.getAttribute("aria-label")));
    assert.match(rotulos[0], /Está acima do seu cargo mais alto/, "o Comandante não diz por que é só leitura");
    assert.match(rotulos[1], /É o seu cargo mais alto/, "a Moderação não diz por que é só leitura");
    assert.doesNotMatch(rotulos[3], /acima/, "o Membro aparece como não gerível");
    // A Guarda não sobe para cima da Moderação, e o botão diz por quê.
    const subirGuarda = await page.$eval('[data-subir="6"]', (b) => ({ off: b.disabled, title: b.title }));
    assert.equal(subirGuarda.off, true, "a Guarda pode subir para cima do cargo mais alto de quem edita");
    assert.match(subirGuarda.title, /acima do seu cargo mais alto/);
    assert.equal(await page.$eval('[data-descer="5"]', (b) => b.disabled), true, "a Quarentena desce abaixo do cargo de todos");
    await capturar(page, `${viewport}-01-hierarquia`);
    // O cargo acima: o editor abre, mostra tudo, e diz por que não muda nada.
    await clicarCargo(page, "Comandante");
    await page.waitForSelector("#cargos-form:not([hidden])");
    assert.equal(await page.$eval("#cargos-editor-motivo", (e) => e.hidden), false, "o editor do Comandante não diz por que não se edita");
    assert.match(await texto(page, "#cargos-editor-motivo"), /acima do seu cargo mais alto/);
    assert.equal(await page.$eval("#cargos-nome", (e) => e.disabled), true);
    assert.equal(await page.$eval("#cargos-salvar", (e) => e.disabled), true);
    assert.equal(await page.$$eval("#cargos-permissoes input:not(:disabled)", (es) => es.length), 0, "uma permissão do Comandante dá para mudar");
    await capturar(page, `${viewport}-02-cargo-acima`);
  },

  /** As permissões em grupos, as do MOD sob o nome dele, e CONCEDE só do que se tem. */
  async permissoes(page, viewport) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await clicarCargo(page, "Membro");
    await page.waitForSelector("#cargos-form:not([hidden])");
    const grupos = await page.$$eval("#cargos-permissoes > fieldset > legend", (ls) => ls.map((l) => l.textContent));
    assert.deepEqual(grupos, ["CONVERSA", "VOZ", "MODERAÇÃO", "SERVIDOR", "MOD seele/mesa"]);
    const mesa = await page.$$eval('#cargos-permissoes > fieldset:has(> legend:text-is("MOD seele/mesa")) .cargos-permissao', (fs) =>
      fs.map((f) => f.querySelector("legend").textContent));
    assert.deepEqual(mesa, ["Fechar a mesa", "Mover peças"]);
    assert.match(await texto(page, '#cargos-permissoes fieldset.cargos-permissao:has(> legend:text-is("Mover peças")) .cargos-permissao-desc'), /ação que este MOD declarou/);
    // O modelo não tem cor: nem o campo nem a amostra aparecem.
    assert.equal(await page.$eval("#cargos-cor", (s) => s.closest("label").hidden), true, "o campo COR aparece sem cor no modelo");
    assert.equal(await page.$$eval(".cargos-cor-amostra", (es) => es.length), 0);
    assert.equal(await page.$eval(radio("FALAR", "concede"), (r) => r.checked), true, "o Membro não aparece concedendo FALAR");
    // BANIR e «Fechar a mesa» Alex não tem: CONCEDE desligado, com o motivo escrito.
    for (const nome of ["BANIR", "Fechar a mesa", "ADMINISTRADOR"]) {
      assert.equal(await page.$eval(radio(nome, "concede"), (r) => r.disabled), true, `${nome}: dá para conceder sem ter`);
      assert.equal(await page.$eval(radio(nome, "restringe"), (r) => r.disabled), false, `${nome}: restringir é livre num cargo abaixo`);
      const motivo = await page.$eval(`#cargos-permissoes fieldset.cargos-permissao:has(> legend:text-is("${nome}")) .cargos-motivo`, (e) => e.textContent);
      assert.match(motivo, /Você não tem esta permissão/);
    }
    // O nome acessível do grupo de rádio leva a descrição e o motivo.
    const descrito = await page.$eval('#cargos-permissoes fieldset.cargos-permissao:has(> legend:text-is("BANIR"))', (f) =>
      f.getAttribute("aria-describedby").split(" ").map((id) => document.getElementById(id)?.textContent).join(" | "));
    assert.match(descrito, /barrar a volta.*Você não tem esta permissão/);
    await capturar(page, `${viewport}-03-permissoes`);
  },

  /** Tirar uma restrição: a prévia diz quem passa a poder o quê, e SALVAR vai a A. */
  async previa(page, viewport) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await clicarCargo(page, "Silenciados");
    await page.waitForSelector("#cargos-form:not([hidden])");
    await page.check(radio("FALAR", "neutro"));
    await page.waitForFunction(() => /PASSA A PODER/.test(document.getElementById("cargos-previa-lista").textContent));
    assert.match(await texto(page, "#cargos-previa-lista"), /Rafa PASSA A PODER: FALAR\./);
    assert.equal(await page.$eval("#cargos-salvar", (b) => b.disabled), false);
    await page.$eval("#cargos-previa", (p) => p.scrollIntoView({ block: "center" }));
    await capturar(page, `${viewport}-04-previa`);
    await page.evaluate(() => { recebidos.length = 0; });
    await page.click("#cargos-salvar");
    await page.waitForFunction(() => recebidos.some((r) => r.cmd === "editar_cargo"));
    const editar = await page.evaluate(() => recebidos.find((r) => r.cmd === "editar_cargo").args);
    assert.equal(editar.carimbo.sessao, 1, "SALVAR não foi à sessão de onde partiu");
    assert.deepEqual(editar.concede, []);
    assert.deepEqual(editar.restringe, [{ Produto: "ShareScreen" }]);
    assert.equal(editar.revisao, 7, "SALVAR não mandou a revisão do conjunto que a tela leu");
    await page.waitForFunction(() => document.getElementById("cargos-rascunho").hidden);
    assert.deepEqual(await page.evaluate(() => servidores[1].cargos.find((c) => c.id === 1).restringe), ["ShareScreen"]);
  },

  /** O ganho além: a prévia diz quem ganharia o que eu não tenho, e SALVAR fica desligado. */
  async "ganho-alem"(page, viewport) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await clicarCargo(page, "Quarentena");
    await page.waitForSelector("#cargos-form:not([hidden])");
    await page.check(radio("BANIR", "neutro"));
    await page.waitForFunction(() => /vai recusar/.test(document.getElementById("cargos-previa-lista").textContent));
    assert.match(await texto(page, "#cargos-previa-lista"), /O servidor vai recusar\. RECUSADO: isto daria a Bia BANIR, que você não tem\./);
    assert.equal(await page.$eval("#cargos-salvar", (b) => b.disabled), true, "SALVAR fica apertável com a prévia dizendo que o servidor recusa");
    await page.$eval("#cargos-previa", (p) => p.scrollIntoView({ block: "center" }));
    await capturar(page, `${viewport}-05-ganho-alem`);
  },

  /** Sem a prévia do servidor: a diferença local avisa, e a recusa dele aparece nomeada. */
  async recusa(page, viewport) {
    await page.evaluate(() => { opcoes.semPrevia = true; abrirCargos({}); });
    await esperarLista(page, "Comandante");
    await clicarCargo(page, "Quarentena");
    await page.waitForSelector("#cargos-form:not([hidden])");
    await page.check(radio("BANIR", "neutro"));
    await page.waitForFunction(() => /calculou a prévia/.test(document.getElementById("cargos-previa-lista").textContent));
    // Tirar uma restrição nunca parece inofensivo, nem sem a prévia.
    assert.match(await texto(page, "#cargos-previa-lista"), /Tirar uma restrição devolve a quem tem este cargo: BANIR/);
    await page.click("#cargos-salvar");
    await page.waitForSelector("#cargos-form-erro:not([hidden])");
    const erro = await texto(page, "#cargos-form-erro");
    assert.match(erro, /RECUSADO: isto daria a Bia BANIR, que você não tem\./, `a recusa não diz quem ganharia o quê: ${erro}`);
    assert.deepEqual(await page.evaluate(() => servidores[1].cargos.find((c) => c.id === 5).restringe), ["Ban", "Kick"]);
    await page.$eval("#cargos-form-erro", (p) => p.scrollIntoView({ block: "center" }));
    await capturar(page, `${viewport}-06-recusa`);
  },

  /** Dar e tirar um cargo: a prévia antes, e o pedido só no CONFIRMAR. */
  async pessoas(page, viewport) {
    await page.evaluate(() => abrirCargos({ aba: "pessoas" }));
    await page.waitForSelector('#cargos-pessoas [data-pessoa="3"]');
    await page.click('#cargos-pessoas [data-pessoa="3"]');
    await page.waitForSelector("#cargos-pessoa:not([hidden])");
    assert.equal(await texto(page, "#cargos-pessoa-titulo"), "CARGOS DE Rafa");
    const comandante = await page.$eval('[data-atribuir="4"]', (c) => ({ off: c.disabled, motivo: document.getElementById(c.getAttribute("aria-describedby"))?.textContent }));
    assert.equal(comandante.off, true);
    assert.match(comandante.motivo, /acima do seu cargo mais alto/);
    await page.evaluate(() => { recebidos.length = 0; });
    await page.uncheck('[data-atribuir="1"]');
    await page.waitForFunction(() => /PASSA A PODER/.test(document.getElementById("cargos-pendente-lista").textContent));
    assert.equal(await texto(page, "#cargos-pendente-titulo"), "TIRAR Silenciados DE Rafa?");
    assert.match(await texto(page, "#cargos-pendente-lista"), /Rafa PASSA A PODER: FALAR e TRANSMITIR A TELA\./);
    assert.deepEqual(await page.evaluate(() => recebidos.filter((r) => r.cmd === "revogar_cargo")), [], "o cargo saiu antes do CONFIRMAR");
    await capturar(page, `${viewport}-07-pessoas-pendente`);
    await page.click("#cargos-pendente-confirmar");
    await page.waitForFunction(() => recebidos.some((r) => r.cmd === "revogar_cargo"));
    const revogar = await page.evaluate(() => recebidos.find((r) => r.cmd === "revogar_cargo").args);
    assert.deepEqual([revogar.pessoa, revogar.cargo, revogar.carimbo.sessao], [3, 1, 1]);
    await page.waitForFunction(() => document.getElementById("cargos-pendente").hidden);
    // Tirar a Quarentena de Bia daria BANIR: CONFIRMAR fica desligado e a frase diz por quê.
    await voltarSeCelular(page, viewport, "#cargos-pessoas-voltar");
    await page.click('#cargos-pessoas [data-pessoa="4"]');
    await page.uncheck('[data-atribuir="5"]');
    await page.waitForFunction(() => /vai recusar/.test(document.getElementById("cargos-pendente-lista").textContent));
    assert.match(await texto(page, "#cargos-pendente-lista"), /isto daria a Bia BANIR, que você não tem/);
    assert.equal(await page.$eval("#cargos-pendente-confirmar", (b) => b.disabled), true);
    // Teo está acima: tudo desligado, com o motivo da pessoa.
    await page.click("#cargos-pendente-cancelar");
    await voltarSeCelular(page, viewport, "#cargos-pessoas-voltar");
    await page.click('#cargos-pessoas [data-pessoa="6"]');
    await page.waitForSelector("#cargos-pessoa-motivo:not([hidden])");
    assert.match(await texto(page, "#cargos-pessoa-motivo"), /cargo mais alto desta pessoa está acima do seu/);
    assert.equal(await page.$$eval("#cargos-da-pessoa input:not(:disabled)", (es) => es.length), 0);
  },

  /** Quem pode o quê: o veredito em palavra e o cargo que decidiu. */
  async explicar(page, viewport) {
    await page.evaluate(() => abrirCargos({ pessoa: 3, aba: "explicar" }));
    await page.waitForSelector(".cargos-explicado");
    const linha = (nome) =>
      page.$eval(`.cargos-explicado:has(.cargos-permissao-nome:text-is("${nome}"))`, (l) => [l.querySelector(".cargos-veredito").textContent, l.querySelector(".cargos-explicado-regra").textContent]);
    assert.deepEqual(await linha("FALAR"), ["NÃO PODE", "Restringida por Silenciados. Membro concede, e a restrição vence."]);
    assert.deepEqual(await linha("ESCREVER NOS CANAIS"), ["PODE", "Concedida por Membro."]);
    assert.deepEqual(await linha("BANIR"), ["NÃO PODE", "Nenhum cargo desta pessoa concede: fica negada."]);
    assert.deepEqual(await linha("Teletransportar"), ["NÃO PODE", "O servidor não conhece esta permissão: ela não concede nada."]);
    await capturar(page, `${viewport}-08-quem-pode`);
    await page.selectOption("#cargos-explicar-quem", "2");
    await page.waitForFunction(() => /proprietário/.test(document.getElementById("cargos-explicar-resumo").textContent));
    assert.deepEqual(await linha("BANIR"), ["PODE", "É o proprietário do servidor: pode tudo."]);
    await page.selectOption("#cargos-explicar-quem", "6");
    await page.waitForFunction(() => /ADMINISTRADOR/.test(document.querySelector(".cargos-explicacao").textContent));
    assert.deepEqual(await linha("BANIR"), ["PODE", "Vem do ADMINISTRADOR, concedido por Comandante."]);
  },

  /**
   * Os cargos são da sessão: um SALVAR de A que responde depois da troca vai a
   * A; a camada de A some em B; B mostra os cargos de B; voltar a A reabre o
   * que era de A, com o rascunho.
   */
  async isolamento(page, viewport) {
    await page.evaluate(() => { opcoes.atraso = { editar_cargo: 600 }; abrirCargos({}); });
    await esperarLista(page, "Comandante");
    await clicarCargo(page, "Silenciados");
    await page.waitForSelector("#cargos-form:not([hidden])");
    await page.check(radio("FALAR", "neutro"));
    await page.waitForFunction(() => /PASSA A PODER/.test(document.getElementById("cargos-previa-lista").textContent));
    await page.evaluate(() => { recebidos.length = 0; });
    await page.click("#cargos-salvar");
    // A troca acontece com a resposta ainda no ar.
    await page.evaluate(() => verSessao(2));
    await page.waitForTimeout(900);
    const emB = await page.evaluate(() => ({
      vista: vistaDaJanela.sessao,
      camada: !$("cargos").hidden,
      lista: [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].map((e) => e.textContent),
    }));
    assert.equal(emB.vista, 2);
    assert.ok(!emB.camada || !emB.lista.includes("Silenciados"), `os cargos de A aparecem em B: ${JSON.stringify(emB)}`);
    const editar = await page.evaluate(() => recebidos.filter((r) => r.cmd === "editar_cargo").map((r) => r.args.carimbo.sessao));
    assert.deepEqual(editar, [1], "o SALVAR de A foi a outra sessão");
    assert.deepEqual(await page.evaluate(() => servidores[1].cargos.find((c) => c.id === 1).restringe), ["ShareScreen"], "o SALVAR de A não chegou a A");
    assert.equal(await page.evaluate(() => servidores[2].revisao), 2, "B mudou com o SALVAR de A");
    // B por ela mesma: os cargos de B, e nada de A.
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Banda");
    assert.deepEqual(await nomesDaLista(page), ["Banda", "Plateia", "TODOS · @everyone"]);
    // Um rascunho em B, sem salvar, e de volta a A.
    await clicarCargo(page, "Plateia");
    await page.waitForSelector("#cargos-form:not([hidden])");
    await page.check(radio("FALAR", "neutro"));
    await capturar(page, `${viewport}-09-sessao-b`);
    await page.evaluate(() => { recebidos.length = 0; return verSessao(1); });
    await page.waitForFunction(() => !$("cargos").hidden && document.querySelector("#cargos-lista .cargos-item-nome")?.textContent === "Comandante");
    assert.equal(await texto(page, "#cargos-editor-titulo"), "Silenciados", "A voltou sem o cargo que estava aberto");
    // O SALVAR de A estava esperando a resposta quando a vista saiu: a volta
    // a A o resolve como aceito, e o rascunho já gravado não fica pendurado.
    await page.waitForFunction(() => document.getElementById("cargos-rascunho").hidden);
    assert.deepEqual(await page.evaluate(() => recebidos.filter((r) => r.cmd === "editar_cargo")), [], "o rascunho de B saiu ao voltar para A");
    // E B guardou o rascunho dela.
    await page.evaluate(() => verSessao(2));
    await page.waitForFunction(() => !$("cargos").hidden && document.querySelector("#cargos-lista .cargos-item-nome")?.textContent === "Banda");
    assert.equal(await page.$eval("#cargos-rascunho", (e) => e.hidden), false, "o rascunho de B sumiu");
    assert.equal(await page.$eval(radio("FALAR", "neutro"), (r) => r.checked), true, "o rascunho de B voltou outro");
  },

  /**
   * Dois gestores, duas sessões, o mesmo cargo: Teo (Comandante) salva antes;
   * o SALVAR de Alex é recusado por revisão velha, a tela diz que outro gestor
   * mudou o cargo, destaca o que mudou, e não reaplica nada por cima sozinha.
   * Depois, a autoridade de Alex é tirada entre a prévia e o SALVAR, e a
   * recusa do servidor aparece com o motivo dele.
   */
  async concorrencia(page, rotulo, { browser, servidor, motor, viewport }) {
    const teo = await abrir(browser, viewport, servidor, { comum: motor, eu: { pessoa: 6, proprietario: false } });
    teo.page.setDefaultTimeout(6000);
    try {
      // Teo prepara a edição dele primeiro; Alex lê e edita depois, e Teo grava
      // antes de Alex: a ordem não depende do relógio da releitura de 4 s.
      await teo.page.evaluate(() => abrirCargos({}));
      await esperarLista(teo.page, "Comandante");
      await clicarCargo(teo.page, "Membro");
      await teo.page.waitForSelector("#cargos-form:not([hidden])");
      await teo.page.check(radio("ANEXAR ARQUIVOS", "restringe"));
      await teo.page.waitForFunction(() => !$("cargos-salvar").disabled);

      await page.evaluate(() => abrirCargos({}));
      await esperarLista(page, "Comandante");
      await clicarCargo(page, "Membro");
      await page.waitForSelector("#cargos-form:not([hidden])");
      await page.check(radio("ESCREVER NOS CANAIS", "neutro"));
      // A prévia de Alex chega antes de Teo gravar: o que se prova aqui é a
      // recusa no SALVAR (a da prévia é a cena «previa-velha»).
      await page.waitForFunction(() => /DEIXA DE PODER/.test(document.getElementById("cargos-previa-lista").textContent));

      await teo.page.click("#cargos-salvar");
      await teo.page.waitForFunction(() => document.getElementById("cargos-rascunho").hidden);

      await page.waitForFunction(() => !$("cargos-salvar").disabled);
      await page.click("#cargos-salvar");
      await page.waitForSelector("#cargos-form-erro:not([hidden])");
      assert.match(await texto(page, "#cargos-form-erro"), /outro gestor mudou os cargos antes de você salvar/);
      const membro = () => motor.servidores[1].cargos.find((c) => c.id === 2);
      assert.ok(membro().concede.includes("WriteChannel"), "a edição de Alex foi gravada por cima da de Teo");
      assert.ok(membro().restringe.includes("AttachFile"), "a edição de Teo sumiu");
      await page.waitForSelector(".cargos-permissao-mudou");
      const destacadas = await page.$$eval(".cargos-permissao-mudou", (fs) =>
        fs.map((f) => [f.querySelector("legend").textContent, f.querySelector(".cargos-mudou").textContent]));
      assert.deepEqual(destacadas, [["ANEXAR ARQUIVOS", "MUDOU AGORA: estava CONCEDE, ficou RESTRINGE."]]);
      assert.match(await texto(page, "#cargos-rascunho"), /OUTRO GESTOR MUDOU ESTE CARGO/);
      assert.equal(await page.$eval(radio("ESCREVER NOS CANAIS", "neutro"), (r) => r.checked), true, "a mudança de Alex sumiu do rascunho");
      assert.equal(await page.$eval("#cargos-salvar", (b) => b.disabled), true, "dá para salvar por cima sem refazer");
      await page.$eval(".cargos-permissao-mudou", (p) => p.scrollIntoView({ block: "center" }));
      await capturar(page, `${rotulo}-11-outro-gestor`);

      // REFAZER SOBRE O ATUAL: as duas mudanças juntas, e só então SALVAR.
      await page.click("#cargos-refazer");
      assert.equal(await page.$eval(radio("ANEXAR ARQUIVOS", "restringe"), (r) => r.checked), true);
      assert.equal(await page.$eval(radio("ESCREVER NOS CANAIS", "neutro"), (r) => r.checked), true);
      await page.waitForFunction(() => !$("cargos-salvar").disabled);
      await page.click("#cargos-salvar");
      await page.waitForFunction(() => document.getElementById("cargos-rascunho").hidden);
      assert.ok(!membro().concede.includes("WriteChannel") && membro().restringe.includes("AttachFile"), JSON.stringify(membro()));

      // A autoridade tirada entre a prévia e o SALVAR: a recusa do servidor.
      await voltarSeCelular(page, rotulo, "#cargos-voltar");
      await clicarCargo(page, "Silenciados");
      await page.check(radio("FALAR", "neutro"));
      await page.waitForFunction(() => /PASSA A PODER/.test(document.getElementById("cargos-previa-lista").textContent));
      motor.servidores[1].cargos.find((c) => c.id === 3).concede = ["Kick", "MovePerson", "RemoveMessage"];
      await page.click("#cargos-salvar");
      await page.waitForSelector("#cargos-form-erro:not([hidden])");
      const erro = await texto(page, "#cargos-form-erro");
      assert.match(erro, /RECUSADO: você não tem GERIR CARGOS neste servidor, agora\./);
      await capturar(page, `${rotulo}-12-autoridade-tirada`);
      assert.deepEqual(teo.erros, []);
    } finally {
      await teo.contexto.close();
    }
  },

  /**
   * A prévia antecipa a revisão velha: Alex leu, Teo gravou, e só depois Alex
   * edita. A prévia do servidor diz «outro gestor mudou»; a tela relê, destaca
   * o que mudou, e SALVAR só volta depois de REFAZER SOBRE O ATUAL.
   */
  async "previa-velha"(page, rotulo, { browser, servidor, motor, viewport }) {
    const teo = await abrir(browser, viewport, servidor, { comum: motor, eu: { pessoa: 6, proprietario: false } });
    teo.page.setDefaultTimeout(6000);
    try {
      await teo.page.evaluate(() => abrirCargos({}));
      await esperarLista(teo.page, "Comandante");
      await clicarCargo(teo.page, "Membro");
      await teo.page.waitForSelector("#cargos-form:not([hidden])");
      await teo.page.check(radio("ANEXAR ARQUIVOS", "restringe"));
      await teo.page.waitForFunction(() => !$("cargos-salvar").disabled);
      await page.evaluate(() => abrirCargos({}));
      await esperarLista(page, "Comandante");
      await teo.page.click("#cargos-salvar");
      await teo.page.waitForFunction(() => document.getElementById("cargos-rascunho").hidden);

      await clicarCargo(page, "Membro");
      await page.waitForSelector("#cargos-form:not([hidden])");
      await page.check(radio("ESCREVER NOS CANAIS", "neutro"));
      // Logo depois da prévia (250 ms mais a leitura), e não pela releitura
      // de 4 s, que também acabaria achando: é a prévia que tem de avisar.
      await page.waitForSelector(".cargos-permissao-mudou", { timeout: 1500 });
      assert.match(await texto(page, "#cargos-rascunho"), /OUTRO GESTOR MUDOU ESTE CARGO/);
      assert.equal(await page.$eval("#cargos-salvar", (b) => b.disabled), true, "dá para salvar por cima da mudança de Teo");
      assert.equal(await page.$eval("#cargos-refazer", (b) => b.hidden), false);
      assert.ok(motor.servidores[1].cargos.find((c) => c.id === 2).concede.includes("WriteChannel"), "algo de Alex foi gravado");
      await page.click("#cargos-refazer");
      await page.waitForFunction(() => !$("cargos-salvar").disabled);
      assert.deepEqual(teo.erros, []);
    } finally {
      await teo.contexto.close();
    }
  },

  /**
   * O aviso do servidor (`CargosChanged`) chega à camada aberta de Alex quando
   * Teo grava: o rascunho dele fica desatualizado, com o que mudou destacado,
   * sem Alex apertar nada e sem a tela perguntar ao servidor.
   */
  async evento(page, rotulo, { browser, servidor, motor, viewport }) {
    const teo = await abrir(browser, viewport, servidor, { comum: motor, eu: { pessoa: 6, proprietario: false } });
    teo.page.setDefaultTimeout(6000);
    try {
      await teo.page.evaluate(() => abrirCargos({}));
      await esperarLista(teo.page, "Comandante");
      await clicarCargo(teo.page, "Membro");
      await teo.page.waitForSelector("#cargos-form:not([hidden])");
      await teo.page.check(radio("ANEXAR ARQUIVOS", "restringe"));
      await teo.page.waitForFunction(() => !$("cargos-salvar").disabled);

      await page.evaluate(() => abrirCargos({}));
      await esperarLista(page, "Comandante");
      await clicarCargo(page, "Membro");
      await page.waitForSelector("#cargos-form:not([hidden])");
      await page.check(radio("ESCREVER NOS CANAIS", "neutro"));
      await page.waitForFunction(() => /DEIXA DE PODER/.test(document.getElementById("cargos-previa-lista").textContent));

      await teo.page.click("#cargos-salvar");
      await page.waitForSelector(".cargos-permissao-mudou", { timeout: 1500 });
      assert.match(await texto(page, "#cargos-rascunho"), /OUTRO GESTOR MUDOU ESTE CARGO/);
      assert.equal(await page.$eval("#cargos-salvar", (b) => b.disabled), true);
      assert.deepEqual(teo.erros, []);
    } finally {
      await teo.contexto.close();
    }
  },

  /** A marca do retrato (`cargos_revision`) andou sem evento: a camada relê. */
  async "revisao-do-retrato"(page) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await page.evaluate(() => {
      const srv = servidores[1];
      srv.cargos.find((c) => c.id === 1).nome = "Silêncio";
      srv.marca = (srv.marca ?? 0) + 1;
      tabela.snapshot.cargos_revision = srv.marca;
      desenhado.cargos_revision = srv.marca;
    });
    await page.waitForFunction(
      () => [...document.querySelectorAll("#cargos-lista .cargos-item-nome")].some((e) => e.textContent === "Silêncio"),
      null,
      { timeout: 1500 },
    );
  },

  /** Parada, a camada não pergunta nada ao servidor: nenhum pedido periódico. */
  async "sem-sondagem"(page) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await page.evaluate(() => { recebidos.length = 0; });
    await page.waitForTimeout(6000);
    const pedidos = await page.evaluate(() => recebidos.filter((r) => /cargo/.test(r.cmd)).map((r) => r.cmd));
    assert.deepEqual(pedidos, [], `a camada parada pediu ao servidor: ${pedidos.join(", ")}`);
  },

  /** A mesma recusa duas vezes aparece duas vezes: o número dela andou. */
  async "recusa-repetida"(page) {
    await page.evaluate(() => { opcoes.semPrevia = true; abrirCargos({}); });
    await esperarLista(page, "Comandante");
    await clicarCargo(page, "Quarentena");
    await page.waitForSelector("#cargos-form:not([hidden])");
    await page.check(radio("BANIR", "neutro"));
    await page.waitForFunction(() => !$("cargos-salvar").disabled);
    for (const vez of [1, 2]) {
      await page.click("#cargos-salvar");
      await page.waitForFunction(
        () => !$("cargos-form-erro").hidden && /isto daria a Bia BANIR/.test($("cargos-form-erro").textContent),
        null,
        { timeout: 2500 },
      ).catch(() => {
        throw new Error(`a ${vez}ª recusa igual não apareceu como recusa`);
      });
      await page.waitForFunction(() => !$("cargos-salvar").disabled);
    }
  },

  /**
   * A revisão subiu por OUTRO gestor e a recusa do meu SALVAR chega um instante
   * depois (como no servidor real): a camada não pode tomar a revisão nova por
   * aceite e jogar fora o rascunho. Achado pela E2E contra o servidor real.
   */
  async "aceite-alheio"(page, rotulo, { browser, servidor, motor, viewport }) {
    const teo = await abrir(browser, viewport, servidor, { comum: motor, eu: { pessoa: 6, proprietario: false } });
    teo.page.setDefaultTimeout(6000);
    try {
      await page.evaluate(() => abrirCargos({}));
      await esperarLista(page, "Comandante");
      await clicarCargo(page, "Membro");
      await page.waitForSelector("#cargos-form:not([hidden])");
      await page.check(radio("ESCREVER NOS CANAIS", "neutro"));
      await page.waitForFunction(() => /DEIXA DE PODER/.test(document.getElementById("cargos-previa-lista").textContent));

      await teo.page.evaluate(() => abrirCargos({}));
      await esperarLista(teo.page, "Comandante");
      await clicarCargo(teo.page, "Guarda");
      await teo.page.waitForSelector("#cargos-form:not([hidden])");
      await teo.page.check(radio("MOVER PESSOAS", "concede"));
      await teo.page.waitForFunction(() => !$("cargos-salvar").disabled);
      await teo.page.click("#cargos-salvar");
      await teo.page.waitForFunction(() => document.getElementById("cargos-rascunho").hidden);

      await page.click("#cargos-salvar");
      await page.waitForSelector("#cargos-form-erro:not([hidden])", { timeout: 4000 });
      assert.match(await texto(page, "#cargos-form-erro"), /outro gestor mudou os cargos/);
      assert.equal(await page.$eval(radio("ESCREVER NOS CANAIS", "neutro"), (r) => r.checked), true, "o rascunho de Alex foi jogado fora");
      assert.ok(motor.servidores[1].cargos.find((c) => c.id === 2).concede.includes("WriteChannel"), "o SALVAR de Alex entrou por cima");
    } finally {
      await teo.contexto.close();
    }
  },

  /** A tela de chegada (entrar noutro servidor) por cima da sessão: a camada não a cobre. */
  async chegada(page) {
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await page.evaluate(() => { $("tela-auth").hidden = false; });
    await page.waitForFunction(() => $("cargos").hidden, null, { timeout: 1000 });
    await page.evaluate(() => { $("tela-auth").hidden = true; });
    await page.waitForFunction(() => !$("cargos").hidden, null, { timeout: 1000 });
  },

  /** Sem proprietário declarado, a tela diz isso no lugar da regra do proprietário. */
  async "sem-dono"(page) {
    await page.evaluate(() => { servidores[1].donoDeclarado = false; abrirCargos({}); });
    await esperarLista(page, "Comandante");
    assert.match(await texto(page, "#cargos-regra-proprietario"), /ESTE SERVIDOR NÃO TEM PROPRIETÁRIO DECLARADO/);
  },

  /** Teclado: as abas pelas setas, Escape fecha, e o foco volta a quem abriu. */
  async teclado(page) {
    await page.evaluate(() => {
      const porta = document.createElement("button");
      porta.id = "porta-de-teste";
      porta.textContent = "abrir";
      porta.addEventListener("click", () => abrirCargos({}));
      $("tela-sessao").append(porta);
    });
    await page.focus("#porta-de-teste");
    await page.keyboard.press("Enter");
    await esperarLista(page, "Comandante");
    assert.equal(await page.evaluate(() => document.activeElement.id), "cargos-aba-cargos");
    await page.keyboard.press("ArrowRight");
    assert.equal(await page.evaluate(() => document.activeElement.id), "cargos-aba-pessoas");
    assert.equal(await page.$eval("#cargos-aba-pessoas", (b) => b.getAttribute("aria-selected")), "true");
    assert.equal(await page.$eval("#cargos-painel-pessoas", (p) => p.hidden), false);
    await page.keyboard.press("End");
    assert.equal(await page.evaluate(() => document.activeElement.id), "cargos-aba-explicar");
    await page.keyboard.press("Escape");
    assert.equal(await page.$eval("#cargos", (c) => c.hidden), true);
    assert.equal(await page.evaluate(() => document.activeElement.id), "porta-de-teste", "o foco não voltou a quem abriu");
  },

  /** Celular: uma coluna, lista ou detalhe, com VOLTAR, e nada passa da largura. */
  async celular(page, viewport) {
    if (viewport !== "celular") return;
    await page.evaluate(() => abrirCargos({}));
    await esperarLista(page, "Comandante");
    await cabeNaLargura(page);
    assert.equal(await page.$eval("#cargos-painel-cargos .cargos-detalhe", (d) => getComputedStyle(d).display), "none");
    await clicarCargo(page, "Membro");
    await page.waitForSelector("#cargos-form:not([hidden])");
    assert.equal(await page.$eval("#cargos-painel-cargos .cargos-mestre", (d) => getComputedStyle(d).display), "none");
    assert.equal(await page.$eval("#cargos-voltar", (b) => getComputedStyle(b).display), "block");
    await cabeNaLargura(page);
    // Os alvos de toque têm 44 px.
    const baixos = await page.$$eval("#cargos-permissoes .cargos-opcao, #cargos-voltar, .cargos-aba", (es) =>
      es.filter((e) => e.getBoundingClientRect().height > 0 && e.getBoundingClientRect().height < 43.5).map((e) => e.textContent));
    assert.deepEqual(baixos, [], "alvos de toque menores que 44 px");
    await capturar(page, "celular-10-editor");
    await page.click("#cargos-voltar");
    assert.equal(await page.$eval("#cargos-painel-cargos .cargos-mestre", (d) => getComputedStyle(d).display), "flex");
  },
};

// ------------------------------------------------------------------- rodar

/** As cenas de dois gestores: o servidor A mora no Node, um para as duas páginas. */
const COMUNS = new Set(["concorrencia", "previa-velha", "evento", "aceite-alheio"]);
/** O servidor comum de uma cena, com o que ela muda nele. */
const OPCOES_DO_MOTOR = { "aceite-alheio": { atrasoDaRecusa: 700 } };
/**
 * As cenas que medem a corrida em que o aviso do servidor AINDA NÃO chegou à
 * página de Alex (a recusa no SALVAR, a prévia que antecipa): nelas a página
 * dele não recebe o `CargosChanged`. A cena «evento» mede o aviso chegando.
 */
const SEM_AVISOS = new Set(["concorrencia", "previa-velha", "aceite-alheio"]);

async function rodarCena(browser, nome, viewport, reversao) {
  const servidor = servir(reversao);
  await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
  const rotulo = viewport === CELULAR ? "celular" : "fhd";
  const motor = COMUNS.has(nome)
    ? motorDosServidores({ 1: structuredClone(SERVIDOR_A), 2: structuredClone(SERVIDOR_B) }, OPCOES_DO_MOTOR[nome] ?? {})
    : null;
  const opcoes = motor ? { comum: motor, eu: { pessoa: 1, proprietario: false }, semAvisos: SEM_AVISOS.has(nome) } : {};
  const { page, contexto, erros } = await abrir(browser, viewport, servidor, opcoes);
  page.setDefaultTimeout(6000);
  try {
    await CENAS[nome](page, rotulo, { browser, servidor, motor, viewport });
    if (rotulo === "celular" && nome !== "celular" && !$naoMede.has(nome)) await cabeNaLargura(page);
    assert.deepEqual(erros, [], `erros na página: ${erros.join(" | ")}`);
  } finally {
    await contexto.close();
    servidor.close();
  }
}
/** As cenas que terminam com a camada fechada, e onde medir a largura não diz nada. */
const $naoMede = new Set(["teclado"]);

async function principal() {
  const motor = WEBKIT ? require(process.env.PLAYWRIGHT || "playwright").webkit : chromium();
  const browser = await motor.launch({ headless: true });
  let falhas = 0;
  try {
    if (PROVAR) {
      const fonte = fs.readFileSync(path.join(RAIZ, "cargos.js"), "utf8");
      for (const [nome, reversao] of Object.entries(REVERSOES)) {
        if (FILTRO && !nome.includes(FILTRO)) continue;
        // Uma reversão cujo trecho não está mais no arquivo não reverte nada:
        // é falha da bancada, dita pelo nome, e não um guarda «provado».
        if (trocasDe(reversao).some(([de]) => fonte.split(de).length !== 2)) {
          falhas += 1;
          console.log(`REVERSÃO ${nome}: inválida — o trecho não aparece exatamente uma vez no cargos.js`);
          continue;
        }
        let reprovou = null;
        try {
          await rodarCena(browser, reversao.cena, FHD, reversao);
        } catch (falha) {
          reprovou = falha;
        }
        if (reprovou && !/reversão inválida/.test(String(reprovou.message))) {
          console.log(`REVERSÃO ${nome}: a cena «${reversao.cena}» REPROVA, como deve — ${String(reprovou.message).split("\n")[0]}`);
        } else {
          falhas += 1;
          console.log(`REVERSÃO ${nome}: ${reprovou ? reprovou.message : `a cena «${reversao.cena}» continua VERDE com o conserto desfeito — o guarda não guarda`}`);
        }
      }
    } else {
      for (const viewport of [FHD, CELULAR]) {
        for (const nome of Object.keys(CENAS)) {
          if (FILTRO && !nome.includes(FILTRO)) continue;
          const rotulo = viewport === CELULAR ? "celular" : "fhd";
          try {
            await rodarCena(browser, nome, viewport, null);
            console.log(`PASSA ${MOTOR} ${rotulo} ${nome}`);
          } catch (falha) {
            falhas += 1;
            const onde = String(falha.stack ?? "").split("\n").find((l) => l.includes("cargos.cjs")) ?? "";
            console.log(`REPROVA ${MOTOR} ${rotulo} ${nome}: ${falha.message.split("\n")[0]} ${onde.trim()}`);
          }
        }
      }
    }
  } finally {
    await browser.close();
  }
  if (falhas > 0) {
    console.log(`${falhas} ${falhas === 1 ? "falha" : "falhas"}.`);
    process.exitCode = 1;
  } else {
    console.log(PROVAR ? "todas as reversões reprovam as cenas delas." : "cargos: todas as cenas passam.");
  }
}

module.exports = { principal, REVERSOES, CENAS };
if (require.main === module) principal().catch((e) => { console.error(e); process.exit(1); });
