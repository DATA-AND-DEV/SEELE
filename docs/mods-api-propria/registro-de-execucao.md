# Registro de execução — API própria de MODs

O registro que o [roteiro](roteiro-de-execucao.md) §8 pede. Cada entrada diz o
que mudou, onde, que comportamento foi **provado**, o que falta e qual etapa
pode começar.

| Etapa | Estado | Evidência |
| --- | --- | --- |
| E0 — inventário | **Feito** | §1 e §2 abaixo |
| E1 — executor e autoridade | **Protótipo QuickJS medido e aprovado na autoridade; falta custo e as outras plataformas** | §3 e §6 abaixo |
| E2 — sessão e encerramento | **Infraestrutura pronta e independente do executor; aceite integrado pendente de E1** | §5 abaixo |
| E3 — renderer e SDK mínimos | Pendente | — |
| E4 — desenho e mídia | Pendente | — |
| E5 — funções completas | Pendente | — |
| E6 — publicação preparada | Pendente | — |

---

## 1. E0 — onde cada repositório estava

Conferido em 18/09/2026, depois de `23fc654`.

| Repositório | HEAD | Trabalho local não commitado |
| --- | --- | --- |
| SEELE | `23fc654` | os documentos desta proposta, ainda não commitados |
| SEELE-MODS-INDEXER | `251c7a0` | guia, `ferramentas/`, `manifesto.py`, testes e site — **trabalho de outra pessoa em curso** |
| SEELE-MOD-MESA | `7ca40d1` | migração para API 3 |
| SEELE-MOD-PERFIS | `ce976fd` | migração para API 3 |
| SEELE-MOD-ESTILO | `0fba9a5` | migração para API 3 |
| SEELE-RELEASES | `4750acc` | — |
| SEELE-SITE | `39a4b73` | — |

**Nada foi descartado.** Os três MODs e o indexador têm alterações de outra
pessoa na árvore; este trabalho não as tocou e não as commitou.

## 2. E0 — a matriz funcional, medida

Não é opinião sobre a API: é a contagem do que cada cliente chamava antes e
chama agora. «Antes» é o `cliente/main.js` commitado (API 2); «agora» é o da
árvore de trabalho (API 3).

| MOD | Linhas antes → agora | O que o cliente de API 2 usava | O que o de API 3 usa |
| --- | --- | --- | --- |
| MESA | 413 → 90 | `canvas`, `dialog`, `button`, `section`, `h*`, dois `FormData`, `FileReader`, `Image`, cliques | quatro formas de leitura |
| PERFIS | 294 → 74 | `FileReader`, `Image`, `change`, `cancel` | quatro formas de leitura |
| ESTILO | 146 → 72 | `cancel`, tokens no documento | quatro formas de leitura, `SeeleUI.tema` |

**Os três clientes de hoje são telas de leitura.** A afirmação da proposta —
«carregou os pacotes e não manteve os produtos» — confere com o código. Nenhum
deles tem entrada, mídia, desenho ou arquivo.

## 3. E1 — a fronteira real do executor, medida

### Como foi medido

`apps/seele-app/testes/sonda-de-fronteira/` é um MOD que **tenta** cada caminho
e grava o resultado no quintal da metade de servidor. Ele rodou no aplicativo
nativo — WKWebView, macOS —, num `SEELE_HOME` descartável, com o servidor
hospedado pelo próprio app. A casa de verdade de quem trabalha aqui não foi
tocada.

O §3.1 do contrato é a razão de ele existir: «não ter `document`, `window` ou o
objeto global do Tauri não prova isolamento de armazenamento, rede, IPC ou
canais entre contextos», e «o código de teste deve tentar acessar diretamente os
caminhos alternativos».

### Como ler a tabela

A diretriz de 18/09 corrigiu a classificação, e a correção vale mais que os
resultados: **um instrumento que devolve «recusado» para ausência, recusa,
falha e prazo esgotado não distingue quatro coisas diferentes.** As colunas
abaixo separam o que foi observado do que aquilo permite concluir.

| Caminho | Observado | O que isso sustenta | O que falta para virar aceite |
| --- | --- | --- | --- |
| `document`, `window`, `__TAURI__`, `__TAURI_INTERNALS__`, `localStorage` | `undefined` | o ambiente de janela não está lá | nada; é ausência de símbolo |
| `SharedWorker` | `undefined` | idem | nada |
| `indexedDB` | abriu, gravou, e **releu a marca depois de reiniciar** | armazenamento de origem sobrevive à sessão | esperar o `oncomplete` da transação, e não só o `onsuccess` do pedido |
| `caches` | `caches.open` devolveu | a API existe | **não é prova de persistência**: falta gravar, ler e reler depois de reiniciar |
| `BroadcastChannel` | abriu, postou, fechou | a API existe | **não é prova de conversa**: faltam duas instâncias e um nonce recebido e confirmado |
| `fetch ipc://localhost/<cmd>` | HTTP 500 | **a rota é alcançável** | execução não demonstrada: falta um comando inofensivo instrumentado, com controle positivo fora do MOD |
| `fetch` externo | `TypeError` | alguma coisa recusou | **não distingue CSP de DNS, rede ou CORS**: falta destino controlado e controle positivo |
| `Worker` filho | ver abaixo | nada, ainda | ver abaixo |

### O que a sonda de hoje não consegue dizer

Três frases que estiveram neste documento e saíram:

- **«a rede externa é bloqueada pela CSP.»** O destino era `example.invalid`, que
  não resolve. `TypeError` é o que se vê quando a CSP recusa e também quando o
  DNS falha. A observação boa é «não saiu»; a causa não foi medida.
- **«dois MODs conversam por `BroadcastChannel`.»** A sonda abriu **um**
  endpoint e postou nele. Disponibilidade da API não é comunicação entre
  instâncias.
- **«`caches` persiste.»** Ela só chamou `open`. Quem persistiu, medido, foi o
  IndexedDB.

E sobre o IPC: HTTP 500 diz que a requisição chegou ao Tauri e foi recusada.
**Não diz que a chave é quebrável**, e a chave morar na janela que constrói o
worker não demonstra vazamento nenhum. O que está medido é que a rota existe e
que a autorização depende de outro mecanismo — o que já é motivo para não
apoiar a fronteira nela, mas por exigência de contrato e não por uma falha
observada.

### O que ficou por medir, e por que ele não conta ainda

**O `Worker` filho.** Uma primeira leitura desta seção dizia que ele morre com o
pai, e a conclusão não se sustenta por três razões, não uma:

1. **o processo do aplicativo foi reiniciado entre as duas leituras**, e a morte
   do processo explica o mesmo resultado;
2. os 2,4 segundos são contados do **desligamento do MOD no banco**, e não de um
   `terminate()` com hora registrada — e a bancada descobriu depois que mexer no
   banco por fora **não encerra sessão nenhuma**: o sinal que acorda o anúncio é
   um `watch` dentro do processo. O que se observou naquele intervalo foi o MOD
   continuando a bater com o servidor recusando cada pedido, por `disabled`;
3. a leitura acontece **depois** de a execução nova criar um filho que escreve na
   mesma chave, então o valor lido pode ser dela.

Para medir de verdade: registrar a hora da revogação e a do término, marcar cada
execução com a geração dela, e **ler a marca da execução antiga antes de iniciar
a nova** — distinguindo execução tardia de transação que só fez o commit
depois.

A especificação do HTML manda que encerrar um worker encerre os que ele possui.
É o que se espera, e é o que falta observar. **A pendência é de automação da
janela, e não de código.**

### O que isto decide

**O Worker de Blob não vai ser o executor.** A marca de IndexedDB relida depois
de reiniciar basta para isso: ela é persistência fora do controle da sessão, e a
garantia de isolamento não pode ser publicada com ela de pé. As outras
observações desta seção são fracas demais para pesar na decisão, e a decisão não
precisou delas.

**O próximo experimento é QuickJS nativo no cliente**, mantendo a WebView
existente e o renderer confiável — uma WebView por MOD continua fora. Origem
isolada para Worker fica como alternativa se o protótipo reprovar nos requisitos
medidos; ela também precisaria demonstrar separação entre instâncias e
servidores, ausência de IPC privilegiado e política de armazenamento, porque uma
origem só diferente da janela continua sendo **compartilhada entre os MODs**.

**E1 não está resolvido.** A orientação escolhe o experimento seguinte; ela não
aprova executor nenhum antes das provas. Enquanto isso, o Worker atual serve
para exercitar os caminhos que já existem — e não para comprovar o isolamento
final.

### Como repetir

```sh
# publica a sonda num SEELE_HOME descartável, hospeda, e lê o que ela mediu
SEELE_HOME=<descartável> ./target/debug/seele-app --hospedar
sqlite3 <descartável>/seele.db "SELECT key, CAST(value AS TEXT) FROM mod_data;"
```

O pacote precisa estar em `<home>/mods/mod-packages/<hash>` — onde o **servidor**
o procura — e a linha de `mods` com `enabled=1`. O `aceites` precisa da
identidade do conjunto, que o `seele.log` escreve quando a entrada é recusada.

## 4. O que muda no que já está escrito

O ADR 0049 afirma, em «Três consequências que decorrem disso»: «Destruir o
contexto destrói o MOD. Temporizadores, ouvintes, áudio e promessas morrem com
ele». A medição sustenta isso para **execução** e para **descendentes**, e não
para **armazenamento**. A frase precisa da ressalva antes de virar promessa
publicada, e o guia de migração precisa dizer o mesmo.

---

## 5. E2 — a geração da sessão

### O que passou a existir

Um número, e ele é a espinha de tudo. `Session.geracao` sobe a cada tentativa de
conexão e a cada desmontagem, nunca desce, e zero quer dizer «nenhuma sessão».
A janela guarda uma cópia e a devolve em todo comando de MOD.

| Onde | O que passou a conferir |
| --- | --- |
| `Bridge::on_event` | evento de geração morta é **contado** e descartado, em vez de emitido para a janela |
| `desmontar_o_cliente` | **revoga na primeira linha**, antes de tirar a conexão do slot |
| `Bridge::on_event`, no `Ended` | revoga assim que a sessão acaba do outro lado, sem esperar a janela pedir |
| `mod_request`, `codigo_do_mod` | recusam a geração que não é a de pé, e contam |
| `montarOMod` | lê a geração antes do `await` e confere depois — «presença de um ID no Map não identifica uma geração» |
| `atenderOMod` | confere **antes** do efeito, e não só antes da resposta |
| `pedirAoServidor` e o ouvinte de `ModReply` | pedido e resposta carregam a geração; um número reaproveitado na sessão seguinte não recebe resposta antiga |
| `encerrarOAmbienteDosMods` | revoga na primeira linha |
| `ejetar` | encerra **antes** de esperar o `disconnect`, que é um `await` sobre a ponte |
| `estado_da_sessao` | geração e os dois contadores, desenhados no bloco de manutenção |

### O que foi provado, e como

Por reversão, na bateria: cinco consertos revertidos, cinco guardas falhando com
a frase certa — a conferência voltando para depois do efeito, a resposta
atrasada sendo entregue pelo número, o encerramento deixando de revogar na
primeira linha, a saída local voltando a esperar a ponte, e a desmontagem
voltando a destruir a conexão antes de revogar.

Na bancada nativa, uma descoberta que só o app de verdade dá: **desligar um MOD
direto no banco não encerra sessão nenhuma.** O sinal que acorda o anúncio é um
`watch` dentro do processo, e o que se observou foi o MOD continuando a bater
com o servidor recusando cada pedido, por `disabled`. Isso invalidou a primeira
tentativa de medir o encerramento por fora — e é a razão de a prova do §3 sobre
o worker filho ter sido retirada.

### O que falta para E3

A prova de corrida no aplicativo nativo — saída local com pedido em voo, troca
A→B, montagem atrasada — precisa de um gesto na janela, e por isso de automação
de interface que esta bancada ainda não tem. O código está escrito e guardado
por reversão; o que falta é a observação, e ela está nomeada aqui em vez de
marcada como verde.

### O contrato de executor

A diretriz de 18/09 mudou a forma desta etapa: «manter a implementação atrás de
um contrato interno de executor: iniciar, entregar evento, solicitar
encerramento e confirmar encerramento».

`apps/seele-app/ui/mods-runtime.js` é esse lugar. Nele moram os quatro estados,
a instância, o registro de recursos e o ciclo de encerramento; `base.js` deixou
de construir `Worker` e passa a pedir **um executor**. Trocar qual é uma linha.

| Peça | O que ela garante |
| --- | --- |
| `ESTADOS_DE_MOD` | `criando`, `ativa`, `encerrando`, `encerrada`, e a revogação não volta atrás |
| `InstanciaDeMod.admite` | efeito só em `ativa` e só na geração de pé — `encerrando` já recusa |
| `InstanciaDeMod.registrar` | quem cria um recurso anota o descarte ali mesmo, sem pedir isso a quem escreve o MOD |
| `InstanciaDeMod.encerrar` | `encerrando` **antes** de qualquer espera; pede ao executor; espera a confirmação; só então descarta e marca `encerrada` |
| `executorDeWorker` | o executor de hoje, e o único lugar que fala `Worker` |
| `recursosDePe` | o que ainda está de pé, por instância, desenhado na bancada |

A ordem dentro de `encerrar` é prendida por reversão: mover o `encerrando` para
depois do pedido de parada, ou o `encerrada` para antes do descarte, reprova.

### O que foi demonstrado, e com qual executor

**Demonstrado, e vale para qualquer executor:** a ordem do encerramento, a
revogação monotônica, a conferência antes do efeito, a correlação de pedido e
resposta por geração, e a recusa no Rust do que vem de geração morta. São
propriedades do ciclo, e o ciclo não conhece o executor.

**Demonstrado só com o Worker de Blob, e a repetir:** que um MOD de verdade sobe,
desenha, pede ao servidor e é descarregado. A sonda de fronteira rodou no
aplicativo nativo pelo contrato novo e produziu os quinze achados. Isso exercita
os caminhos; **não comprova isolamento**, e o executor que os exercitou não é o
que vai ficar.

### O que precisa ser repetido com o executor escolhido

1. Toda a matriz de corrida da E2 — saída local, expulsão, fim remoto, troca
   A→B, reentrada no mesmo servidor, carregamento e resposta atrasados;
2. a confirmação de encerramento: com o Worker ela é imediata, e um motor
   nativo pode levar tempo — é exatamente por isso que `encerrou()` é uma
   promessa e não um retorno;
3. os contadores de recursos depois de dezenas de ciclos de entrar e sair;
4. o custo: memória, CPU ociosa e ativa, latência de interação e de saída, e o
   impacto na voz.

**O aceite integrado de E2 depende de E1.** O que está pronto é a
infraestrutura, e ela está pronta para ser reaproveitada — não para ser
publicada como garantia.

---

## 6. E1 — o protótipo QuickJS

`apps/seele-app/src/executor.rs`, atrás de `cfg(test)`. A diretriz proíbe dois
executores públicos, então ele existe **para a bancada** e não como caminho de
produção — sai de lá no dia em que a decisão de E1 for tomada.

### A diferença que importa

No navegador o ambiente existe, e a fronteira é tudo o que se consegue tirar
dele. Aqui o contexto nasce sem ambiente, e a fronteira é tudo o que alguém
escolheu pôr — uma função por vez, num arquivo. É por isso que **esta prova é
um teste automático** e a da janela não era: não precisa de aplicativo, de
servidor nem de clique.

### A superfície inteira de um contexto de MOD

Medida, e impressa pelo próprio teste:

```
AggregateError, Array, ArrayBuffer, AsyncDisposableStack, Atomics, BigInt,
BigInt64Array, BigUint64Array, Boolean, DOMException, DataView, Date,
DisposableStack, Error, EvalError, FinalizationRegistry, Float16Array,
Float32Array, Float64Array, Function, Infinity, Int16Array, Int32Array,
Int8Array, InternalError, Iterator, JSON, Map, Math, NaN, Number, Object,
Promise, Proxy, RangeError, ReferenceError, Reflect, RegExp, Set,
SharedArrayBuffer, String, SuppressedError, Symbol, SyntaxError, TypeError,
URIError, Uint16Array, Uint32Array, Uint8Array, Uint8ClampedArray, WeakMap,
WeakRef, WeakSet, atob, btoa, decodeURI, decodeURIComponent, encodeURI,
encodeURIComponent, escape, eval, globalThis, isFinite, isNaN, parseFloat,
parseInt, performance, queueMicrotask, seele, undefined, unescape
```

Nada de `indexedDB`, `caches`, `localStorage`, `BroadcastChannel`, `fetch`,
`XMLHttpRequest`, `WebSocket`, `Worker`, `SharedWorker`, `importScripts`,
`document`, `navigator`, `require`, `process`, `std`, `os` — nem dos dois
globais do Tauri. **`seele` é a única porta**, e ela faz uma coisa: pôr texto
num canal.

Duas linhas merecem nota, porque elas estão lá e não foram escolhidas:
`SharedArrayBuffer` existe, e neste desenho não há com quem compartilhar —
não há workers no contexto; e `eval` existe, o que deixa o MOD avaliar o
próprio texto sem ganhar autoridade nova.

### O que foi provado, e por reversão

| Propriedade | Prova | Reversão que a derruba |
| --- | --- | --- |
| nenhum ambiente alcançado | a sonda tenta 21 nomes e nenhum responde | ligar um `fetch` no contexto |
| revogar para um laço infinito **em execução** | o MOD avisa `comecei`, e o encerramento confirma em menos de 5 s | arrancar a revogação do tratador: o executor não confirma em 10 s |
| o prazo sozinho interrompe | sem teto de trabalho, com 50 ms | zerar a conferência de relógio |
| o teto de trabalho sozinho interrompe | sem prazo, com mil consultas | — |
| uma microtarefa não escapa do prazo | o laço dentro de uma `Promise` é interrompido, **e dito** | não rodar os jobs dentro da volta |
| a volta completa de resposta | `iniciar` → `postar` → `entregar` → `aoResponder` → `postar` | — |
| resposta depois da revogação não entra | a fila é conferida ao tirar dela | — |
| a mensagem grande demais é recusada na porta | 13 KiB não passa, a seguinte passa | tirar o teto |

**Os três mecanismos de parada foram separados** — revogação, prazo e teto de
trabalho — e cada teste afrouxa os outros dois. Não foi assim na primeira
versão: dois deles passavam com a revogação arrancada, porque o teto de
trabalho os salvava. E um terceiro passava em 0,00 s porque o laço nunca
chegava a rodar: `pedir_encerramento` marca o revogado antes de mandar a
mensagem, e a thread via a marca ao tirar o código da fila. **Os três eram
guardas vacuosos**, e foi a reversão que os encontrou.

### Um defeito que a medição encontrou

A interrupção de uma microtarefa era **silenciosa**: o MOD era parado no meio de
uma `Promise` e ninguém ficava sabendo — nem a janela, nem quem hospeda, nem
quem escreveu o MOD. É o defeito que o `CLAUDE.md` deste repositório nomeia como
o mais caro daqui, cometido pelo próprio mecanismo de contenção. Agora ela é
dita.

### O que isto ainda não decide

**Autoridade: aprovado.** É o que a E1 pergunta, e a resposta é limpa.

**Custo: não medido.** Memória depois de aquecer, CPU ociosa e ativa, latência
de interação e de saída, impacto na voz — nada disso foi medido, e o teto de
8 MiB veio da metade de servidor como ponto de partida, não como decisão. A
diretriz proíbe reaproveitá-lo como orçamento do cliente.

**Plataformas: não medido.** Windows e Linux continuam pendentes, e a troca de
executor não dispensa medir renderer, mídia e ponte neles.

**A fatia vertical: não feita.** Entrada, alteração incremental, arraste,
desenho e uma mídia gerenciada — é a condição que o §5 do contrato põe para
escolher definitivamente, e ela é E3 e E4.

### As filas têm teto, e a saturação não prende a saída

Havia dois tetos e faltava o terceiro. O de 12 KiB é da **mensagem** — o do
quadro de controle — e ele não contém o **acumulado**: um MOD num laço enchia a
memória de quem usa uma mensagem por vez, todas dentro do limite individual.

Agora a fila de saída tem teto de quantidade (64) e de bytes (256 KiB), e os
dois existem porque as duas saturações são diferentes: sessenta e quatro
mensagens de 12 KiB são 768 KiB, e um milhão de mensagens de dez bytes é uma
inundação que a contagem pega e os bytes não.

**Nada bloqueia.** Cheia, a fila recusa e conta; o MOD recebe `false` e fica
sabendo. Uma fila que fizesse `postar` esperar prenderia a thread do motor — e
prender a thread do motor é prender o encerramento, que é exatamente o que não
pode acontecer. Provado: com a fila cheia e o MOD ainda escrevendo, o
encerramento confirma em menos de cinco segundos.

Ler devolve o lugar, então a saturação é um estado e não uma marca.

### O custo básico, medido

Binário de teste no macOS, `ps -o rss` e `ps -o cputime` sobre o processo
inteiro, depois de aquecer:

| | memória | sobre a base | subida | CPU ociosa | encerramento |
| --- | --- | --- | --- | --- | --- |
| sem instância | 12 608 KiB | — | — | — | — |
| 1 instância | 13 056 KiB | +448 KiB | 742 µs | 0 centésimos em 1 s | 268 µs |
| 3 instâncias | 13 600 KiB | +992 KiB (331 KiB cada) | 1,86 ms | 0 centésimos em 1 s | 624 µs |

A primeira instância custa mais que as seguintes — 448 KiB contra ~272 KiB de
margem —, que é o motor pagando a montagem uma vez só.

**CPU ociosa é zero** na resolução que o sistema dá. Importa num produto de voz:
um motor que rodasse um laço de espera disputaria com o áudio por não estar
fazendo nada.

**O que estes números não são:** um orçamento. São de um binário de teste, sem
renderer, sem mídia e sem janela, numa máquina. A diretriz proíbe transformar
medida em teto sem repetir a carga equivalente — e a comparação com a referência
relatada no Windows continua por fazer.

**Windows e Linux: pendentes.** Nada nesta seção foi medido fora do macOS.
