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
- **com a máquina ociosa há 5 minutos** (`HIDIdleTime`).

Qualquer entrada durante o cenário o marca `"interrompido": true`, e um `rustc`
visto durante a janela conta em `"rustc_durante"`; a campanha repete uma vez o
cenário interrompido ou sujo. Os cliques são pela acessibilidade (`AXPress`), que
não move o mouse; o arraste da MESA usa o tap de sessão, que não conta como
entrada no `HIDIdleTime`.

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
| `m3-servidor.rs` | o servidor da medição: N pacotes, instalados por hash, na porta pedida |
| `m3-falante.rs` | o falante sintético |
| `medidas.json` | os relatos da campanha de base de 08/10 |

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
