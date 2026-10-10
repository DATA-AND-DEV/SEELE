// A ponte do navegador: o `window.__TAURI__` da casca quando não há Tauri.
//
// ADR 0057 (o celular entra pelo navegador) e D-web-3 (o web app também no
// PC, com a casca de desktop). A casca é a mesma `ui/` do app, e ela fala com
// o resto do produto por dois verbos, `invoke` e `listen`
// (`base.js:36-37`). No app, quem os atende é o Rust; aqui, é esta ponte.
//
// # O primeiro script, e síncrono
//
// Ela é o primeiro `<script>` do `<head>` por duas razões:
// - **a marca da casca** (`data-casca` na raiz) tem de existir antes da
//   primeira pintura, e a CSP do pacote não deixa script inline (§9.2 do spec
//   da F8, aprovado pela orquestradora em 09/10);
// - o `base.js` lê `window.__TAURI__` na primeira linha.
//
// # O lugar se decide aqui, e só aqui
//
// O Tauri sempre injeta o `__TAURI_INTERNALS__` (o `__TAURI__` depende do
// `withGlobalTauri`). **Um `__TAURI__` que já existe sem ele é o duble de uma
// bancada** (`bancada/*.cjs`, `tools/carga-da-casca.py`), escrito antes deste
// script: a ponte não o troca, e a marca fica com quem a pôs. Quem mais
// precisa saber onde está lê `PONTE_NO_LUGAR_DO_TAURI` (o `executorDoMod`, em
// `mods-runtime.js`). `tests/web.rs` segura que ninguém mais lê o Tauri para
// isso.
//
// # As listas
//
// Cada comando que a casca chama cai numa lista, e numa só
// (`tests/web.rs` confere contra a casca, e contra o `main.rs` o que mexe na
// hospedagem):
// - `COMANDOS_DA_PONTE`: atendidos nas duas cascas;
// - `SO_NA_CASCA_LARGA`: atendidos no PC, e recusados com a frase no celular
//   (`F4/ponte-no-pc.md`);
// - `RECUSAS_NOMEADAS`: recusados nas duas, com a frase;
// - `NEUTROS_DE_QUEM_NAO_HOSPEDA`: mexem na hospedagem, e respondem sempre o
//   que o desktop responde a quem não hospeda;
// - `ATENDIDOS_APESAR_DA_HOSPEDAGEM`: mexem na hospedagem e são atendidos (o
//   `disconnect`: sair da sessão é do navegador também).
//
// **E `AINDA_A_FAZER`**: os atendidos que ainda não têm corpo, cada um com a
// tarefa do plano que o traz (a sessão pelo navegador é a 2.6, que espera a
// medida 3 do dono e o Plano 3). Chamado, um deles volta com
// `AindaNaoNoNavegador`, e não com um silêncio. A lista encolhe a cada lote, e
// o pacote da página (3.6) só sai com ela vazia.
//
// # O que volta
//
// O mesmo formato do Tauri: a resposta, ou a rejeição com o valor do erro.
// As da ponte são três, nomeadas como os erros do Rust:
// - `{ RecusaDaPonte: { comando, frase } }`;
// - `{ AindaNaoNoNavegador: { comando, tarefa } }`;
// - `{ ComandoDesconhecido: { comando } }`, que o guarda impede de existir.
// Sem sessão, o que pede uma volta com `"NotConnected"`, como no desktop.
//
// **Nada disso é silencioso.** A primeira vez que um comando volta assim, a
// ponte o diz no console, e `CONTAS_DA_PONTE` guarda quantas vezes
// (`CHAMADAS_DA_PONTE`, quantas vezes cada um foi chamado): é por elas que a
// bancada `ponte-sem-gesto.cjs` sabe o que a casca chama sozinha, e se ela
// bateu numa recusa.

/**
 * A ponte está no lugar do Tauri: nem o Tauri, nem um duble de bancada.
 * Decidido uma vez, na carga.
 */
const PONTE_NO_LUGAR_DO_TAURI = !("__TAURI_INTERNALS__" in globalThis) && !("__TAURI__" in globalThis);

// ------------------------------------------------------------------- as frases

const RECUSA_HOSPEDAR = "hospedar é do app do SEELE: o navegador não hospeda";
const RECUSA_PORTARIA = "a portaria mora no app de quem hospeda este servidor";
const RECUSA_REDE = "o navegador não abre porta nem aceita conexão de entrada";
const RECUSA_MODS = "MODs se instalam, se ligam e se aplicam pelo app do SEELE que hospeda o servidor";
const RECUSA_VERSOES = "as versões do servidor moram no app do SEELE: esta página fala só o Servidor 1.0";
const RECUSA_ATUALIZAR = "a versão é a desta página: não há o que instalar";
const RECUSA_SISTEMA = "a página não abre os ajustes do sistema";
const RECUSA_MODULO = "o navegador não baixa módulo de vídeo: quem codifica é ele";
const RECUSA_NATIVO = "no navegador os MODs rodam no executorNoNavegador, e estes comandos são do executor nativo do desktop";

/** Recusados nas duas cascas: o comando e a frase. */
const RECUSAS_NOMEADAS = new Map([
  ["abrir_a_porta_no_firewall", RECUSA_REDE],
  ["abrir_ajustes_de_acessibilidade", RECUSA_SISTEMA],
  ["abrir_ajustes_do_microfone", RECUSA_SISTEMA],
  ["abrir_versao", RECUSA_VERSOES],
  ["apagar_pacote_do_cache", RECUSA_MODS],
  ["apagar_servidor", RECUSA_HOSPEDAR],
  ["aplicar_conjunto_de_mods", RECUSA_MODS],
  ["baixar_modulo_de_video", RECUSA_MODULO],
  ["baixar_versao", RECUSA_VERSOES],
  ["caminho_entre_pares", RECUSA_REDE],
  ["catalogo_de_mods", RECUSA_MODS],
  ["consentir_no_caminho_entre_pares", RECUSA_REDE],
  ["criar_convite_do_server", RECUSA_PORTARIA],
  ["criar_servidor", RECUSA_HOSPEDAR],
  ["decidir_pedido", RECUSA_PORTARIA],
  ["definir_senha_do_server", RECUSA_PORTARIA],
  ["hospedar", RECUSA_HOSPEDAR],
  ["instalar_atualizacao", RECUSA_ATUALIZAR],
  ["instalar_mod", RECUSA_MODS],
  ["ligar_portaria", RECUSA_PORTARIA],
  ["linhas_para_hospedar", RECUSA_HOSPEDAR],
  ["mod_nativo_ativar", RECUSA_NATIVO],
  ["mod_nativo_colher", RECUSA_NATIVO],
  ["mod_nativo_encerrar", RECUSA_NATIVO],
  ["mod_nativo_entregar", RECUSA_NATIVO],
  ["mod_nativo_reservar", RECUSA_NATIVO],
  ["ocupacao_do_deposito", RECUSA_VERSOES],
  ["pacotes_no_cache", RECUSA_MODS],
  ["pedidos_da_portaria", RECUSA_PORTARIA],
  ["procurar_atualizacao", RECUSA_ATUALIZAR],
  ["remover_versao", RECUSA_VERSOES],
  ["revogar_admissao", RECUSA_PORTARIA],
  ["servidor_hospedado", RECUSA_HOSPEDAR],
  ["servidores_guardados", RECUSA_HOSPEDAR]
]);

const SO_NO_PC_MODERAR = "moderar e administrar o servidor é pelo PC: no navegador largo ou no app";
const SO_NO_PC_TECLA = "no celular, falar apertando é o SEGURAR PARA FALAR da faixa de voz";
const SO_NO_PC_CHEIA = "no celular a tela cheia é o aparelho deitado";
const SO_NO_PC_RUIDO = "o celular usa a supressão de ruído do navegador";
const SO_NO_PC_SAIDA = "o celular toca pela saída de som do sistema";
const SO_NO_PC_ARRASTAR = "no celular o arquivo vem do seletor, e não do arraste";
const SO_NO_PC_COMPARTILHAR = "compartilhar a tela é pelo PC";
const SO_NO_PC_TESTE = "no celular o microfone abre só na sala";

/**
 * Atendidos no PC (a casca larga, D-web-3) e recusados no celular, com a
 * frase do celular.
 */
const SO_NA_CASCA_LARGA = new Map([
  ["abrir_teste_de_microfone", SO_NO_PC_TESTE],
  ["ajustar_limites_da_tela", SO_NO_PC_COMPARTILHAR],
  ["ajustar_reducao_de_ruido", SO_NO_PC_RUIDO],
  ["apagar_linha", SO_NO_PC_MODERAR],
  ["apagar_voice_room", SO_NO_PC_MODERAR],
  ["aplicar_icone_do_server", SO_NO_PC_MODERAR],
  ["banir_pessoa", SO_NO_PC_MODERAR],
  ["compartilhar_tela", SO_NO_PC_COMPARTILHAR],
  ["criar_linha", SO_NO_PC_MODERAR],
  ["criar_voice_room", SO_NO_PC_MODERAR],
  ["descrever_arquivo", SO_NO_PC_ARRASTAR],
  ["escolher_icone_do_server", SO_NO_PC_MODERAR],
  ["escolher_saida", SO_NO_PC_SAIDA],
  ["escolher_tecla_de_falar", SO_NO_PC_TECLA],
  ["estado_do_teste_de_microfone", SO_NO_PC_TESTE],
  ["exclusao_do_som_da_captura", SO_NO_PC_COMPARTILHAR],
  ["expulsar_pessoa", SO_NO_PC_MODERAR],
  ["fontes_de_tela", SO_NO_PC_COMPARTILHAR],
  ["modulo_de_video_a_baixar", SO_NO_PC_COMPARTILHAR],
  ["monitorar_o_microfone", SO_NO_PC_TESTE],
  ["mover_pessoa", SO_NO_PC_MODERAR],
  ["parar_de_compartilhar", SO_NO_PC_COMPARTILHAR],
  ["pedir_permissao_de_tela", SO_NO_PC_COMPARTILHAR],
  ["permissao_de_tela", SO_NO_PC_COMPARTILHAR],
  ["peso_da_linha", SO_NO_PC_MODERAR],
  ["preparar_icone_do_server", SO_NO_PC_MODERAR],
  ["renomear_linha", SO_NO_PC_MODERAR],
  ["renomear_server", SO_NO_PC_MODERAR],
  ["renomear_voice_room", SO_NO_PC_MODERAR],
  ["saida_escolhida", SO_NO_PC_SAIDA],
  ["saidas", SO_NO_PC_SAIDA],
  ["tela_cheia", SO_NO_PC_CHEIA],
  ["tirar_icone_do_server", SO_NO_PC_MODERAR]
]);

/** Atendidos nas duas cascas. */
const COMANDOS_DA_PONTE = new Set([
  "abertura", "abrir_no_navegador", "aceitar_mods", "aceite_de_mods",
  "aceites_de_mods", "ajustar_sensibilidade_da_voz", "ajustar_volume_da_tela",
  "analisar_convite", "apelido_local", "apis_de_mod_aceitas", "assistir",
  "atualizacao_que_nao_pegou", "busca_andar", "busca_limpar", "buscar",
  "calar_a_tela", "codigo_do_mod", "conferir_esta_versao", "conhecidos",
  "connect", "consentir_previa", "controles_da_voz", "descartar_mensagem",
  "desmontar_a_sessao", "destino_do_anexo", "dispensar_aviso",
  "dominios_de_gif", "enter_voice_room", "enviar_anexo", "enviar_imagem_mod",
  "escolher_apelido", "escolher_apelido_local", "escolher_arquivo",
  "escolher_microfone", "escolher_minha_imagem", "escolher_para_o_mod",
  "esquecer", "esquecer_aceite_de_mods", "esquecer_previa",
  "estado_da_primeira_abertura", "estado_da_sessao",
  "fechar_teste_de_microfone", "guardar_colado",
  "ha_ajustes_de_acessibilidade", "icone_do_server", "imagem_da_pessoa",
  "instalar_mod_do_catalogo", "leave_voice_room",
  "lembrar_aparencia_do_servidor", "ler_imagem_mod", "limite_da_mensagem",
  "messages", "meu_retrato", "microfone_escolhido", "microfones",
  "midia_do_mod", "midia_em_bytes", "mod_request", "modo_de_voz_escolhido",
  "mods_instalados", "open_channel", "pasta_de_downloads",
  "pedaco_do_escolhido", "pedir_quadro_chave", "permissao_de_microfone",
  "plataforma", "prever_anexo", "previa_de_link", "previas_consentidas",
  "reenviar_mensagem", "registrar_da_janela", "regras_de_previa",
  "regras_do_icone_do_server", "remover_mensagem", "salvar_anexo",
  "send_message", "set_muted", "set_talking", "set_total_isolation",
  "set_voice_mode", "set_volume", "snapshot", "soltar_escolhido",
  "som_da_tela", "som_do_mod", "tecla_de_falar", "tirar_minha_imagem",
  "versoes_instaladas", "volumes_locais"
]);

/**
 * **Mexem na hospedagem local, e não são recusados**: respondem sempre o que
 * o desktop responde a quem não hospeda. A casca os chama sem gesto, e uma
 * recusa ali seria um aviso a cada poucos segundos (`F4/ponte-no-pc.md`).
 * `bancada/ponte-web.cjs` confere cada resposta.
 */
const NEUTROS_DE_QUEM_NAO_HOSPEDA = new Map([
  [
    "estado_da_porta",
    {
      resposta: {
        hospedando: false,
        aberto: false,
        tem_senha: false,
        aceita_convites: false,
        portaria_ligada: false,
        pendentes: 0,
        alcance: "SoRedeLocal",
      },
      motivo: "chamado sem gesto na sessão; é o estado do desktop sem hospedagem (main.rs)",
    },
  ],
  [
    "estou_hospedando",
    { resposta: false, motivo: "a casca pergunta antes de mostrar o que é de quem hospeda" },
  ],
  [
    "conjunto_exigido_agora",
    { resposta: "", motivo: "o desktop responde assim a quem não hospeda (main.rs)" },
  ],
]);

/**
 * **Mexem na hospedagem local, e são atendidos mesmo assim.** No desktop, o
 * `disconnect` também derruba a hospedagem; no navegador não há hospedagem,
 * e sair da sessão é dele também. Como neutro, sair passaria a não fazer
 * nada.
 */
const ATENDIDOS_APESAR_DA_HOSPEDAGEM = new Map([
  ["disconnect", "sair da sessão é do navegador também; não há hospedagem a derrubar"],
]);

const TAREFA_26 = "2.6 (a sessão pelo navegador)";
const TAREFA_33 = "3.3 (a voz)";
const TAREFA_34 = "3.4 (a identidade e a entrada)";
const TAREFA_39 = "3.9 (assistir à tela)";
const TAREFA_310 = "3.10 (compartilhar a tela pelo PC)";
const TAREFA_D = "3.1, lote D (o microfone fora da sala, a saída de som e a tela cheia)";
const TAREFA_MODS = "2.6 e 3.4 (os MODs da sessão)";

/** Atendidos que ainda não têm corpo, com a tarefa do plano que os traz. */
const AINDA_A_FAZER = new Map([
  ["abrir_teste_de_microfone", TAREFA_D],
  ["aceitar_mods", TAREFA_34],
  ["aceite_de_mods", TAREFA_34],
  ["aceites_de_mods", TAREFA_34],
  ["ajustar_limites_da_tela", TAREFA_310],
  ["ajustar_reducao_de_ruido", TAREFA_33],
  ["ajustar_sensibilidade_da_voz", TAREFA_33],
  ["ajustar_volume_da_tela", TAREFA_39],
  ["analisar_convite", TAREFA_34],
  ["apagar_linha", TAREFA_26],
  ["apagar_voice_room", TAREFA_26],
  ["aplicar_icone_do_server", TAREFA_26],
  ["assistir", TAREFA_39],
  ["banir_pessoa", TAREFA_26],
  ["busca_andar", TAREFA_26],
  ["busca_limpar", TAREFA_26],
  ["buscar", TAREFA_26],
  ["calar_a_tela", TAREFA_39],
  ["codigo_do_mod", TAREFA_MODS],
  ["compartilhar_tela", TAREFA_310],
  ["conhecidos", TAREFA_34],
  ["connect", TAREFA_34],
  ["criar_linha", TAREFA_26],
  ["criar_voice_room", TAREFA_26],
  ["descartar_mensagem", TAREFA_26],
  ["destino_do_anexo", TAREFA_26],
  ["dispensar_aviso", TAREFA_26],
  ["enter_voice_room", TAREFA_33],
  ["enviar_anexo", TAREFA_26],
  ["enviar_imagem_mod", TAREFA_MODS],
  ["escolher_apelido", TAREFA_26],
  ["escolher_icone_do_server", TAREFA_26],
  ["escolher_microfone", TAREFA_33],
  ["escolher_minha_imagem", TAREFA_26],
  ["escolher_para_o_mod", TAREFA_MODS],
  ["escolher_saida", TAREFA_D],
  ["escolher_tecla_de_falar", TAREFA_33],
  ["esquecer", TAREFA_34],
  ["esquecer_aceite_de_mods", TAREFA_34],
  ["estado_da_sessao", TAREFA_MODS],
  ["estado_do_teste_de_microfone", TAREFA_D],
  ["exclusao_do_som_da_captura", TAREFA_310],
  ["expulsar_pessoa", TAREFA_26],
  ["fontes_de_tela", TAREFA_310],
  ["icone_do_server", TAREFA_26],
  ["imagem_da_pessoa", TAREFA_26],
  ["instalar_mod_do_catalogo", TAREFA_MODS],
  ["leave_voice_room", TAREFA_33],
  ["lembrar_aparencia_do_servidor", TAREFA_26],
  ["ler_imagem_mod", TAREFA_MODS],
  ["messages", TAREFA_26],
  ["microfone_escolhido", TAREFA_33],
  ["microfones", TAREFA_33],
  ["midia_do_mod", TAREFA_MODS],
  ["midia_em_bytes", TAREFA_MODS],
  ["mod_request", TAREFA_26],
  ["modo_de_voz_escolhido", TAREFA_33],
  ["mods_instalados", TAREFA_MODS],
  ["monitorar_o_microfone", TAREFA_D],
  ["mover_pessoa", TAREFA_26],
  ["open_channel", TAREFA_26],
  ["parar_de_compartilhar", TAREFA_310],
  ["pedaco_do_escolhido", TAREFA_MODS],
  ["pedir_permissao_de_tela", TAREFA_310],
  ["pedir_quadro_chave", TAREFA_39],
  ["permissao_de_microfone", TAREFA_33],
  ["permissao_de_tela", TAREFA_310],
  ["peso_da_linha", TAREFA_26],
  ["preparar_icone_do_server", TAREFA_26],
  ["prever_anexo", TAREFA_26],
  ["previa_de_link", TAREFA_26],
  ["reenviar_mensagem", TAREFA_26],
  ["remover_mensagem", TAREFA_26],
  ["renomear_linha", TAREFA_26],
  ["renomear_server", TAREFA_26],
  ["renomear_voice_room", TAREFA_26],
  ["saida_escolhida", TAREFA_D],
  ["saidas", TAREFA_D],
  ["salvar_anexo", TAREFA_26],
  ["send_message", TAREFA_26],
  ["set_muted", TAREFA_33],
  ["set_talking", TAREFA_33],
  ["set_total_isolation", TAREFA_33],
  ["set_voice_mode", TAREFA_33],
  ["set_volume", TAREFA_33],
  ["soltar_escolhido", TAREFA_MODS],
  ["som_da_tela", TAREFA_39],
  ["som_do_mod", TAREFA_MODS],
  ["tela_cheia", TAREFA_D],
  ["tirar_icone_do_server", TAREFA_26],
  ["tirar_minha_imagem", TAREFA_26]
]);

/**
 * O que o desktop responde sem perguntar a ninguém. **JSON estrito**:
 * `tests/web.rs` o lê e confere cada valor contra o Rust (`main.rs`,
 * `seele_ffi`). O `controles_da_voz` é o do desktop sem sessão e sem ajuste
 * guardado; o ajuste guardado na origem é da 3.3.
 */
const CONSTANTES_DO_DESKTOP = Object.freeze({
  "limite_da_mensagem": 4096,
  "regras_de_previa": { "limit": 4194304, "types": ["image/png", "image/jpeg", "image/gif", "image/webp"] },
  "regras_do_icone_do_server": { "limite_bytes": 8192, "lado": 256 },
  "apis_de_mod_aceitas": [5, 4, 3],
  "dominios_de_gif": ["tenor.com", "giphy.com", "imgur.com"],
  "controles_da_voz": {
    "supressao": 1,
    "abertura_dbfs": -60,
    "abertura_escolhida": false,
    "abertura_minima_dbfs": -72,
    "abertura_maxima_dbfs": -24
  }
});

// ------------------------------------------------------------------- a casca

/**
 * A casca desta carga: `celular` ou `larga`, **lida uma vez** (§9.2 do spec da
 * F8). Primeiro a escolha guardada no aparelho, se houver e se o
 * armazenamento deixar; senão, o critério: toque, ou menos de 900 px.
 */
function cascaDaCarga() {
  try {
    const escolhida = localStorage.getItem("seele.casca");
    if (escolhida === "celular" || escolhida === "larga") return escolhida;
  } catch {
    // Sem armazenamento (navegação privada, ou bloqueado): vale o critério.
  }
  const toque = window.matchMedia("(pointer: coarse)").matches;
  return toque || innerWidth < 900 ? "celular" : "larga";
}

/** A casca desta carga, ou `null` quando a ponte não está no lugar. */
const CASCA_DA_PONTE = PONTE_NO_LUGAR_DO_TAURI ? cascaDaCarga() : null;

// ------------------------------------------------------------------- a sessão

/**
 * A sessão desta página. **Nenhuma, até a 2.6**: o que pede uma sessão volta
 * com `"NotConnected"`, como no desktop sem conexão. É também a costura por
 * onde a bancada põe uma sessão de mentira.
 */
const sessaoDaPonte = { atual: null };

/** O que o desktop faz sem conexão: `session.connection()?`. */
function comASessao(fazer) {
  const sessao = sessaoDaPonte.atual;
  if (!sessao) return Promise.reject("NotConnected");
  return fazer(sessao);
}

/** Sai da sessão desta página, se houver uma, e solta os arquivos guardados. */
async function sairDaSessaoDaPonte() {
  const sessao = sessaoDaPonte.atual;
  sessaoDaPonte.atual = null;
  ALCAS_DA_PONTE.clear();
  await sessao?.encerrar?.();
  return null;
}

// ------------------------------------------------------------ o arquivo por alça

/**
 * As regras do desktop para um arquivo, **JSON estrito**: `tests/web.rs` as
 * confere contra o `main.rs`. O tipo de um arquivo sai da extensão, pela
 * tabela do `tipo_alegado`, e não do `File.type`; uma imagem colada ganha a
 * extensão e o teto do `guardar_colado`.
 */
const REGRAS_DO_ARQUIVO = Object.freeze({
  "tipos_por_extensao": {
    "png": "image/png",
    "jpg": "image/jpeg",
    "jpeg": "image/jpeg",
    "gif": "image/gif",
    "webp": "image/webp",
    "pdf": "application/pdf",
    "txt": "text/plain",
    "md": "text/plain",
    "opus": "audio/opus",
    "mp3": "audio/mpeg",
    "wav": "audio/wav",
    "mp4": "video/mp4"
  },
  "tipo_desconhecido": "application/octet-stream",
  "extensao_da_colagem": {
    "image/png": "png",
    "image/jpeg": "jpg",
    "image/gif": "gif",
    "image/webp": "webp"
  },
  "maior_colagem_bytes": 26214400
});

/**
 * **No navegador não há caminho de disco** (`F4/ponte-no-pc.md`, «O arquivo
 * sem caminho»). A forma das chamadas não muda: o «caminho» é uma alça opaca
 * (`web-arquivo:N`), e o `File` ou o `Blob` mora aqui, sob ela. Uma alça que
 * não está aqui (desconhecida, ou já solta) é recusada como o desktop recusa
 * um caminho que não existe.
 */
const ALCAS_DA_PONTE = new Map();

/**
 * O teto do mapa. A casca guarda um anexo pendente por vez; o resto é o que
 * ela trocou ou desistiu de mandar, e ela não avisa quando desiste
 * (`tirarAnexo` não chama a ponte). As mais velhas saem primeiro.
 */
const TETO_DE_ALCAS = 16;

/**
 * Um `Blob` colado está **todo na memória**, e um `File` escolhido não. Das
 * coladas, ficam só as mais novas.
 */
const TETO_DE_COLADOS = 2;

let ultimaAlcaDaPonte = 0;

/** Guarda um arquivo sob uma alça nova, e solta o que passou do teto. */
function guardarSobAlca(arquivo, nome, colado = false) {
  ultimaAlcaDaPonte += 1;
  const alca = `web-arquivo:${ultimaAlcaDaPonte}`;
  ALCAS_DA_PONTE.set(alca, { arquivo, nome, colado });
  const colados = [...ALCAS_DA_PONTE].filter(([, guardado]) => guardado.colado);
  for (const [velha] of colados.slice(0, -TETO_DE_COLADOS)) ALCAS_DA_PONTE.delete(velha);
  for (const velha of [...ALCAS_DA_PONTE.keys()].slice(0, -TETO_DE_ALCAS)) ALCAS_DA_PONTE.delete(velha);
  return alca;
}

/** O tipo que o nome alega, pela extensão: o `tipo_alegado` do desktop. */
function tipoAlegado(nome) {
  const ponto = nome.lastIndexOf(".");
  const extensao = ponto === -1 ? null : minusculasAscii(nome.slice(ponto + 1));
  return extensao !== null && Object.hasOwn(REGRAS_DO_ARQUIVO.tipos_por_extensao, extensao)
    ? REGRAS_DO_ARQUIVO.tipos_por_extensao[extensao]
    : REGRAS_DO_ARQUIVO.tipo_desconhecido;
}

/** O `ArquivoEscolhido` do desktop, com a alça no lugar do caminho. */
function descreverAlca(alca) {
  const guardado = ALCAS_DA_PONTE.get(alca);
  if (!guardado) return null;
  return { caminho: alca, nome: guardado.nome, tamanho: guardado.arquivo.size, tipo: tipoAlegado(guardado.nome) };
}

/**
 * **O arraste, só no PC.** A ponte ouve o `drop` do documento, guarda cada
 * arquivo sob uma alça e emite o mesmo `tauri://drag-drop` que a casca já ouve
 * (`tela-sessao.js`). Só quando o que se arrasta são arquivos: arrastar texto
 * para um campo continua sendo do navegador. E não mais que o teto, para a
 * alça do primeiro, que é a que a casca usa, não sair no meio.
 */
function ouvirOArraste() {
  const temArquivos = (evento) => Array.from(evento.dataTransfer?.types ?? []).includes("Files");
  document.addEventListener("dragover", (evento) => {
    if (temArquivos(evento)) evento.preventDefault();
  });
  document.addEventListener("drop", (evento) => {
    if (!temArquivos(evento)) return;
    evento.preventDefault();
    const arquivos = Array.from(evento.dataTransfer.files ?? []).slice(0, TETO_DE_ALCAS);
    if (arquivos.length === 0) return;
    emitirNaPonte("tauri://drag-drop", {
      paths: arquivos.map((arquivo) => guardarSobAlca(arquivo, arquivo.name)),
      position: { x: evento.clientX ?? 0, y: evento.clientY ?? 0 },
    });
  });
}

/** O que a origem guarda, ou `null` quando ela não guarda ou não deixa ler. */
function guardadoNaOrigem(chave) {
  try {
    return localStorage.getItem(chave);
  } catch {
    return null;
  }
}

/**
 * Grava na origem (`null` apaga), e diz se conseguiu. Sem armazenamento
 * (navegação privada, cota), a escolha não fica: quem chama diz isso.
 */
function gravarNaOrigem(chave, valor) {
  try {
    if (valor === null) localStorage.removeItem(chave);
    else localStorage.setItem(chave, valor);
    return true;
  } catch {
    return false;
  }
}

/** As chaves da origem: as preferências deste aparelho. */
const CHAVES_DA_ORIGEM = Object.freeze({
  apelido: "seele.apelido",
  retrato: "seele.retrato",
  previas: "seele.previas",
});

/**
 * Sem tab nem quebra de linha: o `sanitise` das preferências do desktop
 * (`seele-core/src/preferences.rs`). Lá é o formato do arquivo; aqui, que um
 * apelido ou um domínio guardado não ganhe uma quebra que o de lá não teria.
 */
const semQuebras = (texto) => texto.replace(/[\t\n\r]/g, "");

/** Minúsculas só no ASCII, como o `to_ascii_lowercase` do Rust. */
const minusculasAscii = (texto) => texto.replace(/[A-Z]/g, (letra) => letra.toLowerCase());

/** Os domínios com a prévia liberada neste aparelho (ADR 0053). */
function previasConsentidas() {
  try {
    const lista = JSON.parse(guardadoNaOrigem(CHAVES_DA_ORIGEM.previas) ?? "[]");
    return Array.isArray(lista) ? lista.filter((dominio) => typeof dominio === "string") : [];
  } catch {
    return [];
  }
}

/** O `previa_consentida` do desktop: o domínio, ou um subdomínio dele. */
const previaConsentida = (lista, alvo) =>
  lista.some((liberado) => alvo === liberado || alvo.endsWith(`.${liberado}`));

/**
 * O endereço que pode sair para uma aba nova, ou a recusa: o
 * `endereco_que_pode_sair` do desktop (`main.rs`). Só `http` e `https`: o resto
 * (`javascript:`, `data:`, `file:` e os esquemas que abrem programas) é alguém
 * de fora escolhendo o que roda aqui.
 */
function enderecoQuePodeSair(bruto) {
  const url = bruto.trim();
  if (new TextEncoder().encode(url).length > 2048) return { recusa: "endereço longo demais" };
  const minusculo = minusculasAscii(url);
  if (!minusculo.startsWith("http://") && !minusculo.startsWith("https://")) {
    return {
      recusa:
        "só abro endereços `http` e `https`: qualquer outro esquema é um programa que alguém de fora escolheu iniciar nesta máquina",
    };
  }
  if (/[\s\p{Cc}]/u.test(url)) return { recusa: "endereço com espaço ou caractere de controle" };
  if (url.length <= "https://".length) return { recusa: "endereço vazio" };
  return { alvo: url };
}

// ------------------------------------------------------------------- os corpos

/**
 * O corpo de cada atendido que já tem um. `bancada/ponte-web.cjs` confere que
 * todo atendido tem corpo aqui ou está em `AINDA_A_FAZER`, e nunca os dois.
 */
const ATENDIMENTOS_DA_PONTE = Object.freeze({
  // **Nunca `entrar`.** Se viesse, `cumprirAAbertura` (`tela-boot.js`)
  // conectaria sem toque, contra o ADR 0057: o link do navegador entra pela
  // tela ENTRAR EM (3.4). E nunca `hospedar`.
  abertura: () => ({ hospedar: false, nome_publico: null, entrar: null, pelo_lancador: false }),
  plataforma: () => "web",
  // Quem recusa uma versão revogada é a origem, que não serve a página.
  conferir_esta_versao: () => null,
  atualizacao_que_nao_pegou: () => null,
  ha_ajustes_de_acessibilidade: () => false,
  // Salvar um anexo é o download do navegador.
  pasta_de_downloads: () => "",
  versoes_instaladas: () => [],
  estado_da_primeira_abertura: () => null,
  // Sem tecla escolhida. A tecla de falar do PC chega com a voz (3.3).
  tecla_de_falar: () => null,
  limite_da_mensagem: () => structuredClone(CONSTANTES_DO_DESKTOP.limite_da_mensagem),
  regras_de_previa: () => structuredClone(CONSTANTES_DO_DESKTOP.regras_de_previa),
  regras_do_icone_do_server: () => structuredClone(CONSTANTES_DO_DESKTOP.regras_do_icone_do_server),
  apis_de_mod_aceitas: () => structuredClone(CONSTANTES_DO_DESKTOP.apis_de_mod_aceitas),
  dominios_de_gif: () => structuredClone(CONSTANTES_DO_DESKTOP.dominios_de_gif),
  controles_da_voz: () => structuredClone(CONSTANTES_DO_DESKTOP.controles_da_voz),
  // Sem sessão, o mapa vazio, como o `unwrap_or_default` do desktop.
  volumes_locais: () => sessaoDaPonte.atual?.volumesLocais?.() ?? {},
  snapshot: () => comASessao((sessao) => sessao.snapshot()),
  disconnect: sairDaSessaoDaPonte,
  // O mesmo que o `disconnect`: não há hospedagem a manter.
  desmontar_a_sessao: sairDaSessaoDaPonte,
  // «Nenhum»: quem codifica é o navegador (só no PC).
  modulo_de_video_a_baixar: () => null,
  // Atendido sempre, aberto ou não: `pararOTesteDeMicrofone` o chama a cada
  // saída de ÁUDIO. Sem teste aberto, não há o que soltar; o teste é do lote D.
  fechar_teste_de_microfone: () => null,
  // **O registro da janela, no console.** O desktop o escreve no `seele.log`;
  // o canal de diagnóstico do navegador está em aberto (ADR 0057, «O que este
  // ADR não decide»), e até lá ele vai aonde o inspetor o lê.
  registrar_da_janela: ({ nivel, onde, oQue, modId } = {}) => {
    const metodo = ["debug", "info", "warn", "error"].includes(nivel) ? nivel : "info";
    const doMod = modId == null ? "" : ` MOD ${modId}:`;
    console[metodo](`[${onde}]${doMod} ${oQue}`);
    return null;
  },
  apelido_local: () => guardadoNaOrigem(CHAVES_DA_ORIGEM.apelido),
  meu_retrato: () => guardadoNaOrigem(CHAVES_DA_ORIGEM.retrato),
  // Aparado e sem quebras; vazio apaga (`Preferences::set_nickname`).
  escolher_apelido_local: ({ apelido } = {}) => {
    const limpo = semQuebras(String(apelido ?? "").trim());
    const valor = limpo.trim() === "" ? null : limpo;
    if (!gravarNaOrigem(CHAVES_DA_ORIGEM.apelido, valor)) return Promise.reject("NaoGravei");
    return null;
  },
  // **É o clique que consente**, para o domínio (ADR 0053). Uma palavra só, em
  // minúsculas; um subdomínio de um liberado já está liberado.
  consentir_previa: ({ dominio } = {}) => {
    const alvo = minusculasAscii(semQuebras(String(dominio ?? ""))).trim();
    if (alvo === "" || /\s/.test(alvo)) return null;
    const lista = previasConsentidas();
    if (previaConsentida(lista, alvo)) return null;
    if (!gravarNaOrigem(CHAVES_DA_ORIGEM.previas, JSON.stringify([...lista, alvo]))) {
      return Promise.reject("não consegui gravar a escolha");
    }
    return null;
  },
  esquecer_previa: ({ dominio } = {}) => {
    const alvo = minusculasAscii(String(dominio ?? ""));
    const lista = previasConsentidas();
    const restam = lista.filter((liberado) => liberado !== alvo);
    if (restam.length === lista.length) return null;
    if (!gravarNaOrigem(CHAVES_DA_ORIGEM.previas, JSON.stringify(restam))) {
      return Promise.reject("não consegui gravar a escolha");
    }
    return null;
  },
  previas_consentidas: () => previasConsentidas(),
  // **O seletor, dentro do gesto.** Nenhum `await` antes do `click()`: o
  // navegador só abre o seletor na ativação de quem clicou, e o executor da
  // promessa roda na hora. Desistir é o `cancel`, e devolve `null`, como no
  // desktop, sem deixar a promessa pendurada.
  escolher_arquivo: () =>
    new Promise((responder) => {
      const entrada = document.createElement("input");
      entrada.type = "file";
      entrada.hidden = true;
      let respondido = false;
      const fim = (valor) => {
        if (respondido) return;
        respondido = true;
        entrada.remove();
        responder(valor);
      };
      entrada.addEventListener("change", () => {
        const arquivo = entrada.files?.[0];
        fim(arquivo ? descreverAlca(guardarSobAlca(arquivo, arquivo.name)) : null);
      });
      entrada.addEventListener("cancel", () => fim(null));
      document.body.append(entrada);
      entrada.click();
    }),
  // As regras e as recusas do `guardar_colado` do desktop; o `Blob` fica na
  // memória, sob uma alça, e não num arquivo temporário.
  guardar_colado: ({ tipo, base64 } = {}) => {
    const texto = String(base64 ?? "");
    const teto = REGRAS_DO_ARQUIVO.maior_colagem_bytes;
    if (texto.length > Math.floor(teto / 3) * 4) return Promise.reject("GrandeDemais");
    let binario;
    try {
      binario = atob(texto);
    } catch {
      return Promise.reject("NaoEImagem");
    }
    if (binario.length > teto) return Promise.reject("GrandeDemais");
    if (binario.length === 0) return Promise.reject("NaoEImagem");
    const bytes = Uint8Array.from(binario, (letra) => letra.charCodeAt(0));
    const extensao = Object.hasOwn(REGRAS_DO_ARQUIVO.extensao_da_colagem, String(tipo))
      ? REGRAS_DO_ARQUIVO.extensao_da_colagem[String(tipo)]
      : "";
    const nome = extensao ? `colado-${Date.now()}.${extensao}` : `colado-${Date.now()}`;
    return descreverAlca(guardarSobAlca(new Blob([bytes]), nome, true));
  },
  // Uma alça que não está aqui é o caminho que não existe do desktop.
  descrever_arquivo: ({ caminho } = {}) => descreverAlca(String(caminho ?? "")) ?? Promise.reject("NotConnected"),
  // **Numa aba nova, que não leva a página junto.** Sem `await` antes do
  // `open`: o navegador só abre a aba dentro do gesto de quem clicou.
  abrir_no_navegador: ({ url } = {}) => {
    const { alvo, recusa } = enderecoQuePodeSair(String(url ?? ""));
    if (recusa) return Promise.reject(recusa);
    window.open(alvo, "_blank", "noopener,noreferrer");
    return null;
  },
});

// ------------------------------------------------------------------- os verbos

/** Quantas vezes a casca chamou cada comando. */
const CHAMADAS_DA_PONTE = new Map();

/** Quantas vezes cada comando voltou recusado, a fazer ou desconhecido. */
const CONTAS_DA_PONTE = new Map();

/** Conta, e diz no console da primeira vez. */
function anotarNaPonte(comando, como, valor) {
  const contas = CONTAS_DA_PONTE.get(comando) ?? { recusas: 0, aFazer: 0, desconhecido: 0 };
  contas[como] += 1;
  CONTAS_DA_PONTE.set(comando, contas);
  if (contas[como] === 1) console.warn(`ponte: \`${comando}\``, valor);
  return Promise.reject(valor);
}

/** O `invoke` do navegador. */
async function invocarNaPonte(comando, args) {
  CHAMADAS_DA_PONTE.set(comando, (CHAMADAS_DA_PONTE.get(comando) ?? 0) + 1);
  const neutro = NEUTROS_DE_QUEM_NAO_HOSPEDA.get(comando);
  if (neutro) return structuredClone(neutro.resposta);
  const frase =
    RECUSAS_NOMEADAS.get(comando) ??
    (CASCA_DA_PONTE === "celular" ? SO_NA_CASCA_LARGA.get(comando) : undefined);
  if (frase !== undefined) {
    return anotarNaPonte(comando, "recusas", { RecusaDaPonte: { comando, frase } });
  }
  const tarefa = AINDA_A_FAZER.get(comando);
  if (tarefa !== undefined) {
    return anotarNaPonte(comando, "aFazer", { AindaNaoNoNavegador: { comando, tarefa } });
  }
  if (!Object.hasOwn(ATENDIMENTOS_DA_PONTE, comando)) {
    return anotarNaPonte(comando, "desconhecido", { ComandoDesconhecido: { comando } });
  }
  return ATENDIMENTOS_DA_PONTE[comando](args ?? {});
}

/** Quem ouve cada evento. */
const OUVINTES_DA_PONTE = new Map();

/** O `listen` do navegador: devolve, como o do Tauri, a promessa do desligar. */
function ouvirNaPonte(evento, aoOuvir) {
  let ouvintes = OUVINTES_DA_PONTE.get(evento);
  if (!ouvintes) {
    ouvintes = new Set();
    OUVINTES_DA_PONTE.set(evento, ouvintes);
  }
  const ouvinte = (payload) => aoOuvir({ event: evento, id: 0, payload });
  ouvintes.add(ouvinte);
  return Promise.resolve(() => {
    ouvintes.delete(ouvinte);
  });
}

/** Emite um evento para quem o ouve, como o `emit` do Rust. */
function emitirNaPonte(evento, payload) {
  for (const ouvinte of [...(OUVINTES_DA_PONTE.get(evento) ?? [])]) ouvinte(payload);
}

if (PONTE_NO_LUGAR_DO_TAURI) {
  document.documentElement.dataset.casca = CASCA_DA_PONTE;
  if (CASCA_DA_PONTE === "larga") ouvirOArraste();
  window.__TAURI__ = Object.freeze({
    core: Object.freeze({ invoke: invocarNaPonte }),
    event: Object.freeze({ listen: ouvirNaPonte }),
  });
}
