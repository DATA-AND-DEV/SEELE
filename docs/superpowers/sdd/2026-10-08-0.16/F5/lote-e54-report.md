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
| este | o relatório |

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
- `o_connect_confere_a_geracao_antes_dos_efeitos_e_ao_gravar_o_slot`: a forma.
  O cenário só exercita a primeira conferência (a velha termina depois de a
  nova ter entrado), e nenhum teste daqui para o `connect` entre a primeira e
  a gravação do slot; o guarda prende a segunda pela forma, sem ler
  comentário.

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
3. **A corrida residual no Rust.** Uma nova que passa pelo `AlreadyConnected`
   do começo antes de a velha gravar o slot, e só revoga depois, grava por
   cima da velha ao terminar (a velha é solta e desligada pelo `Drop`), e o
   pedido de soltar da janela chega com uma geração que já não é a de pé — não
   toca em nada, e diz em INFO. A tela e o slot ficam com a nova.
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

## 7. O que rodou

Depois da pausa de medição de CPU, no HEAD `efb1d356` (o relatório não muda
código):

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
342 testes dele deixam de rodar no Windows, e o remédio é a nota e o `cfg` do
`tests/permissoes.rs` no módulo `a_entrada_velha_que_chega_tarde`.

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
  que acrescentar.

**A metade do Rust porta inteira** (as duas conferências, `largar_a_substituida`,
`substituida`), e é ela que impede a mensagem de ir ao servidor errado.

**A metade da janela não porta como está.** A 0.15 não tem o contador
`tentativas` (lote W). A prova menor que cabe lá, para a orquestradora
decidir: o `catch` de `conectar` trata `falha?.substituida === true` como
calada (registra, e não escreve a linha vermelha); sem `soltar_a_entrada` —
na 0.15 a velha que entra antes de a nova chegar ao Rust continua entrando,
como hoje, e a tela bate com o slot. Não escrevi o porte.
