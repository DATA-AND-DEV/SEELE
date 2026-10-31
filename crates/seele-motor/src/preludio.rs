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

  // **O bilhete de gesto da volta em curso**: o `gesto` do evento `botao` que o
  // ouvinte está recebendo agora, e `null` em qualquer outro momento. O MOD de
  // jogo só abre uma porta de dentro de um clique, e o clique chega aqui como
  // este bilhete, emitido pela casca; fora do ouvinte (depois de um `await`,
  // num temporizador) ele não existe, e `SeeleUI.tunel.entrar` sai com
  // `gesto: null`. Quem confere o bilhete é a casca (prazo de 2 s, uso único);
  // aqui ele só é carregado, e solto ao fim da volta.
  let gestoDaVolta = null;

  // **Os dois ouvintes da metade de máquina** (API 8 parte 3), e a marca de
  // que a parada já foi dada. Só a metade de máquina os registra (`maquina`
  // existe só com a capacidade `maquina`); na de janela ficam nulos, e uma
  // mensagem desses tipos que chegasse lá não acharia ninguém.
  let ouvinteDoAviso = null;
  let ouvinteDaParada = null;
  let paradaDada = false;
  // **A volta de quem hospeda** (API 8, «sem quem hospeda»): a metade seguiu
  // rodando sem ele e foi religada pelo pedido igual ao aceito, sem subir de
  // novo. É a vez de ela servir de novo o túnel e perguntar quem é a dona.
  let ouvinteDaVolta = null;

  // **O que o poder `programa` ouve sem ter pedido** (API 8 parte 4): o
  // progresso de um download (`programa-progresso`), a linha de um processo
  // (`processo-linha`) e o fim dele (`processo-fim`). Montado só com a
  // capacidade `maquina.programa`; sem ela fica nulo, e essas mensagens não
  // acham ninguém.
  let aoProgramaChegou = null;

  globalThis.aoResponder = (texto) => {
    let m;
    try { m = JSON.parse(texto); } catch { return; }
    if (m.tipo === 'aviso-maquina') {
      // O erro de quem ouve fica com quem ouve, e é dito: um `aoAviso` que
      // lança não pode impedir o próximo aviso de chegar, nem derrubar a
      // metade inteira por um defeito de um ouvinte.
      if (!ouvinteDoAviso) return;
      try { ouvinteDoAviso(m.carga); } catch (erro) { dizerOErro('erro no aoAviso:', erro); }
      return;
    }
    if (m.tipo === 'reconectar-maquina') {
      // O erro de quem ouve fica com quem ouve, como no `aoAviso`; uma
      // promessa que rejeita também é dita.
      if (!ouvinteDaVolta) return;
      const volta = Object.freeze({ fora: Number(m.fora ?? 0) });
      try {
        Promise.resolve(ouvinteDaVolta(volta)).catch((erro) => { dizerOErro('erro no aoReconectar:', erro); });
      } catch (erro) { dizerOErro('erro no aoReconectar:', erro); }
      return;
    }
    if (m.tipo === 'parar-maquina') {
      // **Uma vez só**, e a resposta sai quando o `aoParar` acabar — inclusive
      // o que ele esperou (uma promessa devolvida). É por esta mensagem que o
      // anfitrião sabe que pode encerrar antes dos 2 s do teto; sem ela, toda
      // parada custaria o teto inteiro.
      if (paradaDada) return;
      paradaDada = true;
      const quem = ouvinteDaParada;
      Promise.resolve()
        .then(() => (quem ? quem(String(m.motivo ?? '')) : undefined))
        .catch((erro) => { dizerOErro('erro no aoParar:', erro); })
        .finally(() => { seele.postar(JSON.stringify({ tipo: 'maquina-parada-pronta' })); });
      return;
    }
    if (m.tipo === 'programa-progresso' || m.tipo === 'processo-linha' || m.tipo === 'processo-fim') {
      if (aoProgramaChegou) aoProgramaChegou(m);
      return;
    }
    if (m.tipo === 'evento') {
      if (!ouvinte) return;
      // Só o `botao` carrega bilhete: um `gesto` num evento de outro nome não é
      // da casca, e não vale.
      gestoDaVolta = (m.nome === 'botao' && typeof m.gesto === 'string') ? m.gesto : null;
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
      } finally {
        gestoDaVolta = null;
      }
      return;
    }
    const espera = pendentes.get(m.n);
    if (!espera) return;
    pendentes.delete(m.n);
    if (m.ok) {
      espera.resolve(m.valor);
      return;
    }
    // **A recusa leva os campos dela** (rascunho da API 6, §7.1). O nome vai
    // em `message` e em `erro`, e o resto da resposta — o `detectado: 'heic'`
    // de um `tipo-recusado` — vai como propriedade do `Error`, que é como o
    // MOD o lê num `catch`. O `ok`, o número do pedido e o tipo da mensagem são
    // da ponte, e ficam de fora.
    const recusa = new Error(m.erro || 'recusado');
    for (const [campo, valor] of Object.entries(m)) {
      if (campo === 'ok' || campo === 'n' || campo === 'tipo' || campo === '__proto__') continue;
      recusa[campo] = valor;
    }
    recusa.erro = m.erro || 'recusado';
    espera.reject(recusa);
  };

  // Quanto cabe numa mensagem, dito pelo anfitrião. Lido e apagado como as
  // capacidades, e pela mesma razão.
  const tetoDaMensagem = Number(globalThis.__seeleTetoDaMensagem) || 12288;
  delete globalThis.__seeleTetoDaMensagem;

  // **O `Atomics` sai, nos dois alvos** (rascunho da API 6, §16, item 5). Era
  // a única global que o motor nativo tinha e o `.wasm` não: o quickjs-ng o
  // compila sem threads no `wasm32-wasip1`, e sem threads ele não serve para
  // nada em nenhum dos dois. Um MOD que dependesse dele rodaria no desktop e
  // quebraria no celular. No `.wasm` este `delete` não acha nada; a sonda de
  // `tests/casos.json` cobra os dois.
  delete globalThis.Atomics;

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

  // **Uma recusa do próprio prelúdio tem a forma de toda recusa** (rascunho
  // da API 6, §7.1; a auditoria do `v6.json`, B2): o nome em `message` e em
  // `erro`, e o resto em campos. Estas três saíam com o nome só em `message`,
  // e a de tamanho com uma frase no lugar do nome: o MOD que lesse `e.erro`,
  // como lê toda outra recusa, recebia `undefined`.
  const recusaDoPreludio = (nome, campos) => {
    const recusa = new Error(nome);
    if (campos) Object.assign(recusa, campos);
    recusa.erro = nome;
    return recusa;
  };

  const pedir = (tipo, carga) => new Promise((resolve, reject) => {
    if (pendentes.size >= 8) { reject(recusaDoPreludio('too-many-requests', { teto: 8 })); return; }
    const texto = JSON.stringify({ tipo, n: proximo + 1, ...carga });
    // **«Não coube» é diferente de «a fila encheu».** As duas faziam `postar`
    // devolver `false`, e a única frase que existia era `fila-cheia`: um MOD
    // com uma declaração grande demais era mandado procurar saturação. Medido
    // aqui, o erro diz o tamanho, o teto e o tipo da mensagem — que é o que
    // leva a quem escreveu direto ao que ele precisa dividir.
    const bytes = tamanhoEmBytes(texto);
    if (bytes > tetoDaMensagem) {
      // A frase vai ao registro de quem hospeda; ao MOD, o nome e os números.
      dizerOAviso('mensagem-grande: ' + tipo + ' tem ' + bytes
        + ' bytes e o teto por mensagem é ' + tetoDaMensagem
        + '; mande menos de uma vez, ou monte só a parte que aparece');
      reject(recusaDoPreludio('mensagem-grande', { tipoPedido: tipo, tamanho: bytes, teto: tetoDaMensagem }));
      return;
    }
    const n = ++proximo;
    pendentes.set(n, { resolve, reject });
    if (!seele.postar(texto)) {
      pendentes.delete(n);
      reject(recusaDoPreludio('fila-cheia'));
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
  const dizerOAviso = escrever('warn');
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

  // **Sem `snapshot`.** O `Snapshot` inteiro era das APIs 3 a 5, que este
  // build não executa (`APIS_ACEITAS = [6]`); a 6 lê `SeeleUI.retrato()`,
  // projetado por lista branca (rascunho da API 6, §6). Ele saía pela falta da
  // capacidade `retrato`, e um pacote sem ela ainda o ganhava: agora não há
  // caminho que o monte (a limpeza da máquina das APIs 3 a 5, depois do corte).
  // **A metade de máquina não tem tela** (API 8 parte 3): com a capacidade
  // `maquina`, nem `SeeleMods` nem `SeeleUI` existem, e o que existe é o
  // global `maquina`. Ausentes, e não recusados: um `SeeleUI.superficies` que
  // existisse e sempre falhasse faria o autor descobrir num `catch`, em
  // produção, que a metade dele não tem onde desenhar.
  const daMaquina = capacidades.has('maquina');

  if (!daMaquina) globalThis.SeeleMods = Object.freeze({
    request: (id, canal, valor) => pedir('pedido', { id, canal, valor }),
    // **A ação declarada** (API 7, capacidade `acoes`): o `request` que nomeia
    // uma ação do `permissoes` do `mod.json`. Quem confere o MOD ligado, a
    // ação declarada e a permissão de quem pede é o servidor, antes da metade
    // de servidor rodar; a recusa volta como valor, `{ ok: false, recusa:
    // 'acao-nao-declarada' | 'sem-permissao' }`, como toda resposta do MOD.
    ...(capacidades.has('acoes') ? {
      acao: (id, canal, acao, valor) => pedir('acao', { id, canal, acao, valor }),
    } : {}),
  });

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
        // **A montagem por partes** (rascunho da API 6, §12): troca o nó de
        // `chave` e o que está sob ele, numa mensagem própria. Uma ficha que
        // não cabe nos 12 KiB de uma mensagem se monta num esqueleto e numa
        // parte por vez; o teto que vale é o de nós da superfície.
        atualizar: (chave, arvore) => pedir('superficie-atualizar', { superficie: id, chave, arvore }),
        // **A recusa do servidor de volta aos campos** (rascunho da API 6,
        // §5): `{ chave: frase }`, um campo ou vários de uma vez; `{}` limpa.
        formulario: Object.freeze({
          erros: (mapa) => pedir('formulario-erros', { superficie: id, erros: mapa }),
        }),
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
    // Troca o conteúdo sem revogar: o produto não redesenha o nativo no meio
    // (R7 do PERFIS; tarefa 5 da F3, Step 6).
    atualizar: (handle, conteudo) => pedir('atualizar-contribuicao', { handle, conteudo }),
  }));

  // **O túnel de jogo** (API 8, poder `porta`): ausente para quem não o
  // declara, como toda capacidade. `entrar` leva o bilhete de gesto da volta em
  // que foi chamado (`gestoDaVolta`), lido **na hora da chamada**: um `entrar`
  // depois de um `await` sai com `gesto: null` e a casca o recusa como
  // `fora-de-um-clique`. Resolve com o número da porta em `127.0.0.1`.
  const tunel = comCapacidade('tunel', Object.freeze({
    // `async`, como os membros de `superficies`: o bilhete é lido **agora**, na
    // chamada, e a promessa é a da resposta.
    entrar: async (nome) => pedir('tunel-entrar', { nome: String(nome ?? ''), gesto: gestoDaVolta }),
  }));

  // **`SeeleUI.salvar`** (API 8 parte 6; spec §6), do mesmo poder `porta`:
  // quem joga recebe um arquivo que a metade de servidor guardou (o
  // `.mrpack`) pelo diálogo de salvar do sistema. `pedido` vai ao `aoPedir`
  // dela, que autoriza respondendo `{ ok, volume }`; `nome` é só a sugestão —
  // o caminho é da pessoa e não volta. Como o túnel, só dentro de um clique
  // (o bilhete da volta). Resolve com `{ bytes }`, ou `null` se a pessoa
  // cancelou.
  const salvar = capacidades.has('salvar')
    ? async (arquivo) => pedir('salvar', {
      canal: Number(arquivo?.canal) || 0,
      pedido: arquivo?.pedido ?? null,
      nome: String(arquivo?.nome ?? ''),
      gesto: gestoDaVolta,
    })
    : null;

  if (!daMaquina) globalThis.SeeleUI = Object.freeze({
    // **Sem `regiao` e sem `cartoes`** (decisão do dono de 08/10): a região é
    // um ponto de contribuição, e o cartão de uma pessoa é o `pessoa.cartao`.
    // Os dois eram das APIs 3 a 5, que este build não executa, e saíram do
    // prelúdio com a máquina delas na janela.
    // Na 6, a janela só faz prévia (rascunho §9, B4): `{ previa: true,
    // superficie }`. Até a 5, o tema vai como sempre, e a janela ignora os dois.
    ...(capacidades.has('tema') ? {
      tema: (valores, opcoes) => pedir('tema', {
        valores,
        previa: Boolean(opcoes && opcoes.previa === true),
        superficie: opcoes && opcoes.superficie,
      }),
    } : {}),
    ...(capacidades.has('retrato') ? { retrato: () => pedir('retrato', {}) } : {}),
    ...(superficies ? { superficies } : {}),
    ...(contribuicoes ? { contribuicoes } : {}),
    ...(tunel ? { tunel } : {}),
    ...(salvar ? { salvar } : {}),
    // A lista, legível pelo próprio MOD: um pacote que queira degradar em vez
    // de falhar precisa poder perguntar, e perguntar a `SeeleUI.superficies`
    // já é a resposta — mas um nome explícito evita o truque.
    capacidades: () => [...capacidades],

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
    //
    // **Pela capacidade `arquivo`, como o anfitrião confere** (a auditoria do
    // `v6.json`, o espelho do prelúdio com o roteador): no navegador a
    // plataforma não tem o seletor, e as duas existiam e sempre falhavam.
    ...(capacidades.has('arquivo') ? {
      pedaco: (arquivo, inicio) => pedir('pedaco', { arquivo, inicio: Number(inicio) || 0 }),
      soltar: (arquivo) => pedir('soltar-arquivo', { arquivo }),
    } : {}),
  });

  // ---- a metade de máquina ----
  //
  // **O que foi aceito, e só isso.** O anfitrião põe em `__seeleMaquina` o
  // servidor de jogo e a oferta que o dono da máquina aceitou (em JSON), e
  // ela chega aqui congelada até o fundo: a metade lê o que pode servir, e
  // reescrever a cópia não mudaria nada — quem confere `porta.servir` contra a
  // oferta é o Rust, com a cópia dele. Lido e apagado, como as capacidades.
  if (daMaquina) {
    let dados = {};
    try { dados = JSON.parse(String(globalThis.__seeleMaquina ?? '{}')) || {}; } catch { dados = {}; }
    delete globalThis.__seeleMaquina;
    const congelar = (valor) => {
      if (valor && typeof valor === 'object') {
        for (const dentro of Object.values(valor)) congelar(dentro);
        Object.freeze(valor);
      }
      return valor;
    };
    // **O poder `porta`, só para quem o declara**: ausente sem ele, como
    // toda capacidade. As três resolvem com o que o anfitrião respondeu; a
    // recusa chega com o nome em `erro` (`fora-do-aceite`, ou o de
    // `RecusaDeTunel` que o servidor disse).
    const porta = capacidades.has('maquina.porta') ? Object.freeze({
      servir: async (tunel) => {
        await pedir('porta-servir', {
          nome: String(tunel?.nome ?? ''),
          protocolo: String(tunel?.protocolo ?? ''),
          porta: Number(tunel?.porta),
        });
      },
      parar: async (nome) => { await pedir('porta-parar', { nome: String(nome ?? '') }); },
      quedas: (nome) => pedir('porta-quedas', { nome: String(nome ?? '') }),
    }) : null;
    // **O poder `programa`, só para quem o declara** (API 8 parte 4). Tudo
    // aqui é um pedido ao anfitrião, que confere contra o aceite guardado
    // (a cópia que vale é a do Rust, e não `maquina.oferta`). A recusa chega
    // como `Error` com o nome em `erro` e os dados dela em campos (`host`,
    // `aceitos`, `esperado`, `obtido`, `usado`, `teto`...), como toda recusa.
    const programa = capacidades.has('maquina.programa') ? (() => {
      // O progresso de cada download, pelo número do pedido que o começou.
      const progressos = new Map();
      // Cada processo, pelo número que o anfitrião deu. Uma linha que chega
      // antes de o objeto existir (a resposta de `rodar` ainda não virou
      // objeto) ou antes de `aoLinha` fica guardada, até TETO_GUARDADAS; o
      // que passa disso é contado e dito no fim (`linhasNaoOuvidas`).
      const processos = new Map();
      const TETO_GUARDADAS = 256;
      const doProcesso = (numero) => {
        let p = processos.get(numero);
        if (!p) {
          p = { aoLinha: null, aoSair: null, guardadas: [], naoOuvidas: 0, fim: null, resolverFim: null };
          p.promessa = new Promise((resolve) => { p.resolverFim = resolve; });
          processos.set(numero, p);
        }
        return p;
      };
      const ouvirLinha = (p, linha, fluxo) => {
        try { p.aoLinha(linha, fluxo); } catch (erro) { dizerOErro('erro no aoLinha:', erro); }
      };
      const terminar = (p) => {
        if (p.aoSair) {
          try { p.aoSair(p.fim); } catch (erro) { dizerOErro('erro no aoSair:', erro); }
        }
        p.resolverFim(p.fim);
      };
      aoProgramaChegou = (m) => {
        if (m.tipo === 'programa-progresso') {
          const fn = progressos.get(m.n);
          if (!fn) return;
          try { fn({ recebidos: Number(m.recebidos), total: Number(m.total) }); } catch (erro) { dizerOErro('erro no aoProgresso:', erro); }
          return;
        }
        const p = doProcesso(Number(m.processo));
        if (m.tipo === 'processo-linha') {
          const linha = String(m.linha ?? '');
          const fluxo = m.fluxo === 'erro' ? 'erro' : 'saida';
          if (p.aoLinha) ouvirLinha(p, linha, fluxo);
          else if (p.guardadas.length < TETO_GUARDADAS) p.guardadas.push([linha, fluxo]);
          else p.naoOuvidas += 1;
          return;
        }
        // processo-fim: uma vez só.
        if (p.fim) return;
        p.fim = Object.freeze({
          codigo: m.codigo ?? null,
          mortoAPedido: m.mortoAPedido === true,
          linhasPerdidas: Number(m.linhasPerdidas) || 0,
          linhasCortadas: Number(m.linhasCortadas) || 0,
          linhasNaoOuvidas: p.naoOuvidas,
        });
        processos.delete(Number(m.processo));
        terminar(p);
      };
      const objetoDoProcesso = (r) => {
        const numero = Number(r && r.processo);
        const p = doProcesso(numero);
        return Object.freeze({
          numero,
          pid: (r && r.pid) ?? null,
          // Um ouvinte de cada, e o último vence, como `aoAviso`. Ao registrar,
          // o que estava guardado chega primeiro, na ordem.
          aoLinha: (fn) => {
            p.aoLinha = typeof fn === 'function' ? fn : null;
            if (!p.aoLinha) return;
            const guardadas = p.guardadas.splice(0);
            for (const [linha, fluxo] of guardadas) ouvirLinha(p, linha, fluxo);
          },
          aoSair: (fn) => {
            p.aoSair = typeof fn === 'function' ? fn : null;
            // Já tinha saído: o ouvinte novo ouve o fim que passou.
            if (p.aoSair && p.fim) {
              try { p.aoSair(p.fim); } catch (erro) { dizerOErro('erro no aoSair:', erro); }
            }
          },
          // A mesma saída, como promessa.
          fim: p.promessa,
          escrever: (texto) => pedir('processo-escrever', { processo: numero, texto: String(texto ?? '') }),
          matar: () => pedir('processo-matar', { processo: numero }),
        });
      };
      const hashes = (pedido) => {
        const saida = {};
        for (const algoritmo of ['sha1', 'sha256', 'sha512']) {
          if (pedido && pedido[algoritmo] !== undefined) saida[algoritmo] = String(pedido[algoritmo]);
        }
        return saida;
      };
      return Object.freeze({
        // Resolve quando o arquivo chegou e bateu: `{ destino, tamanho,
        // algoritmo, hash }`. `aoProgresso({ recebidos, total })` é chamado
        // enquanto ele corre.
        baixar: (pedido) => {
          const n = proximo + 1;
          if (typeof (pedido && pedido.aoProgresso) === 'function') progressos.set(n, pedido.aoProgresso);
          return pedir('programa-baixar', {
            url: String(pedido?.url ?? ''),
            ...hashes(pedido),
            tamanho: Number(pedido?.tamanho),
            destino: String(pedido?.destino ?? ''),
          }).finally(() => { progressos.delete(n); });
        },
        escrever: (pedido) => pedir('programa-escrever', {
          destino: String(pedido?.destino ?? ''),
          texto: String(pedido?.texto ?? ''),
        }),
        // `mesclar: true` (API 8 parte 6) entra por cima do que já está no
        // destino, arquivo a arquivo (os `overrides` de um modpack); sem ele,
        // o destino inteiro é trocado.
        desempacotar: (pedido) => pedir('programa-desempacotar', {
          arquivo: String(pedido?.arquivo ?? ''),
          destino: String(pedido?.destino ?? ''),
          raiz: String(pedido?.raiz ?? ''),
          mesclar: pedido?.mesclar === true,
        }),
        // **O pack da metade de servidor** (API 8 parte 6): `pedido` vai ao
        // `aoPedir` dela, que autoriza respondendo `{ ok, volume }`; os bytes
        // vêm pelo fluxo de volume, e o anfitrião só os grava se tiverem o
        // hash do pack aceito. Resolve com `{ destino, tamanho, hash,
        // jaEstava }`.
        trazer: (pedido) => pedir('programa-trazer', {
          pedido: pedido?.pedido ?? null,
          destino: String(pedido?.destino ?? ''),
        }),
        // Um texto da pasta do servidor de jogo (o índice do pack), até 4 MiB.
        ler: (pedido) => pedir('programa-ler', { arquivo: String(pedido?.arquivo ?? '') }),
        rodar: async (pedido) => objetoDoProcesso(await pedir('programa-rodar', {
          comando: String(pedido?.comando ?? ''),
          argumentos: Array.isArray(pedido?.argumentos) ? pedido.argumentos.map(String) : [],
          pasta: String(pedido?.pasta ?? ''),
        })),
        // **A cópia do mundo** (API 8 parte 7). `empacotar` faz um `.tar.gz`
        // de uma pasta (depois do salvamento seguro da receita) e resolve com
        // `{ arquivo, tamanho, sha256, arquivos, bytes, linksPulados }`;
        // `oferecerCopia` o serve na porta de um túnel TCP aceito e resolve com
        // `{ codigo, tamanho, sha256, porta }`; `receberCopia` entra no túnel
        // `tunel` da oferta, recebe, confere o sha256 e retoma, e resolve com
        // `{ destino, tamanho, sha256, retomadas, jaEstavam, caminho }`.
        empacotar: (pedido) => pedir('programa-empacotar', {
          origem: String(pedido?.origem ?? ''),
          arquivo: String(pedido?.arquivo ?? ''),
        }),
        oferecerCopia: (pedido) => pedir('programa-oferecer-copia', {
          arquivo: String(pedido?.arquivo ?? ''),
          porta: Number(pedido?.porta),
        }),
        receberCopia: (pedido) => {
          const n = proximo + 1;
          if (typeof (pedido && pedido.aoProgresso) === 'function') progressos.set(n, pedido.aoProgresso);
          return pedir('programa-receber-copia', {
            tunel: String(pedido?.tunel ?? ''),
            codigo: String(pedido?.codigo ?? ''),
            sha256: String(pedido?.sha256 ?? ''),
            tamanho: Number(pedido?.tamanho),
            destino: String(pedido?.destino ?? ''),
          }).finally(() => { progressos.delete(n); });
        },
        // `{ nome: vezes }`: cada recusa desde que esta metade subiu.
        recusas: () => pedir('programa-recusas', {}),
      });
    })() : null;
    globalThis.maquina = Object.freeze({
      servidor: String(dados.servidor ?? ''),
      oferta: congelar(dados.oferta ?? null),
      // O sistema desta máquina, no nome das receitas (`mac-aarch64`...): é o
      // que escolhe, entre os artefatos da oferta, os que esta máquina baixa.
      plataforma: String(dados.plataforma ?? ''),
      // À metade de servidor do MOD, pelo servidor; resolve com o texto que
      // ela respondeu, e rejeita com `sem-aceite` ou `sem-resposta`.
      pedir: (valor) => pedir('maquina-pedir', { valor }),
      // Um ouvinte de cada, e o último vence — a mesma regra de
      // `SeeleUI.aoEvento`, pela mesma razão.
      aoAviso: (fn) => { ouvinteDoAviso = typeof fn === 'function' ? fn : null; },
      aoParar: (fn) => { ouvinteDaParada = typeof fn === 'function' ? fn : null; },
      // `fn({ fora })`: quantos segundos a metade seguiu sem quem hospeda.
      aoReconectar: (fn) => { ouvinteDaVolta = typeof fn === 'function' ? fn : null; },
      ...(porta ? { porta } : {}),
      ...(programa ? { programa } : {}),
    });
  }

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
