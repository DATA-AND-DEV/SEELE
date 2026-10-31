# API 8, revisão final — decisões dos consertos do lado do SEELE

Sobre `desenho/api-8-revisao` a partir de `5d053dbd`. A revisão é
`revisao-final-api8.md` (I1–I10, M1–M13); aqui ficam só os achados do lado do
SEELE (o I2, o I10, o M1, o M10, o M11 e o M12 são do MOD de multiplayer).
Spec: `docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`.
Conferido em 31/10/2026 pelo `curl` do CLAUDE.md: a mais nova é a v0.18.3, e
a v8 (protocolo 12) segue sem release.

Cada decisão no formato `Ruling: <o quê> — <por quê> — <custo se errada>`.

## Decisões

- **Ruling R1 (I1): o que roda é o que foi aceito, por caminhos presos pela
  política, sem estado.** A política prende o comando (a pasta de cima
  inteira quando ele mora numa — `java/` para `java/bin/java` —, ou o arquivo
  com e sem `.exe` quando mora na raiz) e cada argumento que é caminho
  (`server.jar`; `@arquivo` sem a arroba; o que começa com `-` não). Uma
  escrita que não traz os bytes de um artefato aceito — `baixar` pela segunda
  porta com um hash que não é o de um artefato, `escrever`, `trazer`,
  `empacotar`, `receberCopia`, `desempacotar` de um pacote cujos bytes não são
  os de um artefato — não cai sobre um preso, dentro de uma pasta presa, nem
  sobre uma pasta que contém um (por cima, cada entrada é conferida antes de
  qualquer uma entrar); a recusa é `reservado-ao-aceite`, com `caminho` e
  `reservado`. A comparação é pelo caminho real (link dentro da raiz), sem
  caixa e sem os pontos e espaços do fim. O pacote é «aceito» pelos bytes
  (tamanho + hash do mesmo algoritmo de um artefato desta máquina), e não por
  quem o baixou. **E o `rodar` só roda na raiz** (`pasta-fora-do-aceite`): os
  argumentos se leem a partir da pasta, e uma pasta livre rodaria o
  `server.jar` que o MOD escreveu nela. — Por que não «o `rodar` confere a
  proveniência»: a proveniência é estado, e um estado da memória não
  atravessa uma reabertura do app (um `server.jar` plantado na sessão 1 rodaria
  na 2); re-hashear o comando antes de `rodar` não serve ao Java aberto de um
  pacote (a tela mostra o hash do pacote, não o de `bin/java`), nem ao
  `fabric-server-launch.jar` que o instalador cria. A regra pela política vale
  na primeira escrita, em qualquer ordem e depois de reabrir. — Custo: (a) uma
  receita cujo comando mora numa pasta que também guarda dados (o mundo, os
  mods) não deixa a metade escrever ali: o runtime tem de ter a pasta dele; (b)
  um argumento aceito que é uma pasta (`--world world`) prende a pasta inteira
  contra troca; (c) o que o aceite não nomeia segue livre pela decisão Q1 — os
  `mods/*.jar` de um pack e as bibliotecas que um `@unix_args.txt` lista são
  código Java que o Java aceito carrega (é o I2, a tela do MOD); (d) o
  processo do jogo, que já é o código aceito, escreve onde quiser; (e) nomes
  curtos do Windows (`JAVA~1`) e formas Unicode diferentes de um nome não-ASCII
  só se resolvem quando o caminho já existe (o `canonicalize`); (f) a
  `pasta` de `rodar` deixa de ser uma escolha: um MOD que rodava numa
  subpasta passa a ser recusado (nenhum dos de referência roda).
