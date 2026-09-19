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
  let parou = null;
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
      parou = Promise.resolve();
    },
    encerrou() {
      return parou ?? Promise.resolve();
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
        if (!payload || payload.id !== id || payload.geracao !== geracao) return;
        if (payload.tipo === "mensagem") {
          try {
            aoReceber(JSON.parse(payload.corpo));
          } catch {
            aoFalhar("o MOD mandou o que não é JSON");
          }
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
      await invoke("mod_nativo_iniciar", { geracao, id, hash });
    },
    entregar(mensagem) {
      invoke("mod_nativo_entregar", {
        geracao,
        id,
        json: JSON.stringify(mensagem),
      }).catch(() => {});
    },
    pedirEncerramento() {
      invoke("mod_nativo_encerrar", { id }).catch(() => {});
    },
    async encerrou() {
      // **A confirmação vem do lado nativo**, como a fala `parou`. Com prazo:
      // um motor que não confirma não pode prender a saída da sessão, e o que
      // se perde esperando é o que se ganharia sabendo — o contador da bancada
      // diz o resto.
      await Promise.race([parou, new Promise((resolve) => setTimeout(resolve, 2000))]);
      (await ouvindo)?.();
      ouvindo = null;
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
      try {
        this.executor.pedirEncerramento();
        await this.executor.encerrou();
      } catch (falha) {
        // Um executor que falha ao parar não impede o descarte: os recursos
        // são nossos, e deixá-los de pé porque ele não respondeu seria trocar
        // um problema por dois.
        console.warn(`MOD ${this.id}: o executor não confirmou o encerramento`, falha);
      }
      for (const recurso of this.recursos.splice(0).reverse()) {
        try {
          recurso.descartar();
        } catch (falha) {
          console.warn(`MOD ${this.id}: ${recurso.porque} não saiu`, falha);
        }
      }
      this.estado = ESTADOS_DE_MOD.encerrada;
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
    if (!instancia || instancia.recursos.length === 0) continue;
    abertos.push(`${instancia.id}:${instancia.estado}:${instancia.recursos.length}`);
  }
  return abertos;
}
