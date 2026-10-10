# Lote R da F5: relatório

Base `2112c389` (branch `0.16/lotes`, com as ondas 1 a 5 da F5), mais o conserto
do convite da F1: `24e7e249` (do `0.16/protocolo`, sobre a mesma `c8b3cd47`),
mesclado em `269d472c` sem conflito, trazendo `d0b98b9c` e `8d0ecb38`. Antes da
conformidade, o `0.16/lotes` de `cd2227c5` (a `versao/0.16` de
`f7fb8ab9`/`adeb6960`, com o mesmo conserto do convite, o K4, o Z1, a F3 e a F8)
entrou em `a671b3d8`, também sem conflito, para ela rodar uma vez só sobre o
estado final. Branch `0.16/lotes-r`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a0ba6599ca8b619f2`. Tudo
local: sem push, tag nem release.

| commit | item | o que muda |
|---|---|---|
| `269d472c` | preparo | merge do `24e7e249`: recusar os MODs deixa o convite intacto no servidor |
| `211ed155` | o reenvio | a batida que para na pergunta dos MODs entrega a credencial a `aceitePendente` (`camada-mods.js`); `aceitarOsMods` a devolve a `conectar(alvo, null, credencial)`; o parâmetro `token` de `conectar` vira `credencial`; o guarda `o_aceite_obtem_o_conjunto_antes_de_entrar` procura `conectar(`; a cena do lote R na `ajustes-v013.cjs` (o reenvio, o esquecimento ao entrar, o RECONECTAR sem credencial) |
| `21471bfc` | o esquecimento ao desistir | `aceitarOsMods` confere que a pergunta que o sim respondeu ainda é a aberta antes da volta; a cena ganha as três portas de desistir e a desistência no meio da obtenção |
| `3ff15974` | o guarda | `a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta` (`tests/frontend.rs`); a cena procura a credencial onde ela não pode estar; o comentário de `reconectar()` (`tela-fim.js`) passa a dizer a decisão |
| `de3cba3a` | a bancada | o servidor de mentira recusa com `Refused { reason: CredentialRejected }`, a forma que a FFI devolve |
| `e80e086c` | a conformidade | `crates/seele-conformance/tests/volta_do_aceite.rs`, commitada compilada (`--no-run`) a pedido da coordenação, e rodada depois sobre `a671b3d8` |
| `d521b41c` | rodada 1, I1 | o campo do diálogo de servidores é zerado assim que o link colado é lido; a cena bate pelo campo |
| `a910b2e2` | rodada 1, I2 | o `Debug` de `ConnectConfig` (`seele-ffi`) e o de `Destino` (`seele-core`) escondem o segredo |
| `9b1c2473` | rodada 1, I3 | a pergunta recebe o `alvo` da batida, e não o `ultimoAlvo` da janela; a corrida da revisão vira a cena 6 |
| `5eaa7bdf` | rodada 1, M1 | `fecharAceiteDeMods` limpa a credencial dentro do objeto antes de soltá-lo |
| `a6362ded` | rodada 1, M2 | a parte 4 do guarda varre também os scripts por `onCloseRequested` e `beforeunload` |
| `8097d310` | rodada 1, M5 | `o_aceite_obtem_o_conjunto_antes_de_entrar` procura `await conectar(` |
| `af11a6d3` | rodada 1, M6 | o comentário de `baterDeNovo` deixa de dizer «mesmo convite» |
| `789729d5` | rodada 1, ajuste do I1 | o campo é zerado quando o link segue para uma tentativa, e o link que o `analisar_convite` recusa fica nele |
| `6b23b694` | sobra da rodada 1 | uma pergunta que substitui outra limpa a credencial da antiga |
| `80422e54` | sobra da rodada 1 | a parte 4 do guarda procura também `close-requested` |
| `16652527` | sobra da rodada 1 | a cena entra por um link de outra linha, e o ramo `entrarPelaLinha` ganha prova |
| `a671b3d8` | integração | merge do `0.16/lotes` de `cd2227c5`, antes da conformidade |

**O que não mudou.** O fio: `crates/seele-proto` e `crates/seele-server`
intocados (o servidor consertado entrou por merge, sem edição; a reversão do
`session.rs` no item 4 das provas foi local, temporária e devolvida com `cmp`). Nada do
SEELE-ENC/1, dos arquivos da F6, de `api/`, da CSP, de
`apps/seele-app/src/executor.rs`, de `crates/seele-motor`, da tela do celular, nem
dos arquivos do K4 (`main.rs`, `base.js`, `mods-regiao.js`, `regiao-do-mod.cjs`):
o guarda novo **lê** o `main.rs` como texto, e não o edita. Em
`crates/seele-ffi/src/lib.rs` e `crates/seele-core/src/enlace.rs`, só o `Debug`
das duas structs e o teste dele (I2 da rodada 1, por decisão do controlador). Nenhuma dependência
nova, nada baixado. Nenhum `.cjs` novo em `bancada/`: a `ajustes-v013.cjs` já
existia e já está no job `bancadas` do `ci.yml`. Nenhuma promessa nova de API:
não há seção «Para a v6».

As reversões foram feitas trocando o trecho exato (com `sed` ou `Edit`),
rodando o teste, devolvendo o arquivo de uma cópia no scratchpad e conferindo
com `cmp`. Todas voltaram idênticas.

## Onde a credencial mora, e onde é esquecida

**Mora em `aceitePendente`** (`apps/seele-app/ui/camada-mods.js`), o objeto da
pergunta dos MODs, no campo `credencial`, e em nenhum outro lugar entre duas
chamadas. O caminho inteiro:

1. `conectar(alvo, apelido, credencial)` (`tela-boot.js`) a manda no
   `joinSecret` do `connect`, como antes.
2. Se a resposta é `ModsNaoAceitos`, o `catch` a passa a
   `levarParaOAceiteDeMods(motivo, alvo, credencial ?? null)` — com o `alvo`
   desta chamada, e não com o `ultimoAlvo` da janela (I3 da rodada 1) —, que a
   passa a `abrirAceiteDeMods`, que a guarda em `aceitePendente` junto do alvo,
   do conjunto e da lista. Qualquer outra resposta não a guarda em lugar nenhum.
3. `aceitarOsMods` grava o sim, obtém o conjunto, confere que a pergunta ainda é
   a que o sim respondeu, lê `aceitePendente?.credencial`, fecha a pergunta (o
   que a esquece) e chama `conectar(alvo, null, credencial)`: para o **mesmo
   alvo**, com a credencial só como argumento dessa volta.

**É esquecida por `fecharAceiteDeMods`**, em duas linhas: o campo é limpo
dentro do objeto (`aceitePendente.credencial = null`, M1 da rodada 1, para que a
cópia `pergunta` de `aceitarOsMods` não a alcance enquanto o catálogo responde)
e o objeto é solto (`aceitePendente = null`, que já existia para a pergunta).
Toda saída da pergunta passa por ali:

- **entrar**: `aceitarOsMods` fecha a pergunta antes da volta. Dali em diante a
  credencial existe só no quadro da chamada de `conectar`, e some quando ela
  termina, entre a pessoa ou não;
- **desistir**: RECUSAR E VOLTAR, Escape e clicar fora chamam
  `fecharAceiteDeMods`. A pergunta é modal e está sobre a tela de entrada:
  fechá-la é voltar à entrada. Quem desiste **durante a obtenção** do conjunto
  também passa por ali, e a volta que viria depois não sai mais (ver
  «Desistir no meio da obtenção», abaixo);
- **fechar a janela**: a credencial só existe na memória da página. A janela é
  uma só, e fechá-la encerra o processo (o `ExitRequested` do `main.rs` se
  despede e não segura a saída). O guarda confere que o `main.rs` não usa
  `prevent_exit`, `prevent_close` nem `CloseRequested`, e que nenhum script da
  interface usa `onCloseRequested` nem `beforeunload` (M2 da rodada 1): se um
  dia a janela passar a esconder em vez de fechar, ou a página a vetar o
  próprio fechamento, ele reprova e diz que a credencial precisa ser esquecida
  no fechar.

O link colado também tinha uma cópia na página: o campo do diálogo de
servidores, que o submit lia e não zerava. Desde o I1 da rodada 1 ele é zerado
quando o link segue para uma tentativa (antes de `entrarPelaLinha` e antes de
`irParaOServidor`); o link que o `analisar_convite` recusa fica, porque não
inicia tentativa nenhuma.

Não acrescentei um ouvinte de `pagehide`: ele zeraria uma variável que morre no
mesmo instante, e nenhum teste conseguiria distingui-lo da ausência dele.

### Por que não em `esquecerOEstadoDaSessao`

O I-b fez de `esquecerOEstadoDaSessao` (`tela-sessao.js`) o único lugar que
esquece o estado **da sessão**, chamado nas duas saídas de uma sessão (`ejetar`
e `limparSessaoEncerrada`). A credencial da tentativa não é estado da sessão:
ela vive entre a batida e a entrada, antes de haver sessão, e morre na entrada.
Pô-la lá seria um esquecimento que nunca tem o que esquecer — quando uma sessão
sai, a credencial já foi esquecida ao entrar — e as desistências (recusar,
Escape, clicar fora) não passam por saída de sessão nenhuma. O guarda do I-b lê
os `let` de topo de `tela-sessao.js`, e nada mudou ali.

A vida dela é a da pergunta, e por isso ela mora **dentro** do objeto da
pergunta, e não num `let` próprio: um segundo lugar teria de ser esquecido em
cada porta da pergunta, e uma porta esquecida é uma credencial que sobra.

## A senha da portaria

Neste app ela não tem caminho próprio, e segue o do token por construção.

- **Quem hospeda** a define em `camada-portaria.js` (`definir_senha_do_server`,
  ADR 0021); o servidor a guarda como hash em `configuracao.senha_server`
  (`crates/seele-server/src/admissao.rs`).
- **Quem entra** a manda no mesmo campo do convite: o `join_secret` do `Hello`
  é «the invite token or the server password» (`seele-core/src/client.rs`), e
  do lado da casca é o `joinSecret` do `connect`.
- **O app de mesa não tem campo onde quem entra digite a senha do servidor.** O
  `joinSecret` só é alimentado pelo token de um `seele://` — o colado no
  diálogo de servidores (`camada-servidores.js`) e o que o lançador entrega na
  abertura (`cumprirAAbertura`, em `tela-boot.js`) — e o link nunca leva senha
  (`a_senha_nunca_viaja_no_link`, em `seele-proto/src/uri.rs`).

O conserto não distingue as duas: o que a batida mandou no `joinSecret` é o que
a volta reenvia. A conformidade nova tem um teste para cada uma (o convite e a
senha).

## O que decidi sobre `reconectar()`

**Fica como está, sem credencial nenhuma.** O RECONECTAR da tela de fim só
existe depois de uma sessão que **entrou**, e entrar gastou o convite e esqueceu
a credencial; reenviá-la seria gastar de novo o que já foi gasto, que é o que o
brief proíbe. A credencial da tentativa nunca chega lá: a pergunta dos MODs
fecha antes de haver sessão.

O comentário de `reconectar()` dizia o contrário — que «o convite colado não é
limpo aqui», porque «o token vale para este servidor» —, e era falso desde a
0.9.0: o token viaja como argumento de `conectar` e o RECONECTAR sempre chamou
`conectar()` sem nenhum. Ele passou a dizer a decisão, e o que resta a quem
volta: num servidor fechado por convite sem portaria, um convite novo; com
portaria, a decisão de quem hospeda (`portaria::ja_admitido`) é a credencial que
dura.

O guarda confere as duas coisas: o RECONECTAR chama `conectar()` sem argumento,
e só três chamadas de `conectar` em toda a interface levam credencial — o link
colado, o link do lançador e a volta que aceita os MODs. A batida da espera
(`baterDeNovo`), o hospedar e a troca de servidor não levam nenhuma, como antes.

## Desistir no meio da obtenção

Um achado do lote, e a única mudança de comportamento além da credencial.
`aceitarOsMods` grava o sim e obtém o conjunto com a pergunta ainda aberta, e
RECUSAR, Escape e clicar fora continuam à mão nesse tempo. Ela seguia para a
volta mesmo quando a pessoa tinha fechado a pergunta, e a janela entrava
sozinha quando o pacote terminava de chegar — contra o que o próprio arquivo diz
de fechar a pergunta («sair da pergunta não entra em lugar nenhum»). Com a
credencial na pergunta, isso seria entrar com uma credencial que desistir devia
ter esquecido.

Agora ela guarda a pergunta que o sim respondeu e, depois da obtenção, só segue
se a aberta ainda for essa (pela identidade: uma pergunta de outra batida não é
a que o sim respondeu). E lê a credencial da pergunta aberta naquele instante, e
não de uma cópia tirada no começo, para que a ordem dos dois cuidados não
importe.

**É uma mudança de comportamento, e fica** (decisão da coordenação depois da
revisão): é o que o próprio comentário do arquivo promete, e é o que «desistir
esquece» pede. Provada pela reversão R2b, abaixo.

## As provas

Bancada: `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright
perl -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/ajustes-v013.cjs`.

A cena do lote R abre a página real com um servidor de mentira que responde
como o de verdade e na mesma ordem: confere o segredo primeiro (a política de
admissão não tem memória com a portaria desligada), depois os MODs, e gasta o
convite só na entrada (`d0b98b9c`). A recusa sai na forma da FFI,
`{ Refused: { reason: "CredentialRejected" } }`. O convite é gerado na bancada
(`convite-da-bancada-<pid>-<hora>`) e não é de servidor nenhum.

### 1. O reenvio (`211ed155`)

- **Vermelho em `269d472c`** (a base com o servidor consertado), pelo motivo
  certo: «a volta que aceita os MODs saiu sem a credencial da tentativa
  (joinSecret null): num servidor fechado por convite ela recebe CREDENCIAL
  RECUSADA, e o convite que o servidor guardou intacto não serve para nada».
- **Verde** com o conserto: a volta leva o convite e chega à `#tela-auth`.
- **Reversão** (`await conectar(alvo, null, null)` no lugar do reenvio): a
  mesma frase do vermelho. Arquivo devolvido e conferido com `cmp`.

Na mesma cena, depois de entrar:

- **esquecida ao entrar**: `aceitePendente?.credencial` é `null` (provada pela
  reversão do item 2);
- **o RECONECTAR vai sem ela**: depois de `mostrarFim('Kicked')` e do clique em
  RECONECTAR, o terceiro `connect` sai com `joinSecret` nulo. Verde desde a
  base, porque o RECONECTAR nunca levou credencial; quem o prova é o guarda
  (G3, abaixo).

### 2. O esquecimento ao desistir (`21471bfc`)

- As três portas — RECUSAR E VOLTAR, Escape e clicar fora (num ponto do véu
  fora da caixa, achado por `elementFromPoint`) —: com a pergunta aberta, a
  credencial está nela (a cena confere, para não medir nada à toa); fechada, não
  está.
- No meio da obtenção (o MOD faltando, `instalar_mod_do_catalogo` com 600 ms):
  aceitar, RECUSAR durante a obtenção, esperar a conferência que vem depois
  dela, e contar os `connect`.
- **Vermelho em `211ed155`**: «a pessoa fechou a pergunta enquanto o MOD
  chegava, e a janela entrou mesmo assim» (8 `connect` onde devia haver 7). As
  três portas já passavam ali, porque a credencial nasceu dentro da pergunta.
- **Verde** com a conferência da pergunta.
- **Reversão R2a** (tirar o `aceitePendente = null` de `fecharAceiteDeMods`):
  vermelho em «a credencial da tentativa continuou guardada depois de a pessoa
  entrar: um convite de uso único já gasto ficou na memória da janela». Numa
  cópia temporária da bancada sem essa asserção (apagada logo depois), o
  vermelho seguinte foi a primeira porta: «desistir por RECUSAR E VOLTAR fechou
  a pergunta e deixou a credencial da tentativa na memória da janela».
- **Reversão R2b** (tirar `if (aceitePendente !== pergunta) return;`): o mesmo
  vermelho da desistência no meio da obtenção.
- As duas devolvidas e conferidas com `cmp`.

### 3. O guarda (`3ff15974`)

`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`, em cinco
partes:

1. toda linha de `tela-boot.js` e `camada-mods.js` que nomeia `credencial` ou
   `aceitePendente` (e a cópia `pergunta` de `aceitarOsMods`) está numa lista de
   dezesseis (quinze no lote, mais a limpeza do M1), e cada linha da lista ainda
   existe;
2. nenhum outro script nomeia `aceitePendente`;
3. só três chamadas de `conectar` têm três argumentos (as três de cima), e o
   RECONECTAR chama `conectar()`;
4. o `main.rs` não usa `prevent_exit`, `prevent_close` nem `CloseRequested`, e,
   desde o M2 da rodada 1 e a sobra seguinte, nenhum script da interface usa
   `onCloseRequested`, `beforeunload` nem `close-requested`;
5. o `main.rs` nomeia `join_secret` em exatamente duas linhas (o parâmetro do
   `connect` e o campo de `ConnectConfig`), e nenhum campo do `Snapshot` da FFI
   tem nome de segredo (o guarda exige ter lido pelo menos 30 campos, com
   `ended` entre eles, para não ficar cego). **Fica depois do I2**: o `Debug`
   das structs esconde o segredo de um `?config`, mas não alcança o
   `join_secret` solto num `tracing` (o `Debug` de um `Option<String>` o
   imprime), e é esse que esta parte pega.

E a cena da bancada procura a credencial, no fim de tudo, no `localStorage`, no
`sessionStorage`, no DOM, nos campos, no console (com os argumentos serializados
por um embrulho do `console`, porque um objeto chega ao Playwright como
«Object») e em todo comando que não seja o `connect`.

- **Vermelho**: o guarda é novo e nomeia o caminho que o conserto criou; em
  `269d472c` ele não tem o que ler. O que importa é que ele reprove um
  vazamento, e as reversões provam isso.
- **G1** (`console.warn("aceite pendente:", aceitePendente)` em
  `abrirAceiteDeMods`): o guarda reprova com «a credencial da tentativa ganhou
  um caminho novo em `camada-mods.js`», e a bancada com «a credencial da
  tentativa apareceu fora do connect: o console».
- **G2** (`localStorage.setItem("seele.ultimo-convite", credencial)` no começo
  de `conectar`): o guarda reprova em `tela-boot.js`, e a bancada com «…fora do
  connect: localStorage».
- **G3** (`conectar(alvoDoFim, null, ultimaCredencial)` no RECONECTAR): o guarda
  reprova com «`conectar` passou a levar credencial em `tela-fim.js`».
- As metades das partes 4 e 5 que leem o `main.rs` e o `types.rs` ficam sem
  reversão (decisão da coordenação): revertê-las pede editar o `main.rs` (do K4)
  ou o `types.rs` da FFI, e o lote não toca neles. A cegueira delas está coberta
  pelas contagens (duas linhas exatas; 30 campos). A metade da parte 4 que lê os
  scripts foi revertida no M2 da rodada 1.
- Todos os arquivos devolvidos e conferidos com `cmp`.

### 4. A conformidade pela FFI (`e80e086c`, rodada sobre `a671b3d8`)

`crates/seele-conformance/tests/volta_do_aceite.rs`, dois testes com
`let _vaga = vaga::minha();` na primeira linha, pelo caminho do app
(`seele_ffi::Connection::connect`, o sim gravado por `seele_ffi::mods::aceitar`)
contra um servidor de verdade fechado, sem portaria e com um MOD:

- `a_volta_que_aceita_os_mods_entra_com_o_convite_e_e_recusada_sem_ele` — o
  convite de uso único, o caso do relato;
- `a_volta_que_aceita_os_mods_entra_com_a_senha_e_e_recusada_sem_ela` — a
  senha do servidor, a outra metade do `join_secret`.

Cada um: a batida com o segredo para em `ModsNaoAceitos`; o sim é gravado; a
volta **sem** o segredo recebe `Refused { reason: CredentialRejected }` (o
defeito, medido pela FFI); a volta **com** ele entra. Foi commitada compilada
(`--no-run`), a pedido da coordenação, para a revisão vê-la enquanto a
conformidade estava pausada, e rodou quando ela foi liberada, sobre o estado
mesclado (`a671b3d8`), sozinha e com `-- --test-threads=1`:

- **Verde**: `cargo test -p seele-conformance --test volta_do_aceite --
  --test-threads=1`, 2 de 2, em 0,76 s.
- **Reversão do servidor**, autorizada pela coordenação, local e temporária: o
  hunk de `crates/seele-server/src/session.rs` do `d0b98b9c` aplicado ao
  contrário (`git apply -R`), o que leva o `gastar` de volta para o braço `Entra`
  da portaria, antes dos MODs. O teste do convite caiu no passo da volta com a
  credencial: «a volta que aceita os MODs, com o convite, não entrou:
  Refused { reason: CredentialRejected }. … se o convite foi gasto pela batida
  que parou na pergunta, o d0b98b9c da F1 regrediu». O da senha continuou verde,
  como devia: a senha não é gasta. `session.rs` devolvido de uma cópia e
  conferido com `cmp`; nada disso foi commitado, e o teste rodou verde de novo
  (2 de 2) sobre o arquivo restaurado.

O passo 3, a volta **sem** a credencial recusada, passou nos dois estados: ele
mede o que o app consertou, e não o que o servidor consertou.

## O que rodou

- `cargo test -p seele-app --test frontend`: 280 de 280 (279 da base e o guarda
  novo) depois de cada item; 281 de 281 sobre `a671b3d8`, com o que o merge
  trouxe.
- `cargo xtask check-runtime`: verde depois de cada item e sobre `a671b3d8`.
- `ajustes-v013.cjs`: verde no estado final, com as cenas v0.13, do lote J e do
  lote R (a última rodada, sobre `a671b3d8`).
- `cargo fmt --all -- --check` antes de cada commit.
- `cargo clippy -p <crate> --all-targets --all-features -- -D warnings` para
  `seele-app`, `seele-conformance`, `seele-core` e `seele-ffi`: limpos, o último
  sobre `a671b3d8` (o `ld: duplicate -rpath` é ambiental).
- `cargo test -p seele-core --lib`: 512 de 512 na rodada 1, 527 de 527 sobre
  `a671b3d8`. `cargo test -p seele-ffi --lib`: 160 (mais 1 ignorado) na rodada 1,
  166 (mais 1 ignorado) sobre `a671b3d8`.
- A conformidade `volta_do_aceite`, sozinha e com `-- --test-threads=1`: 2 de 2
  sobre `a671b3d8`, e a reversão do servidor descrita no item 4.

## O que não rodou

- As outras bancadas de navegador do `ci.yml`: nenhuma lê os arquivos mexidos
  além da `ajustes-v013.cjs`.
- Nenhum outro arquivo de conformidade: só o `volta_do_aceite`, que é o do
  lote. A bateria inteira roda na integração.
- Nada com `cfg(windows)` ou Linux foi escrito.

## Rodada 1 de conserto

A revisão deu o lote como conforme, com duas lacunas, e deixou três Importantes
e quatro Menores para esta rodada (o M3 e o M4 foram para o Z2). Cada item tem
o vermelho antes do conserto, o verde, e a reversão devolvida com `cmp`.

- **I1 — o link colado ficava no campo (`d521b41c`, ajustado em `789729d5`).**
  O submit do diálogo de servidores (`camada-servidores.js`) lia
  `#servidores-endereco` e não o zerava; só reabrir o diálogo o zerava, e o
  `seele://` com o convite dentro ficava escrito na página depois de a pessoa
  entrar ou desistir. Defeito anterior ao lote. A cena da bancada passa a bater
  **pelo campo** (o diálogo, o link colado e Enter, com `analisar_convite` de
  mentira) e confere o campo vazio depois de entrar e depois de cada
  desistência; a busca do fim deixa de acusar o `analisar_convite`, que é onde o
  link é lido.
  - `d521b41c` zerava o campo logo depois de lido. Vermelho em `e80e086c` («o
    link colado, com o convite dentro, ficou no campo do diálogo de servidores
    depois de a pessoa entrar»), verde, e o mesmo vermelho com a linha tirada.
  - `789729d5`, a pedido da coordenação: o campo é zerado só quando o link
    segue para uma tentativa — antes de `entrarPelaLinha` (o SEELE de outra
    linha) e antes de `irParaOServidor`. O link que o `analisar_convite` recusa
    fica, para a pessoa corrigir o erro de digitação. A cena ganha o caso 4b, um
    link recusado: vermelho em `af11a6d3` («o link que o analisar_convite
    recusou sumiu do campo…»), verde com a mudança. Reversões: o arquivo de
    `d521b41c` (zerar ao ler) reprova o 4b; tirar as duas limpezas reprova «…
    ficou no campo do diálogo de servidores depois de a pessoa entrar».
- **I2 — o `Debug` escondia nada (`a910b2e2`).** Por decisão do controlador,
  `ConnectConfig` (`seele-ffi`) e `Destino` (`seele-core`) trocam o
  `derive(Debug)` por um `impl Debug` à mão, com os mesmos campos na mesma
  ordem e o segredo como `Some(<oculto>)` ou `None`. Um teste em cada crate
  formata `{:?}` e `{:#?}` com um segredo conhecido: vermelhos com o derive,
  verdes com o impl, vermelhos de novo voltando ao derive (a reversão foi
  refeita no arquivo final). Os módulos de teste moram no fim de cada arquivo:
  junto da struct, o do `seele-ffi` virava o primeiro `#[cfg(test)]` do arquivo,
  e três guardas que leem como produção tudo o que vem antes dele
  (`nobody_raises_the_event_without_moving_the_revision`,
  `a_voz_so_abre_pela_porta_da_sala`,
  `o_retrato_com_voz_le_o_silencio_digital_da_telemetria_dela`) ficaram
  vermelhos; mudados de lugar, os três voltaram. A parte 5 do guarda de texto
  fica, pelo motivo escrito nela acima. Fecha o «`Debug` de `Destino`» da lista
  do Z2.
- **I3 — a credencial presa ao alvo da janela (`9b1c2473`).** O `catch` de
  `conectar` passava o `ultimoAlvo` à pergunta, e `hospedar()` o reescreve no
  meio de um `connect` (HOSPEDAR AQUI não fica apagado). A revisão mediu a volta
  indo a `127.0.0.1:9999` com o convite de A; antes do lote essa corrida não
  levava segredo nenhum. Agora a pergunta recebe o `alvo` local. A
  `corrida-r.cjs` da revisão virou a cena 6 da `ajustes-v013.cjs` (o `connect`
  demora 400 ms e o `ultimoAlvo` muda aos 100): vermelha em `a910b2e2` («…a volta
  que aceita os MODs foi a 127.0.0.1:9999, levando o convite de
  192.0.2.7:8384…»), verde (a volta e o `aceitar_mods` vão a 192.0.2.7:8384), e o
  mesmo vermelho com o `ultimoAlvo` de volta.
- **M1 — a cópia da pergunta (`5eaa7bdf`).** `fecharAceiteDeMods` limpa
  `aceitePendente.credencial` antes de soltar o objeto. A cena 5 guarda a mesma
  referência que `aceitarOsMods` guarda e a confere logo depois de RECUSAR, com o
  catálogo ainda sem responder: vermelha em `9b1c2473`, verde, e vermelha de
  novo com a linha tirada.
- **M2 — fechar a janela pela página (`a6362ded`).** A parte 4 do guarda varre
  também `ui/*.js` por `onCloseRequested` e `beforeunload`. Com
  `getCurrentWindow().onCloseRequested((e) => e.preventDefault())` posto em
  `tela-boot.js`, o guarda de `5eaa7bdf` passava, e o de agora reprova
  («`tela-boot.js` passou a usar `onCloseRequested`…»).
- **M5 — o `find` exato (`8097d310`).** `o_aceite_obtem_o_conjunto_antes_de_entrar`
  procura `await conectar(`. Com a volta trocada por `await reconectar();`, o
  guarda de `a6362ded` passava (achava o `conectar(` de dentro de
  `reconectar(`), e o de agora reprova. A recusa deixa de ser o `expect`
  «aceitar continua entrando no servidor» e passa a dizer o que quebra. O
  `troca.find("conectar(")` de outro guarda, mais acima em `frontend.rs`, tem a
  mesma forma e não é do lote; ficou como estava, e a coordenação o anotou para
  o Z2.
- **M6 — o comentário de `baterDeNovo` (`af11a6d3`).** Dizia «mesmo convite», e
  a batida da espera nunca o levou. Passa a dizer que ela vai sem credencial, e
  por que o servidor não precisa dela ali. Só comentário; o guarda já cobra
  que essa chamada não leva credencial.

### As três sobras da re-revisão

A re-revisão deu os sete itens da rodada como resolvidos, sem quebra nova, e
deixou três sobras pequenas, cada uma no seu commit.

- **Uma pergunta que substitui outra (`6b23b694`).** `abrirAceiteDeMods` trocava
  `aceitePendente` sem limpar o objeto antigo, e uma batida nova durante a
  obtenção da antiga deixava a cópia dela alcançando a credencial de A até a
  obtenção acabar. Agora a mesma limpeza do M1 vem antes de reatribuir. A
  `troca-r.cjs` da revisão virou o caso 5b (o servidor de mentira aceita um
  segundo segredo, o da segunda batida): vermelho em `789729d5` («uma pergunta
  nova substituiu a antiga, e a cópia que a obtenção antiga guarda ainda
  alcança a credencial da primeira batida»), verde, e o mesmo vermelho com a
  linha tirada.
- **Um terceiro jeito de vetar o fechar (`80422e54`).** Um ouvinte de
  `tauri://close-requested` com alvo da janela também veta: conferido na fonte
  do `tauri` 2.11.5 (a versão do `Cargo.lock`), `on_window_event` em
  `src/manager/window.rs`, linhas 171 e 172, chama `prevent_close` sempre que há
  ouvinte JS desse evento. A parte 4 do guarda procura `close-requested`, que
  pega também o `onCloseRequested`. Com
  `getCurrentWindow().listen("tauri://close-requested", …)` posto no fim de
  `base.js`, o guarda de `6b23b694` passava, e o de agora reprova; `base.js` foi
  devolvido e conferido com `cmp`, e nada dele entrou no commit.
- **O ramo `entrarPelaLinha` sem prova (`16652527`).** Era barato: o caso 4c cola
  um link cujo `analisar_convite` de mentira devolve um `versoes::ParaEntrar`
  (`Abrir`), responde ABRIR, e confere que o `abrir_versao` recebeu o link, que
  o campo ficou vazio e que nenhum `connect` saiu desta janela. Sem conserto
  próprio; com o `consumirOCampo()` desse ramo tirado, o caso reprova («o link
  de outra linha seguiu para o SEELE daquela linha e ficou escrito no campo…»).
  `camada-versoes.js`, da F6, não foi tocado.

## Observações

- **Vão para o Z2, registrados pela coordenação, e não mexi:** o sim dos MODs
  gravado (e o MOD instalado) quando a pessoa desiste no meio da obtenção (M3,
  ADR 0045); o token esquecido depois de `NicknameTaken`, que obriga a colar o
  link de novo; o link guardado em `Session.convite` e em
  `Session.abertura.entrar` (`main.rs`) e o `Destino.segredo` que o core guarda
  a sessão inteira e reenvia na reconexão automática (M4). O `Debug` de
  `Destino`, que estava nessa lista, saiu dela com o I2.
- **O guarda lê o `main.rs`.** O K4 mexe nele, mas não nas linhas de
  `join_secret` nem nas chamadas da janela; se o guarda reprovar na integração,
  a frase diz o que conferir.
