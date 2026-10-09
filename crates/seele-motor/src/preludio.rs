//! O prelúdio: o JavaScript que roda antes do código de um MOD, dentro do
//! contexto dele, e que monta a API que o MOD conhece sobre as duas funções
//! da ponte.
//!
//! **Um texto só para o desktop e o celular.** Antes de 09/10/2026 ele
//! morava no `executor.rs` do desktop, e o motor do celular o extraía de lá
//! no build.

/// O que roda **antes** do código do MOD, dentro do contexto dele.
///
/// A mesma API que o prelúdio do Worker oferece — `SeeleMods.request`,
/// `SeeleMods.snapshot`, `SeeleUI.regiao`, `SeeleUI.tema` —, montada sobre a
/// ponte que existe aqui: `seele.postar`. O `console` do MOD é montado sobre a
/// outra função da ponte, `seele.console`, que só escreve no registro.
///
/// **Escrito em JavaScript, e não em Rust**, pela mesma razão do outro: é
/// código que roda dentro do contexto do MOD, e um MOD pode redefinir o que
/// quiser depois dele. O que garante a fronteira é o contexto não ter ambiente,
/// e não este texto.
///
/// O teto de oito pedidos em voo é o mesmo, e existe aqui pela mesma razão que
/// existe lá: o MOD recebe o erro onde ele o escreveu.
pub const PRELUDIO: &str = r#"
'use strict';
(() => {
  // **A porta do registro, guardada antes de o MOD rodar.** O `seele` é um
  // objeto que o MOD alcança, e o `console` abaixo, lendo `seele.console` na
  // hora de escrever, deixava um `seele.console = () => {}` no topo do MOD
  // calar até o que o produto diz dele.
  const consoleDoAnfitriao = seele.console;
  const pendentes = new Map();
  let proximo = 0;

  let ouvinte = null;

  globalThis.aoResponder = (texto) => {
    let m;
    try { m = JSON.parse(texto); } catch { return; }
    if (m.tipo === 'evento') {
      if (!ouvinte) return;
      // O erro do MOD fica com o MOD: um ouvinte que lança não pode impedir o
      // próximo evento de chegar. A volta inteira falharia, e o anfitrião a
      // relataria como `Falhou` — que é o MOD com defeito, não este.
      //
      // **E é dito no registro**, primeiro: a mensagem vai à janela sem número
      // de pedido, e a janela a descarta. Antes da mensagem porque `String`
      // de um valor lançado pode lançar, e a linha não pode depender dela.
      try { ouvinte(m); } catch (erro) {
        dizerOErro('erro num ouvinte de evento:', erro);
        seele.postar(JSON.stringify({ tipo: 'erro-no-evento', erro: String(erro) }));
      }
      return;
    }
    const espera = pendentes.get(m.n);
    if (!espera) return;
    pendentes.delete(m.n);
    if (m.ok) espera.resolve(m.valor);
    else espera.reject(new Error(m.erro || 'recusado'));
  };

  // Quanto cabe numa mensagem, dito pelo anfitrião. Lido e apagado como as
  // capacidades, e pela mesma razão.
  const tetoDaMensagem = Number(globalThis.__seeleTetoDaMensagem) || 12288;
  delete globalThis.__seeleTetoDaMensagem;

  // Bytes, e não caracteres: o teto do anfitrião conta UTF-8, e um nome com
  // acento ocupa dois. Medir em `length` diria que cabe o que não cabe.
  //
  // **Contado aqui, sem `TextEncoder`.** A primeira versão o usava e recuava
  // para `texto.length` quando ele não existisse — e ele nunca existe: este
  // contexto é QuickJS sem ambiente, e `TextEncoder` é do navegador. O recuo
  // era a regra, e `length` mede unidades UTF-16. O reteste de `c4fe3ea` mediu
  // no motor real: sete mil «é» passaram pela conferência com 7.000 e chegaram
  // ao Rust com 14.066 bytes, que os recusou como `fila-cheia` de novo.
  //
  // A tabela do UTF-8, e ela é curta: até U+007F um byte, até U+07FF dois, um
  // par substituto vale um ponto de código acima de U+FFFF e ocupa quatro, e o
  // resto ocupa três.
  //
  // Um substituto solto conta três. Ele não chega aqui: o que se mede é a
  // saída de `JSON.stringify`, que escapa substitutos soltos como `\udXXX` —
  // seis caracteres ASCII. Se um motor não os escapar, três é o que uma
  // codificação permissiva escreveria, e a conferência do Rust continua atrás.
  const tamanhoEmBytes = (texto) => {
    let bytes = 0;
    for (let i = 0; i < texto.length; i += 1) {
      const c = texto.charCodeAt(i);
      if (c < 0x80) { bytes += 1; continue; }
      if (c < 0x800) { bytes += 2; continue; }
      if (c >= 0xD800 && c <= 0xDBFF && i + 1 < texto.length) {
        const baixo = texto.charCodeAt(i + 1);
        if (baixo >= 0xDC00 && baixo <= 0xDFFF) { bytes += 4; i += 1; continue; }
      }
      bytes += 3;
    }
    return bytes;
  };

  const pedir = (tipo, carga) => new Promise((resolve, reject) => {
    if (pendentes.size >= 8) { reject(new Error('too-many-requests')); return; }
    const texto = JSON.stringify({ tipo, n: proximo + 1, ...carga });
    // **«Não coube» é diferente de «a fila encheu».** As duas faziam `postar`
    // devolver `false`, e a única frase que existia era `fila-cheia`: um MOD
    // com uma declaração grande demais era mandado procurar saturação. Medido
    // aqui, o erro diz o tamanho, o teto e o tipo da mensagem — que é o que
    // leva a quem escreveu direto ao que ele precisa dividir.
    const bytes = tamanhoEmBytes(texto);
    if (bytes > tetoDaMensagem) {
      reject(new Error('mensagem-grande: ' + tipo + ' tem ' + bytes
        + ' bytes e o teto por mensagem é ' + tetoDaMensagem
        + '; mande menos de uma vez, ou monte só a parte que aparece'));
      return;
    }
    const n = ++proximo;
    pendentes.set(n, { resolve, reject });
    if (!seele.postar(texto)) {
      pendentes.delete(n);
      reject(new Error('fila-cheia'));
    }
  });

  // ---- o console ----
  //
  // **Um `console` que escreve no registro de quem hospeda.** Ele não
  // existia: o `console.warn` que o autor escreve justamente no caminho de
  // erro virava `ReferenceError`, e a falha que ele queria contar virava
  // outra, que não contava nada.
  //
  // O texto é montado aqui e cortado do outro lado: um MOD pode redefinir
  // tudo isto, e o que contém de verdade é `seele.console`. Nunca lança — um
  // `console` que derruba o caminho que ele observa é pior que nenhum.
  //
  // **O que o JSON não sabe dizer, dito de outro jeito.** Sozinho, ele diz
  // `null` para `NaN` e `±Infinity`, `{}` para `Map` e `Set`, some com o
  // `undefined` de um objeto (e o diz `null` numa lista) e lança num `bigint`
  // aninhado. `NaN` é o valor típico de um defeito de conta: um registro que
  // o diz `null` manda o autor procurar o que não existe. Aninhado, o valor
  // sai como texto, entre aspas; `Map` e `Set` saem como a lista do que têm.
  //
  // E ele diz `{}` para um `Error` (o nome e a mensagem não são enumeráveis),
  // e some com a função e o símbolo de um objeto: `console.log({ erro })`
  // saía `{"erro":{}}`. Aninhados, saem como `nome: mensagem`, `[função
  // nome]` e `Symbol(…)` — a pilha fica só para o `Error` passado inteiro ao
  // `console`, que tem a linha para ele.
  const funcaoEmTexto = (f) => '[função ' + (f.name || 'anônima') + ']';
  const semMentir = (_chave, v) => {
    if (typeof v === 'number' && !Number.isFinite(v)) return String(v);
    if (typeof v === 'bigint' || typeof v === 'symbol' || v === undefined) return String(v);
    if (typeof v === 'function') return funcaoEmTexto(v);
    if (v instanceof Error) return String(v.name) + ': ' + String(v.message);
    if (v instanceof Map || v instanceof Set) return [...v];
    return v;
  };
  // **O ciclo, dito onde ele está.** O JSON lança num ciclo, e o recuo era
  // `String(valor)`: um objeto com uma referência para o pai saía
  // `[object Object]`, sem um campo. Quando a primeira passada lança, uma
  // segunda troca por «[cíclico]» **só o que é ancestral do valor de agora**.
  //
  // Os ancestrais saem do `this` do substituto, que é o objeto que contém o
  // valor: a pilha desce até ele, e o que sobra nela é o caminho da raiz até
  // aqui. Não é um `WeakSet` de tudo o que já foi visto — ele marcaria como
  // cíclica a mesma referência em dois campos, que é repetição, e não ciclo.
  // Cada degrau guarda o valor como o MOD o tem e como `semMentir` o
  // devolveu: um `Map` vira uma lista nova, e é ela o `this` dos filhos dele.
  //
  // **E cada passada tem um orçamento de objetos.** Marcar só os ancestrais
  // faz o JSON percorrer todos os caminhos simples do grafo, e eles crescem
  // como (k−1)!: dez nós ligados todos a todos estouravam o prazo da volta, e
  // o `console.log` derrubava o MOD que ele observava (medido na revisão do
  // lote D). Um DAG pesa o mesmo na primeira passada, sem ciclo nenhum: antes
  // do lote, um de vinte nós, cada um ligado a todos os de depois, segurava a
  // thread do motor por 1,13 s e saía `[object Object]` — o MOD seguia, sem
  // `Falhou`, mas a volta passava em mais do dobro do prazo (medido na
  // re-revisão). Do objeto de número `OBJETOS_POR_PASSADA` em diante, a
  // passada diz `[…]`.
  //
  // **Mil e vinte e quatro, o dobro do que aparece.** Cada objeto que a
  // passada vê escreve ao menos um caractere antes de ela ver o seguinte (o
  // `{` dele, o `[` ou o texto que o troca), e o JSON sai na ordem em que os
  // vê: o de depois do teto começa além do milésimo caractere, e a linha é
  // cortada em 512 do outro lado — escapada, em menos ainda. **Os 512
  // caracteres que chegam ao `seele.log` não mudam; o número do aviso do
  // corte, sim.** O aviso conta o texto inteiro que o `console` montou, e
  // acima de 1024 objetos esse texto tem `[…]` no lugar do resto: o número
  // sai menor do que o objeto inteiro daria. O que o orçamento corta é o
  // trabalho de montar o que não chega: cada passada vê no máximo este tanto
  // de objetos, e procura cada um entre no máximo este tanto de ancestrais.
  // As duas passadas, juntas, o dobro.
  const OBJETOS_POR_PASSADA = 1024;
  const passada = (marcaCiclos) => {
    const ancestrais = [];
    let fundo = 0;
    let vistos = 0;
    return function (chave, v) {
      const dito = semMentir(chave, v);
      if (typeof dito !== 'object' || dito === null) return dito;
      if (vistos >= OBJETOS_POR_PASSADA) return '[…]';
      vistos += 1;
      if (!marcaCiclos) return dito;
      while (fundo > 0 && ancestrais[fundo - 1][1] !== this) fundo -= 1;
      for (let i = 0; i < fundo; i += 1) {
        if (ancestrais[i][0] === v) return '[cíclico]';
      }
      ancestrais[fundo] = [v, dito];
      fundo += 1;
      return dito;
    };
  };
  const comoJson = (valor, substituto) => {
    const json = JSON.stringify(valor, substituto);
    return json === undefined ? String(valor) : json;
  };
  // **Cada parte se protege sozinha.** Uma que não vira texto — um objeto
  // sem protótipo com um getter que lança não vira JSON em nenhuma das duas
  // passadas, nem `String`; um `name` com getter que lança — sai como um
  // recuo que diz isso. Sem ele, o erro subia até o `catch` de `escrever` e
  // levava a linha inteira, com o contexto que o autor escreveu antes da
  // parte que falhou.
  const emTexto = (valor) => {
    try {
      if (typeof valor === 'string') return valor;
      if (valor instanceof Error) {
        return String(valor.name) + ': ' + String(valor.message)
          + (valor.stack ? '\n' + String(valor.stack) : '');
      }
      if (valor === undefined) return 'undefined';
      if (typeof valor === 'function') return funcaoEmTexto(valor);
      if (typeof valor === 'bigint' || typeof valor === 'symbol') return String(valor);
      if (typeof valor === 'number') return String(valor);
      try { return comoJson(valor, passada(false)); } catch { /* um ciclo, ou um getter que lança */ }
      try { return comoJson(valor, passada(true)); } catch { /* o getter lançou de novo */ }
      return String(valor);
    } catch {
      return '[valor que não virou texto]';
    }
  };
  const escrever = (metodo) => (...partes) => {
    try { consoleDoAnfitriao(metodo, partes.map(emTexto).join(' ')); } catch { /* o registro é de quem hospeda */ }
  };
  const assertFalhou = escrever('error');
  // **O erro que o MOD não pegou num temporizador ou num ouvinte**, dito por
  // este `console`, que escreve pela porta guardada no começo do prelúdio: o
  // MOD que troca o `console.error` ou o `seele.console` depois não cala esta
  // linha. Não é imunidade: o texto passa por `map`, `join`, `String` e
  // `JSON.stringify`, que o MOD alcança pelos protótipos. Sai como qualquer
  // linha de console — com o id do MOD, o escape, o corte e o balde, que é o
  // que segura um `setInterval` de 4 ms que lança a cada batida.
  const dizerOErro = escrever('error');
  globalThis.console = {
    log: escrever('log'),
    info: escrever('info'),
    warn: escrever('warn'),
    error: escrever('error'),
    debug: escrever('debug'),
    // **Os outros métodos do navegador**, que quem escreve MOD traz de lá.
    // Sem eles, `console.table(…)` seria o mesmo defeito um passo adiante: um
    // `TypeError` que a volta inteira relata como «o MOD lançou». Os que
    // escrevem vão ao nível mais próximo; os que organizam a saída não têm o
    // que organizar num registro de linhas, e não fazem nada.
    dir: escrever('info'),
    table: escrever('info'),
    trace: escrever('debug'),
    assert: (ok, ...partes) => { if (!ok) assertFalhou('Assertion failed:', ...partes); },
    group: () => {},
    groupCollapsed: () => {},
    groupEnd: () => {},
    time: () => {},
    timeEnd: () => {},
    timeLog: () => {},
    count: () => {},
    countReset: () => {},
    clear: () => {},
  };

  globalThis.SeeleMods = Object.freeze({
    request: (id, canal, valor) => pedir('pedido', { id, canal, valor }),
    snapshot: () => pedir('snapshot', {}),
  });

  // ---- as capacidades desta execução ----
  //
  // **O que o pacote declarou, e não o que este build oferece.** O anfitrião
  // põe a lista em `__seeleCapacidades` antes de avaliar este texto, a partir
  // de `capacidades_da_api(manifesto.api)`.
  //
  // O vetor é lido e **apagado**: um MOD que o encontrasse depois poderia
  // reescrevê-lo, e o próximo a lê-lo leria o que o MOD escreveu. Apagar aqui
  // não é a fronteira — a fronteira é o anfitrião conferir cada mensagem —, é
  // não deixar uma pergunta com duas respostas.
  const capacidades = new Set(Array.isArray(globalThis.__seeleCapacidades)
    ? globalThis.__seeleCapacidades
    : []);
  delete globalThis.__seeleCapacidades;

  /**
   * Monta um objeto só com o que esta API tem.
   *
   * **Ausente, e não recusado em tempo de execução.** Um `superficies` que
   * existisse e sempre falhasse faria um MOD de API 3 descobrir o problema
   * dentro de um `catch`, com uma mensagem, em produção. Um método que não
   * existe é um `TypeError` na primeira linha que o chama, com pilha, onde
   * quem escreveu o MOD consegue ver.
   */
  const comCapacidade = (nome, membros) => (capacidades.has(nome) ? membros : null);

  const superficies = comCapacidade('superficies', Object.freeze({
    // Devolve um punho local: os métodos serializam pedidos, e nada aqui é um
    // nó do documento. §3 do plano.
    criar: async (descricao) => {
      const id = String(descricao && descricao.id || '');
      await pedir('superficie-criar', { descricao });
      return Object.freeze({
        id,
        montar: (arvore) => pedir('superficie-montar', { superficie: id, arvore }),
        classes: (mapa) => pedir('superficie-classes', { superficie: id, classes: mapa }),
        mostrar: () => pedir('superficie-mostrar', { superficie: id }),
        ocultar: () => pedir('superficie-ocultar', { superficie: id }),
        suja: (valor) => pedir('superficie-suja', { superficie: id, suja: valor !== false }),
        titulo: (texto) => pedir('superficie-titulo', { superficie: id, titulo: texto }),
        fechar: (motivo) => pedir('superficie-fechar', { superficie: id, motivo }),
        descartar: () => pedir('superficie-descartar', { superficie: id }),
      });
    },
    // Um aviso é uma superfície de vida curta, e por isso tem atalho: um MOD
    // que precise dizer «gravado» não deve ter de montar uma janela.
    avisar: (texto, tom) => pedir('superficie-criar', {
      descricao: { id: 'aviso-' + (++proximo), tipo: 'aviso', titulo: String(texto ?? ''), tom },
    }),
  }));

  const contribuicoes = comCapacidade('contribuicoes', Object.freeze({
    registrar: (pedido) => pedir('contribuir', { pedido }),
    revogar: (handle) => pedir('revogar-contribuicao', { handle }),
  }));

  globalThis.SeeleUI = Object.freeze({
    // **Pela capacidade, como o resto.** A API 6 não tem `regiao` nem
    // `cartoes` (decisão do dono de 08/10), e eles faltam do `SeeleUI` dela
    // como `superficies` falta do da 3: ausentes, e não montados para a
    // janela recusar depois. As APIs 3 a 5 têm os dois.
    ...(capacidades.has('regiao') ? { regiao: (conteudo) => pedir('regiao', { conteudo }) } : {}),
    tema: (valores) => pedir('tema', { valores }),
    ...(superficies ? { superficies } : {}),
    ...(contribuicoes ? { contribuicoes } : {}),
    // A lista, legível pelo próprio MOD: um pacote que queira degradar em vez
    // de falhar precisa poder perguntar, e perguntar a `SeeleUI.superficies`
    // já é a resposta — mas um nome explícito evita o truque.
    capacidades: () => [...capacidades],

    // **Um cartão por pessoa na lista do produto.** A declaração é a mesma da
    // região, montada pelo mesmo renderer, com uma gramática menor: nada que
    // receba foco ou clique, porque a linha do roster já tem um botão do
    // produto. É a única superfície fora da região, e ela sai com ela.
    ...(capacidades.has('cartoes') ? { cartoes: (cartoes) => pedir('cartoes', { cartoes }) } : {}),
    // ---- os eventos ----
    //
    // **A janela fala com o MOD sem que ele tenha perguntado.** Um pedido tem
    // número e resposta; um evento não tem nem um nem outro, porque quem
    // digita não espera o MOD confirmar que recebeu a tecla.
    //
    // Um ouvinte só, e o último vence. Uma lista de ouvintes seria um MOD
    // registrando dentro de um laço e a janela mantendo a lista viva; e
    // «remover» exigiria devolver um cancelador que um MOD pode perder.
    aoEvento: (fn) => { ouvinte = typeof fn === 'function' ? fn : null; },

    // ---- o arquivo que alguém escolheu ----
    //
    // O MOD **não** abre o seletor: ele declara a forma `arquivo`, a pessoa
    // aperta, e o evento traz o identificador. Daqui ele lê os bytes em
    // pedaços e os manda para o servidor dele pelo protocolo que ele já tem.
    //
    // Sem esta função, um MOD teria o identificador e nada para fazer com ele.
    // Com acesso ao disco, ele teria o caminho errado — e nenhum cliente do
    // SEELE abre arquivo por conta de terceiro.
    ...(capacidades.has('volume') ? { enviar: (arquivo, token) => pedir('enviar-imagem', { arquivo, token }) } : {}),
    pedaco: (arquivo, inicio) => pedir('pedaco', { arquivo, inicio: Number(inicio) || 0 }),
    soltar: (arquivo) => pedir('soltar-arquivo', { arquivo }),
  });

  // ---- tempo ----
  //
  // O QuickJS não tem laço de eventos, então `setTimeout` e `setInterval` não
  // existem aqui. Quem tem relógio é a thread do anfitrião, e estas quatro
  // funções são a fachada dela.
  //
  // **Os pedidos vão por um vetor, e não por chamada nativa.** Uma função
  // nativa chamada de dentro do motor teria de travar a tabela enquanto o
  // motor roda — e o motor pode estar a meio de uma interrupção. O anfitrião
  // recolhe este vetor quando a volta termina.
  const relogios = new Map();
  let pedidosDeRelogio = [];
  let proximoRelogio = 0;

  // **O mesmo teto do anfitrião, aqui também.** Sem isto, a fachada guardava o
  // callback de um temporizador que o anfitrião tinha ignorado: `setTimeout`
  // devolvia um número, o MOD acreditava que estava agendado, e ele nunca
  // disparava. Zero é o «não deu», e é o que um MOD confere.
  const TETO = 256;
  const marcar = (fn, ms, repete) => {
    if (typeof fn !== 'function') return 0;
    if (relogios.size >= TETO) return 0;
    const id = ++proximoRelogio;
    relogios.set(id, { fn, repete });
    pedidosDeRelogio.push({ id, ms: Number(ms) || 0, repete, cancelar: false });
    return id;
  };
  const desmarcar = (id) => {
    if (!relogios.delete(id)) return;
    pedidosDeRelogio.push({ id, ms: 0, repete: false, cancelar: true });
  };

  globalThis.setTimeout = (fn, ms) => marcar(fn, ms, false);
  globalThis.setInterval = (fn, ms) => marcar(fn, ms, true);
  globalThis.clearTimeout = desmarcar;
  globalThis.clearInterval = desmarcar;

  // Chamado pelo anfitrião quando um temporizador vence.
  globalThis.__seeleRelogio = (id) => {
    const marca = relogios.get(id);
    if (!marca) return;
    if (!marca.repete) relogios.delete(id);
    // **O erro do MOD fica com o MOD.** Um `setInterval` que lança não pode
    // parar o anfitrião, e a repetição continua: quem decide parar é quem
    // cancela, e não um erro de uma volta. E é dito no registro, antes da
    // mensagem à janela, pela mesma razão do ouvinte de eventos.
    try { marca.fn(); } catch (erro) {
      dizerOErro('erro num temporizador:', erro);
      seele.postar(JSON.stringify({ tipo: 'erro-no-relogio', erro: String(erro) }));
    }
  };

  // E o anfitrião recolhe o que se acumulou, esvaziando.
  globalThis.__seelePedidosDeRelogio = () => {
    const meus = pedidosDeRelogio;
    pedidosDeRelogio = [];
    return JSON.stringify(meus);
  };
})();
"#;
