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

### O executor nativo, ligado no aplicativo de verdade

`SEELE_EXECUTOR=quickjs` no ambiente do processo. Sem a variável, a janela sobe
MODs pelo Worker como sempre; nada na tela oferece a troca. A diretriz proíbe
**dois executores públicos**, e uma variável que ninguém oferece não é um.

A escolha acontece numa linha, em `montarOMod`, que é o que o contrato de
executor existe para permitir. A janela recebe o **papel** — `worker` ou
`nativo` — e não o nome do motor: qual motor está por trás é decisão da casca,
do mesmo jeito que ela não nomeia o transporte.

### A mesma sonda, nos dois executores, no mesmo aplicativo

| Caminho | Worker (WKWebView) | Nativo |
| --- | --- | --- |
| `indexedDB` | alcançou, **e a marca sobreviveu a reiniciar** | recusado |
| `caches` | alcançou | recusado |
| `BroadcastChannel` | alcançou | recusado |
| `Worker` filho | alcançou | recusado |
| `fetch ipc://localhost` | alcançou, HTTP 500 | recusado: não há `fetch` |
| rede externa | recusado | recusado |
| `document`, `window`, Tauri | recusado | recusado |

E o MOD **funcionou**: pediu ao servidor pela API de sempre, e o servidor
gravou. O caminho inteiro está de pé — janela, ponte, motor, fila, bomba,
evento, atendimento, pedido, resposta, e de volta ao MOD.

### Duas coisas que só o aplicativo de verdade mostrou

**Uma corrida de registro.** A instância era guardada no mapa **depois** de o
executor subir, e o MOD começa a falar no instante em que o código roda. As
primeiras mensagens chegavam antes do registro, `meu()` respondia falso, e elas
eram descartadas em silêncio. Com o Worker isso passava despercebido porque a
ordem das microtarefas escondia a corrida; com o motor nativo, que é outra
thread, o MOD ficava mudo e nada dizia por quê. Agora a instância é guardada e
marcada `ativa` **antes** de o executor subir — nada pode produzir efeito antes
disso, então não há janela aberta.

**Não havia tempo.** `setTimeout` e `setInterval` são APIs de navegador, e o
QuickJS não tem laço de eventos. A sonda travava num `clearTimeout` dentro de um
`finally`, e o MOD morria calado. Agora o **anfitrião** tem a tabela de
temporizadores, e o prelúdio é a fachada dela.

Isso não é um contorno: é o que faz um temporizador ser um recurso com dono, que
o §4.2 do contrato exige. Encerrar a instância apaga a tabela, e nenhum
temporizador sobrevive porque o MOD esqueceu de cancelá-lo. Com teto de 256 e
piso de 4 ms, que é o dos navegadores e existe pela mesma razão: um temporizador
de zero em laço é espera ocupada, e espera ocupada disputa com o áudio.

### O que falta de E1 e do item 3

- **as primitivas da fatia**: entrada, atualização incremental, arraste/desenho
  e mídia gerenciada. O executor está integrado; a fatia interativa não está
  feita;
- **medir interação, descarte e impacto na voz** — item 4, e ele depende da
  fatia existir;
- **Windows e Linux**, pendentes. Nada nesta seção saiu do macOS.

E a cada binding novo, repetir a prova de autoridade: `setTimeout` foi o
primeiro, e a sonda já o cobre por tabela — os nomes que ela tenta continuam
todos recusados.

---

## 7. A revisão de `6cc58c1`, corrigida

Cinco correções de ciclo de vida que a integração acrescentou. Nenhuma é mudança
de arquitetura: o contrato de executor, a geração e o registro de recursos
ficaram onde estavam.

### Confirmar parada não era esperar dois segundos

`executorNativo.encerrou` corria a promessa contra um prazo que **resolvia com
sucesso**: a instância se dizia `encerrada` sem que ninguém tivesse parado nada,
o ouvinte era removido, e o contador mostrava zero com o motor ainda parando.

Agora o prazo devolve `false`. A instância só chega a `encerrada` com
confirmação **e** sem recurso que tenha falhado ao sair; sem isso ela fica em
`encerrando`, que é a verdade — pedimos, revogamos, descartamos o que era
nosso, e o executor não disse que parou. Nenhum efeito é admitido nos dois
estados; o que muda é o que o produto **afirma**.

O que não sai fica anotado em `naoSairam`, e o diagnóstico conta as três coisas:
recurso pendente, recurso que falhou, e instância sem confirmação. Do lado
nativo, quem tira da tabela é a bomba ao ver o `Parou` — `mod_nativo_encerrar`
**pede** e não remove.

### A instância nativa não tinha identidade

A tabela era indexada pelo texto do MOD. Sair de um servidor e entrar noutro que
exige o mesmo MOD repõe o mesmo nome, e um encerramento a caminho matava a
execução nova; recarregar um MOD tem o mesmo problema dentro de uma geração só.

Agora a chave é um número que nasce uma vez e nunca se repete, e nome, geração e
hash do pacote viajam com ele — nos eventos, na entrega e no encerramento. O
§3 do contrato: «endereço IP e ID textual do MOD, sozinhos, não identificam uma
execução».

**Registrar vem antes de liberar a execução**, com a conferência de geração
dentro do mesmo cadeado: fora dele, entre conferir e inserir cabe uma
desmontagem inteira. E a revogação nativa encerra os executores dela sozinha, em
`Session::revogar` e na ponte de eventos — a janela pode não pedir, porque pode
estar travada ou já ter fechado.

### O limite não acompanhava os dados

`para_dentro` não tinha teto: `entregar` copiava e enfileirava sem limite. Agora
a entrada tem a mesma contabilidade da saída, e recusa **dizendo** — quem chamou
decide o que fazer, em vez de a fila decidir por ele.

E a bomba devolvia o lugar da saída **antes** de emitir. Agora devolve depois, e
a janela ganhou o teto que faltava: `atenderOMod` é assíncrono, então as
mensagens se acumulam do lado de cá enquanto ele volta ao servidor. Sem ele, um
MOD conversador enchia a memória da janela depois de a fila nativa ter dado o
lugar por livre.

O teto de oito pedidos do prelúdio não conta: ele é código do MOD, e o autor
pode chamar `seele.postar` direto. Há teste para isso.

### O ciclo dos temporizadores tinha um `continue`

No ramo dos vencidos, os pedidos novos não eram recolhidos. Um callback que
agendava outro timeout, ou que cancelava o próprio intervalo, ficava esperando
uma mensagem de fora — que num MOD só de relógio pode nunca chegar. O timeout
encadeado nunca disparava; o intervalo cancelado continuava batendo.

Os atrasos passaram a ser validados: `1e300` é finito, passa por qualquer
conferência de «é número», e `Duration::from_secs_f64` entra em pânico com ele —
derrubar a thread do motor é derrubar o MOD de quem está na sessão por causa de
um argumento. Aparado entre 4 ms e um dia, com `checked_add` no relógio.

E o teto de 256 chega a quem pediu: a fachada guardava o callback de um
temporizador que o anfitrião tinha ignorado, então `setTimeout` devolvia um
número e ele nunca disparava. Agora devolve zero.

### O alcance das medidas

Uma leitura de `ps` que falha passou a ser **medição inválida**, e não zero:
converter em zero faria o relatório dizer «este processo não usa memória».

E o relatório diz o que ele é: binário de teste, sem bomba, sem WebView e sem
mídia, uma coleta. Zero centésimos de CPU é «nada observável nesta resolução», e
não consumo nulo. **A coleta integrada continua por fazer**, e ela é o item 4.

### As provas

Por reversão: o prazo voltando a resolver como sucesso, o ouvinte voltando a
casar pelo nome, e o código voltando a rodar antes do registro — três guardas
falhando com a frase certa.

Por execução: timeout que agenda timeout andando sozinho, intervalo que se
cancela sumindo da tabela do anfitrião, atraso absurdo sem pânico, teto de
temporizadores dito a quem pediu, entrada saturando sem prender o encerramento,
e o teto da fila valendo para quem não usa o prelúdio.

**Windows e Linux: pendentes**, como antes.

---

## 8. As três pendências de `fd1de3a`

### A identidade passou a ser conhecida antes de a execução começar

A subida tinha uma etapa; agora tem duas. `mod_nativo_reservar` devolve o número
**sem rodar nada**, e `mod_nativo_ativar` libera a execução — conferindo a
revogação de novo, porque entre as duas a sessão pode ter acabado.

Isso fecha a corrida sem perder mensagem: o recuo por nome saiu, e ignorar o que
chega antes do número não custa nada, porque entre reservar e ativar **não
existe fala para ouvir**. A reprodução mostrava o pior caso — a mensagem de uma
instância antiga aceita, a resposta perdida porque `entregar` saía com o número
ainda nulo, e o `parou` dela confirmando o encerramento da nova.

### A confirmação tardia conclui a limpeza

O prazo vencido devolvia falso e mantinha o ouvinte, e era só. Um `parou` que
chegasse depois resolvia a promessa antiga e nada mais: o ouvinte ficava, a
instância continuava em `encerrando`, e a promessa já concluída trancava novas
tentativas.

Agora o executor recebe um `quandoConcluir` e o chama quando a confirmação vier
— cedo ou tarde. Ela solta o ouvinte, descarta o que ainda estiver registrado, e
fecha o estado. **Sem readmitir efeito**: o estado só anda de `encerrando` para
`encerrada`, e `admite` é falso nos dois.

E `encerrar` passou a ter três respostas: já acabou devolve verdadeiro, está
esperando devolve a espera em curso, e parou de esperar sem confirmar **tenta de
novo**. Foi a bancada que achou esta última — a versão anterior devolvia o falso
guardado mesmo depois de a instância já estar encerrada.

As instâncias em encerramento saíram do mapa das ativas para um mapa próprio.
«Ativa» voltou a querer dizer ativa, e uma que não confirma fica visível no
diagnóstico em vez de sumir.

### Emitir deixou de ser o critério; colher passou a ser

A bomba não emite carga. Ela guarda a fala na instância, **com o crédito
retido**, e emite um aviso sem corpo: «há o que colher». A janela chama
`mod_nativo_colher`, e é a colheita que devolve o crédito.

É a diferença que a revisão mediu: «no Tauri 2.11.5 presente no checkout,
`webview::emit_js` chama `eval` […]; esse retorno não representa conclusão do
handler JavaScript». Com a janela parada, o MOD bate no teto e nada cresce —
nem do lado nativo, nem no transporte, porque o aviso não carrega bytes.

O caminho de parada é **independente do crédito de dados**: `parou` não entra na
fila de falas, não ocupa byte, e resolve a supervisão na hora. Uma instância que
confirmou mas ainda tem fala a colher continua supervisionada — o que o MOD
disse antes de parar ainda precisa chegar.

### Onde as reproduções moram

`apps/seele-app/bancada/ciclo-do-executor.cjs`, rodada por `cargo xtask
check-runtime`. Ela carrega `ui/mods-runtime.js` de verdade num contexto de VM e
controla a ordem dos eventos — que é a única coisa que separa um caminho correto
de um que só parece correto.

Fora de `cargo test` porque precisa de Node, e a bateria do produto não pode
precisar: quem a roda é quem compila o SEELE, e o SEELE não usa Node para nada.
Sem Node, `check-runtime` **reprova** em vez de passar — quem chamou pediu pela
prova.

E fora de `testes/` porque aquilo guarda **vetores**, cujos bytes são o contrato
e por isso estão declarados `-text`. Um instrumento ali faria aquela regra dizer
de si mesma o que não é verdade — o guarda de fim de linha apontou isso sozinho.

As quatro corridas: fala antiga durante a subida, confirmação depois do prazo,
segunda tentativa de encerrar, e colher em vez de receber. Cada uma reprova
quando o conserto dela é revertido.

**Windows e Linux: pendentes**, como antes.

---

## 9. Os três pontos nativos de `8adebb8`

### Sair com fala na fila deixava dados retidos para sempre

`confirmar_parada` mantinha a instância enquanto houvesse pendentes, e
`mod_nativo_colher` recusa a geração que saiu. Não havia caminho: os bytes e a
instância ficavam retidos depois do `Parou`, esperando uma colheita impossível.

Agora a revogação **descarta nativamente** o que não foi consumido e acerta os
créditos, sem a janela — ela pode ter fechado, e o que ela faria não é entregar:
efeito de uma sessão encerrada não se entrega, se joga fora. A bomba também
descarta o que chega depois da revogação, contando cada fala para o descarte não
ser silencioso.

E `codigos_reservados` entrou no mesmo grupo: a tabela só perdia uma entrada na
ativação, então reservar e sair sem ativar conservava o fonte do MOD na memória
desta janela para sempre.

### Uma parada espontânea não mudava o estado nativo

`confirmar_parada` escrevia `parou = true` só na tabela de encerramento; para a
de vivas ela conferia `i.parou` sem nunca tê-lo escrito. Uma instância cujo
motor caísse sozinho não era removida nem marcada, e continuava descrita como
ativa.

Agora o tratamento é um só para os dois estados: a viva é marcada, movida e
resolvida pelo mesmo critério — sai da supervisão quando não há mais nada dela
para colher.

### Erro e mensagem não podiam dividir a mesma conta

A bomba punha `Falhou` e `Interrompido` na mesma fila das mensagens. Eles são
mandados pelo executor **sem reservar crédito**, e a colheita subtraía
quantidade e bytes de todas as falas — colher um erro devolvia crédito que
ninguém tomou, e repetir erros contornava o teto.

Agora são duas listas. Mensagem reserva e devolve; aviso fica fora da
contabilidade, com teto próprio de 16, agregação do que se repete e um contador
do que não coube. O caminho de parada continua separado dos dois: não entra em
fila nenhuma, não ocupa byte, e resolve a supervisão na hora.

### As provas

Oito testes nativos, **sem janela nenhuma**: nem `AppHandle`, nem evento, nem
colheita da interface. Produzir, impedir a colheita, revogar, confirmar a
parada, e conferir tabelas, bytes e créditos zerados. Reserva sem ativação
entrando no descarte. Parada espontânea com e sem fala pendente. Erro sem
mensagem alguma, erros intercalados com mensagens, e dez mil erros com a janela
parada — os contadores ficam entre zero e o teto, e uma mensagem nova ainda
cabe.

Por reversão: a revogação deixando de soltar, a parada espontânea deixando de
marcar, o erro voltando para a fila de dados, o teto de avisos saindo, e a
colheita voltando a devolver crédito de tudo.

**Duas lições de medição** nestes testes. O erro na fila de dados só aparece
chamando o caminho de verdade — a primeira versão chamava `anotar_aviso` direto
e passava com o defeito de volta; por isso a rota saiu da bomba para um método
que dá para medir. E a conta errada só aparece **com mensagens ainda
pendentes**: drenando tudo de uma vez o contador satura em zero e o excesso fica
invisível.

**Windows e Linux: pendentes**, como antes.

## Seção 9 — a reserva atômica e a cota de avisos até a janela

### O fonte da reserva saiu da tabela ao lado

`codigos_reservados` era um mapa em `Session`, preenchido **depois** de o
cadeado das instâncias ser liberado. Entre as duas inserções cabia uma
revogação: ela varria a instância e apagava um fonte que ainda não existia, e o
comando o inseria depois, num mapa de uma sessão já encerrada.

Agora o fonte mora **dentro** de `InstanciaNativa`, como `codigo:
Option<String>`. A instância nasce inteira, `soltar` larga o fonte pelo mesmo
ato que larga o resto, e a ativação o retira com `take`. A revisão pedia «uma
prova com barreira entre registrar a instância e guardar o fonte»: **essa
barreira não tem onde ser posta**, porque não são duas inserções. E a montagem
que não completa — a bomba que não sobe — desfaz o que já entrou, em vez de
deixar a instância esperando uma ativação que pode nunca vir.

### A cota de avisos vale da produção até a janela

Os avisos saíam do motor sem reservar nada. Um MOD que lança num laço enchia o
canal entre o motor e a bomba, que nenhum teto de fila alcançava. Agora há uma
cota própria, `AVISOS_NA_FILA = 16`, reservada por `avisar` na saída do motor e
devolvida só no consumo ou no descarte — é a **mesma reserva** no canal e na
instância, e por isso há um número só, `avisos_de_pe`.

E a notificação virou **uma** por colheita. A versão anterior emitia um evento
por fala, inclusive pelas que ela mesma agregava ou descartava; um evento sem
corpo ainda ocupa memória enquanto a janela não o processa. `precisa_avisar`
arma uma vez e só rearma quando as duas listas esvaziam.

No descarte, cada classe volta pela cota dela: `devolver_credito` roteia
mensagem para a fila de dados e aviso para a de avisos.

### O que a medição corrigiu no meio do caminho

**Uma corrida que nenhuma disputa alcança.** A primeira prova da reserva foi uma
corrida por thread: registrar de um lado, revogar do outro. Ela passava — e
passava também **com a ordem invertida de propósito**. A janela que a inversão
abre fica entre soltar o cadeado e incrementar a geração, dura poucas
instruções, e acordar de um mutex custa mais do que ela. Duzentas rodadas sem
disputa: nenhuma falha. Trinta rodadas com a tabela povoada e quatro
registradores empilhados no cadeado: nenhuma falha.

Um guarda que não falha com o defeito presente não é um guarda. O arranjo antigo
era correto — quem registrasse antes era varrido, quem registrasse depois via a
geração morta —, mas correto de um jeito **que não dá para provar**. Então a
forma mudou: `revogar_em` fecha a geração **dentro** do cadeado que varre. Não
há mais ordem a inverter, registrar e revogar se excluem inteiros, e a
propriedade virou observável: segurando o cadeado, a geração não muda. Esse
guarda falha na primeira volta quando o incremento sai de dentro.

**Um guarda textual que acusava a si mesmo.** A primeira versão conferia que
`main.rs` não continha mais o nome da tabela antiga — e o próprio `assert`
continha o nome. Partir o literal resolveria a colisão, mas o nome sobrevive de
propósito nos comentários que explicam o defeito: um guarda que proíbe **falar**
do erro é pior que nenhum. Ficou o comportamental, e a forma é conferida onde
ela importa, em `frontend.rs`.

**O produtor de erros real não enche a lista: ele agrega.** A prova pedida
esperava `avisos_perdidos > 0`. Medindo, o número é zero — e está certo: um MOD
que lança num laço manda sempre o mesmo texto, e a agregação transforma as
repetições em contagem, devolvendo o lugar na cota a cada uma. A perda do mais
velho é o **outro** caminho, o de avisos diferentes entre si, e já tinha guarda.
A expectativa é que estava errada, não o produto.

### As provas

Treze testes nativos, sem janela nenhuma. Os cinco novos:

- **Exclusão mútua**, determinística: enquanto um registro segura o cadeado, a
  geração não sobe e a varredura não roda. Por reversão — o incremento de volta
  para fora do cadeado —, falha na volta zero.
- **Corrida com disputa**, 30 rodadas, tabela povoada, quatro registradores: não
  sobra fonte nem instância viva de uma geração revogada. Vale como guarda de
  regressão para o fonte voltar a morar fora da instância; **não** distingue a
  ordem do incremento, e o registro acima diz por quê.
- **Reserva sem ativação**: revogar deixa a supervisão sem fonte órfão, e a
  instância continua supervisionada até confirmar.
- **Produtor de erros real nas três cotas**: o MOD lança a cada resposta
  entregue, ninguém escoa o canal, ninguém colhe. Mede os três pontos de
  retenção — o que o motor recusou, o que está no canal, o que está na
  instância — e conta **uma** notificação. Por reversão, tirando a cota do
  motor: `avisos_recusados` fica em zero e o guarda estoura por prazo.
- **Erro tardio entre mensagens ainda contabilizadas**: o MOD fala cinco vezes e
  **só então** lança; as mensagens seguem retidas quando o erro chega. Assentar
  não mexe em cota nenhuma; a colheita devolve cada classe pela sua. Por
  reversão — a colheita soltando aviso pela cota das mensagens —, a de avisos
  fica em um.

A coordenação saiu dos comandos para `registrar_reserva`, `liberar_reserva` e
`assentar_fala`, e é isso que deixa as provas exercitarem **o caminho real** em
vez de uma reimplementação dele. `mod_nativo_reservar`, `mod_nativo_ativar` e
`bombear` passaram a ser a casca que traz `AppHandle` e `State`.

A bancada também dizia «as três corridas passam» com quatro provas na lista. O
número passou a ser contado.

**Windows e Linux: pendentes**, como antes.

## Seção 12 — a colheita, a API completa, os três MODs e o guia

### A intermitência tinha causa, e ela era a colheita

A revisão reproduziu o que eu não tinha conseguido isolar, na camada JS e sem
tocar no transporte: o Rust atende uma colheita vazia, a resposta ainda está a
caminho do JavaScript, e nisso uma mensagem nova entra e produz o aviso dela. A
janela recebia esse aviso com `colhendo = true` e o ignorava; a resposta vazia
chegava, o coletor saía, e a mensagem ficava retida sem ninguém para buscá-la.

Bate com o que o registro mostrava e eu não soube ler: a terceira fala chegava à
bomba e a colheita seguinte levava zero.

**Confirmado no aplicativo recompilado**: seis execuções de seis sobem a fatia
inteira — cinco falas, mídia servida, zero descartes. Antes eram três de seis
travando. Uma execução boa não prova nada numa falha intermitente; seis contra
três de seis, provam.

E com a fatia subindo de forma confiável, a medição que faltava fechou. As duas
fases colhidas seguidas, mesma regra de processos, janelas de 30 s:

| Fase | Footprint | CPU |
| --- | --- | --- |
| sem MOD | 246,7 MiB | 6,0% de um núcleo |
| fatia montada | 267,9 MiB | 6,1% de um núcleo |

**+21,2 MiB e +0,1 ponto.** A CPU quase não muda porque a fatia, depois de
montar, não faz nada: sem temporizador, sem laço, acordando só por evento. É o
que o desenho previa, e por isso mesmo não diz nada sobre custo **sob
interação** — isso continua pendente pelo impedimento de automação.

### A API cresceu onde um aviso a substituía

O que os três MODs oficiais tinham virado depois da migração, textualmente:

- ESTILO: «Edição de tema, tipografia, espaçamento, arredondamento e brilho
  aguardam suporte do SEELE.»
- PERFIS: «edição, imagens, efeitos e cartões na lista de pessoas aguardam
  suporte do SEELE.»
- MESA: «Criação, edição, rolagens, tabuleiro interativo, imagens e áudio
  aguardam suporte da API do SEELE.»

Três telas de leitura. A emenda de 19/09 do ADR 0049 decide o que entra, e a
razão de cada coisa está lá. Entraram: `campo`, `escolha`, `botao`, `tela` com
**figuras declaradas**, `midia` de duas origens, `SeeleUI.aoEvento`, e no tema
`painel`, `apagado` e `densidade`.

Duas decisões que custaram medição para ficarem certas:

**O alvo do arraste é do produto.** Sem ele o MOD receberia um par de
coordenadas e teria de refazer o acerto — pior, porque ele não sabe a ordem em
que as figuras ficaram na tela. A primeira prova que escrevi passava com o alvo
desligado, porque tinha **uma** peça: qualquer adivinhação por posição acerta.
Com duas peças na mesma casa, a reversão reprova.

**A continuação da mídia é opaca.** Uma imagem de perfil não cabe numa resposta
só, e os dois MODs que paginam o fazem de jeitos diferentes. O produto junta o
`proximo` ao pedido seguinte sem interpretá-lo; interpretar seria o produto
conhecer a forma de um MOD.

### Os três MODs

ESTILO edita as seis cores e a densidade, grava e restaura, e devolve ao
servidor o que ele não edita — zerá-lo seria apagar a escolha de outra pessoa.
PERFIS lista por ID, abre a ficha, edita a própria, e mostra retrato e faixa
vindos da metade de servidor. MESA desenha o tabuleiro com grade, paredes e
peças, arrasta peça com confirmação do servidor, rola dados e passa turno.

Suítes deles: 14, 25 e 30 testes. Três defeitos caíram no caminho, e um deles é
instrutivo: os identificadores da MESA são texto (`token-2`), e convertê-los
para número dava `NaN` — que não é igual a nada, nem a si mesmo. A peça deixava
de se reconhecer e não acompanhava o dedo, **sem erro nenhum aparecer**.

### O que a execução nativa dos três mostrou

Com um build local em API 3 — a separação que a seção 12 autoriza —, os três
pacotes subiram no mesmo servidor: três instâncias ativadas no executor nativo,
111 falas, 49 pedidos ao servidor, **zero falhas, zero descartes e nenhuma
recusa registrada pela janela**.

O bump foi revertido em seguida. Ele não pode entrar sem a publicação: o produto
exige que o manifesto declare **exatamente** a API oferecida, então subir a
constante sozinha faria o aplicativo recusar todo MOD instalado hoje.

### A sequência que falta, e o passo que é seu

1. Regerar o catálogo no indexador com `api_oferecida: 3` e publicar os três
   pacotes em 2.0.0. **Precisa da sua chave** — é o único passo que precisa.
2. Copiar o catálogo novo e a assinatura para `apps/seele-app/testes/`.
3. Subir `MOD_API_VERSION` para 3 neste repositório. O guarda
   `o_indexador_e_este_build_oferecem_a_mesma_api_de_mods` fica verde quando o
   passo 2 tiver sido feito, e vermelho enquanto não — que é o comportamento
   certo dele.

**Windows e Linux: pendentes**, como antes. E a interação nativa — digitar,
arrastar, ouvir, medir sob voz — continua pendente pelo impedimento de
acessibilidade do `osascript`, que é daquele método de automação e não do
produto.
