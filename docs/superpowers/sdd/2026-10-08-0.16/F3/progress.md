# F3 — a API 6 de MODs: andamento

Plano: `docs/superpowers/plans/2026-10-08-0.16-F3-api-6.md`. Rascunho:
`docs/superpowers/specs/2026-10-08-api-6-rascunho.md` (com o `.json` ao lado).
Spec: `docs/superpowers/specs/2026-10-08-0.16-design.md` (§2 F3 e F4, §3).
Branch da frente: `0.16/api-6`, a partir de `versao/0.16`.

## 08/10

- **Publicado, conferido pela API de releases:** `v0.15.1 · 54f542f94 ·
  2026-10-08T04:14:07Z`. O `/Applications/SEELE.app` desta máquina é a 0.15.1.
  O código de `versao/0.16` é o do `54f542f` (só entraram documentos).
- **Nota:** o pedido dizia que a `versao/0.16` estava em `c0337b2`. O branch
  saiu dela em `d4eb6f1` (o achado do ESTILO) e andou por `--ff-only` até
  `92e0d79` (D-nucleo-1, D-capa-1, D-estilo-1), antes do primeiro commit.
- **Etapa desta sessão: documentos, medição e requisitos, sem código de
  produto.** O código espera os lotes C, D, E, K2, K3, K4 e K5 da F5.
- **O rascunho da API 6**: regiões e modos, `pessoa.capa`, camadas, retrato
  projetado, eventos e momentos, capacidades, o celular, os limites, e o que a
  v5 tinha e não listava. Com o que entrou, o que não entrou e por quê das três
  revisões (PERFIS R1–R13, MESA 1–18, ESTILO §1–§8) e da seção «Para a F3» do
  `lotes.md` da F5.
  - Ruling: `SeeleMods.snapshot` sai na 6 e entra `SeeleUI.retrato()`, por
    lista branca; `falando` entra porque o indicador de quem fala não é núcleo
    (29/09) — custo se errado: um campo a tirar antes do congelamento.
  - Ruling: a v6 é escrita por inteiro, sem `extends`, e nomeia o que o runtime
    da metade de servidor expõe (`dados`, `arquivos.*`, `volume.*`, `mundo.*`)
    — custo se errado: nenhum no produto; é o que o MOD escreve.
  - Ruling: `contexto.sala` no pedido feito de `chamada.palco`, em vez de
    amarrar sala a canal na API — custo se errado: um campo de contexto.
  - Ruling: o palco é por pessoa — custo se errado: um palco compartilhado
    viraria estado de servidor, que o MOD já tem.
  - Recomendações para o dono, no rascunho §18: a `folha` substitui o `painel`;
    `regiao` e `cartoes` saem; `tema` fica, com `servidor.aparencia` como porta;
    `repetir: true` por contrato.
  - **Três candidatos ao contrato de fio** (rascunho §14), para a F1 e a
    orquestradora: o aviso da metade de servidor às janelas (recomendo na 1.0),
    os pedidos endereçados ao servidor e o tema como dado do servidor.
- **Respondido à F5:** a API 6 mantém a capacidade `volume`. O lote E (P51-11)
  tem objeto.
- **Achados novos, pela leitura do código** (rascunho §13):
  - os 13 verbos de `actions` e as 19 leituras de `reads` da v1 não são
    expostos à metade de servidor: nenhum desses nomes aparece no código fora
    de `api/`. O `check-api` passa porque confere o símbolo da direita;
  - os nomes de `own` da v1 (`guardar`, `lerArquivo`) não são os que o MOD
    escreve (`dados`, `arquivos.ler`);
  - a forma `midia` numa superfície conta contra o teto da região (10 MiB), e
    não contra os 24 MiB que a v5 promete (`mods-regiao.js:2159-2161`, `:2226`).
- **Achado medido:** o exemplo `crates/seele-server/examples/servidor-com-mod.rs`
  instala o pacote por identificador, e o servidor o procura por hash
  (`RaizesDosMods::pacote_de`). Medido na M3: a metade de servidor recusa todo
  pedido com «No such file or directory», os MODs ativam na janela e não
  desenham nada. É o guia de quem escreve MOD; levado à orquestradora.
- **O esqueleto do ADR 0056** e a linha dele no índice. `cargo test -p xtask
  --test adr_index`: verde com a linha; vermelho sem ela
  (`docs/adr/ holds ADRs the index never links: ["0056"]`).
- **O plano**, com as 14 tarefas do levantamento. A 1 está feita; da 2 à 13
  esperam a liberação; a 14 é das sessões de MOD.
- **A M3 de base**, na v0.15.1 publicada, sem mudar o binário: roteiro em
  `docs/evidencias/m3-custo-dos-mods/`, números no ADR 0056.
  - Ruling: a campanha só roda com a máquina ociosa há 5 min e sem compilação de
    outra sessão (menos de 2 núcleos ocupados), porque a janela precisa estar no
    palco do Stage Manager e um `rustc` em paralelo suja CPU e quadro — custo se
    errado: a campanha demora mais.
  - Ruling: o tempo de quadro sai dos commits de Core Animation do `seele-app`
    (`xctrace`, instrumento «Core Animation Commits», 30 s), e não de um
    `requestAnimationFrame` injetado, porque injetar exigiria mudar o binário —
    custo se errado: a medida é do commit do quadro, e não do laço da página.
  - Medido no caminho: os instrumentos de engasgo (Hitches, Frame Lifetimes)
    deram 526 MB em 60 s, e o `xctrace` ficou mais de 10 min salvando a 99 % de
    CPU; o primeiro cenário falhou por isso e foi descartado. O «Core Animation
    Commits» grava 10 s em 19 MB e salva em 3 s.
- **A M3 de base, parcial: 4 dos 10 cenários**, parada às 14:05 a pedido da
  orquestradora (a integração vai rodar a bateria). Números no ADR 0056.
  - Medido: de nenhum para três MODs com a UI fechada, o total sobe de ~194
    para ~252 MiB, quase todo no WebContent (~56 → ~106); a CPU de repouso vai
    de ~5 % para ~9 % de um núcleo; nenhum commit passou de um quadro.
  - O editor do PERFIS aberto soma ~34 MiB no WebContent e leva a CPU a ~12 %,
    numa rodada com entrada de uma pessoa durante a janela (indicativa).
  - As rodadas sem MOD e com 1 MOD tiveram `rustc` de outra sessão na janela;
    as duas de cada cenário concordam em 4 MiB e 0,6 ponto de CPU.
  - **Falta, com o mesmo roteiro, quando liberado:** `3-mods-editor` limpo,
    `3-mods-mesa` e os cinco `+voz`. O Windows e o iPhone ficam com a
    orquestradora.
- **Respondido à orquestradora:** o retrato da 6 leva só quem está presente; os
  membros fora do ar ficam de fora, como recomendação ao dono (rascunho §6).
  Não segura a P6 da F1.
- **Incorporados ao rascunho:** o spike da F4 (um contrato e um motor só, nada
  fora de uma volta do anfitrião, os mesmos limites nos dois executores) e, no
  plano, o guarda do indexador pela união das linhas (spec-mestre `b9817abc`).
