// O ciclo de vida de um MOD, **sem saber o que executa o código dele**.
//
// Etapa E2 do contrato de API própria, e a diretriz de 18/09 é a razão de este
// arquivo existir separado: «manter a implementação atrás de um contrato
// interno de executor: iniciar, entregar evento, solicitar encerramento e
// confirmar encerramento».
//
// # Por que a separação não é organização
//
// O Worker de Blob **não vai ser o executor**. A sonda de fronteira releu, num
// aplicativo reiniciado, uma marca que um MOD tinha gravado em IndexedDB — e
// armazenamento que sobrevive à sessão é o contrário da promessa que este
// produto faz. O próximo experimento é QuickJS nativo.
//
// Se o ciclo de vida estivesse escrito em cima de `Worker`, trocar o executor
// seria reescrever o ciclo de vida junto, e a parte difícil — revogação,
// corrida de saída, descarte — voltaria ao começo. Escrito contra um contrato,
// a troca é um arquivo.
//
// # O que um executor tem de saber fazer
//
// Quatro verbos, e nenhum deles é «o MOD cooperou»:
//
//   iniciar(fonte, aoReceber)   sobe o código e chama `aoReceber` a cada mensagem
//   entregar(mensagem)          manda uma mensagem para dentro
//   pedirEncerramento()         começa a parar
//   encerrou()                  promessa que resolve quando parou de verdade
//
// O quarto é o que a diretriz separa do resto: «a conclusão do executor é
// diferente da revogação: até confirmar o descarte dos recursos, não anunciar
// que tudo foi limpo». Revogar é imediato e é nosso; parar é do executor e leva
// o tempo que levar.

/**
 * Os quatro estados de uma instância — §5 do contrato.
 *
 * `criando` é antes de o código subir; `ativa` é o único em que um efeito é
 * admitido; `encerrando` é depois do pedido de parada e antes da confirmação; e
 * `encerrada` é depois dela. A revogação é **monotônica**: nenhum caminho volta
 * de `encerrando` ou `encerrada` para `ativa`.
 */
const ESTADOS_DE_MOD = Object.freeze({
  criando: "criando",
  ativa: "ativa",
  encerrando: "encerrando",
  encerrada: "encerrada",
});

/**
 * O executor que existe hoje: um `Worker` de `blob:`.
 *
 * **Provisório, e escrito como provisório.** Ele exercita os caminhos que já
 * existem e não comprova isolamento nenhum — a sonda mediu o que ele deixa
 * aberto. Está aqui atrás do contrato para que a infraestrutura de E2 possa ser
 * construída e provada antes de E1 ser resolvida.
 */
function executorDeWorker(preludio) {
  let worker = null;
  let parou = false;
  return {
    nome: "worker-blob",
    // `async` para casar com o outro executor, e não porque ela espera algo:
    // quem chama precisa poder `await` sem saber qual dos dois respondeu.
    async iniciar(codigo, aoReceber, aoFalhar) {
      const fonte = new Blob([preludio, "\n", codigo], { type: "text/javascript" });
      const endereco = URL.createObjectURL(fonte);
      worker = new Worker(endereco);
      // Revogado assim que o worker o leu: o endereço não precisa sobreviver, e
      // um que sobrevive é memória que ninguém sabe explicar.
      URL.revokeObjectURL(endereco);
      worker.onmessage = (evento) => aoReceber(evento.data);
      worker.onerror = (erro) => aoFalhar(erro?.message ?? "");
    },
    entregar(mensagem) {
      worker?.postMessage(mensagem);
    },
    pedirEncerramento() {
      // `terminate()` é síncrono do ponto de vista de quem chama, e o que ele
      // promete é que o contexto não roda mais. Não é promessa sobre o que
      // outro contexto criou em resposta a ele — a especificação do HTML é
      // explícita, e é por isso que `encerrou()` existe em vez de este verbo
      // devolver «pronto».
      worker?.terminate();
      parou = true;
    },
    encerrou() {
      // Aqui a confirmação é verdadeira de imediato: `terminate()` é síncrono,
      // e quando ele volta o contexto não roda mais. Dizer `true` é dizer o que
      // aconteceu, e não um atalho.
      return Promise.resolve(parou);
    },
  };
}

/**
 * O executor de bancada: o motor nativo, do outro lado da ponte — etapa E1.
 *
 * **Ligado por `SEELE_EXECUTOR=quickjs`**, e por nada na tela. A diretriz
 * proíbe manter dois executores públicos; uma variável de ambiente que ninguém
 * oferece a quem usa não é um segundo executor público, é uma bancada.
 *
 * Ele existe porque medir interação, descarte e impacto na voz exige o
 * aplicativo de verdade. Um protótipo que só roda em teste não mede nada disso.
 *
 * A forma é a mesma do outro, e é esse o ponto: quatro verbos, e quem chama não
 * sabe qual dos dois está do outro lado.
 */
function executorNativo(id, geracao, hash) {
  let ouvindo = null;
  // **O número da instância**, que o lado nativo devolve ao iniciar. Ele é a
  // identidade desta execução: `id` e `geracao` não bastam, porque recarregar
  // um MOD é uma segunda execução dentro da mesma geração, e as duas falariam
  // com a mesma voz.
  let numero = null;
  // Quantas mensagens estão esperando ser atendidas **aqui dentro**.
  //
  // O teto da fila nativa mede o que ainda não saiu de lá; este mede o que já
  // chegou e ainda não foi processado. A diretriz pede a diferença: «o limite
  // dessa fila não demonstra contenção do que se acumula no transporte/eventos
  // da janela».
  let naFila = 0;
  let recusadasAqui = 0;
  let resolverAParada = null;
  // Criada **antes** do ouvinte, e não dentro dele: a fala `parou` pode chegar
  // entre o `listen` e a linha seguinte, e uma promessa que ainda não existe
  // não tem como ser resolvida.
  const parou = new Promise((resolve) => {
    resolverAParada = resolve;
  });
  return {
    nome: "nativo",
    async iniciar(_codigo, aoReceber, aoFalhar) {
      // **O código não passa por aqui.** O lado nativo já o tem — ele o lê pelo
      // mesmo `codigo_do_mod`, com a mesma conferência de hash. Mandá-lo de
      // volta seria fazer o texto atravessar a ponte duas vezes para chegar
      // onde já estava.
      ouvindo = await listen("seele://mod-nativo", ({ payload }) => {
        if (!payload || payload.geracao !== geracao) return;
        // **Pelo número da instância**, e não pelo nome do MOD: duas execuções
        // do mesmo MOD na mesma geração têm o mesmo nome e vozes diferentes.
        // Antes de o número chegar, casa pelo nome — só a própria montagem
        // pode falar nessa janela, e é ela quem está esperando o número.
        if (numero === null ? payload.id !== id : payload.instancia !== numero) return;
        if (payload.tipo === "mensagem") {
          // **Teto do que espera atendimento aqui.** `atenderOMod` é assíncrono
          // — ele vai ao servidor —, então as mensagens se acumulam deste lado
          // enquanto ele volta. Sem teto, um MOD conversador enche a memória da
          // janela depois de a fila nativa ter dado o lugar por livre.
          if (naFila >= 64) {
            recusadasAqui += 1;
            return;
          }
          let m;
          try {
            m = JSON.parse(payload.corpo);
          } catch {
            aoFalhar("o MOD mandou o que não é JSON");
            return;
          }
          naFila += 1;
          Promise.resolve(aoReceber(m)).finally(() => {
            naFila -= 1;
          });
          return;
        }
        if (payload.tipo === "parou") {
          resolverAParada?.();
          return;
        }
        // `falhou` e `interrompido` chegam com motivos diferentes e pedem
        // frases diferentes: um é o MOD com defeito, o outro é o produto
        // parando o MOD.
        aoFalhar(payload.tipo === "interrompido" ? "interrompido pelo produto" : payload.corpo);
      });
      numero = await invoke("mod_nativo_iniciar", { geracao, id, hash });
    },
    entregar(mensagem) {
      if (numero === null) return;
      invoke("mod_nativo_entregar", {
        geracao,
        instancia: numero,
        json: JSON.stringify(mensagem),
      }).catch(() => {});
    },
    pedirEncerramento() {
      // Pelo número: encerrar pelo nome mataria a execução seguinte do mesmo
      // MOD, que é a corrida que a diretriz nomeia.
      if (numero === null) return;
      invoke("mod_nativo_encerrar", { instancia: numero }).catch(() => {});
    },
    async encerrou() {
      // **Confirmar não é esperar.** A primeira versão corria a promessa contra
      // um prazo que resolvia com sucesso, e devolvia igual — a instância se
      // dizia encerrada sem que ninguém tivesse parado nada.
      //
      // Agora o prazo devolve `false`, e quem chama sabe a diferença. A saída
      // continua não travando: a espera tem teto, e o que se perde esperando é
      // o que se ganharia sabendo.
      const confirmou = await Promise.race([
        parou.then(() => true),
        new Promise((resolve) => setTimeout(() => resolve(false), 2000)),
      ]);
      // **O ouvinte fica** quando não houve confirmação: a fala `parou` pode
      // chegar depois, e ela é a única coisa que ainda pode fechar o assunto.
      // Removê-lo aqui apagaria a chance de saber.
      if (confirmou) {
        (await ouvindo)?.();
        ouvindo = null;
      }
      return confirmou;
    },
    /** O que ficou pendente deste lado — para a bancada. */
    pendencias() {
      return { naFila, recusadas: recusadasAqui };
    },
  };
}

/**
 * Uma instância de MOD: o estado, o executor e **os recursos que ela criou**.
 *
 * # O registro de recursos
 *
 * §4.2 do contrato: «ao criar áudio, uma URL de Blob, fonte, imagem, timer do
 * anfitrião ou textura, o próprio binding registra o dono. Não exigir um
 * `registrarParaLimpar` manual do autor».
 *
 * É a diferença entre limpar o que se lembrou e limpar o que existe. Quem cria
 * um recurso em nome de um MOD registra ali mesmo, numa linha, e o descarte
 * percorre a lista — em vez de percorrer a memória de quem escreveu o
 * encerramento.
 */
class InstanciaDeMod {
  constructor(id, hash, geracao, executor) {
    this.id = id;
    this.hash = hash;
    this.geracao = geracao;
    this.executor = executor;
    this.estado = ESTADOS_DE_MOD.criando;
    /** Os descartadores, na ordem em que foram registrados. */
    this.recursos = [];
    /** O que não saiu no descarte, por nome. Vazio é o desfecho normal. */
    this.naoSairam = [];
  }

  /**
   * Anota um recurso e como desfazê-lo.
   *
   * `porque` é para o diagnóstico: uma lista de funções anônimas não diz o que
   * ficou de pé quando alguma coisa fica.
   */
  registrar(porque, descartar) {
    this.recursos.push({ porque, descartar });
  }

  /** Um efeito é admitido? Só em `ativa`, e só na geração de pé. */
  admite(geracaoDePe) {
    return this.estado === ESTADOS_DE_MOD.ativa && this.geracao === geracaoDePe;
  }

  /**
   * Encerra, na ordem do §5 do contrato.
   *
   * 1. **`encerrando` antes de qualquer cancelamento ou espera.** A diretriz é
   *    explícita: «a instância sai de `ativa` para `encerrando` antes de
   *    cancelar ou aguardar qualquer operação». Entre pedir a parada e ela
   *    acontecer há mensagens em voo, e nenhuma delas pode ser admitida.
   * 2. pedir ao executor que pare;
   * 3. esperar ele confirmar;
   * 4. descartar os recursos, e só então marcar `encerrada`.
   *
   * Idempotente: chamar de novo devolve a mesma promessa em vez de descartar
   * duas vezes.
   */
  encerrar() {
    if (this.encerramento) return this.encerramento;
    this.estado = ESTADOS_DE_MOD.encerrando;
    this.encerramento = (async () => {
      let confirmou = false;
      try {
        this.executor.pedirEncerramento();
        confirmou = (await this.executor.encerrou()) === true;
      } catch (falha) {
        // Um executor que falha ao parar não impede o descarte: os recursos
        // são nossos, e deixá-los de pé porque ele não respondeu seria trocar
        // um problema por dois.
        console.warn(`MOD ${this.id}: o executor não confirmou o encerramento`, falha);
      }

      // Os recursos saem de qualquer jeito, e **o que não sair fica anotado**.
      // Uma limpeza que falha e não deixa rastro é pior que uma que não
      // acontece: a segunda pelo menos aparece no contador.
      for (const recurso of this.recursos.splice(0).reverse()) {
        try {
          recurso.descartar();
        } catch (falha) {
          console.warn(`MOD ${this.id}: ${recurso.porque} não saiu`, falha);
          this.naoSairam.push(recurso.porque);
        }
      }

      // **`encerrada` só com confirmação.** Sem ela a instância fica em
      // `encerrando`, que é a verdade: pedimos, revogamos, descartamos o que
      // era nosso, e o executor não disse que parou. Nenhum efeito é admitido
      // nos dois estados — o que muda é o que o produto **afirma**.
      this.estado =
        confirmou && this.naoSairam.length === 0
          ? ESTADOS_DE_MOD.encerrada
          : ESTADOS_DE_MOD.encerrando;
      return this.estado === ESTADOS_DE_MOD.encerrada;
    })();
    return this.encerramento;
  }
}

/**
 * Quantos recursos ainda estão de pé, por instância — para a bancada.
 *
 * «Sem essa observabilidade, "não sobrou nada" vira inspeção visual
 * insuficiente». Uma instância `encerrada` com recursos na lista é a forma que
 * esse defeito tem quando ele acontece.
 */
function recursosDePe(instancias) {
  const abertos = [];
  for (const instancia of instancias.values()) {
    if (!instancia) continue;
    // **Três coisas contam como «de pé»**: recurso ainda registrado, recurso
    // que não saiu, e instância que pediu para parar e não foi confirmada.
    // Contar só a primeira faria um encerramento sem confirmação aparecer como
    // limpo — que é justamente o que não pode.
    const pendentes = instancia.recursos.length;
    const falhos = instancia.naoSairam.length;
    const semConfirmar = instancia.estado === ESTADOS_DE_MOD.encerrando;
    if (pendentes === 0 && falhos === 0 && !semConfirmar) continue;
    abertos.push(
      `${instancia.id}:${instancia.estado}:${pendentes}` +
        (falhos > 0 ? ` (${falhos} não saíram: ${instancia.naoSairam.join(", ")})` : ""),
    );
  }
  return abertos;
}
