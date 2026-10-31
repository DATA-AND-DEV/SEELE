/* Gerado por ferramentas/build.mjs. API 3. */
(() => {
"use strict";
// Executado exclusivamente no executor do MOD. Cada pacote inclui sua cópia.
//
// A casca dos MODs oficiais: o que os três fazem igual, num lugar só.
//
// # O que mudou com a API 4
//
// A casca da API 3 sabia desenhar **uma região** — a faixa de 240 px que a
// auditoria de 20/09/2026 chamou de «o mesmo formulário estreito para
// atividades diferentes». Ela sabia receber evento, redesenhar na hora e
// guardar rascunho, e isso continua valendo.
//
// O que ela não sabia era **abrir**. Não havia gesto para começar uma
// atividade: os três MODs desenhavam ao conectar porque a região era o único
// lugar onde eles podiam existir.
//
// Agora ela sabe mais três coisas:
//
// - **registrar uma entrada.** `entrada()` põe um nome na navegação do
//   servidor; ele não ocupa altura nenhuma até alguém apertá-lo;
// - **abrir uma superfície.** `pagina()`, `painel()` e `dialogo()` devolvem
//   punhos com `montar`, `fechar` e estado de alteração;
// - **apresentar dentro do SEELE.** `contribuir()` registra um ponto
//   semântico — o cartão de uma pessoa, por exemplo — e devolve o handle que
//   o revoga.
//
// # A degradação é explícita
//
// `SeeleUI.superficies` **não existe** quando o pacote declara `api: 3`. Não é
// um método que falha: é um método ausente, e o `TypeError` acontece na linha
// que o chama. `temSuperficies` é como um MOD pergunta antes, quando quer
// degradar em vez de falhar.
function interfaceMod(id, titulo, intervalo = 4000) {
  const api = globalThis.SeeleMods, ui = globalThis.SeeleUI;
  if (!api || !ui) throw new Error('Este MOD exige a API 3 do SEELE.');

  // ---- as formas da API 3 ----
  const texto = dentro => ({ forma: 'texto', dentro: String(dentro ?? '') });
  const cabecalho = dentro => ({ forma: 'titulo', dentro: String(dentro ?? '') });
  const lista = itens => ({ forma: 'lista', dentro: itens.map(dentro => ({ forma: 'item', dentro: String(dentro) })) });
  const campo = (chave, rotulo, valor, extra) => ({ forma: 'campo', chave, rotulo, valor: String(valor ?? ''), ...extra });
  const escolha = (chave, rotulo, valor, opcoes) => ({ forma: 'escolha', chave, rotulo, valor: String(valor ?? ''), opcoes });
  const botao = (chave, dentro, desligado = false, extra) => ({ forma: 'botao', chave, dentro: String(dentro), desligado, ...extra });
  const linha = (dentro, estilo) => ({ forma: 'linha', dentro, ...(estilo ? { estilo } : {}) });

  // ---- as formas de composição da API 4 ----
  //
  // Elas existem para o MOD montar **os componentes dele**. Um perfil com
  // faixa, retrato sobreposto e nome não é um formulário mais largo; é uma
  // caixa dentro de outra, com posição, recorte e camada.
  //
  // **`estilo` e `extra` são argumentos separados, e isso não é arrumação.**
  // `classe`, `chave` e `nomeAcessivel` são do **nó**; `fundo` e `intervalo`
  // são do estilo dele. Misturá-los num objeto só faria `classe: 'editor'`
  // virar uma propriedade de estilo que o validador do produto recusa — e a
  // recusa chegaria como «a superfície não montou», sem dizer qual chave.
  const caixa = (dentro, estilo, extra) => ({ forma: 'caixa', dentro, ...(estilo ? { estilo } : {}), ...extra });
  const pilha = (dentro, estilo, extra) => ({ forma: 'pilha', dentro, ...(estilo ? { estilo } : {}), ...extra });
  const grade = (dentro, estilo, extra) => ({ forma: 'grade', dentro, ...(estilo ? { estilo } : {}), ...extra });
  const rolagem = (dentro, estilo, extra) => ({ forma: 'rolagem', dentro, ...(estilo ? { estilo } : {}), ...extra });
  const separador = (estilo, extra) => ({ forma: 'separador', ...(estilo ? { estilo } : {}), ...extra });
  const espaco = (estilo, extra) => ({ forma: 'espaco', ...(estilo ? { estilo } : {}), ...extra });

  // ---- os controles da API 4 ----
  const formulario = (chave, dentro, extra) => ({ forma: 'formulario', chave, dentro, ...extra });
  const acoes = (dentro, fixas = false) => ({ forma: 'acoes', dentro, ...(fixas ? { fixas: true } : {}) });
  const abas = (chave, valor, dentro) => ({ forma: 'abas', chave, valor, dentro });
  const aba = (chave, rotulo, dentro) => ({ forma: 'aba', chave, rotulo, dentro });
  const textoLongo = (chave, rotulo, valor, extra) => ({ forma: 'textoLongo', chave, rotulo, valor: String(valor ?? ''), ...extra });
  const numero = (chave, rotulo, valor, extra) => ({ forma: 'numero', chave, rotulo, valor, ...extra });
  const deslizante = (chave, rotulo, valor, extra) => ({ forma: 'deslizante', chave, rotulo, valor, ...extra });
  const marca = (chave, rotulo, valor) => ({ forma: 'marca', chave, rotulo, valor: valor === true });
  const interruptor = (chave, rotulo, valor) => ({ forma: 'interruptor', chave, rotulo, valor: valor === true });
  const cor = (chave, rotulo, valor) => ({ forma: 'cor', chave, rotulo, valor: String(valor ?? '#000000') });
  const retrato = (chave, extra) => ({ forma: 'retrato', chave, ...extra });
  const distintivo = (dentro, estilo) => ({ forma: 'distintivo', dentro, ...(estilo ? { estilo } : {}) });
  const link = (chave, dentro, endereco) => ({ forma: 'link', chave, dentro: String(dentro), endereco });
  const arquivo = (chave, dentro, extra) => ({ forma: 'arquivo', chave, dentro: String(dentro), ...extra });
  const midia = (chave, extra) => ({ forma: 'midia', chave, ...extra });

  /**
   * Escreve no registro de quem hospeda, **quando há registro**.
   *
   * O `console` do executor nativo só existe a partir da v0.15.1, e a API 5
   * desde a v0.14.2. Nos apps do meio, um `console.warn` num caminho
   * de erro virava `ReferenceError` e derrubava justamente o que ele só queria
   * contar.
   */
  const anotar = (nivel, mensagem) => {
    try { globalThis.console?.[nivel]?.(mensagem); } catch { /* sem registro, não há a quem dizer */ }
  };

  const canalDe = snapshot => {
    const id = snapshot.open_channel ?? snapshot.channels?.[0]?.id;
    return Number.isSafeInteger(id) && id > 0 ? id : null;
  };

  async function request(canal, valor) {
    const resposta = await api.request(id, canal, valor);
    if (!resposta?.ok) throw new Error(resposta?.error || 'O servidor recusou a consulta.');
    return resposta;
  }

  // O último canal visto, para um evento saber a quem falar sem perguntar de
  // novo: `snapshot` é uma ida à ponte, e um arraste não pode pagar uma por
  // quadro.
  let canalAtual = null;
  let pintando = false;
  let pendente = null;

  /**
   * Uma pintura de cada vez, e **nenhuma perdida**.
   *
   * Dois `regiao` em voo chegariam fora de ordem, e o desenho de trás apagaria
   * o da frente — por isso a segunda espera. Mas a primeira versão **descartava**
   * a segunda, e dois eventos seguidos (escolher a densidade e a fonte, no mesmo
   * quadro) perdiam o desenho do segundo: a tela ficava mostrando a escolha
   * anterior, e só o relógio a corrigia, quatro segundos depois.
   *
   * Guardar a última e pintá-la ao fim da que está em voo é o que resolve. É a
   * mesma forma do aviso que chega durante uma colheita: o que não cabe agora
   * não se joga fora, fica marcado.
   */
  const desenhar = async partes => {
    if (pintando) { pendente = partes; return; }
    pintando = true;
    try {
      let atual = partes;
      for (;;) {
        pendente = null;
        await ui.regiao([cabecalho(titulo), ...atual]);
        if (!pendente) return;
        atual = pendente;
      }
    } finally {
      pintando = false;
    }
  };

  /** Uma atualização em voo e apenas o estado mais recente aguardando. */
  function agruparAtualizacoes(atualizar) {
    let emVoo = null;
    let proxima = null;
    return (...args) => {
      proxima = args;
      if (!emVoo) {
        emVoo = Promise.resolve().then(async () => {
          while (proxima) {
            const atuais = proxima;
            proxima = null;
            await atualizar(...atuais);
          }
        }).finally(() => { emVoo = null; });
      }
      return emVoo;
    };
  }

  // ---- as superfícies ----
  //
  // **Ausente, e não recusado.** Quando o pacote declara `api: 3`, o prelúdio
  // monta um `SeeleUI` em que `superficies` simplesmente não está. Um MOD que
  // queira degradar pergunta por `temSuperficies` antes; um que não pergunte
  // recebe um `TypeError` na linha que chamou, com pilha, em vez de uma
  // promessa que rejeita com um código.
  const temSuperficies = Boolean(ui.superficies);
  const temContribuicoes = Boolean(ui.contribuicoes);

  /**
   * Abre (ou reabre) uma superfície e devolve o punho dela.
   *
   * Reabrir **não recria**: chamar com o mesmo `id` mostra a que já existe, e
   * o que estava escrito nos campos dela continua lá. É a diferença entre
   * «mostre aquela» e «comece de novo», e um MOD que quisesse a segunda
   * `descarta` a primeira.
   */
  async function superficie(descricao) {
    if (!temSuperficies) throw new Error('Este SEELE não oferece superfícies de MOD.');
    return ui.superficies.criar(descricao);
  }

  const pagina = (chaveDaTela, titulo, extra) =>
    superficie({ id: chaveDaTela, tipo: 'pagina', titulo, ...extra });
  const painel = (chaveDaTela, titulo, extra) =>
    superficie({ id: chaveDaTela, tipo: 'painel', titulo, ...extra });
  const dialogo = (chaveDaTela, titulo, extra) =>
    superficie({ id: chaveDaTela, tipo: 'dialogo', modal: true, titulo, ...extra });

  /** Um aviso curto. A fila dele tem fim; ver `enfileirarAviso` no produto. */
  const avisar = (mensagem, tom) => {
    if (!temSuperficies) return Promise.resolve(null);
    return ui.superficies.avisar(mensagem, tom);
  };

  /** Registra uma contribuição e devolve o handle que a revoga. */
  const contribuir = pedido => {
    if (!temContribuicoes) throw new Error('Este SEELE não oferece contribuições de MOD.');
    return ui.contribuicoes.registrar(pedido);
  };
  const revogar = handle => {
    if (!temContribuicoes) return Promise.resolve(false);
    return ui.contribuicoes.revogar(handle);
  };

  /**
   * Uma entrada na navegação do servidor.
   *
   * **U03 pelo nome**: sem ela, o MOD não tem gesto de abertura, e desenhar ao
   * conectar é a única forma de existir. Com ela, a atividade começa quando a
   * pessoa a escolhe.
   */
  const entrada = (rotulo, acao = 'abrir', extra) => contribuir({
    ponto: 'servidor.navegacao',
    modo: 'adicionar',
    rotulo,
    acaoPrincipal: acao,
    ...extra,
  });

  /**
   * Diz alguma coisa a quem está olhando, **sem ocupar a sessão**.
   *
   * # Por que ela existe
   *
   * A validação nativa de 20/09/2026, N2: «a faixa inferior continua nos três
   * MODs, tomando quase um terço da janela». A causa não era a faixa existir:
   * era `iniciar` chamar `desenhar` — e portanto `ui.regiao` — a cada volta do
   * relógio, ligado ou não. Com os três instalados, a sessão perdia 230 px de
   * altura permanentemente, mesmo com nenhuma atividade aberta.
   *
   * # O que muda, e o que **não** muda
   *
   * Na API 4 a atividade mora numa superfície, que abre por um gesto. O que
   * sobra para dizer entre uma abertura e outra é recado curto: «gravado»,
   * «não foi possível atualizar». Isso é um aviso, e um aviso sai sozinho.
   *
   * Na API 3 não há aviso nem superfície, e a região continua sendo o único
   * lugar onde este MOD existe. Ela continua exatamente como era — é o
   * caminho de degradação, e não um caminho pior.
   *
   * **O que não muda é que a falha é dita.** Mandá-la para lugar nenhum seria
   * trocar uma faixa que incomoda por um erro que ninguém vê.
   */
  const dizer = (mensagem, tom = 'normal') => {
    if (temSuperficies) return avisar(mensagem, tom).then(() => {}, () => {});
    return desenhar([texto(mensagem)]);
  };

  function iniciar(consultar, semCanal = async () => {}, aoEvento = null) {
    /**
     * Redesenha com o que o estado local diz **agora**.
     *
     * É o que um evento chama. Ele não vai ao servidor: quem digita espera a
     * letra aparecer, e não esperar a rede.
     *
     * Na API 4 ele não pinta a região: quem desenha é a superfície aberta, e
     * o MOD a atualiza pelo punho dela.
     */
    const repintar = partes => { if (!temSuperficies) void desenhar(partes); };

    if (aoEvento) {
      ui.aoEvento(evento => {
        // O erro do MOD fica com o MOD, e é dito em vez de sumir.
        try {
          const talvez = aoEvento(evento, canalAtual, repintar);
          if (talvez && typeof talvez.catch === 'function') {
            talvez.catch(erro => void dizer('Falhou: ' + (erro.message || String(erro)), 'erro'));
          }
        } catch (erro) {
          void dizer('Falhou: ' + (erro.message || String(erro)), 'erro');
        }
      });
    }

    // Um recado repetido a cada quatro segundos é um recado que vira ruído: a
    // fila de avisos tem fim, e enchê-la com a mesma frase tira dela os
    // recados que importam. Só o que mudou é dito.
    let ultimoRecado = '';
    const recado = (mensagem, tom) => {
      if (mensagem === ultimoRecado) return Promise.resolve();
      ultimoRecado = mensagem;
      return dizer(mensagem, tom);
    };

    async function atualizar() {
      try {
        const snapshot = await api.snapshot(), canal = canalDe(snapshot);
        canalAtual = canal;
        if (canal === null) {
          await semCanal();
          await recado('Entre em um servidor com um canal de texto.');
        } else {
          const resultado = await consultar(snapshot, canal);
          // **Não apresente uma resposta do canal anterior após a navegação.**
          // A conferência vale nas duas versões; o que muda é onde a resposta
          // aparece.
          if (canalDe(await api.snapshot()) === canal) {
            // **A região só na API 3.** Na 4, `consultar` continua rodando —
            // ele é quem atualiza cartões, contribuições e o que estiver
            // aberto —, e o que ele devolve para a faixa não é desenhado.
            if (!temSuperficies) await desenhar(resultado);
            ultimoRecado = '';
          } else {
            await recado('Canal alterado. Atualizando…');
          }
        }
      } catch (erro) {
        try { await recado('Não foi possível atualizar: ' + (erro.message || String(erro)), 'erro'); }
        catch (falha) { anotar('error', titulo + ': ' + (falha.message || String(falha))); }
      } finally {
        // Agenda depois de concluir: nunca sobrepõe consultas nem repete
        // escritas. O SEELE encerra o executor e seus temporizadores ao sair.
        setTimeout(atualizar, intervalo);
      }
    }
    void atualizar();
  }

  return {
    // API 3
    texto, cabecalho, lista, campo, escolha, botao, linha, arquivo, midia,
    request, iniciar, desenhar, dizer, agruparAtualizacoes, anotar,
    // API 4 — composição
    caixa, pilha, grade, rolagem, separador, espaco,
    // API 4 — controle
    formulario, acoes, abas, aba, textoLongo, numero, deslizante,
    marca, interruptor, cor,
    // API 4 — apresentação
    retrato, distintivo, link,
    // API 4 — superfícies e integração
    temSuperficies, temContribuicoes,
    superficie, pagina, painel, dialogo, avisar,
    contribuir, revogar, entrada,
  };
}

// O PERFIS: quem está aqui, e a ficha de cada um — **editável e desenhada**.
//
// # O que esta versão conserta
//
// A auditoria de 20/09/2026 disse duas coisas sobre este MOD, e as duas eram
// sobre apresentação e não sobre dados:
//
// - **U19/U27.** «`ferramentas/perfis.js` lê e grava `accent` e `effect`, mas
//   não os usa na árvore visual. O renderer do produto acrescenta esse conteúdo
//   à linha existente; não substitui a identidade visual e não recebe clique de
//   dentro do cartão.» A versão `ce976fd` tinha faixa, retrato sobreposto, cor,
//   animação e cartão clicável — e a migração para a API 3 trocou tudo isso por
//   uma linha de texto embaixo do apelido do servidor.
// - **U20.** «Editar PERFIS não abre modal: troca a lista por formulário longo;
//   "Sobre mim" é input de uma linha.»
//
// Nenhum dos dois era uma escolha deste pacote. A API 3 não tinha como declarar
// uma caixa dentro de outra, nem uma cor, nem uma superfície. A API 4 tem, e é
// isso que esta versão usa.
//
// **Não se restauram os seletores DOM antigos.** O que volta é a capacidade e o
// resultado para a pessoa, por uma API explícita: `pessoa.cartao` substitui a
// apresentação, `pessoa.detalhes` abre o perfil inteiro, e a identidade
// verificável continua nos detalhes nativos do produto, que recolhem em vez de
// sumir.

const {
  texto, cabecalho, campo, botao, linha, escolha,
  caixa, pilha, grade, separador, espaco,
  formulario, acoes, textoLongo, cor, retrato, distintivo, arquivo, midia,
  request, iniciar, temSuperficies, temContribuicoes,
  pagina, dialogo, contribuir, revogar, entrada, avisar, agruparAtualizacoes, anotar,
} = interfaceMod('seele/perfis', 'PERFIS');

/**
 * Os tetos que o servidor deste MOD confere, repetidos aqui **para recusar
 * antes de ler**.
 *
 * O servidor tem a palavra final (`LIMITS` em `servidor/main.js`); estes
 * números só evitam carregar dez megabytes na memória para descobrir depois
 * que não cabem — e deixam o botão dizer o teto antes de o seletor abrir.
 */
const TETO_DO_RETRATO = 10 * 1024 * 1024;
const TETO_DA_FAIXA = 10 * 1024 * 1024;

/** Quanto cabe num fragmento deste servidor. Ele recusa acima disso. */
const FRAGMENTO = 6000;

const EFEITOS = [
  { valor: 'none', dentro: 'NENHUM' },
  { valor: 'aurora', dentro: 'AURORA' },
  { valor: 'sparkle', dentro: 'BRILHO' },
  { valor: 'pulse', dentro: 'PULSO' },
];

/**
 * O efeito escolhido, virado na animação que a API do produto conhece.
 *
 * **Três nomes deste MOD, quatro presets do produto**, e o mapa é explícito
 * porque os dois vocabulários não têm por que coincidir: `sparkle` é o nome que
 * este perfil usa desde a versão que a auditoria chamou de melhor, e `brilho` é
 * o que o validador de estilo aceita. Um mapa aqui é mais honesto que renomear
 * o que as pessoas já escolheram.
 */
const ANIMACAO_DO_EFEITO = {
  none: 'nenhuma',
  aurora: 'aurora',
  sparkle: 'brilho',
  pulse: 'pulso',
};

/** O último retrato da rede, e o que está sendo editado por cima dele. */
let ultimo = null;

/**
 * O que está sendo editado, **por entidade**.
 *
 * `{ [servidor + pessoa]: perfil }`, e não uma variável só. A diferença
 * aparece no conserto de U26: `FECHAR` apagava `rascunho` sem perguntar nada,
 * e quem tinha escrito uma biografia e fechado sem querer a perdia em silêncio.
 *
 * Amarrar o rascunho à entidade é o que permite às duas coisas conviverem:
 * fechar deixa de ser destrutivo (o que estava escrito continua lá ao reabrir)
 * e descartar continua existindo, explícito, para quem quis mesmo jogar fora.
 */
const rascunhos = new Map();
let aviso = '';
let enviandoImagem = false;
/** Se `DESCARTAR` foi apertado e ainda espera a confirmação. */
let perguntandoDescarte = false;
/** De quem é a ficha aberta no diálogo de detalhes. */
let aberto = null;
/** Os punhos das superfícies de pé, para montar de novo sem recriar. */
const telas = { diretorio: null, detalhes: null, editor: null };
/** O handle da substituição de cartão, para revogá-la ao sair. */
let cartaoRegistrado = null;
/** O handle da entrada «Perfis» na navegação, pela mesma razão. */
let entradaRegistrada = null;
const avataresRegistrados = new Map();

const meuPerfil = () => ultimo?.perfis?.[String(ultimo.me)] ?? {};

/**
 * A chave do rascunho: a pessoa, **neste servidor**.
 *
 * O servidor deste MOD guarda perfil por servidor, e um rascunho não deve
 * viajar de um destino para outro — foi um dos riscos que a auditoria mandou
 * reproduzir. Quem fecha isso é o produto: a instância do MOD vive dentro de
 * uma sessão, e sair do servidor encerra todas. O canal ficava fora disso e
 * dentro da chave: uma troca de canal no mesmo servidor, com o editor aberto,
 * remontava o editor com outra chave e o texto sumia da tela.
 */
const chaveDoRascunho = () => String(ultimo?.me ?? '-');

/** O rascunho desta entidade, ou nada quando ninguém editou. */
const meuRascunho = () => rascunhos.get(chaveDoRascunho()) ?? null;

const emEdicao = () => {
  const gravado = meuPerfil(), rascunho = meuRascunho();
  // Imagens são gravadas ao enviar; o rascunho conserva apenas a edição de
  // texto/aparência e nunca esconde uma foto já enviada ou já removida.
  return rascunho ? { ...rascunho, avatar: gravado.avatar, banner: gravado.banner, revision: gravado.revision } : gravado;
};

const mudouOPerfil = () => {
  const guardado = meuRascunho();
  return guardado !== null && JSON.stringify(emEdicao()) !== JSON.stringify(meuPerfil());
};

/** O apelido do servidor, que é a identidade que o produto conhece. */
const apelidoDe = id => {
  const pessoa = ultimo?.pessoas?.find(p => String(p.id) === String(id));
  return pessoa?.nickname || pessoa?.apelido || 'Pessoa ' + id;
};

/** A cor escolhida, ou a do produto quando ninguém escolheu. */
const acentoDe = perfil => {
  const escolhida = String(perfil?.accent ?? '').trim();
  return /^#[0-9a-f]{6}$/i.test(escolhida) ? escolhida : '#f2521f';
};

/** A inicial que o retrato mostra enquanto não há imagem — ou nunca há. */
const inicialDe = (perfil, id) =>
  (String(perfil?.displayName ?? '').trim() || apelidoDe(id)).charAt(0);

/** De onde o produto busca os bytes de uma imagem deste perfil. */
const imagemDoServidor = (id, slot) => ({
  canal: ultimo.canal,
  pedido: { op: 'asset', person: String(id), slot, path: ultimo.perfis[String(id)]?.[slot] ?? null, ...(ultimo.perfis[String(id)]?.[slot]?.startsWith('volume:') ? { transporte: 'volume' } : {}) },
  campo: 'image',
});

// ------------------------------------------------ a apresentação da pessoa

/** Linha da lista: faixa ao fundo, retrato quadrado e nome; o clique abre o perfil. */
function cartaoDaPessoa(id) {
  const perfil = ultimo.perfis[id] ?? {};
  const nome = String(perfil.displayName ?? '').trim() || apelidoDe(id);
  const acento = acentoDe(perfil);
  if (!perfil.displayName && !perfil.pronouns && !perfil.status && !perfil.avatar && !perfil.banner) return null;
  return [{
    forma: 'caixa', chave: 'pessoa-compacta',
    ...(perfil.banner ? { fundoDeMidia: { doServidor: imagemDoServidor(id, 'banner') } } : {}),
    estilo: { largura: 'total', altura: 44, preenchimento: 6, direcao: 'linha', alinhar: 'centro', intervalo: 10,
      fundo: '#0a0806', borda: { largura: 1, cor: '#3a322a' }, recortar: 'cortar', margem: 0 },
    dentro: [
      retrato('foto', { inicial: inicialDe(perfil, id), formato: 'quadrado', descricao: 'Retrato de ' + nome,
        ...(perfil.avatar ? { doServidor: imagemDoServidor(id, 'avatar') } : {}),
        estilo: { largura: 30, altura: 30, encolher: 0, borda: { largura: 1, cor: acento } } }),
      caixa([nome], { corpo: 14, peso: 'forte', cor: '#eae3cf', linhasMaximas: 1, crescer: 1, larguraMinima: 0 }),
    ],
  }];
}

/**
 * O teto por mensagem da ponte, em bytes UTF-8: o padrão de `tetoDaMensagem`
 * no prelúdio do executor. A folga cobre o envelope `{ tipo, n }` que o
 * prelúdio acrescenta, com um `n` de muitos dígitos numa sessão longa.
 */
const TETO_DA_PONTE = 12288;
const FOLGA_DO_ENVELOPE = 64;
/** Pessoas com cartão por MOD: `LIMITES_DO_CARTAO.cartoes` no produto. */
const CARTOES_POR_MOD = 64;

/** Bytes UTF-8 de um texto, contados como o prelúdio conta. */
function bytesUtf8(texto) {
  let bytes = 0;
  for (let i = 0; i < texto.length; i += 1) {
    const c = texto.charCodeAt(i);
    if (c < 0x80) bytes += 1;
    else if (c < 0x800) bytes += 2;
    else if (c >= 0xD800 && c <= 0xDBFF && i + 1 < texto.length
      && texto.charCodeAt(i + 1) >= 0xDC00 && texto.charCodeAt(i + 1) <= 0xDFFF) { bytes += 4; i += 1; }
    else bytes += 3;
  }
  return bytes;
}

/**
 * Os cartões para `SeeleUI.cartoes`, **só os que cabem numa mensagem**.
 *
 * O conjunto vai inteiro numa mensagem só — o produto troca a lista toda a
 * cada chamada —, e a mensagem tem teto. Mandar todo mundo fazia o prelúdio
 * recusar a partir de 14 pessoas com foto e faixa, e a lista congelava no
 * último conjunto aceito. Aqui o conjunto para antes do teto: o próprio
 * cartão primeiro, depois a ordem da lista. Quem não coube fica com a
 * apresentação do SEELE, e `fora` diz quantos foram.
 */
function cartoesDaLista() {
  const cartoes = {};
  const meu = String(ultimo.me);
  const ordem = ultimo.ids.includes(meu) ? [meu, ...ultimo.ids.filter(id => id !== meu)] : ultimo.ids;
  let bytes = bytesUtf8(JSON.stringify({ tipo: 'cartoes', n: 0, cartoes: {} })) + FOLGA_DO_ENVELOPE;
  let quantos = 0, fora = 0;
  for (const id of ordem) {
    const partes = cartaoDaPessoa(id);
    if (!partes) continue;
    // `"id":[…],` — a vírgula conta a mais no primeiro, e sobrar é o lado certo.
    const custo = bytesUtf8(JSON.stringify(id)) + bytesUtf8(JSON.stringify(partes)) + 2;
    if (quantos >= CARTOES_POR_MOD || bytes + custo > TETO_DA_PONTE) { fora += 1; continue; }
    cartoes[id] = partes;
    bytes += custo;
    quantos += 1;
  }
  return { cartoes, fora };
}

/** Quantos ficaram de fora da última vez, para dizer só quando muda. */
let cartoesDeFora = 0;

/**
 * Registra a substituição da apresentação, uma vez por sessão.
 *
 * **Uma contribuição geral, e não uma por pessoa.** O conteúdo de cada cartão
 * já é por pessoa — `SeeleUI.cartoes` o entrega por `id` — e registrar
 * sessenta e quatro contribuições para dizer a mesma regra sessenta e quatro
 * vezes seria pagar por pessoa uma decisão que é do MOD.
 */
async function registrarApresentacao() {
  if (!temContribuicoes || cartaoRegistrado) return;
  const { handle } = await contribuir({
    ponto: 'pessoa.cartao',
    modo: 'substituir',
    // O clique abre o perfil. O produto monta o alvo e liga o `id` real; este
    // MOD só diz **o que fazer** e recebe o `id` de volta no evento.
    acaoPrincipal: 'abrir-perfil',
    nomeAcessivel: 'perfil',
    prioridade: 10,
  });
  cartaoRegistrado = handle;
}

// ------------------------------------------------------- as superfícies

/**
 * Quantas pessoas o diretório monta de uma vez.
 *
 * **Não é gosto, é o fio.** Cada cartão do diretório dá cerca de 700 bytes de
 * declaração, e o teto por mensagem da ponte é 12.288. Com setenta pessoas, o
 * `superficie-montar` dava 52.862 bytes: a mensagem é recusada, a página nasce
 * vazia, e quem está olhando não descobre por quê.
 *
 * É o mesmo defeito que a validação nativa de 20/09/2026 encontrou no ESTILO
 * (N1). Lá ele aparecia com uma pessoa só, porque eram três abas montadas de
 * uma vez; aqui ele aparece a partir de umas vinte pessoas, e por isso não
 * apareceu naquela rodada — o servidor de teste tinha uma.
 *
 * A conta, medida e não estimada: dezesseis cartões deram 12.792 bytes, ou
 * cerca de 780 por cartão mais 300 de casca. Dez deixa a mensagem perto de
 * 8 KiB e guarda margem para nome longo, status longo e faixa — que é o que
 * um cartão cheio tem e o do teste não tinha.
 */
const PESSOAS_POR_PAGINA = 10;

/** Em que página do diretório estamos. Volta a zero ao reabrir. */
let paginaDoDiretorio = 0;

/** O diretório: quem está aqui, com o cartão de cada um e o que abrir. */
function oDiretorio() {
  if (!ultimo) return [texto('Consultando os perfis deste servidor…')];
  if (!ultimo.ids.length) return [texto('Nenhuma pessoa disponível.')];

  const meu = String(ultimo.me);
  const paginas = Math.max(1, Math.ceil(ultimo.ids.length / PESSOAS_POR_PAGINA));
  const pagina = Math.min(Math.max(paginaDoDiretorio, 0), paginas - 1);
  const primeira = pagina * PESSOAS_POR_PAGINA;
  const daPagina = ultimo.ids.slice(primeira, primeira + PESSOAS_POR_PAGINA);
  const cartoes = daPagina.map(id => {
    const perfil = ultimo.perfis[id] ?? {};
    const eu = id === meu;
    return caixa([
      caixa([
        retrato('r-' + id, {
          inicial: inicialDe(perfil, id),
          formato: 'quadrado',
          descricao: 'retrato de ' + apelidoDe(id),
          ...(perfil.avatar ? { doServidor: imagemDoServidor(id, 'avatar') } : {}),
          estilo: { borda: { largura: 2, cor: acentoDe(perfil) }, largura: 44, altura: 44 },
        }),
        pilha([
          caixa([String(perfil.displayName ?? '').trim() || apelidoDe(id)],
            { peso: 'forte', cor: acentoDe(perfil) }),
          caixa([apelidoDe(id) + (eu ? ' · você' : '')], { corpo: 11, opacidade: 0.7 }),
        ], { intervalo: 2, crescer: 1 }),
      ], { direcao: 'linha', alinhar: 'centro', intervalo: 10 }),
      String(perfil.status ?? '').trim()
        ? caixa([String(perfil.status).trim()], { corpo: 12, linhasMaximas: 2, alturaMinima: 36, crescer: 1 })
        : caixa([], { alturaMinima: 36, crescer: 1 }),
      acoes([
        botao('abrir-' + id, eu ? 'EDITAR MEU PERFIL' : 'VER PERFIL',
          false, { variante: eu ? 'primaria' : 'secundaria' }),
      ]),
    ].filter(Boolean), {
      direcao: 'coluna', intervalo: 12,
      preenchimento: 16,
      borda: { largura: 1, cor: '#3a322a' },
      raio: 0,
    });
  });

  return [
    caixa(['Cada pessoa aqui escolheu como aparecer neste servidor.'],
      { opacidade: 0.75, corpo: 11 }),
    // **Quantas existem, e quais estão nesta página.** A contagem total é o
    // que alguém procura para saber se falta gente; ela não pode sumir só
    // porque a página mostra dezesseis.
    caixa([ultimo.ids.length + ' pessoa(s) neste servidor'
      + (paginas > 1
        ? ' · mostrando ' + (primeira + 1) + '–' + (primeira + daPagina.length)
        : '')],
    { corpo: 11, opacidade: 0.7 }),
    // Grade: duas colunas em janela larga, uma quando o contêiner aperta. A
    // consulta é **do contêiner** e não da janela — ver `classes` abaixo.
    grade(cartoes, { colunas: 2, intervalo: 12 }, { classe: 'diretorio' }),
    ...(paginas > 1 ? [acoes([
      botao('diretorio-anterior', 'ANTERIORES', pagina === 0, { variante: 'discreta' }),
      espaco(),
      caixa(['página ' + (pagina + 1) + ' de ' + paginas], { corpo: 11, opacidade: 0.7 }),
      espaco(),
      botao('diretorio-proximas', 'PRÓXIMAS', pagina >= paginas - 1, { variante: 'discreta' }),
    ])] : []),
  ];
}

/** As classes do diretório: o que muda quando a superfície é estreita. */
const CLASSES_DO_DIRETORIO = {
  diretorio: {
    base: {},
    consultas: [{ ateLargura: 520, estilo: { colunas: 1 } }],
  },
};

/** O perfil de uma pessoa, inteiro, para o diálogo de detalhes. */
function osDetalhes(id) {
  const perfil = ultimo.perfis[id] ?? {};
  const acento = acentoDe(perfil);
  const apelido = apelidoDe(id);
  const exibido = String(perfil.displayName ?? '').trim();
  const partes = [];

  if (perfil.banner) {
    partes.push(caixa(
      [midia('d-faixa', { doServidor: imagemDoServidor(id, 'banner'), descricao: 'faixa de ' + apelido })],
      { altura: 120, recortar: 'cortar', raio: 0 },
    ));
  }

  partes.push(caixa([
    retrato('d-retrato', {
      inicial: inicialDe(perfil, id),
      formato: 'quadrado',
      descricao: 'retrato de ' + apelido,
      ...(perfil.avatar ? { doServidor: imagemDoServidor(id, 'avatar') } : {}),
      estilo: { largura: 72, altura: 72, borda: { largura: 3, cor: acento } },
    }),
    pilha([
      caixa([exibido || apelido], { corpo: 20, peso: 'forte', cor: acento }),
      // **A identidade que o servidor conhece fica visível.** Um nome exibido
      // pode ser qualquer coisa; o apelido é o que o produto usa para moderar,
      // e esconder essa diferença é o que tornaria a apresentação enganosa.
      caixa(['neste servidor: ' + apelido + ' · ID ' + id], { corpo: 11, opacidade: 0.7 }),
      ...(String(perfil.pronouns ?? '').trim()
        ? [caixa([String(perfil.pronouns).trim()], { corpo: 12, opacidade: 0.85 })]
        : []),
    ], { intervalo: 3, crescer: 1 }),
  ], {
    direcao: 'linha',
    alinhar: 'fim',
    intervalo: 12,
    ...(perfil.banner ? { mover: { x: 8, y: -36 } } : {}),
  }));

  if (String(perfil.status ?? '').trim()) {
    partes.push(caixa([String(perfil.status).trim()],
      { borda: { largura: 1, cor: acento }, cor: acento }));
  }

  const bio = String(perfil.bio ?? '').trim();
  if (bio) {
    partes.push(separador());
    partes.push(caixa([bio], { entrelinha: 1.6, alturaMaxima: 320, recortar: 'rolar' }));
  } else if (!ultimo.perfis[id]) {
    partes.push(caixa(['Ainda sem perfil salvo.'], { opacidade: 0.7 }));
  }

  return partes;
}

/**
 * O editor: formulário e prévia lado a lado.
 *
 * U20 pediu exatamente esta forma — «editor em modal, biografia multilinha e
 * prévia» — e o §2 do plano a detalha: «prévia lado a lado em janela larga,
 * empilhados em janela estreita». O empilhamento é uma consulta de contêiner,
 * e não de janela: o diálogo tem largura própria, e é ela que decide.
 */
/**
 * Um título de grupo dentro de uma superfície.
 *
 * O terceiro e último nível de tipo desta tela. Acima dele está a cartela do
 * título da janela, que o produto monta; abaixo, o rótulo de campo, que o
 * produto desenha em mono apagado. Tudo o que não for uma destas três coisas
 * não é título de nada — e era esse o defeito da versão anterior, em que cada
 * campo carregava um rótulo do mesmo peso e a tela inteira lia como uma lista
 * de coisas iguais.
 */
function grupo(nome) {
  return caixa([nome], {
    familia: 'sans',
    peso: 'forte',
    corpo: 13,
    espacamento: 1,
    transformar: 'maiuscula',
    cor: '#908574',
  });
}

/**
 * O editor: **o cartão em tamanho real, e os controles servindo ele.**
 *
 * # Por que a prévia subiu
 *
 * A composição anterior punha o formulário à esquerda e a prévia à direita, e
 * a coluna da direita ficava 60% vazia — o cartão tem 90px de altura e o
 * formulário tem 700. O que se está editando aparecia como um detalhe ao lado
 * da ferramenta de editar.
 *
 * Aqui ele abre a janela, com a largura inteira, do tamanho em que vai
 * aparecer na lista. Quem digita o nome vê o nome mudar no lugar para onde já
 * está olhando, e a coluna vazia deixa de existir porque não há mais duas
 * colunas de alturas diferentes.
 *
 * # Por que dois grupos, e não seis campos
 *
 * «Identidade» e «imagens» são decisões de naturezas diferentes: uma é texto
 * que se digita, a outra é arquivo que se escolhe e que pode falhar no envio.
 * Separá-las é hierarquia de comando — `specs/07` — e não arrumação.
 */
function oEditor() {
  const perfil = emEdicao();
  const mudou = mudouOPerfil();
  const meu = String(ultimo.me);
  const acento = acentoDe(perfil);
  const cartao = cartaoDaPreviaComRascunho(meu);

  // **A prévia, em tamanho real.** Sem moldura extra em volta: o cartão já tem
  // a dele, e uma segunda borda a 10px da primeira é a moldura de uma moldura.
  // O que diz que aquilo é uma prévia é a linha acima dela, não um quadro.
  const previa = pilha([
    caixa([
      grupo('Seu cartão na lista de pessoas'),
      caixa([mudou ? 'ainda não gravado' : 'como está gravado'], {
        corpo: 11,
        cor: mudou ? acento : '#908574',
      }),
    ], { direcao: 'linha', alinhar: 'fim', distribuir: 'entre', intervalo: 12, quebra: 'sim' }),
    caixa(cartao ?? [caixa([
      'Escreva um nome, escolha uma cor, e o cartão aparece aqui.',
    ], {
      preenchimento: 20,
      alinhamento: 'centro',
      cor: '#908574',
      borda: { largura: 1, estilo: 'tracejada', cor: '#3a322a' },
      largura: 'total',
    })], { largura: 'total', larguraMaxima: 288 }),
  ], { intervalo: 10 });

  const identidade = pilha([
    grupo('Identidade'),
    campo('displayName', 'NOME EXIBIDO', perfil.displayName ?? '', { erro: erroDeTexto('displayName', perfil) }),
    // Pronome e status dividem a linha: são dois campos curtos, e um deles
    // sozinho numa linha de 900px é uma linha dizendo que sobrou espaço.
    caixa([
      caixa([campo('pronouns', 'PRONOMES', perfil.pronouns ?? '', { erro: erroDeTexto('pronouns', perfil) })],
        { crescer: 1, base: 0, larguraMinima: 160 }),
      caixa([campo('status', 'STATUS', perfil.status ?? '', { erro: erroDeTexto('status', perfil) })],
        { crescer: 1, base: 0, larguraMinima: 160 }),
    ], { direcao: 'linha', intervalo: 16, quebra: 'sim' }),
    // **Multilinha.** U20: «"Sobre mim" é input de uma linha.» Não era uma
    // escolha deste pacote: a API 3 não tinha outra forma para declarar.
    textoLongo('bio', 'SOBRE MIM', perfil.bio ?? '', {
      erro: erroDeTexto('bio', perfil),
      linhas: 4,
      sugestao: 'O que você quer que as pessoas deste servidor saibam.',
    }),
  ]);

  const aparencia = pilha([
    grupo('Aparência'),
    // **Seletor e hexadecimal.** U23 pediu os dois juntos para o ESTILO, e a
    // mesma razão vale aqui: escolher uma cor num campo de texto é escolher
    // uma cor sem vê-la.
    cor('accent', 'COR', acento),
    escolha('effect', 'EFEITO', perfil.effect ?? 'none', EFEITOS),
    arquivo('avatar', 'RETRATO', {
      desligado: enviandoImagem,
      // **Para quê, de que tipo, até quanto.** O produto usa a finalidade
      // como título do diálogo do sistema, o tipo como filtro de extensões e
      // o teto para recusar antes de ler. A auditoria de 20/09/2026 abriu
      // este mesmo botão e leu «Escolha um arquivo para este MOD», com JSONs
      // na lista.
      finalidade: 'Escolha o retrato do seu perfil neste servidor',
      tipos: ['imagem'],
      limiteDeBytes: TETO_DO_RETRATO,
    }),
    arquivo('banner', 'FAIXA', {
      desligado: enviandoImagem,
      finalidade: 'Escolha a faixa que aparece atrás do seu retrato',
      tipos: ['imagem'],
      limiteDeBytes: TETO_DA_FAIXA,
    }),
    ...(meuPerfil().avatar || meuPerfil().banner ? [caixa([
      botao('tirar-avatar', 'TIRAR', !meuPerfil().avatar, { variante: 'discreta' }),
      botao('tirar-banner', 'TIRAR FAIXA', !meuPerfil().banner, { variante: 'discreta' }),
    ], { direcao: 'linha', intervalo: 12, quebra: 'sim' })] : []),
  ]);

  return [
    previa,
    separador(),
    formulario('perfil', [
      caixa([
        caixa([identidade], { crescer: 1.4, base: 0, larguraMinima: 280 }),
        caixa([aparencia], { crescer: 1, base: 0, larguraMinima: 220 }),
      ], { direcao: 'linha', intervalo: 32, quebra: 'sim', alinhar: 'inicio' }),
    ]),
    ...(aviso ? [caixa([aviso], { corpo: 12, cor: acento })] : []),
    acoes([
      botao('descartar', perguntandoDescarte ? 'CONFIRMAR DESCARTE' : 'DESCARTAR',
        !mudou, { variante: perguntandoDescarte ? 'perigo' : 'discreta' }),
      espaco(),
      botao('gravar', mudou ? 'GRAVAR' : 'GRAVADO', !mudou, { variante: 'primaria' }),
    ], true),
  ];
}

/** As classes do editor: empilhado quando o diálogo é estreito. */
const CLASSES_DO_EDITOR = {
  editor: {
    base: {},
    consultas: [{ ateLargura: 560, estilo: { direcao: 'coluna' } }],
  },
};

/** O cartão da prévia: o desenho de sempre, com o rascunho por cima. */
function cartaoDaPreviaComRascunho(id) {
  const gravado = ultimo.perfis[id];
  ultimo.perfis[id] = emEdicao();
  try {
    return cartaoDaPessoa(id);
  } finally {
    // Reposto sempre: a prévia é uma leitura, e deixá-la no lugar do gravado
    // faria a gravação seguinte comparar o rascunho consigo mesmo e concluir
    // que nada mudou.
    if (gravado === undefined) delete ultimo.perfis[id];
    else ultimo.perfis[id] = gravado;
  }
}

// -------------------------------------------------- abrir e fechar telas

async function abrirDiretorio() {
  if (!temSuperficies) return;
  // Abrir começa na primeira página: quem fechou na página quatro e voltou
  // meia hora depois procura o começo, e não onde parou.
  if (!telas.diretorio) paginaDoDiretorio = 0;
  telas.diretorio ??= await pagina('perfis-diretorio', 'Perfis deste servidor');
  await telas.diretorio.classes(CLASSES_DO_DIRETORIO);
  await telas.diretorio.montar(oDiretorio());
  await telas.diretorio.mostrar();
}

async function abrirDetalhes(id) {
  if (!temSuperficies) return;
  aberto = String(id);
  telas.detalhes ??= await dialogo('perfis-detalhes', 'Perfil', {
    tamanho: { largura: 520 },
  });
  await telas.detalhes.titulo('Perfil de ' + (ultimo.perfis[aberto]?.displayName?.trim() || apelidoDe(aberto)));
  await telas.detalhes.montar(osDetalhes(aberto));
  await telas.detalhes.mostrar();
}

async function abrirEditor() {
  if (!temSuperficies) return;
  telas.editor ??= await dialogo('perfis-editor', 'Editar meu perfil', {
    tamanho: { largura: 820 },
    focoInicial: 'displayName',
    // **O host pergunta antes de descartar.** Ver `pedirFechamento` no produto:
    // o MOD é avisado e pode responder, mas não pode vetar a saída para sempre.
    fecharComAlteracoes: 'confirmar',
  });
  await telas.editor.classes(CLASSES_DO_EDITOR);
  await telas.editor.montar(oEditor());
  await telas.editor.suja(mudouOPerfil());
  await telas.editor.mostrar();
}

/** Redesenha a tela que estiver aberta, sem ir ao servidor. */
const repintarTelas = agruparAtualizacoes(async () => {
  if (telas.editor) {
    await telas.editor.montar(oEditor());
    await telas.editor.suja(mudouOPerfil());
  }
  if (telas.detalhes && aberto) await telas.detalhes.montar(osDetalhes(aberto));
  if (telas.diretorio) await telas.diretorio.montar(oDiretorio());
});

// --------------------------------------------------------- o envio de imagem

/**
 * Manda ao servidor o arquivo que alguém escolheu, em fragmentos.
 *
 * Os bytes nunca estão inteiros aqui: o produto os entrega em pedaços, e cada
 * pedaço é recortado no tamanho que este servidor aceita. O primeiro fragmento
 * carrega o prefixo `data:` porque é isso que o servidor confere para saber que
 * recebeu uma imagem, e não um texto qualquer.
 */
async function enviarImagem(canal, slot, escolhido) {
  if (escolhido.papel !== 'imagem') throw new Error('Escolha uma imagem.');
  if (typeof SeeleUI.enviar === 'function') {
    try {
      const inicio = await request(canal, { op: 'upload-start', slot, transporte: 'volume', bytes: escolhido.bytes });
      await SeeleUI.enviar(escolhido.id, inicio.token);
      await request(canal, { op: 'upload-finish', token: inicio.token });
      return;
    } finally {
      try { await SeeleUI.soltar(escolhido.id); }
      catch (erro) { anotar('error', 'PERFIS: falha ao soltar arquivo: ' + (erro.message || erro)); }
    }
  }
  const prefixo = 'data:' + escolhido.tipo + ';base64,';
  // O tamanho anunciado é o da cadeia inteira, prefixo incluído: é o que o
  // servidor compara ao somar os fragmentos.
  const total = prefixo.length + Math.ceil(escolhido.bytes / 3) * 4;
  let inicio;

  let sobra = prefixo;
  let lidos = 0;
  let indice = 0;
  let enviado = 0;
  // **`finally`, e não depois do laço** — o mesmo risco que a auditoria de
  // 20/09/2026 apontou no MESA, e que vale igual aqui: um envio que falha no
  // meio deixava os bytes presos no produto até a saída da sessão.
  try {
  inicio = await request(canal, { op: 'upload-start', slot, length: total });
  for (;;) {
    if (sobra.length < FRAGMENTO && lidos < escolhido.bytes) {
      const pedaco = await SeeleUI.pedaco(escolhido.id, lidos);
      if (!pedaco) throw new Error('O arquivo acabou antes do esperado.');
      // Quatro caracteres por três bytes: é assim que se sabe quanto do
      // arquivo o pedaço cobriu, sem ter os bytes na mão.
      lidos += (pedaco.length / 4) * 3;
      sobra += pedaco;
      continue;
    }
    if (!sobra.length) break;
    // O último fragmento é o único que pode ser menor: o servidor recusa um
    // fragmento curto no meio, porque um curto no meio é um upload truncado.
    const parte = sobra.slice(0, FRAGMENTO);
    sobra = sobra.slice(parte.length);
    enviado += parte.length;
    if (parte.length < FRAGMENTO && enviado !== total) {
      throw new Error('O arquivo mudou no meio do envio.');
    }
    const resposta = await request(canal, {
      op: 'upload-part', slot, token: inicio.token, index: indice, part: parte,
    });
    indice += 1;
    if (resposta.finished) break;
  }
  } finally {
    // **Devolvido na hora.** Dez megabytes presos até a saída da sessão seriam
    // dez megabytes que ninguém mais vai ler.
    try {
      await SeeleUI.soltar(escolhido.id);
    } catch (erro) {
      // Uma falha ao devolver não pode substituir a falha que a trouxe.
      anotar('error', 'PERFIS: o arquivo não foi devolvido: ' + (erro.message || erro));
    }
  }
}

// ------------------------------------------------------------ a região

/**
 * O que continua na faixa, e por que é tão pouco.
 *
 * U01: «O centro da janela fica disponível para conversa vazia, enquanto
 * editar perfil e jogar exige rolar um rodapé.» A resposta não é uma faixa
 * melhor — é uma faixa que não precisa carregar a atividade inteira.
 *
 * O que sobra aqui é o que **cabe numa linha**: quem você é neste servidor, e
 * a porta para o resto. Sem superfícies — um SEELE de API 3 — a faixa volta a
 * ser tudo o que há, e o MOD degrada em vez de falhar.
 */
function aRegiao() {
  if (!ultimo) return [texto('Consultando os perfis deste servidor…')];
  if (!temSuperficies) return oDiretorioNaFaixa();
  const meu = String(ultimo.me);
  const perfil = meuPerfil();
  return [
    caixa([
      retrato('meu-retrato', {
        inicial: inicialDe(perfil, meu),
        formato: 'quadrado',
        descricao: 'seu retrato',
        ...(perfil.avatar ? { doServidor: imagemDoServidor(meu, 'avatar') } : {}),
        estilo: { borda: { largura: 2, cor: acentoDe(perfil) } },
      }),
      pilha([
        caixa([String(perfil.displayName ?? '').trim() || apelidoDe(meu)],
          { peso: 'forte', cor: acentoDe(perfil) }),
        caixa([ultimo.ids.length + ' pessoa(s) neste servidor'],
          { corpo: 11, opacidade: 0.7 }),
      ], { intervalo: 2, crescer: 1 }),
      botao('abrir-diretorio', 'PERFIS'),
      botao('abrir-editor', 'EDITAR', false, { variante: 'primaria' }),
    ], { direcao: 'linha', alinhar: 'centro', intervalo: 8, quebra: 'sim' }),
    ...(aviso ? [texto(aviso)] : []),
  ];
}

/** A faixa de antes, para um SEELE que ainda não tem superfícies. */
function oDiretorioNaFaixa() {
  const partes = [];
  for (const id of ultimo.ids) {
    const perfil = ultimo.perfis[id] ?? {};
    const eu = id === String(ultimo.me);
    partes.push(linha([
      texto((perfil.displayName || apelidoDe(id)) + (eu ? ' (eu)' : '') + ' · ID ' + id),
      botao('abrir-' + id, eu ? 'EDITAR' : 'VER'),
    ]));
  }
  if (aviso) partes.push(texto(aviso));
  return partes;
}

// --------------------------------------------------------------- gravar

const LIMITES_DE_TEXTO = { displayName: 40, pronouns: 30, status: 60, bio: 280 };
const NOMES_DE_TEXTO = { displayName: 'Nome exibido', pronouns: 'Pronomes', status: 'Status', bio: 'Sobre mim' };
function erroDeTexto(chave, perfil) {
  const tamanho = String(perfil[chave] ?? '').length;
  const teto = LIMITES_DE_TEXTO[chave];
  return tamanho > teto ? `${NOMES_DE_TEXTO[chave]}: use até ${teto} caracteres (atual: ${tamanho}).` : '';
}

async function gravar(canal) {
  const perfil = emEdicao();
  const erro = Object.keys(LIMITES_DE_TEXTO).map(chave => erroDeTexto(chave, perfil)).find(Boolean);
  if (erro) throw new Error(erro);
  aviso = 'gravando…';
  const resposta = await request(canal, {
    op: 'save',
    revision: meuPerfil().revision ?? 0,
    profile: {
      displayName: perfil.displayName ?? '',
      pronouns: perfil.pronouns ?? '',
      bio: perfil.bio ?? '',
      status: perfil.status ?? '',
      accent: acentoDe(perfil),
      effect: perfil.effect || 'none',
    },
  });
  // **O que o servidor devolveu, e não o que foi mandado.** A revisão sobe a
  // cada gravação, e guardar o rascunho no lugar dela faria a gravação seguinte
  // ser recusada com «seu perfil mudou em outra janela» — que é verdade sobre a
  // revisão e mentira sobre o que aconteceu.
  ultimo.perfis[String(ultimo.me)] = resposta.profile ?? perfil;
  // Gravado é a única saída que apaga o rascunho sem perguntar: o que ele
  // guardava está no servidor agora.
  rascunhos.delete(chaveDoRascunho());
  perguntandoDescarte = false;
  aviso = 'gravado';
  await publicarCartoes();
}

async function tirarImagem(canal, slot) {
  aviso = 'removendo…';
  const resposta = await request(canal, {
    op: 'clear-image',
    slot,
    revision: meuPerfil().revision ?? 0,
  });
  ultimo.perfis[String(ultimo.me)] = resposta.profile ?? meuPerfil();
  aviso = 'removida';
  await publicarCartoes();
}

/** Consulta e edição compartilham a fila para não duplicar handles em voo. */
const publicarAvatares = agruparAtualizacoes(async () => {
  if (!temContribuicoes) return;
  const desejados = new Map();
  for (const id of ultimo?.ids ?? []) {
    if (!ultimo.perfis[id]?.avatar) continue;
    const doServidor = imagemDoServidor(id, 'avatar');
    desejados.set(String(id), { doServidor, assinatura: JSON.stringify(doServidor) });
  }
  for (const [id, anterior] of avataresRegistrados) {
    if (desejados.get(id)?.assinatura === anterior.assinatura) continue;
    await revogar(anterior.handle);
    avataresRegistrados.delete(id);
  }
  for (const [id, atual] of desejados) {
    if (avataresRegistrados.has(id)) continue;
    const { handle } = await contribuir({
      ponto: 'pessoa.avatar', modo: 'substituir', alvo: id, prioridade: 10,
      conteudo: { doServidor: atual.doServidor },
    });
    avataresRegistrados.set(id, { handle, assinatura: atual.assinatura });
  }
});

/** Entrega os cartões ao produto. A recusa não derruba o painel. */
async function publicarCartoes() {
  try {
    await publicarAvatares();
  } catch (erro) {
    anotar('warn', 'PERFIS: os avatares foram recusados: ' + (erro.message || erro));
  }
  try {
    const { cartoes, fora } = cartoesDaLista();
    if (fora !== cartoesDeFora) {
      cartoesDeFora = fora;
      if (fora) {
        anotar('warn', 'PERFIS: ' + fora + ' pessoa(s) ficaram com a apresentação do SEELE na lista: '
          + 'os cartões de todos não cabem numa mensagem da ponte (' + TETO_DA_PONTE + ' bytes, '
          + CARTOES_POR_MOD + ' pessoas).');
      }
    }
    await SeeleUI.cartoes(cartoes);
  } catch (erro) {
    anotar('warn', 'PERFIS: a lista recusou os cartões: ' + (erro.message || erro));
  }
}

iniciar(
  async (snapshot, canal) => {
    const pessoas = snapshot.presentes || [];
    const ids = [...new Set(pessoas.map(p => String(p.id)))];
    if (snapshot.me != null && !ids.includes(String(snapshot.me))) ids.push(String(snapshot.me));
    const perfis = {};
    let me = snapshot.me;
    for (let i = 0; i < ids.length; i += 32) {
      if (i) await new Promise(resolve => setTimeout(resolve, 150));
      const resposta = await request(canal, { op: 'view', people: ids.slice(i, i + 32) });
      Object.assign(perfis, resposta.profiles);
      me = resposta.me ?? me;
    }
    ultimo = { ids, pessoas, perfis, me, canal };
    await publicarCartoes();
    // A entrada e a substituição, uma vez por sessão. Registrá-las a cada
    // consulta seria uma entrada nova a cada quatro segundos.
    if (temContribuicoes && (!entradaRegistrada || !cartaoRegistrado)) {
      try {
        if (!entradaRegistrada) entradaRegistrada = (await entrada('Perfis', 'abrir-diretorio')).handle;
        await registrarApresentacao();
      } catch (erro) {
        anotar('warn', 'PERFIS: a integração foi recusada: ' + (erro.message || erro));
      }
    }
    await repintarTelas();
    return aRegiao();
  },
  async () => {
    ultimo = null;
    try { await publicarAvatares(); } catch { /* a sessão pode já ter saído */ }
    avataresRegistrados.clear();
    aberto = null;
    // **Revogadas, e não esquecidas.** Sem canal não há perfil de ninguém para
    // abrir; esquecer os handles sem revogá-los fazia a volta ao canal
    // registrar outra entrada «Perfis» e outra substituição do cartão.
    for (const handle of [entradaRegistrada, cartaoRegistrado]) {
      if (handle) {
        try { await revogar(handle); } catch { /* a sessão pode já ter saído */ }
      }
    }
    entradaRegistrada = null;
    cartaoRegistrado = null;
    telas.diretorio = null;
    telas.detalhes = null;
    telas.editor = null;
    // Fora de canal não há perfil de ninguém, e um cartão de antes seria uma
    // afirmação sobre gente que este MOD não está mais vendo.
    try { await SeeleUI.cartoes({}); } catch { /* a sessão pode já ter saído */ }
  },
  (evento, canal, repintar) => {
    if (!ultimo) return null;

    // ---- o clique numa apresentação, vindo do produto ----
    //
    // O `id` é o que o **produto** escreveu no alvo, e não nada que este MOD
    // tenha desenhado: é a diferença entre apresentar uma identidade e afirmar
    // uma. Ver `ligarAcoesDeApresentacao` em `base.js`.
    if (evento.nome === 'acao') {
      if (evento.acao === 'abrir-diretorio') return abrirDiretorio();
      if (evento.acao === 'abrir-editor') return abrirEditor();
      if (evento.acao === 'abrir-perfil' && evento.pessoa) {
        // **O próprio cartão abre o editor, e o dos outros abre a leitura.**
        // Clicar no próprio nome para «ver» o que você mesmo escreveu é um
        // clique a mais para chegar onde a pessoa já queria ir.
        return String(evento.pessoa) === String(ultimo.me)
          ? abrirEditor()
          : abrirDetalhes(evento.pessoa);
      }
      return null;
    }

    // O host pediu para fechar uma superfície. Aceitar é a regra: ver
    // `pedirFechamento` no produto.
    if (evento.nome === 'fechar' || evento.nome === 'fechar-pedido') {
      if (evento.superficie === 'perfis-detalhes') aberto = null;
      // **Quando o produto diz que descartou, descartou.** Com o editor sujo,
      // fechar passa pela confirmação dele — «o que você escreveu e não gravou
      // será perdido» —, e `descartou` é a resposta. Guardar o rascunho aqui
      // tornaria falsa a frase que a pessoa acabou de aceitar.
      if (evento.nome === 'fechar' && evento.superficie === 'perfis-editor' && evento.descartou) {
        rascunhos.delete(chaveDoRascunho());
        perguntandoDescarte = false;
        aviso = '';
        return repintarTelas();
      }
      return null;
    }

    if (evento.nome === 'arquivo') {
      // **Cancelar e falhar deixaram de ser a mesma resposta.** Os dois
      // chegavam como `arquivo: null`; agora `resultado` os separa, e o
      // cancelamento volta a ser o que a auditoria pediu — neutro e sem drama.
      if (!evento.arquivo) {
        aviso = evento.resultado === 'falhou'
          ? (evento.porque || 'não foi possível abrir o seletor')
          : '';
        repintar(aRegiao());
        return repintarTelas();
      }
      if (canal === null || enviandoImagem) return SeeleUI.soltar(evento.arquivo.id);
      enviandoImagem = true;
      aviso = 'enviando…';
      repintar(aRegiao());
      return repintarTelas().then(() => enviarImagem(canal, evento.chave, evento.arquivo)).then(
        async () => {
          const visto = await request(canal, { op: 'view', people: [String(ultimo.me)] });
          Object.assign(ultimo.perfis, visto.profiles);
          aviso = 'enviada';
          await publicarCartoes();
          repintar(aRegiao());
          await repintarTelas();
        },
        async erro => {
          aviso = erro.message || String(erro);
          repintar(aRegiao());
          await repintarTelas();
        },
      ).finally(async () => { enviandoImagem = false; await repintarTelas(); });
    }

    if (evento.nome === 'campo' || evento.nome === 'escolha' || evento.nome === 'cor') {
      const editado = { ...(meuRascunho() ?? meuPerfil()) };
      editado[evento.chave] = evento.valor;
      rascunhos.set(chaveDoRascunho(), editado);
      perguntandoDescarte = false;
      aviso = '';
      repintar(aRegiao());
      return repintarTelas();
    }

    if (evento.nome !== 'botao') return null;

    if (evento.chave === 'abrir-diretorio') return abrirDiretorio();
    if (evento.chave === 'abrir-editor') return abrirEditor();

    // As páginas do diretório. Elas existem porque a declaração inteira não
    // cabe na ponte — ver `PESSOAS_POR_PAGINA`.
    if (evento.chave === 'diretorio-anterior' || evento.chave === 'diretorio-proximas') {
      paginaDoDiretorio += evento.chave === 'diretorio-proximas' ? 1 : -1;
      if (paginaDoDiretorio < 0) paginaDoDiretorio = 0;
      return repintarTelas();
    }
    if (evento.chave.startsWith('abrir-')) {
      const id = evento.chave.slice('abrir-'.length);
      if (!temSuperficies) {
        // Sem superfícies, a faixa continua sendo tudo o que há.
        aberto = id;
        repintar(aRegiao());
        return null;
      }
      return id === String(ultimo.me) ? abrirEditor() : abrirDetalhes(id);
    }

    // **Descartar é explícito, e confirma.** Ele é a única porta que joga fora
    // o que foi escrito, então ele pergunta uma vez antes de fazê-lo.
    if (evento.chave === 'descartar') {
      if (!perguntandoDescarte) {
        perguntandoDescarte = true;
        aviso = 'Descartar apaga o que você escreveu e não gravou. Aperte de novo para confirmar.';
        repintar(aRegiao());
        return repintarTelas();
      }
      rascunhos.delete(chaveDoRascunho());
      perguntandoDescarte = false;
      aviso = 'Alterações descartadas.';
      repintar(aRegiao());
      return repintarTelas();
    }

    if (canal === null) return null;
    const feito = evento.chave === 'gravar' ? gravar(canal)
      : evento.chave === 'tirar-avatar' ? tirarImagem(canal, 'avatar')
        : evento.chave === 'tirar-banner' ? tirarImagem(canal, 'banner')
          : null;
    // A recusa do servidor — permissão, limite, revisão trocada — vira a linha
    // de aviso desta ficha, e não um erro que ninguém lê.
    return feito?.then(
      async () => {
        repintar(aRegiao());
        await repintarTelas();
        if (evento.chave === 'gravar') await avisar('Perfil gravado.');
      },
      async erro => {
        aviso = erro.message || String(erro);
        repintar(aRegiao());
        await repintarTelas();
      },
    ) ?? null;
  },
);

})();
