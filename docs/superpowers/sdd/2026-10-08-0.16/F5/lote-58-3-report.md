# Relatório do lote 58-3 (F5, 0.16)

O item 52 da pendência 58: a tentativa substituída apagava a linha de etapa da
nova, e as etapas da velha chegavam à linha da nova antes do `connect` dela.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-aeb9cfc4380b179c4`
- Branch: `0.16/lotes-58-3`, aberto em `40ceb429` (a ponta dada na mensagem).
- Brief: `.superpowers/sdd/lotes/lote-58-3-brief.md`, com `regras-58.md`, a
  seção do item 52 em `triagem-58.md`, o texto do item em `docs/pendencias.md`
  e o «A linha de etapa» do relatório do W2.
- Arquivos mudados: `apps/seele-app/ui/tela-boot.js` e
  `apps/seele-app/bancada/ajustes-v013.cjs`, mais este relatório.
- Status: **DONE_WITH_CONCERNS**. As ressalvas estão no fim, e nenhuma bloqueia.

## Os commits

| commit | o que entra |
|---|---|
| `bb5d9bdf` | o conserto em `tela-boot.js` e a cena que o prova, em `ajustes-v013.cjs` |
| este | o relatório |

## A prova: a cena e o duble

A cena é `aLinhaDeEtapaEDaMaisNova`, a última da `ajustes-v013.cjs`. Nenhuma
cena emitia etapas: o `listen` de todas é vazio, e o W2 leu o defeito no
código sem medi-lo. O duble desta cena:

- **guarda os ouvintes** de `seele://event`, e o `connect` emite
  `ConnectStageChanged` nos tempos que o caso dá, contados do começo dele;
- **modela a geração como o `main.rs`.** Cada `connect` revoga a anterior ao
  começar, e a etapa de uma geração revogada não é emitida, como na
  `Bridge::on_event`;
- **lê a linha em cada emissão.** Cada etapa guarda o que `#boot-etapa` diz
  logo depois dela. O ouvinte escreve na mesma volta, então a leitura não
  depende de amostrar no tempo certo;
- **não deixa ninguém entrar.** A tentativa revogada volta como substituída, e
  a de pé, recusada. A cena fica na tela de entrada, onde a linha mora.

As duas tentativas começam pela lista (`irParaOServidor`). A velha vai a
`192.0.2.17` e a nova, aos 100 ms, a `192.0.2.18`. A etapa usada é `Tentando`,
porque a frase dela leva o endereço: dá para ver de qual servidor é a linha.

São três casos. Os dois primeiros são os do brief. O terceiro é a outra
metade do segundo, e explico por que ele entrou na ressalva 1.

| caso | a corrida | exige |
|---|---|---|
| 1 | a velha leva 1,2 s; a nova chega ao `connect`, diz uma etapa aos 50 ms dele e leva 2 s | com a velha de volta, a linha ainda diz a etapa da nova |
| 2 | a velha diz etapas aos 20 e aos 400 ms do `connect` dela; a nova espera 600 ms no `conferir_esta_versao` | a segunda etapa da velha, com a geração dela de pé e a nova antes do `connect`, não chega à linha; e a etapa da nova, depois do `connect` dela, chega |
| 3 | a velha espera 400 ms no `conferir_esta_versao` e a nova, 800 ms; a velha chega ao `connect` primeiro e diz uma etapa | a etapa da velha não chega à linha da nova |

Cada caso também confere as próprias condições: que a etapa foi emitida e
que a nova ainda não tinha chegado ao `connect`. Se a corrida mudar, ele
reprova dizendo que não mede nada, em vez de passar por engano. No fim de
cada caso, a nova volta recusada e o `finally` dela tem de apagar a linha.

## O vermelho, antes do conserto

A cena rodou contra o `tela-boot.js` de `40ceb429`. Os três casos reprovaram,
cada um com a sua frase. Para medir cada caso sozinho, usei uma cópia da cena
no rascunho que pula os casos de cima. A bancada do ramo não tem esse modo.

| caso | a frase |
|---|---|
| 1 | «a tentativa a 192.0.2.17:8384, que a de 192.0.2.18:8384 já tinha substituído, terminou e apagou a linha de etapa da nova enquanto ela andava: a linha diz null, e não «TENTANDO O ENDEREÇO 1 DE 2 DO CONVITE · 192.0.2.18:8384» até a etapa seguinte da nova» |
| 2 | «entre o começo da tentativa a 192.0.2.18:8384 e o connect dela, a etapa da tentativa a 192.0.2.17:8384, que ela substituiu, foi escrita na linha de etapa: «TENTANDO O ENDEREÇO 2 DE 2 DO CONVITE · 192.0.2.17:8384», sobre um servidor que a pessoa deixou» |
| 3 | «a tentativa a 192.0.2.17:8384 saiu do conferir_esta_versao depois de a de 192.0.2.18:8384 começar, e a etapa dela foi escrita na linha de etapa da nova: «TENTANDO O ENDEREÇO 1 DE 1 DO CONVITE · 192.0.2.17:8384» — foi a velha que reabriu a linha que a nova tinha calado» |

A bancada inteira, rodada contra a base, parou no caso 1, com a mesma frase.

## O conserto

Em `conectar`, os dois passos do brief, sem tocar no fio nem na FFI:

1. **No `finally`,** `if (esta === tentativas) mostrarEtapa(null);`. O
   comentário que admitia o defeito saiu, e o novo diz por que a velha não
   precisa apagar nada.
2. **A bandeira `etapasCaladas`.** Ela é posta logo depois do `++tentativas` e
   tirada logo depois do `await invoke("conferir_esta_versao")`. Entre a
   tirada e o `invoke("connect")` não há espera. No ouvinte de
   `seele://event`, um `ConnectStageChanged` que chega com ela posta é
   descartado.

**A tirada é condicionada:** `if (esta === tentativas) etapasCaladas = false;`.
Uma velha que sai do `conferir_esta_versao` depois de a nova começar não
reabre a linha. O brief diz «tirada logo antes do `invoke("connect")`», e não
diz por quem. O caso 3 mostra que, sem a condição, a velha a tira (ressalva 1).

São 5 linhas de código. Uma delas substitui o `mostrarEtapa(null)` de antes.
Com os comentários, o `tela-boot.js` ganha 30 linhas e perde 9. A cena tem 196
linhas.

## As reversões

Cada uma foi aplicada por script (`reverter.py`, no rascunho) sobre uma cópia
do arquivo consertado. Depois rodei a cena três vezes: inteira, sem o caso 1,
e só o caso 3. Por fim devolvi o arquivo com `cp` sem `-p` e conferi com
`cmp` que voltou idêntico.

| reversão | o que se desfez | inteira | sem o 1 | só o 3 |
|---|---|---|---|---|
| R1 | o `finally` apaga a linha para toda tentativa | reprova o 1 | verde | verde |
| R2a | o ouvinte não olha a bandeira | reprova o 2 | reprova o 2 | reprova o 3 |
| R2b | a bandeira nunca é posta | reprova o 2 | reprova o 2 | reprova o 3 |
| R3 | a tirada sem `esta === tentativas` | reprova o 3 | reprova o 3 | reprova o 3 |
| R4 | a bandeira nunca é tirada | reprova o 1, pela condição: a etapa da nova não chegou | reprova o 2: «calar as etapas antes do `connect` calou também as de depois» | verde |

Do R1 ao R3, cada reprovação veio com a frase do vermelho do seu caso. O R4
reprova por outras frases, e prova que a tirada existe e funciona: sem ela, a
linha nunca mais mostra etapa nenhuma.

A primeira rodada das reversões veio antes de uma mudança só de comentário:
as três linhas, no doc da bandeira, sobre a mais nova que termina antes do
`connect`. Rodei tudo de novo sobre o arquivo do commit `bb5d9bdf`: a cena
verde nas três formas, e as cinco reversões, com os mesmos resultados da
tabela. Cada uma foi devolvida idêntica, conferida com `cmp`.

## Os guardas

- **A `ajustes-v013.cjs` e as bancadas do job `bancadas`:** as 20 de `node`
  do `ci.yml`, na ordem dele, deram verde, com `PLAYWRIGHT` apontado para a
  instalação de sempre. A `ajustes-v013.cjs` rodou inteira de novo sobre o
  arquivo do commit, e deu verde.
- **A bancada do celular** (`tools/bancada-do-celular.py`, do mesmo job, no
  Chromium e no WebKit): «a bancada passa», com os 150 casos em «passa».
  Ela importa aqui porque a «ENTRAR EM» do celular escreve na mesma
  `#boot-etapa`.
- **Os roteiros de `tools/roteiros/`,** numa chamada à `carga-da-casca.py`:
  os 46 passaram, e nenhum script estourou.
- **`cargo --offline test -p seele-app --test frontend`,** com HOME falsa,
  por script executável: 306 passaram, com 746 linhas no log. **Nenhum guarda
  de texto precisou mudar.** Os que leem `conectar` (os do CONECTAR, os do
  E54, o da recusa por apelido e o do ouvinte de etapas) cobram linhas que o
  conserto não tocou. Nenhum guarda novo entrou no `frontend.rs`: a prova é a
  bancada, como a triagem pede para o lote 3.
- **`cargo --offline xtask check-runtime`,** com a mesma HOME falsa: verde.

## As ressalvas

1. **Um terceiro caso e uma condição a mais que o brief.** O brief diz
   «tirada logo antes do `invoke("connect")`», e o código não confere a
   tentativa entre o `conferir_esta_versao` e o `connect`. Sem a condição
   `esta === tentativas`, uma velha parada no `conferir_esta_versao` quando a
   nova começa sai dele, tira a bandeira da nova, chega ao `connect` com a
   geração dela de pé no Rust, e as etapas dela voltam à linha da nova. É o
   mesmo trecho do passo 2, com a velha um passo atrás. O caso 3 o reproduz
   na base, e a reversão R3 reprova só ele. Se a orquestradora preferir o
   brief ao pé da letra, o R3 é a forma, e o caso 3 sai junto.
2. **A bandeira fica posta quando a mais nova termina antes do `connect`.**
   É o caso da versão recusada pelo `conferir_esta_versao`. Fica assim de
   propósito: a bandeira cala as etapas de uma velha que ainda anda, e a
   tentativa seguinte a tira no `connect` dela. O comentário diz isso. No
   desktop, só o `conectar` chama o `connect`, então nada além de uma velha
   fica calado. A «ENTRAR EM» do celular conecta por `identidade-web.js`, sem
   passar pelo `conectar`, e escreve na mesma `#boot-etapa`. Lá a bandeira não
   fica presa, por dois motivos lidos no código e não medidos: a ponte
   responde `null` ao `conferir_esta_versao`, e o `perguntarONome` não lança.
3. **O preço que o W2 apontou ficou menor, mas ainda existe.** Ele não foi
   medido. Uma velha que escreveu uma etapa antes de a nova começar, e
   termina enquanto a nova ainda está no `apelido_local` ou na pergunta do
   nome, deixa essa etapa na linha. Antes, o `finally` dela a apagava. Agora
   ela fica até a nova chegar ao `mostrarEtapa(null)` de antes do `try`. O
   trecho só tem espera quando a janela não tem apelido lembrado: na primeira
   entrada da execução, ou na seguinte a uma recusa por apelido. Aí ela lê o
   `apelido_local` e, sem nome nenhum, pergunta. Com a pergunta aberta, a
   caixa fica por cima da linha.
4. **O `target`.** Foi semeado com `cp -c` de
   `/Users/dev-alexandre/SEELE/target` (`debug/{deps,build,.fingerprint}`).
   Nada foi movido. O `wasi-sdk` não foi clonado, porque o lote não pede o
   `xtask web`: não mudou nada de Rust nem de WASM.

## Depois: a bandeira saiu no lote 52b

A `etapasCaladas` saiu de `tela-boot.js` no lote 52b (`a45f3dc7`). O
mecanismo que ficou é o da F4 (`0975f08a`): o `connect` leva a tentativa da
janela, a `Bridge` a devolve em cada `ConnectStageChanged`, e o ouvinte mostra
só as da mais nova. O `esta === tentativas` do `finally` fica. Os três casos
desta cena ficam também: o duble passou a devolver a tentativa do `connect`
na etapa (`fd10230c`), e sem o filtro da tentativa reprovam o 2 e o 3. As
ressalvas 1 e 2 acima falam da bandeira e valem só para ela.
