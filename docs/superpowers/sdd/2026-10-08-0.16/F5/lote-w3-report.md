# Lote W3 da F5: relatório

Base `b4dcf282` (a ponta do lote W2). Branch `0.16/lotes-w3`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a8ba192d9719be169`. Tudo
local: sem push, tag nem release. Sem conformidade. Nada no fio.

| commit | o que muda |
|---|---|
| `a5451bb2` | `tools/roteiros/aceite-de-mods.js` responde a `mods_instalados` e passa a exigir o que o cabeçalho dizia provar (14 `exigir`) |
| `beafc181` | as frases de reprovação dos itens, sem plural a errar |
| `c27d84cb` | o cabeçalho do roteiro diz onde estão as reversões |
| este | o relatório |

**O que não mudou.** Nada em `apps/seele-app/ui/`, em `crates/`, em
`tools/carga-da-casca.py`. A única alteração de código é o roteiro. Por isso os
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
lista. O `null` só existe no duble.

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
também sumiu do roteiro. São 1347 commits entre a `ea75ac5d` e esta base, e o
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

## 3. As exigências

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

As reprovações de E8 e E11 dizem a sequência de comandos que o roteiro viu, até
o `connect` da volta, e é o que teria dito a causa do `alvo=undefined` no dia em
que ela nasceu.

## 4. As reversões

Uma por exigência, e não só as três do pedido. Cada uma é uma linha editada no
produto (ou no duble, para R0 e R0b), o roteiro rodado, o arquivo restaurado e
conferido com `cmp` contra a cópia (`cmp` igual em todas), e o `git status`
limpo no fim. A edição de cada uma foi conferida: o script de reversão acusa se
a substituição não mudou nada. Todas saem com `exit=1` e `REPROVADO` com a
frase da exigência.

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
`ajustes-v013.cjs` prova. Neste roteiro os pacotes do anúncio já estão na
máquina.

## 6. Os outros roteiros da área

O `grep` por aceite e MODs (`mods`, `MOD`, `aceit`, `ModsNaoAceitos`,
`mods-aceite`, `aceitar_mods`) acha só dois roteiros em `tools/roteiros/`: este
e `catalogo-de-mods.js`. O resto do que cita o aceite é Rust, `ui/`, a
bancada e documentos.

**`catalogo-de-mods.js` só relata o que diz provar.** O cabeçalho promete três
coisas (BUSCAR desenha o que o catálogo devolveu, INSTALAR pede aquele MOD
naquela versão, e a assinatura que não confere vira frase, e não lista
vazia). Tem um único `exigir`: a seção MODS abriu. As três promessas são
`relatar`. Rodado hoje, **os valores estão certos** (não é a mesma quebra):

```
buscou ao abrir: false          linhas no catálogo: 1
selo da versão exibida: versão 1.1.0 · VERIFICADO
pediu instalação: id=alguem/rpg versao=1.1.0
recusa vira frase: O CATÁLOGO NÃO É O NOSSO.      marcada como erro: true
```

Não consertei. São seis exigências (`buscou ao abrir` falso, uma linha, o selo
da 1.1.0, o pedido com id e versão, a frase não vazia, a marca de erro), e
cada uma pede a sua reversão para valer: não é trivial, é um lote pequeno
igual a este. Fica a lista para quem o pegar.

Fora da área, só lendo: 20 dos outros 28 roteiros de `tools/roteiros/` não têm
nenhum `exigir` (`barra-mac`, `avatar-na-chamada`, `atualizacao`,
`dispositivos`, `clique-nos-glifos`, `icone-na-trilha`, `perfil-entrada`,
`compartilhar`, `renomear-canal`, `icone-do-server`, `medidas`, `perfil-salva`,
`tela-de-outra-versao`, `palco`, `perfil-sessao`, `perfil-salva-sessao`,
`rodape-do-operador`, `versao-do-servidor`, `trocar-de-janela`,
`botoes-da-entrada`). Alguns são medidas de propósito (`medidas`, `palco`); não
li cada um para separar os que relatam o que dizem provar.

## 7. O que rodou

- `python3 tools/carga-da-casca.py --roteiro tools/roteiros/aceite-de-mods.js`:
  `carga limpa`, exit 0; as 14 reversões, exit 1 com a frase certa; as duas
  cascas antigas, na tabela do §1. Os outros roteiros com `exigir` não foram
  rodados de novo: o aparelho não mudou.
- `cargo test -p seele-app --test frontend`: 287 passed. O lote não mexe em
  `ui/`; foi rodado para confirmar. As reversões tocaram `camada-mods.js` e
  `tela-boot.js` e as restauraram byte a byte (`cmp`).
- `cargo test -p seele-app --bin seele-app`: 312 passed.
- `cargo xtask check-runtime`: exit 0, 12 bancadas.
- `cargo fmt --all -- --check`: limpo.
- `ajustes-v013.cjs` (`PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`):
  exit 0, todas as cenas, incluindo as do lote R, W e W2 sobre o aceite.
- Não rodados: `cargo test --workspace`, conformidade.

## 8. Preocupações

1. **O `.catch` de `camada-mods.js:415` é o «o produto sabe e não conta» de
   uma rejeição que hoje não acontece.** `aceitarOsMods().catch(console.warn)`
   deixa o ACEITAR apagado e a tela sem frase nenhuma se `aceitarOsMods`
   rejeitar. É o que o roteiro mediu com o duble ruim. No produto, o único
   `await` fora de `try` que poderia rejeitar é o `.some` sobre uma resposta
   que não seja lista, e `mods_instalados` devolve `Vec`; por isso não é
   defeito alcançável. Não consertei (mexeria em `ui/`, e o pedido foi parar se
   fosse do produto): fica para decisão, e a frase do roteiro agora mostra a
   sequência que o `console.warn` esconderia.
2. **Existir não era funcionar, por três semanas.** A `ea75ac5d` e o lote W
   quebraram o roteiro sem que nada reprovasse; quem o notou foi a medida do
   W2, lendo as linhas à mão. A `ajustes-v013.cjs` cobre o mesmo caminho em
   profundidade e não quebrou, e é por isso que o produto nunca ficou sem
   prova; o roteiro é a cobertura leve que ficou muda.
3. **O que o roteiro não cobre** (dito no cabeçalho): a obtenção de pacote que
   falta e a desistência no meio dela, que a `ajustes-v013.cjs` cobre (junto
   com o sim que não fica gravado quando se desiste). A ordem «`aceitar_mods`
   antes do `connect`» aparece na sequência que a reprovação imprime, mas
   nenhuma exigência a cobra por si.
4. **`catalogo-de-mods.js`** (§6): a mesma forma de risco, com os valores
   certos hoje.
