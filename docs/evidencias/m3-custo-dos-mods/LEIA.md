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
| `preparar-api6.sh` | monta a pasta de trabalho da campanha da API 6 |
| `remendar-api6.py` | os remendos da cópia da medição |
| `campanha-api6.sh` | os 24 cenários da campanha da API 6 |
| `gerar-carga.py` | as variantes do MOD de carga, com os GIFs da capa |
| `m3-carga/` | o modelo do MOD de carga sintético da API 6 |
| `conferir-carga.rs` | confere uma variante contra o prelúdio da cópia |

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
`carga-capas` interrompido), e são refeitos. **Até os refeitos saírem
válidos, o jitter sob carga fica no §12 do rascunho e no `v6.json` como «não
medido nesta campanha»**, e não como zero; CPU, memória e quadros dos cenários
sem voz valem.

**A árvore de acessibilidade do WebKit, na 0.16, às vezes só se monta depois da
primeira consulta** (a orquestradora, 10/10). O `esperar` do `m3.py` repete a
consulta até o prazo, e por isso já cobre o caso; uma ferramenta que consulte
uma vez só leria uma tela vazia.

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
