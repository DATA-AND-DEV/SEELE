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
// tarefa do plano que o traz. Chamado, um deles volta com
// `AindaNaoNoNavegador`, e não com um silêncio. A lista encolhe a cada lote, e
// o pacote da página (3.6) só sai com ela vazia.
//
// # A sessão
//
// O `connect` abre a sessão pelo navegador (`sessao-web.js`, Tarefa 2.6): a
// WebTransport para o servidor do link, e a ligação do `.wasm` no meio. Os
// comandos da sessão vão ao `.wasm` (`seele_web::comandos`) por
// `pelaSessao`, que faz em cada um o que o método da FFI faz; aqui ficam só o
// que é do navegador — a voz da página, as escolhas guardadas na origem, a
// espera do `peso_da_linha`.
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
const RECUSA_VERSOES = "as versões do servidor moram no app do SEELE: esta página fala só o Servidor 2.0";
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
  // Os amigos moram na agenda desta máquina (0.17): o navegador não tem agenda.
  ["adicionar_amigo", RECUSA_AGENDA],
  ["amigos", RECUSA_AGENDA],
  ["tirar_dos_amigos", RECUSA_AGENDA],
  // A pré-conferência de hospedar (E2 da 0.17): o navegador não hospeda.
  ["antes_de_hospedar", RECUSA_HOSPEDAR],
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
  // Desfazer a escolha de MOD de um servidor guardado desta máquina (P1).
  ["desfazer_escolha_de_mod", RECUSA_HOSPEDAR],
  ["definir_senha_do_server", RECUSA_PORTARIA],
  ["encerrar_a_ligacao", RECUSA_SALA],
  // Quem pinta o servidor hospedado (tarefa 17f.0 da F3): escolher é de quem
  // hospeda, e o navegador não hospeda.
  ["escolher_atualizacao", RECUSA_ATUALIZAR],
  ["escolher_o_tema_do_servidor", RECUSA_MODS],
  ["hospedar", RECUSA_HOSPEDAR],
  ["instalar_atualizacao", RECUSA_ATUALIZAR],
  ["instalar_mod", RECUSA_MODS],
  ["ligar_portaria", RECUSA_PORTARIA],
  ["linhas_para_hospedar", RECUSA_HOSPEDAR],
  // O link do navegador é do servidor que esta máquina hospeda (D-web-6), e o
  // navegador não hospeda.
  ["link_do_navegador", RECUSA_HOSPEDAR],
  ["meu_codigo", RECUSA_AGENDA],
  ["mod_nativo_ativar", RECUSA_NATIVO],
  ["mod_nativo_colher", RECUSA_NATIVO],
  ["mod_nativo_encerrar", RECUSA_NATIVO],
  ["mod_nativo_entregar", RECUSA_NATIVO],
  ["mod_nativo_reservar", RECUSA_NATIVO],
  ["ocupacao_do_deposito", RECUSA_VERSOES],
  ["onde_estou", RECUSA_SALA],
  ["pacotes_no_cache", RECUSA_MODS],
  ["para_entrar_pela_recusa", RECUSA_VERSOES],
  ["parar_de_hospedar", RECUSA_HOSPEDAR],
  ["pedidos_da_portaria", RECUSA_PORTARIA],
  // A cópia de segurança antes de converter (E2 da 0.17): é do banco de um
  // servidor desta máquina, e o navegador não hospeda.
  ["preparar_copia", RECUSA_HOSPEDAR],
  ["procurar_atualizacao", RECUSA_ATUALIZAR],
  ["receber_ligacoes", RECUSA_SALA],
  ["recusar", RECUSA_SALA],
  ["remover_versao", RECUSA_VERSOES],
  // As saídas de quem hospeda com um MOD de API antiga (o corte da API 6;
  // tarefa 13 do plano da F3, Step 5b): mexem no banco de um servidor guardado
  // desta máquina, e o navegador não guarda servidor.
  ["resolver_mod_de_api_antiga", RECUSA_HOSPEDAR],
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
  "calar_a_tela", "cancelar_ligacao", "codigo_do_mod", "conferir_esta_versao", "conhecidos",
  "connect", "consentir_previa", "controles_da_voz", "descartar_mensagem",
  "destino_do_anexo", "disconnect", "dispensar_aviso", "encerrar_sessao",
  "focar_voz", "sessoes", "ver_sessao",
  "pedir_cargos", "explicar_permissoes", "prever_cargo", "criar_cargo", "editar_cargo",
  "mover_cargo", "apagar_cargo", "atribuir_cargo", "revogar_cargo",
  "dominios_de_gif", "enter_voice_room", "enviar_anexo", "enviar_imagem_mod",
  "escolher_apelido", "escolher_apelido_local", "escolher_arquivo",
  "escolher_microfone", "escolher_minha_imagem", "escolher_para_o_mod",
  "esquecer", "esquecer_aceite_de_mods", "esquecer_previa",
  "estado_da_primeira_abertura", "estado_da_sessao",
  "fechar_teste_de_microfone", "guardar_colado",
  "ha_ajustes_de_acessibilidade", "icone_do_server", "imagem_da_pessoa",
  "instalar_mod_do_catalogo", "leave_voice_room",
  "lembrar_aparencia_do_servidor", "ler_entrada", "ler_imagem_mod", "limite_da_mensagem",
  "limites_dos_campos", "membros_fora_do_ar", "messages", "meu_retrato",
  "microfone_escolhido",
  "microfones",
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

/**
 * Atendidos que ainda não têm corpo, com a tarefa do plano que os traz (o
 * texto dela, numa constante de uma linha: o `cargo xtask pacote-web` lê as
 * duas). Vazia desde a Tarefa 2.6: a ponte atende todos.
 */
const AINDA_A_FAZER = new Map([
]);

/**
 * **As capacidades que esta plataforma tem** (`compatibilidade.por-plataforma`;
 * a auditoria do `v6.json`, B10): o executor do navegador omite, como já omite
 * por versão, a capacidade cujos comandos a ponte não faz — recusados ou ainda
 * por fazer. Um MOD que subisse aqui via o `SeeleUI.enviar` (a capacidade
 * `volume`) e o seletor (`arquivo`), e só recebia recusa.
 */
const COMANDOS_DE_CADA_CAPACIDADE = Object.freeze({
  volume: Object.freeze(["enviar_imagem_mod", "ler_imagem_mod"]),
  arquivo: Object.freeze(["escolher_para_o_mod", "pedaco_do_escolhido", "soltar_escolhido"]),
});

function capacidadesDaPlataforma(capacidades) {
  const faz = (comando) => !AINDA_A_FAZER.has(comando) && !RECUSAS_NOMEADAS.has(comando);
  return (capacidades ?? []).filter((capacidade) =>
    (COMANDOS_DE_CADA_CAPACIDADE[capacidade] ?? []).every(faz));
}

/**
 * O que o desktop responde sem perguntar a ninguém. **JSON estrito**:
 * `tests/web.rs` o lê e confere cada valor contra o Rust (`main.rs`,
 * `seele_ffi`). O `controles_da_voz` é o do desktop sem sessão e sem ajuste
 * guardado; o ajuste guardado na origem é da 3.3.
 */
const CONSTANTES_DO_DESKTOP = Object.freeze({
  "limite_da_mensagem": 4096,
  "limites_dos_campos": {
    "apelido": 32,
    "nome_de_canal": 48,
    "lugares_da_sala": 250,
    "nome_do_servidor": 64,
    "motivo_do_banimento": 512
  },
  "regras_de_previa": { "limit": 4194304, "types": ["image/png", "image/jpeg", "image/gif", "image/webp"] },
  "regras_do_icone_do_server": { "limite_bytes": 8192, "lado": 256 },
  "apis_de_mod_aceitas": [7, 6],
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

// ---------------------------------------------- várias sessões (E3, 0.17)
//
// **A página também abre várias sessões ao mesmo tempo** (contrato E3;
// fronteiras.md v2, §7). Cada `connect` soma uma sessão ao registro da ponte,
// com um `SessaoId` que nunca volta; `ver_sessao` troca a que a casca mostra
// (a VISTA) sem derrubar ninguém; e a voz da página é de UMA sessão (o FOCO),
// trocado só por ato (`focar_voz`, ou entrar numa sala de voz).
//
// **O pedido vai à sessão do carimbo.** A casca (`invoke`, em `base.js`) põe
// em todo comando o carimbo da vista no instante do clique. `invocarNaPonte` o
// tira dos argumentos e faz de `sessaoDaPonte.atual` a sessão do carimbo
// durante a parte síncrona do atendimento — que é onde todo corpo daqui pega a
// sessão (`comASessao`, `pelaSessao`). Um carimbo de sessão que já saiu volta
// `NotConnected`; uma geração velha do enlace, `SessaoNaoConfere`.

/** As sessões abertas pela página, pelo `SessaoId`. */
const sessoesDaPonte = new Map();

/** O próximo `SessaoId`: começa em 1 e nunca volta. */
let proximaSessaoDaPonte = 1;

/** A sessão mostrada, e a que tem a voz; 0 é nenhuma. */
let vistaDaPonte = 0;
let focoDaPonte = 0;

/** A maior época de troca de vista já aplicada (`ver_sessao`). */
let epocaAplicadaNaPonte = 0;

/** A sessão do pedido que está sendo atendido agora, ou `null`. */
let sessaoDoPedido = null;

/**
 * A sessão desta página para quem atende: a do pedido em curso, ou a vista.
 * Sem ela, o que pede uma sessão volta com `"NotConnected"`, como no desktop.
 * É também a costura por onde a bancada põe uma sessão de mentira: atribuir
 * soma a sessão ao registro e a põe na vista (e no foco, se ninguém o tem).
 */
const sessaoDaPonte = {
  get atual() {
    return sessaoDoPedido ?? sessoesDaPonte.get(vistaDaPonte) ?? null;
  },
  set atual(sessao) {
    if (!sessao) {
      if (vistaDaPonte) sessoesDaPonte.delete(vistaDaPonte);
      if (focoDaPonte === vistaDaPonte) focoDaPonte = 0;
      vistaDaPonte = [...sessoesDaPonte.keys()].pop() ?? 0;
      return;
    }
    if (!sessao.idDaPonte) sessao.idDaPonte = proximaSessaoDaPonte++;
    sessoesDaPonte.set(sessao.idDaPonte, sessao);
    vistaDaPonte = sessao.idDaPonte;
    // **Conectar só ganha a voz sendo a ÚNICA sessão** (fronteiras §5; o
    // achado do Codex): depois de a sessão com a voz cair, o foco fica vazio,
    // e uma entrada nova ao lado de outras não o toma sozinha.
    if (!focoDaPonte && sessoesDaPonte.size === 1) focoDaPonte = sessao.idDaPonte;
  },
};

/**
 * **A cadeia única do foco de voz**: toda mudança dele — `focar_voz`, entrar
 * numa sala de outra sessão, a sessão com a voz que cai — entra aqui, uma
 * depois da outra. Nenhuma é feita fora dela (o mesmo desenho do registro web
 * do browser-signaling, `criarRegistroDeSessoesWeb`).
 */
let cadeiaDoFoco = Promise.resolve();
function naCadeiaDoFoco(passo) {
  const vez = cadeiaDoFoco.catch(() => {}).then(passo);
  cadeiaDoFoco = vez;
  return vez;
}

/**
 * As despedidas do servidor que NÃO acabam a sessão, e que a casca diz como
 * queda que volta sozinha (`CaiuComMotivo`). A regra do desktop é a do core
 * (`despedida::a_sessao_acabou_aqui`); a única medida na web hoje é o
 * «ficou para trás» (browser-signaling e B).
 */
const DESPEDIDAS_QUE_VOLTAM = new Set(["FellBehind"]);

/** O carimbo de uma sessão da ponte agora. */
function carimboDaPonte(sessao) {
  return { sessao: sessao.idDaPonte, geracao: sessao.geracaoDoEnlace ?? 1 };
}

/** O que mudou nas sessões: o sinal «releia `sessoes()`» (`seele://sessoes`). */
function avisarAsSessoes(sessao, motivo) {
  emitirNaPonte("seele://sessoes", { sessao, motivo });
}

/**
 * As sessões, na forma do `SessaoNaTela` do app (fronteiras.md §6). Uma página
 * nunca hospeda: todo servidor dela é de outra pessoa (`onde: "Outro"`).
 */
function sessoesNaPonte() {
  return [...sessoesDaPonte.values()].map((sessao) => {
    let lido = null;
    try {
      lido = sessao.acabou ? null : sessao.retrato?.();
    } catch {
      lido = null;
    }
    const sala = lido?.sala ?? {};
    const naSala = (sala.voice_rooms ?? []).find((s) => s.occupied_by_us)?.name ?? null;
    return {
      carimbo: carimboDaPonte(sessao),
      endereco: sessao.alvo?.endereco ?? "",
      nome: sala.server || null,
      na_vista: sessao.idDaPonte === vistaDaPonte,
      foco_de_voz: sessao.idDaPonte === focoDaPonte,
      na_sala_de_voz: naSala,
      onde: sessao.onde ?? "Outro",
      // O navegador não liga por código: aqui é sempre `null`. Só a costura das
      // bancadas põe `ligacao` numa sessão de mentira, para provar a vista.
      ligacao: sessao.ligacao ?? null,
      enlace: sessao.acabou ? "encerrada" : sessao.voltando ? "reconectando" : "online",
      encerrada: sessao.motivoDoFim ?? null,
      nao_lidas: sessao.idDaPonte === vistaDaPonte ? 0 : sessao.naoLidas ?? 0,
      mudou: sessao.idDaPonte !== vistaDaPonte && Boolean(sessao.mudou),
    };
  });
}

/**
 * **Troca o foco de voz, em ordem**: a sessão que tinha a voz sai da sala dela
 * e a voz da página sai com ela ANTES de a nova ganhar o foco. Nunca duas
 * sessões com voz; e nada aqui é chamado sozinho (só `focar_voz` e entrar numa
 * sala). Responde `{ antes, agora, saiu_da_sala }`.
 */
function focarAVozNaPonte(id, manterOMicrofone = false) {
  return naCadeiaDoFoco(() => trocarOFocoDaPonte(id, manterOMicrofone));
}

/** O corpo da troca, só chamado pela cadeia. */
async function trocarOFocoDaPonte(id, manterOMicrofone = false) {
  const antes = focoDaPonte;
  const nova = id ? sessoesDaPonte.get(id) : null;
  // Uma sessão que saiu ou caiu enquanto a troca esperava a vez não ganha a voz.
  if (id && (!nova || nova.acabou)) throw "NotConnected";
  if (antes === (id || 0)) return { antes: antes || null, agora: antes || null, saiu_da_sala: null };
  focoDaPonte = 0;
  let saiuDaSala = null;
  const velha = sessoesDaPonte.get(antes);
  if (velha && !velha.acabou) {
    try {
      const lido = velha.retrato?.();
      saiuDaSala = (lido?.sala?.voice_rooms ?? []).find((s) => s.occupied_by_us)?.name ?? null;
      if (saiuDaSala) velha.atender("leave_voice_room", {});
    } catch (falha) {
      console.warn("ponte: a sessão que perdeu a voz não saiu da sala:", falha);
    }
  }
  // **O microfone segue o requisito: fora da sala da sessão em foco, sem microfone.** Trocar o foco sozinho (`focar_voz`) deixa a pessoa fora de
  // qualquer sala, e a captura é solta. A única exceção é ENTRAR numa sala de outra sessão (`enter_voice_room`): o `ligar` já abriu o microfone
  // dentro do GESTO dessa sala nova (o iOS só deixa abrir ali), e soltá-lo agora o perderia e pediria o `getUserMedia` outra vez.
  if (manterOMicrofone) sairDaVozMantendoOMicrofone();
  else sairDaVozDaSessao();
  focoDaPonte = id || 0;
  avisarAsSessoes(id || antes, "foco");
  return { antes: antes || null, agora: id || null, saiu_da_sala: saiuDaSala };
}

/**
 * A geração da sessão desta página: sobe a cada `connect`, como o `revogar`
 * do desktop. O `mod_request` e o `soltar_a_entrada` a conferem, e os eventos
 * de uma geração velha não chegam à casca.
 */
let geracaoDaPonte = 0;

/** O que o desktop faz sem conexão: `session.connection()?`. */
function comASessao(fazer) {
  const sessao = sessaoDaPonte.atual;
  if (!sessao) return Promise.reject("NotConnected");
  return fazer(sessao);
}

/**
 * Sai de uma sessão desta página (a do pedido, ou a vista), e só dela: as
 * outras continuam. A voz fecha só se era dela; os arquivos guardados, só com
 * a última.
 */
async function sairDaSessaoDaPonte(sessao = sessaoDaPonte.atual) {
  if (!sessao) {
    // Sem sessão nenhuma, o `disconnect` de sempre: solta o que a página
    // guardava e a voz, como antes da multi-sessão.
    if (sessoesDaPonte.size === 0) {
      ALCAS_DA_PONTE.clear();
      fecharAVozDaSessao();
    }
    return null;
  }
  const id = sessao.idDaPonte;
  sessoesDaPonte.delete(id);
  if (vistaDaPonte === id) vistaDaPonte = [...sessoesDaPonte.keys()].pop() ?? 0;
  // O foco, pela cadeia: a saída espera a troca em curso, e a solta depois.
  await naCadeiaDoFoco(() => {
    if (focoDaPonte === id) {
      focoDaPonte = 0;
      fecharAVozDaSessao();
    }
  });
  if (sessoesDaPonte.size === 0) ALCAS_DA_PONTE.clear();
  sessao.transmissao?.fechar();
  await sessao.encerrar?.();
  avisarAsSessoes(id, "saiu");
  return null;
}

// ------------------------------------------- a entrada pela casca larga

/**
 * **O link colado no SERVIDORES** (`analisar_convite`), só na memória e só até
 * o `connect` que o usa: é o caminho do segredo do convite na casca larga do PC
 * (D-web-3), como a tentativa da identidade o é no celular. O `connect` o
 * entrega à identidade, que o abre e entra; ninguém mais o lê.
 */
let linkColadoDaPonte = null;
// Um convite recebido na URL substitui também o que ficou no diálogo da
// casca larga. Para o mesmo endereço, ele não pode recuperar o token antigo.
document.addEventListener("seele:link-do-navegador", () => {
  linkColadoDaPonte = null;
});

/** A entrada em curso, para o `cancelar_ligacao`. */
const entradaEmCurso = { cancelar: null };

/** As frases dos links que o navegador não abre, pelo erro do `analisar_link`. */
const FRASES_DO_LINK = Object.freeze({
  OutraOrigem: "este não é um link do navegador para o SEELE: peça a quem hospeda o link do navegador",
  // Na 0.17 o navegador fala só o Servidor 2.0: um link da 0.16 ou de antes
  // chega como `OutraLinha` (`seele_proto::uri::ErroDoNavegador`), e não mais
  // como falta de hashes.
  OutraLinha:
    "este link é de um servidor de uma versão anterior do SEELE (0.16 ou antes), que o navegador não abre: abra-o no app do SEELE, que abre a versão certa ao lado",
  SemFragmento: "o link chegou sem o servidor: copie-o de novo, inteiro",
  ConsultaForaDoFragmento: "o link chegou partido: copie-o de novo, inteiro",
  SemImpressaoDigital: "o link chegou sem a impressão do servidor: copie-o de novo, inteiro",
  Convite: "o link chegou incompleto ou alterado: copie-o de novo, inteiro",
});

/** As entradas pela identidade que acabam antes do servidor, com a frase. */
const FRASES_DA_IDENTIDADE = Object.freeze({
  "preparacao-expirada": "o navegador não concluiu a preparação da entrada a tempo; tente de novo nesta página",
  "armazenamento-indisponivel": "o navegador não guardou a identidade; mantenha esta página aberta e tente de novo",
  "link-vencido": "o link deste servidor venceu, e ele não trouxe um novo: peça a quem hospeda um link novo",
  "convite-vencido": "este link venceu: peça a quem hospeda um link novo",
  "sem-tentativa": "outra entrada começou no meio desta",
  // A portaria de quem hospeda (browser-signaling, 0.17): a batida do navegador
  // fica na fila dele, como a do app.
  "espera-de-admissao":
    "quem hospeda ainda não decidiu: a sua batida está na porta do servidor, e esta página entra quando ele admitir",
  "admissao-recusada": "quem hospeda recusou esta entrada: fale com ele por outro canal se for engano",
});

/** O link lido pelo `.wasm` da página, com as regras do produto. */
async function lerOLink(texto) {
  const { instancia } = await carregarOWasmDaPagina();
  return analisadorDoWasm(instancia.exports)(texto);
}

/**
 * **O `connect` da casca larga**: um endereço em texto, como o desktop o
 * recebe. A chave e os hashes estão com a identidade: o link colado que ainda
 * espera, a tentativa que ela tem aberta (a volta depois do aceite de MODs), ou
 * um servidor deste aparelho. Um endereço que ela não conhece não traz a chave
 * que confere quem atendeu, e é recusado com a frase.
 */
async function entrarPeloEndereco({ server, nickname } = {}) {
  const identidade = await identidadeDaPagina();
  if (identidade.ler()?.estado === "armazenamento-indisponivel") await identidade.reler();
  const apelido = (nickname ?? "").trim() || guardadoNaOrigem(CHAVES_DA_ORIGEM.apelido) || "pessoa";
  let resultado;
  if (linkColadoDaPonte?.alvo === server) {
    const { texto } = linkColadoDaPonte;
    linkColadoDaPonte = null;
    await identidade.abrir(texto);
    resultado = await identidade.entrar(apelido);
  } else if (identidade.ler()?.endereco === server) {
    resultado = await identidade.entrar(apelido);
  } else {
    const conhecido = (await identidade.conhecidos()).find((servidor) => servidor.endereco === server);
    if (!conhecido) {
      return Promise.reject({ RecusaDaPonte: { comando: "connect", frase: RECUSA_ENDERECO_SOZINHO } });
    }
    resultado = await identidade.entrarNoConhecido(conhecido.impressao, apelido);
  }
  if (resultado.ok) return resultado.resposta;
  if (resultado.motivo !== undefined) return Promise.reject(resultado.motivo);
  const frase = FRASES_DA_IDENTIDADE[resultado.erro] ?? resultado.razao ?? resultado.erro;
  return Promise.reject({ RecusaDaPonte: { comando: "connect", frase } });
}

/**
 * A impressão do servidor de um endereço: o da sessão aberta, o do link colado,
 * ou um deste aparelho. Os aceites de MODs são guardados por ela, e a casca do
 * desktop pergunta pelo endereço.
 */
async function impressaoDoEndereco(alvo) {
  const sessao = sessaoDaPonte.atual;
  if (sessao?.alvo?.endereco === alvo) return sessao.alvo.impressao;
  if (linkColadoDaPonte?.alvo === alvo) return linkColadoDaPonte.impressao;
  try {
    const identidade = await identidadeDaPagina();
    return (await identidade.conhecidos()).find((servidor) => servidor.endereco === alvo)?.impressao ?? null;
  } catch {
    return null;
  }
}

/** A fonte da tela no navegador: quem escolhe é o seletor dele. */
const FONTE_DO_NAVEGADOR = "a tela ou a janela que o navegador oferecer";

/** A captura que o navegador não deu (a pessoa fechou o seletor, ou a permissão está negada). */
const RECUSA_DA_CAPTURA = "o navegador não deu a tela a esta página: escolha a tela ou a janela no seletor dele";

/** Um endereço digitado não traz a chave do servidor: no navegador se entra pelo link. */
const RECUSA_ENDERECO_SOZINHO =
  "no navegador se entra pelo link do servidor: um endereço sozinho não traz a chave que confere quem atendeu";

/**
 * **O `connect` do navegador**: o alvo que a identidade montou
 * (`identidade-web.js`), e a sessão pela WebTransport (`sessao-web.js`).
 * Resolve com a `Entrada` do desktop (`{snapshot, veredito, geracao,
 * origem_da_impressao, a_lista_guardou}`) e rejeita com o `ConnectionError`
 * dele, nu: é o que o `desfecho` da identidade lê.
 */
async function abrirASessaoPelaPonte({ alvo, nickname, joinSecret, server, preparacao } = {}) {
  if (!alvo || typeof alvo !== "object" || !alvo.chave) {
    if (typeof server === "string") return entrarPeloEndereco({ server, nickname });
    return Promise.reject({ RecusaDaPonte: { comando: "connect", frase: RECUSA_ENDERECO_SOZINHO } });
  }
  // **Várias sessões** (E3): uma entrada nova soma, e não recusa. Só duas
  // entradas no MESMO servidor ao mesmo tempo são recusadas, como no desktop.
  if ([...sessoesDaPonte.values()].some((s) => !s.acabou && s.alvo?.impressao === alvo.impressao)) {
    return Promise.reject("AlreadyConnected");
  }
  geracaoDaPonte += 1;
  const geracao = geracaoDaPonte;
  // O `SessaoId` nasce na tentativa: os eventos da chegada já saem com ele.
  const id = proximaSessaoDaPonte++;
  const tentativa = { idDaPonte: id };
  /** Esta sessão ainda é do registro (ou ainda está entrando). */
  const valeAinda = () => !sessoesDaPonte.has(id) ? tentativa.entrando !== false : true;
  tentativa.entrando = true;
  /** O evento carimbado, e as marcas da placa quando a sessão está ao fundo. */
  const emitirDaSessao = (evento) => {
    const aberta = sessoesDaPonte.get(id);
    if (aberta && vistaDaPonte !== id) {
      aberta.mudou = true;
      if (evento === "MessagesChanged") aberta.naoLidas = (aberta.naoLidas ?? 0) + 1;
    }
    emitirNaPonte("seele://event", {
      sessao: id, geracao: aberta?.geracaoDoEnlace ?? 1, vista: vistaDaPonte === id, evento,
    });
    if (aberta && vistaDaPonte !== id) avisarAsSessoes(id, evento === "MessagesChanged" ? "mensagens" : "aviso");
  };
  const controle = new AbortController();
  const cancelamento = { cancelada: false, aoCancelar: null };
  const cancelar = () => {
    cancelamento.cancelada = true;
    controle.abort(preparacao?.sinal?.reason ?? "Cancelada");
    cancelamento.aoCancelar?.();
  };
  // Instalado antes do WASM: cancelar durante a preparação também vale.
  entradaEmCurso.cancelar = cancelar;
  preparacao?.sinal?.addEventListener("abort", cancelar, { once: true });
  if (preparacao?.sinal?.aborted) cancelar();
  const preparo = { sinal: controle.signal, ate: preparacao?.ate ?? Date.now() + PRAZO_DA_PREPARACAO_DA_ENTRADA_MS };
  let wasm;
  let sessao;
  try {
    wasm = await esperarEtapaDaEntrada(async () => typeof instanciarOWasmDaSessao === "function"
      ? instanciarOWasmDaSessao()
      : (await carregarOWasmDaPagina()).instancia.exports, preparo);
    sessao = await abrirASessaoDoNavegador({
      wasm,
      alvo,
      cancelamento,
      preparacao: preparo,
      apelido: nickname ?? "",
      convite: joinSecret ?? null,
      aceito: aceiteDeModsGuardado(alvo.impressao),
      // A voz só sai e só toca na sessão com o foco (o portão do transporte).
      podeUsarAVoz: () => focoDaPonte === id,
      emitir: (evento) => {
        if (!valeAinda()) return;
        if (evento && evento.ScreenClosed && focoDaPonte === id) vozDaPonte.atual?.esquecerOSomDaTela(evento.ScreenClosed.screen);
        emitirDaSessao(evento);
      },
      aoAcabar: (motivo) => {
        // Os arquivos escolhidos para os MODs saem com a sessão, como no
        // desktop, acabe ela por quem acabar.
        soltarOsEscolhidosDa(geracao).catch(() => {});
        const aberta = sessoesDaPonte.get(id);
        if (!aberta) return;
        aberta.motivoDoFim = motivo ?? aberta.motivoDoFim ?? "LinkLost";
        aberta.transmissao?.fechar();
        // A voz fecha só se era desta; o foco fica vazio, e ninguém o ganha
        // sozinho (fronteiras §5). **Pela mesma cadeia do foco** (achado do
        // Codex): solto fora dela, um `focar_voz` em curso podia dar a voz a
        // esta sessão já caída, ou ser desfeito por ela.
        naCadeiaDoFoco(() => {
          if (focoDaPonte === id) {
            focoDaPonte = 0;
            fecharAVozDaSessao();
            avisarAsSessoes(id, "foco");
          }
        }).catch(() => {});
        avisarAsSessoes(id, "encerrada");
      },
    });
    if (controle.signal.aborted) {
      if (sessao.abortar) sessao.abortar(); else sessao.encerrar();
      throw controle.signal.reason ?? "Cancelada";
    }
  } finally {
    preparacao?.sinal?.removeEventListener("abort", cancelar);
    if (entradaEmCurso.cancelar === cancelar) entradaEmCurso.cancelar = null;
    tentativa.entrando = false;
  }
  sessao.idDaPonte = id;
  sessao.geracao = geracao;
  sessao.alvo = alvo;
  sessao.snapshot = () => retratoDaSessao(sessao);
  sessao.transmissao = criarATransmissaoDoNavegador({
    sessao,
    wasm,
    eu: () => sessao.retrato()?.sala?.me ?? null,
    emitir: (evento) => {
      if (sessoesDaPonte.has(id)) emitirDaSessao(evento);
    },
  });
  // A voz da página segue a sala confirmada só na sessão com o foco: as
  // outras ficam no texto, na presença e na tela, sem som.
  sessao.aoDobrar = (mensagem) => {
    if (focoDaPonte === id) conferirAVozDaSessao(sessao);
    sessao.transmissao.aoDobrar(mensagem);
    // **A despedida recuperável, dita à casca** como o desktop a diz (o
    // `CaiuComMotivo` do ffi do B, carimbado pela geração que caiu): a
    // conexão volta sozinha, sem tela de fim. Pelo quadro ou pelo código de
    // fechamento, o `sessao-web.js` dobra os dois num `Disconnecting` só; uma
    // vez por geração do enlace.
    const motivo = mensagem?.Disconnecting?.reason;
    if (DESPEDIDAS_QUE_VOLTAM.has(motivo) && sessao.caiuNaGeracao !== sessao.geracaoDoEnlace) {
      sessao.caiuNaGeracao = sessao.geracaoDoEnlace;
      emitirDaSessao({ CaiuComMotivo: { reason: motivo } });
    }
  };
  sessao.aCadaVolta = () => {
    seguirOsVolumes().catch((falha) => console.warn("volumes na mistura:", falha));
  };
  // **A queda e a volta**, como no desktop: a voz fica de pé na bateria (os
  // datagramas não saem, e ela segue no transporte novo), a tela que estava no
  // ar fecha com o transporte, e a volta repete o mudo e o isolamento. A sala
  // que o servidor não resgatou fecha e abre de novo pelo `aoDobrar`, e um
  // número de voz novo faz a voz entrar com ele (`conferirAVozDaSessao`).
  sessao.escolhas = () => ({ mudo: vozDaSessao.mudo, isolado: vozDaSessao.isolado });
  sessao.aoCair = () => sessao.transmissao.fechar();
  sessao.aoVoltar = () => {
    if (focoDaPonte === id) conferirAVozDaSessao(sessao);
  };
  // O som da tela vai à mistura; a tela que fecha leva o dela.
  sessao.aoSomDaTela = (tela, opus) => {
    if (vozDaSessao.dentro && focoDaPonte === id) vozDaPonte.atual?.somDaTela(tela, Uint8Array.from(opus));
  };
  // No registro, na vista, e com o foco só se ninguém o tem (o caso de uma
  // sessão só, que é a página de antes): conectar nunca toma a voz de outra.
  sessaoDaPonte.atual = sessao;
  avisarAsSessoes(id, "entrou");
  // O retrato guardado sobe com a entrada, como no `connect` do desktop.
  const retrato = bytesDoDataUri(guardadoNaOrigem(CHAVES_DA_ORIGEM.retrato));
  if (retrato) sessao.atender("minha_imagem", { icone: Array.from(retrato) });
  aVozDaPonte().catch((falha) => console.warn("ponte: a voz da página não carregou:", falha));
  return {
    // A `Entrada` do app (fronteiras §2): o carimbo da sessão nova, e a época
    // da vista, que os MODs conferem como `geracao`.
    carimbo: carimboDaPonte(sessao),
    vista: geracao,
    snapshot: sessao.snapshot(),
    veredito: alvo.conhecido ? "Known" : { FirstContactVerified: { fingerprint: alvo.impressao } },
    geracao,
    origem_da_impressao: alvo.conhecido ? "Lista" : "Link",
    a_lista_guardou: false,
  };
}

/**
 * **O retrato do desktop**: o que sai do `Room` vem do `.wasm` (a sala, os
 * contados e a telemetria que a ligação mede), e o que esta ponta mede (o
 * enlace, a voz) vem daqui. Os campos são os do `Snapshot`, e cada um tem um
 * lado (`MEDIDOS_POR_CADA_PONTA`, em `dois_produtores_de_snapshot.rs`).
 */
function retratoDaSessao(sessao) {
  const lido = sessao.retrato();
  if (!lido) throw "NotConnected";
  // A voz da página é da sessão com o foco. As outras não medem nada dela:
  // nem nível, nem fala, nem abertura (E3; fronteiras §5).
  const comAVoz = !focoDaPonte || focoDaPonte === sessao.idDaPonte;
  const voz = comAVoz
    ? vozDaSessao
    : { ...vozDaSessao, bitrate: 0, nivel: 0, recusados: 0, falando: false, abertura: "fora_de_sala" };
  return {
    // O carimbo junto, achatado, como o `RetratoNaSessao` do app.
    carimbo: sessao.idDaPonte ? carimboDaPonte(sessao) : undefined,
    caminho: null,
    link: sessao.enlace(),
    // A WebTransport só abre com um dos hashes que o link trouxe: quem atendeu
    // foi conferido.
    link_state: "Verified",
    ...lido.sala,
    ...lido.contados,
    telemetry: {
      ...lido.telemetria,
      bitrate_bps: voz.bitrate,
      input_level: voz.nivel,
      local_fault: false,
      frames_refused: voz.recusados,
      microfone_entrega_silencio_digital: false,
      cancelamento_de_eco_desligado: voz.semEco === true && voz.abertura === "aberto",
    },
    muted: voz.mudo,
    total_isolation: voz.isolado,
    speaking: voz.falando,
    voice_mode: modoDeVozGuardado(),
    audio_available: audioDisponivel(),
    abertura_do_audio: voz.abertura,
    capture: null,
    playback: null,
    aparelho: "funcionando",
    trocas_de_aparelho: 0,
  };
}

/**
 * Um comando que o `.wasm` responde sem sessão (a busca, que é da página):
 * pela ligação da página, aberta ou não.
 */
function pelaPagina(comando) {
  return async (args) => {
    const { instancia } = await carregarOWasmDaPagina();
    const lido = fronteiraDaLigacao(instancia.exports).atender(comando, args);
    return lido.codigo === 0 ? lido.resposta : Promise.reject(lido.erro);
  };
}

/**
 * Um comando da sessão pelo `.wasm` (`seele_web::comandos`), sempre numa
 * promessa: o `.wasm` responde na hora, e quem chama encadeia como no `invoke`.
 */
/** O mesmo, numa sessão já escolhida: para depois de uma espera (E3). */
function pelaSessaoDe(sessao, comando) {
  return async (args) => {
    const feito = sessao.atender(comando, args);
    if (!feito) return Promise.reject({ ComandoDesconhecido: { comando } });
    return "erro" in feito ? Promise.reject(feito.erro) : feito.resposta;
  };
}

function pelaSessao(comando) {
  return (args) =>
    comASessao(async (sessao) => {
      const feito = sessao.atender(comando, args);
      if (!feito) return Promise.reject({ ComandoDesconhecido: { comando } });
      return "erro" in feito ? Promise.reject(feito.erro) : feito.resposta;
    });
}

// ------------------------------------------------------------ a voz na sessão

/**
 * A voz da sessão: as escolhas desta página (o mudo e o isolamento valem
 * fora da sala, como no desktop) e o que a voz aberta mede.
 */
const vozDaSessao = {
  mudo: false,
  isolado: false,
  tecla: false,
  dentro: false,
  /** O número da voz com que ela entrou: a volta traz outro. */
  numero: null,
  ligando: null,
  /**
   * O PEDIDO mais novo de entrar ou sair da sala (cada ENTRAR e cada SAIR toma um número). Uma resposta (a rejeição de uma entrada, a resposta de um
   * sair) só age sobre a captura se o pedido dela ainda é o mais novo: a resposta velha de um pedido não solta nem desliga a captura de um pedido novo.
   */
  pedido: 0,
  /**
   * Quantas vezes a voz abriu ou fechou. O que uma abertura deixou pendente (os
   * callbacks de `conferirAVozDaSessao`) só vale se o número que ela viu é o de
   * agora: uma abertura velha que termina depois de outra nascer não escreve o
   * estado da nova. O erro da abertura VIGENTE continua escrevendo o dele.
   */
  geracao: 0,
  abertura: "fora_de_sala",
  falando: false,
  /** Se o microfone desta abertura só abriu sem o cancelamento de eco (`semEcoPorque` do motor). */
  semEco: false,
  /** O volume (0 a 4) e o mudo do som da tela, como o desktop os guarda. */
  volumeDaTela: 1,
  telaCalada: false,
  nivel: 0,
  bitrate: 0,
  recusados: 0,
};

/** O byte do modo para o `voz_modo` do `.wasm`. */
const BYTE_DO_MODO = Object.freeze({ PushToTalk: 0, VoiceActivated: 1, Open: 2 });

/**
 * **Liga o áudio dentro do gesto** de quem pediu a sala: o `AudioContext` e o
 * `play()` do iOS só valem ali. A voz da página já nasceu com a sessão, então
 * nada espera antes do `ligar`.
 */
function ligarOAudioNoGesto() {
  const voz = vozDaPonte.atual;
  if (!voz || vozDaSessao.ligando) return;
  // **A abertura é dona do próprio `ligando`**: uma abertura antiga que falha
  // depois de uma nova ter nascido (o `pagehide`, a sessão que acabou) não zera
  // o da nova.
  vozDaSessao.geracao += 1;
  const esta = voz
    .ligar({ microfone: guardadoNaOrigem(CHAVES_DA_ORIGEM.microfone), saida: guardadoNaOrigem(CHAVES_DA_ORIGEM.saida) })
    .catch((falha) => {
      if (vozDaSessao.ligando === esta) vozDaSessao.ligando = null;
      throw falha;
    });
  vozDaSessao.ligando = esta;
  esta.catch(() => {});
}

/**
 * **A voz segue a sala confirmada**, como a `PortaDaVoz` do desktop: abre
 * quando o servidor já não pode recusar a entrada (o `Pong` do `Ping`
 * marcado), e fecha quando a pessoa sai.
 */
function conferirAVozDaSessao(sessao) {
  const lido = sessao.atender("sala_da_voz", {});
  const sala = lido && !("erro" in lido) ? lido.resposta.sala : null;
  const voz = vozDaPonte.atual;
  if (sala === null) {
    // `Voltou` confirma o transporte, não a reentrada: o assento desejado já
    // existe, mas o confirmado espera o Pong marcado. Preserve a captura até
    // essa decisão; recusa, saída e sala apagada desfazem o desejado no Rust.
    if (lido?.resposta?.desejada != null) return;
    // Quem estava DENTRO e já não está (saiu, foi expulso, a sala acabou):
    // o microfone é solto. Quem ainda não entrou (o `ligar` do gesto espera
    // a confirmação do servidor) mantém a abertura pendente: soltá-la agora
    // derrubaria a entrada que acabou de ser pedida.
    if (vozDaSessao.dentro) sairDaVozDaSessao();
    else mudarAAbertura("fora_de_sala");
    return;
  }
  // A conexão nova da volta tem outro número de voz: a voz sai e entra com ele, e o microfone FICA (outra captura pediria a permissão de novo).
  if (vozDaSessao.dentro && vozDaSessao.numero !== lido.resposta.voz) sairDaVozMantendoOMicrofone();
  if (vozDaSessao.dentro) return;
  if (!voz || !vozDaSessao.ligando) {
    mudarAAbertura("sem_aparelho");
    return;
  }
  vozDaSessao.dentro = true;
  mudarAAbertura("abrindo");
  const numero = lido.resposta.voz;
  vozDaSessao.numero = numero;
  const geracao = vozDaSessao.geracao;
  vozDaSessao.ligando.then(
    () => {
      if (geracao !== vozDaSessao.geracao || !vozDaSessao.dentro || sessao.acabou) return;
      voz.mudo(vozDaSessao.mudo);
      voz.isolar(vozDaSessao.isolado);
      voz.modo(BYTE_DO_MODO[modoDeVozGuardado()] ?? 1);
      voz.tecla(vozDaSessao.tecla);
      const sensibilidade = milesimosGuardados(CHAVES_DA_ORIGEM.sensibilidade);
      voz.regua(sensibilidade === null ? 0 : Math.round(sensibilidade * 1000));
      voz.volumeDaTela(vozDaSessao.volumeDaTela);
      voz.calarATela(vozDaSessao.telaCalada);
      voz.entrar(sessao.transporteDaVoz, numero);
      // A recepção recomeça as alças: os ganhos se aplicam de novo.
      ganhosAplicados.clear();
      mudarAAbertura("aberto");
    },
    (falha) => {
      // De outra geração (a abertura velha que termina depois de a nova nascer): nada a escrever.
      if (geracao !== vozDaSessao.geracao) return;
      vozDaSessao.dentro = false;
      if (falha && falha.name === "NotAllowedError") recusaDoMicrofone.nestaPagina = true;
      console.warn("ponte: a voz da sala não abriu:", falha);
      mudarAAbertura("sem_aparelho");
    },
  );
}

/**
 * **A voz sai da sala E o microfone é solto** (o requisito do dono: o microfone não persiste fora da sala). É a saída REAL: a pessoa saiu, foi
 * expulsa, a sala acabou, ou o foco de voz trocou sem entrar noutra sala. `desligar()` do motor para a faixa (`track.stop()`), o codec e o contexto;
 * a geração sobe, e uma abertura pendente (um `getUserMedia` que ainda não voltou) que resolver depois é DESCARTADA e a faixa que chegou, parada.
 * Entrar de novo (o gesto de ENTRAR NA SALA) abre um `getUserMedia` novo.
 */
function sairDaVozDaSessao() {
  soltarOMicrofoneDaVoz();
}

/**
 * A voz sai da sala, e o microfone FICA: só para a volta da conexão (o número da voz mudou: a voz sai e entra com o novo, na mesma captura) e para
 * a troca de foco que entra noutra sala (o microfone dela foi aberto no gesto). Nunca para uma saída real.
 */
function sairDaVozMantendoOMicrofone() {
  if (vozDaSessao.dentro) {
    vozDaPonte.atual?.sair();
    vozDaSessao.dentro = false;
    vozDaSessao.numero = null;
  }
  mudarAAbertura("fora_de_sala");
}

/** Solta o microfone da página, sem tocar no TESTE do microfone de CONFIGURAÇÕES (outro caminho, com o próprio ciclo): este fica de pé até a pessoa pará-lo. */
function soltarOMicrofoneDaVoz() {
  const voz = vozDaPonte.atual;
  const haCaptura = vozDaSessao.dentro || Boolean(vozDaSessao.ligando);
  Object.assign(vozDaSessao, { dentro: false, numero: null, ligando: null, falando: false });
  vozDaSessao.geracao += 1;
  mudarAAbertura("fora_de_sala");
  if (!haCaptura || !voz) return;
  // Com o teste de CONFIGURAÇÕES aberto, o áudio é dele: só a voz da sala sai, e o `fecharTeste` desliga quando ele acabar.
  if (voz.estadoDoTeste && voz.estadoDoTeste() !== null) {
    voz.sair();
    return;
  }
  voz.desligar().catch(() => {});
}

function mudarAAbertura(abertura) {
  if (vozDaSessao.abertura === abertura) return;
  vozDaSessao.abertura = abertura;
  emitirNaPonte("seele://event", "TelemetryChanged");
}

/** A sessão acabou: a voz da sala fecha, e o microfone é solto. */
function fecharAVozDaSessao() {
  const ligando = vozDaSessao.ligando;
  Object.assign(vozDaSessao, { dentro: false, numero: null, ligando: null, abertura: "fora_de_sala", falando: false });
  vozDaSessao.geracao += 1;
  // **Cancela AGORA**, e não quando a abertura terminar: o `desligar` do motor
  // marca a geração, e uma abertura pendente (o `getUserMedia` que ainda não
  // voltou) para o que receber depois, sem desligar a voz de uma sessão nova.
  if (ligando) vozDaPonte.atual?.desligar().catch(() => {});
}

/** As alças cujo ganho já foi aplicado, e com que ganho. */
const ganhosAplicados = new Map();

/**
 * **Os volumes seguem as pessoas**, como o `seguir_os_volumes` do desktop: a
 * cada volta, o ganho de quem tem volume escolhido, pela alça de agora (quem
 * voltou com outra voz ganha outra alça, e o mesmo ganho).
 */
async function seguirOsVolumes() {
  // Os ganhos são da mistura, que toca só a sessão com o foco (E3).
  const sessao = sessoesDaPonte.get(focoDaPonte) ?? sessaoDaPonte.atual;
  const voz = vozDaPonte.atual;
  if (!sessao || !voz || !vozDaSessao.dentro) return;
  // **Os volumes estão na instância da SESSÃO, e as alças da mistura na da
  // PÁGINA** (medido pelo browser-signaling no servidor real: `ganhosDaVoz()`
  // devolvia `{}`). O `ganhosNaMistura` leva um ao outro num bloco opaco.
  const daPagina = (await carregarOWasmDaPagina()).instancia.exports;
  if (vozDaPonte.atual !== voz) return;
  for (const [alca, ganho] of sessao.ganhosNaMistura(daPagina)) {
    if (ganhosAplicados.get(alca) === ganho) continue;
    voz.ganho(alca, ganho);
    ganhosAplicados.set(alca, ganho);
  }
}

/** Se esta página pode ter áudio: o `quer_audio` do desktop. */
const audioDisponivel = () => Boolean(navigator.mediaDevices?.getUserMedia) && typeof AudioContext === "function";

/** O modo de voz guardado nesta origem (o `modo_de_voz_escolhido`). */
function modoDeVozGuardado() {
  const guardado = guardadoNaOrigem(CHAVES_DA_ORIGEM.modo);
  return MODOS_DE_VOZ.includes(guardado) ? guardado : "VoiceActivated";
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

// ------------------------------------------------------------- os anexos

/** Onde o navegador grava um anexo salvo, para a frase do SALVAR: a página não vê a pasta. */
const PASTA_DO_NAVEGADOR = "a pasta de downloads do navegador";

/** O passo do andamento de um anexo: o bloco do `seele_proto::attachment`. */
const BLOCO_DO_ANEXO = 64 * 1024;

/** Um `TransferChanged` para a casca, como o do desktop. */
const transferencia = (transfer) => emitirNaPonte("seele://event", { TransferChanged: { transfer } });

/**
 * **Um anexo pedido ao servidor**, inteiro na memória, ou o porquê de não.
 * `teto` desiste ao saber o tamanho (a prévia não lê mais que o dela), e
 * `aoAndar(feito, total)` diz o andamento. Resolve `{ bytes }`, `{ grande }`
 * ou `{ naoVeio }`, sempre com a `descricao` que o histórico tem dele (o nome,
 * o tipo alegado, o tamanho); rejeita `"NotConnected"` se a sessão acaba antes.
 */
function receberUmAnexo(sessao, anexo, opcoes = {}) {
  const teto = opcoes.teto ?? Infinity;
  return new Promise((responder, rejeitar) => {
    let descricao = null;
    const resolver = (lido) => responder({ ...lido, descricao });
    const pedacos = [];
    let total = 0;
    let feito = 0;
    sessao.receberOAnexo(anexo, {
      abriu(tamanho, cancelar) {
        total = tamanho;
        if (tamanho > teto) {
          cancelar();
          resolver({ grande: true });
        }
      },
      pedaco(bytes) {
        pedacos.push(bytes);
        feito += bytes.length;
        opcoes.aoAndar?.(feito, total);
      },
      chegou(inteiro) {
        if (!inteiro) return resolver({ naoVeio: true });
        const bytes = new Uint8Array(feito);
        let onde = 0;
        for (const pedaco of pedacos) {
          bytes.set(pedaco, onde);
          onde += pedaco.length;
        }
        resolver({ bytes });
      },
      acabou: () => rejeitar("NotConnected"),
    });
    const pedido = sessao.atender("pedir_anexo", { anexo });
    if (!pedido || "erro" in pedido) rejeitar(pedido?.erro ?? "NotConnected");
    else descricao = pedido.resposta;
  });
}

/** Entrega bytes ao download do navegador, com o nome conferido. */
function baixar(bytes, nome) {
  const endereco = URL.createObjectURL(new Blob([bytes]));
  const ancora = document.createElement("a");
  ancora.href = endereco;
  ancora.download = nome;
  ancora.hidden = true;
  document.body.append(ancora);
  ancora.click();
  ancora.remove();
  setTimeout(() => URL.revokeObjectURL(endereco), 60_000);
}

/** O SHA-256 dos bytes, em hex: o `content_hash` do cabeçalho do envio. */
async function hashDoArquivo(bytes) {
  return hexDe(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)));
}

/**
 * **O envio de um anexo**, depois de a casca ter o número: o fluxo próprio, o
 * cabeçalho que o `.wasm` montou e os bytes em blocos, com o andamento. O fim é
 * o do desktop: `Sent`, `Refused` quando o servidor para o fluxo, `Fell` quando
 * ele cai.
 */
async function subirOAnexo(sessao, id, cabecalho, bytes) {
  let escritor;
  try {
    escritor = (await sessao.abrirUmFluxo()).getWriter();
    await escritor.write(new Uint8Array(cabecalho));
    for (let feito = 0; feito < bytes.length; ) {
      const bloco = bytes.subarray(feito, feito + BLOCO_DO_ANEXO);
      await escritor.write(bloco);
      feito += bloco.length;
      transferencia({ kind: "Sending", client_message_id: id, done: feito, total: bytes.length });
    }
    await escritor.close();
    transferencia({ kind: "Sent", client_message_id: id });
  } catch (falha) {
    const parado = falha && falha.source === "stream";
    transferencia({ kind: parado ? "Refused" : "Fell", client_message_id: id });
  }
}

// ----------------------------------------------------- a prévia de um link

/** O prazo de uma busca de prévia: o do `buscar_com_teto` do desktop. */
const PRAZO_DA_PREVIA_MS = 15_000;

/**
 * **Os bytes de um endereço, até o teto da prévia**: o `buscar_com_teto` do
 * desktop, pelo `fetch` do navegador — sem cookie, sem referência, e com o
 * corpo cortado ao passar do teto. O navegador segue os redirecionamentos e
 * resolve os nomes por conta própria (o desktop confere cada um; aqui o que
 * protege é a regra de rede privada do navegador), e só lê o que o site libera
 * a uma página de fora (CORS): o que ele não libera volta como `NaoRespondeu`.
 */
async function buscarComTeto(endereco) {
  const teto = CONSTANTES_DO_DESKTOP.regras_de_previa.limit;
  let resposta;
  try {
    resposta = await fetch(endereco, {
      credentials: "omit",
      referrerPolicy: "no-referrer",
      cache: "no-store",
      signal: AbortSignal.timeout(PRAZO_DA_PREVIA_MS),
    });
  } catch (falha) {
    return Promise.reject({
      NaoRespondeu: `o site não respondeu, ou não deixa uma página de fora ler o que ele serve (${falha && falha.name ? falha.name : falha})`,
    });
  }
  if (!resposta.ok) return Promise.reject({ NaoRespondeu: `o site respondeu ${resposta.status}` });
  const alegado = minusculasAscii((resposta.headers.get("content-type") ?? "").split(";")[0].trim());
  const pedacos = [];
  let lidos = 0;
  const leitor = resposta.body.getReader();
  for (;;) {
    const { value, done } = await leitor.read();
    if (done) break;
    lidos += value.length;
    if (lidos > teto) {
      leitor.cancel().catch(() => {});
      return Promise.reject("GrandeDemais");
    }
    pedacos.push(value);
  }
  const bytes = new Uint8Array(lidos);
  let onde = 0;
  for (const pedaco of pedacos) {
    bytes.set(pedaco, onde);
    onde += pedaco.length;
  }
  return { alegado, bytes };
}

/** O `data:` de uns bytes que o juiz do desktop desenha, ou `null`. */
async function desenhavel({ alegado, bytes }) {
  const { instancia } = await carregarOWasmDaPagina();
  return fronteiraDaLigacao(instancia.exports).previa({ attachment: 0, claimed: alegado, chegada: "bytes" }, bytes).image;
}

/** Se o endereço é de um domínio de GIF desta máquina, ou de um subdomínio dele. */
function eDeUmDominioDeGif(endereco) {
  let host;
  try {
    host = minusculasAscii(new URL(endereco).hostname);
  } catch {
    return false;
  }
  return CONSTANTES_DO_DESKTOP.dominios_de_gif.some((dominio) => host === dominio || host.endsWith(`.${dominio}`));
}

// ------------------------------------------------------------- as imagens

/**
 * **A escada do ícone do desktop** (`apps/seele-app/src/icone.rs`), JSON
 * estrito: `tests/web.rs` a confere contra o `DEGRAUS` de lá. Cada degrau é o
 * maior lado e os bits por canal de cor; a primeira imagem que cabe no teto do
 * fio (`regras_do_icone_do_server`) vale. E o teto do arquivo de origem, acima
 * do qual não se lê.
 */
const REGRAS_DO_ICONE = Object.freeze({
  "degraus": [[256, 8], [256, 5], [256, 4], [192, 4], [160, 4], [128, 3], [96, 3], [64, 2]],
  "teto_da_origem": 12582912
});

/**
 * **O seletor de imagem, dentro do gesto**: nenhum `await` antes do
 * `click()`, como o `escolher_arquivo`. Desistir resolve `null`.
 */
function escolherUmaImagem() {
  // Os formatos que o `image` do desktop lê (PNG, JPEG, WebP e GIF): o iOS
  // converte a foto HEIC para um deles ao entregar.
  return abrirOSeletor("image/png,image/jpeg,image/webp,image/gif");
}

/**
 * **A imagem escolhida, virada num ícone que cabe**: o `icone::encolher` do
 * desktop, pela escada dele, com o canvas no lugar do `image`. Os bytes não são
 * os do desktop (o PNG do navegador é outro codificador), e a regra é a mesma:
 * menos cores antes de menos pixels, a proporção mantida, nunca ampliar, e o
 * alfa intacto. `null` quando não é imagem, ou quando nem o último degrau cabe.
 */
async function encolherAImagem(arquivo) {
  const { limite_bytes: teto } = CONSTANTES_DO_DESKTOP.regras_do_icone_do_server;
  if (arquivo.size > REGRAS_DO_ICONE.teto_da_origem) return null;
  let imagem;
  try {
    imagem = await createImageBitmap(arquivo);
  } catch {
    return null;
  }
  try {
    for (const [ladoMaximo, bits] of REGRAS_DO_ICONE.degraus) {
      const escala = Math.min(1, ladoMaximo / Math.max(imagem.width, imagem.height));
      const largura = Math.max(1, Math.round(imagem.width * escala));
      const altura = Math.max(1, Math.round(imagem.height * escala));
      const tela = document.createElement("canvas");
      tela.width = largura;
      tela.height = altura;
      const contexto = tela.getContext("2d");
      contexto.imageSmoothingQuality = "high";
      contexto.drawImage(imagem, 0, 0, largura, altura);
      if (bits < 8) achatarAsCores(contexto, largura, altura, bits);
      const png = await new Promise((resolver) => tela.toBlob(resolver, "image/png"));
      if (png && png.size <= teto) return new Uint8Array(await png.arrayBuffer());
    }
    return null;
  } finally {
    imagem.close();
  }
}

/** Cada canal de cor em `bits` níveis, o preto preto e o branco branco: o `achatar_cores` do desktop. */
function achatarAsCores(contexto, largura, altura, bits) {
  const quadro = contexto.getImageData(0, 0, largura, altura);
  const passo = 255 / Math.max(1, 2 ** bits - 1);
  for (let i = 0; i < quadro.data.length; i += 4) {
    for (let canal = 0; canal < 3; canal += 1) {
      quadro.data[i + canal] = Math.min(255, Math.max(0, Math.round(Math.round(quadro.data[i + canal] / passo) * passo)));
    }
  }
  contexto.putImageData(quadro, 0, 0);
}

/** Um `data:` de PNG, como o `meu_retrato` do desktop o devolve. */
function dataUriDoPng(bytes) {
  let binario = "";
  for (const byte of bytes) binario += String.fromCharCode(byte);
  return `data:image/png;base64,${btoa(binario)}`;
}

/** Os bytes de um `data:` em base64, ou `null`. */
function bytesDoDataUri(uri) {
  const virgula = typeof uri === "string" ? uri.indexOf(",") : -1;
  if (virgula < 0) return null;
  try {
    return Uint8Array.from(atob(uri.slice(virgula + 1)), (letra) => letra.charCodeAt(0));
  } catch {
    return null;
  }
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

/**
 * O que o motor da voz conta à ponte. Só o fim da página importa aqui: o áudio
 * fechou (`pagehide`, `freeze`), e o estado da ponte diz que a voz não está
 * ligada, em vez de «aberta». Nada reabre o microfone sozinho: quem volta entra
 * na sala de novo, com um gesto (`ligarOAudioNoGesto`).
 */
function aoEventoDaVoz(evento) {
  // O microfone que só abriu sem o cancelamento de eco (o motor tentou com ele,
  // e a faixa chegou encerrada): a sala diz isso a quem fala, em uma frase.
  if (evento && evento.tipo === "audio-ligado") {
    vozDaSessao.semEco = Boolean(evento.semEcoPorque);
    return;
  }
  if (!evento || evento.tipo !== "audio-fechado") return;
  vozDaSessao.semEco = false;
  // **O microfone que parou em uso** (o navegador ou o sistema encerrou a
  // faixa): dentro da sala, a tela diz SEM ÁUDIO, e não «fora de sala», que não
  // acusa falta de nada. Um gesto (sair e entrar) abre de novo.
  const perdeuNaSala = evento.motivo === "captura-perdida" && vozDaSessao.dentro;
  if (perdeuNaSala) console.warn("ponte: o microfone parou durante a sala (a faixa foi encerrada pelo navegador ou pelo sistema)");
  Object.assign(vozDaSessao, { dentro: false, numero: null, ligando: null, abertura: perdeuNaSala ? "sem_aparelho" : "fora_de_sala", falando: false });
  vozDaSessao.geracao += 1;
  emitirNaPonte("seele://event", "TelemetryChanged");
}

function aVozDaPonte() {
  if (!vozDaPonte.nascendo) {
    vozDaPonte.nascendo = carregarOWasmDaPagina().then(({ bytes, instancia }) => {
      vozDaPonte.atual = criarVozDoNavegador({ wasm: instancia.exports, bytesDoWasm: bytes, aoAcontecer: aoEventoDaVoz });
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
  if (nome === "SeeleCapturaEncerrada") {
    return "O microfone abriu e parou na mesma hora: o sistema não entregou o som ao navegador. Tente de novo; se repetir, reinicie o navegador.";
  }
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

// ------------------------------------------------------------ os MODs da sessão

/**
 * **O armazém dos pacotes desta origem**: o `mod-packages` do desktop, num
 * IndexedDB à parte da identidade, endereçado pelo conteúdo. Cada pacote é
 * `{ hash, id, versao, arquivos: [[caminho, bytes], …] }`, guardado sob o hash
 * que a conferência deu, e lido de novo pelas regras do desktop (o `.wasm`
 * confere o conteúdo contra o hash a cada leitura, como o `ler_por_hash`).
 */
const ARMAZEM_DOS_MODS = Object.freeze({ banco: "seele-mods", versao: 1, pacotes: "pacotes" });

/** O prazo de cada busca ao catálogo: o do `cliente()` do desktop. */
const PRAZO_DO_CATALOGO_MS = 20000;

function abrirOArmazemDosMods() {
  return new Promise((resolver, rejeitar) => {
    const pedido = indexedDB.open(ARMAZEM_DOS_MODS.banco, ARMAZEM_DOS_MODS.versao);
    pedido.onupgradeneeded = () => pedido.result.createObjectStore(ARMAZEM_DOS_MODS.pacotes, { keyPath: "hash" });
    pedido.onsuccess = () => resolver(pedido.result);
    pedido.onerror = () => rejeitar(pedido.error);
  });
}

/** Uma transação no armazém; resolve com o resultado do pedido que `fazer` devolve. */
async function noArmazemDosMods(modo, fazer) {
  const banco = await abrirOArmazemDosMods();
  try {
    return await new Promise((resolver, rejeitar) => {
      const transacao = banco.transaction(ARMAZEM_DOS_MODS.pacotes, modo);
      const pedido = fazer(transacao.objectStore(ARMAZEM_DOS_MODS.pacotes));
      transacao.oncomplete = () => resolver(pedido?.result);
      transacao.onerror = () => rejeitar(transacao.error);
      transacao.onabort = () => rejeitar(transacao.error);
    });
  } finally {
    banco.close();
  }
}

const pacotesGuardados = () => noArmazemDosMods("readonly", (loja) => loja.getAll());
const pacoteGuardado = (hash) => noArmazemDosMods("readonly", (loja) => loja.get(String(hash ?? "")));
const guardarOPacote = (pacote) => noArmazemDosMods("readwrite", (loja) => loja.put(pacote));

/** As funções dos MODs no `.wasm` da página (`seele_web::mods`). */
function fronteiraDosMods(w) {
  const codificador = new TextEncoder();
  const decodificador = new TextDecoder();
  const memoria = () => new Uint8Array(w.memory.buffer);
  const saida = () => decodificador.decode(memoria().slice(w.saida_ptr(), w.saida_ptr() + w.saida_len()));
  const texto = (valor) => codificador.encode(valor);
  /** Põe cada bloco na memória do `.wasm`, chama com os pares, e os solta. */
  function comBlocos(blocos, fazer) {
    const postos = [];
    try {
      for (const bytes of blocos) {
        const p = w.alocar(bytes.length);
        memoria().set(bytes, p);
        postos.push([p, bytes.length]);
      }
      return fazer(...postos.flat());
    } finally {
      for (const [p, n] of postos) w.liberar(p, n);
    }
  }
  /** Entrega os pacotes, um arquivo por vez: o comando lê o último (ou todos). */
  function entregar(pacotes) {
    w.pacotes_limpar();
    for (const { hash, arquivos } of pacotes) {
      comBlocos([texto(hash ?? "")], (p, n) => w.pacote_novo(p, n));
      for (const [caminho, bytes] of arquivos ?? []) {
        comBlocos([texto(caminho), bytes], (cp, cn, bp, bn) => w.pacote_arquivo(cp, cn, bp, bn));
      }
    }
  }
  const lido = (codigo) => (codigo === 0 ? { ok: JSON.parse(saida() || "null") } : { falha: JSON.parse(saida()) });
  const pedir = (exportada, pacote, pedido) => {
    entregar(pacote ? [pacote] : []);
    return comBlocos([texto(JSON.stringify(pedido))], (p, n) => exportada(p, n));
  };
  return {
    naTela(pacotes) {
      entregar(pacotes);
      w.mods_na_tela();
      return JSON.parse(saida());
    },
    codigo: (pacote, pedido) => lido(pedir(w.mod_codigo, pacote, pedido)),
    midia: (pacote, pedido) => lido(pedir(w.mod_midia, pacote, pedido)),
    /** Os bytes crus do som, como o `tauri::ipc::Response` do desktop. */
    som(pacote, pedido) {
      if (pedir(w.mod_som, pacote, pedido) !== 0) return { falha: JSON.parse(saida()) };
      return { ok: memoria().slice(w.saida_ptr(), w.saida_ptr() + w.saida_len()) };
    },
    midiaDoServidor: (base64, uso) =>
      lido(comBlocos([texto(base64), texto(uso)], (tp, tn, up, un) => w.mod_midia_do_servidor(tp, tn, up, un))),
    endereco: () => (w.catalogo_endereco(), saida()),
    lerOCatalogo: (catalogo) => lido(comBlocos([catalogo.bytes, texto(catalogo.assinatura)], (...pares) => w.catalogo_ler(...pares))),
    planejar(catalogo, revogacoes, pedido) {
      const blocos = [catalogo.bytes, texto(catalogo.assinatura), revogacoes.bytes, texto(revogacoes.assinatura), texto(JSON.stringify(pedido))];
      return lido(comBlocos(blocos, (...pares) => w.catalogo_planejar(...pares)));
    },
    conferir(arquivos, hash) {
      entregar([{ hash: "", arquivos }]);
      return lido(comBlocos([texto(hash)], (p, n) => w.catalogo_conferir(p, n)));
    },
    // O seletor do MOD e os escolhidos: as regras e o guarda moram no `.wasm`
    // (`seele_sessao::escolhidos`), e os bytes, na página.
    regrasDoSeletor: (pedido) => lido(comBlocos([texto(JSON.stringify(pedido))], (p, n) => w.escolha_regras(p, n))).ok,
    guardarOEscolhido: (pedido, comeco) =>
      lido(comBlocos([texto(JSON.stringify(pedido)), comeco], (pp, pn, cp, cn) => w.escolha_guardar(pp, pn, cp, cn))),
    pedacoDoEscolhido: (pedido) => lido(comBlocos([texto(JSON.stringify(pedido))], (p, n) => w.escolha_pedaco(p, n))),
    soltarOEscolhido: (pedido) => lido(comBlocos([texto(JSON.stringify(pedido))], (p, n) => w.escolha_soltar(p, n))).ok,
    tirarOEscolhido: (pedido) => lido(comBlocos([texto(JSON.stringify(pedido))], (p, n) => w.escolha_tirar(p, n))),
    soltarAGeracao: (geracao) =>
      lido(comBlocos([texto(JSON.stringify({ geracao }))], (p, n) => w.escolhas_soltar_geracao(p, n))).ok,
    base64De: (bytes) => (comBlocos([bytes], (p, n) => w.base64_de(p, n)), saida()),
    // As imagens pelo volume: o pedido, a resposta aos pedaços, e o prazo.
    pedidoDeImagem(pedido) {
      if (comBlocos([texto(JSON.stringify(pedido))], (p, n) => w.volume_pedido(p, n)) !== 0) return { falha: JSON.parse(saida()) };
      return { ok: memoria().slice(w.saida_ptr(), w.saida_ptr() + w.saida_len()) };
    },
    prazoDaImagem: () => (w.volume_prazo(), JSON.parse(saida())),
    respostaNova: () => w.volume_resposta_nova(),
    respostaChegou: (alca, pedaco) => comBlocos([pedaco], (p, n) => w.volume_resposta_chegou(alca, p, n)),
    respostaAcabou: (alca, { imagem, viva, uso }) =>
      lido(comBlocos([texto(uso ?? "")], (p, n) => w.volume_resposta_acabou(alca, imagem ? 1 : 0, viva ? 1 : 0, p, n))),
  };
}

/** Os bytes dos arquivos escolhidos para os MODs, pelo número que o `.wasm` deu. */
const BYTES_DOS_ESCOLHIDOS = new Map();

/** A sessão `geracao` acabou: os escolhidos dela saem, e os bytes com eles. */
async function soltarOsEscolhidosDa(geracao) {
  for (const numero of (await osModsDaPagina()).soltarAGeracao(geracao) ?? []) BYTES_DOS_ESCOLHIDOS.delete(numero);
}

/** A geração `geracao` é a da sessão de pé (o `recusar_se_morta`, sem lançar)? */
function geracaoViva(geracao) {
  const sessao = sessaoDaPonte.atual;
  return Boolean(sessao && !sessao.acabou && geracao && sessao.geracao === geracao);
}

/** O bloco em que os bytes de uma imagem sobem: o `BLOCK_LEN` dos anexos. */
const BLOCO_DA_IMAGEM = 64 * 1024;

/**
 * **Uma imagem de MOD pelo volume**: o fluxo bidirecional próprio, como o
 * `imagem_de_mod` do núcleo. Sobe o pedido (e os bytes, num envio), fecha a
 * ida, lê a resposta inteira pelo `.wasm`, e a entrega a `acabar`, que diz o
 * que ela é. No prazo do desktop; a falha do transporte vem em texto, como o
 * desktop diz o erro dele ao MOD.
 *
 * **A resposta é lida mesmo quando a ida falha.** O servidor que recusa
 * responde e para de ler o que chega (`STOP_SENDING`), e o `close()` da página
 * rejeita por isso; o desktop fecha a ida sem esperar e lê a recusa. Aqui, o
 * erro da ida só vale quando a resposta não vem.
 */
async function transferirAImagem(sessao, mods, pedido, corpo, acabar) {
  const prazo = mods.prazoDaImagem();
  const alca = mods.respostaNova();
  const comoTexto = (falha) => (typeof falha === "string" ? falha : String(falha?.message ?? falha));
  let daIda = null;
  const transferir = async () => {
    const fluxo = await sessao.abrirUmFluxoDeImagem();
    const escritor = fluxo.writable.getWriter();
    try {
      await escritor.write(pedido);
      for (let inicio = 0; corpo && inicio < corpo.length; inicio += BLOCO_DA_IMAGEM) {
        await escritor.write(corpo.subarray(inicio, inicio + BLOCO_DA_IMAGEM));
      }
      await escritor.close();
    } catch (falha) {
      daIda = comoTexto(falha);
    }
    const leitor = fluxo.readable.getReader();
    for (;;) {
      const { value, done } = await leitor.read();
      if (done) break;
      mods.respostaChegou(alca, value);
    }
  };
  try {
    await comPrazo(transferir(), prazo.ms, prazo.motivo);
  } catch (falha) {
    // A resposta que não acabou é esquecida pelo `.wasm` do mesmo jeito.
    mods.respostaAcabou(alca, { imagem: false, viva: false });
    throw { Recusado: { motivo: daIda ?? comoTexto(falha) } };
  }
  const feito = acabar(alca);
  if (daIda && feito.falha?.Recusado?.motivo === "incompleto") return { falha: { Recusado: { motivo: daIda } } };
  return feito;
}

/**
 * **O seletor do navegador**, com o filtro `accept`: o arquivo, ou `null`
 * quando a pessoa desiste. Um por chamada, e tirado da página ao responder.
 */
function abrirOSeletor(accept = "") {
  return new Promise((responder) => {
    const entrada = document.createElement("input");
    entrada.type = "file";
    if (accept) entrada.accept = accept;
    entrada.hidden = true;
    let respondido = false;
    const fim = (valor) => {
      if (respondido) return;
      respondido = true;
      entrada.remove();
      responder(valor);
    };
    entrada.addEventListener("change", () => fim(entrada.files?.[0] ?? null));
    entrada.addEventListener("cancel", () => fim(null));
    document.body.append(entrada);
    entrada.click();
  });
}

/** Quanto do começo de um arquivo escolhido prova o tipo dele (os formatos abrem nos primeiros bytes). */
const COMECO_DO_ESCOLHIDO = 4096;

/** O pacote guardado sob `hash`, ou `null` (dito no console) sem armazém. */
async function pacoteDoArmazem(hash) {
  try {
    return await pacoteGuardado(hash);
  } catch (falha) {
    console.warn("ponte: o armazém dos MODs não abriu:", falha);
    return null;
  }
}

/** Os MODs no `.wasm` desta página. */
async function osModsDaPagina() {
  return fronteiraDosMods((await carregarOWasmDaPagina()).instancia.exports);
}

/**
 * **A geração de um comando de MOD** (`recusar_se_morta` do desktop): zero, ou
 * a de uma sessão que acabou, é recusada com `sessao-encerrada`.
 */
function recusarSeMorta(geracao) {
  const sessao = sessaoDaPonte.atual;
  if (!sessao || sessao.acabou || !geracao || sessao.geracao !== geracao) {
    throw { Recusado: { motivo: "sessao-encerrada" } };
  }
  return sessao;
}

/** Uma busca ao catálogo, com o prazo do desktop: os bytes, ou o texto. */
async function buscarNoCatalogo(url, comoTexto) {
  try {
    const resposta = await fetch(url, {
      signal: AbortSignal.timeout(PRAZO_DO_CATALOGO_MS),
      credentials: "omit",
      referrerPolicy: "no-referrer",
    });
    if (!resposta.ok) throw new Error(`${url}: ${resposta.status}`);
    return comoTexto ? await resposta.text() : new Uint8Array(await resposta.arrayBuffer());
  } catch (falha) {
    throw { NaoRespondeu: String(falha?.message ?? falha) };
  }
}

/** Um arquivo do catálogo e a assinatura dele: um sem o outro não serve. */
async function buscarAssinado(endereco, nome) {
  const url = `${endereco}/${nome}`;
  const bytes = await buscarNoCatalogo(url, false);
  return { bytes, assinatura: await buscarNoCatalogo(`${url}.minisig`, true) };
}

/**
 * **O `instalar_do_catalogo` do desktop**, na mesma ordem: o catálogo
 * conferido, as revogações conferidas, o plano, os bytes baixados, **o
 * conjunto conferido**, e só então o armazém. Rejeita com a `FalhaNoCatalogo`
 * do desktop; resolve com o hash do que ficou guardado.
 */
async function instalarDoCatalogoNaPagina(id, versao) {
  const mods = await osModsDaPagina();
  const endereco = mods.endereco();
  const catalogo = await buscarAssinado(endereco, "catalogo.json");
  const lido = mods.lerOCatalogo(catalogo);
  if (lido.falha) throw lido.falha;
  // As revogações que não respondem **não** viram lista vazia: seguir sem
  // elas é instalar sem saber se aquela versão foi retirada.
  const revogacoes = await buscarAssinado(endereco, "revogacoes.json");
  const plano = mods.planejar(catalogo, revogacoes, { id, versao });
  if (plano.falha) throw plano.falha;
  const arquivos = [];
  for (const [caminho, url] of plano.ok.arquivos) arquivos.push([caminho, await buscarNoCatalogo(url, false)]);
  const conferido = mods.conferir(arquivos, plano.ok.hash);
  if (conferido.falha) throw conferido.falha;
  try {
    await guardarOPacote({ hash: conferido.ok.hash, id: conferido.ok.id, versao, arquivos });
  } catch (falha) {
    // A falha é desta página, e não do catálogo nem da rede.
    throw { NaoInstalou: { NaoCopiei: String(falha?.message ?? falha) } };
  }
  return conferido.ok.hash;
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
  limites_dos_campos: () => structuredClone(CONSTANTES_DO_DESKTOP.limites_dos_campos),
  regras_de_previa: () => structuredClone(CONSTANTES_DO_DESKTOP.regras_de_previa),
  regras_do_icone_do_server: () => structuredClone(CONSTANTES_DO_DESKTOP.regras_do_icone_do_server),
  apis_de_mod_aceitas: () => structuredClone(CONSTANTES_DO_DESKTOP.apis_de_mod_aceitas),
  dominios_de_gif: () => structuredClone(CONSTANTES_DO_DESKTOP.dominios_de_gif),
  controles_da_voz: controlesDaVozGuardados,
  // Sem sessão, o mapa vazio, como o `unwrap_or_default` do desktop.
  volumes_locais: () => (sessaoDaPonte.atual ? Promise.resolve(pelaSessao("volumes_locais")({})).catch(() => ({})) : {}),
  snapshot: () => comASessao((sessao) => sessao.snapshot()),
  disconnect: () => sairDaSessaoDaPonte(),
  // **As várias sessões da página** (E3; fronteiras §6), o mesmo vocabulário
  // do app.
  sessoes: () => sessoesNaPonte(),
  ver_sessao: ({ sessao, epoca } = {}) => {
    const aberta = sessoesDaPonte.get(sessao);
    if (!aberta) return Promise.reject("NotConnected");
    // **A época da troca** (base.js): um pedido mais velho que o último
    // aplicado não muda a vista daqui, nem que chegue depois.
    if (typeof epoca === "number") {
      if (epoca < epocaAplicadaNaPonte) return Promise.reject({ VistaSuperada: { sessao, epoca } });
      epocaAplicadaNaPonte = epoca;
    }
    vistaDaPonte = sessao;
    aberta.naoLidas = 0;
    aberta.mudou = false;
    // A época da vista sobe: os MODs da anterior param, como numa saída.
    geracaoDaPonte += 1;
    aberta.geracao = geracaoDaPonte;
    avisarAsSessoes(sessao, "vista");
    return {
      carimbo: carimboDaPonte(aberta), vista: geracaoDaPonte, geracao: geracaoDaPonte,
      snapshot: aberta.acabou ? null : retratoDaSessao(aberta), veredito: "Known",
    };
  },
  encerrar_sessao: ({ carimbo } = {}) => {
    const aberta = sessoesDaPonte.get(carimbo?.sessao);
    if (!aberta) return null;
    sessaoDoPedido = aberta;
    try {
      return sairDaSessaoDaPonte();
    } finally {
      sessaoDoPedido = null;
    }
  },
  focar_voz: ({ carimbo } = {}) => focarAVozNaPonte(carimbo?.sessao ?? 0),
  // **A sessão pelo navegador** (2.6): o `connect` pela WebTransport, e os
  // comandos da sessão pelo `.wasm`, que faz o que a FFI faz em cada um.
  connect: abrirASessaoPelaPonte,
  soltar_a_entrada: ({ geracao } = {}) => {
    const sessao = [...sessoesDaPonte.values()].find((s) => s.geracao === geracao);
    if (!sessao || sessao.acabou || geracao == null || sessao.geracao !== geracao) return false;
    sessao.abortar?.();
    return sairDaSessaoDaPonte(sessao).then(() => true);
  },
  // **A entrada pela casca larga** (D-web-3): o SERVIDORES e o link colado.
  // Um link do produto é convite; o resto, endereço, que o `connect` só aceita
  // de um servidor deste aparelho.
  ler_entrada: async ({ texto } = {}) => {
    const escrito = String(texto ?? "").trim();
    const lido = await lerOLink(escrito);
    return lido.erro === "OutraOrigem" ? { Endereco: escrito } : { Convite: escrito };
  },
  analisar_convite: async ({ link } = {}) => {
    const texto = String(link ?? "").trim();
    const lido = await lerOLink(texto);
    if (lido.erro) {
      // O erro chega como nome (`"OutraOrigem"`) ou como objeto com campo
      // (`{ OutraLinha: { versao } }`): a frase é a do nome, nos dois.
      const nome = typeof lido.erro === "object" && lido.erro ? Object.keys(lido.erro)[0] : lido.erro;
      const frase = FRASES_DO_LINK[nome] ?? FRASES_DO_LINK.Convite;
      return Promise.reject({ RecusaDaPonte: { comando: "analisar_convite", frase } });
    }
    linkColadoDaPonte = { texto, alvo: lido.link.alvo, impressao: lido.link.impressao };
    // A página fala só a linha dela: o link do navegador entra aqui mesmo.
    return { alvo: lido.link.alvo, token: lido.link.convite ?? null, versao: lido.link.versao ?? null, para_entrar: "Aqui" };
  },
  cancelar_ligacao: () => {
    const cancelar = entradaEmCurso.cancelar;
    if (!cancelar) return false;
    cancelar();
    return true;
  },
  // Os SERVIDORES DESTE APARELHO da identidade, na forma do `Conhecido` do
  // desktop. Sem identidade, a lista vazia, como o desktop sem o arquivo.
  conhecidos: async () => {
    try {
      const identidade = await identidadeDaPagina();
      return (await identidade.conhecidos()).map((servidor) => ({
        alvo: servidor.endereco,
        apelido: servidor.apelido ?? "",
        voice_room: null,
        visto_em: servidor.visto ? Math.floor(servidor.visto / 1000) : 0,
        nome: servidor.nome ?? null,
        ...(servidor.icone ? { icone: servidor.icone } : {}),
        caminhos: [],
        bilhete: null,
        impressao: servidor.impressao,
        caminho_bps: null,
      }));
    } catch (falha) {
      console.warn("ponte: os servidores deste aparelho não abriram:", falha);
      return [];
    }
  },
  esquecer: async ({ alvo } = {}) => {
    const identidade = await identidadeDaPagina();
    for (const servidor of await identidade.conhecidos()) {
      if (servidor.endereco === alvo) await identidade.esquecerOServidor(servidor.impressao);
    }
    return null;
  },
  // Os aceites de MODs são guardados pela impressão do servidor (a chave da
  // identidade); a casca do desktop pergunta pelo endereço.
  aceite_de_mods: async ({ alvo } = {}) => {
    const impressao = await impressaoDoEndereco(alvo);
    return impressao ? aceiteDeModsGuardado(impressao) : null;
  },
  aceitar_mods: async ({ alvo, conjunto } = {}) => {
    const impressao = await impressaoDoEndereco(alvo);
    if (!impressao) return Promise.reject({ Recusado: { motivo: "EnderecoInvalido" } });
    return guardarOAceiteDeMods(impressao, String(conjunto ?? "")) ? null : Promise.reject({ Recusado: { motivo: "NaoDeuParaGravar" } });
  },
  esquecer_aceite_de_mods: async ({ alvo } = {}) => {
    const impressao = await impressaoDoEndereco(alvo);
    if (!impressao) return Promise.reject({ Recusado: { motivo: "EnderecoInvalido" } });
    return guardarOAceiteDeMods(impressao, null) ? null : Promise.reject({ Recusado: { motivo: "NaoDeuParaGravar" } });
  },
  aceites_de_mods: async () => {
    let servidores = [];
    try {
      servidores = await (await identidadeDaPagina()).conhecidos();
    } catch {
      servidores = [];
    }
    return aceitesDeModsGuardados().map((aceite) => ({
      alvo: servidores.find((servidor) => servidor.impressao === aceite.impressao)?.endereco ?? aceite.impressao,
      conjunto: aceite.conjunto,
      quando: aceite.quando,
    }));
  },
  // O nome e o ícone do servidor, anotados com ele neste aparelho; calado em
  // toda falha, como no desktop.
  lembrar_aparencia_do_servidor: async () => {
    const sessao = sessaoDaPonte.atual;
    if (!sessao || sessao.acabou) return null;
    try {
      const nome = sessao.snapshot().server;
      const icone = sessao.atender("icone_do_server", {})?.resposta ?? null;
      await (await identidadeDaPagina()).lembrarAAparencia(sessao.alvo.impressao, { nome, icone });
    } catch (falha) {
      console.warn("ponte: a aparência do servidor não ficou guardada:", falha);
    }
    return null;
  },
  // **A prévia de um link** (ADR 0053), com as regras do desktop: o endereço
  // que pode sair, o destino que não é reservado, o consentimento do domínio, o
  // juiz das imagens, e a mídia que uma página de GIF declara.
  previa_de_link: async ({ url } = {}) => {
    const { alvo, recusa } = enderecoQuePodeSair(String(url ?? ""));
    if (recusa) return Promise.reject({ NaoEAberto: recusa });
    let host;
    try {
      host = new URL(alvo).hostname;
    } catch (falha) {
      return Promise.reject({ NaoEAberto: String(falha && falha.message ? falha.message : falha) });
    }
    if (!(await pelaPagina("destino_de_previa")({ host }))) return Promise.reject({ DestinoPrivado: minusculasAscii(host) });
    const dominio = minusculasAscii(host);
    if (!previaConsentida(previasConsentidas(), dominio)) return Promise.reject({ SemConsentimento: { dominio } });
    const primeira = await buscarComTeto(alvo);
    const imagem = await desenhavel(primeira);
    if (imagem) return imagem;
    if (eDeUmDominioDeGif(alvo) && primeira.alegado.startsWith("text/html")) {
      const midia = await pelaPagina("midia_da_pagina")({ html: new TextDecoder().decode(primeira.bytes) });
      if (!midia || !eDeUmDominioDeGif(midia)) return Promise.reject("NaoEImagem");
      const daMidia = await desenhavel(await buscarComTeto(midia));
      return daMidia ?? Promise.reject("NaoEImagem");
    }
    return Promise.reject("NaoEImagem");
  },
  // **Os anexos**: pedidos ao servidor e lidos pelo `.wasm` (o hash
  // conferido lá), julgados pelo mesmo juiz do desktop na prévia, e salvos pelo
  // download do navegador com o nome conferido pelas regras do desktop.
  prever_anexo: ({ anexo } = {}) =>
    comASessao(async (sessao) => {
      const lido = await receberUmAnexo(sessao, anexo, { teto: CONSTANTES_DO_DESKTOP.regras_de_previa.limit });
      const chegada = lido.bytes ? "bytes" : lido.grande ? "grande_demais" : "nao_veio";
      return sessao.previa({ attachment: anexo, claimed: lido.descricao?.tipo ?? "", chegada }, lido.bytes);
    }),
  destino_do_anexo: async ({ anexo } = {}) => {
    const sessao = sessaoDaPonte.atual;
    if (!sessao) return Promise.reject({ reason: "AnexoDesconhecido", claimed: "", folder: PASTA_DO_NAVEGADOR });
    return pelaSessao("destino_do_anexo")({ anexo, pasta: PASTA_DO_NAVEGADOR });
  },
  // Responde já, como o desktop; o andamento e o fim vêm como
  // `TransferChanged`, e o arquivo vai ao download do navegador.
  salvar_anexo: ({ anexo } = {}) =>
    comASessao(async (sessao) => {
      const destino = sessao.atender("destino_do_anexo", { anexo, pasta: PASTA_DO_NAVEGADOR });
      if (!destino || "erro" in destino) {
        transferencia({ kind: "NotSaved", attachment: anexo, reason: destino?.erro?.reason ?? "Falhou" });
        return null;
      }
      const { file_name: nome } = destino.resposta;
      receberUmAnexo(sessao, anexo, {
        aoAndar: (feito, total) => transferencia({ kind: "Receiving", attachment: anexo, done: feito, total }),
      }).then(
        (lido) => {
          if (!lido.bytes) return transferencia({ kind: "NotSaved", attachment: anexo, reason: "Falhou" });
          baixar(lido.bytes, nome);
          transferencia({ kind: "Saved", attachment: anexo, path: nome });
        },
        () => transferencia({ kind: "NotSaved", attachment: anexo, reason: "Falhou" }),
      );
      return null;
    }),
  // O número volta quando o cabeçalho está pronto (o hash pede os bytes); a
  // subida segue depois, como no desktop. O arquivo que não está mais aqui cai
  // com o número, como o caminho que sumiu do desktop.
  enviar_anexo: ({ channel, body, caminho, nome, tipo } = {}) =>
    comASessao(async (sessao) => {
      const guardado = ALCAS_DA_PONTE.get(String(caminho ?? ""));
      const bytes = guardado ? new Uint8Array(await guardado.arquivo.arrayBuffer()) : new Uint8Array(0);
      const hash = guardado ? await hashDoArquivo(bytes) : "0".repeat(64);
      const feito = await pelaSessao("cabecalho_do_anexo")({
        channel,
        body: String(body ?? ""),
        nome: String(nome ?? ""),
        tipo: String(tipo ?? ""),
        tamanho: bytes.length,
        hash,
      });
      if (feito.recusa) {
        console.warn(`ponte: o anexo ${feito.id} não sobe: ${feito.recusa}`);
        setTimeout(() => transferencia({ kind: "Fell", client_message_id: feito.id }), 0);
        return feito.id;
      }
      setTimeout(() => subirOAnexo(sessao, feito.id, feito.cabecalho, bytes), 0);
      return feito.id;
    }),
  // **As imagens**: o seletor dentro do gesto, a escada do desktop, e o fio
  // pelo `.wasm` (que confere o PNG como o `check_server_icon`).
  preparar_icone_do_server: () =>
    escolherUmaImagem().then(async (arquivo) => {
      if (!arquivo) return null;
      const png = await encolherAImagem(arquivo);
      return png ? Array.from(png) : Promise.reject("IconNotAPicture");
    }),
  // O seletor abre antes de a sessão ser conferida, como no desktop.
  escolher_icone_do_server: () =>
    escolherUmaImagem().then(async (arquivo) => {
      if (!arquivo) return false;
      const png = await encolherAImagem(arquivo);
      if (!png) return Promise.reject("IconNotAPicture");
      await pelaSessao("aplicar_icone_do_server")({ icone: Array.from(png) });
      return true;
    }),
  // Guardada nesta origem, e subida agora se há sessão; sem sessão, sobe na
  // próxima entrada, como no desktop.
  escolher_minha_imagem: () =>
    escolherUmaImagem().then(async (arquivo) => {
      if (!arquivo) return false;
      const png = await encolherAImagem(arquivo);
      if (!png || !gravarNaOrigem(CHAVES_DA_ORIGEM.retrato, dataUriDoPng(png))) return Promise.reject("IconNotAPicture");
      if (sessaoDaPonte.atual) await pelaSessao("minha_imagem")({ icone: Array.from(png) });
      return true;
    }),
  tirar_minha_imagem: async () => {
    if (!gravarNaOrigem(CHAVES_DA_ORIGEM.retrato, null)) return Promise.reject("IconNotAPicture");
    if (sessaoDaPonte.atual) await pelaSessao("minha_imagem")({ icone: null });
    return null;
  },
  estado_da_sessao: () => ({
    geracao: sessaoDaPonte.atual?.geracao ?? 0,
    eventos_descartados: 0,
    comandos_recusados: sessaoDaPonte.atual?.contas.naoSairam ?? 0,
    falas_descartadas: 0,
    mods_nativos: [],
  }),
  messages: pelaSessao("messages"),
  open_channel: pelaSessao("open_channel"),
  send_message: pelaSessao("send_message"),
  reenviar_mensagem: pelaSessao("reenviar_mensagem"),
  descartar_mensagem: pelaSessao("descartar_mensagem"),
  remover_mensagem: pelaSessao("remover_mensagem"),
  dispensar_aviso: pelaSessao("dispensar_aviso"),
  escolher_apelido: pelaSessao("escolher_apelido"),
  membros_fora_do_ar: pelaSessao("membros_fora_do_ar"),
  icone_do_server: pelaSessao("icone_do_server"),
  // Sem sessão, `null`: o desktop não erra aqui.
  imagem_da_pessoa: (args) => (sessaoDaPonte.atual ? pelaSessao("imagem_da_pessoa")(args).catch(() => null) : null),
  // A geração que a casca guardou do `connect`: zero ou velha é a de uma
  // sessão que acabou (`confere_geracao`).
  // **Os MODs da sessão**: os pacotes no armazém da página, lidos pelas
  // regras do desktop no `.wasm`.
  mods_instalados: async () => {
    let pacotes;
    try {
      pacotes = await pacotesGuardados();
    } catch (falha) {
      // Sem armazém, a lista vazia, e dito: um MOD que some da lista sem
      // motivo é a pergunta que volta dias depois.
      console.warn("ponte: o armazém dos MODs não abriu:", falha);
      pacotes = [];
    }
    return (await osModsDaPagina()).naTela(pacotes);
  },
  codigo_do_mod: async ({ geracao, id, hash } = {}) => {
    recusarSeMorta(geracao);
    const lido = (await osModsDaPagina()).codigo(await pacoteDoArmazem(hash), { id, hash });
    if (lido.falha) throw lido.falha;
    return lido.ok;
  },
  // A mídia declarada do pacote: o som volta sem `data:`, e os bytes dele
  // pelo `som_do_mod`.
  midia_do_mod: async ({ geracao, id, hash, caminho } = {}) => {
    recusarSeMorta(geracao);
    const lido = (await osModsDaPagina()).midia(await pacoteDoArmazem(hash), { id, hash, caminho });
    if (lido.falha) throw lido.falha;
    return lido.ok;
  },
  som_do_mod: async ({ geracao, id, hash, caminho } = {}) => {
    recusarSeMorta(geracao);
    const lido = (await osModsDaPagina()).som(await pacoteDoArmazem(hash), { id, hash, caminho });
    if (lido.falha) throw lido.falha;
    return lido.ok;
  },
  // **O seletor do MOD**: a pessoa escolhe, e o MOD recebe um número e lê os
  // bytes em pedaços. As regras e o guarda moram no `.wasm`; os bytes, aqui.
  escolher_para_o_mod: async ({ geracao, id, finalidade, papeis, extensoes, limiteDeBytes } = {}) => {
    recusarSeMorta(geracao);
    const mods = await osModsDaPagina();
    const regras = mods.regrasDoSeletor({ finalidade, extensoes, limiteDeBytes });
    const arquivo = await abrirOSeletor(regras.extensoes.map((extensao) => `.${extensao}`).join(","));
    if (!arquivo) return null;
    if (arquivo.size > regras.teto) throw { Recusado: { motivo: "arquivo-grande-demais" } };
    let bytes;
    try {
      bytes = new Uint8Array(await arquivo.arrayBuffer());
    } catch {
      throw { Recusado: { motivo: "arquivo-ilegivel" } };
    }
    const guardado = mods.guardarOEscolhido(
      { id, geracao, papeis, tamanho: bytes.length, viva: geracaoViva(geracao) },
      bytes.subarray(0, COMECO_DO_ESCOLHIDO),
    );
    if (guardado.falha) throw guardado.falha;
    BYTES_DOS_ESCOLHIDOS.set(guardado.ok.id, bytes);
    return guardado.ok;
  },
  pedaco_do_escolhido: async ({ geracao, id, arquivo, inicio } = {}) => {
    recusarSeMorta(geracao);
    const mods = await osModsDaPagina();
    const faixa = mods.pedacoDoEscolhido({ geracao, id, arquivo, inicio });
    if (faixa.falha) throw faixa.falha;
    return mods.base64De(BYTES_DOS_ESCOLHIDOS.get(arquivo).subarray(faixa.ok.inicio, faixa.ok.fim));
  },
  // Sem conferir a geração, como o desktop: soltar o que não é de quem pede
  // não faz nada, e não é erro.
  soltar_escolhido: async ({ geracao, id, arquivo } = {}) => {
    if ((await osModsDaPagina()).soltarOEscolhido({ geracao, id, arquivo })?.soltou) BYTES_DOS_ESCOLHIDOS.delete(arquivo);
    return null;
  },
  // O escolhido sobe ao volume do MOD, e é consumido antes, dê o envio certo
  // ou não.
  enviar_imagem_mod: async ({ geracao, id, arquivo, token } = {}) => {
    recusarSeMorta(geracao);
    const mods = await osModsDaPagina();
    const tirado = mods.tirarOEscolhido({ geracao, id, arquivo });
    if (tirado.falha) throw tirado.falha;
    const bytes = BYTES_DOS_ESCOLHIDOS.get(arquivo);
    BYTES_DOS_ESCOLHIDOS.delete(arquivo);
    const sessao = sessaoDaPonte.atual;
    if (!sessao || !bytes) throw { Recusado: { motivo: "sessao-encerrada" } };
    const pedido = mods.pedidoDeImagem({ enviar: { id, token, bytes: bytes.length } });
    if (pedido.falha) throw pedido.falha;
    const feito = await transferirAImagem(sessao, mods, pedido.ok, bytes, (alca) =>
      mods.respostaAcabou(alca, { imagem: false, viva: geracaoViva(geracao) }),
    );
    if (feito.falha) throw feito.falha;
    return null;
  },
  // A imagem que o MOD guardou no servidor, lida pelo volume depois de ele
  // autorizar, e pronta pelo caminho comum (reduzida ao uso).
  ler_imagem_mod: async ({ geracao, id, channel, payload, uso } = {}) => {
    recusarSeMorta(geracao);
    const sessao = sessaoDaPonte.atual;
    if (!sessao) throw { Recusado: { motivo: "sessao-encerrada" } };
    const mods = await osModsDaPagina();
    const pedido = mods.pedidoDeImagem({ ler: { id, channel, payload: String(payload ?? "") } });
    if (pedido.falha) throw pedido.falha;
    const lida = await transferirAImagem(sessao, mods, pedido.ok, null, (alca) =>
      mods.respostaAcabou(alca, { imagem: true, viva: geracaoViva(geracao), uso: String(uso ?? "") }),
    );
    if (lida.falha) throw lida.falha;
    return lida.ok;
  },
  // A mídia que o MOD recebeu do servidor, reduzida ao uso que a janela nomeia.
  midia_em_bytes: async ({ geracao, base64, uso } = {}) => {
    recusarSeMorta(geracao);
    const lido = (await osModsDaPagina()).midiaDoServidor(String(base64 ?? ""), String(uso ?? ""));
    if (lido.falha) throw lido.falha;
    return lido.ok;
  },
  // `false`: o navegador não hospeda, e não há servidor daqui a reaplicar.
  instalar_mod_do_catalogo: async ({ id, versao } = {}) => {
    await instalarDoCatalogoNaPagina(String(id ?? ""), String(versao ?? ""));
    return false;
  },
  mod_request: ({ geracao, ...pedido } = {}) => {
    const sessao = sessaoDaPonte.atual;
    if (!sessao || !geracao || sessao.geracao !== geracao) return Promise.reject("NotConnected");
    return pelaSessao("mod_request")(pedido);
  },
  apagar_linha: pelaSessao("apagar_linha"),
  apagar_voice_room: pelaSessao("apagar_voice_room"),
  aplicar_icone_do_server: pelaSessao("aplicar_icone_do_server"),
  banir_pessoa: pelaSessao("banir_pessoa"),
  criar_linha: pelaSessao("criar_linha"),
  criar_voice_room: pelaSessao("criar_voice_room"),
  expulsar_pessoa: pelaSessao("expulsar_pessoa"),
  mover_pessoa: pelaSessao("mover_pessoa"),
  renomear_linha: pelaSessao("renomear_linha"),
  renomear_server: pelaSessao("renomear_server"),
  renomear_voice_room: pelaSessao("renomear_voice_room"),
  tirar_icone_do_server: pelaSessao("tirar_icone_do_server"),
  // O único que espera o servidor: o `ChannelWeighed` de volta. Uma sessão que
  // acaba antes responde `NotConnected`, como o oneshot caído do desktop.
  peso_da_linha: (args = {}) =>
    comASessao(async (sessao) => {
      const peso = sessao.esperarOPeso(args.channel);
      await pelaSessao("peso_da_linha")(args);
      const lido = await peso;
      if (!lido) return Promise.reject("NotConnected");
      return { channel: lido.channel, messages: lido.messages, authors: lido.authors, oldest_at_seconds: lido.oldest_at_seconds };
    }),
  assistir: pelaSessao("assistir"),
  pedir_quadro_chave: pelaSessao("pedir_quadro_chave"),
  // **A sala, e o áudio dentro do gesto**: o `ligar` vem antes de qualquer
  // espera, e a voz abre quando o servidor confirma a entrada.
  // **Entrar numa sala é o pedido do foco** (fronteiras §5: «um ato, não
  // dois»): a sessão que tinha a voz sai da sala dela e a voz sai com ela
  // antes de esta entrar. A sessão é pega antes de qualquer espera.
  enter_voice_room: (args) =>
    comASessao(async (sessao) => {
      const pedido = ++vozDaSessao.pedido;
      // O que este pedido abriu (o microfone do gesto) é dele até a entrada ser confirmada: qualquer saída por erro, ANTES ou DEPOIS do `await` da
      // entrada (o foco recusado, a entrada recusada), solta a captura, e só se nenhum pedido mais novo a tomou para si.
      try {
        ligarOAudioNoGesto();
        if (sessao.idDaPonte && focoDaPonte !== sessao.idDaPonte) await focarAVozNaPonte(sessao.idDaPonte, true);
        const resposta = await pelaSessaoDe(sessao, "enter_voice_room")(args);
        if (focoDaPonte === sessao.idDaPonte || !sessao.idDaPonte) conferirAVozDaSessao(sessao);
        return resposta;
      } catch (falha) {
        if (pedido === vozDaSessao.pedido && !vozDaSessao.dentro) soltarOMicrofoneDaVoz();
        throw falha;
      }
    }),
  leave_voice_room: (args) =>
    comASessao((sessao) => {
      const pedido = ++vozDaSessao.pedido;
      // **O SAIR revoga a captura NO GESTO**, antes de esperar a resposta do servidor: a faixa para quando a pessoa toca, e não quando a resposta chega.
      const daFoco = focoDaPonte === sessao.idDaPonte || !sessao.idDaPonte;
      if (daFoco) soltarOMicrofoneDaVoz();
      return Promise.resolve(pelaSessaoDe(sessao, "leave_voice_room")(args)).then((resposta) => {
        // A resposta de um SAIR antigo não mexe numa reentrada que já nasceu: só o pedido mais novo confere a sala.
        if (daFoco && pedido === vozDaSessao.pedido) conferirAVozDaSessao(sessao);
        return resposta;
      });
    }),
  // O mudo e o isolamento são escolhas desta ponta (valem fora da sala), e o
  // servidor fica sabendo.
  set_muted: ({ on } = {}) =>
    comASessao(() => {
      vozDaSessao.mudo = Boolean(on);
      vozDaPonte.atual?.mudo(vozDaSessao.mudo);
      return pelaSessao("set_muted")({ on: vozDaSessao.mudo });
    }),
  set_total_isolation: ({ on } = {}) =>
    comASessao(() => {
      vozDaSessao.isolado = Boolean(on);
      vozDaPonte.atual?.isolar(vozDaSessao.isolado);
      return pelaSessao("set_total_isolation")({ on: vozDaSessao.isolado });
    }),
  // O volume de cada pessoa é da sessão, pelo apelido; o ganho vai à mistura
  // pela alça, que o `.wasm` traduz (a SSRC não chega aqui).
  set_volume: ({ nickname, percent } = {}) =>
    comASessao(() =>
      Promise.resolve(pelaSessao("set_volume")({ nickname, percent, comAudio: audioDisponivel() })).then((resposta) => {
        seguirOsVolumes().catch((falha) => console.warn("volumes na mistura:", falha));
        return resposta;
      }),
    ),
  // A busca: `buscar` lê a conversa da sessão; andar e limpar respondem sem
  // sessão, como no desktop, porque a busca é da página.
  buscar: pelaSessao("buscar"),
  // **Os cargos** (F e G, 0.17): os nove, à sessão do carimbo (a
  // `invocarNaPonte` já tirou o carimbo e escolheu a sessão), com o quadro e
  // os membros DELA, no `.wasm` (`seele-web/src/cargos.rs`). As respostas e
  // as recusas têm as formas do desktop (`CargoRecusado`, `SemQuadro`).
  pedir_cargos: pelaSessao("pedir_cargos"),
  explicar_permissoes: pelaSessao("explicar_permissoes"),
  prever_cargo: pelaSessao("prever_cargo"),
  criar_cargo: pelaSessao("criar_cargo"),
  editar_cargo: pelaSessao("editar_cargo"),
  mover_cargo: pelaSessao("mover_cargo"),
  apagar_cargo: pelaSessao("apagar_cargo"),
  atribuir_cargo: pelaSessao("atribuir_cargo"),
  revogar_cargo: pelaSessao("revogar_cargo"),
  // **Andar e limpar vão à SESSÃO** (medido pelo browser-signaling no servidor
  // real): o estado da busca fica na instância da sessão, e a da página
  // respondia `{atual: null, total: 0}`. Sem sessão, a da página responde o
  // estado vazio, como o desktop.
  busca_andar: (args) => (sessaoDaPonte.atual ? pelaSessao("busca_andar")(args) : pelaPagina("busca_andar")(args)),
  busca_limpar: (args) => (sessaoDaPonte.atual ? pelaSessao("busca_limpar")(args) : pelaPagina("busca_limpar")(args)),
  // **O som da tela que se assiste** (R17): o volume e o mudo são escolhas
  // desta ponta, que valem antes de a tela tocar; sem áudio na página, o
  // desktop responde `null` e recusa os dois ajustes.
  som_da_tela: () => comASessao(async () => (audioDisponivel() ? { volume: vozDaSessao.volumeDaTela, calada: vozDaSessao.telaCalada } : null)),
  ajustar_volume_da_tela: ({ volume } = {}) =>
    comASessao(async () => {
      if (!audioDisponivel()) return Promise.reject("NotConnected");
      vozDaSessao.volumeDaTela = Math.min(4, Math.max(0, Number(volume) || 0));
      vozDaPonte.atual?.volumeDaTela(vozDaSessao.volumeDaTela);
      return null;
    }),
  calar_a_tela: ({ calada } = {}) =>
    comASessao(async () => {
      if (!audioDisponivel()) return Promise.reject("NotConnected");
      vozDaSessao.telaCalada = Boolean(calada);
      vozDaPonte.atual?.calarATela(vozDaSessao.telaCalada);
      return null;
    }),
  // **Compartilhar a tela pelo PC** (3.10; no celular, recusado com a frase):
  // o seletor é o do navegador, chamado dentro do gesto, e a transmissão é a
  // de `compartilhar-web.js`. Sem codificador que tire um quadro de verdade,
  // o controle some (`NaoSeSabe`), como no desktop sem captura.
  fontes_de_tela: () =>
    comASessao(async () => {
      if (!(await sondarATela(await pelaPagina("codec_da_tela")({})))) return Promise.reject("ScreenShareUnavailable");
      return [{ id: 1, nome: FONTE_DO_NAVEGADOR, monitor: true, largura: Math.round(screen.width * devicePixelRatio), altura: Math.round(screen.height * devicePixelRatio) }];
    }),
  permissao_de_tela: () => comASessao(async () => ((await sondarATela(await pelaPagina("codec_da_tela")({}))) ? "Concedida" : "NaoSeSabe")),
  pedir_permissao_de_tela: () =>
    comASessao(async () => ((await sondarATela(await pelaPagina("codec_da_tela")({}))) ? "Concedida" : "NaoSeSabe")),
  // O som desta página fora da captura: o `suppressLocalAudioPlayback` do
  // navegador, quando ele o tem.
  exclusao_do_som_da_captura: () => {
    if (!navigator.mediaDevices?.getDisplayMedia) return "SemCaptura";
    return navigator.mediaDevices.getSupportedConstraints?.().suppressLocalAudioPlayback ? "Excluido" : "NaoExcluido";
  },
  compartilhar_tela: ({ limites } = {}) =>
    comASessao((sessao) => {
      // Sem as peças da captura (o WebKit não tem o `MediaStreamTrackProcessor`),
      // a recusa vem ANTES do seletor, dita como é do navegador.
      if (!temCapturaParaCodificar()) {
        return Promise.reject({ RecusaDaPonte: { comando: "compartilhar_tela", frase: FRASES_DA_TELA_NO_NAVEGADOR.recusa } });
      }
      if (!sessao.transmissao) return Promise.reject("ScreenShareUnavailable");
      const comSom = limites?.com_som !== false;
      // Dentro do gesto: nada espera antes do seletor.
      return navigator.mediaDevices
        .getDisplayMedia({
          video: { frameRate: { ideal: limites?.quadros_maximos ?? 30 }, height: { ideal: limites?.altura_maxima ?? 720 } },
          audio: comSom ? { suppressLocalAudioPlayback: true } : false,
          systemAudio: comSom ? "include" : "exclude",
          selfBrowserSurface: "exclude",
        })
        .then(
          (midia) => {
            sessao.transmissao.comecar(midia, limites);
            return null;
          },
          (falha) =>
            Promise.reject(
              falha && falha.name === "NotAllowedError"
                ? { RecusaDaPonte: { comando: "compartilhar_tela", frase: RECUSA_DA_CAPTURA } }
                : "ScreenShareUnavailable",
            ),
        );
    }),
  parar_de_compartilhar: () =>
    comASessao(async (sessao) => {
      if (sessao.transmissao?.noAr) sessao.transmissao.parar();
      else await pelaSessao("parar_de_compartilhar")({});
      return null;
    }),
  ajustar_limites_da_tela: ({ limites } = {}) =>
    comASessao(async (sessao) => {
      if (sessao.transmissao?.noAr) sessao.transmissao.ajustar(limites);
      else await pelaSessao("ajustar_limites_da_tela")({ limites });
      return null;
    }),
  // Nunca erra, com ou sem sessão: só a tecla da voz.
  set_talking: ({ talking } = {}) => {
    vozDaSessao.tecla = Boolean(talking);
    vozDaPonte.atual?.tecla(vozDaSessao.tecla);
    return null;
  },
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
  escolher_arquivo: async () => {
    const arquivo = await abrirOSeletor();
    return arquivo ? descreverAlca(guardarSobAlca(arquivo, arquivo.name)) : null;
  },
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
  modo_de_voz_escolhido: modoDeVozGuardado,
  // Grava; numa sala, a voz aberta passa a usá-lo na hora.
  set_voice_mode: ({ mode } = {}) => {
    if (!MODOS_DE_VOZ.includes(mode)) return Promise.reject(`modo de voz desconhecido: ${String(mode)}`);
    if (!gravarNaOrigem(CHAVES_DA_ORIGEM.modo, mode)) return Promise.reject("NaoGravei");
    vozDaPonte.atual?.modo(BYTE_DO_MODO[mode]);
    return null;
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
  // **O carimbo escolhe a sessão** (E3). Tirado dos argumentos — o `.wasm` e
  // os corpos daqui não o conhecem — e posto como a sessão do pedido durante a
  // parte síncrona do atendimento, que é onde todo corpo pega a sessão.
  const { carimbo, ...semCarimbo } = args ?? {};
  let doPedido = null;
  if (carimbo && typeof carimbo.sessao === "number" && !SEM_SESSAO_DO_CARIMBO.has(comando)) {
    doPedido = sessoesDaPonte.get(carimbo.sessao) ?? null;
    if (!doPedido) return Promise.reject("NotConnected");
    const agora = doPedido.geracaoDoEnlace ?? 1;
    if ((carimbo.geracao ?? agora) < agora) {
      return Promise.reject({ SessaoNaoConfere: { esperado: agora, veio: carimbo.geracao } });
    }
  }
  const antes = sessaoDoPedido;
  sessaoDoPedido = doPedido ?? sessaoDoPedido;
  try {
    return ATENDIMENTOS_DA_PONTE[comando](
      SEM_SESSAO_DO_CARIMBO.has(comando) ? (args ?? {}) : semCarimbo,
    );
  } finally {
    sessaoDoPedido = antes;
  }
}

/**
 * Os comandos cujo `carimbo` é o argumento deles (a sessão a que se refere), e
 * não a sessão do pedido: trazer a voz, sair de uma sessão.
 */
const SEM_SESSAO_DO_CARIMBO = new Set(["focar_voz", "encerrar_sessao", "connect", "sessoes", "foco_de_voz", "ver_sessao"]);

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
