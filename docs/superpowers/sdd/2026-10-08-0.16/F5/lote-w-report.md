# Lote W da F5: relatório

Base `9599bec5` (branch `0.16/lotes`, com o K4, o Z1 e o R integrados). Branch
`0.16/lotes-w`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-adc3534f0615b2550`. Tudo
local: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `645819a2` | 1 (I-3, item 46 da pendência 58) | `aceitarOsMods` obtém o conjunto antes de gravar o sim, grava só se a pergunta ainda é a aberta, e desfaz com `esquecer_aceite_de_mods` o sim de quem desistiu enquanto ele era gravado; cenas 5 e 5c na `ajustes-v013.cjs`; `o_aceite_obtem_o_conjunto_antes_de_entrar` cobra as duas ordens; o guarda do R ganha a linha nova |
| `55393d9a` | achado do I-4 | a recusa por `NicknameTaken` esquece `ultimoApelido`, e a tentativa seguinte lê o perfil; a cena do lote W na `ajustes-v013.cjs`; `the_shell_never_remembers_a_nickname_nobody_chose` cobra o esquecimento |
| `8f5d54d5` | 2 (I-4, item 48 da pendência 58) | a credencial de uma tentativa recusada pelo apelido fica em `recusaDoApelido` (`tela-boot.js`) até a tentativa seguinte, e só vai a uma do mesmo alvo; a cena do lote W cresce; o caso `apelido-tomado` no roteiro `tools/roteiros/apelido-recusado.js`; o guarda do R cresce para seis partes |
| `2329873a` | 3 | `GLIFOS` perde a `chave` e a `aparencia` de cima; `nenhuma_tabela_constante_da_ui_repete_uma_chave` em `tests/frontend.rs` |
| `6b41f811` | 4 (doc) | o doc de `girar_o_log` volta para cima dela no `main.rs`, sem uma palavra mudada |

**O que não mudou.** Nada em `crates/`, no fio, no servidor, nos arquivos da
F6 (`ui/camada-versoes.js` só é **lido** pelo guarda novo), em `api/` ou na
CSP. Nenhuma dependência nova, nada baixado. Em `main.rs`, só as 28 linhas de
doc do item 4: o arquivo sem as linhas `///` é idêntico ao da base, e as linhas
ordenadas também. Nenhum `.cjs` novo em `bancada/`: as cenas entraram na
`ajustes-v013.cjs`, que já está no job `bancadas` do `ci.yml`. Nenhuma promessa
nova de API: não há seção «Para a v6». Nada em `docs/pendencias.md` nem no
índice da 1.0.

As reversões foram feitas trocando o trecho exato (por um script no scratchpad
ou por `Edit`), rodando a prova, devolvendo o arquivo de uma cópia e conferindo
com `cmp`. Todas voltaram idênticas. Arquivos fora da lista do lote
(`tela-fim.js`, `frases.js`) só foram mexidos dentro de uma reversão, e voltaram
idênticos.

## Item 1 (I-3) — o sim dos MODs não fica gravado depois de desistir (`645819a2`)

### O defeito

`aceitarOsMods` gravava o sim (`invoke("aceitar_mods")`) **antes** de obter os
pacotes. Quem apertava RECUSAR, Escape ou clicava fora durante a obtenção não
entrava (o R fez a volta não sair), mas o sim já estava no disco: na entrada
seguinte nesse servidor o `Destino.aceito` batia, ninguém perguntava, e o MOD
rodava contra o «não» que a pessoa deu por último (ADR 0045).

### A escolha: mudar a ordem, e o `esquecer` só para a janela que a ordem não fecha

Escolhi **obter primeiro e gravar o sim só depois de conferir que a pergunta
que ele responde ainda é a aberta**. A ordem pôde mudar porque
`instalar_mod_do_catalogo` não lê o aceite: conferido no `main.rs`, ele baixa e
instala pelo catálogo, e só reaplica (`aplicar_mod`) um MOD que esta janela
**hospeda** e já exigia — o sim de quem entra não participa. Com a ordem nova,
quem desiste no meio da obtenção nunca chega a ter o sim gravado, e quem falha
na obtenção também não (antes, a falha deixava o sim gravado e o botão em
TENTAR DE NOVO).

A gravação também espera, e RECUSAR, Escape e clicar fora seguem à mão durante
ela: a ordem nova **não fecha** essa janela. Por isso, depois do `await` da
gravação, a mesma conferência roda de novo, e quem desistiu nesse meio tem o
sim desfeito por `esquecer_aceite_de_mods` — o verbo que já existia, o mesmo da
seção MODS de CONFIGURAÇÕES. Ou seja: a ordem é o conserto, e o `esquecer` é a
rede da janela que sobra.

Por que não só o `esquecer` ao desistir (a alternativa do brief): ele teria de
saber, em `fecharAceiteDeMods`, se havia uma gravação em curso ou feita, e uma
falha dele deixaria o sim gravado. Com a ordem nova, o caminho comum não grava
nada que precise ser desfeito.

### As provas

A `ajustes-v013.cjs` passa a guardar o sim por alvo, como o disco
(`testAceites`: o `aceitar_mods` grava quando termina, o
`esquecer_aceite_de_mods` apaga).

- **Cena 5** (desistir no meio da obtenção, que já existia no R): o sim da cena
  1 é apagado antes, e depois de desistir a cena confere que nenhum sim ficou.
- **Cena 5c** (nova): o MOD já está na máquina, o `aceitar_mods` leva 500 ms, e
  a pessoa recusa durante a gravação. Confere que o sim não ficou e que a janela
  não entrou.
- **Vermelho na base** (`9599bec5`): a cena 5 reprova com «a pessoa desistiu
  enquanto o MOD chegava, e o sim dos MODs ficou gravado: na próxima entrada
  neste servidor ninguém pergunta e o MOD roda, contra o «não» que ela deu por
  último (ADR 0045)». Numa cópia temporária da bancada sem essa asserção
  (apagada logo depois), a 5c reprova com «a pessoa desistiu enquanto o sim dos
  MODs era gravado, e ele ficou valendo…».
- **Verde** com o conserto.
- **Guarda de texto**: `o_aceite_obtem_o_conjunto_antes_de_entrar` passa a
  cobrar que o `invoke("aceitar_mods"` vem depois de `obterOConjunto` e de uma
  conferência `if (aceitePendente !== pergunta)`, e que depois dele há a mesma
  conferência seguida de `esquecer_aceite_de_mods`.
- **Reversão R1a** (a gravação de volta para antes da obtenção, com o resto
  intacto): a bancada reprova na cena 5, com a frase do vermelho; o guarda
  reprova com «`aceitarOsMods` grava o sim antes de obter o conjunto e de
  conferir que a pergunta continua aberta…».
- **Reversão R1b** (sem o `esquecer`; a conferência depois da gravação só
  retorna): a bancada reprova na cena 5c; o guarda reprova com «`aceitarOsMods`
  deixou de desfazer o sim de quem desistiu enquanto ele era gravado…».
- O guarda do R reprovou o conserto antes de crescer («a credencial da
  tentativa ganhou um caminho novo em `camada-mods.js`: if (aceitePendente !==
  pergunta) {»), e a lista dele ganhou essa linha: o corpo dela só chama o
  `esquecer`, que leva o alvo e mais nada.

## Achado do I-4 — o apelido recusado era lembrado (`55393d9a`)

Ao montar a cena do I-4, **medi antes de consertar** (uma bancada temporária,
apagada depois): o link colado com o apelido «Bia» tomado, a troca para «Bia2»
pelo perfil da entrada (o perfil gravou «Bia2») e a tentativa seguinte pelo
campo do diálogo de servidores. O resultado foi
`[["192.0.2.7:8384","Bia","convite"],["192.0.2.7:8384","Bia",null]]`: a
tentativa seguinte saiu **sem o convite** (o defeito do I-4) **e com o apelido
recusado**.

A causa: `conectar` lembra em `ultimoApelido` todo apelido não vazio, inclusive
o que veio do perfil (`apelido_local`), e com ele lembrado o perfil não é mais
lido naquela execução. A frase manda «escolha outro apelido e entre de novo», e
o lugar de escolher é o perfil: o nome novo fica gravado sem sair da máquina.
É o relato do `d084cfb4` («troquei o nick várias vezes e continua dando o
erro»), que aquele commit consertou só para o nome de recurso. Sem este
conserto, a «troca de apelido» que a cena do I-4 pede não tem efeito na tela, e
a tentativa seguinte, mesmo com o convite, receberia `NicknameTaken` de novo.

**O conserto**: a recusa por `NicknameTaken` esquece `ultimoApelido`. O nome
desta visita que o diálogo de conhecidos e a troca de servidor passam por
argumento continua indo quando eles o passam.

- **Vermelho** em `645819a2`: «a pessoa trocou o apelido no perfil e a
  tentativa seguinte mandou «Bia»: o apelido recusado ficou lembrado, o perfil
  não foi lido, e a frase que manda escolher outro apelido e entrar de novo não
  tem como ser cumprida».
- **Verde** com a linha.
- **Reversão** (a linha tirada): a bancada reprova com a mesma frase; o guarda
  `the_shell_never_remembers_a_nickname_nobody_chose`, que ganhou a regra,
  reprova com «a casca deixou de esquecer o apelido que o servidor recusou por
  ser de outra chave…».

Ficou num commit próprio, antes do I-4, por ser uma mudança de comportamento
que o brief não pediu. Ver «Preocupações».

## Item 2 (I-4) — a credencial sobrevive ao apelido tomado (`8f5d54d5`)

### O mecanismo, reaproveitado do R

O servidor recusa o apelido **antes** de gastar o convite («o convite ficou
intacto», em `seele-server/src/session.rs`; conferido na ordem: segredo,
portaria, apelido, MODs, gasto). A credencial da tentativa passa a ficar em
`recusaDoApelido` (`tela-boot.js`), `{ alvo, credencial }`, só na memória:

1. no `catch` de `conectar`, `NicknameTaken` com credencial guarda
   `{ alvo, credencial }`, com o `alvo` **desta chamada**, e não o `ultimoAlvo`
   (a razão do I3 da rodada 1 do R);
2. na primeira linha de `conectar` que sabe o alvo, antes de qualquer `await`,
   a credencial guardada é lida **só** se o alvo é o mesmo e a tentativa não
   trouxe credencial própria (`credencial ??= …`), e é esquecida **em toda
   tentativa**.

**Quando ela é esquecida.** Entrar, ser recusado de novo por outro motivo,
tentar outro servidor e hospedar aqui (quando o servidor sobe e `hospedar`
chama `conectar()`) passam todos pelo começo de `conectar`: é isso que a
esquece **ao entrar** e **ao trocar de alvo**. **Ao desistir**: a entrada não
tem pergunta aberta a fechar (o que fica é a linha vermelha); desistir aqui é
fazer outra coisa, e toda outra coisa que conecta passa por ali. Se a tentativa
seguinte para na pergunta dos MODs, a credencial passa à pergunta (o
mecanismo do R) e **morre com ela** ao recusar. **Ao fechar**: só memória da
página, e a parte 4 do guarda do R continua cobrando que nada segura o fechar.

**Por que não em `esquecerOEstadoDaSessao`**: pela mesma razão do R. A
credencial é anterior à sessão e morre na entrada; uma saída de sessão nunca
teria o que esquecer.

**A senha da portaria** segue o mesmo caminho por construção, como no R: o que
a tentativa mandou no `joinSecret` é o que fica.

### O guarda do R cresceu

`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta` passou de
cinco para seis partes:

- **parte 1**: segue também `recusaDoApelido`, e a lista ganhou quatro linhas
  de `tela-boot.js` (o `let`, a leitura presa ao alvo, o esquecimento e a
  guarda na recusa). Sem seguir o nome novo, um `console.warn` do objeto
  passaria (ver G1 abaixo);
- **parte 2**: nenhum outro script lê `aceitePendente` **nem
  `recusaDoApelido`**;
- **parte 3**: intacta — continuam só três chamadas de `conectar` com
  credencial; a nova credencial entra pelo **parâmetro**, dentro de
  `conectar`, e o RECONECTAR continua chamando `conectar()`. Uma reconexão
  depois de uma sessão que entrou não tem o que levar: entrar a esqueceu;
- **parte 6, nova**: `recusaDoApelido = null;` vem antes da primeira espera de
  `conectar`. A lista da parte 1 prende as linhas, e não a ordem.

### As provas

**A cena do lote W na `ajustes-v013.cjs`**
(`oApelidoTomadoNaoPerdeATentativa`). O servidor de mentira confere na ordem do
de verdade (o segredo, depois o apelido, depois os MODs) e só gasta o convite
na entrada; outro servidor recusa por credencial.

1. o link colado, com «Bia» tomado: recusa pelo apelido;
2. a troca de apelido pelo perfil da entrada (`#boot-perfil`, o campo, Enter);
3. a tentativa seguinte **digitando o endereço** (o link já não está em lugar
   nenhum: o R zera o campo): leva «Bia2» e o convite, entra na `#tela-auth`, e
   a credencial não está mais guardada;
4. nova recusa pelo apelido, a credencial guardada; uma tentativa a
   `192.0.2.18:8384` sai sem convite; a volta a `192.0.2.17:8384` também
   (trocar de alvo esqueceu);
5. nova recusa, troca para «Bia3», e a tentativa seguinte para na pergunta dos
   MODs com a credencial nela; RECUSAR E VOLTAR a esquece dos dois lugares, e a
   tentativa seguinte sai sem ela;
6. a credencial não está no `localStorage`, no `sessionStorage`, no DOM, nos
   campos, no console (embrulhado, como no R) nem em comando nenhum além do
   `connect` e do `analisar_convite`.

**O roteiro** `tools/roteiros/apelido-recusado.js` ganhou o caso
`apelido-tomado` no estilo dele (`relatar` e `exigir`): recusa pelo apelido
com um convite, o apelido novo gravado por `escolher_apelido_local`, a
tentativa seguinte ao mesmo alvo com o convite e o apelido novo, outra a outro
servidor sem convite, e a volta sem convite. **Como os roteiros rodam** (a 12ª
etapa da bateria, segundo `docs/superpowers/sdd/2026-10-08-0.16/integracao.md`):
`python3 tools/carga-da-casca.py --roteiro tools/roteiros/<roteiro>.js`, um por
roteiro, com o `chrome-headless-shell` do cache do Playwright. Sai com 1 em
`REPROVADO` ou estouro.

- **Vermelho** em `55393d9a`: a bancada reprova com «a tentativa seguinte ao
  mesmo servidor saiu sem o convite (joinSecret null): o servidor o guardou
  intacto e a frase diz que não é o convite, e a tela acaba dizendo CREDENCIAL
  RECUSADA sobre um convite válido»; o roteiro, com «REPROVADO: a tentativa
  seguinte ao mesmo servidor saiu sem o convite que o apelido tomado não gastou:
  CREDENCIAL RECUSADA sobre um convite válido» (a linha do apelido, ali, já dizia
  `apelido=rafa-2`, por causa do achado).
- **Verde** com o conserto: a bancada inteira e os 28 roteiros.
- **Reversões**, cada uma rodando a bancada, o roteiro e o guarda:
  - **R2a** (a recusa não guarda): bancada com a frase do vermelho; roteiro com
    a frase do vermelho; guarda com «a lista deste guarda tem uma linha que
    `tela-boot.js` não tem mais».
  - **R2b** (`conectar` não esquece no começo): bancada com «a credencial que o
    apelido tomado deixou continuou guardada depois de a pessoa entrar…»;
    roteiro com «trocar de servidor não esqueceu o convite que o apelido tomado
    deixou»; guarda pela lista.
  - **R2c** (leva a qualquer alvo): bancada com «uma tentativa a
    192.0.2.18:8384 levou o convite de 192.0.2.17:8384…»; roteiro com «uma
    tentativa a outro servidor levou o convite deste»; guarda com «a
    credencial da tentativa ganhou um caminho novo em `tela-boot.js`».
  - **R2d** (o esquecimento descido para depois da primeira espera): bancada e
    roteiro **verdes** (eles são sequenciais), e só a parte 6 do guarda
    reprova («`conectar` deixou de esquecer a credencial do apelido tomado
    antes da primeira espera…»). É para isso que ela existe.
  - **G1** (`console.warn("apelido tomado:", recusaDoApelido)` na recusa):
    bancada com «…apareceu fora do connect: o console»; guarda com «…ganhou um
    caminho novo em `tela-boot.js`». A linha não nomeia `credencial`: só é
    pega porque a parte 1 passou a seguir `recusaDoApelido`.
  - **G2** (`const deixada = recusaDoApelido?.credencial;` posto em
    `reconectar()`, `tela-fim.js`): a parte 2 reprova com «`tela-fim.js` lê
    `recusaDoApelido`…». `tela-fim.js` devolvido e conferido com `cmp`.

## Item 3 — as chaves repetidas de `GLIFOS` e o guarda (`2329873a`)

### O que o `git log -S` mostrou

- `git log -S "chave: [" -- apps/seele-app/ui/glifos.js`: `c312f87a`
  (16/08, «os oito glifos do comp v3, desenhados») criou a **de baixo**, com o
  comentário «⚿ — a seção de identidade»; `d9d5febd` (01/09, «o link de
  convite vira diálogo ao subir a rede, e ganha casa», a seção A PORTA) criou a
  **de cima**, com «A chave, da comp da 0.9.0… nomeia tanto a seção A PORTA
  quanto a identidade».
- `git log -S "aparencia: ["`: `c312f87a` criou a **de baixo** («◧ — a seção
  de aparência»); `8f2d0e53` (31/08, «MICROFONE E SOM ganha o nome da comp, e
  APARÊNCIA volta») criou a **de cima**.

As de cima foram redesenhadas por cima das originais sem as ver, e **os
desenhos são os mesmos**: os mesmos elementos e os mesmos valores, mudando só a
formatação e, no `path` da chave, a ordem dos atributos no texto. Nada no log
diz que a de cima era a pretendida. Ficaram as **de baixo**, que são as que a
tela mostra (num objeto literal a segunda vence). Conferido avaliando o arquivo
em Node com um `document` de mentira: o `GLIFOS` de antes e o de depois têm os
mesmos 22 glifos, com os mesmos atributos na mesma ordem, e nenhum desenho
diferente. Nada percorre `GLIFOS` em ordem (só `GLIFOS[nome]` e
`GLIFOS.cortado`), então a posição das duas chaves no objeto não importa.

O comentário da `chave` que ficou ganhou a frase da de cima que dizia que ela
nomeia também A PORTA (`index.html` a usa nas seções da porta e na portaria). O
cabeçalho do arquivo dizia «São 24, e o número saiu daqui» — 24 contando as
duas repetidas, quando eram 22 —, e deixou de dizer o número, que era o que a
própria frase pregava.

### O guarda e as tabelas que ele cobre

`nenhuma_tabela_constante_da_ui_repete_uma_chave` lê as chaves de topo de
**toda `const` de topo em caixa alta que abre um objeto literal**, em todo
script da `ui/`, com uma leitura (`chaves_de_topo`) que pula cadeias, modelos
com `${…}`, comentários e chaves aninhadas. Escolhi a regra, em vez de uma
lista, para uma tabela nova nascer coberta. Os objetos de estado
(`estadoDosContornos`, `escolhidoDe`, `nomeDe`, `iconeDesenhado`) são escritos
em caixa baixa e mudam em tempo de execução, e ficam de fora.

As 23 tabelas que ela lê hoje, com as chaves de cada uma:

- `glifos.js`: `GLIFOS` (24 na base, 22 depois);
- `frases.js`: `MOTIVOS` (25), `AVISOS` (17), `CAUSAS_DE_RECUSA` (3),
  `PERMISSAO_DE_MICROFONE` (3), `ABERTURA_DO_AUDIO` (5), `ETAPAS` (7),
  `CAMINHOS` (4), `PARADAS` (2), `MOTIVOS_DE_REVOGACAO` (7),
  `NOMES_RECUSADOS` (5), `FRASES` (48), `ANEXOS` (10), `TRANSFERENCIAS` (4),
  `NAO_SALVOS` (4), `PREVIAS` (3);
- `camada-compartilhar.js`: `PERMISSAO_DE_TELA` (3);
- `camada-mods.js`: `FASES_DO_MOD` (9), `NIVEIS` (3);
- `camada-versoes.js`: `FRASES_DA_VERSAO` (11) — só lida;
- `tela-auth.js`: `PADROES` (3);
- `tela-server.js`: `VOLTA` (2);
- `tela-sessao.js`: `RECUSAS_DE_MENSAGEM` (3).

As contagens foram conferidas contra uma contagem independente, por
indentação, num script Python no scratchpad: as 23 batem.

**Contra a cegueira**: uma leitura que não chega à `}` que fecha a tabela, ou
que não lê chave nenhuma, reprova; e as seis tabelas do brief têm de estar
entre as lidas com um mínimo de chaves (`GLIFOS` 20, `MOTIVOS` 5, `AVISOS` 3,
`ETAPAS` 3, `MOTIVOS_DE_REVOGACAO` 1, `FRASES` 30).

### As provas

- **Vermelho** na base: «uma tabela constante da `ui/` repete chave…
  glifos.js: GLIFOS.aparencia (2 vezes) · glifos.js: GLIFOS.chave (2 vezes)»,
  e nada mais nas outras 22 tabelas.
- **Verde** com as duas de cima tiradas.
- **Reversão 1** (o `glifos.js` da base de volta): a mesma frase do vermelho.
- **Reversão 2** (`NotConnected: "FORA DO AR",` posto em `FRASES`, logo depois
  de `Unreachable`): «… frases.js: FRASES.NotConnected (2 vezes)».
- `glifos.js` e `frases.js` devolvidos e conferidos com `cmp`.

## Item 4 — o doc do `main.rs` (`6b41f811`, «docs:»)

O doc «Troca o rastro por um novo quando o atual passa do teto. R15.» (28
linhas) estava colado em cima do doc de `instalar_gancho_de_panico`, que o I-a
(`520f880a`) inseriu entre ele e `girar_o_log`: o rustdoc do gancho começava
falando de girar o rastro, e `girar_o_log` ficava sem doc. As 28 linhas foram
para cima de `girar_o_log`, sem uma palavra mudada.

- Conferido: o `main.rs` sem as linhas `///` é idêntico ao da base, e as linhas
  ordenadas também (só houve movimento).
- **Sem vermelho**: é doc. Nenhum lint nem guarda cobra doc de função privada
  (`missing_docs` é para item público), e não inventei um guarda para isto.
- `cargo test -p seele-app --test frontend` (282 de 282) e
  `cargo test -p seele-app --bin seele-app` (284 de 284; a linha «panicked» da
  saída é a thread `seele-teste-do-panico`, o pânico proposital do teste do
  gancho), fmt e clippy do app.

## O que rodou

Sobre o estado final (`6b41f811`), e também depois de cada item:

- `cargo test -p seele-app --test frontend`: 281 de 281 na base e depois dos
  itens 1, achado e 2 (os guardas cresceram dentro de testes que já existiam);
  282 de 282 depois do item 3, com o guarda novo.
- `cargo test -p seele-app --bin seele-app`: 284 de 284 (item 4).
- `cargo xtask check-runtime`: verde depois de cada item e no fim.
- `cargo fmt --all -- --check` antes de cada commit, e
  `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`:
  limpos (o `ld: duplicate -rpath` é ambiental).
- `ajustes-v013.cjs`, com
  `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`:
  verde no fim, com as cenas v0.13, J, R e W.
- `telas.cjs`: as nove cenas (`sessao`, `pessoas-gaveta`, `canais-gaveta`,
  `moderacao`, `moderacao-confirmar`, `portaria`, `queda`, `fim`, `anexos`)
  saem com 0. Todas imprimem uma linha de «erros:» — «Não foi possível conferir
  os MODs: Error: disconnected» (`base.js:584`) e «voz: TypeError: Cannot read
  properties of null (reading 'supressao')» (`tela-server.js:535`, o duble dela
  responde `null` a `controles_da_voz`). **Já estavam na base**: rodei a cena
  `sessao` com o `tela-boot.js` e o `camada-mods.js` de `9599bec5` no lugar, e a
  linha é a mesma. Nenhum dos dois arquivos é do lote.
- Os 28 roteiros de `tools/roteiros/`, um por um, pelo
  `tools/carga-da-casca.py`: 28 de 28 no fim.

## O que não rodou

- **A conformidade**: pausada, e o lote não precisou dela — nada no fio, no
  servidor nem na FFI mudou.
- As outras bancadas de navegador do `ci.yml`: nenhuma lê `tela-boot.js`,
  `camada-mods.js` ou `glifos.js` de um jeito que o lote mude, além da
  `ajustes-v013.cjs`; o `check-runtime` roda as que ele roda.
- O roteiro no celular: o caso `apelido-tomado` entrou no roteiro do app de
  mesa; quem destrava o caso no celular é a F8, com ele.
- Nada com `cfg(windows)` ou Linux foi escrito.

## Preocupações

1. **O achado do I-4 (`55393d9a`) é uma mudança de comportamento que o brief
   não pediu.** Entrou porque, sem ela, a troca de apelido não tinha efeito e a
   cena do I-4 não teria como mostrar a tentativa seguinte entrando; ficou num
   commit próprio, com prova e reversão, para poder ser revisto ou separado. O
   efeito colateral: depois de uma recusa por apelido, o RECONECTAR e a batida
   da espera, que chamam `conectar()`, passam a ler o perfil em vez de repetir o
   nome recusado — que é o que se quer, mas é diferente de antes.
2. **O `esquecer` do I-3 apaga também um sim anterior a outra lista.** Quem
   desiste durante a gravação perde o aceite deste servidor por inteiro, e a
   próxima pergunta não mostra o aviso «a lista mudou». Antes do lote, o
   `aceitar_mods` já tinha sobrescrito esse sim do mesmo jeito, então nada que
   existia se perde além do que a gravação já perdia.
3. **A obtenção continua depois de desistir.** O I-3 não interrompe o laço de
   `obterOConjunto`: os pacotes que faltavam ainda chegam ao disco, desligados e
   sem sim que os autorize (a próxima entrada pergunta). O brief pedia o sim, e
   não a obtenção.
4. **Hospedar que falha antes de `conectar`** deixa a credencial do apelido
   tomado guardada, presa ao alvo dela, até a tentativa seguinte. Ela não vai a
   outro servidor por causa disso; só não é esquecida naquele instante.
5. **Conflitos de integração.** O guarda do R agora lista linhas exatas de
   `conectar` em `tela-boot.js`; a F4, que mexe depois em `tela-boot.js`, vai
   ter de manter essas linhas (ou atualizar a lista com a razão). Em
   `camada-mods.js` o diff ficou só dentro de `aceitarOsMods`, longe das 8
   linhas da F3.

## O merge do `0.16/lotes` (`5b47b598`)

A pedido da coordenação, o `0.16/lotes` de `baed898c` entrou no ramo depois do
relatório (`6904ab06`): a onda 6, a nona integração e o conserto da
orquestradora no guarda do R (`963593b7`). Sem conflito de texto; o guarda é o
único ponto em que os dois trabalhos se encontram.

**As duas intenções ficaram.** A do W segue `recusaDoApelido` (partes 1, 2 e
6). A de `963593b7` aceita o `beforeunload` da casca larga (`larga-casca.js`,
da F8) só com `fecharAceiteDeMods()` antes do `preventDefault`, mantém
proibidos os dois vetos do Tauri, e conta as ocorrências de `beforeunload`. A
contagem não pega linha nenhuma do W: nenhuma cita `beforeunload`.

**Um aperto, no laço dela, sem afrouxar nada.** O `fecharAceiteDeMods()` do
ouvinte esquece a credencial da pergunta dos MODs, e não a que um apelido
tomado deixou. O ouvinte só segura com a sessão à vista, e aí `recusaDoApelido`
está vazia: `#tela-sessao` só aparece pela `#tela-auth` (`tela-auth.js`), depois
de um `conectar` que entrou e a esqueceu na primeira linha (parte 6), e toda
tentativa de dentro da sessão passa antes por `ejetar()`, que esconde a sessão.
Esse raciocínio dependia de o ouvinte só segurar com a sessão à vista, e agora
o guarda cobra isso: um `beforeunload` que segura precisa ter
`getElementById("tela-sessao").hidden) return;` antes do `preventDefault`, ou
reprova dizendo que `recusaDoApelido` pode estar viva fora da sessão.
`larga-casca.js` não foi editada.

- **Reversão do aperto** (a linha da sessão tirada da `larga-casca.js`): «`larga-casca.js` segura o `beforeunload` sem conferir antes que a sessão está à vista…».
- **A parte dela continua reprovando** (o `fecharAceiteDeMods();` tirado): «`larga-casca.js` segura o `beforeunload` sem esquecer antes a credencial da tentativa…».
- Nas duas vezes, o arquivo voltou idêntico, conferido com `cmp`.

**Sobre o estado mesclado:** `--test frontend` 283 de 283; `check-runtime`,
fmt e `clippy -p seele-app` limpos; a `ajustes-v013.cjs` verde; os 28 roteiros
passando, `apelido-recusado.js` entre eles; as nove cenas da `telas.cjs` saindo
com 0, com a mesma linha de «erros:» de antes, que não é do lote.

**Uma preocupação a mais.** Com duas credenciais da tentativa, a casca larga
esquece só a dos MODs no `beforeunload`, e cobre a do apelido pelo estado da
tela. Se um dia ela precisar segurar fora da sessão, o aperto reprova, e o
conserto natural é uma função de `tela-boot.js` que esqueça as duas, chamada
pela casca larga. Esse conserto mexe num arquivo da F8.

## Rodada 1

FIX_BASE `e0329141`. A revisão aprovou o lote — conforme, sem Crítico nem
Importante, sem vazamento da credencial nas sondas, e o consentimento só falha
para o lado conservador — e deixou seis Menores. Esta rodada fecha cinco; o M3
(o `esquecer` atrasado que apaga um sim dado de propósito a uma pergunta mais
nova, conservador) e a ressalva de `instalar_mod_do_catalogo` no `main.rs`
ficaram fora, por decisão da coordenação.

| commit | item | o que muda |
|---|---|---|
| `1efff430` | M1 | `conectar` conta as tentativas (`tentativas`), e a recusa por apelido só guarda a credencial se a dela ainda é a mais nova; o passo 5b da cena W; a parte 6 do guarda cobra a contagem e a conferência |
| `c30a6ef7` | M2 | a falha do `esquecer` de quem desistiu vai a `#boot-erro`, dizendo que o sim ficou gravado e onde desfazê-lo; a cena 5d |
| `114e1c7a` | M4 | o guarda das tabelas lê também `Object.freeze({`, e o doc dele diz o que fica de fora |
| `0c66a3ad` | M5 | docs: o comentário da parte 3 do guarda do R |
| `383718ed` | M5, acréscimo | docs: os comentários de `baterDeNovo` (`tela-auth.js`) e de `reconectar` (`tela-fim.js`), autorizados pela orquestradora |
| `98b7eacf` | M6 | o passo 3b da cena W prova o `??=`, e o passo 5 mede o que diz |

### M1 — a recusa de uma tentativa mais velha não guarda a credencial (`1efff430`)

A sonda P1 da revisão virou o passo 5b da cena W. Uma tentativa ao servidor da
cena, com o convite, leva 600 ms e volta com `NicknameTaken`; aos 100 ms,
`hospedar()` entra no servidor daqui. O servidor de mentira ganhou um `connect`
lento por alvo e o servidor daqui, que aceita a entrada. A cena confere que a
corrida foi a esperada (o convite ao alvo, depois o servidor daqui sem nada) e
que o `hospedar` entrou, para não medir à toa.

- **Vermelho** em `e0329141`: «uma tentativa a 192.0.2.17:8384, recusada pelo
  apelido depois que o hospedar já tinha entrado no servidor daqui, deixou a
  credencial guardada para uma «tentativa seguinte» que já tinha acontecido…».
- **O conserto**: `let tentativas = 0;` em `tela-boot.js`; `const esta =
  ++tentativas;` em `conectar`, junto do esquecimento e antes da primeira
  espera; e a guarda passa a ser `if (credencial != null && esta ===
  tentativas)`. É a mesma ideia da identidade da pergunta em `aceitarOsMods`.
  Só a guarda da credencial depende da conferência; o esquecimento do
  `ultimoApelido` na mesma recusa continua incondicional, porque reler o perfil
  depois de uma recusa atrasada não custa nada.
- **Guarda**: a linha da guarda muda na lista da parte 1, e a parte 6 cobra
  `const esta = ++tentativas;` antes da primeira espera e `esta === tentativas`
  na linha que guarda.
- **Reversão** (a conferência tirada da guarda): a bancada reprova com a frase
  do vermelho; o guarda, pela lista da parte 1 («…ganhou um caminho novo…»).
  **Reversão da posição** (a contagem descida para depois da primeira espera,
  que a parte 1 não segue): a bancada e o roteiro ficam verdes, porque são
  sequenciais, e a parte 6 reprova com «`conectar` deixou de contar a tentativa
  antes da primeira espera…». Arquivo devolvido e conferido com `cmp` nas duas.

### M2 — a falha de desfazer o sim é dita na tela (`c30a6ef7`)

A sonda P2 virou a cena 5d da cena do R: o servidor de mentira passou a poder
falhar um comando por nome (`testFalhas`). O `aceitar_mods` leva 500 ms, a
pessoa recusa durante a gravação, e o `esquecer_aceite_de_mods` falha com
`{ Recusado: { motivo: "o-disco-recusou" } }`. A cena confere que o sim de fato
ficou, e que a tela o diz.

- **Vermelho** em `1efff430`: «…desfazê-lo falhou, e a tela não disse que o sim
  ficou nem onde desfazê-lo — a entrada diz null…».
- **O conserto**: a falha vai a `#boot-erro`. A pergunta já fechou, e quem
  desistiu está na entrada, sob ela. A frase é «O SIM AOS MODS DE <alvo> FICOU
  GRAVADO.», seguida da falha e de «Desfaça em CONFIGURAÇÕES · MODS, na lista
  SERVIDORES A QUEM VOCÊ DISSE SIM.», que são os nomes da seção e do bloco no
  `index.html`. Ela mora em `camada-mods.js` (`simQueFicouGravado`), e não em
  `frases.js`, por três razões. Não é a frase de uma variante de erro: leva o
  alvo e a falha dentro dela, e nenhuma tabela de `frases.js` tem essa forma.
  Só existe nesta pergunta, pela razão que deu frases próprias a
  `camada-versoes.js`. E `frases.js` é de outra frente.
- **Reversão** (o `camada-mods.js` de antes do M2): a bancada reprova com a
  frase do vermelho. O guarda de texto não cobre o M2; a prova pedida é a cena.
  Arquivo devolvido e conferido com `cmp`.

### M4 — o guarda das tabelas lê as congeladas (`114e1c7a`)

- **Vermelho antes**: com um `6:` repetido posto em `CAPACIDADES_POR_API`
  (`base.js`, por um instante), o guarda de `e0329141` **passava**: ele só lia a
  linha terminada em `= {`.
- **O conserto**: o filtro aceita `{` e `Object.freeze({`. O guarda passa a ler
  **45 tabelas** (as 23 de antes e 22 congeladas, como a revisão mediu), nenhuma
  repetida e nenhuma leitura perdida; as chaves das 45 foram conferidas contra a
  contagem independente por indentação, que também passou a ler as congeladas.
  `CAPACIDADES_POR_API` entrou na lista contra a cegueira. O doc diz o que fica
  de fora: as chaves dentro de entradas aninhadas, a tabela numa linha só ou que
  não abre um objeto na linha da `const` (`Object.freeze([`, ou montada por
  outra expressão), e os objetos de estado em caixa baixa.
- **Prova**: com o `6:` repetido, reprova com «base.js: CAPACIDADES_POR_API.6 (2
  vezes)». **Reversão** (o filtro antigo de volta, sem duplicata): reprova por
  cegueira, «este guarda não leu `CAPACIDADES_POR_API`…». `base.js` e
  `frontend.rs` devolvidos e conferidos com `cmp`.

### M5 — os comentários que o I-4 tornou falsos (`0c66a3ad`, `383718ed`)

- A parte 3 do guarda do R (doc e comentário do corpo) diz agora que três
  chamadas **passam** credencial por argumento, que `conectar` anexa sozinho a
  do apelido tomado quando o alvo é o mesmo — inclusive nas chamadas sem
  argumento —, e por que ela nunca é um convite gasto.
- Com a autorização da orquestradora, o comentário de `baterDeNovo`
  (`tela-auth.js`) deixou de dizer que `conectar()` «não lembra segredo» e que a
  credencial «só é guardada pela pergunta dos MODs», e o de `reconectar`
  (`tela-fim.js`) deixou de dizer que o token «não fica guardado em lugar
  nenhum». Os dois dizem os dois lugares e por que nenhum chega ali com um
  convite gasto. Só comentário: os dois arquivos sem as linhas de comentário são
  idênticos aos de antes.

### M6 — o `??=` e o passo 5 (`98b7eacf`)

- **O `??=`**: o passo 3b novo da cena W. Depois de uma recusa pelo apelido, a
  pessoa cola um link novo ao mesmo servidor (o servidor de mentira passou a
  aceitar um segundo convite, e o `analisar_convite` a devolver o token do link
  novo), e a tentativa tem de levar o convite do link. Verde com o produto, que
  já estava certo; com o mutante `credencial =`, reprova com «…a credencial que
  a recusa anterior deixou passou por cima da que a pessoa acabou de colar» (o
  roteiro continua verde com ele, e o guarda reprova pela lista da parte 1).
- **O passo 5**: a credencial muda de lugar — a tentativa a tira do lugar do
  apelido tomado ao começar, e a entrega à pergunta dos MODs; RECUSAR a esquece
  de lá. O lugar do apelido passou a ser conferido logo depois da tentativa, e
  não depois do RECUSAR, que não mexe nele; a frase diz isso. Com um mutante que
  guarda a credencial também na recusa pelos MODs, o guarda e o roteiro ficam
  verdes, e o passo 5 reprova com «a tentativa que parou na pergunta dos MODs
  deixou a credencial também no lugar do apelido tomado…».

### Re-revisão — a mesma corrida pela pergunta dos MODs (sonda P6)

Uma tentativa velha que volta `ModsNaoAceitos` depois que o `hospedar` entrou
**não abre a pergunta**: `if (esta === tentativas && levarParaOAceiteDeMods(…))`
em `conectar`, e a recusa dela cai na linha vermelha da entrada, como a de toda
tentativa velha. Não abrir sem credencial, porque isso seria o mesmo modal por
cima da sessão, para um sim que não entra. É coerente com o R: a pergunta é da
batida, e só de uma que ainda vale. Provas:
- O passo 5c da cena W ficou vermelho antes do conserto: «…guardou o convite na pergunta…».
- Com o conserto, ficou verde.
- Com a conferência tirada, a bancada e o guarda reprovam.
- A parte 6 do guarda passou a cobrar a conferência também nesta chamada.

### O que rodou na rodada

Depois de cada item, e no fim (`98b7eacf`): `cargo fmt --all -- --check`;
`cargo clippy -p seele-app --all-targets --all-features -- -D warnings`;
`cargo test -p seele-app --test frontend`, 283 de 283; `cargo xtask
check-runtime`; a `ajustes-v013.cjs`, verde; e os 28 roteiros, verdes. A
conformidade não foi necessária. Nada em `crates/` nem em
`ui/camada-versoes.js`. `base.js` só foi mexido dentro das provas do M4, e
voltou idêntico; de `tela-auth.js` e `tela-fim.js`, só os dois comentários.
