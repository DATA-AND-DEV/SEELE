# Lote W3 da F5: relatório

Base `b4dcf282` (a ponta do lote W2). Branch `0.16/lotes-w3`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a8ba192d9719be169`. Tudo
local: sem push, tag nem release. Sem conformidade. Nada no fio.

| commit | o que muda |
|---|---|
| `a5451bb2` | `aceite-de-mods.js` responde a `mods_instalados` e passa a exigir o que o cabeçalho dizia provar |
| `beafc181` | as frases de reprovação dos itens, sem plural a errar |
| `c27d84cb` | o cabeçalho do roteiro diz onde estão as reversões |
| `7b5e53fd` | a primeira versão deste relatório |
| `b91d2641` | `catalogo-de-mods.js` passa a exigir o que o cabeçalho diz provar (§6) |
| `5bc3c84e` | **produto**: a rejeição de `aceitarOsMods` vira frase no diálogo, o ACEITAR volta e o motivo vai ao `seele.log`; o roteiro novo `aceite-de-mods-que-falha.js` (§7) |
| `9a53cc13` | `aceite-de-mods.js` exige o sim gravado antes do `connect` da volta (§3, E14) |
| `142505db` | o relatório com as medidas e o que falta rodar |
| `ee11e9f9` | a rodada 1 (escrita em curso, sem rodar): I1, I2 e os Menores M1, M2, M4 e M5 (§11) |
| `23b9cdbc` | a rodada 2 (escrita em curso, sem rodar): a corrida de `abrirAceiteDeMods` (§12) |
| `a46f3f86` | as rodadas 1 e 2 rodadas e verdes, com as medidas no §13 |
| `0d56ef6d` | rodada 3, M6: o ACEITAR apagado na janela da abertura nova (§14) |
| `b0e032ff` | rodada 3, M2b: o guarda da credencial lê os três aliases (§14) |
| este | o relatório da rodada 3 |

**O que mudou fora de `tools/roteiros/`.** Em `apps/seele-app/ui/camada-mods.js`,
uma função nova (`aceiteFalhou`) e o `.catch` do clique em ACEITAR; em
`apps/seele-app/tests/frontend.rs`, uma linha na lista do guarda da credencial
(§7). **Na rodada 1 (§11, não rodada):** em `camada-mods.js`,
`levarParaOAceiteDeMods` e duas funções novas (`perguntaNaoAbriu`,
`motivoEmTexto`), e a frase de `aceiteFalhou`; em `camada-mods.css`, o
`pre-line` de `#mods-erro`; na rodada 2, a conferência de `abrirAceiteDeMods` depois do `await` (§12); em `frontend.rs`, três linhas na lista do guarda da
credencial e a leitura de `pergunta` nas três funções. Nada em `crates/`, no
fio, em `tools/carga-da-casca.py`, em `main.rs`, em `tela-boot.js`. Os
outros roteiros que usam `exigir` não foram rodados de novo: o aparelho que os
roda não mudou.

## 1. A medida: o roteiro, e não o produto

O roteiro relatava, na base e no W2:

```
gravou o sim: alvo=undefined conjunto=undefined
tentou entrar de novo: false
diálogo depois: VISIVEL
```

e saía com `carga limpa`, porque não havia um `exigir`.

**A sequência de `invoke` que o roteiro vê depois de apertar ACEITAR**
(`window.__SEELE_CHAMADAS`, com o `__SEELE_ARGS` limpo antes do clique):

```
["mods_instalados"]
aceitar_mods args: undefined      botão ACEITAR: disabled      #mods-erro: escondida
console.warn: aceitar mods: TypeError: Cannot read properties of null (reading 'some')
              (camada-mods.js:415)
```

O fluxo para no primeiro comando. `aceitarOsMods` chama `obterOConjunto`, que
chama `invoke("mods_instalados")` e faz `instalados.some(...)`. O duble de
`carga-da-casca.py` não tinha resposta para `mods_instalados`, e a ponta
final do duble devolve `null` a todo comando que não conhece (a lista
`o duble respondeu null a:` do roteiro já dizia `mods_instalados`). O `.some`
de `null` estoura, a promessa de `aceitarOsMods` rejeita, e o `.catch` do
`click` a transforma num `console.warn`. `aceitar_mods` nunca é chamado
(o `alvo=undefined` vinha do `?? {}` do roteiro antigo, que lia o argumento de
um comando que não foi chamado), e `connect` nunca é chamado de novo.

**Não é defeito do produto.** O comando de verdade devolve
`Result<Vec<ModNaTela>, FalhaNoMod>` (`main.rs`, `mods_instalados`), e
`mods_ligados` e `quem_pinta_o_servidor` devolvem vazio quando a janela não
hospeda: no momento do ACEITAR, com a entrada recusada, o comando responde uma
lista. O `null` só existe no duble. (O que o produto fazia com um `null` quando
ele viesse é o §7.)

**A prova de quando começou**, com o mesmo roteiro antigo contra a casca de
cada época (`--casca` aponta o `ui/` exportado do histórico):

| casca | `gravou o sim` | `tentou entrar de novo` | `diálogo depois` |
|---|---|---|---|
| `ea75ac5d^` (18/09, antes de o aceite obter o conjunto) | `alvo=203.0.113.9:8383 conjunto=c0ffee1234567890` | `true` | `escondida` |
| `ea75ac5d` (o aceite passa a obter o conjunto) | `alvo=203.0.113.9:8383 conjunto=c0ffee1234567890` | **`false`** | `VISIVEL` |
| `b4dcf282` (a base deste lote) | **`alvo=undefined conjunto=undefined`** | `false` | `VISIVEL` |

Foram duas quebras, em dois momentos. A `ea75ac5d` (2026-09-18) pôs a
conferência de `mods_instalados` antes da volta, e a volta deixou de ser
tentada no roteiro; o sim ainda era gravado antes da conferência, e por isso
`alvo` ainda saía certo. O lote W (I-3) moveu a gravação para depois da
conferência (o «não» de quem desiste não pode ser desmentido), e aí o sim
também sumiu do roteiro. São 1347 commits entre a `ea75ac5d` e a base, e o
roteiro leu «carga limpa» em todos.

## 2. O que faltava

Uma linha: o duble precisava responder a `mods_instalados` com os pacotes que o
anúncio exige, já no disco, para a conferência passar sem obter nada:

```js
SEELE_RESPOSTAS.mods_instalados = ANUNCIADOS.map(({ id, hash }) => ({ id, hash }));
```

Só `id` e `hash` importam a `obterOConjunto` (a `ajustes-v013.cjs` usa o mesmo
formato). Com os dois iguais aos do anúncio, nada é obtido e o aceite segue
direto para `aceitar_mods` e `connect`. O resto do lote é o roteiro passar a
cobrar o que relatava.

O argumento `aceitar_mods` não mudou de nome (`{ alvo, conjunto }`), e nenhum
comando novo precisou de resposta além de `mods_instalados`.

## 3. As exigências de `aceite-de-mods.js`

A sequência depois de ACEITAR agora é `mods_instalados → aceitar_mods →
conferir_esta_versao → connect`, e o roteiro a relata. O primeiro `connect` do
roteiro leva um convite (`conectar(ALVO, "eu", CONVITE)`), para a volta ter
credencial a devolver.

| # | exigência | reversão que a faz reprovar |
|---|---|---|
| E0 | a primeira tentativa levou o convite (sem isso o roteiro não mede a volta) | R0b |
| E1 | o diálogo `#mods-aceite` está aberto | R1 |
| E2 | a linha vermelha `#boot-erro` está fechada | R2 |
| E3 | tantos itens quanto o anúncio | R3 |
| E4 | cada item: id, `versão X` e o hash do `title`, na ordem do anúncio | R3b |
| E5 | `#mods-contagem` diz «2 MODS» | R4 |
| E6 | a linha «roda na máquina de quem hospeda» só no MOD que a tem | R5 |
| E7 | o aviso de lista mudada, porque o aceite anterior era de outra lista | R6 |
| E8 | `aceitar_mods` foi chamado | R0 |
| E9 | o sim leva o alvo da tentativa (`203.0.113.9:8383`) | R7 |
| E10 | o sim leva o conjunto do anúncio (`c0ffee1234567890`) | R8 |
| E11 | a volta chamou `connect` | R9 |
| E12 | a volta leva a credencial da tentativa (`joinSecret`) | R10 |
| E13 | o diálogo está fechado depois | R11 |
| E14 | **`aceitar_mods` vem antes do `connect` da volta** | R12 |

As reprovações de E8, E11 e E14 dizem a sequência de comandos que o roteiro
viu, até o `connect` da volta, e é o que teria dito a causa do `alvo=undefined`
no dia em que ela nasceu.

## 4. As reversões de `aceite-de-mods.js`

Uma por exigência, e não só as três do pedido. Cada uma é uma linha editada no
produto (ou no duble, para R0 e R0b), o roteiro rodado, o arquivo restaurado e
conferido com `cmp` contra a cópia (`cmp` igual em todas), e o `git status`
limpo no fim. O script de reversão acusa se a substituição não mudou nada.
Todas saem com `exit=1` e `REPROVADO` com a frase da exigência.

| # | a reversão | arquivo | a frase que o roteiro escreve |
|---|---|---|---|
| R0 | o duble sem a resposta de `mods_instalados` (**a causa medida**) | roteiro (cópia) | `ACEITAR não gravou o sim: aceitar_mods nunca foi chamado (a sequência foi: mods_instalados)` |
| R0b | a primeira tentativa sem convite | roteiro (cópia) | `a primeira tentativa não levou o convite do roteiro, e a volta que aceita não tem o que devolver: o roteiro não mede nada` |
| R1 | a recusa por MODs não é reconhecida (**o diálogo que não abre**): `motivo?.ModsNaoAceitosX` | `camada-mods.js` | `a recusa por MODs não abriu o diálogo de aceite (diálogo=escondida, linha vermelha=VISIVEL: «ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU.»)` |
| R2 | abre o diálogo e cai também na linha vermelha: `levarParaOAceiteDeMods(...) && false` | `tela-boot.js` | `a recusa por MODs abriu o diálogo e também escreveu a linha vermelha da entrada: «ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU.»` |
| R3 | um item a menos: `mods.slice(0, 1).map(linhaDeMod)` | `camada-mods.js` | `os itens desenhados não são os do anúncio: o diálogo desenhou 1, e o anúncio trouxe 2` |
| R3b | os itens em outra ordem: `mods.slice().reverse().map(linhaDeMod)` | `camada-mods.js` | `o item 1 do diálogo é «ponte/retransmissor · versão 0.8.2 · bb22…», e o anúncio trouxe «seele/placar · versão 1.4.0 · aa11…»` |
| R4 | a contagem errada: `${mods.length + 1} MODS` | `camada-mods.js` | `a contagem do diálogo diz «3 MODS», e o anúncio trouxe 2` |
| R5 | sem a linha do que roda no servidor: `if (false && mod.no_servidor)` | `camada-mods.js` | `as linhas de «roda na máquina de quem hospeda» não são as do anúncio: o diálogo desenhou 0, e o anúncio marca 1 (no_servidor)` |
| R6 | sem o aviso de lista mudada: `.hidden = true` | `camada-mods.js` | `esta máquina já tinha dito sim a outra lista deste servidor, e o diálogo não avisou que a lista mudou` |
| R7 | **o sim gravado sem alvo**: `invoke("aceitar_mods", { conjunto })` | `camada-mods.js` | `ACEITAR gravou o sim sem o alvo da tentativa: alvo=undefined, e a tentativa era a 203.0.113.9:8383` |
| R8 | o sim sem conjunto: `{ alvo }` | `camada-mods.js` | `ACEITAR gravou o sim de outra lista: conjunto=undefined, e o anúncio trouxe c0ffee1234567890` |
| R9 | **a volta que não tenta**: a linha `await conectar(alvo, null, credencial)` comentada | `camada-mods.js` | `ACEITAR gravou o sim e não tentou entrar de novo: connect não foi chamado (a sequência foi: mods_instalados → aceitar_mods)` |
| R10 | a volta sem a credencial: `conectar(alvo, null)` | `camada-mods.js` | `a volta que aceita saiu sem a credencial da tentativa (joinSecret=null): num servidor fechado por convite isso é CREDENCIAL RECUSADA sobre um convite que a recusa dos MODs não gastou` |
| R11 | o diálogo que não fecha: `fecharAceiteDeMods()` antes da volta comentada | `camada-mods.js` | `o diálogo de aceite continuou aberto depois de ACEITAR (diálogo=VISIVEL)` |
| R12 | **a volta antes do sim**: o `await conectar(...)` movido para antes do `try` do `aceitar_mods` | `camada-mods.js` | `a volta tentou entrar antes de o sim ser gravado, e sem o aceite em disco ela bate de novo no ponto que recusou a tentativa anterior (a sequência foi: mods_instalados → conferir_esta_versao → connect)` |

E o roteiro, sem reversão nenhuma, sai com `carga limpa` e `exit=0`.

## 5. O cabeçalho

O cabeçalho dizia que o roteiro prova «que ACEITAR grava o sim com a identidade
certa e tenta entrar de novo», e não provava. Agora ele diz que reprova, conta
a causa medida e o histórico das duas quebras em poucas linhas, e aponta este
relatório para as reversões.

O parágrafo histórico («antes desta tela, o que a pessoa lia era uma frase
terminando em “a tela para ler e aceitar esta lista ainda não existe neste app”»)
ficou, porque é história, e ganhou o que vale hoje: a recusa da tentativa mais
nova abre o diálogo e é isso que o roteiro exige; a frase continua em
`frases.js` para o ramo que um dia chegar a ela, já terminando em «entre de
novo neste servidor» (lote W2); a recusa de uma tentativa que outra substituiu
vai ao `seele.log`.

E ele diz o que **não** cobre: a obtenção de um pacote que falta
(`instalar_mod_do_catalogo`) e a desistência no meio dela, que a
`ajustes-v013.cjs` prova; o aceite que rejeita por dentro é de
`aceite-de-mods-que-falha.js`. Neste roteiro os pacotes do anúncio já estão na
máquina.

## 6. `catalogo-de-mods.js` entra no lote

O `grep` por aceite e MODs (`mods`, `MOD`, `aceit`, `ModsNaoAceitos`,
`mods-aceite`, `aceitar_mods`) acha só dois roteiros em `tools/roteiros/`:
`aceite-de-mods.js` e `catalogo-de-mods.js`. O resto do que cita o aceite é
Rust, `ui/`, a bancada e documentos.

O catálogo tinha o mesmo defeito: o cabeçalho prometia três coisas (BUSCAR
desenha o que o catálogo devolveu, INSTALAR pede aquele MOD naquela versão, e a
assinatura que não confere vira frase, e não lista vazia), e o único `exigir`
era a seção abrir. Medido antes, **os valores estavam certos** (não era a mesma
quebra: era o mesmo risco). Agora são dez `exigir`, e doze reversões:

| # | exigência | reversão | a frase que o roteiro escreve |
|---|---|---|---|
| K0 | a seção MODS abriu (já existia) | `$("secao-mods").hidden = true` (`tela-server.js`) | `a seção MODS não abriu` |
| K1 | abrir a seção não vai à rede (ADR 0026) | `desenharApresentacoes()` chama `catalogo_de_mods` | `abrir a seção MODS foi à rede buscar o catálogo sem ninguém apertar BUSCAR (ADR 0026)` |
| K2a | uma linha por MOD do catálogo | `.slice(0, 0)` na lista | `BUSCAR não desenhou o que o catálogo devolveu: 0 linhas desenhadas, e o catálogo trouxe 1` |
| K2b | a linha é a do MOD (título e id) | o título trocado por «MOD» | `a linha desenhada não é a do MOD do catálogo («Salas de RPG», alguem/rpg): «MODalguem/rpg…»` |
| K2c | o estado diz «1 no catálogo.» | `mods.length + 1` | `o estado da busca diz «2 no catálogo.», e o catálogo trouxe 1` |
| K3a | o selo é o da versão exibida, e não o do MOD | `NIVEIS[mod.nivel]` no lugar de `NIVEIS[ultima.nivel]` | `o selo desenhado é «versão 1.1.0 · OFICIAL», e devia ser «versão 1.1.0 · VERIFICADO»: a versão exibida é a 1.1.0, verificada, e o MOD é «oficial»` |
| K3b | a versão exibida é a última, a 1.1.0 | `mod.versoes[0]` | `o selo desenhado é «versão 1.0.0 · PUBLICADO COM NOTAS», e devia ser «versão 1.1.0 · VERIFICADO»…` |
| K4a | INSTALAR pede a versão exibida | `mod.versoes[0].versao` | `INSTALAR pediu «alguem/rpg 1.0.0», e a linha desenhada era a alguem/rpg 1.1.0` |
| K4b | INSTALAR pede o `id`, e não o título | `mod.titulo` | `INSTALAR pediu «Salas de RPG 1.1.0», e a linha desenhada era a alguem/rpg 1.1.0` |
| K4c | INSTALAR chama `instalar_mod_do_catalogo` | a chamada trocada por `Promise.resolve()` | `INSTALAR não pediu nada: instalar_mod_do_catalogo nunca foi chamado (a sequência foi: snapshot → volumes_locais → snapshot → estado_do_servidor)` |
| K5 | a recusa de assinatura vira a frase que manda parar | o `catch` escreve «o catálogo está no ar e ainda não lista nenhum MOD.» | `a assinatura do catálogo que não confere não virou a frase que manda parar: o estado diz «o catálogo está no ar e ainda não lista nenhum MOD.»` |
| K6 | a recusa é marcada como erro (`mods-recusado`) | a marca tirada do `catch` | `a recusa de assinatura foi escrita como um estado qualquer, e não marcada como erro (mods-recusado)` |

Todas com `exit=1` e `cmp` igual depois de restaurar.

**Uma mudança no catálogo do roteiro, que o selo exige.** O MOD tinha `nivel:
"verificado"` no topo e a 1.1.0 também era `"verificado"`: uma linha que lesse o
selo do MOD no lugar do da versão desenharia o mesmo texto e passaria. O topo
passou a `"oficial"`. Sem isso K3a não reprovava; é o tipo de exigência que
existe e não funciona.

## 7. A rejeição de `aceitarOsMods` (produto)

### A medida: qual `invoke` rejeita de verdade

Nenhum rejeita para fora. Medido com um roteiro de rascunho (apagado), nove
cenas, cada uma com um `invoke` de dentro de `aceitarOsMods` simulado:

| cena | o que a tela faz |
|---|---|
| tudo certo | `mods_instalados → aceitar_mods → conferir_esta_versao → connect`, diálogo fecha |
| `mods_instalados` recusa (`Err`) | frase «não consegui ler o que está instalado aqui…», ACEITAR aceso |
| a obtenção (`instalar_mod_do_catalogo`) recusa | frase «não consegui obter seele/placar 1.4.0…», ACEITAR aceso |
| a segunda `mods_instalados` (a conferência) recusa | frase «não consegui conferir o que chegou…», ACEITAR aceso |
| `aceitar_mods` recusa (`Err`) | a frase da recusa, ACEITAR aceso, não entra |
| a volta (`connect`) recusa | o `conectar` escreve a linha vermelha da entrada («CREDENCIAL RECUSADA») |
| **`mods_instalados` devolve `null`, `{}` ou `[null]`** | **nada: o diálogo fica, o ACEITAR fica apagado, sem frase** |

Os cinco `invoke` do aceite estão dentro de um `try` que escreve uma frase, e a
volta é do `conectar`. O que escapa é a **resposta de forma errada**, que
estoura em `.some`/`.filter` fora de qualquer `try`. O produto não a manda
hoje: `mods_instalados` devolve `Vec`, e o anúncio (`mods`, `conjunto`) é
decodificado no Rust antes de chegar à casca. O defeito é real e hoje
inalcançável; o conserto é local (uma função e uma linha), e não pediu parar.

### O conserto

`camada-mods.js`: o clique em ACEITAR guarda a pergunta a que responde e passa
a falha de `aceitarOsMods` a `aceiteFalhou(pergunta, falha)`, no lugar do
`console.warn`.

- **Sempre** vai ao `seele.log`, por `registrarNoAnfitriao("aceite-de-mods",
  …, "aviso")`, com o alvo e o motivo. Nunca a credencial: `aceiteFalhou` não a
  nomeia em linha alguma, e o guarda da credencial confere.
- **Se a pergunta que falhou ainda é a da tela**, a frase vai a `#mods-erro`
  («não consegui concluir o aceite dos MODs de <alvo>: <o motivo>. Tente de novo,
  ou recuse a entrada.», a mesma forma da obtenção) e o ACEITAR volta.
- **Se a pergunta largou** (a pessoa desistiu, ou há uma pergunta nova), a tela
  não fala: o erro de uma pergunta que ela deixou não aparece na seguinte. É a
  regra da recusa da tentativa substituída (W2). Fica só o registro.

O guarda `a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`
reprovou na primeira rodada, como devia: `if (!pergunta || aceitePendente !==
pergunta) return;` é uma linha nova que toca `aceitePendente`. Conferi que ela
só compara identidade e não leva a credencial a lugar nenhum, e a acrescentei à
lista dele, com o comentário. O `aceitarOsMods` ficou intacto (o guarda
`o_aceite_obtem_o_conjunto_antes_de_entrar` lê o corpo dele).

### A prova: `tools/roteiros/aceite-de-mods-que-falha.js`

Um roteiro novo, com 11 `exigir`, que simula a única resposta que rejeita
(`mods_instalados` → `null`), em duas cenas.

1. **A pergunta que falhou fala.** A frase no diálogo com o alvo e o «Tente de
   novo», o ACEITAR aceso e o diálogo aberto, um registro `aceite-de-mods` de
   nível `aviso` com o alvo e o `TypeError`, **sem o convite**; e o mesmo ACEITAR,
   com a resposta certa, grava o sim e entra levando o convite que a falha não
   comeu, com o diálogo e a frase fechados.
2. **A pergunta que largou não fala.** O aceite da pergunta 1 espera uma leitura
   pendente; a pessoa recusa; vem a pergunta 2; a leitura chega com a resposta
   errada. A pergunta 2 não mostra o erro da 1, e o registro foi escrito.

| # | a reversão | a frase que o roteiro escreve |
|---|---|---|
| F1 | **o `console.warn` de volta** no `.catch` do clique | `ACEITAR rejeitou por dentro e a tela não disse o que falhou, com o alvo e o caminho de volta (mods-erro=escondida, a sequência foi: mods_instalados)` |
| F2 | o ACEITAR não volta (a linha `disabled = false` tirada) | `ACEITAR rejeitou por dentro e o botão ACEITAR ficou apagado: a pessoa não tem como tentar de novo` |
| F3 | o motivo não vai ao `seele.log` | `ACEITAR rejeitou por dentro e o motivo não foi ao seele.log com o alvo, o erro e o nível aviso (registros de «aceite-de-mods»: [])` |
| F4 | o registro leva a credencial da tentativa | `o registro do aceite que falhou levou o convite da tentativa, que só pode ir ao connect` |
| F5 | a frase sem o alvo | `ACEITAR rejeitou por dentro e a tela não disse o que falhou, com o alvo e o caminho de volta (mods-erro=VISIVEL, …)` |
| F6 | a falha come a credencial (`pergunta.credencial = null`) | `a falha comeu a credencial da tentativa: a volta saiu com joinSecret=null` |
| F7 | a pergunta que largou também fala (a conferência de identidade tirada) | `o erro de uma pergunta que a pessoa largou apareceu na pergunta nova: «não consegui concluir o aceite dos MODs de 203.0.113.9:8383: ALGO FALHOU…»` |
| F8 | a pergunta que largou não vai ao `seele.log` (o `return` sobe para antes do registro) | `o aceite de uma pergunta que largou rejeitou e não foi ao seele.log (0 registros em vez de 1)` |
| F9 | a frase da falha sobrevive à segunda tentativa (`erro.hidden = true` tirado de `aceitarOsMods`) | `a segunda tentativa entrou, e o diálogo ou a frase da falha continuou na tela (diálogo=escondida, mods-erro=VISIVEL)` |
| F10 | a falha deixa a pergunta sem dono (`aceitePendente = null`) | `depois da falha, o mesmo ACEITAR com a resposta certa não gravou o sim e entrou (a sequência foi: nenhuma)` |

Duas exigências são precondição da cena («a recusa por MODs não abriu o
diálogo» na cena 1, «a segunda recusa… não abriu a pergunta nova» na cena 2) e
não têm reversão própria.

## 8. Os outros roteiros, fora da área

Fora da área, só lendo: 20 dos outros 28 roteiros de `tools/roteiros/` não têm
nenhum `exigir`, e ficam como estão:

`barra-mac`, `avatar-na-chamada`, `atualizacao`, `dispositivos`,
`clique-nos-glifos`, `icone-na-trilha`, `perfil-entrada`, `compartilhar`,
`renomear-canal`, `icone-do-server`, `medidas`, `perfil-salva`,
`tela-de-outra-versao`, `palco`, `perfil-sessao`, `perfil-salva-sessao`,
`rodape-do-operador`, `versao-do-servidor`, `trocar-de-janela`,
`botoes-da-entrada`.

Alguns são medidas de propósito (`medidas`, `palco`); não li cada um para
separar os que relatam o que dizem provar.

## 9. O que rodou

- Os três roteiros: `carga limpa`, exit 0; as 15 + 12 + 10 reversões, exit 1
  com a frase certa e `cmp` igual depois de restaurar; as duas cascas antigas,
  na tabela do §1. Os outros roteiros com `exigir` não foram rodados de novo:
  o aparelho não mudou.
- `cargo test -p seele-app --test frontend`: 287 passed (depois da linha nova
  no guarda da credencial).
- `cargo test -p seele-app --bin seele-app`: 312 passed.
- `cargo xtask check-runtime`: exit 0, 12 bancadas.
- `cargo fmt --all -- --check`: limpo.
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpo.
- `ajustes-v013.cjs`, `diagnostico-de-mods.cjs` e
  `contribuicoes-e-camadas.cjs` (`PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`):
  exit 0 nas três, **sobre `5bc3c84e`** (o commit do produto). A `ajustes-v013.cjs`
  tem a cena que procura a credencial em todo comando que não seja o `connect`,
  e continua verde com o registro novo.
- **Falta rodar, por causa da pausa da orquestradora (CPU e quadros):** as três
  bancadas de novo sobre a ponta (`9a53cc13`). Desde `5bc3c84e` o produto não
  mudou — só `aceite-de-mods.js` (uma exigência e uma linha de cabeçalho) e este
  relatório —, então não se espera diferença; mas não foi medido. Tudo o mais
  deste item foi rodado sobre `9a53cc13`: os três roteiros, `frontend` (287),
  `--bin` (312), `check-runtime`, fmt e clippy.
- Não rodados: `cargo test --workspace`, conformidade.

## 10. Preocupações

1. **Nenhum roteiro roda no Actions** (*corrigido na rodada 1: eles rodam na bateria da integração, a 12ª etapa, que não é o CI; ver o §11, M3*). `grep -rn "roteiros\|carga-da-casca"` em
   `.github/` e em `xtask/` não acha nada: a única menção é a tabela do
   `README.md`. Tudo o que este lote fez reprovar só reprova quando alguém roda
   `python3 tools/carga-da-casca.py --roteiro …` à mão — foi assim que o
   `aceite-de-mods.js` ficou três semanas calado, e é por isso que a
   `ajustes-v013.cjs`, que roda no job `bancadas`, nunca ficou sem prova. A
   exigência do `console.warn` (F1) mora num roteiro, como pediu a decisão;
   uma cena na `ajustes-v013.cjs` a faria valer em CI. Não fiz: é outro
   escopo, e a decisão é sua.
2. **O gêmeo do `.catch`, 7 linhas acima** (*tratado na rodada 1: §11, I2*). `levarParaOAceiteDeMods` faz
   `abrirAceiteDeMods(…).catch(console.warn("abrir aceite"))`, e retorna
   `true`: se abrir a pergunta rejeitar, não há diálogo **nem** linha vermelha,
   e o CONECTAR volta aceso sem uma palavra. Mesma forma, mesma inalcançabilidade
   hoje (o anúncio vem decodificado do Rust; o `invoke` de dentro está num
   `try`). O pedido nomeou só o `:415`, e não mexi.
3. **O que `aceite-de-mods.js` não cobre** (dito no cabeçalho): a obtenção de
   pacote que falta e a desistência no meio dela, que a `ajustes-v013.cjs` cobre.
4. **O catálogo do roteiro mudou de dado** (§6, o `nivel` do topo): quem
   comparar a saída antiga com a nova do `catalogo-de-mods.js` vê o selo do MOD
   diferente. É de propósito.

## 11. Rodada 1: a revisão (escrita sem rodar; rodada no §13)

**Estado.** Escrito, commitado e **não rodado**: a pausa da orquestradora (CPU
e quadros) proíbe `cargo`, roteiros, bancadas e `check-runtime`. Nem um
`node --check` foi feito. Tudo abaixo é leitura; o que é afirmação sobre
comportamento está marcado **a medir**, e não vale como achado até o aviso. A
revisão do W3 (`.superpowers/sdd/lotes/lote-w3-review-report.md`) deu Crítico 0,
Importante 2, Menor 5.

### I1 — a lista do diálogo era conferida pela metade

`aceite-de-mods.js` agora confere, **por item e na ordem do anúncio**, o `id`, a
`versão` e o hash (como antes), mais:

- **o repositório** (`.mods-repo`, o `span` que não é o rótulo);
- **o alcance** (`reach`), etiqueta por etiqueta e na ordem, que o ADR 0045 manda
  a pessoa ler antes de aceitar;
- **a linha «roda na máquina de quem hospeda» no item certo**: `.mods-servidor`
  dentro do item, uma no que tem `no_servidor` e nenhuma nos outros. A exigência
  antiga, que só contava as linhas da lista inteira, saiu: a nova a subsume.

Reversões **a rodar** (cada uma tem de dar `REPROVADO` com a frase do item):

| # | a reversão | a frase esperada |
|---|---|---|
| R5 (muda de frase) | `if (false && mod.no_servidor)` | `o item 2 (ponte/retransmissor) não traz a linha «roda na máquina de quem hospeda», e o anúncio marca no_servidor nele` |
| R13 | `if (!mod.no_servidor)` — a linha no MOD errado, com a mesma contagem | `o item 1 (seele/placar) traz a linha «roda na máquina de quem hospeda», e o anúncio não marca no_servidor nele` |
| R14 | `Array.isArray(mod.reach) && mod.reach.length > 0` → `false` | `o item 1 (seele/placar) mostra o alcance [nada declarado], e o anúncio trouxe [dom · mensagens do servidor]` |
| R15 | `mod.repo \|\| "não declarado"` → `"não declarado"` | `o item 1 (seele/placar) mostra o repositório «não declarado», e o anúncio trouxe «https://example.invalid/placar»` |

### I2 — o gêmeo do `.catch` entra

**A medida (leitura).** `levarParaOAceiteDeMods` devolvia `true` e deixava
`abrirAceiteDeMods(…).catch(console.warn)`. `abrirAceiteDeMods` grava
`aceitePendente` — com a credencial — **antes** do primeiro `await`, e abre o
diálogo quase no fim: rejeitando no meio, a credencial ficava na memória com uma
pergunta invisível, que nenhuma porta fecha, e a pessoa via o CONECTAR aceso e
nenhuma palavra. O `true` é o que impedia a linha vermelha de falar.

**Por que não devolvo `false`, como a revisão propôs.** A abertura é assíncrona:
quando ela rejeita, `levarParaOAceiteDeMods` já devolveu `true` há tempo. Devolver
`false` pediria `await` na linha de `conectar` que o guarda da credencial e os
guardas da tentativa substituída leem, e seguraria o CONECTAR apagado enquanto a
pergunta abre. Fiz o equivalente sem tocar `conectar` nem `tela-boot.js`:
`perguntaNaoAbriu` escreve a linha vermelha da entrada ela mesma, com a mesma
conferência.

**O conserto (`camada-mods.js`).**
- `levarParaOAceiteDeMods` guarda `desta = tentativas` (a tentativa mais nova, a
  única que `conectar` deixa chegar ali: o ramo `esta !== tentativas` vem antes)
  e a referência da pergunta que a abertura gravou, e passa a falha a
  `perguntaNaoAbriu`.
- `perguntaNaoAbriu`: (1) vai ao `seele.log` com o alvo e o motivo, sem a
  credencial; (2) **fecha a pergunta, e só ela** (`aceitePendente === pergunta`),
  o que esquece a credencial — se outra a substituiu ou a pessoa a fechou, não há
  o que fechar, e fechar mataria a pergunta nova; (3) **só se a tentativa ainda é
  a mais nova** (`desta === tentativas`, a regra do W2), escreve `#boot-erro` com
  `fraseDeErro(motivo)` — a frase de sempre da recusa por MODs, que lista a lista
  e manda «entrar de novo neste servidor», o ramo que o W2 deixou «para o caminho
  que um dia chegar»; (4) se a frase da recusa também não se monta (a mesma lista
  malformada), diz a falha de abrir, e não cala.
- `motivoEmTexto` tira de `aceiteFalhou` o texto da falha para o registro, e
  `perguntaNaoAbriu` o reusa.

**Guarda.** A linha de `abrirAceiteDeMods` que o guarda da credencial lia mudou
(agora `const abrindo = abrirAceiteDeMods(…)`), e a lista dele trocou uma linha
por outra, mais a de `if (aceitePendente === pergunta) fecharAceiteDeMods();`.
Conferi que nenhuma delas leva a credencial a lugar nenhum. `tela-boot.js` e as
linhas de `conectar` ficaram intactas.

**A prova: `tools/roteiros/aceite-de-mods-que-nao-abre.js`**, um roteiro novo
(a conta, sem rodar: as cenas atuais do `aceite-de-mods-que-falha.js` somam cerca
de 2,3 s virtuais do orçamento de 5 s do aparelho, e as quatro novas, cerca de
1,9 s; juntas sobrariam menos de 1 s de folga, sem medida. Separado, este fica
em cerca de 2,7 s). A
lista malformada é um `hash` que não é texto (`linhaDeMod` estoura nele, e a
frase da recusa não toca o hash). Quatro cenas e catorze `exigir`, duas delas
precondição de cena (sem reversão própria):

1. **a pergunta que não abriu fala**: o diálogo fechado, a linha vermelha com a
   recusa («entre de novo»), nenhuma pergunta pendente (`aceitePendente`
   `null`, lido do próprio escopo da página), um registro `aceite-de-mods` de
   nível `aviso` com o alvo e o `TypeError`, e o convite fora de todo registro;
2. **a tentativa que outra substituiu não fala**: a abertura espera uma leitura
   pendente, outra tentativa entra, a leitura chega malformada — a linha
   vermelha continua fechada, a pergunta foi esquecida e o registro foi escrito;
3. **a falha da pergunta antiga não fecha a nova**: a outra tentativa abre a
   própria pergunta, e a falha da antiga a deixa aberta e sem erro;
4. **a frase da recusa também não se monta** (`mods` nem é lista): a linha
   vermelha diz o `TypeError`.

Reversões **a rodar**: G1 o `console.warn` de volta · G2 sem fechar a pergunta
que não abriu · G3 sem o registro · G4 sem a conferência `desta !== tentativas` ·
G5 sem a frase de reserva · G6 `fecharAceiteDeMods()` incondicional.
**E o vermelho antes**: o roteiro novo contra o `ui/` de `9a53cc13`
(`--casca` com o `git archive` dele), que deve reprovar na cena 1.

### M1 — o motivo na frase, e em linhas

`aceiteFalhou` escreve a frase em três partes — «não consegui concluir o aceite
dos MODs de <alvo>.», o motivo (`fraseDeErro(falha)`) e «Tente de novo, ou
recuse a entrada.» —, e `camada-mods.css` ganha `#mods-erro { white-space:
pre-line; }`. (A frase já levava o motivo; o que faltava era **exigi-lo**.)
`aceite-de-mods-que-falha.js` exige que a frase traga o `TypeError` e que o
`white-space` calculado seja `pre-line`. Reversões **a rodar**: F11 sem o motivo
na frase · F12 sem o `pre-line`.

### M2 — o guarda lia só o `aceitarOsMods`

A parte 1 do guarda da credencial passa a ler as linhas que nomeiam `pergunta`
nas **três** funções que a recebem: `aceitarOsMods`, `aceiteFalhou` e
`perguntaNaoAbriu`. Uma linha nova que nomeie `pergunta` ali (um
`JSON.stringify(pergunta)` no registro) reprova até alguém conferi-la. Entrou na
lista só `const alvo = pergunta?.alvo ?? "";`. **A sonda a rodar**:
`JSON.stringify(pergunta)` no registro de `aceiteFalhou` e de `perguntaNaoAbriu`
contra `cargo test -p seele-app --test frontend` — esperado: reprovar com «a
credencial da tentativa ganhou um caminho novo». Se passar, o guarda ainda tem
buraco e o conserto dele é a próxima rodada.

### M3 — os roteiros fora do Actions (lote RT, não deste)

Correção aceita: os roteiros rodam na bateria da integração (a 12ª etapa de
`integracao.md`), e a preocupação 1 do §10 estava incompleta. O que fazê-los
valer a cada PR vai ao lote RT, que vem depois.

### M4 — o servidor da volta

`aceite-de-mods.js` exige `volta.server === ALVO` (era um `relatar` solto).
Reversão **a rodar**: R16, a volta a outro endereço.

### M5 — o alcance da versão exibida, no catálogo

O fixture do catálogo dá à 1.1.0 um alcance próprio (`mensagens do servidor`; a
1.0.0 declara «falar com um serviço de fora»), e `catalogo-de-mods.js` exige que
as etiquetas desenhadas sejam as da versão exibida. Com `alcanca: []` na 1.1.0 a
exigência passaria também sem etiqueta nenhuma. Reversões **a rodar**: K7, o
alcance da 1.0.0 · K8, nenhum alcance.

### Uma correção minha nos roteiros

O aparelho lê o relato até a primeira aspa dupla (`SEELE-RELATO ([^"]*)`), e uma
frase de reprovação com `"` dentro sai cortada ali. Tirei os `JSON.stringify` das
frases dos três roteiros (alcance, registros, `joinSecret`) e escrevi os dois
roteiros novos sem eles. Hoje o que se perdia era só o resto de uma frase já
reprovada.

### Observação (tratada na rodada 2: §12)

`abrirAceiteDeMods` não confere, depois do `await` do aceite anterior, que a
pergunta ainda é a dela. Uma abertura **antiga** que termine depois de outra
nova repovoa a lista e reabre o diálogo por cima, com `aceitePendente` já sendo
o da nova: a pessoa leria a lista de uma e aceitaria o conjunto da outra. É
anterior a este lote, exige duas recusas por MODs dentro da latência de um
`invoke` local, e eu a vi lendo, não medindo. Fica para decisão.

### O que falta rodar (depois do aviso)

Nesta ordem: (1) o vermelho do roteiro novo contra o `ui/` de `9a53cc13`;
(2) as reversões novas — R5, R13 a R16, F11, F12, G1 a G6, K7, K8 — e a sonda
do M2; (3) as 37 anteriores, de novo, porque os três roteiros mudaram (o script
`reverter.sh` do scratchpad as tem todas); (4) os **quatro** roteiros, limpos;
(5) `--test frontend` (a lista do guarda mudou duas vezes), `--bin seele-app`,
`check-runtime`, fmt e clippy do app; (6) `ajustes-v013`, `diagnostico-de-mods`
e `contribuicoes-e-camadas`. Tudo isto está **por rodar**: o relatório não diz
«verde» para nada desta rodada.

## 12. Rodada 2: a corrida de `abrirAceiteDeMods` (escrita sem rodar; rodada no §13)

**Estado.** Escrito e commitado, **não rodado**: a pausa continua. A corrida
abaixo é como eu a li. **Se a cena nova não ficar vermelha sem o conserto, a
corrida não existe como li, e o conserto sai.**

### A corrida, como eu a vi

`abrirAceiteDeMods(alvo, mods, conjunto, credencial)` faz, em ordem:

1. grava a pergunta em `aceitePendente` (e esquece a credencial da antiga);
2. `await invoke("aceite_de_mods", { alvo })`, a leitura do sim anterior;
3. **sem conferir nada**, escreve no diálogo — `mods-mudou`, `mods-contagem`,
   `mods-alvo`, a lista (`repovoar`), o texto do ACEITAR, `mods-erro` escondido,
   ACEITAR aceso — e o abre.

Só o passo 2 espera. Se uma segunda recusa por MODs chega dentro dele (duas
batidas, a servidores diferentes, dentro da latência de um `invoke` local), a
segunda grava a **sua** pergunta em `aceitePendente`, abre e desenha a lista
dela. A primeira, a velha, termina depois e repovoa tudo por cima: a lista e o
servidor no cabeçalho são os dela; `aceitePendente`, a da segunda. ACEITAR lê
`aceitePendente`: obtém o conjunto da segunda, grava o sim da segunda e entra na
segunda. A pessoa leu um pedido e aceitou o outro. O `await` da leitura não
trava a janela, e nada lá em cima impede duas aberturas ao mesmo tempo (a regra
do W2 só impede que a tentativa velha **comece** uma pergunta).

Duas coisas a mais, do mesmo defeito: a abertura velha também faz
`mods-erro.hidden = true` e `mods-aceitar.disabled = false`, e assim apaga a
mensagem de obtenção e acende o botão da pergunta nova.

### O conserto (`camada-mods.js`)

Depois do `await`, antes de qualquer escrita no diálogo: `if (aceitePendente
!== pergunta)`, com `const pergunta = aceitePendente;` logo depois da gravação.
Pela referência, e não por «há uma pergunta aberta»: a nova também é uma. A
abertura que perdeu a vez **não falha** (não houve erro, houve outra pergunta):
vai ao `seele.log` — `aceite-de-mods`, `aviso`, com o alvo dela — e volta. Vale
também para a pergunta que largou (fechada, ou esquecida por `perguntaNaoAbriu`)
antes de a leitura responder. As duas linhas novas que tocam `aceitePendente`
têm o mesmo texto de linhas que o guarda da credencial já conhece
(`const pergunta = aceitePendente;` e `if (aceitePendente !== pergunta) {`), e a
função não nomeia a credencial.

### A prova: `tools/roteiros/aceite-de-mods-em-corrida.js` (novo)

Duas recusas por MODs em sequência, a servidores diferentes, com a leitura do
aceite anterior da **primeira** pendente (uma `Promise` no duble) até a segunda
ter aberto. Depois de a leitura responder, o roteiro exige:

- **o que se lê é o que se aceita**: os itens desenhados são os de
  `aceitePendente.mods`;
- **a lista e o servidor desenhados são os da segunda** (`ponte/retransmissor`,
  `198.51.100.4:8383`), e não os da primeira;
- **a abertura velha foi ao `seele.log`**, uma linha, `aviso`, com o alvo dela;
- **ACEITAR grava o sim da segunda** (alvo e conjunto) e a volta vai a ela.

Duas precondições (a primeira não abriu ainda; a segunda abriu com a lista de B)
dizem que a cena não mede nada se faltarem. A conta do orçamento, sem rodar:
cerca de 1,6 s virtuais dos 5 s.

### As reversões, e o vermelho, a rodar

| # | a reversão | a frase esperada |
|---|---|---|
| **H1 = o vermelho** | sem a conferência: o bloco `if (aceitePendente !== pergunta) {…}` tirado (é o código de antes) | `o diálogo mostra [seele/placar] e o ACEITAR responde por [ponte/retransmissor]: a pessoa lê a lista de um pedido e aceita o conjunto do outro` |
| H2 | a conferência fica, o registro sai | `a abertura que perdeu a vez não foi ao seele.log com o alvo e o nível aviso (registros de «aceite-de-mods»: nenhum)` |

O vermelho tem duas formas, e vale rodar as duas: **H1** no worktree, e o
roteiro contra o `ui/` de `ee11e9f9` (`git archive ee11e9f9:apps/seele-app/ui`,
`--casca`), que não tem o conserto. As duas têm de reprovar na primeira
exigência da cena 3.

### O que falta rodar (depois do aviso)

**Primeiro o vermelho**: o roteiro novo sem o conserto, nas duas formas acima.
Se ele passar, a corrida não existe como li, o bloco sai de `camada-mods.js`, o
roteiro vira só a prova de que ela não acontece, e este parágrafo é reescrito.
Depois H2; o roteiro novo, limpo; e a verificação inteira da rodada 1 (§11) — os
**cinco** roteiros, as reversões todas, `--test frontend`, `--bin`,
`check-runtime`, fmt, clippy e as três bancadas. A `ajustes-v013.cjs` (cenas 5b
e 6) é a que mais pode sentir a conferência nova: ela abre perguntas em
sequência.

## 13. O que rodou nas rodadas 1 e 2 (VERDE)

A pausa acabou, e tudo o que os §11 e §12 deixaram por rodar foi rodado, na
ordem que eles pediam. Nada falhou por tempo, nem precisou de segunda rodada
(o lote W2 roda bancadas noutro worktree ao mesmo tempo; nenhuma das minhas
falhou).

### A corrida existe como li

O roteiro `aceite-de-mods-em-corrida.js`, **sem o conserto**, fica vermelho nas
duas formas, com a frase que o §12 previa:

| forma | resultado |
|---|---|
| H1: o bloco `if (aceitePendente !== pergunta) {…}` tirado | `REPROVADO: o diálogo mostra [seele/placar] e o ACEITAR responde por [ponte/retransmissor]: a pessoa lê a lista de um pedido e aceita o conjunto do outro` |
| contra o `ui/` de `ee11e9f9` (`git archive`, `--casca`), que não tem o bloco | a mesma frase, e o relato `itens [seele/placar], servidor 203.0.113.9:8383, o ACEITAR responde por [ponte/retransmissor]` |
| H2: a conferência fica, o registro sai | `REPROVADO: a abertura que perdeu a vez não foi ao seele.log com o alvo e o nível aviso (registros de «aceite-de-mods»: nenhum)` |
| com o conserto | `carga limpa`; `itens [ponte/retransmissor]`, `servidor 198.51.100.4:8383`, 1 registro, o sim com o alvo e o conjunto de B e a volta a B |

Ou seja: com o `await` da leitura pendente, a abertura velha repovoa a lista **e
o servidor** por cima da pergunta nova, e o ACEITAR responde pela nova. O
conserto fica. **Fechava só a repintura da abertura velha**; a janela de antes
do desenho da nova ficou aberta, e a revisão a achou (M6, §14).

### I2: o vermelho antes

`aceite-de-mods-que-nao-abre.js` contra o `ui/` de `9a53cc13` (sem a rodada 1):
`REPROVADO: a pergunta dos MODs não abriu e a entrada não diz nada: sem diálogo e
sem linha vermelha com a recusa (linha vermelha=escondida, o registro tem 0
linha(s))`, e o relato mostra **a pergunta pendente de `203.0.113.9:8383` com o
diálogo escondido**: a credencial na memória sem diálogo que a feche. É o defeito
que a revisão leu.

### As reversões

**53 reversões, todas vermelhas com a frase do §4, do §6, do §7 ou dos §11 e
§12**, e `cmp` igual depois de cada restauração (o `git status` limpo no fim):

| roteiro | casos | resultado |
|---|---|---|
| `aceite-de-mods.js` | R0, R0b, R1 a R16 (19) | as 19 vermelhas |
| `catalogo-de-mods.js` | K0 a K8 (14) | as 14 vermelhas |
| `aceite-de-mods-que-falha.js` | F1 a F12 (12) | as 12 vermelhas |
| `aceite-de-mods-que-nao-abre.js` | G1 a G6 (6) | **5 vermelhas; G6 não** |
| `aceite-de-mods-em-corrida.js` | H1, H2 (2) | as 2 vermelhas |

As 37 antigas (R0 a R12, K0 a K6, F1 a F10) saíram todas vermelhas de novo,
com as frases do relatório, salvo três que mudaram de texto por causa da
rodada 1: o R0 agora termina em `a sequência foi: mods_instalados →
registrar_da_janela` (a falha de `aceitarOsMods` passou a ir ao registro); o R5
diz `o item 2 (ponte/retransmissor) não traz a linha «roda na máquina de quem
hospeda», e o anúncio marca no_servidor nele`; e o K2b traz, no excerto da
linha, o alcance da 1.1.0. As 16 novas:

| # | a frase medida |
|---|---|
| R13 `if (!mod.no_servidor)` | `o item 1 (seele/placar) traz a linha «roda na máquina de quem hospeda», e o anúncio não marca no_servidor nele` |
| R14 o alcance que não aparece | `o item 1 (seele/placar) mostra o alcance [nada declarado], e o anúncio trouxe [dom · mensagens do servidor]` |
| R15 o repositório trocado | `o item 1 (seele/placar) mostra o repositório «não declarado», e o anúncio trouxe «https://example.invalid/placar»` |
| R16 a volta a outro servidor | `a volta foi a 198.51.100.4:8383, e a tentativa que o servidor recusou era a 203.0.113.9:8383: aceitar os MODs de um servidor e entrar em outro` |
| K7 o alcance da 1.0.0 | `o alcance desenhado é [falar com um serviço de fora], e o da versão exibida (1.1.0) é [mensagens do servidor]; o da 1.0.0 é [falar com um serviço de fora]` |
| K8 nenhum alcance | `o alcance desenhado é [], e o da versão exibida (1.1.0) é [mensagens do servidor]; …` |
| F11 sem o motivo | `a frase da falha não traz o motivo (o TypeError que a casca viu): …` |
| F12 sem o `pre-line` | `o motivo da falha sai emendado: #mods-erro tem white-space «normal», e a frase tem uma linha por parte` |
| G1 o `console.warn` de volta | `a pergunta dos MODs não abriu e a entrada não diz nada: sem diálogo e sem linha vermelha com a recusa (linha vermelha=escondida, o registro tem 0 linha(s))` |
| G2 sem fechar a pergunta | `a pergunta que não abriu ficou na memória com a credencial da tentativa, sem diálogo que a feche: uma pergunta de 203.0.113.9:8383, com o diálogo escondida` |
| G3 sem o registro | `a pergunta dos MODs não abriu e o motivo não foi ao seele.log com o alvo, o erro e o nível aviso (registros de «aceite-de-mods»: nenhum)` |
| G4 a tentativa substituída fala | `a pergunta de uma tentativa que outra substituiu não abriu e a falha apareceu na linha vermelha da entrada: «ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU.»` |
| G5 sem a frase de reserva | `a lista da recusa nem é lista, a frase dela não se montou e a entrada ficou calada, em vez de dizer a falha (linha vermelha=escondida: …)` |
| H1, H2 | acima |

**G6 não fica vermelho, e é um mutante equivalente, não um furo.** Com a
conferência de `abrirAceiteDeMods` (§12), a abertura que perdeu a vez **volta
antes de falhar**, então `perguntaNaoAbriu` nunca roda para uma pergunta que
outra já substituiu: `aceitePendente === pergunta` ali passou a valer sempre, e
tirar a condição (`fecharAceiteDeMods()` incondicional) não muda nada
observável. A defesa em camadas é real e foi medida:

- **G6c** (tira a conferência da abertura, **deixa** a de `perguntaNaoAbriu`):
  `carga limpa`. A segunda camada sozinha segura a cena 3.
- **G6b** (tira as duas): `REPROVADO: a pergunta antiga não abriu e a falha dela
  fechou a pergunta nova, que a pessoa estava lendo (diálogo=escondida, pergunta
  pendente: nenhuma)`.

A condição de `perguntaNaoAbriu` fica como defesa: se alguém puser outro `await`
em `abrirAceiteDeMods` depois da conferência, ela volta a decidir. A cena 3 do
`aceite-de-mods-que-nao-abre.js` prova o resultado (a pergunta nova fica
aberta), não qual das duas camadas o produz; para isso, G6b.

### M2: a sonda e o buraco

- **Antes** (o guarda de `9a53cc13` e o `ui/` dele, com `JSON.stringify(pergunta)`
  no registro de `aceiteFalhou`): `a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`
  **passa**. O buraco que a revisão leu existia.
- **Depois** (o guarda desta ponta): a mesma sonda no registro de `aceiteFalhou`
  **reprova** com `a credencial da tentativa ganhou um caminho novo em
  `camada-mods.js`: …${JSON.stringify(pergunta)}…`, e a sonda no registro de
  `perguntaNaoAbriu` também. O conserto do guarda (a leitura de `pergunta` nas
  três funções) vale.

### A verificação

- Os **cinco roteiros**, limpos, `exit=0`: `aceite-de-mods`, `catalogo-de-mods`,
  `aceite-de-mods-que-falha`, `aceite-de-mods-que-nao-abre`,
  `aceite-de-mods-em-corrida`. As contas de orçamento virtual (1,6 s e 2,7 s)
  não precisaram de ajuste: nenhum deles ficou sem chegar ao fim.
- `cargo test -p seele-app --test frontend`: **287 passed**.
- `cargo test -p seele-app --bin seele-app`: **312 passed**.
- `cargo xtask check-runtime`: exit 0, 12 bancadas.
- `cargo fmt --all -- --check`: limpo.
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpo.
- `ajustes-v013.cjs`: exit 0. As linhas `lote R: …` e `lote W: …` só são
  escritas no fim, depois de **todas** as asserções, e saíram as nove. As cenas
  que o coordenador pediu para olhar passaram: a **5b** (uma pergunta nova
  substitui a antiga durante a obtenção; é a linha `lote R: desistir da
  pergunta…`) e a **6** (o último alvo trocado no meio da batida; a linha `lote R:
  com o último alvo trocado no meio da batida, a volta e o sim vão ao servidor
  que perguntou`). A conferência nova da abertura não as atrapalhou: a 5b abre
  a pergunta nova quando a antiga já está aberta e na obtenção, e a conferência
  só vale **antes** de a abertura desenhar.
- `diagnostico-de-mods.cjs` e `contribuicoes-e-camadas.cjs`: exit 0.
- Não rodados: `cargo test --workspace`, conformidade.

### O que sobra

- **M3** vai ao lote RT, como a decisão disse.
- A **observação** do §11 (a corrida) deixou de ser observação, mas **não estava
  consertada inteira**: este parágrafo dizia «consertada e provada», e a revisão
  mostrou a metade que faltava (M6). Ela está no §14.
- Os dois `.catch` da mesma classe estão fechados; a preocupação 2 do §10 está
  tratada. A 1 (roteiros no Actions) é do lote RT.

## 14. Rodada 3: a janela da abertura nova (M6) e os três aliases (M2b) — VERDE

A revisão aprovou o W3 com três Menores (Crítico 0, Importante 0). O M3 (os
roteiros fora do Actions) é do lote RT. Os outros dois:

### M6: a corrida não estava fechada inteira

**O caso.** Um diálogo já está **aberto** com a pergunta Z, e chega outra recusa
por MODs. `abrirAceiteDeMods` troca `aceitePendente` na hora e só desenha depois
do `await invoke("aceite_de_mods")`. A conferência da rodada 2 fechava o desenho
da abertura **velha**; não fechava esta janela, a de antes do desenho da
**nova**: a lista de Z ficava na tela, com o ACEITAR aceso, e o ACEITAR
respondia pela nova. O mesmo dano de consentimento, durante a latência de uma
leitura local.

**O conserto** (o que o revisor validou numa cópia): `$("mods-aceitar").disabled
= true;` logo depois de `const pergunta = aceitePendente;` em
`abrirAceiteDeMods`. O desenho já fazia `disabled = false`: a pergunta nova o
acende quando desenha. Uma abertura que perdeu a vez não toca o botão, e quem
tomou o lugar dela o acende. Uma que falha (`perguntaNaoAbriu`) fecha o
diálogo, e a próxima abertura apaga e acende de novo.

**A prova: a cena 5 de `aceite-de-mods-em-corrida.js`**, a sonda do revisor
trazida para o roteiro e com uma exigência a mais. A pergunta de A está aberta;
a de B chega com a leitura pendente; na janela, o roteiro exige que o ACEITAR
esteja apagado **ou** que o diálogo mostre o que ele responde, aperta o ACEITAR
e exige que nada seja gravado, e depois de a leitura responder exige a lista de
B, o servidor de B e o ACEITAR aceso.

| passo | resultado |
|---|---|
| **o vermelho, sem o conserto** | `REPROVADO: na janela entre a pergunta nova e o desenho dela, o diálogo mostra 203.0.113.9:8383 [seele/placar] com o ACEITAR aceso, e o ACEITAR responde por 198.51.100.4:8383 [ponte/retransmissor]` — a frase da sonda do revisor |
| com o conserto | `na janela: … ACEITAR apagado`, `ACEITAR na janela: nada gravado`, `depois da leitura: o diálogo mostra 198.51.100.4:8383 [ponte/retransmissor], ACEITAR aceso`; `carga limpa` |
| **J1**, a reversão (a linha tirada) | a mesma frase do vermelho, `exit=1`, `cmp` igual depois de restaurar |
| a sonda original do revisor (`janela.js`) com o conserto | `botão=apagado`, `o sim gravado: undefined undefined`, `carga limpa` |

A exigência de «aperta o ACEITAR e nada é gravado» não tem reversão própria: a
primeira a subsume, porque um botão apagado não grava. Ela fica como a leitura
da sonda do revisor.

### M2b: o guarda da credencial era cego a três aliases

A rodada 1 estendeu a leitura de `pergunta` a `aceiteFalhou` e a
`perguntaNaoAbriu`, mas pôs o alias `const pergunta = aceitePendente;` em mais
três lugares que o guarda não lia: `abrirAceiteDeMods`,
`levarParaOAceiteDeMods` e o ouvinte do clique.

- **Antes** (o guarda da rodada 2): `JSON.stringify(pergunta)` num log de
  cada uma das três **passa**, 3 em 3 (`levar`, `abrir`, `clique`), todas com
  `cmp` igual depois de restaurar. O buraco que a revisão mediu existia.
- **O conserto:** a parte 1 do guarda lê as linhas que nomeiam `pergunta` nas
  **seis** funções (`aceitarOsMods`, `abrirAceiteDeMods`,
  `levarParaOAceiteDeMods`, `aceiteFalhou`, `perguntaNaoAbriu` e o ouvinte do
  clique, achado pelo início), e a lista ganha as duas linhas que passam a
  pergunta adiante (`abrindo.catch(… perguntaNaoAbriu(…, pergunta, …))` e
  `aceitarOsMods().catch(… aceiteFalhou(pergunta, …))`). As aliases em si já
  nomeiam `aceitePendente` e são conhecidas.
- **Depois:** as três sondas **reprovam** com `a credencial da tentativa ganhou
  um caminho novo em camada-mods.js` e a linha da sonda. `--test frontend` limpo
  (287).

### A verificação

- **Os cinco roteiros**, limpos, `exit=0`.
- `cargo test -p seele-app --test frontend`: **287 passed**.
- `cargo test -p seele-app --bin seele-app`: **312 passed**.
- `cargo xtask check-runtime`: exit 0.
- `cargo fmt --all -- --check`: limpo.
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`: limpo.
- `ajustes-v013.cjs`: exit 0, as dez linhas `lote …`. `diagnostico-de-mods.cjs`
  e `contribuicoes-e-camadas.cjs`: exit 0.
- Reversões novas desta rodada: **J1** e as **3 sondas do M2b**. As 53 do §13
  **não foram refeitas** nesta rodada (o pedido era os cinco roteiros limpos e
  a verificação, com pressa). A única mudança de comportamento é o
  `disabled = true` do M6, que o desenho da pergunta desfaz; pela leitura, nenhuma
  delas o observa, mas isso é leitura, e fica para a próxima integração se
  alguém quiser a prova.
- Não rodados: `cargo test --workspace`, conformidade.
