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
| `c46a62d0` | — | o relatório da primeira entrega |
| `6af0bedd` | rodada da revisão, 1 | o `finally` de `conectar` só reacende o CONECTAR quando `esta === tentativas`; o passo 2 da cena cobra o botão apagado depois que a velha termina e aceso depois que a nova termina; a parte 5 do guarda |
| `8f4f4724` | rodada da revisão, 3 | só comentário: o histórico da `AGUARDANDO_TELA` em `tests/frontend.rs` (perto de :543) passa ao passado, e o doc de `registrarNoAnfitriao` (`base.js`) diz quem lê |
| `7cd7d98c` | rodada da revisão, 2 | só comentário: o da linha de etapa no `finally` corrige o motivo de ela ter ficado fora (ver 2, «A linha de etapa») |
| `b4dcf282` | — | o relatório da rodada 1 da revisão |
| `525b1907` | rodada 2 da revisão, I1 | a tentativa substituída desiste antes de cada apagar do CONECTAR (depois do `apelido_local` e depois da pergunta do nome), e registra; o `finally` da pergunta do nome confere a tentativa; a cena `oConectarTemDono` (bloco próprio); o guarda `so_a_tentativa_mais_nova_mexe_no_conectar`, que substitui a parte 5 do guarda da rodada 1 |
| `9a62a8f4` | rodada 2 da revisão, M1 — **EM CURSO, NÃO RODADO** | `batendoDaEspera` guarda o número da tentativa da batida, e só a falha da própria batida se cala em `levarParaAEspera`, indo ao `seele.log`; a cena `aBatidaDaEsperaSoCalaASiMesma` (bloco próprio). Escrito durante a pausa de medição de CPU: nada compilado, nada rodado |
| este | — | o relatório da rodada 2, com o que falta rodar |

**O que não mudou.** Nada em `crates/`, no fio
(`crates/seele-proto/src/{control,version,media,screen}.rs` intocados), no
servidor, na CSP, em `api/`, em `camada-mods.js` depois da primeira entrega.
Nenhuma dependência nova. Nenhum `.cjs` novo: as cenas entraram na
`ajustes-v013.cjs`, que já está no job `bancadas`; as duas da rodada 2, cada
uma num bloco próprio no fim, para o merge com o W3. Em `main.rs` e `base.js`,
só linhas de comentário: cada um, sem as linhas de comentário, é idêntico ao
da base. `tela-auth.js` só tinha comentário até a rodada 2; o M1 mexe no
código dele (`9a62a8f4`).

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
   CONECTAR, que a nova apagou, está aceso: o `finally` da velha o reacende
   (medido na mesma cópia: `disabled` é `false` aos 900 ms, com a nova
   andando).
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
| `conectar`, espera largada com batida no ar (`tela.hidden && batendoDaEspera`, em `levarParaAEspera`) | não leva à frase | Corrigido na rodada 2 (M1 da revisão). Antes do W2, qualquer tentativa chegava ali; depois do ramo da tentativa substituída, **só a mais nova** chega, e era calada sem tela e sem registro sempre que uma batida da espera estava no ar — inclusive uma entrada nova que não era a batida (medido pela revisão: VOLTAR e HOSPEDAR AQUI, igual na base). Desde `9a62a8f4` (em curso, não rodado), só a falha da própria batida se cala ali, e vai ao `seele.log`; a de outra tentativa segue para a linha vermelha. Ver 2, «A batida da espera» |
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
`finally` mudou na rodada da revisão (ver «O CONECTAR», abaixo).

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

### O CONECTAR (rodada da revisão, `6af0bedd`)

O `finally` de `conectar` reacendia o botão para toda tentativa. Medido no
passo 2 da cena: a velha terminava aos 600 ms e reacendia o CONECTAR que a
nova tinha apagado, com a nova ainda andando, e um clique ali abria uma
terceira entrada por baixo dela. Agora:

```js
if (esta === tentativas) botao.disabled = false;
```

A mais nova sempre chega ao próprio `finally` (entre o `botao.disabled =
true` e o `try` só há duas linhas que não falham), ou ao `finally` da pergunta do nome,
que também devolve o botão; e o único `return` antes de contar a tentativa
(`!ultimoAlvo`) não apaga nada. Então o botão não fica apagado sem dono.

- **A cena**, passo 2: depois que a velha voltou (conferido pela chamada
  respondida) e com a nova andando, o CONECTAR está apagado; depois que a nova
  volta, está aceso. Vermelho antes do conserto com «com a tentativa a
  192.0.2.18:8384 ainda andando, a anterior, a 192.0.2.17:8384, terminou e
  reacendeu o CONECTAR que a nova tinha apagado: um clique ali abre uma
  terceira entrada por baixo da que está em curso».
- **O guarda**, na rodada 1, era a parte 5 de
  `a_tentativa_que_outra_substituiu_nao_fala_na_tela`, e só olhava o `finally`
  do `connect`. Na rodada 2 ele virou `so_a_tentativa_mais_nova_mexe_no_conectar`,
  que cobre todo apagar e todo acender de `conectar` (ver abaixo).

### O CONECTAR, de novo (rodada 2 da revisão, I1, `525b1907`)

**O defeito, regressão do `6af0bedd`** (medido pela revisão). A velha podia
ficar suspensa **antes** de apagar o botão — na pergunta do nome, a primeira
entrada sem nome desta execução — enquanto a mais nova começava e terminava
pelo HOSPEDAR AQUI, alcançável por Tab por baixo da caixa. Fechada a caixa, a
velha seguia: apagava o CONECTAR depois de a nova o ter devolvido, chamava o
`connect` (que volta `AlreadyConnected` com a nova dentro, ou revogaria a
geração da nova se ela ainda andasse), e o `finally` dela não devolvia o
botão. Quem saía da sessão achava o CONECTAR apagado e a lista de servidores
sem abrir. O comentário «o botão nunca fica apagado sem dono» era falso.

**O vermelho na ponta `b4dcf282`**, pela cena nova `oConectarTemDono`
(bloco próprio na `ajustes-v013.cjs`), numa cópia que não parava:
- caso 1 (a sequência do revisor: a velha na pergunta do nome, `hospedar()`,
  a caixa fecha, a pessoa sai da sessão): «a pergunta do nome fechou depois que
  o hospedar já tinha entrado, e a tentativa velha apagou o CONECTAR que a mais
  nova tinha devolvido…»; a velha chamou o `connect` a `192.0.2.17:8384`; de
  volta à entrada, o CONECTAR apagado e o clique sem abrir a lista;
- caso 2 (as duas esperando 500 ms pelo `apelido_local`, a velha volta
  primeiro): «a pergunta do nome está aberta por cima da sessão: foi a
  tentativa velha que a abriu…»; a velha chamou o `connect`; o CONECTAR
  apagado no fim;
- caso 3 (a velha na pergunta do nome, a nova ainda andando quando a caixa
  fecha): verde na ponta, porque ali a velha reacendia e logo apagava de novo.
  Ele ficou vermelho no estado intermediário — com as duas desistências e sem
  conferir o `finally` da pergunta —, com «a pergunta do nome da tentativa
  velha fechou com a mais nova ainda andando, e o `finally` dela reacendeu o
  CONECTAR…». É ele que prova a conferência do `finally` da pergunta.

**O conserto** (`tela-boot.js`):
- depois do `apelido_local` e depois da pergunta do nome — as duas esperas que
  vêm antes dos dois apagares —, a tentativa que já não é a mais nova desiste:
  `if (esta !== tentativas) { registrarNoAnfitriao("entrada", …, "aviso");
  return; }`. Ela não apaga o botão, não pergunta o nome por cima da nova e
  não chama o `connect`;
- o `finally` da pergunta do nome passa a `if (esta === tentativas)
  $("botao-conectar").disabled = false;`;
- o comentário do `finally` do `connect` diz o que vale agora: só a mais nova
  apaga e só a mais nova devolve, e a última a começar sempre termina por um
  caminho que devolve.

**O guarda** `so_a_tentativa_mais_nova_mexe_no_conectar`: em `conectar` sem
comentários, todo `.disabled = false` começa com `if (esta === tentativas)`;
todo `.disabled = true` tem, entre a última espera (`await`) antes dele e ele,
um `if (esta !== tentativas) {` cujo bloco registra e volta (sem espera no
meio, nada mais roda entre a conferência e o apagar); há ao menos dois de cada;
e o `finally` do `connect` devolve o botão.

**Verde** em `525b1907`: a `ajustes-v013` inteira (com os três casos) e
`--test frontend`, 288 de 288. As reversões R6 a R9 estão escritas e **ainda
não rodaram** (ver 6).

### A batida da espera (rodada 2 da revisão, M1, `9a62a8f4`, EM CURSO, NÃO RODADO)

**O defeito, anterior ao W2** (medido pela revisão, igual na base). A espera
bate de novo sozinha (`baterDeNovo`). Quem sai dela pela porta com uma batida
no ar deixa essa batida calada quando ela volta — de propósito: a espera foi
largada. Mas a bandeira `batendoDaEspera` era um sim, e calava **qualquer**
falha que chegasse com a batida no ar. Quem saía e apertava HOSPEDAR AQUI
tinha a recusa da entrada nova calada, sem tela e sem registro. Depois do W2,
só a tentativa mais nova chega a esse ramo, e era justamente ela que sumia.

**O conserto escrito** (sem rodar):
- `batendoDaEspera` guarda o número da tentativa da batida (ou `null`).
  `baterDeNovo` o lê de `tentativas` logo depois de chamar `conectar`, que
  conta a tentativa antes da primeira espera (o guarda da credencial cobra
  isso); sem alvo, `conectar` volta sem contar, e a bandeira fica `null`;
- `conectar` passa `esta` a `levarParaAEspera`, que só cala quando a falha é
  da própria batida (`batendoDaEspera === tentativa`), e então a registra com
  `registrarNoAnfitriao("espera", …, "aviso")`, com o endereço e o motivo;
- a falha de outra tentativa segue o caminho de sempre: a linha vermelha, ou a
  espera dela se for `AdmissionPending`.

**Mostrar na tela**: feito, porque é o mesmo número que o W2 já usa, e só
muda quem é calado. A batida largada continua calada na tela, que é o que a
espera prometia; a entrada nova passa a falar.

**A cena escrita** `aBatidaDaEsperaSoCalaASiMesma` (bloco próprio):
1. a sonda do revisor: a espera aberta por `AdmissionPending` em
   `192.0.2.19:8384`; `baterDeNovo()` com o servidor lento (1,5 s); aos
   100 ms `voltarParaAEntrada()`; aos 200 ms `hospedar()`, com o servidor daqui
   recusando `ServerFull`. Cobra que a linha vermelha diz SERVIDOR LOTADO; que
   a batida, voltando depois, não reabre a espera nem tira a frase da linha, e
   deixa uma linha (e uma só) no registro;
2. a batida sozinha: a pessoa sai da espera e não faz mais nada; a batida
   volta calada na tela e deixa no registro uma linha de aviso com o motivo.

**O que se espera e falta medir**: na ponta `525b1907`, a cena reprova no
caso 1 (a linha vermelha calada); com o código de `9a62a8f4`, fica verde. As
reversões R10 (a bandeira volta a ser um sim) e R11 (o ramo cala sem
registrar) estão planejadas e não rodaram.

### A linha de etapa: o que foi medido e o que falta (fora do lote)

O mesmo `finally` chama `mostrarEtapa(null)` para toda tentativa, e o da velha
apaga a linha de etapa da nova enquanto ela anda. **Isso foi lido no código, e
não medido**: a bancada não emite eventos de etapa (o `listen` dela é vazio).

Na primeira entrega eu disse que calar a velha ali deixaria a etapa dela na
tela quando ela termina depois da nova. Lendo o `main.rs` para escrever esta
seção, isso é verdade só em parte:

- os eventos `ConnectStageChanged` não dizem de qual tentativa são;
- mas o `connect` do `main.rs` revoga a geração anterior ao começar
  (`let geracao = session.revogar();`), e a `Bridge` de cada tentativa
  descarta todo evento cuja geração não é mais a de pé (`Bridge::on_event`,
  contado em `eventos_de_geracao_morta`). Então, **a partir do `connect` da
  nova**, as etapas da velha não chegam mais à janela;
- o que sobra é o trecho entre o começo da nova em `conectar`
  (`++tentativas`) e o `invoke("connect")` dela: o `apelido_local`, a pergunta
  do nome e o `conferir_esta_versao`. Nesse trecho a geração ainda é a da
  velha, e as etapas dela ainda chegam.

**A forma de consertar que sugiro**, nesta ordem:

1. gatear também o `mostrarEtapa(null)` com `esta === tentativas`; o preço é
   que, se a velha terminar dentro daquele trecho, a última etapa dela fica na
   linha até a nova dizer a primeira (o `connect` da nova começa a emitir logo,
   e o `finally` da nova apaga de qualquer jeito);
2. para fechar o trecho, a janela apagar a linha e passar a ignorar etapas
   assim que a nova começa, até o `connect` dela: por exemplo, uma bandeira
   «a etapa é de `esta`» posta em `++tentativas` e só reaberta logo antes do
   `invoke("connect")`, com o ouvinte de `ConnectStageChanged` (`tela-boot.js`,
   perto de :439) descartando o que chegar com ela fechada. Não toca no fio
   nem na FFI;
3. a alternativa no Rust é a geração ser revogada mais cedo — um comando para
   «começou uma tentativa» chamado em `++tentativas` —, que muda a ponte e
   pede outro lote.

O comentário do `finally` dizia o motivo antigo; `7cd7d98c` o corrige (só
comentário).

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
- **R5** (rodada da revisão: o `finally` volta a reacender o CONECTAR para
  toda tentativa): a cena reprova no passo 2 com «com a tentativa a
  192.0.2.18:8384 ainda andando, a anterior, a 192.0.2.17:8384, terminou e
  reacendeu o CONECTAR…»; o guarda reprova com «o `finally` do `connect`
  reacende um botão sem conferir que a tentativa ainda é a mais nova
  (["botao.disabled = false;"])…».
- **R5b** (o `finally` não devolve o botão a ninguém, para provar que a
  outra metade também é cobrada): a cena reprova com «a tentativa mais nova
  terminou e o CONECTAR ficou apagado: quem devolve o botão é ela»; o guarda,
  com «o `finally` do `connect` deixou de devolver o botão que a entrada
  apaga…».

### O que falta rodar (rodada 2, depois da pausa de medição de CPU)

Escrito durante a pausa, sem compilar nem rodar nada:

1. **R6 a R9**, as reversões do I1, já escritas no scratchpad, cada uma
   contra a `ajustes-v013` e o guarda `so_a_tentativa_mais_nova_mexe_no_conectar`,
   com `cmp` e o arquivo tocado:
   - **R6**: sem a desistência depois da pergunta do nome (esperado: o caso 1
     reprova pelo botão; o guarda, pelo apagar sem conferência);
   - **R7**: sem a desistência antes da pergunta (esperado: o caso 2 reprova
     pela caixa por cima da sessão; o guarda, pelo apagar da pergunta);
   - **R8**: o `finally` da pergunta volta a reacender sempre (esperado: o
     caso 3 reprova; o guarda, pelo acender sem conferência);
   - **R9**: as desistências sem o registro (esperado: a cena reprova pela
     linha que falta no `seele.log`; o guarda, pelo bloco sem
     `registrarNoAnfitriao(`).
2. **O vermelho do M1 antes do conserto**: a cena
   `aBatidaDaEsperaSoCalaASiMesma` com o `tela-auth.js` e o `tela-boot.js` de
   `525b1907` no lugar (a reversão do commit), e depois verde com os de
   `9a62a8f4`.
3. **R10 e R11**, as reversões do M1.
4. **A rodada inteira**: `--test frontend`, `--bin`, `check-runtime`, fmt,
   clippy do app, e as bancadas `ajustes-v013`, `conversa-com-estado` e
   `telas`.
5. Com tudo verde, o `9a62a8f4` deixa de ser «em curso» (um commit novo que o
   diga, sem reescrever o histórico), e este relatório ganha os resultados,
   nos dois lugares.

## 7. O que rodou

Na primeira entrega (`866d46a8`): `cargo test -p seele-app --bin seele-app`, 312 de 312;
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

Na rodada da revisão (`8f4f4724`, e de novo depois de `7cd7d98c`, que é só
comentário): `--test frontend`, 287 de 287; `--bin`, 312 de 312;
`check-runtime`, verde; fmt, limpo; clippy `-D warnings`, limpo (o mesmo aviso
do ligador); a `ajustes-v013`, verde.

## Preocupações

1. **A linha de etapa** fica fora, por decisão da revisão: o que foi medido, o
   que falta e a forma sugerida estão em 2, «A linha de etapa». A razão que eu
   tinha dado para deixá-la de fora era mais forte do que a leitura sustenta
   (a `Bridge` já corta as etapas da velha a partir do `connect` da nova), e
   está corrigida no comentário e aqui.
2. **O roteiro `tools/roteiros/aceite-de-mods.js` não exige nada**, e fica
   fora do lote, por decisão da revisão. O que vi, na base e aqui, rodando
   `python3 tools/carga-da-casca.py --roteiro tools/roteiros/aceite-de-mods.js`:
   `diálogo: VISIVEL`, `linha vermelha da entrada: escondida`,
   `gravou o sim: alvo=undefined conjunto=undefined`,
   `tentou entrar de novo: false`, `diálogo depois: VISIVEL`, e «carga limpa».
   O roteiro só chama `relatar`, nunca `exigir`, então ele nunca reprova — e o
   que ele relata é que ACEITAR não gravou o sim nem tentou entrar. A causa não
   medi; a suspeita é a ordem nova do lote W (`aceitarOsMods` obtém o conjunto
   antes de gravar, e o duble do roteiro não tem o MOD instalado nem o
   catálogo), o que deixaria a pergunta em TENTAR DE NOVO.
3. **O worktree.** As edições a este worktree vindas da sessão do controlador
   eram recusadas por um gancho («belongs to a different worktree»); entrei
   nele com `EnterWorktree` (só este agente), e foi daqui que tudo foi escrito.
   A cópia do relatório no worktree do controlador foi feita entrando nele do
   mesmo jeito.

Os comentários velhos (o de `tests/frontend.rs` perto de :543 e o doc de
`registrarNoAnfitriao`) e o CONECTAR, que eram preocupações da primeira
entrega, entraram nesta rodada.
