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
- **Decisões do dono de 08/10 sobre o rascunho**, todas como recomendado: a
  `folha` substitui o `painel`; `regiao` e `cartoes` saem e o `tema` fica,
  ampliado, com `servidor.aparencia` como porta; `repetir: true` por contrato;
  os membros fora do ar ficam fora do retrato. O aviso do servidor (`ModAviso`)
  e o tema como dado (`TemaMudou`) foram para o contrato da 1.0, pela F1.
- **O R5 do PERFIS, medido:** o escopo de servidor já existe no fio
  (`ChannelId(0)`, correção da F1). Um cliente sem janela mandou ao PERFIS
  3.3.0 `view` e `save` com canal 0 e com canal 1, e os quatro voltaram
  `ok: true`. A causa é a casca comum dos MODs oficiais, que exige um canal de
  texto e nunca manda o 0 — conserto do porte.
  - Ruling: no escopo de servidor, o `write` do contexto é o `WriteChannel`
    global (o de hoje, agora escrito), e não uma permissão nova — custo se
    errado: uma permissão de servidor a criar numa API somada, com migração.
  - Ruling: no escopo de canal, o `write` passa a `pode_no_canal(…, Escrita)`,
    e o pedido num canal que a pessoa não lê é recusado — custo se errado:
    nenhum hoje, porque nada grava papel mínimo em canal.
- **Regras da v6 vindas dos achados**, com guarda no `check-api` (tarefa 2):
  os nomes da metade de servidor conferidos contra o runtime, e os tetos
  declarados contra as constantes do renderer.
- **Tarefas novas no plano:** 15 (o `servidor-com-mod.rs` instala por hash,
  fora da 8384, e o guia o documenta; defeito, pela orquestradora) e 16 (o
  escopo de um pedido).
- **A M3 de base fechada no Mac: os 10 cenários, duas rodadas cada** (ADR
  0056). De nenhum para três MODs, a memória sobe ~58 MiB, quase toda no
  WebContent, e a CPU de repouso vai de ~5 % para ~9 %. A voz soma ~60 MiB sem
  MOD (a tela da chamada) e de 3 a 5 pontos de CPU. O jitter fica em 1 ms de
  mediana com 0, 1 ou 3 MODs. A MESA em arraste vai a ~395 MiB e ~16 %. Nenhum
  commit passou de um quadro, o que mede o commit do app, e não o quadro da
  página.
  - Falhas do roteiro achadas e consertadas no caminho: o editor tocado antes
    de o PERFIS registrar o cartão; o arraste sintético contado como entrada
    de uma pessoa; e a sessão bloqueada, em que a janela não responde.
  - O jitter não foi lido com o editor aberto nem com a MESA: a barra de
    telemetria fica inerte sob o diálogo e coberta pela página imersiva. Vai
    para a M3 com a API 6 (tarefa 11).
  - Falta, e é do dono: o Windows (WebView2) e o iPhone.

## 09/10

- **A tarefa 2 do plano, liberada pela orquestradora antes do K4** (só toca o
  `xtask/src/check_api.rs` e uma função nova no fim do `frontend.rs`):
  - `2cb0c3b`: o `check-api` ganha as regras da v6 — os pontos e os tipos de
    superfície nos dois sentidos (o sentido casca → API é o que pega o
    `pessoa.avatar`), os nomes da metade de servidor contra o que o runtime
    registra, e os tetos declarados contra os do produto (um teto que o guarda
    não sabe conferir reprova). Só cobram a partir da v6; a saída do
    `cargo xtask check-api` não muda. 45 testes do `check_api`, 72 do binário do
    `xtask`, clippy e fmt limpos.
  - `0b012c1`: `a_casca_confere_as_mesmas_capacidades_que_o_nucleo`, o guarda que
    o `base.js` citava e nunca existiu. Compara os dois mapas inteiros.
  - **Prova por reversão de cada um**: o sentido casca → API dos pontos, os
    nomes de servidor, a comparação dos tetos e a volta das regras na lista de
    `conferir_arquivo` ficam vermelhos no teste deles; o espelho reprova com a
    casca sem `volume` na API 5, com o núcleo com uma capacidade a mais na API 3,
    e com uma API 6 só na casca.
  - Ruling: um bloco ausente na v6 conta como vazio, e não como violação
    própria — custo se errado: nenhum, porque o sentido contrário reprova cada
    ponto e tipo que falta, nomeado.
- **Respostas aos planos do porte das três sessões de MOD** (rascunho §17, uma
  tabela por sessão; commits `365646c`, `9b27862`):
  - MESA: o `ModAviso` coalescido por canal e só depois de o quintal gravar;
    `volume` em toda mídia do servidor, inclusive o `fundo` da `tela`;
    `contexto.sala` preenchido pelo servidor em todo pedido; o palco escondido
    continua soando; `superficie.atualizar(chave, subarvore)`.
  - ESTILO: a lacuna do tema (B0) virou a tarefa 17 — `mundo.tema`, o servidor
    guardando e mandando o `TemaMudou`, a porta no manifesto, `servidor.aparencia`
    como porta de quem vê, o evento `tema`, os pares e os derivados em números, a
    prévia, a herança e a ampliação. O B9 é a tarefa 18, com a F6.
  - PERFIS: o teto de contribuições muda de modelo (os pontos por alvo fora do
    agregado); o sujo com `inicial`; a carga do aviso como valor JSON; os nomes
    das recusas, com `detectado: 'heic'`; os portes em 4.0.0 (D-mods-1).
  - Ruling: `contexto.sala` é do servidor, pela presença, e não da janela —
    custo se errado: nenhum dado novo ao MOD, que já recebe `PersonJoined`.
  - Ruling: os pontos por alvo aceitam uma contribuição por alvo e por MOD —
    custo se errado: um MOD que quisesse duas por pessoa usa uma só, com mais
    nós.
  - Para o dono, pela orquestradora: um tema por servidor (17c) e os números
    do teto (256 alvos por ponto, 128 sem alvo), que saem da M3.
- **A orquestradora aprovou a revisão** e passou as decisões do dono: um tema
  por servidor, o escolhido por quem administra (D-api-5); o modelo do teto
  decidido, e os números com a M3. E liberou o servidor do tema (17a a 17c) no
  `session.rs` e no `mods/mod.rs`, com aviso antes de tocar o `admitir` (dado e
  aprovado: uma leitura a mais e um quadro depois do `Instancia`).
- **Tarefas 17a a 17c feitas** (depois de `git merge versao/0.16`, `181b266`):
  - `Manifest.tema` (a porta), com teste em `seele-proto`.
  - Migração 15, `temas_dos_mods`, com o índice parcial `um_tema_por_servidor`;
    `persistence/tema.rs` (`em_vigor`, `escolhido_ligado`, `declarar`). Seis
    testes, e **as quatro regras provadas por reversão**: o tema em vigor só com
    o MOD ligado; a recusa de outro MOD; tirar não devolve a porta; o desligado
    não segura a porta.
  - `mundo.tema` no runtime: `{ ok: true }` ou `{ ok: false, erro }`; a
    declaração só vale se a chamada terminar, nos dois caminhos (pedido e
    momento). Sete testes de unidade no runtime e **sete reversões que
    reprovam** (a porta, o escolhido, o teto, a lista como tema, a falha que
    não esquece, e a limpeza no começo de `chamar` e de `pedir`). A limpeza
    passou na primeira bateria, sem teste que a cobrisse, e virou o teste
    `um_tema_nao_tomado_nao_passa_para_a_chamada_do_vizinho`. No despacho, um
    teste (`um_tema_declarado_num_momento_e_gravado_e_vai_ao_barramento`) e
    duas reversões que reprovam (sem publicar, e sem recolher o tema na thread).
  - O pedido e o momento gravam o tema depois do quintal, na mesma tomada, e
    mandam `Event::TemaMudou` só quando o tema em vigor muda; o `admitir` manda o
    `TemaMudou` logo depois do `Instancia`, sempre (um tema guardado inválido
    vira `None`, e o quadro sai assim mesmo).
  - Conformidade `tema_do_servidor.rs`, três testes verdes: todos recebem e o
    tema sobrevive ao reinício; desligar tira, religar devolve; só o escolhido
    pinta, e quem não tem a porta é recusado. Das cinco reversões de integração,
    **só uma está provada**: sem o quadro no `admitir`, os três reprovam.
  - **Correção do que registrei antes (09/10, tarde).** Escrevi aqui, e disse à
    orquestradora, que «publicar sem gravar» reprovava. **Não reprovava: não
    compilava.** O `match` sobre um literal de struct pede parênteses, o
    compilador recusou com um `error:` sem código, e o meu detector só
    reconhecia `error[`. Nas duas rodadas, o «reprova» era o build falhando.
    A saída inteira de cada reversão agora fica guardada, e o detector lê
    `could not compile`.
  - **Um teste que travava em vez de reprovar** (achado da orquestradora). A
    reversão «sem traduzir o evento» pendurou o `tema_do_servidor.rs`: o prazo
    de 15 s era por leitura, e o servidor manda quadros o tempo todo (o estado
    de cada pessoa, a telemetria), então a espera por um `TemaMudou` que nunca
    vinha não vencia nunca. Matei o processo e devolvi o `session.rs` ao commit.
    O conserto é um prazo **total** em cada espera (`PRAZO`, com
    `timeout_at`), e a falha diz o que se esperava e os temas vistos até ali.
  - **Rodado depois do aviso da orquestradora** (a sexta integração verde em
    `cb426b19`): os três testes verdes com o prazo novo, e **as cinco reversões
    de integração reprovam, todas compiladas**, com a saída inteira guardada:
    | reversão | reprovam | por quê |
    |---|---|---|
    | sem o quadro no `admitir` | os 3 | o quadro depois do `Instancia` não é o `TemaMudou` |
    | publicar sem gravar | os 3 | religar devolve `None`; o segundo MOD passa a pintar; o reinício perde o tema |
    | sem traduzir o evento | os 3, em 45 s (3 × 15) | «esperou 2 TemaMudou … vistos até aqui: [None]» — reprova, e não pendura |
    | sem ler quem pinta | `so_o_escolhido_pinta…` | o segundo MOD lê `ok` (o banco recusa na gravação, tarde demais para ele) |
    | sem a porta do manifesto | os 3 | o ESTILO recebe `sem-porta-de-tema` |
  - **Um defeito meu, pego por um teste que já existia**: o `acompanhar` passou
    a guardar um `Sender` do barramento, e o laço, que só sai quando o
    barramento fecha, não saía mais. Em produção seria uma tarefa sobrevivendo
    ao servidor, segurando o banco. Agora é um `WeakSender`, e o teste
    `um_evento_do_barramento_chega_ao_mod_e_o_quintal_e_gravado`, que travou, é
    a prova contra a volta.
  - Ruling: escolhido é o primeiro MOD com a porta que declara, enquanto estiver
    ligado; desligado, a porta fica livre — custo se errado: com dois MODs de
    tema, desligar o escolhido e religá-lo depois de o outro declarar deixa o
    outro pintando, até quem administra trocar (17f.0, na casca).
  - Ruling: o servidor não confere quem pediu o `mundo.tema`; a porta é o
    manifesto (O3 do ESTILO) — custo se errado: um MOD que quisesse restringir
    a quem administra lê `contexto.admin`, como já pode.
  - Achado para o corte: um manifesto com `"tema": true` e sem metade de
    servidor tem uma porta que ninguém abre (tarefa 13, passo 6b). E o
    indexador precisa ler a chave `tema` (tarefa 13, passo 6).
- **Segunda volta dos planos do porte** (rascunho §17): ESTILO O1 a O5 (a
  recusa é valor; o par com os dois nomes, `{ frente, fundo, razao, piso }`; cai
  com o quintal; passa no pedido de quem não administra; decidido; números com a
  M3) e PERFIS I e J (o `Error` do prelúdio leva os campos da recusa, como
  `detectado`; números com a M3, como opções no laboratório).
- **ADR 0056**: a seção «Os candidatos da API 7» (`decorar`; tipo e escala no
  tema; o fundo por mídia se não medido a tempo; `tela` e transmissões no
  retrato; os membros fora do ar), e o «não decide» atualizado.
- **O `executor.rs` é da F4 até a extração** (aviso da orquestradora): a volta
  do executor vai para um crate que o desktop e o motor do celular usam. As
  tarefas 4 e 8, e o passo 5 da 3, começam em cima da extração, mesmo se o K4
  entrar antes; o plano diz isso na tabela de estado. As outras seguem com o K4,
  sem confirmação a cada passo, com aviso nos marcos (pedido do dono, pela
  orquestradora).
- **As perguntas da F8** (a tela do celular, D-celular-1), pela orquestradora:
  - O `operador.barra` no celular: aceita a proposta da F8 — no alto da aba
    AJUSTES, abaixo de SEU PERFIL, com os tamanhos das outras regiões e altura
    limitada (a gestão de MODs de AJUSTES continua à vista, guarda 11). Com isso
    nenhuma das oito regiões fica sem casa no celular; o `regiao-ausente` fica
    no contrato para a casca que não tiver uma região, e a bancada do passo 8
    da tarefa 5 tira uma região do mapa para prová-lo. Passo 8b novo.
  - As chaves do tema: lista fechada de nomes do contrato, traduzidos por
    tabela fechada para `--seele-*` (é o que a casca já faz, `TEMA_DA_API`).
    **Achado: o `mundo.tema` que fiz na 17a aceita qualquer chave**, e um MOD
    leria `ok` para um tema que a casca recusa. Vira a tarefa 17j, antes do
    congelamento, com um teste espelho entre o servidor e a casca. O guarda do
    `--nucleo-*` é da F8.
  - Ruling: a altura do `operador.barra` no celular é limitada, com rolagem por
    dentro — custo se errado: uma barra de MOD alta rola em vez de crescer, e o
    número sai da M3 no iPhone.
- **A extração do executor entrou** (`d6d1a115`; merge no meu ramo em
  `4734141`). O prelúdio está em `crates/seele-motor/src/preludio.rs`. As
  tarefas 3 (passos 5 e 5b), 4 e 8 esperam só a orquestradora liberar, depois
  da bateria da sétima integração.
- **O `Atomics` sai do desktop na API 6** (decisão minha, com a recomendação da
  F4): é a única global que difere entre o nativo e o `.wasm`, sem threads não
  serve em nenhum dos dois, e nenhum dos três MODs oficiais o usa (lido nos três
  repositórios; os três arquivos achados na MESA são cabeçalhos do QuickJS numa
  pasta de build). Tarefa 3, passo 5b.
  - Ruling: o `SharedArrayBuffer` fica — custo se errado: nenhum, porque ele
    existe nos dois alvos e, sem threads, é só um buffer.
- **Dois achados medidos no protótipo da F8**, pela orquestradora:
  - **Uma superfície de MOD cobria a faixa de voz e as abas** (o diálogo do
    ESTILO em tela cheia cobria o MUDO e o caminho para AJUSTES). Regra do
    núcleo: no celular, «tela cheia» é a tela acima da faixa e das abas, para
    `pagina`, `dialogo`, `folha` e `aviso`, e elas nunca ficam inertes — a
    garantia do ADR 0052 de que um modal torna o resto inerte ganha essa
    exceção, com o foco do teclado ainda contido no diálogo. Tarefa 7, passo
    5d; tarefa 9, passo 4b. No desktop, o guarda mede antes de concluir se um
    `dialogo` aberto deixa o MUDO e o SAIR à vista e clicáveis.
  - **As cores de estado fora dos pares** (o «juli fala» some num `painel`
    claro). Decidido: derivadas das superfícies do tema, com o matiz do produto
    e 4,5:1 sobre `fundo` e `painel`; o padrão reproduz os tokens; o MOD não as
    escreve; o núcleo lê `--nucleo-*`. Tarefa 17e.
  - Ruling: derivar, e não pôr as três nos pares — custo se errado: um tema
    muda o tom do fósforo, do azul e do vermelho dentro da sessão; com pares,
    temas claros inteiros seriam recusados por cores que o MOD nem escolhe.
  - Ruling: no desktop não decido sem medir — custo se errado: até a medida, o
    contrato não promete o MUDO clicável sob um diálogo de MOD no desktop.
  - Corrigido junto: o §4 dizia que a capa do operador não aparece no celular;
    com o `operador.barra` em AJUSTES, ela pinta o cabeçalho SEU PERFIL.
- **A telemetria e o campo de privacidade do I-b** (achado da revisão do I-b da
  F5): o retrato da 6 já deixa a `telemetry` inteira de fora (a lista permitida
  dela é vazia). O que faltava era a garantia de que todo campo novo nasce
  fora: o plano da tarefa 4 conferia uma lista de nomes proibidos, e não a
  forma. Passo 4b novo: as chaves de cada nível são exatamente as da lista, e
  um `Snapshot` com um campo desconhecido em cada nível sai sem nenhum deles.
  - Onde o campo chega hoje [c]: em nenhum lugar — só existe no
    `0.16/lotes-i-b`. Na v0.15.1 publicada (`54f542f`, conferido pela API de
    releases) ele não existe. Quando o I-b entrar, chega aos MODs da API 5 na
    build da 0.16 antes do corte (o `snapshot` sem filtro, `base.js:2335`),
    e nenhum MOD instalado o recebe, porque o app 0.16 só sai com
    `APIS_ACEITAS = [6]`. A linha 0.15 só o receberia se o I-b fosse levado
    a ela.
