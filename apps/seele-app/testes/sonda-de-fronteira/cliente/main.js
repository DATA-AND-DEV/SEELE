// A sonda de fronteira — etapa E1 do contrato de API própria de MODs.
//
// **Ela não é um exemplo. É um instrumento.** O §3.1 do contrato diz, com todas
// as letras: «não ter `document`, `window` ou o objeto global do Tauri não prova
// isolamento de armazenamento, rede, IPC ou canais entre contextos», e «o código
// de teste deve tentar acessar diretamente os caminhos alternativos».
//
// Então é o que este arquivo faz: tenta, um por um, cada caminho que a ausência
// de `document` **não** fecha, e manda o resultado para a metade de servidor,
// que o grava. Quem lê o registro depois lê o que o motor de verdade respondeu,
// e não o que alguém supôs lendo a especificação.
//
// Nada aqui é malicioso: nenhuma tentativa lê dado de ninguém, nenhuma grava
// fora do próprio nome, e a de IPC manda um comando inexistente. O que se mede
// é **se o caminho existe**, e não o que se poderia fazer com ele.

const EU = "seele/sonda-de-fronteira";
const SEM_CANAL = 0;

/**
 * Roda uma tentativa e devolve o que aconteceu, sem deixar nada escapar.
 *
 * **Com prazo.** A primeira versão desta sonda travou: uma promessa de
 * IndexedDB que nunca resolvia parou a medição inteira, e o registro ficou
 * vazio — o instrumento calado é pior que o instrumento errado, porque não há
 * o que ler para descobrir que ele quebrou. Um caminho que não responde em
 * cinco segundos vira um achado, e os outros seguem.
 */
async function tentar(nome, o_que) {
  let prazo;
  try {
    const valor = await Promise.race([
      o_que(),
      new Promise((_, falhou) => {
        prazo = setTimeout(() => falhou(new Error("sem-resposta")), 5000);
      }),
    ]);
    return { nome, alcancou: true, detalhe: String(valor ?? "").slice(0, 200) };
  } catch (erro) {
    return { nome, alcancou: false, detalhe: String(erro?.name ?? erro).slice(0, 200) };
  } finally {
    clearTimeout(prazo);
  }
}

async function medir() {
  const achados = [];

  // ---- 1. O ambiente que o prelúdio diz não existir ----
  for (const nome of ["document", "window", "__TAURI__", "__TAURI_INTERNALS__", "localStorage"]) {
    achados.push(await tentar(`global:${nome}`, () => {
      const valor = globalThis[nome];
      if (valor === undefined) throw new Error("undefined");
      return typeof valor;
    }));
  }

  // ---- 2. Armazenamento da origem ----
  //
  // Um worker de `blob:` herda a origem de quem o criou. Se ela tem IndexedDB,
  // o MOD escreve num lugar que **sobrevive à sessão** e que todo outro MOD
  // desta máquina enxerga — e «some quando você sai» deixa de ser verdade.
  //
  // **Escreve e lê de volta**, e não só abre. A pergunta que decide não é «o
  // banco existe?» — é «o que um MOD guardou nele continua lá depois de sair
  // do servidor?». Se continuar, «some quando você sai» é falso para
  // armazenamento, e nenhum `terminate()` conserta isso.
  achados.push(await tentar("armazenamento:indexedDB", () => {
    if (typeof indexedDB === "undefined") throw new Error("undefined");
    return new Promise((ok, falhou) => {
      // Versão 2: a 1 foi criada por uma sonda anterior, sem o armazém.
      const pedido = indexedDB.open("sonda-de-fronteira", 2);
      pedido.onerror = () => falhou(new Error("recusado"));
      pedido.onupgradeneeded = () => {
        if (!pedido.result.objectStoreNames.contains("marcas")) {
          pedido.result.createObjectStore("marcas");
        }
      };
      pedido.onsuccess = () => {
        const banco = pedido.result;
        // Dentro do `try`: um `transaction` sobre armazém que não existe lança
        // **sincronamente** aqui dentro, onde nada o pegaria, e a promessa
        // ficaria pendente para sempre. Foi o que travou a primeira medição.
        try {
        const leitura = banco.transaction("marcas", "readonly").objectStore("marcas").get("quando");
        leitura.onsuccess = () => {
          const antes = leitura.result;
          const escrita = banco
            .transaction("marcas", "readwrite")
            .objectStore("marcas")
            .put(Date.now(), "quando");
          escrita.onsuccess = () => {
            banco.close();
            ok(antes ? `achei marca de ${antes}` : "primeira vez, gravei");
          };
          escrita.onerror = () => { banco.close(); falhou(new Error("nao-gravou")); };
        };
        leitura.onerror = () => { banco.close(); falhou(new Error("nao-leu")); };
        } catch (erro) {
          banco.close();
          falhou(erro);
        }
      };
    });
  }));
  achados.push(await tentar("armazenamento:caches", async () => {
    if (typeof caches === "undefined") throw new Error("undefined");
    await caches.open("sonda-de-fronteira");
    return "abriu";
  }));

  // ---- 3. Conversa entre contextos, por fora da API ----
  //
  // Mesma origem quer dizer que dois workers de MODs diferentes podem falar
  // um com o outro sem passar pelo produto — e com a janela, se alguém
  // escutar. É a fronteira que `Object.freeze` não fecha.
  achados.push(await tentar("conversa:BroadcastChannel", () => {
    if (typeof BroadcastChannel === "undefined") throw new Error("undefined");
    const canal = new BroadcastChannel("sonda-de-fronteira");
    canal.postMessage("oi");
    canal.close();
    return "abriu e postou";
  }));

  // ---- 4. O IPC do Tauri, pelo caminho que não precisa de `window` ----
  //
  // O Tauri 2.11 manda `fetch` para `ipc://localhost/<comando>` com a chave
  // `Tauri-Invoke-Key` no cabeçalho, e a CSP desta janela **permite**
  // `connect-src ipc:`. O que falta a um worker é a chave, que vive no escopo
  // do script da janela — não o caminho.
  //
  // Duas medidas separadas, porque elas respondem coisas diferentes: se o
  // `fetch` **sai** (a CSP deixou), e se ele é **atendido** (a chave bastou).
  for (const [nome, cabecalhos] of [
    ["ipc:sem-chave", {}],
    ["ipc:chave-inventada", { "Tauri-Invoke-Key": "inventada" }],
  ]) {
    achados.push(await tentar(nome, async () => {
      const resposta = await fetch("ipc://localhost/comando_que_nao_existe", {
        method: "POST",
        headers: { "Content-Type": "application/json", ...cabecalhos },
        body: "{}",
      });
      return `status ${resposta.status}`;
    }));
  }

  // ---- 5. Descendentes ----
  //
  // Um worker que cria outro worker cria vida fora do que `terminate()`
  // alcança, se o motor não os derrubar junto. O contrato exige saber.
  //
  // **O filho bate.** A pergunta não é «dá para criar?» — é «ele morre quando o
  // pai morre?». O ADR 0049 promete que `terminate()` é garantia e não pedido,
  // e essa promessa só vale se ela alcançar o que o MOD criou.
  //
  // Então o filho grava a hora num lugar que sobrevive à janela, a cada meio
  // segundo. A medição seguinte lê a última batida: se ela for **depois** do
  // encerramento da sessão anterior, o filho continuou vivo.
  achados.push(await tentar("descendente:Worker", () => {
    if (typeof Worker === "undefined") throw new Error("undefined");
    const corpo = `
      setInterval(() => {
        const p = indexedDB.open('sonda-de-fronteira', 2);
        p.onsuccess = () => {
          const b = p.result;
          try {
            b.transaction('marcas', 'readwrite').objectStore('marcas').put(Date.now(), 'filho');
          } catch (e) {}
          setTimeout(() => b.close(), 50);
        };
      }, 500);
    `;
    const fonte = new Blob([corpo], { type: "text/javascript" });
    const endereco = URL.createObjectURL(fonte);
    const filho = new Worker(endereco);
    URL.revokeObjectURL(endereco);
    globalThis.__sonda_filho = filho;
    return "criou e pôs para bater";
  }));

  // E a última batida que o filho da execução **anterior** deixou.
  achados.push(await tentar("descendente:ultima-batida-do-filho", () => {
    return new Promise((ok, falhou) => {
      const pedido = indexedDB.open("sonda-de-fronteira", 2);
      pedido.onerror = () => falhou(new Error("recusado"));
      pedido.onsuccess = () => {
        const banco = pedido.result;
        try {
          const leitura = banco.transaction("marcas", "readonly").objectStore("marcas").get("filho");
          leitura.onsuccess = () => { banco.close(); ok(leitura.result ?? "nenhuma"); };
          leitura.onerror = () => { banco.close(); falhou(new Error("nao-leu")); };
        } catch (erro) {
          banco.close();
          falhou(erro);
        }
      };
    });
  }));
  achados.push(await tentar("descendente:SharedWorker", () => {
    if (typeof SharedWorker === "undefined") throw new Error("undefined");
    return "existe";
  }));

  // ---- 6. Rede para fora ----
  //
  // `connect-src` da janela não lista origem externa nenhuma. Se sair, a CSP
  // não está valendo aqui dentro — e essa é a descoberta mais importante que
  // esta sonda pode fazer.
  achados.push(await tentar("rede:externa", async () => {
    const resposta = await fetch("https://example.invalid/sonda");
    return `status ${resposta.status}`;
  }));

  return achados;
}

/**
 * Bate no servidor sem parar — para a etapa E2.
 *
 * A pergunta da E2 não se responde parado: «nenhum efeito antigo admitido»
 * precisa que **haja** efeito antigo tentando entrar. Um MOD que pede uma vez
 * no início nunca tem um pedido em voo no instante da saída, e a corrida que
 * importa nunca acontece.
 *
 * Então este laço pede a cada 250 ms, para sempre. Quando a sessão encerrar,
 * haverá pedido no ar — e o que o produto fizer com ele é o que o registro e o
 * log mostram.
 *
 * Ele também é o MOD que não coopera: não há aqui nenhum `seele-mod-unload`,
 * nenhum `clearInterval`, nenhuma despedida. Se o laço parar, foi porque o
 * produto o parou.
 */
function baterSemParar() {
  setInterval(() => {
    SeeleMods.request(EU, SEM_CANAL, { op: "registrar", achados: [] }).catch(() => {});
  }, 250);
}

async function comecar() {
  baterSemParar();
  const achados = await medir();
  // Vai para a metade de servidor, que grava. O registro tem de sobreviver à
  // janela: quem lê o resultado lê o banco, e não um console que ninguém vê.
  await SeeleMods.request(EU, SEM_CANAL, { op: "registrar", achados });

  // E também na tela, em palavras, para quem estiver olhando.
  await SeeleUI.regiao({
    forma: "linha",
    dentro: [
      { forma: "titulo", dentro: "SONDA DE FRONTEIRA" },
      {
        forma: "lista",
        dentro: achados.map((a) => ({
          forma: "item",
          dentro: `${a.alcancou ? "ALCANÇOU" : "recusado"} · ${a.nome} · ${a.detalhe}`,
        })),
      },
    ],
  });
}

comecar();
