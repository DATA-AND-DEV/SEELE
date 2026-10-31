// SEELE — o perfil público (0.19, `docs/0.19/perfis-oficiais.md` §5).
//
// O PERFIS deixou de ser MOD: o servidor 0.19 guarda nome exibido, pronomes,
// status, bio, cor, efeito, foto e capa de cada pessoa (protocolo 12), e esta
// casca os desenha. Aqui moram:
//
// - **o armazém**, alimentado pelo `perfis` quando o `perfis_revision` do
//   retrato anda (o acordo do `cargos_revision`), e as imagens, pedidas uma vez
//   por hash pelo `imagem_do_perfil` e guardadas por ele para sempre;
// - **a precedência** (§5.2): `fotoOficial`/`capaOficial` são o segundo degrau
//   de `aplicarAvatar`/`aplicarCapa` (`tela-sessao.js`) — a contribuição de
//   outro MOD vence, por ser escolha de quem hospeda; depois o perfil; depois o
//   ícone nativo; por fim as iniciais;
// - **o cartão**, a mesma peça no diretório «Perfis», na folha de outra pessoa
//   e na prévia viva do editor;
// - **o editor**, que é o «Seu perfil» (`#perfil`, `camada-nomear.js`) com os
//   campos do perfil: a abertura, a saída única e o SALVAR moram lá; o que é do
//   perfil — os campos, a prévia, as imagens, as recusas por campo — mora aqui.
//
// **Num servidor 0.18 nada disto aparece.** O `perfis_revision` fica em zero
// (o servidor 11 não manda `Perfis`), o armazém fica vazio, e o MOD PERFIS,
// que ali ainda roda, segue desenhando pelas contribuições como sempre.

"use strict";

/** A cor de quem nunca escolheu uma: a do MOD, que a migração manteve. */
const COR_PADRAO_DO_PERFIL = "#f2521f";

/** As oito amostras de cor do editor (§5.4, item 7). */
const AMOSTRAS_DE_COR = Object.freeze([
  "#f2521f", "#f47732", "#ffd166", "#6bffb6", "#4fd1c5", "#2f8fe0", "#b48cff", "#eae3cf",
]);

/** Os efeitos, na ordem das pílulas; o nome é o do fio (`Efeito`). */
const EFEITOS_DO_PERFIL = Object.freeze(["Nenhum", "Aurora", "Brilho", "Pulso"]);

/** O que o armazém sabe da sessão da vista. */
const perfisOficiais = {
  /** `String(pessoa)` → `PerfilNaTela`, como o `perfis` devolve. */
  porPessoa: new Map(),
  /** O `perfis_revision` da última leitura pedida; `null` relê no próximo quadro. */
  marca: null,
  /** A leitura em curso, ou `null`. */
  lendo: null,
  relerDepois: false,
  /** `perfis_revision > 0`: o servidor fala o protocolo 12 e mandou a lista. */
  disponivel: false,
  /** Os lados do próprio perfil que a migração deixou para trás. */
  pendencia: [],
  /** Quem usa esta janela, no servidor da vista. */
  me: null,
};

/**
 * **As imagens, pelo hash** — o endereço delas, que não muda: a mesma imagem
 * nunca é pedida duas vezes nesta janela. `{ estado, url, estatico, falhouEm }`.
 */
const imagensDosPerfis = new Map();

/** Quantas imagens o mapa segura antes de esquecer as mais antigas. */
const TETO_DE_IMAGENS_DOS_PERFIS = 400;

/** Uma imagem que falhou é pedida de novo depois disto, e não a cada quadro. */
const ESPERA_DEPOIS_DA_FALHA_MS = 30_000;

/** O perfil oficial de uma pessoa, ou `null`. */
function perfilOficial(id) {
  if (id === null || id === undefined || id === "") return null;
  return perfisOficiais.porPessoa.get(String(id)) ?? null;
}

/** Se o servidor da vista tem perfis oficiais (protocolo 12). */
function perfisOficiaisDisponiveis() {
  return perfisOficiais.disponivel;
}

/**
 * A imagem pronta de um lado do perfil, ou `null` — e, quando ainda não foi
 * pedida, o pedido. Quem chama desenha o degrau seguinte enquanto isso, e o
 * redesenho vem quando ela chegar.
 */
function imagemOficialPronta(id, lado) {
  const perfil = perfilOficial(id);
  const imagem = lado === "Capa" ? perfil?.capa : perfil?.foto;
  if (!imagem?.hash) return null;
  const guardada = imagensDosPerfis.get(imagem.hash);
  if (guardada?.estado === "pronta") return guardada;
  const podeTentar = !guardada
    || (guardada.estado === "falhou" && Date.now() - guardada.falhouEm > ESPERA_DEPOIS_DA_FALHA_MS);
  if (podeTentar) carregarImagemDoPerfil(perfil.pessoa, lado, imagem);
  return null;
}

/** A foto oficial em `data:`, ou `null` (o degrau 2 de `aplicarAvatar`). */
function fotoOficial(id) {
  return imagemOficialPronta(id, "Foto")?.url ?? null;
}

/** A capa oficial `{ url, estatico }`, ou `null` (o degrau 2 de `aplicarCapa`). */
function capaOficial(id) {
  const pronta = imagemOficialPronta(id, "Capa");
  return pronta ? { url: pronta.url, estatico: pronta.estatico } : null;
}

/** O primeiro quadro de uma imagem animada, para o movimento reduzido. */
async function quadroParadoDoPerfil(url) {
  const imagem = new Image();
  imagem.src = url;
  try {
    await imagem.decode();
  } catch {
    return null;
  }
  return typeof quadroEstatico === "function" ? quadroEstatico(imagem) : null;
}

/** Esquece as imagens mais antigas quando o mapa passa do teto. */
function podarImagensDosPerfis() {
  while (imagensDosPerfis.size > TETO_DE_IMAGENS_DOS_PERFIS) {
    const primeira = imagensDosPerfis.keys().next().value;
    imagensDosPerfis.delete(primeira);
  }
}

/**
 * Pede uma imagem pelo hash. `Mudou` quer dizer que o perfil trocou a imagem
 * entre a leitura e o pedido: o armazém relê, e a nova vem pelo hash novo.
 * Qualquer outra falha é dita no console e tentada de novo mais tarde — uma
 * foto que não chega não pode ser uma pergunta sem dado dias depois.
 */
async function carregarImagemDoPerfil(pessoa, lado, imagem) {
  const registro = { estado: "carregando", url: null, estatico: null, falhouEm: 0 };
  imagensDosPerfis.set(imagem.hash, registro);
  podarImagensDosPerfis();
  try {
    const url = await invoke("imagem_do_perfil", { pessoa, lado, hash: imagem.hash });
    registro.url = url;
    // Só GIF e WebP animam; das outras, a própria imagem já é o quadro parado.
    const animada = /gif|webp/.test(imagem.tipo ?? "");
    registro.estatico = animada ? await quadroParadoDoPerfil(url) : url;
    registro.estado = "pronta";
  } catch (falha) {
    if (falha === "Mudou" || falha?.Mudou !== undefined) {
      imagensDosPerfis.delete(imagem.hash);
      perfisOficiais.marca = null;
      return;
    }
    console.warn(`imagem_do_perfil (${lado} de ${pessoa}):`, falha);
    registro.estado = "falhou";
    registro.falhouEm = Date.now();
    return;
  }
  agendarRedesenhoDosPerfis();
}

/**
 * **O armazém acompanha o retrato**, chamado por `desenhar` a cada quadro: a
 * lista é relida só quando a marca anda, e uma leitura por vez.
 */
function sincronizarPerfis(snapshot) {
  if (!snapshot) return;
  const marca = Number(snapshot.perfis_revision ?? 0);
  const pendencia = Array.isArray(snapshot.pendencia_do_perfil) ? snapshot.pendencia_do_perfil : [];
  let mudou = false;
  if (perfisOficiais.me !== (snapshot.me ?? null)) {
    perfisOficiais.me = snapshot.me ?? null;
    mudou = true;
    // Uma sessão nova na vista: quem hospeda lê o relatório da migração.
    if (perfisOficiais.me !== null && typeof desenharRelatorioDosPerfis === "function") {
      desenharRelatorioDosPerfis().catch((falha) => console.warn("relatório dos perfis:", falha));
    }
  }
  const disponivel = marca > 0;
  if (perfisOficiais.disponivel !== disponivel) {
    perfisOficiais.disponivel = disponivel;
    mudou = true;
  }
  if (pendencia.join() !== perfisOficiais.pendencia.join()) {
    perfisOficiais.pendencia = [...pendencia];
    mudou = true;
  }
  if (mudou) agendarRedesenhoDosPerfis();
  if (marca === perfisOficiais.marca) return;
  if (perfisOficiais.lendo) {
    perfisOficiais.relerDepois = true;
    return;
  }
  perfisOficiais.marca = marca;
  if (!disponivel) {
    if (perfisOficiais.porPessoa.size > 0) {
      perfisOficiais.porPessoa = new Map();
      agendarRedesenhoDosPerfis();
    }
    return;
  }
  perfisOficiais.lendo = lerOsPerfis().finally(() => {
    perfisOficiais.lendo = null;
    if (perfisOficiais.relerDepois) {
      perfisOficiais.relerDepois = false;
      perfisOficiais.marca = null;
      if (typeof desenhado !== "undefined" && desenhado) sincronizarPerfis(desenhado);
    }
  });
}

async function lerOsPerfis() {
  let lista;
  try {
    lista = await invoke("perfis");
  } catch (falha) {
    // A leitura que falhou é dita, e tentada de novo no próximo quadro depois
    // de um respiro: nada de uma volta de IPC por quadro.
    console.warn("perfis:", falha);
    setTimeout(() => {
      perfisOficiais.marca = null;
    }, 2000);
    return;
  }
  const novo = new Map();
  for (const perfil of Array.isArray(lista) ? lista : []) novo.set(String(perfil.pessoa), perfil);
  perfisOficiais.porPessoa = novo;
  agendarRedesenhoDosPerfis();
  avisarQuemEsperaOPerfil();
  // **O perfil padrão** (`perfil-padrao.js`): com a lista nova, a conferência
  // de «não tenho perfil aqui» e de «este servidor segue o padrão».
  // Depois de quem esperava esta lista seguir (o SALVAR do editor, que desliga
  // o «segue» deste servidor antes de a conferência o ler).
  if (typeof conferirOPadrao === "function") {
    setTimeout(() => conferirOPadrao().catch((falha) => console.warn("perfil padrão:", falha)), 0);
  }
}

/** A sessão acabou: o armazém da próxima começa vazio. As imagens ficam (o hash é o endereço). */
function esquecerOsPerfis() {
  perfisOficiais.porPessoa = new Map();
  perfisOficiais.marca = null;
  perfisOficiais.relerDepois = false;
  perfisOficiais.disponivel = false;
  perfisOficiais.pendencia = [];
  perfisOficiais.me = null;
  if (typeof esquecerOPadraoDaSessao === "function") esquecerOPadraoDaSessao();
  fecharDiretorioDosPerfis();
  fecharFolhaDoPerfil();
  desenharEntradaDosPerfis();
  const relatorio = $("relatorio-perfis");
  if (relatorio) relatorio.hidden = true;
}

/** Um redesenho por quadro, por mais imagens e listas que cheguem nele. */
let redesenhoDosPerfisAgendado = false;
function agendarRedesenhoDosPerfis() {
  if (redesenhoDosPerfisAgendado) return;
  redesenhoDosPerfisAgendado = true;
  requestAnimationFrame(() => {
    redesenhoDosPerfisAgendado = false;
    redesenharOsPerfis();
  });
}

/** Tudo o que mostra um perfil, com o armazém de agora. */
function redesenharOsPerfis() {
  if (typeof redesenharAsPessoas === "function") redesenharAsPessoas();
  if (typeof redesenharAvatares === "function") redesenharAvatares();
  if (typeof redesenharCapas === "function") redesenharCapas();
  document.querySelectorAll("[data-moldura-do-perfil]").forEach((no) => vestirMolduraDoPerfil(no, no.dataset.molduraDoPerfil));
  desenharEntradaDosPerfis();
  if (diretorioDosPerfisAberto()) desenharDiretorioDosPerfis();
  if (folhaDoPerfil.id !== null) desenharFolhaDoPerfil();
  if (editorDoPerfil.aberto) atualizarEditorDoPerfil();
}

// --------------------------------------------------------- a moldura do efeito

/**
 * **A moldura do efeito** (§5.2): um anel na cor da pessoa em volta da foto,
 * animado pela folha do produto (`perfis.css`), que para com o movimento
 * reduzido. Só em volta da foto do perfil: quando outro MOD dá o avatar, a
 * moldura do perfil em volta dele seria o perfil assinando o que não é dele.
 */
function vestirMolduraDoPerfil(no, id, perfil = perfilOficial(id)) {
  if (!no) return no;
  no.dataset.molduraDoPerfil = id === null || id === undefined ? "" : String(id);
  const doMod = typeof avatarContribuido === "function" && id !== null && id !== undefined && id !== ""
    ? avatarContribuido(String(id)) : null;
  const efeito = !doMod && perfil?.efeito && perfil.efeito !== "Nenhum" ? perfil.efeito.toLowerCase() : null;
  if (efeito) {
    no.dataset.efeito = efeito;
    no.style.setProperty("--perfil-cor", perfil.cor || COR_PADRAO_DO_PERFIL);
  } else {
    delete no.dataset.efeito;
    no.style.removeProperty("--perfil-cor");
  }
  return no;
}

// ------------------------------------------------------------------ o cartão

/** O apelido e a tag de uma pessoa, como o servidor a apresenta. */
function apelidoETagDe(id) {
  const par = typeof identidadesApresentadas !== "undefined" ? identidadesApresentadas?.[String(id)] : null;
  if (Array.isArray(par)) return { apelido: par[0] ?? "", tag: par[1] ?? "" };
  const nome = typeof apelidoApresentado === "function" ? apelidoApresentado(Number(id), "") : "";
  return { apelido: nome || `pessoa ${id}`, tag: "" };
}

/** Se o servidor já disse quem a pessoa é (`IdentidadeApresentada`, protocolo 11 em diante). */
function identidadeConhecida(id) {
  return typeof identidadesApresentadas !== "undefined" && Array.isArray(identidadesApresentadas?.[String(id)]);
}

/** A linha «pronomes · status», ou vazio. */
function linhaDoPerfil(perfil) {
  return [perfil?.pronomes, perfil?.status].map((parte) => (parte ?? "").trim()).filter(Boolean).join(" · ");
}

/**
 * **O cartão de uma pessoa**: a capa recortada ao fundo de uma faixa (nunca
 * dita o tamanho), a foto com a moldura, o nome exibido, o apelido#tag, a
 * linha de pronomes e status e a bio. A capa e a foto passam por
 * `vestirCapa`/`vestirAvatar`: a mesma precedência e o mesmo teto de capas
 * animadas de toda a tela.
 *
 * `perfil` é o do armazém, ou o rascunho do editor (a prévia viva).
 */
function montarCartaoDoPerfil(id, { perfil = perfilOficial(id), comoBotao = false, comBio = true, apelido: apelidoDado = null, tag: tagDada = null, imagens = null } = {}) {
  // `apelido` e `imagens` são do **perfil padrão** (`perfil-padrao.js`), que
  // não é de pessoa nenhuma de servidor nenhum: o apelido vem do campo, e a
  // foto e a capa, dos `data:` que esta máquina guarda. Na prévia do editor
  // dentro do servidor, o apelido é o do campo e a tag é a da chave: o nome
  // que o SALVAR vai registrar, com a tag que o servidor mantém.
  const { apelido, tag } = apelidoDado === null ? apelidoETagDe(id) : { apelido: apelidoDado, tag: tagDada ?? "" };
  const nome = perfil?.nome_exibido?.trim() || apelido;
  const raiz = elemento(comoBotao ? "button" : "article", "perfil-cartao");
  raiz.dataset.pessoa = String(id);
  raiz.style.setProperty("--perfil-cor", perfil?.cor || COR_PADRAO_DO_PERFIL);
  if (comoBotao) {
    raiz.type = "button";
    raiz.setAttribute("aria-label", FRASES_DOS_PERFIS.verPerfilDe(nome));
  }
  const capa = elemento("span", "perfil-cartao-capa");
  capa.setAttribute("aria-hidden", "true");
  const foto = elemento("span", "perfil-cartao-foto", iniciaisDoApelido(apelido));
  foto.setAttribute("aria-hidden", "true");
  if (imagens) {
    vestirComAsImagensDoPadrao(capa, foto, imagens);
    vestirMolduraDoPerfil(foto, null, perfil);
    // Sem a marca: o redesenho dos perfis relê a moldura pelo id, e o padrão não tem id.
    delete foto.dataset.molduraDoPerfil;
  } else {
    // Sem capa, a faixa já pinta a cor do perfil (`--perfil-cor`, a do
    // rascunho na prévia): a camada de cor de `aplicarCapa`, que lê a cor do
    // armazém, mostraria a cor de antes enquanto se escolhe outra.
    capa.dataset.corPropria = "sim";
    vestirCapa(capa, id);
    vestirAvatar(foto, id);
    vestirMolduraDoPerfil(foto, id, perfil);
  }
  const nomes = elemento("span", "perfil-cartao-nomes");
  nomes.append(
    elemento("span", "perfil-cartao-nome", nome),
    elemento("span", "perfil-cartao-apelido", tag ? `${apelido}#${tag}` : apelido),
  );
  if (String(id) === String(perfisOficiais.me)) nomes.append(elemento("span", "perfil-cartao-voce", FRASES_DOS_PERFIS.voce));
  const corpo = elemento("span", "perfil-cartao-corpo");
  corpo.append(foto, nomes);
  const linha = linhaDoPerfil(perfil);
  if (linha) corpo.append(elemento("span", "perfil-cartao-linha", linha));
  if (comBio && perfil?.bio?.trim()) corpo.append(elemento("span", "perfil-cartao-bio", perfil.bio.trim()));
  raiz.append(capa, corpo);
  return raiz;
}

/**
 * A foto e a capa do perfil padrão num cartão: direto, sem a precedência (o
 * padrão não tem MOD nem ícone nativo por cima) e sem `data-pessoa-da-capa`,
 * que o redesenho das capas releria por um id que o padrão não tem.
 */
function vestirComAsImagensDoPadrao(capa, foto, { foto: urlDaFoto = null, capa: urlDaCapa = null } = {}) {
  if (urlDaCapa) {
    const camada = elemento("span", "capa-camada");
    camada.setAttribute("aria-hidden", "true");
    camada.style.backgroundImage = `url(${urlDaCapa})`;
    capa.prepend(camada);
    capa.dataset.comCapa = "sim";
  }
  if (urlDaFoto) {
    foto.style.backgroundImage = `url(${urlDaFoto})`;
    foto.dataset.comRetrato = "sim";
    foto.textContent = "";
  }
}

// -------------------------------------------- a entrada «Perfis» e a pendência

/**
 * A entrada fixa «Perfis» na coluna do servidor, acima de «DOS MODS» (§9, Q5),
 * e o aviso gentil da pendência da migração logo abaixo dela. Escondidas num
 * servidor sem perfis oficiais.
 */
function desenharEntradaDosPerfis() {
  const lista = $("lista-perfis-navegacao");
  if (!lista) return;
  lista.hidden = !perfisOficiais.disponivel;
  const entrada = $("perfis-entrada");
  if (entrada) {
    if (diretorioDosPerfisAberto()) entrada.setAttribute("aria-current", "page");
    else entrada.removeAttribute("aria-current");
  }
  const aviso = $("perfis-pendencia");
  if (aviso) {
    const lados = perfisOficiais.disponivel ? perfisOficiais.pendencia : [];
    // A falha de aplicar o perfil padrão aqui (`perfil-padrao.js`) usa o mesmo
    // aviso gentil quando a migração não deixou nada: o produto que sabe que
    // não aplicou diz isso onde se olha, e não só no editor.
    const falhaDoPadrao = perfisOficiais.disponivel && typeof falhaDoPadraoNaVista === "function"
      ? falhaDoPadraoNaVista() : null;
    const texto = lados.length ? FRASES_DOS_PERFIS.pendenciaAviso(lados)
      : falhaDoPadrao ? FRASES_DOS_PERFIS.padrao.aviso : "";
    aviso.hidden = texto === "";
    aviso.textContent = texto;
  }
  $("tela-sessao")?.classList.toggle("com-diretorio-de-perfis", diretorioDosPerfisAberto());
}

// ------------------------------------------------------------- o diretório

/**
 * A folha do celular, quando a casca é a dele: o ajudante só existe lá
 * (`celular-folhas.js`). O desktop não lê a marca da casca.
 */
const folhaDoCelular = () => globalThis.CELULAR?.ajudante("folhaDeConteudo") ?? null;

/** Quem aparece no diretório: quem tem perfil e quem está no servidor agora. */
function pessoasDoDiretorio() {
  const ids = new Set(perfisOficiais.porPessoa.keys());
  const retrato = typeof desenhado !== "undefined" ? desenhado : null;
  for (const pessoa of retrato?.presentes ?? []) if (pessoa?.id !== undefined && pessoa?.id !== null) ids.add(String(pessoa.id));
  const eu = perfisOficiais.me === null ? null : String(perfisOficiais.me);
  if (eu !== null) ids.add(eu);
  const nome = (id) => (perfilOficial(id)?.nome_exibido || apelidoETagDe(id).apelido).toLocaleLowerCase();
  return [...ids].sort((a, b) => {
    if (a === eu) return -1;
    if (b === eu) return 1;
    const comA = perfilOficial(a) ? 0 : 1;
    const comB = perfilOficial(b) ? 0 : 1;
    return comA - comB || nome(a).localeCompare(nome(b));
  });
}

function diretorioDosPerfisAberto() {
  const pagina = $("perfis-diretorio");
  return Boolean(pagina && !pagina.hidden);
}

/** A grade de cartões; cada cartão abre a folha da pessoa. */
function gradeDosPerfis() {
  const lista = elemento("ul", "perfis-grade");
  const ids = pessoasDoDiretorio();
  for (const id of ids) {
    const item = elemento("li", "perfis-grade-item");
    const cartao = montarCartaoDoPerfil(id, { comoBotao: true, comBio: true });
    cartao.addEventListener("click", () => abrirPerfilDaPessoa(id));
    item.append(cartao);
    lista.append(item);
  }
  return { lista, quantos: ids.length };
}

function desenharDiretorioDosPerfis() {
  const corpo = $("perfis-diretorio-corpo");
  if (!corpo) return;
  const { lista, quantos } = gradeDosPerfis();
  $("perfis-diretorio-sub").textContent = FRASES_DOS_PERFIS.diretorioSub(quantos);
  const vazio = perfisOficiais.porPessoa.size === 0;
  const nota = elemento("p", "nota perfis-diretorio-vazio", FRASES_DOS_PERFIS.diretorioVazio);
  nota.hidden = !vazio;
  corpo.replaceChildren(nota, lista);
}

/**
 * **Abre o diretório**: no desktop e no navegador largo, uma página na coluna
 * central, por cima da conversa (como a página de um MOD); no celular, uma
 * folha de altura total, aberta pela entrada de AJUSTES.
 */
function abrirDiretorioDosPerfis() {
  const folha = folhaDoCelular();
  if (folha) {
    const { lista, quantos } = gradeDosPerfis();
    const editar = elemento("button", "botao-fantasma perfis-editar-o-seu", FRASES_DOS_PERFIS.editarOSeu);
    editar.type = "button";
    editar.addEventListener("click", () => abrirPerfil());
    const sub = elemento("p", "nota perfis-diretorio-sub", FRASES_DOS_PERFIS.diretorioSub(quantos));
    folha({
      id: "cel-folha-perfis",
      titulo: FRASES_DOS_PERFIS.diretorioTitulo,
      alta: true,
      conteudo: [sub, editar, lista],
    });
    return;
  }
  // Uma página de MOD aberta sai, como a conversa: o centro é um lugar só.
  if (typeof mostrarDestinoNativo === "function") mostrarDestinoNativo();
  $("perfis-diretorio").hidden = false;
  desenharDiretorioDosPerfis();
  desenharEntradaDosPerfis();
  $("perfis-diretorio-titulo").focus?.();
}

function fecharDiretorioDosPerfis() {
  const pagina = $("perfis-diretorio");
  if (!pagina || pagina.hidden) return;
  pagina.hidden = true;
  $("perfis-diretorio-corpo")?.replaceChildren();
  desenharEntradaDosPerfis();
}

// ------------------------------------------------- a folha de outra pessoa

/** A folha aberta: de quem, e o foco de antes. */
const folhaDoPerfil = { id: null, focoAntes: null };

/**
 * **O perfil de outra pessoa** (§5.3): no desktop, uma folha à direita no
 * lugar do painel PESSOAS; no celular, uma folha que sobe da borda. Capa,
 * foto, nome exibido, apelido#tag, pronomes, status e bio.
 */
function abrirPerfilDaPessoa(id) {
  if (id === null || id === undefined) return;
  // **0.19, pedido do dono: um modal, e não a folha à direita.** A ficha da
  // pessoa (`abrirFichaDaPessoa`, em `tela-sessao.js`) traz este mesmo
  // conteúdo (`conteudoDaFolhaDoPerfil`) num modal centrado no desktop, e na
  // folha que sobe da borda no celular — com o volume e o MODERAR junto.
  if (typeof abrirFichaDaPessoa === "function") {
    abrirFichaDaPessoa(id);
    return;
  }
  const folha = folhaDoCelular();
  if (folha) {
    const { apelido } = apelidoETagDe(id);
    const nome = perfilOficial(id)?.nome_exibido || apelido;
    folha({
      id: "cel-folha-perfil",
      titulo: FRASES_DOS_PERFIS.perfilDe(nome),
      conteudo: conteudoDaFolhaDoPerfil(id),
    });
    return;
  }
  folhaDoPerfil.focoAntes = document.activeElement;
  folhaDoPerfil.id = String(id);
  $("perfil-folha").hidden = false;
  desenharFolhaDoPerfil();
  $("perfil-folha-fechar").focus();
}

function conteudoDaFolhaDoPerfil(id) {
  const perfil = perfilOficial(id);
  const nos = [montarCartaoDoPerfil(id, { comBio: true })];
  if (!perfil) nos.push(elemento("p", "nota perfil-folha-vazia", FRASES_DOS_PERFIS.semPerfil));
  if (String(id) === String(perfisOficiais.me)) {
    const editar = elemento("button", "botao-fantasma perfis-editar-o-seu", FRASES_DOS_PERFIS.editarOSeu);
    editar.type = "button";
    editar.addEventListener("click", () => abrirPerfil());
    nos.push(editar);
  }
  return nos;
}

function desenharFolhaDoPerfil() {
  const id = folhaDoPerfil.id;
  if (id === null) return;
  const { apelido } = apelidoETagDe(id);
  const nome = perfilOficial(id)?.nome_exibido || apelido;
  $("perfil-folha-titulo").textContent = FRASES_DOS_PERFIS.perfilDe(nome);
  $("perfil-folha-fechar").setAttribute("aria-label", FRASES_DOS_PERFIS.fecharPerfilDe(nome));
  $("perfil-folha-corpo").replaceChildren(...conteudoDaFolhaDoPerfil(id));
}

function fecharFolhaDoPerfil() {
  const folha = $("perfil-folha");
  if (!folha || folha.hidden) {
    folhaDoPerfil.id = null;
    return;
  }
  folha.hidden = true;
  folhaDoPerfil.id = null;
  $("perfil-folha-corpo").replaceChildren();
  const antes = folhaDoPerfil.focoAntes;
  folhaDoPerfil.focoAntes = null;
  if (typeof focavel === "function" && focavel(antes)) antes.focus();
}

// ---------------------------------------------------------------- o editor

/**
 * O que o editor sabe: se está aberto no modo do perfil, o perfil como ele
 * estava ao abrir (`base`, para saber o que mudou e com que revisão gravar), e
 * quem espera o desfecho de um SALVAR.
 */
const editorDoPerfil = {
  aberto: false,
  /**
   * O nome que o campo mostrou ao abrir: o nome exibido do perfil, ou o
   * apelido quando o perfil não tem um. É contra ele que «tem alteração» se
   * mede — um perfil cujo nome exibido difere do apelido abre sem alteração, e
   * o SALVAR alinha os dois.
   */
  nomeNoCampo: "",
  /**
   * `true` quando os campos do perfil aparecem: no servidor da vista com
   * perfis oficiais (`modo` `"oficial"`) e na tela inicial, onde o que se
   * edita é o perfil padrão desta máquina (`modo` `"padrao"`).
   */
  oficial: false,
  /** `"oficial"`, `"padrao"` ou `"nativo"` (o «Seu perfil» de sempre: imagem e apelido). */
  modo: "nativo",
  base: null,
  apelido: "",
  esperando: [],
  estadoDaImagem: { Foto: "", Capa: "" },
};

/** Os campos do editor, pelo nome do campo no fio (`CampoDoPerfil`). */
const CAMPOS_DO_EDITOR = Object.freeze({
  // Um nome só (0.19): o campo do nome é o do apelido, e a recusa do
  // `nome_exibido` volta a ele.
  NomeExibido: { id: "perfil-apelido", chave: "nome_exibido" },
  Pronomes: { id: "perfil-pronomes", chave: "pronomes" },
  Status: { id: "perfil-status", chave: "status" },
  Bio: { id: "perfil-bio", chave: "bio" },
  Cor: { id: "perfil-cor-hex", chave: "cor" },
});

/** Os textos de um perfil (ou do padrão, para quem ainda não tem). */
function textosDoPerfil(perfil) {
  return {
    nome_exibido: perfil?.nome_exibido ?? "",
    pronomes: perfil?.pronomes ?? "",
    status: perfil?.status ?? "",
    bio: perfil?.bio ?? "",
    cor: (perfil?.cor ?? COR_PADRAO_DO_PERFIL).toLowerCase(),
    efeito: perfil?.efeito ?? "Nenhum",
  };
}

/** O que está escrito no editor agora, no formato do `gravar_perfil`. */
function entradaDoEditor() {
  const efeito = document.querySelector("#perfil-efeitos [aria-pressed='true']")?.dataset.efeito ?? "Nenhum";
  return {
    nome_exibido: $("perfil-apelido").value,
    pronomes: $("perfil-pronomes").value,
    status: $("perfil-status").value,
    bio: $("perfil-bio").value,
    cor: $("perfil-cor-hex").value.trim().toLowerCase(),
    efeito,
  };
}

/** Dois conjuntos de textos dizem o mesmo (aparados, como o servidor grava). */
function mesmosTextos(a, b) {
  return ["nome_exibido", "pronomes", "status", "bio"].every((k) => (a[k] ?? "").trim() === (b[k] ?? "").trim())
    && (a.cor ?? "").toLowerCase() === (b.cor ?? "").toLowerCase()
    && a.efeito === b.efeito;
}

/** Se o editor tem alguma coisa escrita que ainda não foi salva. */
function perfilTemAlteracoes() {
  if (!editorDoPerfil.aberto) return false;
  if ($("perfil-apelido").value.trim() !== editorDoPerfil.nomeNoCampo.trim()) return true;
  return editorDoPerfil.oficial
    && !mesmosTextos(entradaDoEditor(), { ...editorDoPerfil.base, nome_exibido: editorDoPerfil.nomeNoCampo });
}

/**
 * **Prepara o editor ao abrir** (`abrirPerfil`). Com perfis oficiais, os
 * campos aparecem e começam com o perfil de agora. **Fora de sessão**, com o
 * perfil padrão desta máquina lido (`padrao`, de `perfil-padrao.js`), os
 * mesmos campos editam o padrão. Num servidor 0.18 (ou sem padrão legível), o
 * «Seu perfil» é o de sempre: a imagem desta máquina e o apelido.
 */
function prepararOEditorDoPerfil(snapshot, apelido, padrao = null) {
  const noServidor = Boolean(snapshot && perfisOficiais.disponivel && snapshot.me !== null && snapshot.me !== undefined);
  const modo = noServidor ? "oficial" : !snapshot && padrao ? "padrao" : "nativo";
  const oficial = modo !== "nativo";
  editorDoPerfil.aberto = true;
  editorDoPerfil.oficial = oficial;
  editorDoPerfil.modo = modo;
  editorDoPerfil.apelido = apelido ?? "";
  editorDoPerfil.nomeNoCampo = editorDoPerfil.apelido;
  editorDoPerfil.estadoDaImagem = { Foto: "", Capa: "" };
  const raiz = $("perfil");
  raiz.dataset.oficial = oficial ? "sim" : "nao";
  raiz.dataset.modo = modo;
  for (const no of raiz.querySelectorAll("[data-so-oficial]")) no.hidden = !oficial;
  $("perfil-imagem").hidden = oficial;
  mostrarRodapeDeSaida(false);
  limparErrosDoEditor();
  // O andamento das imagens é de uma abertura: a seguinte começa limpa.
  for (const id of ["perfil-foto-estado", "perfil-capa-estado"]) {
    $(id).dataset.frase = "";
    $(id).dataset.tomDaFrase = "";
  }
  if (!oficial) {
    editorDoPerfil.base = null;
    if (typeof desenharFaixaDoPadrao === "function") desenharFaixaDoPadrao();
    return;
  }
  const fonte = modo === "padrao" ? padrao : perfilOficial(snapshot.me);
  editorDoPerfil.base = { ...textosDoPerfil(fonte), revisao: fonte?.revisao ?? 0 };
  preencherOEditor(editorDoPerfil.base);
  atualizarEditorDoPerfil();
}

/** As imagens que o editor mostra agora: as do perfil do servidor, ou as do padrão. */
function imagensDoEditor() {
  if (editorDoPerfil.modo === "padrao") {
    const padrao = typeof perfilPadraoAtual === "function" ? perfilPadraoAtual() : null;
    return { foto: padrao?.foto ?? null, capa: padrao?.capa ?? null, pessoa: null };
  }
  const atual = perfilOficial(perfisOficiais.me);
  return { foto: atual?.foto ?? null, capa: atual?.capa ?? null, pessoa: atual?.pessoa ?? null };
}

function preencherOEditor(textos) {
  // **Um nome só**: o nome exibido, e o apelido quando o perfil não tem um.
  const nome = (textos.nome_exibido ?? "").trim() || editorDoPerfil.apelido;
  $("perfil-apelido").value = nome;
  editorDoPerfil.nomeNoCampo = nome;
  $("perfil-pronomes").value = textos.pronomes;
  $("perfil-status").value = textos.status;
  $("perfil-bio").value = textos.bio;
  escolherACor(textos.cor);
  escolherOEfeito(textos.efeito);
  contarABio();
}

/** O editor fechou: nada mais espera por ele. */
function soltarOEditorDoPerfil() {
  editorDoPerfil.aberto = false;
  editorDoPerfil.oficial = false;
  editorDoPerfil.modo = "nativo";
  for (const esperando of editorDoPerfil.esperando.splice(0)) esperando({ fechou: true });
}

function escolherACor(cor) {
  const valor = /^#[0-9a-fA-F]{6}$/.test(cor ?? "") ? cor.toLowerCase() : (cor ?? "");
  $("perfil-cor-hex").value = valor;
  for (const amostra of $("perfil-amostras").querySelectorAll("button[data-cor]")) {
    amostra.setAttribute("aria-checked", amostra.dataset.cor === valor ? "true" : "false");
  }
}

function escolherOEfeito(efeito) {
  for (const pilula of $("perfil-efeitos").querySelectorAll("button[data-efeito]")) {
    pilula.setAttribute("aria-pressed", pilula.dataset.efeito === efeito ? "true" : "false");
  }
}

/** O contador da bio, em code points, como o servidor conta. */
function contarABio() {
  const max = FRASES_DOS_PERFIS.limites.Bio;
  const n = [...$("perfil-bio").value].length;
  const contador = $("perfil-bio-contador");
  contador.textContent = FRASES_DOS_PERFIS.contador(n, max);
  contador.dataset.passou = n > max ? "sim" : "nao";
}

/** Redesenha a prévia, os blocos das imagens e a pendência com o armazém de agora. */
function atualizarEditorDoPerfil() {
  if (typeof desenharFaixaDoPadrao === "function") desenharFaixaDoPadrao();
  if (!editorDoPerfil.aberto || !editorDoPerfil.oficial) return;
  const me = perfisOficiais.me;
  const imagens = imagensDoEditor();
  // A prévia viva: o rascunho dos textos, com as imagens que valem agora.
  const rascunho = { ...entradaDoEditor(), foto: imagens.foto, capa: imagens.capa };
  if (!/^#[0-9a-f]{6}$/.test(rascunho.cor)) rascunho.cor = editorDoPerfil.base?.cor ?? COR_PADRAO_DO_PERFIL;
  const previa = $("perfil-previa-cartao");
  const cartao = editorDoPerfil.modo === "padrao"
    ? montarCartaoDoPerfil(null, {
      perfil: rascunho,
      apelido: $("perfil-apelido").value.trim() || editorDoPerfil.apelido || "?",
      imagens: { foto: imagens.foto?.url || null, capa: imagens.capa?.url || null },
    })
    : montarCartaoDoPerfil(me, {
      perfil: rascunho,
      apelido: $("perfil-apelido").value.trim() || editorDoPerfil.apelido || "?",
      tag: apelidoETagDe(me).tag,
    });
  previa.replaceChildren(cartao);
  for (const lado of ["Foto", "Capa"]) desenharBlocoDaImagem(lado, imagens);
}

/**
 * O bloco de uma imagem: a amostra, os botões, a nota de formato e o estado.
 * `imagens` é o que `imagensDoEditor` dá: as do servidor (pelo hash, pedidas
 * pela ponte) ou as do padrão (já em `data:`).
 */
function desenharBlocoDaImagem(lado, imagens) {
  const prefixo = lado === "Capa" ? "perfil-capa" : "perfil-foto";
  const amostra = $(`${prefixo}-amostra`);
  const imagem = lado === "Capa" ? imagens?.capa : imagens?.foto;
  let url = null;
  if (imagem && editorDoPerfil.modo === "padrao") {
    url = imagem.url || null;
  } else if (imagem) {
    const pronta = imagemOficialPronta(imagens.pessoa, lado);
    url = lado === "Capa" ? (pronta?.estatico ?? pronta?.url) : pronta?.url;
  }
  if (url) {
    amostra.style.backgroundImage = `url(${url})`;
    amostra.dataset.tem = "sim";
    amostra.textContent = "";
  } else {
    amostra.style.removeProperty("background-image");
    amostra.dataset.tem = "nao";
    amostra.textContent = lado === "Foto" ? iniciaisDoApelido($("perfil-apelido").value || editorDoPerfil.apelido || "?") : "";
  }
  $(`${prefixo}-remover`).disabled = !imagem || editorDoPerfil.estadoDaImagem[lado] === "enviando";
  $(`${prefixo}-escolher`).disabled = editorDoPerfil.estadoDaImagem[lado] === "enviando";
  // O estado do bloco: a recusa ou o andamento, e, sem nada disso, a pendência
  // da migração. Sempre no próprio bloco, onde se acabou de apertar.
  const estado = $(`${prefixo}-estado`);
  const frase = estado.dataset.frase ?? "";
  const pendente = editorDoPerfil.modo === "oficial" && perfisOficiais.pendencia.includes(lado) && !imagem;
  const texto = frase || (pendente ? FRASES_DOS_PERFIS.pendencia(lado) : "");
  estado.textContent = texto;
  estado.hidden = texto === "";
  estado.dataset.tom = estado.dataset.tomDaFrase || (pendente ? "pendencia" : "");
}

/** Escreve (ou limpa) a frase do bloco de uma imagem. */
function dizerNoBlocoDaImagem(lado, frase, tom = "") {
  const estado = $(lado === "Capa" ? "perfil-capa-estado" : "perfil-foto-estado");
  estado.dataset.frase = frase;
  estado.dataset.tomDaFrase = tom;
  desenharBlocoDaImagem(lado, imagensDoEditor());
}

/**
 * **ESCOLHER manda na hora** (F4: o comando abre o seletor e sobe): o bloco
 * diz «Enviando…» e depois «enviada», ou a recusa — o HEIC pelo nome.
 */
async function escolherImagemDoPerfil(lado) {
  // Na tela inicial, a imagem vai para o perfil padrão desta máquina.
  if (editorDoPerfil.modo === "padrao") return escolherImagemDoPadrao(lado);
  editorDoPerfil.estadoDaImagem[lado] = "enviando";
  dizerNoBlocoDaImagem(lado, FRASES_DOS_PERFIS.enviando);
  let feito;
  try {
    feito = await invoke("enviar_imagem_do_perfil", { lado });
  } catch (falha) {
    console.warn(`enviar_imagem_do_perfil (${lado}):`, falha);
    editorDoPerfil.estadoDaImagem[lado] = "";
    dizerNoBlocoDaImagem(lado, fraseDaImagemDoPerfil(falha), "erro");
    return;
  }
  editorDoPerfil.estadoDaImagem[lado] = "";
  if (!feito?.escolheu) {
    dizerNoBlocoDaImagem(lado, "");
    return;
  }
  const nota = lado === "Foto" && feito.icone_nativo ? ` ${FRASES_DOS_PERFIS.iconeAntigo}` : "";
  dizerNoBlocoDaImagem(lado, FRASES_DOS_PERFIS.enviada(lado) + nota, "feito");
  anunciar(FRASES_DOS_PERFIS.enviada(lado));
  // Mexer no perfil aqui é personalizar este servidor: ele deixa de seguir o padrão.
  if (typeof deixarDeSeguirOPadrao === "function") deixarDeSeguirOPadrao();
}

/** ESCOLHER na tela inicial: o seletor, a conferência, e a imagem guardada no padrão. */
async function escolherImagemDoPadrao(lado) {
  editorDoPerfil.estadoDaImagem[lado] = "enviando";
  dizerNoBlocoDaImagem(lado, "");
  let feito;
  try {
    feito = await invoke("escolher_imagem_do_perfil_padrao", { lado });
    if (feito?.escolheu) await lerOPerfilPadrao();
  } catch (falha) {
    console.warn(`escolher_imagem_do_perfil_padrao (${lado}):`, falha);
    editorDoPerfil.estadoDaImagem[lado] = "";
    dizerNoBlocoDaImagem(lado, fraseDaImagemDoPerfil(falha), "erro");
    return;
  }
  editorDoPerfil.estadoDaImagem[lado] = "";
  if (!feito?.escolheu) {
    dizerNoBlocoDaImagem(lado, "");
    return;
  }
  dizerNoBlocoDaImagem(lado, FRASES_DOS_PERFIS.padrao.guardada(lado), "feito");
  atualizarEditorDoPerfil();
  anunciar(FRASES_DOS_PERFIS.padrao.guardada(lado));
}

async function removerImagemDoPerfil(lado) {
  const doPadrao = editorDoPerfil.modo === "padrao";
  editorDoPerfil.estadoDaImagem[lado] = "enviando";
  dizerNoBlocoDaImagem(lado, FRASES_DOS_PERFIS.removendo);
  try {
    if (doPadrao) {
      await invoke("tirar_imagem_do_perfil_padrao", { lado });
      await lerOPerfilPadrao();
    } else {
      await invoke("tirar_imagem_do_perfil", { lado });
    }
    editorDoPerfil.estadoDaImagem[lado] = "";
    const frase = doPadrao ? FRASES_DOS_PERFIS.padrao.removida(lado) : FRASES_DOS_PERFIS.removida(lado);
    dizerNoBlocoDaImagem(lado, frase, "feito");
    anunciar(frase);
    if (doPadrao) atualizarEditorDoPerfil();
    else if (typeof deixarDeSeguirOPadrao === "function") deixarDeSeguirOPadrao();
  } catch (falha) {
    console.warn(`tirar_imagem_do_perfil (${lado}):`, falha);
    editorDoPerfil.estadoDaImagem[lado] = "";
    dizerNoBlocoDaImagem(lado, fraseDaImagemDoPerfil(falha), "erro");
  }
}

// -------------------------------------------------------- as recusas, por campo

function limparErrosDoEditor() {
  for (const { id } of Object.values(CAMPOS_DO_EDITOR)) {
    const erro = $(`${id}-erro`);
    if (erro) {
      erro.hidden = true;
      erro.textContent = "";
    }
    $(id)?.removeAttribute("aria-invalid");
  }
  const geral = $("perfil-erro");
  geral.hidden = true;
  geral.textContent = "";
  $("perfil-recarregar").hidden = true;
}

/** A frase de um `ErroDeCampo`. */
function fraseDoCampo({ campo, motivo }) {
  const f = FRASES_DOS_PERFIS.campo;
  if (motivo === "Longo") return f.Longo(FRASES_DOS_PERFIS.limites[campo] ?? "");
  if (motivo === "Formato") return f.Formato;
  return campo === "Bio" ? f.Controle : f.ControleSemQuebra;
}

/** **Cada recusa volta ao campo dela** (§5.4), e o foco vai à primeira. */
function mostrarErrosDeCampo(campos) {
  let primeiro = null;
  for (const erro of campos ?? []) {
    const alvo = CAMPOS_DO_EDITOR[erro.campo];
    if (!alvo) continue;
    const onde = $(`${alvo.id}-erro`);
    onde.textContent = fraseDoCampo(erro);
    onde.hidden = false;
    $(alvo.id).setAttribute("aria-invalid", "true");
    primeiro ??= $(alvo.id);
  }
  primeiro?.focus();
}

function mostrarErroDoPerfil(frase, { recarregar = false } = {}) {
  const geral = $("perfil-erro");
  geral.textContent = frase;
  geral.hidden = false;
  $("perfil-recarregar").hidden = !recarregar;
}

// ------------------------------------------------------------- o nome
//
// **Um nome só** (0.19, pedido do dono). O editor tinha o apelido e o nome
// exibido, e a lista mostrava os dois; agora o campo é um, e o SALVAR o manda
// ao registro do servidor (`SetNickname`, que mantém a tag da chave) e depois
// ao perfil (`GravarPerfil`, o `nome_exibido`). A regra é a mais estrita das
// duas: não vazio, o teto do fio do apelido (em bytes, lido da FFI pelo
// `passaDoTeto`), os caracteres que o perfil recusa e o limite do nome exibido.

/**
 * Se `c` é recusado no nome: o `seele_proto::perfil::e_recusado` fora da bio
 * (os controles C0 e C1, U+061C, LRM/RLM, U+202A–202E e U+2066–2069; a quebra
 * de linha é um C0). O guarda `o_nome_recusa_os_mesmos_caracteres_que_o_perfil`
 * o confere contra o Rust, código a código.
 */
function caractereRecusadoNoNome(c) {
  const n = c.codePointAt(0);
  return n <= 0x1f || (n >= 0x7f && n <= 0x9f) || n === 0x061c || n === 0x200e || n === 0x200f
    || (n >= 0x202a && n <= 0x202e) || (n >= 0x2066 && n <= 0x2069);
}

/**
 * **A conferência do nome antes de qualquer pedido**: a frase da recusa, ou
 * `null`. `nome` já vem aparado, como os dois lados gravam.
 */
async function conferirONome(nome) {
  const f = FRASES_DOS_PERFIS;
  if (nome === "") return f.nome.vazio;
  if ([...nome].some(caractereRecusadoNoNome)) return f.campo.ControleSemQuebra;
  const grandeDemais = typeof passaDoTeto === "function" ? await passaDoTeto("apelido", nome) : null;
  if (grandeDemais) return grandeDemais;
  if ([...nome].length > f.limites.NomeExibido) return f.campo.Longo(f.limites.NomeExibido);
  return null;
}

/** Diz no campo do nome por que ele não passou, e leva o teclado até lá. */
function dizerNoNome(frase) {
  const onde = $("perfil-apelido-erro");
  onde.textContent = frase;
  onde.hidden = false;
  $("perfil-apelido").setAttribute("aria-invalid", "true");
  $("perfil-apelido").focus();
}

/** Quem espera a recusa de um `SetNickname` (o `PedidoRecusado` do servidor). */
const esperandoOApelido = new Set();

/**
 * **O nome ao registro do servidor, e o desfecho.** O `SetNickname` não tem
 * resposta: o sim é a identidade apresentada da pessoa (`[base, tag]`)
 * passar a ter este nome; o não é o aviso `PedidoRecusado` do `SetNickname`,
 * que só quem pediu recebe; e sem nenhum dos dois, o prazo. `{ ok }` ou
 * `{ frase }` — e quem recebe a frase não grava o perfil: um nome exibido que
 * o registro recusou seria o nome divergente que este pedido veio acabar.
 */
async function mandarONomeAoServidor(nome, { carimbo = null, prazoMs = 10_000 } = {}) {
  let feito = false;
  let prazo = null;
  let resolver;
  const desfecho = new Promise((r) => { resolver = r; });
  const acabar = (valor) => {
    if (feito) return;
    feito = true;
    clearTimeout(prazo);
    esperandoOApelido.delete(recusar);
    resolver(valor);
  };
  const recusar = (causa) => acabar({ frase: FRASES_DOS_PERFIS.nome.recusado(causa) });
  const olhar = () => {
    if (feito) return;
    if (apelidoETagDe(perfisOficiais.me).apelido.trim() === nome) return acabar({ ok: true });
    setTimeout(olhar, 50);
  };
  esperandoOApelido.add(recusar);
  prazo = setTimeout(() => acabar({ frase: FRASES_DOS_PERFIS.nome.semResposta }), prazoMs);
  try {
    await invoke("escolher_apelido", carimbo ? { apelido: nome, carimbo } : { apelido: nome });
  } catch (falha) {
    console.warn("escolher_apelido:", falha);
    acabar({ frase: fraseDeErro(falha), falha });
    return desfecho;
  }
  olhar();
  return desfecho;
}

// ------------------------------------------------------------- o SALVAR

/** Acorda quem espera o desfecho de um SALVAR, com o armazém novo. */
function avisarQuemEsperaOPerfil(evento = null) {
  for (const esperando of [...editorDoPerfil.esperando]) esperando(evento);
  // E quem aplica o perfil padrão (`perfil-padrao.js`), que espera o mesmo.
  if (typeof avisarQuemAplicaOPadrao === "function") avisarQuemAplicaOPadrao(evento);
}

/**
 * **O desfecho de um `GravarPerfil`**: o servidor não responde ao pedido, ele
 * difunde o `PerfilMudou` (o sim) ou manda o `PerfilRecusado` só a quem pediu
 * (o não). Espera um dos dois, ou o prazo.
 */
function esperarODesfechoDoPerfil(entrada, revisao) {
  return new Promise((resolver) => {
    let feito = false;
    const acabar = (valor) => {
      if (feito) return;
      feito = true;
      clearTimeout(prazo);
      editorDoPerfil.esperando = editorDoPerfil.esperando.filter((f) => f !== olhar);
      resolver(valor);
    };
    const olhar = (evento) => {
      if (evento?.fechou) return acabar({ fechou: true });
      if (evento?.PerfilRecusado) return acabar({ recusa: evento.PerfilRecusado });
      const meu = perfilOficial(perfisOficiais.me);
      if (meu && meu.revisao > revisao && mesmosTextos(textosDoPerfil(meu), entrada)) acabar({ ok: true });
    };
    const prazo = setTimeout(() => acabar({ semResposta: true }), 10_000);
    editorDoPerfil.esperando.push(olhar);
  });
}

/**
 * **A revisão com que gravar.** Trocar a foto ou a capa sobe a revisão, e
 * isso não é conflito: se os textos de agora são os que o editor viu ao abrir,
 * vale a revisão de agora. Se os textos mudaram por baixo (outra janela), vale
 * a de quando abriu — e o servidor responde `RevisaoVelha`, que é a verdade.
 */
function revisaoParaGravar() {
  const atual = perfilOficial(perfisOficiais.me);
  if (atual && mesmosTextos(textosDoPerfil(atual), editorDoPerfil.base)) return atual.revisao;
  return editorDoPerfil.base?.revisao ?? 0;
}

/**
 * **SALVAR, a parte do perfil**: `GravarPerfil` com os textos, se mudaram, e
 * a espera do desfecho. `true` fecha o editor. O nome já foi antes ao registro
 * do servidor (`SetNickname`, por `guardarOApelido`), e só seguiu até aqui
 * porque o servidor o aceitou; as imagens já foram ao escolher.
 */
async function gravarOPerfilOficial() {
  if (!editorDoPerfil.oficial) return true;
  if (editorDoPerfil.modo === "padrao") return gravarOPerfilPadrao();
  limparErrosDoEditor();
  const entrada = entradaDoEditor();
  if (mesmosTextos(entrada, editorDoPerfil.base)) return true;
  const salvar = $("perfil-salvar");
  salvar.disabled = true;
  try {
    let desfecho = await mandarOPerfil(entrada);
    // A troca de imagem desta janela subiu a revisão entre a leitura e o
    // pedido: os textos ainda são os que o editor viu, e o segundo pedido vai
    // com a revisão de agora.
    if (desfecho.recusa?.motivo === "RevisaoVelha"
      && mesmosTextos(textosDoPerfil(perfilOficial(perfisOficiais.me)), editorDoPerfil.base)) {
      desfecho = await mandarOPerfil(entrada);
    }
    if (desfecho.ok) {
      anunciar(FRASES_DOS_PERFIS.salvo);
      // Os textos mudaram aqui: este servidor passa a ter o perfil dele.
      if (typeof deixarDeSeguirOPadrao === "function") deixarDeSeguirOPadrao();
      return true;
    }
    if (desfecho.fechou) return false;
    if (desfecho.semResposta) {
      mostrarErroDoPerfil(FRASES_DOS_PERFIS.semResposta);
      return false;
    }
    const { motivo, campos } = desfecho.recusa;
    if (motivo === "Campos") mostrarErrosDeCampo(campos);
    else if (motivo === "RevisaoVelha") mostrarErroDoPerfil(FRASES_DOS_PERFIS.revisaoVelha, { recarregar: true });
    else mostrarErroDoPerfil(fraseDaImagemDoPerfil({ Recusada: { motivo } }));
    return false;
  } catch (falha) {
    // As recusas antes do fio: a mesma validação do servidor, por campo.
    if (falha?.Campos) {
      mostrarErrosDeCampo(falha.Campos.campos);
      return false;
    }
    console.warn("gravar_perfil:", falha);
    mostrarErroDoPerfil(falha?.ServidorAntigo ? FRASES_DOS_PERFIS.imagem.ServidorAntigo : fraseDeErro(falha));
    return false;
  } finally {
    salvar.disabled = false;
  }
}

/**
 * **SALVAR na tela inicial**: os textos vão ao perfil padrão desta máquina,
 * validados pela regra do servidor — a recusa volta por campo antes de
 * qualquer servidor. Depois, quem segue o padrão o recebe (`perfil-padrao.js`).
 */
async function gravarOPerfilPadrao() {
  limparErrosDoEditor();
  const entrada = entradaDoEditor();
  const salvar = $("perfil-salvar");
  salvar.disabled = true;
  try {
    const salvo = await invoke("gravar_perfil_padrao", { entrada });
    if (typeof padraoMudou === "function") padraoMudou(salvo);
    anunciar(FRASES_DOS_PERFIS.padrao.salvo);
    return true;
  } catch (falha) {
    if (falha?.Campos) {
      mostrarErrosDeCampo(falha.Campos.campos);
      return false;
    }
    console.warn("gravar_perfil_padrao:", falha);
    mostrarErroDoPerfil(fraseDaImagemDoPerfil(falha));
    return false;
  } finally {
    salvar.disabled = false;
  }
}

async function mandarOPerfil(entrada) {
  const revisao = revisaoParaGravar();
  const desfecho = esperarODesfechoDoPerfil(entrada, revisao);
  try {
    await invoke("gravar_perfil", { revisao, entrada });
  } catch (falha) {
    avisarQuemEsperaOPerfil({ fechou: true });
    throw falha;
  }
  return desfecho;
}

/** RECARREGAR, depois de `RevisaoVelha`: o perfil que vale agora, nos campos. */
function recarregarOEditorDoPerfil() {
  if (!editorDoPerfil.aberto || editorDoPerfil.modo !== "oficial") return;
  const atual = perfilOficial(perfisOficiais.me);
  editorDoPerfil.base = { ...textosDoPerfil(atual), revisao: atual?.revisao ?? 0 };
  preencherOEditor(editorDoPerfil.base);
  limparErrosDoEditor();
  atualizarEditorDoPerfil();
}

// ------------------------------------------------- o rodapé que pergunta

/** Troca o rodapé de SALVAR pelo «Descartar alterações?», ou volta. */
function mostrarRodapeDeSaida(perguntando) {
  $("perfil-fim-normal").hidden = perguntando;
  $("perfil-fim-descartar").hidden = !perguntando;
  if (perguntando) $("perfil-continuar").focus();
}

function perguntandoSeDescarta() {
  return !$("perfil-fim-descartar").hidden;
}

// --------------------------------------------------------------- a ligação

/** Os eventos do perfil: a lista andou, ou o servidor recusou um pedido. */
function perfisAoEvento(mensagem) {
  const evento = mensagem?.payload?.evento ?? mensagem?.payload;
  if (!evento || typeof evento !== "object") return;
  if (evento.PerfisChanged) {
    perfisOficiais.marca = null;
    return;
  }
  if (evento.PerfilRecusado) avisarQuemEsperaOPerfil(evento);
  // O servidor não aceitou o nome: quem espera o `SetNickname` ouve por quê.
  const recusa = evento.NoticeRaised?.notice?.reason?.PedidoRecusado;
  if (recusa?.pedido === "SetNickname") {
    for (const recusar of [...esperandoOApelido]) recusar(recusa.causa ?? null);
  }
}
Promise.resolve(listen("seele://event", perfisAoEvento)).catch((falha) => console.warn("perfis: ouvir os eventos", falha));

(() => {
  // A entrada, o diretório e a folha (desktop e navegador largo).
  $("perfis-entrada")?.addEventListener("click", () => {
    if (diretorioDosPerfisAberto()) fecharDiretorioDosPerfis();
    else abrirDiretorioDosPerfis();
  });
  $("perfis-pendencia")?.addEventListener("click", () => abrirPerfil());
  $("perfis-diretorio-voltar")?.addEventListener("click", fecharDiretorioDosPerfis);
  $("perfis-diretorio-editar")?.addEventListener("click", () => abrirPerfil());
  $("perfil-folha-fechar")?.addEventListener("click", fecharFolhaDoPerfil);
  // Uma página de MOD que abre toma o centro: o diretório sai.
  const palco = $("palco-de-paginas");
  if (palco) {
    new MutationObserver(() => {
      if (!palco.hidden) fecharDiretorioDosPerfis();
    }).observe(palco, { attributes: true, attributeFilter: ["hidden"] });
  }
  // O Escape fecha a folha da pessoa quando nenhuma caixa está por cima.
  window.addEventListener("keydown", (evento) => {
    if (evento.key !== "Escape" || folhaDoPerfil.id === null) return;
    if (!$("perfil").hidden || document.querySelector(".camada-nomear:not([hidden]), [aria-modal='true']:not([hidden])")) return;
    evento.preventDefault();
    fecharFolhaDoPerfil();
  });
  // «Ver perfil» de uma linha de pessoa, por delegação: a linha é redesenhada a cada quadro.
  document.addEventListener("click", (evento) => {
    const botao = evento.target.closest?.("button[data-ver-perfil]");
    if (!botao) return;
    evento.preventDefault();
    abrirPerfilDaPessoa(botao.dataset.verPerfil);
  });

  // O editor: os campos redesenham a prévia; as amostras e as pílulas escolhem.
  const amostras = $("perfil-amostras");
  for (const cor of AMOSTRAS_DE_COR) {
    const amostra = elemento("button", "perfil-amostra");
    amostra.type = "button";
    amostra.dataset.cor = cor;
    amostra.setAttribute("role", "radio");
    amostra.setAttribute("aria-checked", "false");
    amostra.setAttribute("aria-label", cor);
    amostra.style.setProperty("--amostra", cor);
    amostra.addEventListener("click", () => {
      escolherACor(cor);
      atualizarEditorDoPerfil();
    });
    amostras.append(amostra);
  }
  const efeitos = $("perfil-efeitos");
  for (const efeito of EFEITOS_DO_PERFIL) {
    const pilula = elemento("button", "perfil-segmento", FRASES_DOS_PERFIS.efeitos[efeito]);
    pilula.type = "button";
    pilula.dataset.efeito = efeito;
    pilula.setAttribute("aria-pressed", "false");
    pilula.addEventListener("click", () => {
      escolherOEfeito(efeito);
      atualizarEditorDoPerfil();
    });
    efeitos.append(pilula);
  }
  for (const id of ["perfil-pronomes", "perfil-status", "perfil-bio", "perfil-apelido"]) {
    $(id).addEventListener("input", () => {
      if (id === "perfil-bio") contarABio();
      atualizarEditorDoPerfil();
    });
  }
  $("perfil-cor-hex").addEventListener("input", () => {
    escolherACor($("perfil-cor-hex").value.trim());
    atualizarEditorDoPerfil();
  });
  $("perfil-foto-escolher").addEventListener("click", () => escolherImagemDoPerfil("Foto"));
  $("perfil-capa-escolher").addEventListener("click", () => escolherImagemDoPerfil("Capa"));
  $("perfil-foto-remover").addEventListener("click", () => removerImagemDoPerfil("Foto"));
  $("perfil-capa-remover").addEventListener("click", () => removerImagemDoPerfil("Capa"));
  $("perfil-recarregar").addEventListener("click", recarregarOEditorDoPerfil);
})();
