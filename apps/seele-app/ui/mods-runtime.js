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
    iniciar(codigo, aoReceber, aoFalhar) {
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
