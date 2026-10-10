# Lote W2 da F5: relatório

Base `d9124cea` (branch `0.16/lotes`). Branch `0.16/lotes-w2`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a4c73fc301003acb3`. Tudo
local: sem push, tag nem release. Sem conformidade.

| commit | item | o que muda |
|---|---|---|
| `c47c60dc` | 1 (medida) | este relatório, com a medida e a v0.15.1, escrito antes do conserto |
| `d99d363d` | 2 | o `catch` de `conectar` abre com o ramo da tentativa substituída (`esta !== tentativas`): ela não abre a pergunta, não vai à espera nem à linha vermelha, e a recusa vai ao `seele.log` por `registrarNoAnfitriao`, como aviso; a cena `aTentativaSubstituidaNaoFala` na `ajustes-v013.cjs`; o guarda `a_tentativa_que_outra_substituiu_nao_fala_na_tela`; o guarda da credencial lê a conferência da pergunta pelo mesmo ramo; os comentários do lote W e de perto de :375 em `tela-boot.js` |
| `4fd9aaa7` | 3 | a última linha da frase do `ModsNaoAceitos` passa a «Para aceitar ou recusar esta lista, entre de novo neste servidor.»; o passo 5 da cena; os comentários de `frases.js` e `camada-mods.js:10` |
| `e110014e` | 4 | só o comentário do `main.rs` (perto de :8020), isolado |
| `866d46a8` | 2 (comentários) | o doc de `levarParaAEspera` (`tela-auth.js`) e o comentário da espera no `catch` de `conectar` dizem que só a falha da tentativa mais nova chega à espera; só comentário |
| este | — | o relatório final |

**O que não mudou.** Nada em `crates/`, no fio
(`crates/seele-proto/src/{control,version,media,screen}.rs` intocados), no
servidor, em `base.js`, na CSP, em `api/`. Nenhuma dependência nova. Nenhum
`.cjs` novo: a cena entrou na `ajustes-v013.cjs`, que já está no job
`bancadas`. Em `main.rs`, só as linhas de comentário: o arquivo sem as linhas
`//` é idêntico ao da base. Em `tela-auth.js`, idem.

## 1. A medida do caminho

### A reprodução

Uma cena nova na `ajustes-v013.cjs`, `aTentativaSubstituidaNaoFala`, com um
servidor de mentira que responde por alvo, com atraso por alvo. A tentativa
velha vai a `192.0.2.17:8384` com um convite, leva 600 ms e volta
`ModsNaoAceitos`; aos 100 ms, a nova a substitui. Medido na base (`d9124cea`),
com uma cópia temporária da bancada em que as asserções escreviam em vez de
parar (apagada logo depois):

1. **O `hospedar()` substitui a velha** (o caso do achado). O `hospedar` entra
   no servidor daqui; a pessoa desconecta e volta à entrada, e lê na linha
   vermelha, sobre um servidor que ela deixou:

   ```
   ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA NÃO ACEITOU.
   • fulano/exemplo 1.0.0
     repositório: https://github.com/seele/exemplo
     alcança: ler
   A tela para ler e aceitar esta lista ainda não existe neste app.
   ```

   Nada no `seele.log` vindo da janela (o Rust escreve «could not reach the
   server» para toda tentativa que falha, e nada diz que a tela não a mostrou).
2. **Outra escolha na lista substitui a velha**, e a nova é lenta (1,4 s). Com
   a nova ainda andando, a mesma frase aparece na linha vermelha da entrada,
   **visível**, até a nova voltar e escrever por cima. Enquanto isso, o
   CONECTAR, que a nova apagou, está aceso: o `finally` da velha o reacende.
3. **A nova parou na espera da portaria** (`AdmissionPending`), e a velha volta
   pelos MODs: `levarParaAEspera` trata «qualquer falha com a espera na
   frente», e a frase dos MODs toma o lugar da frase da espera, e o botão passa
   de AGUARDANDO PERMISSÃO a TENTANDO DE NOVO.
4. **Não só pelos MODs.** A velha recusada pela portaria (`AdmissionPending`)
   depois que o `hospedar` entrou abre a espera **por cima da sessão**, e a
   espera passa a bater no servidor desta máquina (o `ultimoAlvo` é o dele).

Ou seja: a regra «só a tentativa mais nova abre a pergunta» do lote W fechou
uma porta, e a recusa da velha caiu nas outras duas — a linha vermelha e a
espera —, e a da linha vermelha usa a única frase do `fraseDeErro` que ainda
diz que a tela de aceite não existe.

### Todo caminho que leva à frase do `ModsNaoAceitos`

`ModsNaoAceitos` só nasce num lugar: o `connect` da FFI
(`classify_connect_failure`, `crates/seele-ffi/src/lib.rs`), devolvido pelo
comando `connect` do `main.rs`. Na janela há **um** `invoke("connect"`
(`tela-boot.js`, em `conectar`), e `conectar` nunca relança: o `catch` dele
decide tudo. Então a frase só é alcançável por quem recebe o erro dentro desse
`catch`, ou por quem chama `fraseDeErro` com um valor que veio dele.

| caminho | veredito | como |
|---|---|---|
| `conectar`, tentativa velha, espera fora da frente → `#boot-erro` | **alcançável** | passos 1 e 2 acima |
| `conectar`, tentativa velha, espera na frente → `levarParaAEspera` → `fraseDeErro` em `tela-auth.js` (`#auth-espera-frase` e o registro da tela) | **alcançável** | passo 3 acima |
| `conectar`, tentativa velha, espera largada com batida no ar (`tela.hidden && batendoDaEspera`) | não | `levarParaAEspera` devolve `true` e cala |
| `conectar`, tentativa mais nova | **não** | `levarParaOAceiteDeMods` só devolve `false` quando `motivo?.ModsNaoAceitos` é falso; com ele, abre a pergunta e devolve `true`, sempre — inclusive com a espera na frente, porque a pergunta vem antes da espera |
| a volta do aceite (`aceitarOsMods` → `conectar(alvo, null, credencial)`) | não | é uma tentativa nova: um `ModsNaoAceitos` nela (o conjunto mudou entre a pergunta e a volta) reabre a pergunta |
| `camada-mods.js`: `simQueFicouGravado`, o `catch` de `aceitar_mods`, `obterOConjunto` (`mods_instalados`, `instalar_mod_do_catalogo`) | não | as falhas são de `FalhaNoMod` e do catálogo, nunca do `connect` |
| `tela-fim.js` (`reconectar`) | não | chama `conectar()`, e cai na primeira linha; `tela-fim.js` não chama `fraseDeErro` |
| `tela-auth.js`, a batida da espera (`baterDeNovo`) | não | chama `conectar()`: é uma tentativa nova |
| `tela-boot.js`: `hospedar` (e `hospedarGuardado`, que passa por ele), `cumprirAAbertura`, `abrirVersao`, o `apagar_servidor`, o alcance, a porta no firewall, o ícone do preparar | não | os `catch` deles recebem o erro de `hospedar`, `analisar_convite`, `abrir_versao`, `apagar_servidor`, `abrir_a_porta_no_firewall` e `preparar_icone_do_server`; o `conectar()` que os dois primeiros chamam não relança |
| `camada-servidores.js` (`irParaOServidor`, o `analisar_convite` do campo) e `tela-sessao.js` (`trocarDeServidor`) | não | chamam `conectar`, que não relança; o `fraseDeErro` deles é do `analisar_convite` |
| `base.js` (`atenderOMod`), `mods-regiao.js`, `camada-*.js` restantes, `tela-server.js`, `tela-sessao.js` (prévias) | não | comandos que não devolvem `ConnectionError::ModsNaoAceitos` |
| a ponte do navegador (`ponte-web.js`) | não | o `connect` dela responde `AindaNaoNoNavegador` (tarefa 3.4), e não chega a servidor nenhum |

Ou seja: os dois caminhos alcançáveis são **da tentativa velha**, e nenhum
caminho da tentativa mais nova alcança a frase.

## 2. O conserto: a tentativa substituída não fala, e conta ao `seele.log`

O `catch` de `conectar` (`tela-boot.js`) passa a decidir primeiro de quem é a
recusa:

```js
if (esta !== tentativas) {
  const dito = motivo instanceof Error ? String(motivo) : JSON.stringify(motivo);
  registrarNoAnfitriao(
    "entrada",
    `a tentativa a ${alvo} voltou recusada depois que outra a substituiu, e a tela não a mostra: ${dito}`,
    "aviso",
  );
} else if (levarParaOAceiteDeMods(motivo, alvo, credencial ?? null)) {
  // Nada de linha vermelha: quem manda agora é o diálogo.
} else if (!levarParaAEspera(motivo, ultimoAlvo ?? "")) {
  … a linha vermelha, como antes …
}
```

**Para toda recusa, e não só a dos MODs.** A medida mostrou o motivo: nenhuma
recusa da velha é sobre o lugar em que a pessoa está agora, e a pela portaria
(`AdmissionPending`) era a pior — abria a espera por cima da sessão e a punha
a bater no servidor desta máquina. Não achei recusa da velha que a pessoa
precise ver: a nova está em curso (e fala quando voltar) ou já entrou.

**O que fica igual.** O bloco do `NicknameTaken`, antes do ramo, continua
valendo para todas (esquecer `ultimoApelido`; a credencial só é guardada pela
mais nova, regra do lote W). A trilha continua indo ao console para todas. O
`finally` não mudou (ver «Preocupações»).

**O registro.** A janela tem um caminho para o `seele.log`, e é o usado no
caso parecido de «a janela sabe e a tela não mostra»: `registrarNoAnfitriao`
(`base.js`) → `registrar_da_janela` (`main.rs`). A linha sai como
`a janela disse onde=entrada frase="a tentativa a 192.0.2.17:8384 voltou
recusada depois que outra a substituiu, e a tela não a mostra:
{\"ModsNaoAceitos\":…}"`. `"aviso"` porque é o nível que o filtro de
produção (`seele_app=info`) escreve; o resto sai em DEBUG. O motivo é o erro
como a ponte o mandou, e o `connect` nunca devolve a credencial nele; a cena
confere que o convite da tentativa velha não aparece em chamada nenhuma de
`registrar_da_janela`, e a cena 6 do lote W, que procura o convite em todo
comando fora do `connect`, continua verde com as recusas das corridas 5b e 5c
passando a ir ao registro. O Rust já escrevia «could not reach the server»
para toda falha do `connect` (`seele-ffi`); o que faltava era a janela dizer
que **não mostrou** — sem isso, quem lê o log supõe que a pessoa leu a recusa.

**Os comentários.** O do lote W em `tela-boot.js` («A recusa dela cai na linha
vermelha da entrada, como a de toda tentativa velha») deu lugar ao parágrafo
do lote W2; o de perto de :375 diz que a frase sobreviveu à tela e deixou de
mentir no lote W2, e que nenhuma recusa da mais nova chega a ela; o doc de
`tentativas` ganhou a regra; o comentário da cena 5c da bancada também dizia
«a recusa dela vai à linha vermelha», e foi corrigido.

### As provas

**A cena** `aTentativaSubstituidaNaoFala` (`ajustes-v013.cjs`), com um servidor
de mentira que responde por alvo — o atraso e a recusa lidos quando a chamada
chega, para uma tentativa lenta responder como o servidor dela respondia
quando ela saiu:

1. o `hospedar()` substitui a velha (recusa pelos MODs); a pessoa desconecta,
   volta à entrada, e a linha vermelha está calada; a recusa está no registro,
   como aviso;
2. outra escolha na lista substitui a velha, e a nova leva 1,4 s: com a nova
   andando, a linha vermelha está calada; quando a nova volta
   (`CredentialRejected`), a linha diz a recusa **dela**;
3. a nova para na espera da portaria, e a velha volta pelos MODs: a frase da
   espera continua a da espera, e o botão continua AGUARDANDO PERMISSÃO;
4. a velha recusada pela portaria depois que o `hospedar` entrou não abre a
   espera por cima da sessão;
5. (a frase, abaixo);
6. o convite da velha não chega ao registro.

**Vermelho na base** (`d9124cea`): a cena reprova no passo 1 com «a recusa
pelos MODs de uma tentativa a 192.0.2.17:8384, que o hospedar já tinha
substituído, foi escrita na linha vermelha da entrada, e quem desconecta lê
ali, sobre um servidor que deixou: «ESTE SERVIDOR EXIGE MODS QUE VOCÊ AINDA
NÃO ACEITOU. … A tela para ler e aceitar esta lista ainda não existe neste
app.»». A cópia que não parava (a medida, acima) mostrou os doze vermelhos dos
seis passos.

**O guarda** `a_tentativa_que_outra_substituiu_nao_fala_na_tela`
(`tests/frontend.rs`). Ele lê o `catch` que segue o `invoke("connect"` (sem
comentários, por `js_function`), acha o ramo `if (esta !== tentativas) {` e
casa a chave dele contando, e cobra: nada do que fala na tela
(`levarParaOAceiteDeMods(`, `levarParaAEspera(`, `fraseDeErro(`,
`.textContent`, `.hidden = false`) vem antes do ramo nem está dentro dele; os
três primeiros continuam depois dele (um guarda que não acha o que cobra passa
sem olhar); depois do ramo vem `else` (ou o ramo volta); e dentro dele há uma
chamada só de `registrarNoAnfitriao`, com `"aviso"` no terceiro argumento, o
`alvo` no segundo, e o `motivo` nomeado no ramo. Ele não lê comentário nem
procura uma marca posta no código para ele: um ramo que voltasse a falar, ou a
calar sem registrar, reprova pela forma.

O guarda da credencial
(`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`) cobrava
a conferência da pergunta como `esta === tentativas &&` na mesma linha do
`levarParaOAceiteDeMods(`. A linha mudou (agora é o `else if` do ramo), e ele
passou a cobrar pelo mesmo ajudante (`ramo_da_tentativa_substituida`) que a
pergunta só é aberta depois do ramo. A linha da lista dele foi trocada pela
nova.

## 3. A frase nova

A medida mostrou que, com o conserto, **nenhum caminho de hoje chega à
frase**: a mais nova sempre abre a pergunta, e a velha não fala. O ramo fica
para o caminho que um dia chegar, e então ela tem de dizer a verdade com esta
versão. A última linha passou de

> A tela para ler e aceitar esta lista ainda não existe neste app.

a

> Para aceitar ou recusar esta lista, entre de novo neste servidor.

Escolhida pelo que a medida mostrou: entrar de novo é uma tentativa nova, e a
tentativa mais nova leva todo `ModsNaoAceitos` à pergunta. É o mesmo remédio
que as frases vizinhas da mesma família já davam (`ModsRecusados`: «Entrar de
novo mostra a lista outra vez»; `ModsMudaram`: «entre de novo para ler a
lista nova»). O título e a lista ficaram como estavam.

**O teste da frase** é o passo 5 da cena: `fraseDeErro` com um
`ModsNaoAceitos` mostra a lista (id e versão do MOD), não diz que a tela «não
existe», e manda «entrar de novo» — e o que ela manda fazer funciona: uma
tentativa nova ao mesmo servidor abre a pergunta. Vermelho com a frase velha
(medido antes do conserto da frase, e de novo na reversão R4).

Os comentários: o de `frases.js` (:673-683) dizia «é uma pergunta que ainda
não tem tela. Enquanto ela não existe, a lista é lida aqui»; passa a dizer que
a pergunta tem tela, quem leva a ela, que nenhum caminho de hoje chega ao
ramo, e por que a última linha mudou. O de `camada-mods.js:10` cita a frase
velha como história, e continua citando, com o que aconteceu depois.

## 4. O comentário do `main.rs`

Perto de :8020 dizia «A tela que mostra a lista e tem o botão ainda não
existe». Passa a dizer que ela é `ui/camada-mods.js` desde 17/09/2026
(`65ad0653`) e quem chama cada um dos três verbos (conferido por
`grep 'invoke("aceite_de_mods"…'`). Commit isolado (`e110014e`), só
comentário: o `main.rs` sem as linhas de comentário é idêntico ao do commit
anterior.

## 5. A v0.15.1 publicada

Conferido pelo `CLAUDE.md` (o curl dos releases):

```
v0.15.1 · 54f542f94 · 2026-10-08T04:14:07Z
```

O commit é o que a orquestradora cita (`54f542f`). Lido só com
`git show 54f542f94:…`; nada mudou na linha 0.15.

- A frase está lá: `apps/seele-app/ui/frases.js:505`, idêntica.
- A tela de aceite existia: `camada-mods.js` tem `abrirAceiteDeMods` e
  `levarParaOAceiteDeMods(motivo, alvo)`.
- Tentativas que se substituem existiam **mecanicamente** (o `hospedar` e a
  lista de servidores não esperam o `connect` em curso), mas **não havia
  contador**: o `catch` de `conectar` era
  `if (levarParaOAceiteDeMods(motivo, ultimoAlvo ?? "")) … else if (!levarParaAEspera(…)) …`,
  sem condição.
- Por isso **a frase não é alcançável na v0.15.1**: todo `ModsNaoAceitos`,
  da tentativa velha ou da nova, abre a pergunta antes de qualquer outra
  coisa, e o único `invoke("connect"` da janela é esse. A corrida existe lá
  com outro sintoma — a pergunta da velha abre por cima da sessão, e com o
  `ultimoAlvo` (o servidor novo), que é o que o lote R (o alvo da chamada) e
  o lote W (`tentativas`) consertaram na 0.16.
- A frase passou a ser alcançável em `8effec98` (09/10, lote W, re-revisão da
  rodada 1, sonda P6), que pôs `esta === tentativas &&` antes de
  `levarParaOAceiteDeMods` e deixou o resto da cadeia como estava.
- O passo 4 da medida, esse sim, existe na v0.15.1: o `levarParaAEspera` de lá
  é o mesmo (`AdmissionPending` vem sempre), e nada no `catch` distingue a
  tentativa velha. Uma tentativa lenta pendente na portaria, voltando depois
  que o `hospedar` entrou, abre a espera por cima da sessão. Não medido numa
  máquina com a v0.15.1, só lido; nada muda na linha 0.15.

## 6. As reversões

Cada uma: um script no scratchpad aplicava o mutante, o arquivo era tocado, a
prova rodava, o arquivo voltava de uma cópia, era tocado de novo, e o `cmp`
conferia. **Todas voltaram idênticas**, e o `git status` ficou limpo depois de
cada uma.

- **R1** (a cadeia do `catch` de volta à da base — a velha passa pela
  pergunta com a conferência do lote W, pela espera e pela linha vermelha):
  a cena reprova no passo 1 com a frase do vermelho; o guarda novo reprova com
  «o `catch` de `conectar` não tem mais o ramo da tentativa que outra já
  substituiu…»; o guarda da credencial reprova pela lista («ganhou um caminho
  novo em `tela-boot.js`: if (esta === tentativas && levarParaOAceiteDeMods…»).
- **R2** (o ramo fica, e a velha volta a escrever na linha vermelha dentro
  dele): a cena reprova no passo 1, com a frase; o guarda reprova com «o ramo
  da tentativa que outra já substituiu fala na tela (`fraseDeErro(`)…».
- **R3** (o ramo cala a velha e não registra nada): a cena reprova no passo 1
  com «a recusa por ModsNaoAceitos da tentativa a 192.0.2.17:8384, que outra
  já tinha substituído, não chegou ao seele.log como aviso…»; o guarda
  reprova com «o ramo da tentativa substituída não escreve a recusa no
  registro, uma vez, por `registrarNoAnfitriao` (0 chamada(s))…».
- **R4** (a frase velha de volta em `frases.js`): a cena reprova no passo 5
  com «a frase dos MODs ainda diz que a tela de aceitar não existe, e ela
  existe: «ESTE SERVIDOR EXIGE MODS…»».

## 7. O que rodou

No fim (`866d46a8`): `cargo test -p seele-app --bin seele-app`, 312 de 312;
`cargo test -p seele-app --test frontend`, 287 de 287 (286 na base, mais o
guarda novo; o da credencial mudou por dentro); `cargo xtask check-runtime`,
verde; `cargo fmt --all -- --check`, limpo; `cargo clippy -p seele-app
--all-targets -- -D warnings`, limpo (o único aviso é o do ligador, `duplicate
-rpath '/usr/lib/swift'`, que já estava na base). As bancadas
`conversa-com-estado`, `telas`, `diagnostico-de-mods`,
`contribuicoes-e-camadas` e `ajustes-v013`, verdes. A `telas` imprime um
`TypeError … 'supressao'` em `desenharQualidadeDaVoz` e «Não foi possível
conferir os MODs: Error: disconnected»; a saída é idêntica à da base (rodada
com os três arquivos da `ui/` da base no lugar). Os roteiros
`aceite-de-mods.js` e `apelido-recusado.js` pela `tools/carga-da-casca.py`:
carga limpa, e o relato do primeiro idêntico ao da base. Nenhuma conformidade,
nenhum `cargo test --workspace`.

## Preocupações

1. **O `finally` da tentativa velha ainda mexe no que é da nova.** Medido no
   passo 2 (cópia temporária da bancada): com a nova andando, o `finally` da
   velha reacende o CONECTAR que a nova apagou — o que abre uma terceira
   entrada por baixo dela —, e apaga a linha de etapa (`mostrarEtapa(null)`).
   Não consertei: o botão se conserta com `if (esta === tentativas)` em volta
   do `botao.disabled = false`, mas a linha de etapa é escrita pelos eventos
   `ConnectStageChanged` das duas tentativas, sem dizer de qual são, e calar o
   `mostrarEtapa(null)` da velha deixaria a etapa dela na tela quando ela
   termina depois da nova. Fica para quem decidir o escopo.
2. **O roteiro `tools/roteiros/aceite-de-mods.js` relata que o aceite não
   grava nem entra, e não reprova.** Na base e aqui, ele imprime
   `gravou o sim: alvo=undefined conjunto=undefined` e
   `tentou entrar de novo: false` (o lote W passou a obter o conjunto antes de
   gravar, e o duble do roteiro não tem o MOD instalado), e não tem `exigir`:
   «existir não é funcionar». Não mexi; está fora do lote.
3. **Comentários que ainda contam a tela como futura**, fora da lista do
   brief: `tests/frontend.rs` perto de :543 («o que falta é a tela com o botão
   de aceitar, que é de um plano posterior»), num parágrafo que logo abaixo diz
   que a lista ficou vazia em 17/09 — lido como história, mas com o verbo no
   presente. Não mexi.
4. **`registrarNoAnfitriao` ganhou o primeiro chamador que não é de MOD.** O
   doc dele (`base.js`) diz «quem lê isto é quem hospeda»; para a recusa de uma
   entrada, quem lê é quem investiga o `seele.log` desta máquina. Não mexi em
   `base.js`.
5. **O worktree.** As edições a este worktree vindas da sessão do controlador
   eram recusadas por um gancho («belongs to a different worktree»); entrei
   nele com `EnterWorktree` (só este agente), e foi daqui que tudo foi escrito.
