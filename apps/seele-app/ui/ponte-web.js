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
// bancada** (`bancada/*.cjs`, `tools/carga-da-casca.py`,
// `tools/bancada-do-celular.py`), escrito antes deste script: a ponte não o
// troca, e a marca fica com quem a pôs.
//
// **Por quê:** o duble faz o papel do desktop. Se a ponte o trocasse, a
// bancada passaria a medir a ponte no lugar da casca do desktop que ela finge,
// e os roteiros de `tools/roteiros/` e o G3 da F8 (o desktop igual pixel a
// pixel) deixariam de provar o que dizem. E a marca que a
// `bancada-do-celular.py` escolhe seria trocada pelo critério da ponte. Achado
// ao escrever o lote A: essas ferramentas põem o duble antes do primeiro
// `<script src=`, que passou a ser este.
//
// Quem mais precisa saber onde está lê `PONTE_NO_LUGAR_DO_TAURI` (o
// `executorDoMod`, em `mods-runtime.js`: sob um duble, o executor é o nativo,
// como no desktop que ele finge). `tests/web.rs` segura que ninguém mais lê o
// Tauri para isso, e a bancada `ponte-web.cjs` prova a regra nos três lugares.
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
//   que o desktop responde a quem não hospeda.
//
// Havia uma sexta, `ATENDIDOS_APESAR_DA_HOSPEDAGEM`, só com o `disconnect`, que
// derrubava a hospedagem no desktop. Desde a tarefa 19 da F7 ele desmonta só a
// sessão, e é um atendido comum: a lista saiu inteira, com o ramo dela no guarda,
// porque uma lista vazia que o guarda ainda lê é onde um comando se esconde.
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
// A sala viva da F7 (`F4/ponte-no-pc.md`, «A sala viva da F7»).
const RECUSA_SALA = "a sala pessoal e as ligações moram no app do SEELE: o navegador não recebe ligação";
const RECUSA_AGENDA = "o código e a agenda moram no app do SEELE, nesta máquina";

/** Recusados nas duas cascas: o comando e a frase. */
const RECUSAS_NOMEADAS = new Map([
  ["abrir_a_porta_no_firewall", RECUSA_REDE],
  ["abrir_ajustes_de_acessibilidade", RECUSA_SISTEMA],
  ["abrir_ajustes_do_microfone", RECUSA_SISTEMA],
  ["abrir_versao", RECUSA_VERSOES],
  ["apagar_pacote_do_cache", RECUSA_MODS],
  ["apagar_servidor", RECUSA_HOSPEDAR],
  ["aplicar_conjunto_de_mods", RECUSA_MODS],
  ["atender", RECUSA_SALA],
  ["baixar_modulo_de_video", RECUSA_MODULO],
  ["baixar_versao", RECUSA_VERSOES],
  ["bloquear", RECUSA_SALA],
  ["caminho_entre_pares", RECUSA_REDE],
  ["catalogo_de_mods", RECUSA_MODS],
  ["consentir_no_caminho_entre_pares", RECUSA_REDE],
  ["contatos_da_agenda", RECUSA_AGENDA],
  ["criar_convite_do_server", RECUSA_PORTARIA],
  ["criar_servidor", RECUSA_HOSPEDAR],
  ["dar_nome_na_agenda", RECUSA_AGENDA],
  ["decidir_pedido", RECUSA_PORTARIA],
  ["definir_senha_do_server", RECUSA_PORTARIA],
  ["encerrar_a_ligacao", RECUSA_SALA],
  // Quem pinta o servidor hospedado (tarefa 17f.0 da F3): escolher é de quem
  // hospeda, e o navegador não hospeda.
  ["escolher_o_tema_do_servidor", RECUSA_MODS],
  ["hospedar", RECUSA_HOSPEDAR],
  ["instalar_atualizacao", RECUSA_ATUALIZAR],
  ["instalar_mod", RECUSA_MODS],
  ["ligar_portaria", RECUSA_PORTARIA],
  ["linhas_para_hospedar", RECUSA_HOSPEDAR],
  ["meu_codigo", RECUSA_AGENDA],
  ["mod_nativo_ativar", RECUSA_NATIVO],
  ["mod_nativo_colher", RECUSA_NATIVO],
  ["mod_nativo_encerrar", RECUSA_NATIVO],
  ["mod_nativo_entregar", RECUSA_NATIVO],
  ["mod_nativo_reservar", RECUSA_NATIVO],
  ["ocupacao_do_deposito", RECUSA_VERSOES],
  ["onde_estou", RECUSA_SALA],
  ["pacotes_no_cache", RECUSA_MODS],
  ["parar_de_hospedar", RECUSA_HOSPEDAR],
  ["pedidos_da_portaria", RECUSA_PORTARIA],
  ["procurar_atualizacao", RECUSA_ATUALIZAR],
  ["receber_ligacoes", RECUSA_SALA],
  ["recusar", RECUSA_SALA],
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
  "destino_do_anexo", "disconnect", "dispensar_aviso",
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
  "set_voice_mode", "set_volume", "snapshot", "soltar_a_entrada", "soltar_escolhido",
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
  [
    "estado_do_servidor",
    {
      resposta: { hospedando: false, mensagens_desconhecidas: 0, sessoes_com_desconhecidas: 0 },
      motivo: "a P12 da F1: a seção ESTE SERVIDOR pergunta a cada volta, e a janela que não hospeda esconde o número (main.rs)",
    },
  ],
  // A sala viva da F7: `null` e `[]` dizem «não há sala neste lugar», e nunca
  // `Desligada`, que diria que existe uma e que alguém a desligou. A casca
  // esconde a linha da sala com o `null` (`fraseDaSala`).
  [
    "estado_da_sala",
    { resposta: null, motivo: "o navegador não tem sala; chamado sem gesto na carga (camada-sala.js)" },
  ],
  [
    "toques_agora",
    { resposta: [], motivo: "o navegador não tem sala; chamado sem gesto na carga (camada-toque.js)" },
  ],
  // As tarefas 19 e 20 da F7: `null` diz «não há servidor no ar», e o bloco do
  // VOLTAR AO MEU SERVIDOR fica escondido.
  [
    "meu_servidor",
    { resposta: null, motivo: "o navegador não hospeda; chamado sem gesto quando a entrada aparece (camada-hospedagem.js)" },
  ],
]);

const TAREFA_26 = "2.6 (a sessão pelo navegador)";
const TAREFA_33 = "3.3 (a voz, e o teste do microfone fora da sala, que usa o mesmo portão)";
const TAREFA_34 = "3.4 (a identidade e a entrada)";
const TAREFA_39 = "3.9 (assistir à tela)";
const TAREFA_310 = "3.10 (compartilhar a tela pelo PC)";
const TAREFA_MODS = "2.6 e 3.4 (os MODs da sessão)";

/** Atendidos que ainda não têm corpo, com a tarefa do plano que os traz. */
const AINDA_A_FAZER = new Map([
  ["aceitar_mods", TAREFA_34],
  ["aceite_de_mods", TAREFA_34],
  ["aceites_de_mods", TAREFA_34],
  ["ajustar_limites_da_tela", TAREFA_310],
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
  ["escolher_minha_imagem", TAREFA_26],
  ["escolher_para_o_mod", TAREFA_MODS],
  ["esquecer", TAREFA_34],
  ["esquecer_aceite_de_mods", TAREFA_34],
  ["estado_da_sessao", TAREFA_MODS],
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
  ["midia_do_mod", TAREFA_MODS],
  ["midia_em_bytes", TAREFA_MODS],
  ["mod_request", TAREFA_26],
  ["mods_instalados", TAREFA_MODS],
  ["mover_pessoa", TAREFA_26],
  ["open_channel", TAREFA_26],
  ["parar_de_compartilhar", TAREFA_310],
  ["pedaco_do_escolhido", TAREFA_MODS],
  ["pedir_permissao_de_tela", TAREFA_310],
  ["pedir_quadro_chave", TAREFA_39],
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
  ["salvar_anexo", TAREFA_26],
  ["send_message", TAREFA_26],
  ["set_muted", TAREFA_33],
  ["set_talking", TAREFA_33],
  ["set_total_isolation", TAREFA_33],
  ["set_volume", TAREFA_33],
  ["soltar_a_entrada", TAREFA_34],
  ["soltar_escolhido", TAREFA_MODS],
  ["som_da_tela", TAREFA_39],
  ["som_do_mod", TAREFA_MODS],
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
  saida: "seele.saida",
  modo: "seele.modo-de-voz",
  microfone: "seele.microfone",
  sensibilidade: "seele.sensibilidade-da-voz",
  reducao: "seele.reducao-de-ruido",
  tecla: "seele.tecla-de-falar",
});

/**
 * A regra da voz que o desktop aplica sem sessão, **JSON estrito**:
 * `tests/web.rs` a confere contra o `seele_ffi`. Sem redução de ruído, a
 * abertura padrão do portão é esta, e não a de com supressão.
 */
const REGRAS_DA_VOZ = Object.freeze({
  "abertura_sem_supressao_dbfs": -42
});

/** Os modos de voz do desktop (`VoiceMode`). */
const MODOS_DE_VOZ = Object.freeze(["PushToTalk", "VoiceActivated", "Open"]);

/**
 * A voz da página (`voz-web.js`), nascida no primeiro uso: por ora, o teste do
 * microfone. Uma carga que falhou não fica guardada.
 */
const vozDaPonte = { atual: null, nascendo: null };

function aVozDaPonte() {
  if (!vozDaPonte.nascendo) {
    vozDaPonte.nascendo = carregarOWasmDaPagina().then(({ bytes, instancia }) => {
      vozDaPonte.atual = criarVozDoNavegador({ wasm: instancia.exports, bytesDoWasm: bytes });
      return vozDaPonte.atual;
    });
    vozDaPonte.nascendo.catch(() => {
      vozDaPonte.nascendo = null;
    });
  }
  return vozDaPonte.nascendo;
}

/**
 * Por que o microfone não abriu, numa frase. O desktop devolve a do aparelho;
 * aqui quem recusa é o navegador, e cada recusa dele tem um conserto
 * diferente.
 */
function fraseDoMicrofoneQueNaoAbriu(erro) {
  const nome = erro && erro.name;
  if (nome === "NotAllowedError") {
    return "O navegador não deu o microfone a esta página. Permita o microfone nos ajustes do site e tente de novo.";
  }
  if (nome === "NotFoundError") return "Esta máquina não mostrou nenhum microfone ao navegador.";
  if (nome === "NotReadableError") {
    return "O microfone não abriu: outro programa pode estar com ele, ou o sistema não o entregou ao navegador.";
  }
  return `O microfone não abriu (${nome ?? "erro"}: ${erro && erro.message ? erro.message : erro}).`;
}

/**
 * A recusa do microfone que o `getUserMedia` deu nesta página
 * (`NotAllowedError`). Só na memória: ela vale enquanto a página vive, que é o
 * tempo em que o navegador a mantém sem perguntar de novo.
 */
const recusaDoMicrofone = { nestaPagina: false };

/** O teste do microfone com o que está guardado nesta origem. */
async function abrirOTesteNaPonte() {
  let voz;
  try {
    voz = await aVozDaPonte();
  } catch (erro) {
    return Promise.reject(`A voz desta página não carregou (${erro && erro.message ? erro.message : erro}).`);
  }
  const supressao = milesimosGuardados(CHAVES_DA_ORIGEM.reducao) ?? CONSTANTES_DO_DESKTOP.controles_da_voz.supressao;
  const escolhida = milesimosGuardados(CHAVES_DA_ORIGEM.sensibilidade);
  try {
    await voz.abrirTeste({
      microfone: guardadoNaOrigem(CHAVES_DA_ORIGEM.microfone),
      saida: guardadoNaOrigem(CHAVES_DA_ORIGEM.saida),
      supressao: supressao > 0,
      abertura: escolhida === null ? 0 : Math.round(escolhida * 1000),
    });
  } catch (erro) {
    if (erro && erro.name === "NotAllowedError") recusaDoMicrofone.nestaPagina = true;
    return Promise.reject(fraseDoMicrofoneQueNaoAbriu(erro));
  }
  recusaDoMicrofone.nestaPagina = false;
  return null;
}

/** Um número guardado em milésimos, como as preferências do desktop o guardam. */
function milesimosGuardados(chave) {
  const texto = guardadoNaOrigem(chave);
  const milesimos = texto === null ? NaN : Number(texto);
  return Number.isInteger(milesimos) ? milesimos / 1000 : null;
}

/**
 * Os controles da voz sem sessão, pela regra do `controles_da_voz` do desktop:
 * a força guardada (ou a padrão), a abertura escolhida (ou a padrão daquela
 * força) e os extremos do portão.
 */
function controlesDaVozGuardados() {
  const padrao = CONSTANTES_DO_DESKTOP.controles_da_voz;
  const supressao = milesimosGuardados(CHAVES_DA_ORIGEM.reducao) ?? padrao.supressao;
  const escolhida = milesimosGuardados(CHAVES_DA_ORIGEM.sensibilidade);
  return {
    supressao,
    abertura_dbfs:
      escolhida ?? (supressao > 0 ? padrao.abertura_dbfs : REGRAS_DA_VOZ.abertura_sem_supressao_dbfs),
    abertura_escolhida: escolhida !== null,
    abertura_minima_dbfs: padrao.abertura_minima_dbfs,
    abertura_maxima_dbfs: padrao.abertura_maxima_dbfs,
  };
}

/**
 * As saídas de som do PC, na forma do `PlaybackDevice` do desktop. Sem os
 * pseudo-aparelhos do navegador (`default` e `communications` repetem um
 * aparelho de verdade; o padrão é o do mesmo grupo do `default`) e sem os ids
 * vazios que ele dá antes de o microfone ser permitido. **Onde não há
 * `setSinkId`** não há como tocar noutra saída, e a lista é vazia: a casca diz
 * que esta máquina não listou nada, e o som sai pela do sistema.
 */
async function saidasDeSom() {
  if (!("setSinkId" in window.HTMLMediaElement.prototype)) return [];
  return aparelhosDoTipo("audiooutput", "SAÍDA");
}

/**
 * Os aparelhos de um tipo, como o desktop os lista (`CaptureDevice`,
 * `PlaybackDevice`): sem os pseudo-aparelhos, sem os ids vazios, e com o
 * padrão pelo grupo do `default`. Um aparelho sem nome (o navegador só os
 * nomeia depois da permissão) leva o número dele.
 */
async function aparelhosDoTipo(tipo, semNome) {
  const aparelhos = (await navigator.mediaDevices.enumerateDevices()).filter((aparelho) => aparelho.kind === tipo);
  const grupoDoPadrao = aparelhos.find((aparelho) => aparelho.deviceId === "default")?.groupId;
  return aparelhos
    .filter((aparelho) => aparelho.deviceId !== "" && !["default", "communications"].includes(aparelho.deviceId))
    .map((aparelho, ordem) => ({
      id: aparelho.deviceId,
      name: aparelho.label || `${semNome} ${ordem + 1}`,
      default: grupoDoPadrao !== undefined && aparelho.groupId === grupoDoPadrao,
    }));
}

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
  // **Opaca**, como no desktop: um `KeyboardEvent.code`, que quem lê o teclado
  // (a casca) nomeia. No celular não há tecla: falar apertando é o SEGURAR
  // PARA FALAR da faixa de voz.
  tecla_de_falar: () => (CASCA_DA_PONTE === "larga" ? guardadoNaOrigem(CHAVES_DA_ORIGEM.tecla) : null),
  limite_da_mensagem: () => structuredClone(CONSTANTES_DO_DESKTOP.limite_da_mensagem),
  regras_de_previa: () => structuredClone(CONSTANTES_DO_DESKTOP.regras_de_previa),
  regras_do_icone_do_server: () => structuredClone(CONSTANTES_DO_DESKTOP.regras_do_icone_do_server),
  apis_de_mod_aceitas: () => structuredClone(CONSTANTES_DO_DESKTOP.apis_de_mod_aceitas),
  dominios_de_gif: () => structuredClone(CONSTANTES_DO_DESKTOP.dominios_de_gif),
  controles_da_voz: controlesDaVozGuardados,
  // Sem sessão, o mapa vazio, como o `unwrap_or_default` do desktop.
  volumes_locais: () => sessaoDaPonte.atual?.volumesLocais?.() ?? {},
  snapshot: () => comASessao((sessao) => sessao.snapshot()),
  disconnect: sairDaSessaoDaPonte,
  // «Nenhum»: quem codifica é o navegador (só no PC).
  modulo_de_video_a_baixar: () => null,
  // O teste do microfone do PC (3.3, decidido em 09/10): o `getUserMedia`, com
  // o mesmo portão da voz. No celular os três primeiros são recusados
  // (`SO_NA_CASCA_LARGA`). O fechar é atendido sempre, aberto ou não:
  // `pararOTesteDeMicrofone` o chama a cada saída de ÁUDIO, e fora da sala ele
  // solta o microfone de verdade (`F4/ponte-no-pc.md`).
  abrir_teste_de_microfone: abrirOTesteNaPonte,
  estado_do_teste_de_microfone: () => vozDaPonte.atual?.estadoDoTeste() ?? null,
  monitorar_o_microfone: ({ ligado } = {}) => {
    vozDaPonte.atual?.monitorar(ligado);
    return null;
  },
  fechar_teste_de_microfone: () => (vozDaPonte.atual ? vozDaPonte.atual.fecharTeste().then(() => null) : null),
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
  saidas: saidasDeSom,
  microfones: () => aparelhosDoTipo("audioinput", "MICROFONE"),
  microfone_escolhido: () => guardadoNaOrigem(CHAVES_DA_ORIGEM.microfone),
  // Grava; numa sala, a voz (3.3, lote C) reabre a captura com ele.
  escolher_microfone: ({ dispositivo } = {}) =>
    gravarNaOrigem(CHAVES_DA_ORIGEM.microfone, dispositivo == null ? null : String(dispositivo))
      ? null
      : Promise.reject("NaoGravei"),
  // Sem escolha, o VOZ, que é o que funciona com a tela travada (matriz da F8).
  modo_de_voz_escolhido: () => {
    const guardado = guardadoNaOrigem(CHAVES_DA_ORIGEM.modo);
    return MODOS_DE_VOZ.includes(guardado) ? guardado : "VoiceActivated";
  },
  set_voice_mode: ({ mode } = {}) => {
    if (!MODOS_DE_VOZ.includes(mode)) return Promise.reject(`modo de voz desconhecido: ${String(mode)}`);
    return gravarNaOrigem(CHAVES_DA_ORIGEM.modo, mode) ? null : Promise.reject("NaoGravei");
  },
  // Em milésimos, como as preferências do desktop; `null` volta ao padrão.
  ajustar_sensibilidade_da_voz: ({ dbfs } = {}) => {
    if (dbfs != null && !Number.isFinite(dbfs)) return Promise.reject("a sensibilidade tem de ser um número");
    const valor = dbfs == null ? null : String(Math.trunc(dbfs * 1000));
    return gravarNaOrigem(CHAVES_DA_ORIGEM.sensibilidade, valor) ? null : Promise.reject("não consegui gravar a escolha");
  },
  // A força entre 0 e 1, como o desktop; o navegador só liga ou desliga o
  // `noiseSuppression`, e a voz (3.3, lote C) o liga com qualquer força acima de 0.
  ajustar_reducao_de_ruido: ({ forca } = {}) => {
    if (forca != null && !Number.isFinite(forca)) return Promise.reject("a força tem de ser um número");
    const valor = forca == null ? null : String(Math.trunc(Math.min(1, Math.max(0, forca)) * 1000));
    return gravarNaOrigem(CHAVES_DA_ORIGEM.reducao, valor) ? null : Promise.reject("não consegui gravar a escolha");
  },
  escolher_tecla_de_falar: ({ tecla } = {}) =>
    gravarNaOrigem(CHAVES_DA_ORIGEM.tecla, tecla == null ? null : String(tecla)) ? null : Promise.reject("NaoGravei"),
  // A Permissions API: concedida, negada pelo navegador (achado 3 da matriz da
  // F8), ou não se sabe. **E a recusa que esta página já viu**: o Safari nem
  // sempre conta o microfone pela Permissions API, e depois de um «Não
  // Permitir» de verdade ela pode dizer «perguntar» ou nem responder. Sem a
  // lembrança, o aviso sumiria justo quando a sala ficou muda por isso
  // (pergunta da F8, 10/10). Uma concessão posterior vence a lembrança.
  permissao_de_microfone: async () => {
    let estado = null;
    try {
      ({ state: estado } = await navigator.permissions.query({ name: "microphone" }));
    } catch {
      estado = null;
    }
    if (estado === "granted") return "Permitida";
    if (estado === "denied" || recusaDoMicrofone.nestaPagina) return "NegadaPeloNavegador";
    return "NaoSeSabe";
  },
  saida_escolhida: () => guardadoNaOrigem(CHAVES_DA_ORIGEM.saida),
  // Grava; numa sessão, a voz (3.3) a aplica com `setSinkId`.
  escolher_saida: ({ dispositivo } = {}) =>
    gravarNaOrigem(CHAVES_DA_ORIGEM.saida, dispositivo == null ? null : String(dispositivo))
      ? null
      : Promise.reject("NaoGravei"),
  // **A tela cheia do navegador**, no lugar da janela. Pedida dentro do clique,
  // sem `await` antes. Como no desktop, não trocar de modo não lança na casca;
  // aqui ele é dito no console.
  tela_cheia: async ({ ligada } = {}) => {
    const dentro = document.fullscreenElement != null;
    try {
      if (ligada && !dentro) await document.documentElement.requestFullscreen();
      if (!ligada && dentro) await document.exitFullscreen();
    } catch (falha) {
      console.warn("ponte: a tela cheia não trocou", falha);
    }
    return null;
  },
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
