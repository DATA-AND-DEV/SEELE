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
- **Ferramentas → integrado** (`fbeff8e..184697b`, merge `84c142e`), em 2 rodadas.
  - **Commits que fecham cada item:**
    - P51-15: `1345e0e`;
    - P51-21: `d7ecb3e`;
    - L1-m2: `19e04ce`;
    - L1-m3: `f9c2a94` (o lock é o do `1277d85` da F1, e a diferença entre os dois sai vazia) e `92783b5`;
    - L1-m4: `02c720d`.
  - **A primeira revisão aprovou com seis menores, e todos foram consertados antes do merge.** Quatro são os padrões que o `CLAUDE.md` diz terem custado mais caro:
    - o teste do L1-m2 casava com o próprio dado;
    - o `publicar.sh` calava a falha do git;
    - o `relatar` saía com falha calada;
    - os guardas do `ci.yml` aceitavam a forma errada.
  - **Os outros dois menores:** os comentários com história não medida, e o `warn` no upload.
  - **A re-revisão deu os seis por resolvidos.** Ficam como menores adiados, da mesma família e fora do diff:
    - o `changelog_dos_commits` ainda cala a falha do git;
    - a conferência dos alvos do fuzz lê o `ci.yml` inteiro;
    - o guarda não olha o `continue-on-error` do passo `--locked`.
  - **Nota para o plano:** a forma opcional do fuzz que o brief sugeria (`fuzz run uri fuzz/sementes/uri`) grava nas sementes versionadas. A forma certa é a do CI, com `fuzz/corpus/<alvo>` na frente.
- **K2 → integrado** (`fbeff8e..f565a0b`, merge na `0.16/lotes`), em 2 rodadas.
  - **Commits:**
    - P51-07: `75fb60f`;
    - P49-5: `75fb60f` e `82fd154`;
    - K1-m2: `677f973`;
    - consertos da revisão: `73866cc`, `86d0a2d`, `af7d6ef`, `6ca4fdf` e `f565a0b`.
  - **Primeira revisão:** aprovou, com quatro menores, e três deles foram consertados antes do merge:
    - o guarda de `podeFalar()` depois do `decode()` ganhou prova;
    - o passo de aplicar saiu do caminho de recusa, com `.then(aplicar, recusar)` nos quatro caminhos;
    - o «API 3» saiu do texto novo.
  - **Decisão do controlador:** o tropeço do passo de aplicar vai também ao `seele.log`, com o id do MOD, e não só ao `console` da janela.
  - **A prova do desenho do bloco** («a região e as páginas de cada MOD») vai para o K3, que é o lote que edita o `diagnostico-de-mods.cjs`.
  - **Para a v6:**
    - o `fundo-falhou` não diz qual fonte falhou;
    - o retrato que fala com o MOD, e o papel errado do fundo que diz `falhou`, ficam anotados.
- **M1 → integrado** (`fbeff8e..e48b1b4`, merge `64abdbf`), em 3 rodadas. Os 17 commits de entrega e os subitens estão em `lote-m1-report.md`. O T1b fechou sem teste novo: a reversão `..15`/`..17` já deixava vermelho o teste que existia.
  - **O que a primeira revisão pediu.** Um Importante: um teste de tempo com ~50 ms de margem, que podia reprovar à toa na bateria dos três sistemas. A escuta passou para os 15 ms, e a margem para ~85 ms. Medido em 30 de 30 rodadas sob carga, e a reversão fica vermelha 5 de 5.
  - **Os menores, consertados:**
    - o `compile_fail` ganhou um gêmeo que compila, porque o código de erro só é conferido no nightly;
    - o doc de `resolver_ponto` foi acertado;
    - a fiação do C4 ganhou um guarda de fonte.
  - **A re-revisão achou um defeito nesse guarda:** ele casava com o próprio texto num renome, o que foi medido. A assinatura passou a ser ancorada no começo da linha, e o guarda grita quando o alvo some.
  - **Para a integração.** O `sementes_do_fuzz.rs` só mudou no trecho do `datagrama_do_encontro`: os campos de `Marcas` viraram os leitores. O `seele-app` e os testes do `seele-ffi` não rodaram no lote, e o risco é baixo, porque nenhum leitor de campo de `Marcas` está fora do diff.
  - **Para o M2:** o doc de `onde_mora_hoje` ficou atrás do código, como manda o brief.
- **C → integrado** (`fbeff8e..c18a287`, merge `7979cda`), em 4 rodadas.
  - **Commits por item:** P51-05 `0844129`, P51-08 `37a5d90`, P51-10 com a metade do app do P51-26 `9f90888`, P51-12 `53587a1`.
  - **A revisão achou um Importante que nascia de uma decisão do controlador.** As linhas INFO servidas dividiam o balde com os avisos, e um aviso segurado não atrasava: ele era descartado. Com mais de 32 mídias, a recusa da 35ª sumia. É o defeito de 23/09 de volta.
  - **A decisão foi corrigida:** um balde por MOD **e por classe** (`Aviso`, `Servida`, `Depuracao`), com o transbordo também por classe.
  - **O resto das rodadas:**
    - o `revogar` diz as contas fora do cadeado, por `Session::dizer_as_contas_do_registro`, que o lote F vai chamar na saída do app;
    - `transbordo=true` e `mod_id_ilegivel`;
    - um guarda que prende os comandos aos baldes da sessão;
    - a linha DEBUG da mídia vinda do servidor ficou sob teto, com um teste que prova a classe contra `Aviso`.
  - **Ficam para o Z decidir:** quatro portas sem balde fora do lote (`pedido_a_caminho_dito`, `pedido_recusado_na_saida_dito`, `mod_nativo_colher` e `mod_nativo_entregar`).
- **Onda 1 fechada**: os seis lotes estão em `0.16/lotes`, no `7979cda`.
  - **A conferência do branch combinado:** o fmt e o `clippy --workspace --all-targets --all-features -D warnings` saíram limpos. Os testes passaram:

    | alvo | testes |
    |---|---|
    | `seele-app --bin` | 211 |
    | `seele-app --test frontend` | 261 |
    | `seele-core --lib` | 432 |
    | `seele-ffi --lib` | 124, mais 1 ignorado |
    | `seele-proto` | 259, 5, 1 e 3 doctests |
    | `seele-server --lib` | 555 |
    | `xtask` | 50, 4, 96, 1, 1 e 2 |
    | `check-runtime` | verde |
    | conformidade | 15, 15, 6, 10, 2, 9 e 6 |

    A conformidade rodou `bateria_interna`, `quarto`, `ocupante_da_escuta`, `volta_pela_trilha`, `acceptance_m5`, `estados` e `anexos`. O `quarto_pelo_link` ficou de fora, por causa da porta 8384.
  - **Uma medida errada, corrigida.** A primeira rodada do xtask, com um `target/` emprestado de outro worktree, deu três vermelhos no `adr_index`: o binário reaproveitado guardava o `CARGO_MANIFEST_DIR` de outra árvore. Com o `target/` deste worktree, o mesmo teste dá 4 de 4.
  - **Ficam para as ondas seguintes:**
    - o F chama `dizer_as_contas_do_registro` em `despedir_se`;
    - o K3 prova o desenho do bloco «a região e as páginas de cada MOD» em `diagnostico-de-mods.cjs`;
    - o M2 reescreve o doc de `onde_mora_hoje`.
  - **Os menores adiados** estão no ledger da frente, para a revisão final e para o Z.
- **A integração da onda 1.** A orquestradora juntou `0.16/lotes`, `0.16/protocolo`, `0.16/launcher` e `0.16/web` em `versao/0.16` (`2a37ffc5`). A bateria inteira passou com 2912 testes. O único conflito foi o `EndReason` (H contra F1), e as duas listas ficaram.
- **O merge de volta.** O `git merge versao/0.16` no `0.16/lotes` deu `0f94be6`, sem conflito.
- **Onda 2 despachada:** D · K5 · M2 (com a montagem de certificado do 1B) · P53, em paralelo e sem dividir arquivo.
- **P53 → integrado** (`0f94be6..2d2a914`, merge `8476815`), em 2 rodadas. A pendência 53 fecha em `c6ca70e` e `6038abb`.
  - **O Importante da revisão:** a linha WARN nova registrava a pasta em `%` cru, e uma quebra de linha no nome forjava uma linha no `seele.log`, como o revisor mediu. A pasta passou a sair por `caminho_no_registro`, com um teste. O teste do Windows compara a forma escapada.
  - **O guarda `variants_of`:**
    - passou a ler `Nome(…),` e `Nome { … },`;
    - prende a profundidade;
    - enxerga as variantes com campo de `FalhaAoHospedar` (as da F6 inclusive) e de `FalhaAoInstalarMod`.
  - **A frase do atalho** diz o remédio para todo atalho, e não só para o `node_modules`.
  - **Ficam para o Z:**
    - a frase de `ManifestoRecusado` mostra o código cru;
    - `FalhaAoApagarPacote` não tem frase;
    - duas recusas vizinhas não deixam linha no registro;
    - nenhum guarda mede o tamanho da frase do atalho.
- **M2 → integrado** (`0f94be6..80755ef`), em 2 rodadas. P51-16 em `8669d99`, P51-18 em `aa106bc`, P51-20 em `70a0f12`, e a escuta de teste compartilhada em `092dbfc`.
  - **O Importante da revisão:** um teste novo escrevia `$SEELE_ENCONTRO` fora da trava, e era o terceiro escritor fora dela. O defeito foi reproduzido antes do conserto (3 em 6000 rodadas) e sumiu depois (0 em 6000).
  - **Achado anterior à F5, medido neste Mac:** os testes do `seele-server` (`hospedagem.rs`) e o `ejetar.rs` falam com o ponto de encontro de produção, `encontro.seele.app.br`, sempre que `$SEELE_ENCONTRO` não está posta. Cada rodada registra a marca de um servidor de teste no quarto e deixa lá o IP público da máquina.
    - Nenhum teste afirma esse degrau.
    - A orquestradora foi avisada.
    - O conserto sugerido é uma costura que passe `convocacao = None` à `Hospedagem::iniciar` em teste.
- **K5 → integrado** (`0f94be6..7a54ba8`), em 3 rodadas e um fecho. Os commits que fecham os itens: P49-7c em `6be3d24`, P49-8 em `d9f481e` e P51-25 em `8cadbc2`.
  - **A primeira revisão achou dois Importantes:**
    - o `data:` de todo som do servidor ficava retido em toda montagem. Medido: um som de 1 MiB em 64 destinos ocupava +90 MiB, e passou a +6 MiB depois do conserto;
    - a soltura do tocador não tinha guarda.
  - **A decisão do controlador** se apoia nos riscos do plano: o som que tocava quando o destino saiu recomeça do começo onde a marca vai, inclusive no mesmo destino que volta. Com isso, o P49-8 tem efeito audível hoje.
  - **As rodadas seguintes:**
    - a marca devolvida não é tomada fora da tela;
    - o som pedido de novo ao servidor conta no teto o tamanho da resposta nova;
    - os guardas novos ganharam prova por reversão;
    - o `data:` de um som recusado sai.
  - **Para a v6:** o religar de uma contribuição só existe lá; hoje o P49-8 vale para o som que tocava.
- **D → integrado** (`0f94be6..a840ed0`, merge `59a4221`), em 4 rodadas. Os commits que fecham os itens:
  - P51-01: `3f1d2e9`;
  - P51-14: m8 `59d66aa`, T3 M2 `6bf9ddd`, T1 `bead90c`, T2 N1 `74a5c42`;
  - o m9 já vinha do C.
  - **A primeira revisão mediu dois Importantes:**
    - A conta do console, dita sem esperar o MOD parar, tomava a ficha das linhas dele. Num MOD que escreve sem parar, 15 linhas dele e 0 de conta viravam 1 e 14, e o teste que pegava isso tinha sido ajustado ao código.
    - A segunda passada do `emTexto` não tinha orçamento: um grafo de 10 nós derrubava a volta.
  - **As rodadas seguintes:**
    - A conta só sai com uma ficha de sobra. Primeiro só na espera; depois, no fim de toda volta.
    - Cada passada do `emTexto` tem um orçamento de 1024 objetos.
    - O teste que tinha sido ajustado voltou byte a byte ao original.
  - **A conta de fim de volta com uma ficha só vinha de antes do lote** (`7953a3b`). Um MOD com ouvinte de eventos, ou com trabalho depois da linha, perdia quase tudo para ela. Medido: 0/15 com 120 ms de trabalho; agora dá 15/0 em todos os casos.
- **Onda 2 fechada.** Os quatro lotes estão em `0.16/lotes`, no `59a4221`.
  - **O que o branch combinado mostrou**, com o `target/` deste worktree:
    - fmt e `clippy --workspace --all-targets --all-features -D warnings` limpos;
    - `seele-app`: todos os binários verdes (259, 263 e os menores);
    - `seele-core --lib` 432, `seele-ffi --lib` 124 (mais 1 ignorado), `seele-proto` 316 com os doctests, `seele-server --lib` 562, `xtask` 161;
    - `check-runtime` verde;
    - conformidade verde em `bateria_interna`, `quarto`, `ocupante_da_escuta`, `volta_pela_trilha`, `acceptance_m5`, `estados`, `anexos` e `mod_de_referencia`;
    - o `quarto_pelo_link` ficou de fora, por causa da porta 8384.
  - **Ficam para a orquestradora decidir:** onde mora o conserto dos testes que falam com o ponto de encontro de produção, no G-servidor da onda 3 ou numa pendência nova.
- **A orquestradora recebeu a onda 2 e decidiu o achado do ponto de encontro:** os testes não podem falar com a VPS. O conserto entra na onda 3 como passo 0 do G-servidor (`2046cd8`), por três motivos:
  - os dois mexem em `hospedagem.rs`;
  - lotes da mesma onda não dividem arquivo;
  - o teste novo do G-servidor também sobe uma hospedagem.
  
  O passo leva um guarda que reprova o teste que resolver o ponto padrão de produção, com prova por reversão.
