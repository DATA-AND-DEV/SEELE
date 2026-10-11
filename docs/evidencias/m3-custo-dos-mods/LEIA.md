# M3 — o custo dos MODs na janela, medido

O roteiro da medição de custo que o ADR 0052 pediu (`0052:189-193`) e a spec de
23/09 chamou de M3. Os números e a leitura deles estão no
[ADR 0056](../../adr/0056-mods-na-tela-inteira.md). Aqui fica o **como**, para
que a mesma medição se repita com as regiões da API 6 (tarefa 11 do plano da
F3) e em outra máquina.

**Nada daqui é código de produto.** Os dois `.rs` só compilam numa cópia do
código feita por `git archive`, e o repositório não muda.

## O que é medido, e por quê assim

| o quê | como | por que assim |
|---|---|---|
| memória do app e da webview | `phys_footprint` do `seele-app` e dos três processos do WebKit que nasceram com ele (WebContent, GPU, Networking), por `footprint`; mediana das amostras de 0, 15, 30, 45 e 60 s | o WebKit abre esses processos por XPC, com o `launchd` como pai; o parentesco não os acha, e o roteiro os reconhece por PID logo acima do do app e por terem nascido até 15 s depois dele |
| CPU | o tempo de CPU somado dos quatro processos no intervalo, por `ps`, em % de um núcleo | — |
| tempo de quadro | `xctrace record --instrument "Core Animation Commits" --attach <pid> --time-limit 30s`, nos primeiros 30 s da janela; da tabela `coreanimation-commit-interval`, os commits do `seele-app` (contagem por segundo, duração p50, p95, p99 e máximo) e os que passam do orçamento de um quadro a 60 Hz (16,7 ms), contados como engasgo | no WebKit, quem faz o commit do quadro da página é o processo do app. Medir dentro da página exigiria injetar script, e isso exigiria mudar o binário. **Os instrumentos de engasgo (Hitches, Frame Lifetimes) foram descartados pela medida:** 60 s deles deram um trace de 526 MB, e o `xctrace` passou mais de 10 min salvando a 99 % de CPU, o que sujaria o cenário seguinte. O «Core Animation Commits» grava 10 s em 19 MB e salva em 3 s |
| jitter da voz | o número que a barra de telemetria mostra ao lado de JITTER, lido pela acessibilidade a cada 2 s, com o `m3-falante` falando na sala 1 | é o número que a pessoa vê; o falante sintético manda Opus de verdade (440 Hz, −20 dBFS, 32 kbit/s) a cada 20 ms, e imprime a própria cadência, para um desvio dele não ser lido como custo dos MODs |

**Os cenários** são os do ADR 0052, mais o 0/1/3 do plano da F3:

| cenário | servidor | o que a janela faz |
|---|---|---|
| `sem-mod` | nenhum MOD | fica na conversa |
| `1-mod` | PERFIS 3.3.0 | fica na conversa |
| `3-mods` | PERFIS 3.3.0, MESA 3.1.0, ESTILO 3.1.0 | fica na conversa, sem abrir nada («UI fechada») |
| `3-mods-editor` | os três | «EDITAR MEU PERFIL», do PERFIS, aberto (toque no próprio cartão da lista) |
| `3-mods-mesa` | os três | a MESA aberta, com uma mesa, uma cena e quatro peças, e uma peça arrastada por 1,5 s, ida e volta, a cada volta do laço |
| `<cenário>+voz` | o mesmo, com o falante sintético na sala 1 | a janela entra na sala 1 e fica ouvindo |

**O app é o publicado, sem mudança nenhuma.** O `.app` de `/Applications` foi
conferido contra o release: o tarball bate com o `SHA256SUMS` da v0.15.1, e o
`diff -rq` entre o `.app` extraído dele e o instalado não acha diferença. Ele é
aberto com `open -n -a /Applications/SEELE.app --env SEELE_HOME=<pasta
descartável> --args --entrar seele://127.0.0.1:8391/?fp=<impressão>`: a pasta
de dados, a identidade e o aceite são do cenário, e a instalação de quem usa a
máquina não é tocada. O armazenamento do WKWebView é o do identificador
`tech.datadev.seele`, compartilhado com a instalação habitual; o roteiro não
muda nenhuma preferência guardada nele (as duas chaves que a casca grava,
`camada-mods.js:598` e `:1093`, são a escolha de apresentação e o modo de
desenvolvedor, e nenhum cenário mexe nelas).

**Os MODs são os publicados.** O `preparar.sh` baixa do indexador os arquivos
que o catálogo lista para cada versão, e o servidor da medição calcula o hash
do conteúdo e o imprime: os três batem com o catálogo. O app baixa os pacotes
do indexador e confere o hash como faria para qualquer pessoa, ou os acha num
cache por hash, se ele existir (ver «Preparar»).

**Não atrapalha quem usa a máquina, e quem usa a máquina não atrapalha a
medida.** A janela precisa estar no palco (com o Stage Manager, uma janela fora
dele vira miniatura, e uma janela encoberta tem o desenho estrangulado pelo
WebKit), e trazê-la tira o foco de quem estiver digitando. Por isso cada
cenário só começa:

- **sem compilação de outra sessão** (nenhum `rustc`) e com menos de 2 núcleos
  ocupados no total: a média de carga não serve de critério, porque decai
  devagar depois de uma compilação;
- **com a máquina ociosa há 5 minutos** (`HIDIdleTime`);
- **com a sessão gráfica desbloqueada**: com ela bloqueada a janela não vai ao
  palco e a árvore de acessibilidade não responde, e o critério de máquina
  ociosa se cumpre justamente quando ela bloqueia (medido em 08/10: um cenário
  com voz falhou às 18:15 com a sessão bloqueada, sem achar o botão de aceite).
  Se ela bloquear durante o cenário, ele sai marcado como interrompido.

Uma entrada de uma pessoa durante o cenário o marca `"interrompido": true`,
e um `rustc` visto durante a janela conta em `"rustc_durante"`; a campanha
repete uma vez o cenário interrompido ou sujo. Os cliques são pela
acessibilidade (`AXPress`), que não move o mouse nem conta como entrada. **O
arraste da MESA conta**: o evento sintético zera o `HIDIdleTime` mesmo pelo
tap de sessão (medido em 08/10). Por isso «uma pessoa mexeu» é o tempo ocioso
menor que o tempo desde o último evento do próprio roteiro, conferido antes de
cada arraste e no fim.

## Preparar

Pré-requisitos: macOS com o Xcode (para o `xctrace`) e o `swiftc`; o
processo que roda o roteiro precisa da permissão de acessibilidade (o roteiro
não a pede nem a muda); o SEELE 0.15.1 em `/Applications`.

```sh
sh docs/evidencias/m3-custo-dos-mods/preparar.sh /tmp/m3
```

Ele copia o código do `54f542f` por `git archive`, põe os dois exemplos e os
compila em `--release` (cerca de 1 min 40 s num M5 Pro), compila o `ax`, baixa
os três pacotes e confere o hash. **O cache de pacotes é opcional:** sem ele, o
app baixa do indexador a cada cenário; com ele (`/tmp/m3/cache/mod-packages`,
copiado de um cenário anterior), a rede sai da medida.

## Rodar

```sh
python3 -I /tmp/m3/roteiro/m3.py 3-mods+voz      # um cenário
sh /tmp/m3/roteiro/campanha.sh                   # os dez, em série
```

Cada cenário escreve `medidas/<cenario>-<hora>/relato.json` e a tabela do
`xctrace` em XML (o trace é apagado depois de lido). O
servidor escuta na 8391: nunca a 8383 (a do app) nem a 8384 (a da bateria de
testes).

## Os arquivos

| arquivo | o que é |
|---|---|
| `preparar.sh` | monta a pasta de trabalho do zero |
| `campanha.sh` | os dez cenários em série |
| `m3.py` | um cenário: servidor, app, ações, medidas, relato |
| `ax.swift` | conduz a janela pela acessibilidade, só pelas janelas do app (nunca pela barra de menus do sistema), acha a peça da MESA numa captura da janela, e arrasta |
| `xctrace_tabela.py` | lê uma tabela exportada pelo `xctrace`, resolvendo `id`/`ref` |
| `sessao.swift` | diz se a sessão gráfica está bloqueada |
| `m3-servidor.rs` | o servidor da medição: N pacotes, instalados por hash, na porta pedida |
| `m3-falante.rs` | o falante sintético |
| `m3-pedido.rs` | a medida do R5 do PERFIS: um `view` e um `save` com o canal 0 (escopo de servidor) e com o canal 1, contra a metade de servidor publicada. Em 08/10, os quatro voltaram `ok: true` (rascunho da API 6, §8) |
| `medidas.json` | os relatos da campanha de base de 08/10 |
| `medidas-api6.json` | os 24 relatos da campanha da API 6, de 10/10 (commit `6e845e0d`) |
| `preparar-cartoes.sh` | a `carga-cartoes` na pasta de uma campanha já preparada, sem recompilar o app |
| `resumir-api6.py` | a tabela da campanha da API 6, contra o `sem-mod` dela, depois de conferir o commit, a interrupção e a compilação |
| `preparar-api6.sh` | monta a pasta de trabalho da campanha da API 6 |
| `remendar-api6.py` | os remendos da cópia da medição |
| `campanha-api6.sh` | os 24 cenários da campanha da API 6 |
| `gerar-carga.py` | as variantes do MOD de carga, com os GIFs da capa |
| `m3-carga/` | o modelo do MOD de carga sintético da API 6 |
| `conferir-carga.rs` | confere uma variante contra o prelúdio da cópia |
| `preparar-conserto-63.sh` | a `carga-cartoes` de novo, com o conserto da pendência 63 na cópia da campanha, que fica marcada `<commit>+6717dc2c` |
| `remendar-conserto-63.py` | o `tela-sessao.js` do `6717dc2c` na cópia, letra por letra, e o número da voz do falante |

## A campanha da API 6 (tarefa 11 do plano da F3)

A mesma medição, com três diferenças. Os cenários da base continuam rodando
como antes, com o app publicado.

**O app é uma build de medição, e não o publicado.** `preparar-api6.sh` copia
o código de um commit (o `HEAD` de quem chama, por padrão; a campanha de 10/10
passou o da décima segunda integração, `6e845e0d`, explícito) por
`git archive`, e `remendar-api6.py` muda **só a cópia**:

- a API 6 passa a ser aceita (`MOD_API_VERSION = 6`, `APIS_ACEITAS = [6, 5, 4,
  3]`, e as duas constantes da casca). No produto, ela só entra com o
  `v6.json` (tarefa 12);
- a casca escreve o jitter no `seele.log` a cada 2 s, o mesmo número da barra
  (`modId` `m3`, com balde próprio). Com o diálogo modal e com a página
  imersiva, a barra fica inerte, e um nó inerte sai da árvore de
  acessibilidade: a base não leu o jitter desses cenários. O remendo vale em
  todos os cenários desta campanha, para o custo dele ser o mesmo em todos;
- o `m3-servidor.rs` ganha o `api` do `EnabledMod` (migração 14).

O app sai de `cargo tauri build --bundles app`, com a assinatura ad-hoc da
configuração, em `<pasta>/alvo/release/bundle/macos/SEELE.app`. O commit fica
em `<pasta>/fonte/COMMIT_DA_MEDICAO`, e cada relato o leva.

**O MOD de carga sintético** (`m3-carga/`, gerado por `gerar-carga.py`) põe
cada ponto da API 6 no teto declarado, uma variante por cenário:

| variante | o que fica de pé |
|---|---|
| `regioes` | as oito regiões ao mesmo tempo, cada uma no teto do cartão (64 nós), e o acréscimo de `conversa.mensagem` em cada uma das 30 mensagens |
| `capas` | 7 capas animadas (60 quadros, 480 × 144, 9,6 s) na chamada e na barra, e 20 paradas, citadas pelo cartão na lista: 121,6 MB decodificados, 91 % do bolso de 128 MiB |
| `folhas` | três folhas empilhadas, montadas em 20 partes de 199 nós cada (3980 nós; uma mensagem tem 12 KiB) |
| `avisos` | um pedido a cada 250 ms, cada um com um `mundo.avisar` de 1 KiB (o teto: 4 por segundo, 1024 bytes) |
| `palco` | o `chamada.palco` com uma `tela` de 1024 × 768 e as figuras que cabem numa mensagem (155 completas) |
| `dialogo`, `pagina` | um diálogo modal e uma página imersiva, para o jitter sob eles |
| `tudo` | regiões, capas, folhas, avisos e o palco (escondido, para a grade das capas ficar à vista) |

Ele não está no indexador: a preparação confere cada variante contra o
prelúdio da cópia (`conferir-carga.rs`, no `Motor` de verdade, com as
capacidades da API 6) e a põe no cache por hash, onde o app a acha. **A
conferência achou dois defeitos antes da campanha**: o MOD chamava
`SeeleMods.snapshot()`, que a API 6 não tem (o retrato é `SeeleUI.retrato()`), e
teria parado na primeira linha; e o palco com 200 figuras completas passava
dos 12 KiB de uma mensagem. **O teto de 256 figuras só se alcança com figuras
sem `chave` e sem `cor`** (45 bytes cada): com elas completas (71 bytes),
cabem 155.

**Os participantes.** Nos cenários de carga, o `m3-falante --calado` sobe
participantes que não falam: um posta 30 mensagens no canal 1 (a região da
conversa); nas capas, a sala 1 fica com sete pessoas, contando quem usa e o
falante, e o servidor com vinte e sete. Quem usa entra na sala nas capas e no
palco, mesmo sem voz.

**Os cenários novos**, além dos dez da base:

| cenário | o que muda |
|---|---|
| `3-mods-mesa-continuo` | a peça vai e volta em arrastes de 1,9 s seguidos, sem a pausa do laço: na base, o arraste ocupava 1,5 s de cada ~2 s |
| `carga-<variante>` | o MOD de carga; o relato diz o que ele pôs à vista (`carga_a_vista`, pela acessibilidade) e o que o `seele.log` diz dele (`tropecos_do_mod`) |

O jitter vai em `jitter_ms` (a barra, como na base) e em
`jitter_do_registro_ms` (o `seele.log`), que é o que vale para o diálogo e a
página.

**A casa nova pergunta o nome.** Na 0.16, o app de uma pasta nova abre «COMO OS
OUTROS VÃO TE CHAMAR?» antes de seguir o `--entrar` (medido pela orquestradora
em 10/10, no primeiro cenário da campanha: «a entrada não apareceu»). O
`m3.py` espera também esse diálogo, aperta CANCELAR — a chegada segue com o
nome tirado da chave, como quem pula a pergunta — e o relato leva
`"apelido": "pulado"`. Na base o diálogo não aparece, e nada muda nela.

**Contra o que se lê o custo da API 6: o `sem-mod` da 0.16, e não a base.** Ao
abrir, a 0.16 sobe a sala pessoal (um servidor em `[::]:8385`, a escada do
alcance e o registro no ponto de encontro, ENC/2), e a 0.15.1 da base não tinha
nada disso. O `sem-mod` da 0.16 já carrega esse custo, então comparar um
cenário de MOD da 0.16 com a base mistura a API 6 com a sala pessoal. Cada
cenário desta campanha é lido contra o `sem-mod` (e o `sem-mod+voz`) **desta
mesma campanha**; a comparação com a base fica só para o custo total de subir
de versão, e é dita como tal.

**O diálogo e a página abrem quando quem usa já está numa sala** (a
orquestradora, 10/10). Abertos na carga do MOD, o diálogo modal e a página
imersiva deixavam inerte o resto da janela, e o roteiro não achava mais o
ENTRAR NA SALA: o `carga-dialogo+voz` e o `carga-pagina+voz` falharam com «não
entrou na sala» (em `medidas/descartados/dialogo-aberto-na-carga/`). O molde
ganhou `quandoNaSala(abrir)`, que abre quando o `SeeleUI.retrato()` mostra
uma sala `minha`, olhando de novo a cada evento `sala`. **A função entra em
toda variante, e por isso muda o hash de todas**, mas só as duas a chamam: as
outras seis (`regioes`, `capas`, `folhas`, `avisos`, `palco`, `tudo`) foram
medidas com o molde anterior, e o comportamento delas é o mesmo. As novas, na
campanha: o `dialogo` como `c005c3fb…`, e a `pagina` como `dd585396…`; as
antigas ficaram em `/tmp/m3-6/pacotes-antigos/`.

**A `carga-cartoes`, a lista de 251 cartões** (pedida pela orquestradora em
10/10, depois da campanha: o teto de contribuições da 6 esperava a medida da
lista, e nenhuma variante a fazia). A variante `cartoes` do molde registra um
`pessoa.cartao` no teto do cartão (64 nós, `substituir`) para cada pessoa do
retrato, de novo a cada evento `pessoas`, e as 128 contribuições sem alvo do
teto agregado, cada uma no teto do cartão, em seis pontos. Conferida contra o
prelúdio da 0.16 (`conferir-carga`): 155 contribuições com o retrato de 27 do
conferente, maior mensagem de 2,3 KiB, nenhum tropeço. Ela roda **na mesma
build e na mesma pasta** dos outros cenários, sem recompilar o app, para ser
lida contra o mesmo `sem-mod`: `preparar-cartoes.sh` troca o roteiro (só se
a cópia de trabalho for a da campanha), recompila só o exemplo do falante,
refaz o `ax` e põe **só a variante nova** no cache — as outras ficam como
foram medidas, e o molde mudado não as alcança.

- **Como os 250 calados entram: no ritmo da portaria, e não com a taxa
  folgada.** O `taxa.rs` guarda 30 apertos de rajada por endereço, repostos a
  30 por minuto, e todos saem de 127.0.0.1, como o app. Folgar a taxa pedia
  remendar o servidor da medição, e o servidor deixaria de ser o dos outros
  cenários. O `m3-falante --calados <n>` sobe os `n` num processo só (num
  processo por participante, 250 pesariam na máquina medida): 28 de rajada,
  um a cada 2,05 s depois, tentando de novo quando recusado, cada um drenando o
  que chega. São uns 7,6 minutos antes do app (medido com 40: 25 s, nenhuma
  recusa, nenhuma queda, 40 pessoas no banco). Uma queda vai ao `.out` com o
  motivo, e um `FellBehind` não passa calado.
- **O primeiro `carga-cartoes` falhou, e o meu ensaio não tinha como pegar**
  (medido pela orquestradora, 10/10; `medidas/descartados/cartoes-portaria/`).
  Com o servidor exigindo o MOD de carga, cada calado gastava **dois apertos**:
  a recusa `ModsNaoAceitos` e a volta com o sim. A rajada de 28 acabou no 15º,
  o ritmo de 2,05 s não repunha dois por calado, e o `m3-cartao-015` esgotou as
  tentativas. **O ensaio com 40 foi contra um `m3-servidor` sem MOD**, em que
  cada calado gasta um aperto só: ele provou o ritmo, e não o ritmo com o
  aceite, que é o caso da campanha. O conserto, da orquestradora: o conjunto
  aceito na primeira recusa vai na primeira tentativa de todo calado seguinte,
  e só o primeiro paga dois. Conferido por ela contra o `m3-servidor` **com** o
  `m3-carga-cartoes`: 40 em 25 s, nenhuma recusa.
- **Um calado que não entra diz e segue** (decidido em 10/10, depois da
  campanha): antes, o `Err` derrubava o processo e os 250 sumiam, e o roteiro
  esperava 15 minutos por um «prontos» que não vinha. Agora ele vai ao `.out`
  com o motivo, os outros ficam, e a linha final diz quantos entraram de quantos
  (`prontos=<entraram> de <n>`); o roteiro não mede sem os 251 na lista, então a
  medida continua protegida, e a falha diz quantos faltam. **A campanha que deu
  números não usou o `--calados`**, e por isso o que ela mediu não muda; só a
  `carga-cartoes` o usa, e a de 10/10 rodou a versão anterior, que aborta.
- **O cenário só mede com a lista inteira**: o roteiro conta pela
  acessibilidade os cartões distintos (`cartão <apelido> · linha 1`) e só
  segue com os 251; o relato leva `cartoes_na_lista`. O `carga_a_vista` leva o
  primeiro cartão, o último e a última das 128 sem alvo.
- **A fundura da acessibilidade**: o `ax textos` passou a aceitar a fundura
  (60, como sempre, quando não dita). A lista e a linha de uma contribuição
  dentro de uma mensagem são procuradas a 200. No `carga-regioes`, o relato
  ganha `conversa.mensagem · linha 1 (fundura 200)`, para «a região montou»
  ser medida, e não lida do custo em MiB.

**O jitter se lê só nos cenários com voz.** Sem falante não há fonte, e o
`jitter_do_registro_ms` fica no zero inicial: nos cenários sem voz, o zero não
é medida.

**O falante mandava voz que a 0.16 jogava fora** (achado da F4 e medido pela
orquestradora, 10/10). Os dois leitores do jitter, `jitter_ms` e
`jitter_do_registro_ms`, davam **0 exato** em todo cenário com voz, e a base,
com o mesmo falante, dava mediana de 1 ms. A causa: o `m3-falante.rs` grava no
cabeçalho de mídia o `PROTOCOL_VERSION` (9), que era o número da voz na v0.15.1;
desde o `f3aaf9ce`, a voz tem número próprio, o `MEDIA_HEADER_VERSION` (1), e o
`decode` recusa outro. **Nos sete primeiros cenários com voz, quem ouve jogou
fora todo datagrama do falante**: nada foi decodificado nem misturado, e os
+4,6 pontos de CPU eram só a captura e a supressão. A base mediu certo, porque
lá o número era o mesmo nas duas pontas.

O conserto é o item 4 do `remendar-api6.py`, e não uma troca no arquivo: o
mesmo `m3-falante.rs` continua compilando para a base, onde a constante nova
não existe. Conferido à mão pela orquestradora: com ele, o JITTER foi a 1 ms, e
o do registro a 1,06–1,58 ms, como na base. Os sete cenários com voz feitos
antes do conserto estão em `medidas/descartados/falante-com-versao-9/` (com o
`carga-capas` interrompido), e foram refeitos com o falante consertado: **os
doze cenários com voz da campanha mediram o jitter**, e ele vale neles.

**A árvore de acessibilidade do WebKit, na 0.16, às vezes só se monta depois da
primeira consulta** (a orquestradora, 10/10). O `esperar` do `m3.py` repete a
consulta até o prazo, e por isso já cobre o caso; uma ferramenta que consulte
uma vez só leria uma tela vazia.

**A `carga-cartoes` travou a página, e a causa é do produto, não do MOD**
(10/10). Com os 251, o WebContent passou de 100% de CPU por 4,5 min, a memória
subiu de 1,13 para 1,33 GB, e nenhum cartão nem o agregado chegou à tela; com
10 pessoas, chegaram na hora. Pela leitura: cada sessão do servidor manda um
`PersonState` por segundo (`TELEMETRY_INTERVAL`, `session.rs:175`) a **todas**
as conexões (`session.rs:5404`); cada um marca `changed.roster`
(`seele-sessao/src/state.rs:1169`), que vira um `Event::RosterChanged` sem
junção nenhuma (`seele-ffi/src/lib.rs:5870`, `notify`); o app o emite tal qual
(`apps/seele-app/src/main.rs:906`), e a janela responde a cada um com um
`atualizar()` (`tela-sessao.js:4973`) — o retrato inteiro pela ponte e a lista
inteira redesenhada. Com N pessoas, N redesenhos completos por segundo, cada
um do tamanho de N: o custo cresce com o quadrado.

Medido com `tools/lista-grande.cjs`, com os eventos
vindos de fora da página e no ritmo do servidor (5 s; o Chromium do Playwright,
não o WebKit, e a ponte simulada por JSON — a de verdade custa mais):

| pessoas | MOD | um evento, um `atualizar()` (hoje) | no máximo um por quadro | só o tique de 500 ms |
|---|---|---|---|---|
| 251 | cartões + 128 | **fila de 4,9 s em 5 s**, 68 quadros em 9,9 s | 99%, 88 quadros, sem fila | 14%, 548 quadros |
| 251 | só os cartões | **fila de 3,5 s**, 64 quadros em 8,5 s | 100%, 134 quadros | 9%, 578 quadros |
| 251 | só as 128 sem alvo | 98%, 98 quadros, sem fila | 100%, 188 quadros | — |
| 251 | sem | 99%, 232–377 quadros, sem fila | 98%, 557 quadros | 6%, 598 quadros |
| 200 | só os cartões | 99%, 67 quadros, fila de 0,1 s | — | — |
| 100 | só os cartões | 100%, 325 quadros | — | — |
| 100 | sem | 42%, 600 quadros | — | — |

(600 quadros em 5 s é a página livre.) **Depois do conserto** (`6717dc2c`: no
máximo um `atualizar()` por quadro, na janela; guarda em
`apps/seele-app/bancada/um-redesenho-por-quadro.cjs`), medido pelo ouvinte de
verdade: com 251, os cartões e as 128, fila de 0,1 s e a página a 99%, 93
quadros — o mesmo da coluna do meio. A fila não esvazia enquanto os eventos
chegarem, e eles chegam enquanto houver sessão: é a CPU presa e a memória
subindo da M3. Juntar os eventos por quadro tira a fila e não tira o custo —
cada quadro ainda refaz a lista inteira. Que o cartão não tenha aparecido na
M3 é coerente com isso (o registro do MOD esperando atrás da fila), mas não
foi medido.

**Para a `carga-cartoes` depois do conserto**: sem MOD nenhum, 251 pessoas
ainda deixam a página a 98% — cada quadro refaz a lista inteira, e redesenhar
só a linha que mudou ficou para depois (pendência 63). Contra o `sem-mod` de
uma pessoa, a lista entraria na conta do MOD.

**Para medir de novo** (decisão da orquestradora, 10/10: com 251, na build da
M3 remendada com o conserto, e não com menos pessoas):

```sh
sh docs/evidencias/m3-custo-dos-mods/preparar-conserto-63.sh /tmp/m3-6
sh /tmp/m3-6/roteiro/campanha-api6.sh sem-mod sem-mod+voz carga-cartoes carga-cartoes+voz
```

O remendo é o diff do `6717dc2c`, conferido linha por linha contra o
`tela-sessao.js` do `6e845e0d`, e a bancada `um-redesenho-por-quadro.cjs`
reprova a cópia sem ele (251 retratos) e passa com ele (73 ms de atraso
máximo). O app medido nos 24 fica guardado ao lado (`SEELE-6e845e0d.app`), e o
`COMMIT_DA_MEDICAO` marcado faz o `resumir-api6.py` recusar ler a
`carga-cartoes` nova contra o `sem-mod` velho: as referências são medidas de
novo, na mesma build.

**O teto de 256 alvos por ponto não congela sobre isto.** Com os cartões, a
fila aparece a partir de 200 pessoas, abaixo do teto; e sem MOD nenhum, com
251, a página já passa o tempo todo redesenhando (99%), só que sem fila. O dono
do conserto (juntar o `RosterChanged`, redesenhar só a linha que mudou) está
por definir; o pedido está com a orquestradora.

```sh
sh docs/evidencias/m3-custo-dos-mods/preparar-api6.sh /tmp/m3-6
sh /tmp/m3-6/roteiro/campanha-api6.sh        # os 24
```

A preparação compila o app inteiro em release numa pasta de alvo nova (ver o
tempo no fim do `preparar-api6.sh`).

## O que este roteiro não mede

- **Windows (WebView2) e iPhone.** O `footprint` e o `xctrace` são do macOS. No
  Windows o equivalente é o `Get-Process` (working set privado) e o
  `PresentMon`; no iPhone, o Instruments com o aparelho. Pedidos à orquestradora.
- **A voz ouvida.** O jitter é o que a telemetria do app calcula; ninguém
  ouviu o som nos cenários.
- **Duas pessoas.** O app medido é o único humano da sala; o falante é um
  cliente do `seele-core` sem janela.

## Um achado da preparação

O exemplo `crates/seele-server/examples/servidor-com-mod.rs`, de onde este
servidor partiu, **instala o pacote por identificador** (`mundo/mods/<id>`), e
o servidor o procura **por hash** (`RaizesDosMods::pacote_de`,
`crates/seele-server/src/lib.rs:137-138`, `<pacotes>/mod-packages/<hash>`).
Medido: com o pacote instalado como o exemplo instala, a janela aceita os MODs,
o executor os ativa (`MOD ativado no executor nativo`), e a metade de servidor
recusa todo pedido com `MOD request refused … No such file or directory (os
error 2)`. Na tela, nenhum MOD desenha nada, e a aba DIAGNÓSTICO diz «nenhum MOD
usa este lugar agora» nos onze lugares. O `m3-servidor.rs` instala por hash.
**Consertado no exemplo em 10/10** (tarefa 15 da F3, `18043d51`): ele instala
por hash, pela mesma função que `crates/seele-server/tests/exemplo_com_mod.rs`
usa, e escuta na 8390, nunca na 8384.
