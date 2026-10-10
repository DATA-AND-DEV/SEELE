# Lote E54 da F5: relatório

Base `b101e385` (branch `0.16/lotes`). Branch `0.16/lotes-e54`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-acd7897834b5baf52`. Tudo
local: sem push, tag nem release. Sem conformidade. É a pendência 58, item 54:
a tentativa de entrada velha que **entra** depois da nova.

| commit | o que muda |
|---|---|
| `ecc3d9b5` | passo 1a: o `connect` e os auxiliares dele genéricos sobre `R: tauri::Runtime` (só assinatura; ver 1) |
| `05f0326c` | **vermelho**: a medida do Rust, `a_entrada_velha_que_chega_tarde` |
| `dbe9340e` | passo 2: as duas conferências do `connect`, `largar_a_substituida`, `FalhaDaEntrada.substituida` |
| `231fdff4` | o `command_parameters` de `tests/frontend.rs` volta a ler o `connect` genérico, e reprova o comando não lido |
| `cd8d9d32` | vermelho de `soltar_a_entrada` (o corpo devolve `false`) e o teste dele |
| `9b84a8f8` | o corpo de `soltar_a_entrada_em` |
| `dcfd5a6b` | a cena `aEntradaVelhaNaoTomaATela` (passos 1 e 2), com o `tela-boot.js` da base |
| `4e0d0ea6` | passo 3: a conferência da janela, `largarAEntrada`, o guarda do `frontend.rs`, a `ponte-web.js` |
| `7ef34a8a` | o guarda de forma das duas conferências do `connect` |
| `3a981b6a` | a cena ganha o passo 1b (a recusa «substituída» chega calada) e o 3 (a medida da batida) |
| `f6a17740` | o guarda identifica o slot por `snapshot().server`, para o teste ser o mesmo na 0.15 |
| `26e3f0d3` | a rodada depois da pausa: `confere_geracao` nas três conferências, e `body_of` e a âncora da `Bridge` no `frontend.rs` |
| `a138cbb0` | a batida de uma espera largada, admitida depois, não toca na tela (`aBatidaFoiLargada`), num commit separado |
| `efb1d356` | o `corpo_do_comando` de `tests/web.rs` volta a achar o `connect` genérico |
| `e3e1ed77` | o relatório da primeira entrega |
| `60e37628` | rodada 1 da revisão, I1: o teste de comportamento da segunda conferência (a sonda P2) |
| `e26cb67b` | rodada 1, I1: o guarda de forma exige que a substituída volte, e o doc dele deixa de dizer que nenhum teste para o `connect` ali |
| `bbb62528` | rodada 1, M1: o `connect` confere o slot e revoga com o cadeado dele na mão, com o teste da ordem forçada (a sonda P53) |
| este | o relatório da rodada 1 (ver 10) |

De `231fdff4` a `3a981b6a` e em `f6a17740` os commits foram escritos durante a
pausa de medição de CPU e commitados como «em curso, não rodado». O vermelho
dos dois que nasceram vermelhos (`cd8d9d32` e `dcfd5a6b`) foi medido depois,
pelas reversões R4 e R6 (ver 6): o mutante de R4 é o corpo de `cd8d9d32`, e o
de R6 é o caminho de sucesso da base.

**O que não mudou.** Nada em `crates/`, no fio
(`crates/seele-proto/src/{control,version,media,screen}.rs` intocados), no
servidor, na CSP, em `api/`. `disconnect`, `parar_de_hospedar`,
`meu_servidor` e `desmontar_o_cliente` não foram tocados. Nenhuma dependência
nova. Nenhum `.cjs` novo: a cena entrou num bloco próprio no fim da
`ajustes-v013.cjs`.

## 1. A medida do Rust

**O lugar: um teste do `main.rs`, e não uma conformidade.** O slot e o
`connect` só existem no binário do app; uma conformidade mediria a FFI, e o
defeito é de quem grava o slot. A conformidade também estava fechada.

**O `connect` de verdade, pelo runtime falso do Tauri.** O `connect` recebia
`AppHandle` (o `Wry`), que um teste não monta. O `ecc3d9b5` o tornou genérico
sobre `R: tauri::Runtime`, com o `Wry` como padrão. Mudou só a assinatura
destas funções do `main.rs` (para o encontro com a F7 e a F8):

| função | antes | depois |
|---|---|---|
| `struct Bridge` | `app: AppHandle` | `Bridge<R: tauri::Runtime = tauri::Wry>`, `app: AppHandle<R>` |
| `impl EventListener for Bridge` | — | `impl<R: tauri::Runtime> EventListener for Bridge<R>` |
| `config_dir` | `(app: &AppHandle)` | `<R: tauri::Runtime>(app: &AppHandle<R>)` |
| `connect` | `(app: AppHandle, …)` | `<R: tauri::Runtime>(app: AppHandle<R>, …)` |
| `caminho_do_retrato` | `(app: &AppHandle)` | `<R: tauri::Runtime>(app: &AppHandle<R>)` |
| `retrato_guardado` | `(app: &AppHandle)` | `<R: tauri::Runtime>(app: &AppHandle<R>)` |
| `preferencias` | `(app: &AppHandle)` | `<R: tauri::Runtime>(app: &AppHandle<R>)` |
| `caminho_dos_conhecidos` | `(app: &AppHandle)` | `<R: tauri::Runtime>(app: &AppHandle<R>)` |

Todo chamador de produção infere o mesmo `Wry` de antes.

**O que o genérico quebrou nos guardas, e eu só vi na rodada.** No `ecc3d9b5`
eu rodei `--bin`, fmt e clippy, e não o `--test frontend` nem o `--test web`.
Os guardas que procuram as assinaturas pelo texto pararam de achá-las:

- `command_parameters` (frontend.rs) lia o nome até o `(`, e o `connect`
  virava `connect<R: tauri::Runtime>`. `todo_argumento_que_o_frontend_manda_existe_no_comando`
  **pulava o `connect` calado** — este era cego, e não reprovava. Achado
  lendo, durante a pausa (`231fdff4`): o nome sai sem genéricos, e o guarda
  passa a reprovar quando um comando definido no `main.rs` não foi lido;
- `body_of(…, "async fn connect(")` (frontend.rs, oito guardas) e a âncora
  `"impl EventListener for Bridge"` (um guarda) reprovaram nove testes na
  rodada (`26e3f0d3`): `body_of` procura de novo com `<` no lugar do `(`
  final, e a âncora passou a `"EventListener for Bridge"`;
- `corpo_do_comando` (web.rs, da F4) reprovou
  `todo_comando_que_mexe_na_hospedagem_e_recusado_neutro_ou_atendido_com_motivo`
  (`efb1d356`): aceita também `\nasync fn {comando}<`.

Os últimos dez reprovaram, em vez de passarem cegos; o primeiro era o
«existir não é funcionar», e agora reprova (R9).

**Os servidores.** Dois `Daemon::bind` de verdade em `127.0.0.1:0`, banco em
memória. Nada de `Hospedagem::iniciar(`, que o guarda da F7 proíbe no
`main.rs`, nem da sala, que recusa uma `Connection` comum.

**A velha fica lenta sem relógio.** O aperto de mão do servidor pega o cadeado
do banco antes de responder (a portaria, em `session.rs`). O teste segura o
cadeado do servidor velho até a nova ter entrado, e só então o solta. A ordem é
a do cenário, e não a de um temporizador.

**A casa do app** é uma pasta temporária em `SEELE_HOME`, uma só para o
processo; os testes do módulo rodam um por vez (`vez()`), porque dividem a
identidade e os pinos.

### O que a medida mostrou (vermelho, `05f0326c`)

```
a velha devolveu: Ok(Entrada { geracao: 1 })
no slot: o servidor velho
geração de pé: 2 (a da nova é 2)
endereço da sessão: o do servidor velho
send_message devolveu: Ok(…)
a mensagem chegou a: ["o servidor velho"]      (MessagePosted no barramento do servidor)
a janela ouviu 5 evento(s); fins: []           (todos da nova; nenhum Ended)
eventos descartados por geração morta: 13      (os da conexão velha: a Bridge dela é da geração 1)
presentes depois: no velho 1, no novo 0        (a nova saiu do servidor dela, calada)
```

Igual em duas rodadas. Na janela de hoje, ela também seguia a velha
(`geracaoDaSessao` 1) contra o Rust na 2: comandos de MOD recusados como
geração morta, e todo evento da conexão viva descartado — a tela congelava.
Consertando só a janela, a tela mostraria o servidor novo e o slot falaria com
o velho: a mensagem, e os comandos de MOD (a geração bate), iriam ao servidor
que a pessoa deixou. Por isso a metade do Rust veio primeiro.

## 2. O conserto do Rust

**Duas conferências no `connect`, as duas por `confere_geracao`:**

1. logo depois do `connect_watching`, **antes de qualquer efeito de entrada**:
   as escolhas gravadas, o retrato, a semente da sonda e o consentimento de par
   não vão ao servidor que a pessoa deixou;
2. ao gravar o slot, **com o cadeado dele na mão**, sem espera entre conferir e
   gravar: ou a geração ainda é desta tentativa e o slot é dela, ou ela é
   largada antes do endereço da sessão, do alvo e da lista de visitados.

`confere_geracao`, e não `!geracao_vale`: o guarda
`nenhuma_geracao_morta_e_recusada_por_fora_de_recusar_se_morta` reprovou a
negação crua na rodada. A regra do arquivo é que a recusa por geração morta
conte em `comandos_de_geracao_morta`, e a velha que chega tarde é uma coisa da
sessão anterior falando depois da hora.

**`largar_a_substituida`** fecha a conexão (`disconnect`), escreve uma linha
WARN no `seele.log` com o alvo, a geração dela e a de pé, e devolve
`FalhaDaEntrada` com `substituida: true` (o erro é `NotConnected`, sem trilha).
A marca é de `FalhaDaEntrada`, que é deste arquivo: o `ConnectionError` do
`seele-ffi` e o fio não mudam. Fora do JSON quando falsa — toda outra recusa
chega à janela como chegava, e
`a_falha_leva_o_erro_a_trilha_e_a_origem_no_topo` cobra isso.

**O sair com um `connect` andando** fica coberto pela mesma geração revogada:
`desmontar_o_cliente` revoga, e o `connect` que termina depois é largado.

**E o começo do `connect`, com o slot na mão** (rodada 1 da revisão, M1,
`bbb62528`). O `AlreadyConnected` e a revogação eram duas linhas soltas, e a
velha podia gravar o slot entre elas: a nova passava com o slot vazio e
revogava depois. Se a nova falhasse, a velha ficava órfã no slot (ver 10). Agora
o `connect` confere o slot vazio e revoga com o cadeado do slot tomado, na mesma
ordem de cadeados de `soltar_a_entrada_em`: ou a velha grava antes e a nova
recusa, ou a nova revoga antes e a velha é largada na conferência dela.

### `soltar_a_entrada(geracao)` (decisão aceita pela orquestradora)

**A outra ordem da corrida, que o Rust sozinho não vê.** A nova começa na
janela (`++tentativas`) e ainda não chamou o `connect` — está no
`conferir_esta_versao`. A velha termina nesse meio, com a geração ainda de pé,
e entra. A janela não a mostra (passo 3); sem soltá-la, a conexão dela fica no
slot sem tela, e todo `connect` seguinte volta `AlreadyConnected` até o app
fechar. **Medido** (R7, com uma cópia da cena que não parava no passo 1): a
pessoa fica na entrada com «JÁ HÁ UMA SESSÃO ABERTA» na linha vermelha, e o
slot com o servidor que ela deixou.

`soltar_a_entrada_em`: com o slot na mão, se a geração dada ainda é a de pé e
há conexão, revoga (ali mesmo, para um `connect` novo que chegue no mesmo
instante ficar com a geração seguinte) e tira a conexão; fora do cadeado, a
fecha, esquece o endereço da sessão e escreve WARN. Se não, não toca em nada e
diz em INFO. **O convite fica**: ele pode ser da tentativa nova, que ainda vai
lê-lo — é a diferença para `desmontar_o_cliente`. O pedido com uma geração que
já não é a de pé passa por `confere_geracao`, e é contado.

A revogação com o slot na mão toma, dentro dele, os cadeados de
`Session::revogar` (`mods_nativos`, `arquivos_escolhidos`, `recusas_de_carga`,
`baldes_do_registro`). Conferi, lendo, que nenhum caminho toma um deles e
depois o do slot. Lido, e não provado por teste.

### As provas do Rust

- `a_velha_que_entra_depois_da_nova_nao_toma_a_conexao`: a sequência da
  medida, com o `seele.log` capturado. Cobra a recusa marcada como
  substituída, o slot com a nova (pelo nome do servidor), a geração de pé, o
  endereço da sessão, a mensagem só ao servidor novo, a conexão da velha fora
  do servidor dela, a pessoa presente no novo, **o retrato desta máquina não
  mandado ao servidor velho** (o efeito de entrada que o servidor vê) e uma
  linha WARN;
- `a_entrada_que_a_janela_larga_e_solta_pela_geracao_dela`: a outra ordem. A
  velha entra; a janela a larga; ela sai do slot e do servidor dela, a geração
  dela cai, o endereço é esquecido, a nova entra em vez de receber
  `AlreadyConnected`, a mensagem vai à nova, um pedido com a geração velha não
  derruba a nova, e uma linha WARN só para o que soltou;
- `a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot`
  (rodada 1, I1, `60e37628`): a segunda conferência pelo comportamento, na
  forma da sonda P2 do revisor. O teste para o `connect` da velha no cadeado
  do slot, que ele segura numa thread própria; espera o retrato da velha
  chegar ao servidor velho (ela passou da primeira conferência e aplicou os
  efeitos); revoga a geração, como o `connect` da nova faz ao começar; e só
  então solta o slot. Cobra a recusa marcada, o slot vazio, o endereço da
  sessão intocado e a velha fora do servidor velho. **A primeira entrega dizia
  que nenhum teste daqui conseguia parar o `connect` entre as duas
  conferências, e não era verdade**: o cadeado do slot é a costura, e a sonda
  P2 da revisão a achou;
- `a_nova_que_falha_no_meio_da_velha_nao_a_deixa_orfa_no_slot` (rodada 1, M1,
  `bbb62528`): a ordem forçada da sonda P53 (ver 10);
- `o_connect_confere_a_geracao_antes_dos_efeitos_e_ao_gravar_o_slot`: a forma
  que dá aquele comportamento, sem ler comentário. Desde a rodada 1
  (`e26cb67b`) ele exige `return Err(largar_a_substituida(` nas duas
  conferências: só a ordem do texto deixava passar o mutante R3b da revisão,
  que largava sem voltar.

## 3. O conserto da janela

**Num lugar só**, como a orquestradora pediu: em `conectar`, logo depois de o
`invoke("connect")` resolver e antes do primeiro passo do sucesso,

```js
if (esta !== tentativas || aBatidaFoiLargada(esta)) {
  largarAEntrada(alvo, resposta?.geracao, esta !== tentativas ? "outra a substituiu" : "a pessoa saiu da espera");
  return;
}
```

Nada da geração, do desenho nem da autenticação vale para ela.
**`largarAEntrada(alvo, geracao, porque)`**, função própria logo acima de
`conectar`, registra por `registrarNoAnfitriao("entrada", …, "aviso")` com o
alvo, a geração e a razão, **sem a credencial**, e pede `soltar_a_entrada`
com a geração.

A recusa marcada como substituída chega pelo `catch` e segue o ramo do W2
(`esta !== tentativas`), que já a cala e registra: o `catch` não mudou.

**As linhas, para o encontro com a F8** (numeração do HEAD do ramo):
- `tela-boot.js`: `largarAEntrada` e o doc dela, linhas 181–219 (novas, entre
  `deOndeVeio` e `conectar`); a conferência em `conectar`, linhas 398–405
  (novas, entre o `invoke("connect", …)` e o
  `const { snapshot, veredito, geracao } = resposta;`). Quando a F8 extrair
  `depoisDeConectar(resposta, alvo)`, as linhas 398–405 ficam logo antes da
  chamada a ele;
- `tela-auth.js` (só no `a138cbb0`): `aBatidaFoiLargada` e o doc dela, linhas
  689–703 (novas, depois de `let batendoDaEspera = null;`); a linha 572 de
  `levarParaAEspera` passa a `if (aBatidaFoiLargada(tentativa)) {`. O
  `entrarNaAutenticacao` não foi tocado;
- `ponte-web.js` (da F4): `soltar_a_entrada` nos atendidos e em
  `AINDA_A_FAZER`, com a tarefa 3.4, como o `connect`. A orquestradora avisa a
  F4.

### As provas da janela

**A cena** `aEntradaVelhaNaoTomaATela` (bloco próprio na `ajustes-v013.cjs`):

1. a sequência que a revisão do W2 mediu: a velha a 192.0.2.17 leva 1,2 s e
   entra; aos 100 ms o `hospedar()` entra no servidor daqui. A tela (entrada,
   sessão, autenticação, linha vermelha e `geracaoDaSessao`) depois de a velha
   voltar é a mesma de antes; a janela pede `soltar_a_entrada` com a geração da
   velha, e só ela; uma linha de aviso;
2. a outra ordem, com o slot do Rust modelado na ponte de mentira: a velha
   entra antes de a nova chegar ao `connect` (a nova espera 600 ms no
   `conferir_esta_versao`). A nova fica com a sessão, sem linha vermelha, e a
   velha foi solta pela geração dela;
   - 1b: a mesma corrida do passo 1 com a geração do Rust modelada: a velha
     volta como `largar_a_substituida` a devolve, e cai calada no ramo do W2 —
     a tela não muda, nada é pedido a `soltar_a_entrada`, uma linha de aviso;
3. a batida da espera (ver 4);
- o convite da velha não vai a comando nenhum além do `connect`.

**O guarda** `a_entrada_que_outra_substituiu_nao_toca_na_tela_e_e_solta`
(`tests/frontend.rs`), sem ler comentário: a conferência é a primeira decisão
do sucesso, sem espera antes; o bloco dela não faz nada do sucesso
(`entrarNaGeracao(`, `desenhar(`, `entrarNaAutenticacao(`, e o
`depoisDeConectar(` da F8), volta, e larga pela geração; o sucesso continua
depois; `largarAEntrada` registra uma vez, como aviso, com o alvo, pede
`soltar_a_entrada` com a geração, e não nomeia a credencial.

## 4. A batida da espera

**Medida** (passo 3 da cena, vermelho com o conserto de `4e0d0ea6`): a espera
abre numa batida pendente na portaria; quem hospeda admite; a batida seguinte
leva 600 ms, e aos 100 ms a pessoa sai da espera pela porta. Quando a batida
volta admitida:

```
antes:  {"entrada":true,"sessao":false,"autenticacao":null,"linha":null,"geracao":0}
depois: {"entrada":false,"sessao":false,"autenticacao":"aperto 192.0.2.19:8384","linha":null,"geracao":8}
soltar: []   registro: []
```

`#tela-auth` reabre em aperto de mão por cima da entrada que a pessoa
escolheu. **O conserto do brief não a cobre**: sair da espera não começa outra
tentativa, e a batida continua sendo a mais nova. Igual na base pela leitura —
o caminho de sucesso da tentativa mais nova não mudou.

**O conserto, separado** (`a138cbb0`), como a orquestradora decidiu:
`aBatidaFoiLargada(tentativa)` (tela-auth.js) é a condição que o W2 já usava
para calar a falha da batida largada em `levarParaAEspera`, numa função e
usada nos dois lugares; `conectar` larga a entrada também com ela. Tirar o
commit volta ao conserto do brief, sem a batida. Verde depois dele; R10 é a
reversão.

## 5. O que fica de fora, lido e não medido

1. **A nova chega ao Rust antes de a janela soltar a velha.** Na ordem do
   passo 2, se o `connect` da nova chega ao Rust entre a velha gravar o slot e
   a janela pedir para soltá-la, a nova recebe `AlreadyConnected` («JÁ HÁ UMA
   SESSÃO ABERTA»). A velha é solta logo depois, e a tentativa seguinte entra.
   A janela é de milissegundos (a volta do `connect` da velha até o pedido).
2. **O `hospedar()` antes de contar a tentativa.** O `hospedar()` sobe o
   servidor (`invoke("hospedar")`) antes de chamar `conectar`, e só ali a
   tentativa é contada. Uma velha que termina nesse meio ainda é a mais nova:
   entra, e a entrada do `hospedar` recebe `AlreadyConnected` em seguida. A
   tela bate com o slot (os dois são da velha). Anterior a este lote, e fora
   dele.
3. **A corrida residual no Rust — fechada na rodada 1 da revisão.** Na
   primeira entrega este item dizia que uma nova que passava pelo
   `AlreadyConnected` antes de a velha gravar o slot, e só revogava depois,
   gravava por cima da velha ao terminar. Só considerava a nova que **entra**.
   A revisão mediu a que **falha** (sonda P53): a velha ficava órfã no slot.
   Desde `bbb62528` o `connect` confere o slot e revoga com o cadeado dele, e
   essa ordem deixou de existir (ver 10).
4. **A linha de etapa** do W2 (o `mostrarEtapa(null)` do `finally`) não
   mudou.

## 6. As reversões

Cada uma por um script no scratchpad (`e54/reverter.py`, num diretório só
deste lote): o mutante aplicado, o arquivo tocado, a prova rodada, o arquivo
devolvido da cópia, tocado de novo, e conferido com `cmp` contra a cópia e
contra `git show HEAD:`. **Todas voltaram idênticas**, e o `git status` ficou
limpo depois de cada uma.

- **R1** (`main.rs`, o `connect` sem as duas conferências — o de antes do
  lote): `a_velha_que_entra_depois_da_nova_nao_toma_a_conexao` reprova com os
  oito pontos («a tentativa velha voltou como entrada (geração 1)…», «o slot da
  sessão ficou com o servidor velho…», «a mensagem mandada em seguida chegou a
  ["o servidor velho"]…», «…mandou o retrato ao servidor que a pessoa
  deixou…», «o seele.log não tem uma linha WARN…»); o guarda de forma, com
  «o `connect` aplica os efeitos de entrada sem conferir antes…».
- **R2** (sem a primeira): o cenário reprova só com «a tentativa velha mandou
  o retrato ao servidor que a pessoa deixou (1 vez(es)): os efeitos de entrada
  valeram para ela» — a segunda conferência pega o resto —; o guarda de forma,
  com a mesma frase de R1.
- **R3** (sem a segunda): o cenário **passa** (a primeira basta para ele), e o
  guarda de forma reprova com «o `connect` grava o slot sem conferir a geração
  com o cadeado na mão…». É o motivo de o guarda de forma existir.
- **R4** (`soltar_a_entrada_em` não solta nada, como em `cd8d9d32`): o teste
  dele reprova com sete pontos, entre eles «a tentativa nova não entrou depois
  de a janela largar a velha: {"error":"AlreadyConnected",…}».
- **R5** (soltar sem conferir a geração): «um pedido com a geração da velha,
  que já não era a de pé, soltou a entrada nova», «o pedido com a geração
  velha revogou a geração da nova», «a mensagem não saiu: NotConnected».
- **R6** (`tela-boot.js` sem a conferência — o sucesso da base): a cena
  reprova no passo 1 com «…tomou a tela da sessão: antes
  {…"autenticacao":null,…"geracao":2}, depois {…"autenticacao":"aperto
  192.0.2.17:8384",…"geracao":1}» — o defeito que a revisão do W2 mediu; o
  guarda, com «o caminho de sucesso de `conectar` não confere…».
- **R7** (larga sem pedir para soltar): a cena reprova no passo 1 com «a
  janela não pediu ao Rust que soltasse a sessão da tentativa largada (geração
  1)…»; numa cópia da cena sem essa asserção (apagada logo depois), o passo 2
  reprova com «…a entrada nova não ficou com a sessão:
  {"tela":{"entrada":true,…"linha":"JÁ HÁ UMA SESSÃO ABERTA"…},"slot":{"server":"192.0.2.17:8384",…}}»;
  o guarda, com «`largarAEntrada` não pede ao Rust que solte a sessão largada».
- **R8** (larga sem registrar): a cena, com «a entrada largada não deixou uma
  linha (e uma só) no seele.log como aviso: []»; o guarda, com
  «`largarAEntrada` não escreve a entrada largada no registro, uma vez (0
  chamada(s))…».
- **R9** (`frontend.rs`, o `command_parameters` de antes): o próprio guarda
  reprova com «`connect` está em main.rs, e este guarda não leu os parâmetros
  dele: o que o frontend manda a ele deixou de ser conferido».
- **R10** (`tela-boot.js`, a conferência sem `aBatidaFoiLargada`): a cena
  reprova no passo 3 com a medida da seção 4.

As da rodada 1 da revisão estão em 10.

## 7. O que rodou

Na primeira entrega, depois da pausa de medição de CPU, no HEAD `efb1d356` (a
rodada 1 da revisão está em 10):

- `cargo test -p seele-app` (todos os alvos, sem `--workspace`): `--bin` 342
  de 342 (339 na base, mais os três do módulo do E54); `frontend` 294 de 294
  (293 na base, mais o guarda da janela); `web` 12, `celular` 8,
  `empacotamento` 22, `encerramento` 3, `fontes` 5, `manifesto_hibrido` 11,
  `marca` 9, `permissoes` 4, `tokens` 3. O módulo do E54, três vezes seguidas,
  verde;
- `cargo xtask check-runtime`: saída 0;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-app --all-targets -- -D warnings`: limpo (o único
  aviso é o do ligador, `duplicate -rpath '/usr/lib/swift'`, que já estava na
  base);
- as bancadas `ajustes-v013` (com as duas linhas do E54), `conversa-com-estado`
  e `telas`: saída 0. A `telas` imprime o `TypeError … 'supressao'` em
  `desenharQualidadeDaVoz` e «Não foi possível conferir os MODs: Error:
  disconnected», que o relatório do W2 registra como iguais aos da base; não
  rodei a `telas` na base de novo;
- e, porque a `ponte-web.js` mudou, a bancada `ponte-web.cjs`: «as 20 provas
  passam».

Nenhum passo falhou por tempo, com a bateria da orquestradora rodando ao mesmo
tempo. Nenhuma conformidade, nenhum `cargo test --workspace`.

## 8. O Windows

Os testes novos do `main.rs` sobem o runtime falso do Tauri dentro do binário
de testes do app, sem `cfg`. O CI roda `cargo test --workspace` no
`windows-2022`, e isso inclui esse binário. **Não foi medido no Windows.** O
que se sabe: a pendência 7 registra a medida de 25/08/2026, numa máquina
Windows de verdade, em que só o `seele-app --test permissoes` falhou
(`STATUS_ENTRYPOINT_NOT_FOUND`), e o binário de testes do app rodou; o runtime
falso é Rust puro, sem webview. O último run do CI (22/09) está vermelho nos
três sistemas, e os logs dele pedem autenticação. O risco é este: se o
problema da DLL valer para o binário do app quando ele monta o app falso, os
344 testes dele deixam de rodar no Windows, e o remédio é a nota e o `cfg` do
`tests/permissoes.rs` no módulo `a_entrada_velha_que_chega_tarde`. A revisão
leu um dado que diminui o risco: o binário de testes do app já sobe o runtime
falso sem `cfg` desde `7546c9e8` (F7, `sala.rs` :657), que está na base. O E54
não abre classe nova de risco.

## 9. O porte para a 0.15 (`6702b944`), lido e não compilado

**O genérico (`ecc3d9b5`) cabe como está.** As mesmas sete funções existem lá
com `&AppHandle`, e o `connect` da 0.15 chama exatamente `config_dir`,
`preferencias`, `retrato_guardado` e `caminho_dos_conhecidos`, e a `Bridge`.
O Tauri é o mesmo 2.11.5, com `features = ["test"]` nos dev-deps
(`mock_builder`, `mock_context`, `noop_assets`). `tempfile`,
`rastro_de_teste::capturar`, `Daemon::bind`, `server().persistence`,
`events`, `presentes`, `Event::MessagePosted` e `PersonIconChanged` existem; o
cadeado do banco é pego no aperto de mão. **Com:**

- os consertos dos guardas que liam as assinaturas pelo texto
  (`command_parameters` — o da 0.15, em :163, é o mesmo —, `body_of`, a âncora
  da `Bridge`, `corpo_do_comando`), conferidos lá contra os guardas que a 0.15
  tiver;
- o teste já identifica o slot por `snapshot().server` (`f6a17740`), porque
  `Connection::impressao_do_servidor` não existe na FFI da 0.15;
- o `tokio` do app na 0.15 declara só `["net", "rt"]`, e o teste usa `sync`
  (`broadcast`) e `time`: o porte declara os dois. Na 0.16 eles já estão
  declarados nas dependências do app (pelo toque da sala pessoal), e não há o
  que acrescentar. Os testes da rodada 1 não pedem `rt-multi-thread`: o
  `connect` parado num cadeado roda numa thread própria, com runtime de uma
  thread só;
- **o teste `a_falha_leva_o_erro_a_trilha_e_a_origem_no_topo` da 0.15**
  (:10513) monta `FalhaDaEntrada` à mão, e tem de ganhar o campo `substituida`
  (lido pela revisão).

**A metade do Rust porta inteira** (as duas conferências, `largar_a_substituida`,
`substituida`, e o começo do `connect` com o slot na mão, da rodada 1), e é ela
que impede a mensagem de ir ao servidor errado. Os testes dela vão juntos:
`a_velha_que_entra_depois_da_nova_nao_toma_a_conexao`,
`a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot` e o
guarda de forma, que lá seria a única proteção da conferência sem o teste de
comportamento. O teste do M1
(`a_nova_que_falha_no_meio_da_velha_nao_a_deixa_orfa_no_slot`) imita a janela
chamando `soltar_a_entrada_em`, que a 0.15 não tem: na 0.15 a janela desenha a
velha que entra, e a tela bate com o slot, então o conserto do M1 porta, mas o
teste dele pediria outra asserção (a velha recusada como substituída). Fica
para a orquestradora decidir.

**A metade da janela não porta como está.** A 0.15 não tem o contador
`tentativas` (lote W). A prova menor que cabe lá, para a orquestradora
decidir: o `catch` de `conectar` trata `falha?.substituida === true` como
calada (registra, e não escreve a linha vermelha); sem `soltar_a_entrada` —
na 0.15 a velha que entra antes de a nova chegar ao Rust continua entrando,
como hoje, e a tela bate com o slot. Não escrevi o porte.

## 10. Rodada 1 da revisão

O veredito (`lote-e54-review-report.md`) pediu conserto: 1 Importante e 4
Menores. Nesta rodada entraram o I1 e o M1; o M2 e o M4 vão para a pendência
58; o M3 é processo, e já está no começo deste relatório.

### I1 · a segunda conferência do `connect` ganha prova de comportamento

**O achado.** O guarda de forma só conferia a ordem do texto
(`largar_a_substituida(` antes de `endereco_da_sessao`). O mutante R3b do
revisor mantinha a segunda conferência e trocava
`return Err(largar_a_substituida(…))` por `let _ = largar_a_substituida(…);`:
a conexão era fechada, mas o `connect` seguia gravando o endereço da sessão do
servidor deixado, o alvo e a lista, e voltava `Ok`. `cargo test -p seele-app`
passava inteiro. E este relatório e o doc do guarda diziam que nenhum teste
conseguia parar o `connect` entre as duas conferências: a sonda P2 do revisor o
parou no cadeado do slot.

**O conserto:**
- `60e37628`: o teste
  `a_velha_que_a_nova_substitui_entre_as_duas_conferencias_nao_grava_o_slot`,
  na forma da P2 (ver 2, «As provas do Rust»). Sem o runtime de várias threads
  da sonda: o app declara só `rt`, e o `connect` parado no cadeado roda numa
  thread própria (`connect_noutra_thread`), com o cadeado segurado por outra
  (`segurar`) — nenhuma guarda atravessa um `await`. Verde no HEAD em 0,4 s;
- `e26cb67b`: o guarda de forma exige `return Err(largar_a_substituida(` antes
  do primeiro efeito de entrada e dentro do ramo
  `if let Some(connection) = substituida`, e o ramo antes do endereço da
  sessão. O doc dele aponta o teste de comportamento e diz por que mudou; a
  frase da seção 2 deste relatório também.

**As reversões** (o mesmo script, o arquivo tocado, `cmp` contra a cópia e
contra `git show HEAD:`, `git status` limpo — todas voltaram idênticas):
- **R3** (sem a segunda conferência): o teste novo reprova com «a velha voltou
  como entrada (geração 1) depois de a nova revogar a geração dela no meio dos
  efeitos de entrada», «o slot ficou com a conexão da velha (Velho)», «o
  endereço da sessão ficou Some("127.0.0.1:…"), o do servidor que a pessoa
  deixou», «a velha continua presente no servidor velho (1)»; o guarda de
  forma, com «o `connect` grava o slot sem conferir a geração com o cadeado na
  mão…». O cenário da primeira conferência continua passando, como antes.
- **R3b** (o mutante do revisor), antes do guarda endurecido: o teste novo
  reprova com «a velha voltou como entrada (geração 1)…» e «o endereço da
  sessão ficou Some(…), o do servidor que a pessoa deixou»; **o guarda de forma
  passava**. Depois de `e26cb67b`: os dois reprovam, o guarda com «depois de
  gravar o slot, a tentativa que não gravou não volta largada antes de o
  endereço da sessão ser escrito…».

### M1 · a nova que falha no meio da velha não a deixa órfã no slot

**O achado** (sonda P53 do revisor). O começo do `connect` eram duas linhas
soltas: `connection().is_ok()` e `revogar()`. Uma nova passava pelo
`AlreadyConnected` com o slot vazio e revogava depois de a velha já ter
gravado o slot com a geração dela de pé. Se a nova falhasse, a janela largava
a velha, o pedido de soltá-la chegava com uma geração que já não era a de pé, e
a velha ficava no slot, presente no servidor deixado; toda entrada seguinte
recebia `AlreadyConnected` até o app fechar. A janela natural é de
submicrossegundos, salvo disputa por `mods_nativos`.

**O vermelho** (antes do conserto, no HEAD `e26cb67b`), pelo teste
`a_nova_que_falha_no_meio_da_velha_nao_a_deixa_orfa_no_slot`, com a ordem
forçada (o teste segura `mods_nativos` para a nova parar antes de revogar; a
nova falha na hora, numa porta 99999 que o `separar` recusa sem rede — a sonda
levava 23 s esperando um `SemResposta`, o teste leva 6):

```
a velha ficou órfã no slot (Velho): a janela a largou e o Rust não a soltou
a velha continua presente no servidor que a pessoa deixou (1)
a entrada seguinte não entrou: {"error":"AlreadyConnected","trail":[],…}
```

**O conserto** (`bbb62528`): o `connect` confere o slot vazio e revoga com o
cadeado do slot tomado, na mesma ordem de cadeados de `soltar_a_entrada_em` (o
do slot, e dentro dele os de `Session::revogar`). Ou a velha grava antes e a
nova recusa, ou a nova revoga antes e a velha é largada na conferência dela.
Verde: o módulo do E54, 5 de 5 — **numa rodada só, e por sorte de ordem**: o
teste do M1 não montava a casa nem gravava o retrato, e só passava quando
outro teste do módulo o gravava antes (ver 11, R1-I2).

**A reversão R11** (o começo do `connect` da base): o teste do M1 reprova com
os três pontos acima, e volta idêntica. E **R1**, rodada de novo com os testes
novos: reprova os quatro testes do `connect` (os dois cenários, o teste do M1
e o guarda de forma), e o da outra ordem, que não depende do `connect`, passa.

### M2 · fora do lote, para a pendência 58

**A recusa `AlreadyConnected` da tentativa nova mente na tela e não vai ao
`seele.log`.** É o 5.1 deste relatório, medido pelo revisor no modelo da
bancada (sonda P9: o slot modelado, 200 ms de latência no `soltar_a_entrada`
antes de esvaziar o slot, a velha a 300 ms, a nova com 250 ms no
`conferir_esta_versao`):

```
tela: {"entrada":true,"sessao":false,"autenticacao":null,"linha":"JÁ HÁ UMA SESSÃO ABERTA","geracao":0}
slot: null   CONECTAR: aceso
chamadas: connect(192.0.2.17) · soltar_a_entrada(5) → true · connect(127.0.0.1:8383)
registro: só a linha da largada
```

**A leitura.** O `catch` de `conectar` leva a recusa da tentativa mais nova à
linha vermelha sem registrá-la. A frase diz que há uma sessão aberta, e não há:
a janela acabou de largar a velha e não junta as duas coisas. A próxima
tentativa entra. A janela de tempo vai da velha gravar o slot até o pedido de
soltar chegar (a escrita dos conhecidos e a volta do IPC: milissegundos). O
conserto provável: com uma largada pedida no ar, um `AlreadyConnected` da mais
nova espera o `soltar_a_entrada` voltar e tenta de novo, ou ao menos vai ao
registro com a frase certa.

### M4 · fora do lote, para a pendência 58

**`session.alvo` nunca é esquecido, e `soltar_a_entrada` herda isso de
`desmontar_o_cliente`.** Lido, e não medido.

- o `alvo` só é escrito pelo `connect` (`main.rs` :1521), e só para endereço
  que não é desta máquina; nenhum caminho o limpa — nem `desmontar_o_cliente`,
  nem `soltar_a_entrada_em`;
- é lido por `lembrar_aparencia_do_servidor` (:10530), que a janela chama ao
  entrar em qualquer servidor (`tela-auth.js` :490), e por
  `desmontar_o_cliente` (:3815), que anota nele a medida do caminho ao sair.

**O efeito, pela leitura:** depois de uma sessão remota (solta ou encerrada),
uma sessão hospedada aqui escreve o nome e o distintivo do servidor local na
entrada do servidor remoto da lista de conhecidos, e, ao sair, a medida do
caminho também vai para ele. É exatamente o cenário do W2 (192.0.2.17, e
depois HOSPEDAR AQUI) quando a velha entra antes de a nova chegar ao Rust; e
já existia na base, com «visitar X, sair, hospedar».

### O que rodou na rodada 1

No HEAD `bbb62528`:
- `cargo test -p seele-app` (todos os alvos, sem `--workspace`): `--bin` 344
  de 344 (339 na base, mais os cinco do módulo do E54) **numa rodada, e esse
  número não se reproduzia**: a re-revisão mediu 343/344 em 5 de 6 rodadas,
  pela ordem dos testes (ver 11, R1-I2), e as minhas rodadas desta seção
  foram com a HOME de verdade; `frontend` 294,
  `web` 12, `celular` 8, `empacotamento` 22, `encerramento` 3, `fontes` 5,
  `manifesto_hibrido` 11, `marca` 9, `permissoes` 4, `tokens` 3;
- `cargo xtask check-runtime`: saída 0;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-app --all-targets -- -D warnings`: limpo (só o aviso
  do ligador, da base);
- as bancadas `ajustes-v013` (com as duas linhas do E54), `conversa-com-estado`,
  `telas` e `ponte-web` («as 20 provas passam»): saída 0.

Nada falhou por tempo. Nenhuma conformidade, nenhum `--workspace`. Os commits
desta rodada foram feitos verdes, cada um depois de rodado; o vermelho do M1 e
os das reversões estão acima.

## 11. A re-revisão da rodada 1

Dois achados, os dois no teste novo do M1. O I1 e o M1 estão fechados no
código. Conserto em `b5809c33`.

### R1-I2 (Importante) · o teste do M1 não montava a casa nem gravava o retrato

`a_nova_falha_no_meio_da_velha` não chamava `casa()` nem
`guardar_um_retrato()`, e os outros testes de comportamento do módulo chamam.
Duas consequências:

- **O `--bin` dependia da ordem.** O teste usa o retrato para saber que a
  velha passou da primeira conferência. Sem gravá-lo, ele só passava quando
  outro teste do módulo o gravava antes: a re-revisão mediu 343/344 em 5 de 6
  rodadas, com «o retrato da velha não chegou ao servidor velho». O «5 de 5» e
  o «344 de 344» da seção 10 eram uma rodada de sorte.
- **Rodado sozinho, ou antes de qualquer teste do módulo que montasse a casa,
  ele usava a casa de quem roda a bateria.** Sem `SEELE_HOME`, o `config_dir`
  cai em `$HOME/.config/seele`.

**O conserto:**
- `app()` chama `casa()`, e nenhum teste do módulo pode esquecer;
- `app()` confere, antes de qualquer `connect`, que o `config_dir` do app
  falso é a casa temporária, e reprova se não for;
- o teste do M1 grava o retrato.

### R1-M5 (Menor) · o teste do M1 não conferia que a ordem foi a forçada

`da_nova.is_err()` aceitava `AlreadyConnected`. Se a nova chegasse ao slot
depois de a velha gravá-lo, a janela soltaria a velha pela geração ainda de pé,
e a R11 passaria calada. Agora o teste cobra, nesta ordem:
- a nova falha, e **não** por `AlreadyConnected`;
- a velha volta recusada como substituída (`substituida: true`);
- o slot fica vazio sem pedido nenhum da janela;
- a velha some do servidor velho;
- a entrada seguinte entra.

A imitação da janela (soltar a velha que voltou entrando) fica só para o
vermelho dizer a consequência. São as asserções que o revisor descreveu para
o porte (ver 9), e valem igual na 0.15.

### A HOME de verdade: quais rodadas minhas a usaram

**Sim, houve rodadas minhas com a HOME de verdade.** O teste do M1 nasceu por
volta das 20:25, depois de `e26cb67b` (20:24:38). Desde então, toda rodada
minha em que ele rodou antes de algum teste que monta a casa usou
`$HOME/.config/seele`. Os horários saem dos commits e das datas dos meus
arquivos no scratchpad:

1. **~20:25, certa.** O vermelho do M1, com o teste sozinho
   (`cargo test -p seele-app --bin seele-app a_nova_que_falha_no_meio_da_velha`).
   Ninguém chamou `casa()` nesse processo. Ele reprovou pela velha órfã, e
   não por falta de retrato: o `retrato.png` da casa real foi lido e mandado ao
   servidor de teste. Isso é inferência minha, porque não li a pasta.
2. **~20:26, provável.** O módulo inteiro depois do conserto do M1
   (`a_entrada_velha_que_chega_tarde`, 5 testes), e o `cargo test -p seele-app`
   que veio logo em seguida, antes de `msg-m1.txt` (20:27:06) e do commit
   `bbb62528` (20:27:09). Em cada um, o teste do M1 usou a HOME real se pegou
   o `vez()` antes de um teste que monta a casa. O `mtime` 20:26:36 do `pins`
   real bate com um destes dois.
3. **20:27–20:28, R11 e R1** (o módulo inteiro). Poderiam ter usado, mas o
   `mtime` do `pins` é anterior a elas. Pela data, não escreveram nada.

Antes das 20:25 nenhum teste meu deixava de montar a casa. Desde `b5809c33`,
toda rodada minha passa por `e54/cargo-falso.sh`: uma HOME nova por chamada,
sem `SEELE_HOME` nem `XDG_CONFIG_HOME`.

**O que essas rodadas fizeram na pasta real**, pela leitura do código e sem
ler os arquivos:
- `identity.key`: lida (`identity::load_or_create` só cria se não existir);
- `pins`: ganhou entradas TOFU de `127.0.0.1:<porta efêmera>`. São duas por
  rodada do teste do M1: o servidor velho e o novo, da entrada seguinte. A
  tentativa à porta 99999 falha no `resolve`, antes da identidade e dos pinos.
  As quatro entradas que o revisor contou batem com duas rodadas;
- `retrato.png`: lido e mandado ao servidor de teste, em `127.0.0.1`;
- `preferences` e `conhecidos`: só lidos (`open` e `abrir` criam a pasta, e
  não o arquivo), e o `connect` não grava a lista para endereço desta
  máquina;
- nada saiu de `127.0.0.1`. As contas «velha» e «seguinte», com a chave real,
  ficaram em servidores em memória que morreram com o teste.

Não li nem toquei `~/.config/seele`.

### As provas

Todas com HOME falsa:
- o `--bin` seis vezes: 344 de 344 nas seis;
- o teste do M1 sozinho (`--exact`) três vezes: verde nas três;
- nenhuma das HOMEs falsas ganhou `.config/seele`.

### As reversões

O mesmo script (cópia, mutante, toque, prova, devolução, toque, `cmp` contra
a cópia e contra `git show HEAD:`, `git status`). Todas voltaram idênticas, e
todas rodaram com HOME falsa.

- **R11** (o começo do `connect` da base): o teste do M1 reprova com «a velha
  voltou como entrada (geração 1) com a geração da nova de pé», «a velha ficou
  no slot (Velho) sem a janela pedir nada», «a velha ficou órfã no slot (Velho):
  a janela a largou e o Rust não a soltou», «a velha continua presente no
  servidor que a pessoa deixou (1)», «a entrada seguinte não entrou:
  AlreadyConnected». **Sem** «a nova recebeu `AlreadyConnected`»: a ordem foi a
  forçada. Os outros quatro passam.
- **R11b** (o mutante do revisor, que confere com o cadeado e o solta antes de
  revogar): reprova igual, e também sem a linha do `AlreadyConnected` da nova.
  O teste prova que a revogação tem de ficar dentro do cadeado.
- **R12** (a guarda da casa: `casa()` deixa de pôr o `SEELE_HOME`): os quatro
  testes que usam `app()` reprovam na conferência da casa, antes de qualquer
  `connect`, com «a casa do app falso não é a temporária deste módulo: um
  `connect` daqui escreveria a identidade e os pinos em
  …/home-falsa.…/.config/seele». A HOME falsa dessa rodada ficou vazia.

### O que rodou

No HEAD `b5809c33`, com HOME falsa:
- `cargo test -p seele-app` (todos os alvos, sem `--workspace`): `--bin` 344
  (339 na base, mais os cinco do módulo do E54), `frontend` 294, `web` 12,
  `celular` 8, `empacotamento` 22, `encerramento` 3, `fontes` 5,
  `manifesto_hibrido` 11, `marca` 9, `permissoes` 4, `tokens` 3;
- `cargo fmt --all -- --check`: limpo;
- `cargo clippy -p seele-app --all-targets -- -D warnings`: limpo (só o aviso
  do ligador, da base).

Esta rodada só mexeu nos testes do `main.rs`. Não rodei `check-runtime` nem as
bancadas, e nenhuma conformidade.
