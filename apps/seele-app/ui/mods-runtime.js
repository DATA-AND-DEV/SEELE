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
      // `postMessage` não recusa: a fila é do agente, e ela não tem teto que
      // este lado alcance. Devolver uma promessa resolvida mantém a forma do
      // verbo igual à do executor nativo, que recusa.
      worker?.postMessage(mensagem);
      return Promise.resolve();
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
    encerrou(_quandoConcluir) {
      // Aqui a confirmação é verdadeira de imediato: `terminate()` é síncrono,
      // e quando ele volta o contexto não roda mais. Dizer `true` é dizer o que
      // aconteceu, e não um atalho — e por isso não há conclusão tardia para
      // avisar.
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
  // **O número da instância, conhecido antes de o MOD poder falar.**
  //
  // A revisão de `fd1de3a`: enquanto ele era `null`, o ouvinte casava pelo nome
  // do MOD, e uma instância anterior do mesmo MOD falava naquela janela — a
  // mensagem dela era aceita, a resposta se perdia, e o `parou` dela confirmava
  // o encerramento da nova.
  //
  // Agora a subida tem duas etapas: `reservar` devolve o número **sem rodar
  // nada**, e `ativar` libera a execução. Entre as duas não existe fala para
  // ouvir, então ignorar o que chega antes do número não perde nada.
  let numero = null;
  let parada = null;
  let resolverAParada = null;
  // O que fazer quando a parada for confirmada — inclusive **depois** do prazo.
  let aoConcluir = null;
  let colhendo = false;

  /// Colhe até esvaziar. É a colheita que devolve o crédito do lado nativo.
  const colher = async (aoReceber, aoFalhar) => {
    if (colhendo || numero === null) return;
    colhendo = true;
    try {
      for (;;) {
        const falas = await invoke("mod_nativo_colher", {
          geracao,
          instancia: numero,
          limite: 32,
        });
        if (!falas || falas.length === 0) return;
        for (const fala of falas) {
          if (fala.tipo === "mensagem") {
            try {
              await aoReceber(JSON.parse(fala.corpo));
            } catch {
              aoFalhar("o MOD mandou o que não é JSON");
            }
            continue;
          }
          // `falhou` e `interrompido` pedem frases diferentes: um é o MOD com
          // defeito, o outro é o produto parando o MOD.
          aoFalhar(fala.tipo === "interrompido" ? "interrompido pelo produto" : fala.corpo);
        }
      }
    } catch {
      // Sessão encerrada ou instância que sumiu: não há o que colher, e
      // insistir seria bater numa porta que não existe mais.
    } finally {
      colhendo = false;
    }
  };

  /// A parada foi confirmada — seja dentro do prazo ou muito depois dele.
  const concluir = async () => {
    resolverAParada?.();
    (await ouvindo)?.();
    ouvindo = null;
    // **A confirmação tardia também conclui.** Ela era só resolvida: o ouvinte
    // ficava, a supervisão não era atualizada, e uma segunda tentativa de
    // encerrar devolvia falso para sempre.
    const terminar = aoConcluir;
    aoConcluir = null;
    terminar?.();
  };

  return {
    nome: "nativo",
    async iniciar(_codigo, aoReceber, aoFalhar) {
      // **O código não passa por aqui.** O lado nativo já o tem — ele o lê pelo
      // mesmo `codigo_do_mod`, com a mesma conferência de hash.
      parada = new Promise((resolve) => {
        resolverAParada = resolve;
      });
      ouvindo = await listen("seele://mod-nativo", ({ payload }) => {
        if (!payload || payload.geracao !== geracao) return;
        // **Sem recuo por nome.** Antes do número, nada é desta instância:
        // entre reservar e ativar o MOD não rodou, então não há fala legítima
        // para perder.
        if (numero === null || payload.instancia !== numero) return;
        if (payload.parou) {
          // Colhe o que sobrou antes de concluir: o que o MOD disse antes de
          // parar ainda precisa chegar.
          colher(aoReceber, aoFalhar).finally(concluir);
          return;
        }
        colher(aoReceber, aoFalhar);
      });

      numero = await invoke("mod_nativo_reservar", { geracao, id, hash });
      // Só agora o MOD pode falar, e o ouvinte já sabe com quem.
      await invoke("mod_nativo_ativar", { geracao, instancia: numero });
    },
    /**
     * Manda uma mensagem para dentro, **devolvendo a recusa**.
     *
     * A fila do executor tem teto, e engolir a recusa aqui faria um evento
     * perdido virar silêncio: quem digitou veria a tecla não chegar e não
     * haveria onde ler por quê. Quem chama decide o que fazer com ela.
     */
    entregar(mensagem) {
      if (numero === null) return Promise.reject(new Error("instancia-nao-reservada"));
      return invoke("mod_nativo_entregar", {
        geracao,
        instancia: numero,
        json: JSON.stringify(mensagem),
      });
    },
    pedirEncerramento() {
      // Pelo número: encerrar pelo nome mataria a execução seguinte do mesmo
      // MOD, que é a corrida que a revisão nomeia.
      if (numero === null) return;
      invoke("mod_nativo_encerrar", { instancia: numero }).catch(() => {});
    },
    /**
     * Espera a confirmação, com prazo — e **avisa quando ela chega tarde**.
     *
     * `quandoConcluir` é chamado se a confirmação vier depois do prazo. É o que
     * permite à instância fechar o estado e soltar o que faltava sem que
     * ninguém precise perguntar de novo.
     */
    async encerrou(quandoConcluir) {
      aoConcluir = quandoConcluir ?? null;
      const confirmou = await Promise.race([
        parada.then(() => true),
        new Promise((resolve) => setTimeout(() => resolve(false), 2000)),
      ]);
      if (confirmou) {
        // `concluir` já rodou pelo ouvinte; aqui só se garante que rodou.
        await concluir();
      }
      return confirmou;
    },
    /** O que ficou pendente deste lado — para a bancada. */
    pendencias() {
      return { colhendo };
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
    /** Há uma espera de confirmação em curso. */
    this.aguardando = false;
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
    // **Uma tentativa que terminou sem confirmação não tranca as seguintes.**
    //
    // A promessa ficava guardada resolvida em `false`, e toda chamada posterior
    // devolvia o mesmo `false` sem nunca mais olhar — inclusive depois de a
    // confirmação ter chegado e a instância já estar encerrada.
    //
    // Três respostas, e elas são diferentes:
    //
    // - **já acabou**: devolve `true`, que é o que aconteceu;
    // - **está esperando**: devolve a espera em curso, para duas chamadas não
    //   virarem dois pedidos de parada;
    // - **parou de esperar sem confirmar**: tenta de novo, porque a
    //   confirmação pode ter chegado no meio-tempo.
    if (this.estado === ESTADOS_DE_MOD.encerrada) return Promise.resolve(true);
    if (this.aguardando && this.encerramento) return this.encerramento;
    this.estado = ESTADOS_DE_MOD.encerrando;
    this.aguardando = true;
    this.encerramento = (async () => {
      let confirmou = false;
      try {
        this.executor.pedirEncerramento();
        // O segundo argumento é o que fecha o caso quando a confirmação chega
        // **depois** do prazo: sem ele, a instância ficava em `encerrando` para
        // sempre mesmo tendo parado.
        confirmou = (await this.executor.encerrou(() => this.concluirTardio())) === true;
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
      this.aguardando = false;
      this.estado =
        confirmou && this.naoSairam.length === 0
          ? ESTADOS_DE_MOD.encerrada
          : ESTADOS_DE_MOD.encerrando;
      return this.estado === ESTADOS_DE_MOD.encerrada;
    })();
    return this.encerramento;
  }

  /**
   * A confirmação chegou **depois** do prazo: conclui o que ficou pendente.
   *
   * Sem isto, uma instância que parou tarde ficava em `encerrando` para sempre,
   * com o ouvinte de pé e o diagnóstico acusando sobra — de um MOD que já tinha
   * parado. O contrário do que o diagnóstico existe para dizer.
   *
   * **Não readmite efeito**: o estado só anda de `encerrando` para `encerrada`,
   * e `admite` continua falso nos dois.
   */
  concluirTardio() {
    if (this.estado !== ESTADOS_DE_MOD.encerrando) return;
    // O que ainda estiver registrado sai agora — pode ter sido criado entre o
    // pedido de parada e a confirmação.
    for (const recurso of this.recursos.splice(0).reverse()) {
      try {
        recurso.descartar();
      } catch (falha) {
        console.warn(`MOD ${this.id}: ${recurso.porque} não saiu`, falha);
        this.naoSairam.push(recurso.porque);
      }
    }
    if (this.naoSairam.length === 0) this.estado = ESTADOS_DE_MOD.encerrada;
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
