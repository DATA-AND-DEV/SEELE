// Versões lado a lado — ADR 0046.
//
// # O que estava faltando
//
// O núcleo (`seele-lancador`) estava pronto, com noventa testes, e **nenhum
// crate dependia dele**. Depois de ligado, ele listava e abria versões
// instaladas — e nada nunca instalava uma, então a lista era sempre vazia e o
// bloco ficava sempre escondido. Um caminho alcançável sobre uma fonte que
// nunca tinha nada é a mesma coisa que caminho nenhum.
//
// Este arquivo fecha as duas pontas que faltavam:
//
// - **baixar uma versão do catálogo**, que é o que faz a lista deixar de ser
//   vazia;
// - **abrir a versão do servidor ao entrar nele**, que é a frase da decisão:
//   «ao conectar, o cliente roda a versão daquele servidor».
//
// # Por que o link, e não o fio
//
// A versão que hospeda viaja no `seele://`, e é lida **antes** de conectar. Um
// servidor de uma versão anterior pode falar um protocolo que este cliente já
// não alcança: ele é recusado com «versão incompatível» sem chegar a dizer uma
// palavra sobre si. O link chega antes disso, por uma conversa, e não depende
// de protocolo nenhum.

"use strict";

/**
 * As frases das recusas do launcher, uma por variante de
 * `versoes::FalhaAoAbrirVersao` e `versoes::FalhaAoBaixarVersao`.
 *
 * Aqui, e não no `FRASES` de `frases.js`, porque aquele arquivo é editado em
 * série por outra frente; estas recusas só aparecem nas telas deste arquivo.
 * Sem «linha» nas frases: o ADR 0033 tirou a palavra da tela.
 */
const FRASES_DA_VERSAO = {
  IdentificadorInvalido: "ESTE PEDIDO NÃO DIZ UMA VERSÃO QUE SE POSSA ABRIR.",
  SemCatalogo:
    "AINDA NÃO HÁ A LISTA DE VERSÕES NESTA MÁQUINA.\nConecte-se à internet uma vez para buscá-la.",
  NaoPublicada: "ESTA VERSÃO NÃO ESTÁ NA LISTA DE VERSÕES PUBLICADAS.",
  LinhaNaoDeclarada: "A LISTA DE VERSÕES NÃO DIZ QUE SERVIDOR ESTA VERSÃO FALA.",
  InstalacaoIncompleta:
    "A CÓPIA GUARDADA DESTA VERSÃO NÃO TERMINOU DE INSTALAR.\nBaixe de novo.",
  ExecutavelAusente: "O PROGRAMA DESTA VERSÃO SUMIU DA MÁQUINA.\nBaixe de novo.",
  ExecutavelNaoDeclarado:
    "A LISTA DE VERSÕES NÃO DIZ QUAL PROGRAMA ABRIR NESTE SISTEMA.",
  ExecutavelInvalido:
    "A LISTA DE VERSÕES APONTA UM PROGRAMA FORA DA PASTA DA VERSÃO, E ELE NÃO FOI ABERTO.",
  InstalacaoAlterada:
    "O PROGRAMA GUARDADO DESTA VERSÃO NÃO É MAIS O QUE FOI INSTALADO.\nApague-o em CONFIGURAÇÕES · VERSÕES: ele é baixado de novo na próxima vez que for preciso.",
  ListaDeRevogacaoRecusada:
    "A LISTA DE VERSÕES REVOGADAS QUE CHEGOU NÃO CONFERE.\nTente de novo mais tarde.",
  CatalogoNaoRespondeu:
    "A LISTA DE VERSÕES NÃO RESPONDEU.\nConfira a conexão e tente de novo daqui a pouco.",
};

/** A frase de uma recusa do launcher, ou a de sempre quando não é dele. */
function fraseDaVersao(falha) {
  if (typeof falha === "string" && falha in FRASES_DA_VERSAO) {
    return FRASES_DA_VERSAO[falha];
  }
  if (falha && typeof falha === "object") {
    if (falha.LinhaNaoPublicada) {
      return `NÃO HÁ VERSÃO PUBLICADA DO SERVIDOR ${falha.LinhaNaoPublicada}.`;
    }
    if (falha.Revogada) {
      const { defeito, corrigida_em: corrigida } = falha.Revogada;
      return (
        "ESTA VERSÃO FOI REVOGADA.\n" +
        `O defeito: ${defeito}` +
        (corrigida ? `\nA ${corrigida} corrige.` : "")
      );
    }
    if (falha.NaoInstalada) {
      return falha.NaoInstalada.tem_pacote
        ? "ESTA VERSÃO AINDA NÃO ESTÁ NESTA MÁQUINA."
        : "NÃO HÁ PACOTE DESTA VERSÃO PARA ESTE SISTEMA.";
    }
    if (falha.SemPacoteParaEsteSistema) {
      return "NÃO HÁ PACOTE DESTA VERSÃO PARA ESTE SISTEMA.\nNo Mac Intel e no Linux, o Servidor 0.15 nunca teve pacote.";
    }
    if (falha.NaoBaixei) return "O DOWNLOAD NÃO TERMINOU.\nTente de novo.";
    if (falha.PacoteRecusado) {
      return "O PACOTE BAIXADO NÃO PASSOU NA CONFERÊNCIA, E NADA FOI INSTALADO.";
    }
    if (falha.CatalogoNaoServe) return "A LISTA DE VERSÕES NÃO PÔDE SER LIDA.";
    if (falha.NaoIniciou) return "O SISTEMA NÃO DEIXOU ABRIR A OUTRA JANELA.";
  }
  return fraseDeErro(falha);
}

/**
 * Pergunta, no painel de servidores, se abre o SEELE do outro servidor.
 *
 * Devolve uma promessa que resolve `true` para ABRIR e `false` para VOLTAR. A
 * frase das conversas vem **antes** (spec 0.16, §1): elas não atravessam de um
 * servidor para o outro.
 */
function confirmarOutraVersao(servidor, perdeConversas, vaiBaixar) {
  const caixa = $("versoes-confirmar");
  const sim = $("versoes-confirmar-sim");
  const nao = $("versoes-confirmar-nao");
  $("versoes-confirmar-frase").textContent =
    `Este servidor roda o Servidor ${servidor}. Ele abre noutra janela do SEELE, ` +
    `com as conversas dele: o que você escreveu aqui não aparece lá, e o que ` +
    `estiver lá não aparece aqui.` +
    (perdeConversas ? " Nesta máquina ele ainda não tem conversa nenhuma." : "");
  $("versoes-confirmar-estado").textContent = "";
  sim.textContent = vaiBaixar ? "BAIXAR E ABRIR" : "ABRIR";
  sim.hidden = false;
  sim.disabled = false;
  caixa.hidden = false;
  // **O foco vai à pergunta**: ela é a próxima coisa que a pessoa decide, e
  // um diálogo que aparece sem o teclado é um diálogo que o leitor de tela
  // não anuncia.
  sim.focus();
  return new Promise((resolver) => {
    const responder = (resposta) => {
      sim.removeEventListener("click", aoSim);
      nao.removeEventListener("click", aoNao);
      resolver(resposta);
    };
    const aoSim = () => responder(true);
    const aoNao = () => {
      caixa.hidden = true;
      responder(false);
    };
    sim.addEventListener("click", aoSim);
    nao.addEventListener("click", aoNao);
  });
}

/**
 * Entra num servidor de outra linha: abre o SEELE dela, baixando antes se for
 * preciso, já indo para lá.
 *
 * `decisao` é o `versoes::ParaEntrar` que o `analisar_convite` (o link com
 * `v=`) ou o `para_entrar_pela_recusa` (a recusa do servidor) devolveu.
 * **Esta janela não fecha**: quem lançou não segura o processo filho, e fechar
 * daqui deixaria a pessoa sem nada na tela se o outro não subisse.
 *
 * `link` é o `seele://` inteiro, quando houve um: ele leva a outra versão já ao
 * servidor. Sem ele, ela abre na tela de entrada dela.
 *
 * `aoVoltar` devolve a pessoa ao lugar de onde ela veio quando recusa. Sem ele,
 * o campo do painel, que é de onde o link colado veio.
 */
async function entrarPelaLinha(decisao, link, aoVoltar) {
  $("servidores-erro").hidden = true;
  if (decisao.MaisNova) {
    recusarServidor(
      "Este servidor é de uma versão mais nova do SEELE. Atualize o app para entrar nele.",
    );
    return;
  }
  if (decisao.LinhaNaoOferecida) {
    recusarServidor(
      `Este servidor roda o Servidor ${decisao.LinhaNaoOferecida.linha}, que ` +
        "deixou de ser oferecido. Peça a quem hospeda para passar a uma versão mais nova.",
    );
    return;
  }
  const dados = decisao.Abrir ?? decisao.Baixar;
  if (!dados) return;
  const vaiBaixar = Boolean(decisao.Baixar);
  if (!(await confirmarOutraVersao(dados.servidor, dados.perde_conversas, vaiBaixar))) {
    if (aoVoltar) aoVoltar();
    else $("servidores-endereco").focus();
    return;
  }

  const sim = $("versoes-confirmar-sim");
  const estado = $("versoes-confirmar-estado");
  sim.disabled = true;
  try {
    if (vaiBaixar) {
      estado.textContent = `baixando o SEELE do Servidor ${dados.servidor}…`;
      try {
        await invoke("baixar_versao", { linha: dados.linha });
      } catch (falha) {
        // Já estar guardada não é falha: é a resposta certa para quem apertou
        // duas vezes, e o que se quer depois é abrir.
        if (!(falha && typeof falha === "object" && "JaInstalada" in falha)) throw falha;
      }
    }
    estado.textContent = `abrindo o SEELE do Servidor ${dados.servidor}…`;
    await invoke("abrir_versao", {
      versao: null,
      linha: dados.linha,
      hospedar: false,
      nomePublico: null,
      link,
    });
    // **Sem link, ela não vai sozinha ao servidor**, e a frase não promete:
    // uma entrada da lista e um endereço digitado não têm o `seele://` que a
    // outra versão lê no `--entrar`.
    estado.textContent = link
      ? `O SEELE do Servidor ${dados.servidor} abriu noutra janela, já indo para lá. ` +
        "Esta continua aberta."
      : `O SEELE do Servidor ${dados.servidor} abriu noutra janela. Entre no servidor ` +
        "por lá, pela lista ou pelo link. Esta continua aberta.";
    sim.hidden = true;
  } catch (falha) {
    estado.textContent = "";
    sim.disabled = false;
    recusarServidor(fraseDaVersao(falha));
  }
}

/**
 * O caminho pela recusa (tarefa 13 da F6, passo 5; P8 da F1; ADR 0058).
 *
 * O servidor recusou o número deste app e disse, no byte 0 da resposta, qual
 * fala. **Só com o `servidor` presente** — a FFI o manda só para uma versão que
 * o launcher abre — a casca pergunta ao Rust o que fazer e segue pelo mesmo
 * `entrarPelaLinha` do link com `v=`. Sem ele (um servidor mais novo, ou de
 * antes da 0.15), não há versão a abrir: mais nova quer dizer «atualize», e a
 * frase da recusa é tudo.
 *
 * `link` é o `seele://` colado nesta tentativa, quando houve um. Ele vai ao
 * `abrir_versao` e a nenhum outro lugar: pode levar um convite de uso único,
 * que não foi gasto, porque a recusa por versão vem antes do Hello, e por isso
 * não entra em registro nem em frase.
 *
 * A pergunta mora no painel de servidores, que está fechado na hora da recusa:
 * ele reabre, e o VOLTAR o fecha e devolve o foco ao CONECTAR, de onde a
 * pessoa veio.
 *
 * Responde se ofereceu.
 */
async function entrarPelaRecusa(motivo, link) {
  const servidor = motivo?.OutraLinha?.servidor;
  if (!servidor) return false;
  let decisao;
  try {
    decisao = await invoke("para_entrar_pela_recusa", { linha: servidor });
  } catch (falha) {
    console.warn("para_entrar_pela_recusa:", falha);
    return false;
  }
  if (!decisao || typeof decisao !== "object") return false;
  await abrirServidores($("botao-conectar"));
  entrarPelaLinha(decisao, link ?? null, fecharServidores);
  return true;
}

/** Um tamanho em bytes, para gente: «12 MB», «840 KB». */
function tamanhoLegivel(bytes) {
  if (bytes >= 1048576) return `${Math.round(bytes / 1048576)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/**
 * Desenha a lista de versões em CONFIGURAÇÕES, com o que cada uma ocupa.
 *
 * A tela de disco mínima (spec 0.16, §2, F6; ADR 0046: «uma tela que diz quanto
 * e permite apagar»). Cada app guardado tem o tamanho e um REMOVER; as
 * conversas de cada versão do servidor aparecem à parte, **sem** botão, e
 * dizendo que ficam: apagar o app não as leva, e o próximo app da mesma versão
 * do servidor as abre.
 */
async function desenharPainelDeVersoes() {
  let versoes = [];
  let ocupacao = { total: 0, versoes: [], dados: [] };
  try {
    versoes = await invoke("versoes_instaladas");
    ocupacao = await invoke("ocupacao_do_deposito");
  } catch (falha) {
    console.warn("versoes:", falha);
  }
  const tamanhoDe = new Map(ocupacao.versoes.map((o) => [o.versao, o.bytes]));

  const lista = $("lista-versoes-instaladas");
  if (versoes.length === 0 && ocupacao.dados.length === 0) {
    // **Vazia não é defeito**, e a frase precisa dizer isso: é o estado de toda
    // instalação que nunca guardou uma segunda versão, que é quase todas.
    repovoar(lista, [
      elemento("li", "server-dispositivos-vazio", "NENHUMA VERSÃO GUARDADA AO LADO DESTA"),
    ]);
    return;
  }
  const itens = versoes.map((v) => {
    const linha = elemento("li");
    const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
    const texto = elemento("span", "server-dispositivo-nome");
    texto.append(elemento("span", "mods-id", v.versao));
    if (v.em_uso) {
      texto.append(elemento("span", "mods-versao", "é a que está aberta agora"));
    } else if (!v.abrivel) {
      // Dita, e não oferecida: a instalação está lá e não sabe dizer o que
      // rodar — marcador antigo, ou executável apagado depois.
      texto.append(elemento("span", "mods-recusado", "sem procedência: apague para baixar de novo"));
    }
    if (tamanhoDe.has(v.versao)) {
      texto.append(elemento("span", "mods-versao", tamanhoLegivel(tamanhoDe.get(v.versao))));
    }
    caixa.append(texto);
    if (!v.em_uso && v.abrivel) {
      const abrir = elemento("button", "botao-fantasma");
      abrir.type = "button";
      abrir.textContent = "ABRIR";
      abrir.addEventListener("click", () => {
        invoke("abrir_versao", {
          versao: v.versao,
          linha: null,
          hospedar: false,
          nomePublico: null,
          link: null,
        }).catch((falha) => console.warn("abrir versão:", falha));
      });
      caixa.append(abrir);
    }
    if (!v.em_uso && tamanhoDe.has(v.versao)) {
      const remover = elemento("button", "botao-fantasma");
      remover.type = "button";
      remover.textContent = "REMOVER";
      remover.addEventListener("click", () => {
        removerVersao(v.versao).catch((falha) => console.warn("remover versão:", falha));
      });
      caixa.append(remover);
    }
    linha.append(caixa);
    return linha;
  });
  for (const d of ocupacao.dados) {
    const linha = elemento("li");
    const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
    const texto = elemento("span", "server-dispositivo-nome");
    texto.append(elemento("span", "mods-id", `conversas do Servidor ${d.linha}`));
    texto.append(
      elemento("span", "mods-versao", `${tamanhoLegivel(d.bytes)} — ficam mesmo apagando o app`),
    );
    caixa.append(texto);
    linha.append(caixa);
    itens.push(linha);
  }
  repovoar(lista, itens);
}

/** Apaga um app guardado, sem tocar nas conversas, e diz o que houve. */
async function removerVersao(versao) {
  const estado = $("versoes-estado");
  estado.classList.remove("mods-recusado");
  try {
    const liberado = await invoke("remover_versao", { versao });
    estado.textContent =
      `${versao} apagada: ${tamanhoLegivel(liberado)} liberados. As conversas ficaram, ` +
      "e são abertas pelo próximo app da mesma versão do servidor.";
  } catch (falha) {
    estado.textContent =
      falha && typeof falha === "object" && "NaoRemovi" in falha
        ? "Não deu para apagar: ela está aberta? Feche a janela dela e tente de novo."
        : fraseDaVersao(falha);
    estado.classList.add("mods-recusado");
  }
  await desenharPainelDeVersoes();
}

/** Baixa do catálogo a versão mais nova e a guarda ao lado desta. */
async function baixarAMaisNova() {
  const botao = $("versoes-baixar");
  const estado = $("versoes-estado");
  botao.disabled = true;
  estado.textContent = "procurando no catálogo…";
  estado.classList.remove("mods-recusado");
  try {
    const versao = await invoke("baixar_versao", { linha: null });
    estado.textContent = `${versao} guardada ao lado desta. Ela não substituiu nada — a versão aberta continua sendo a mesma.`;
  } catch (falha) {
    // `JaInstalada` não é falha e não merece vermelho: é a resposta certa para
    // quem apertou duas vezes.
    const jaTem = falha && typeof falha === "object" && "JaInstalada" in falha;
    estado.textContent = jaTem
      ? `${falha.JaInstalada} já está guardada aqui.`
      : fraseDaVersao(falha);
    if (!jaTem) estado.classList.add("mods-recusado");
  } finally {
    botao.disabled = false;
  }
  await desenharPainelDeVersoes();
  // O bloco da tela de entrada lê a mesma lista, e ela acabou de mudar.
  desenharVersoes().catch((falha) => console.warn("versões da entrada:", falha));
}

/**
 * O que a primeira abertura desta versão fez com as conversas da 0.15.
 *
 * Spec 0.16, §1: as conversas da 0.15 são copiadas para o Servidor 0.15 antes
 * de esta versão mexer na pasta dela, e as conversas de um não aparecem no
 * outro. Dito uma vez — só a abertura que copiou responde `Guardados` —, e dito
 * também quando não deu, porque aí o Servidor 0.15 vai abrir vazio.
 *
 * Sem «linha» na frase: o ADR 0033 tirou a palavra da tela.
 */
async function dizerOQueAPrimeiraAberturaFez() {
  let estado = null;
  try {
    estado = await invoke("estado_da_primeira_abertura");
  } catch (falha) {
    console.warn("primeira abertura:", falha);
    return;
  }
  const dados = estado?.dados;
  if (!dados || typeof dados !== "object") return;
  const nota = $("boot-linha-legada");
  if ("Guardados" in dados) {
    nota.textContent =
      "As conversas da 0.15 ficaram guardadas para o Servidor 0.15. O que você " +
      "escrever nesta versão não aparece lá, e o que estiver lá não aparece aqui.";
    nota.hidden = false;
  } else if ("BancoJaMigrado" in dados || "NaoGuardei" in dados) {
    nota.textContent =
      "Não deu para guardar as conversas da 0.15 para o Servidor 0.15: ele vai " +
      "abrir vazio. As conversas continuam aqui, nesta versão.";
    nota.hidden = false;
  }
}

dizerOQueAPrimeiraAberturaFez();

$("versoes-baixar").addEventListener("click", () => {
  baixarAMaisNova().catch((falha) => console.warn("baixar versão:", falha));
});
