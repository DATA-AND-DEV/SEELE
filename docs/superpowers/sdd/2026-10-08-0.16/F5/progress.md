# F5 — os lotes abertos e as pendências: andamento

Plano: `docs/superpowers/sdd/2026-10-08-0.16/F5/lotes.md`. Spec: `docs/superpowers/specs/2026-10-08-0.16-design.md` (§2 F5, §4).
Branch da frente: `0.16/lotes`, a partir de `versao/0.16` em `d4eb6f1`. Cada lote em `0.16/lotes-<id>`, num worktree próprio, e entra por `git merge --no-ff` depois de revisado.

## 08/10

- **Publicado, conferido pela API de releases:** `v0.15.1 · 54f542f94 · 2026-10-08T04:14:07Z`. A `versao/0.16` (`d4eb6f1`) contém o `54f542f`.
- **Nota:** o pedido dizia que a `versao/0.16` estava em `c0337b2`. Ela está em `d4eb6f1`, um commit de docs à frente (o achado do ESTILO no spec-mestre). A frente parte de `d4eb6f1`.
- **Passo 1, o `lotes.md` reescrito.** Cinco leitores só-leitura conferiram as âncoras de todos os lotes contra `d4eb6f1`. Nenhum passo de nenhum lote estava feito.
  - Ruling: o K2 sai **sem** o filtro `cartao`, como decidido em 08/10 e como a pendência 49 (item 5) propõe; a mesma mídia de cartão aparece em dois blocos da aba, que não os soma — custo se errado: um filtro de uma linha a pôr de volta.
  - Ruling: o K2 reserva os bytes **antes** do `decode()` e os devolve na recusa, em vez de somar só depois — somar depois abriria uma corrida no teto — custo se errado: nenhum no produto.
  - Ruling: o retrato continua sem falar com o MOD; um evento novo para ele vai para a v6 — custo se errado: o MOD do retrato segue sem saber da imagem que não decodificou, como hoje.
  - Ruling: o G-servidor **não** troca a frase de `ServerShuttingDown`, porque ela também nomeia a recusa por falha de banco — custo se errado: a tela de fim diz «O SERVIDOR ESTÁ ENCERRANDO» em vez de dizer que quem hospeda fechou.
  - Ruling: o `Event` novo do G-servidor leva a razão (`ServidorEncerrando { reason }`), para o Plano 2 mandar `ChamadaEncerrada` pela mesma via — custo se errado: um campo a mais.
  - Ruling: o F não toca a hospedagem; o P50-4 da casca vai com o G-casca, no Plano 2, e até lá fechar o app hospedando continua sem aviso — custo se errado: a 0.16 sairia sem esse aviso se o Plano 2 atrasar.
  - Ruling: o I é redesenhado para a D-52 — a abertura na sala, numa thread própria com prazo, os controles guardados em `Shared`, e uma medida da reconexão antes de escolher o gatilho — custo se errado: o lote I fica maior; o plano diz onde dividi-lo.
  - Ruling: a frase do J sem nome derivado («um nome tirado da sua chave»), porque a janela não conhece o `pessoa-…` e o lote não mexe no Rust do apelido — custo se errado: a frase é menos concreta.
  - Ruling: o M1 faz `Bilhete::novo("")` recusar junto com o `separar_ponto("")`, para que os docs de `Bilhete::ponto` e de `com_porta_escrita` continuem verdadeiros — custo se errado: um link com ponto vazio passa a ser recusado também na montagem, o que `Bilhete::ler` já fazia.
  - Ruling: o C põe também o `som_servido_dito` no teto de vazão, e espelha os três braços da metade servidor do P51-26 nas duas linhas de `mod_request` — custo se errado: duas portas a mais com teto.
  - Ruling: o E pergunta à orquestradora, antes da onda 3, se a v6 mantém o `volume`; sem ele, o P51-11 sai — custo se errado: um conserto de registro num comando que a F3 tiraria.
  - Ruling: o Z registra também o fechamento das pendências 52, 53 e 54, porque nenhum lote de código edita `docs/pendencias.md` — custo se errado: nenhum.
  - Ruling: o M2 troca só a oração do quarto no parágrafo dos 60 s de `docs/ponto-de-encontro.md`, que é a cadeia do Lote C no G1; a frase fica mais precisa e não ganha cadeia — custo se errado: o dono pode querer o parágrafo intocado.
