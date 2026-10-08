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
- **A orquestradora aprovou o `lotes.md` (`46f227a`)**, ponto a ponto.
  - **Dívida com dono:** fechar o app hospedando (a metade casca do P50-4) fica sem aviso até o Plano 2 chamar a `Hospedagem::despedir` que o G-servidor cria. O dono da dívida é o Plano 2.
  - **E / P51-11:** a F3 confirmou que a API 6 mantém a capacidade `volume` (ADR 0048). O P51-11 tem objeto, e a pergunta da onda 3 está respondida.
  - **`sementes_do_fuzz.rs`:** a F1 regenera as sementes quando o número do protocolo subir para 9. O M1 mexe só no trecho do `datagrama_do_encontro`, e a orquestradora resolve o encontro na integração.
- **Passo 2, a onda 1:** C · H · K2 · M1 · ferramentas · P54, em paralelo, cada um num worktree isolado e num branch `0.16/lotes-<id>` a partir de `0.16/lotes`. O `target/` de cada worktree é semeado por clone APFS do `target/debug` do worktree `abertos-da-1.0`, que tem o mesmo `Cargo.lock` (cerca de 41 s por clone).
- **P54 → integrado** (`fbeff8e..2598b06`, merge `90e1377`), em 1 rodada. A pendência 54 fecha em `0d68c95`.
  - A bancada `qualidade-da-voz.cjs` ficou vermelha antes do conserto e verde depois.
  - O revisor refez a reversão pelos dois lados, captura e saída, e aprovou sem crítico nem importante.
  - Ficaram quatro menores: a linha do padrão repete o nome que a linha PADRÃO já diz, para o dono conferir em tela; duas cenas a mais; uma asserção redundante; dois comentários.
- **H → integrado** (`fbeff8e..7c9366c`, merge `8d28d5d`), em 2 rodadas.
  - **Commits que fecham os itens:**
    - P51-03: `4807207`;
    - P51-02: `663363d`;
    - K1-m1: `b570f86`;
    - conserto da revisão: `4205d29`, `1c199b9` e `47d2f98`.
  - **A revisão achou um Importante mandado pelo plano.** A frase de `ServidorTrocouNaVolta` que o plano ditou era falsa por três motivos:
    - o «nada» não vale, porque o ClientHello sai;
    - o «convite» e o «link» não valem para quem volta pela lista;
    - o remédio está errado para o `ChaveNaoProvada` da volta.
  - **Decisão:** a frase passou a ser «QUEM ATENDEU NA VOLTA NÃO É O SERVIDOR DESTA SESSÃO.\nO convite, a senha e o apelido não chegaram a ele. Confirme com quem hospeda, por outro canal, antes de entrar de novo.». Um guarda em `frontend.rs` a prende. O `lotes.md` foi corrigido.
  - **Menores da mesma rodada:**
    - o `ChaveNaoProvada` da corrida sai como WARN;
    - três reversões que faltavam;
    - a mensagem de um teste;
    - um guarda de fonte que prende `drive` a `fim_da_sessao`.
  - **A re-revisão** resolveu os seis achados, sem quebra nova. Nenhum assunto casa com as cadeias do G1.
