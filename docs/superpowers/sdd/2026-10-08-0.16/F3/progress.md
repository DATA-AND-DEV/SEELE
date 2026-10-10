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
- **Tarefas 3 e 4 feitas** (liberadas pela orquestradora depois da sétima
  integração; ramo em cima de `c8b3cd47`):
  - `4da1e4b`: as capacidades da 6 (sem `regiao` e `cartoes`), e o prelúdio
    montando `regiao` e `cartoes` pela capacidade.
  - `b06f289`: o `Atomics` sai do contexto do MOD nos dois alvos (o nativo e o
    `cargo xtask web`).
  - `6a5ee25`: o retrato projetado por lista branca, a capacidade nova
    `retrato` no lugar do `snapshot` (prelúdio e roteador), `onde` pela marca
    `data-casca`, `regioes` pelo que a casca monta, e o guarda 12.
  - `f8120c0`: a recusa pelo nome na 6 (`recusaParaOMod`), o `Error` do
    prelúdio com os campos, e o HEIC recusado pelo nome no seletor.
  - Reversões: 3 + 2 + 2 na tarefa 3; 11 no retrato; 7 no 5b — todas
    reprovando.
  - Ruling: `retrato` é uma capacidade, e não um teste de versão — custo se
    errado: uma linha a mais na lista da 6, e o prelúdio continua montando o
    `SeeleUI` só pelas capacidades.
  - Ruling: a recusa sem nome chega ao MOD da 6 como `falhou` até as tarefas 5
    e 7 nomearem as das contribuições e das superfícies — custo se errado: até
    lá o MOD da 6 lê `falhou` onde lia uma frase, e o motivo está no
    `seele.log`.
  - Ruling: o `detectado` do HEIC vem do cliente, que reconhece o arquivo antes
    de enviar, e não do servidor — custo se errado: nenhum no fio, que não
    muda; o servidor continua recusando o tipo pelo `sniff`.
  - **Um erro meu, pego antes de avisar**: os testes do executor das tarefas 3
    e 4 indexavam JSON (`seis[nome]`), que o `clippy` recusa, e eu fiz os dois
    primeiros commits sem rodá-lo. Consertado no `f8120c0`. E um script meu
    esvaziou o plano (abria o arquivo para escrita antes de lê-lo); restaurado
    do git, sem perda.
  - Pendente: o `mod_de_referencia` da conformidade, depois da bateria da
    oitava integração.
- **Tarefa 8 feita no código** (`733c1a5`): `mundo.avisar`, o `Avisador` (4 por
  segundo em qualquer janela, o excedente junto por canal), `Event::ModAviso` a
  quem pode ler o canal, `contexto.sala`, o evento `servidor` na janela da 6, e
  os eventos da tela pela comparação de retratos. Quinze reversões reprovando;
  duas que passaram mudaram o código (a conferência redundante saiu) ou o teste
  (o aviso aos 600 ms).
  - Ruling: o `ModAviso` pede a permissão de ler o canal, e não a assinatura —
    custo se errado: quem não abriu a conversa daquele canal recebe o aviso do
    MOD dele, que é o que a MESA precisa.
  - Ruling: janela deslizante, e não balde de fichas — custo se errado: o
    primeiro segundo deixa passar 4, e não 8.
  - Ruling: o `lugar` sai com o tamanho da sessão até a tarefa 5 dar o de cada
    região — custo se errado: um MOD lê a forma da casca, e não a da região
    dele, até lá.
  - Pendente: a conformidade `aviso_do_servidor.rs` (escrita), depois da bateria.
  - Visto no caminho: `alcance::encontro::testes::a_janela_fecha_dentro_do_atender_e_nao_so_no_auxiliar`
    reprovou uma vez na rodada paralela do `seele-server` e passou 3 de 3
    sozinho (7,8 s cada). Não é desta frente; dito à orquestradora.
- **O tema claro no desktop** (achado da F8, decisão D-linha-3): medido nas
  três versões com a interface real no Chromium, e o defeito estava na v0.15.1
  publicada também — o corpo das mensagens a 1,09:1, o SAIR DA SALA a 1,04:1.
  A causa: o `body` resolve a cor com o token da raiz, e o tema o redefine em
  `#tela-sessao`, abaixo dele. Conserto: `#tela-sessao { color:
  var(--seele-osso) }`.
  - 0.16: `6ae45d1`, com a bancada `tema-claro-legivel.cjs` (reprova sem a
    regra, passa com ela) e a evidência em
    `docs/evidencias/tema-claro-2026-10-09/`. Sem tema, as capturas são iguais
    byte a byte em 1280 e 900 px.
  - 0.15.2: o ramo `conserto/tema-claro-0.15` (`43b5899`, `b46032e`), com o
    mesmo conserto, a mesma bancada e a nota; o merge é da orquestradora.
  - O que sobra abaixo de 3:1 é da 17e: as cores de estado e os tons
    derivados.
  - Ruling: a varredura mede todo texto visível da sessão, e não três
    seletores — custo se errado: nenhum, a bancada que fica no CI confere os
    três lugares que a orquestradora pediu.
- **As conformidades das tarefas 4 e 8** (liberadas depois das duas baterias,
  em cima de `c992a03`, com o conserto da orquestradora no `cascaDoRetrato`):
  - `aviso_do_servidor.rs`: 3 de 3 verdes, e três reversões, todas compiladas e
    reprovando — os pedidos sem entregar o aviso (dois casos), a sessão sem o
    filtro de leitura do canal, o contexto sem a sala.
  - `mod_de_referencia`: 4 de 4 verdes.
  - O aviso que cai com o quintal tem a reversão no runtime («a falha não
    esquece os avisos»); na integração, o `?` da chamada sai antes de tomar os
    avisos, e uma reversão ali teria de quebrar duas coisas juntas.
  - Nota da orquestradora (`12ed17d1`): o `onde` deixou de ler `dataset.casca`,
    que o guarda G2 da F8 proíbe ao desktop, e passou a perguntar se o
    `CELULAR` existe. O retrato segue a regra da casca, e não uma marca minha.
- **17e, as cores de estado derivadas do tema** (`bf8d8fb6`, sobre o K4 em
  `cd2227c5`). Sob um tema, o fósforo, o azul, o vermelho e os preenchimentos
  saem das superfícies dele, em OKLCH, com 4,5:1 sobre o `fundo` e o `painel`;
  sem tema, nada é derivado.
  - `tons-do-tema.cjs` (check-runtime): padrão → cores de estado iguais, fills a
    ΔE2000 3,2–4,2; tema claro → tudo ≥ 4,5:1; cinza-médio → recusado pelo
    fósforo; cor da raiz que não carregou → nem recusa nem tom.
  - `tema-claro-legivel.cjs` (navegador): sincronia e nome de quem usa a
    4,52:1; sem a derivação, 1,05:1.
  - Sem tema: capturas iguais byte a byte ao HEAD anterior, 1280 e 900.
  - frontend 280/280, check-runtime limpo, tokens e xtask verdes.
  - Ruling: o ΔE ≤ 2 prometido não se cumpre e o contrato fica em ≤ 4,5 para
    os preenchimentos — custo se errado: sob um tema com as cores do padrão,
    um `-fraco` um pouco diferente do token, e só ali; sem tema, nada muda.
  - Ruling: o `acento` sobre o `acentoFraco` sai da lista de recusa e passa a
    ser garantido pela derivação — custo se errado: nenhum tema é recusado
    por esse par, e o par continua em 4,5:1.
  - Ruling: `bordaForte` não é derivado — custo se errado: num tema claro, a
    régua forte sai escura, visível, talvez pesada.
  - Ruling: a leitura da raiz ganhou um nome (`corDaRaiz`) em vez de afrouxar o
    guarda do ADR 0049, e o guarda confere que ela só lê.
- **17j, as chaves do tema fechadas no servidor** (`f9050574`). O `mundo.tema`
  confere por `seele_proto::mods::CHAVES_DO_TEMA` e responde
  `chave-desconhecida` ou `valor-invalido`, com a `chave`.
  - Vetor comum `crates/seele-proto/tests/vetores/chaves-do-tema.json` (37
    casos), lido pelo servidor e pela casca (`chaves-do-tema.cjs`, no
    check-runtime). Achou a casca conferindo com `in`: `fonte: "toString"`
    aplicado, `constructor` recusado como medida. Consertado (`Object.hasOwn`).
  - Espelho em `frontend.rs` nos dois sentidos; cobertura do vetor sobre a
    tabela; dois testes de runtime, um por recusa.
  - Sete reversões, todas reprovando por asserção (conferido: nenhuma por
    compilação).
  - frontend 281/281, `mods::` do servidor 125/125, seele-proto verde,
    conformidade `tema_do_servidor` 3/3 (sem bateria rodando), check-runtime
    limpo, clippy limpo.
  - Ruling: feita antes da 17i, com a tabela de hoje — custo se errado:
    nenhum; a 17g–i soma `modo` numa linha de cada tabela e um caso no vetor,
    e o espelho reprova se faltar um dos três.
  - Ruling: um vetor comum além do espelho pelo texto — custo se errado: um
    arquivo a mais para manter; foi ele, e não o espelho, que achou o `in`.
  - Com mais de uma chave errada, o servidor nomeia a primeira **em ordem
    alfabética** (o `serde_json::Map` é um `BTreeMap`, sem `preserve_order`).
    Eu tinha escrito «na ordem do objeto»; medido antes de avisar, corrigido,
    e fixado num teste. A orquestradora pediu as duas pontas iguais: a casca
    passou a ordenar por ponto de código (a ordem dos bytes UTF-8), com dois
    casos novos no vetor (39) e as reversões (sem ordenar; ordenando por
    unidade UTF-16) reprovando.
- **O atalho da 17e** (pedido da orquestradora): com o `fundo` e o `painel`
  exatos do produto, os preenchimentos são os tokens (ΔE 0); um acento
  diferente ganha o dele derivado. `tons-do-tema.cjs` lê os tokens do
  `tokens.css`. Sem o atalho, 7 falhas; sem conferir a cor, o acento azul
  recebe `#331704` e reprova. A um passo das superfícies, a derivação e o
  ΔE2000 ≤ 4,5 continuam medidos.
  - Próximo: as tarefas que esperavam o K4, na ordem do plano: 5, 6, 7, 9, 10,
    17d, 17f.0, 17f, 17g, 17h, 17i.
- **Tarefa 5, as regiões** (em andamento; bateria da nona integração no ar,
  sem conformidade até o aviso).
  - Step 1, o desenho da região substituída que carrega o núcleo: `1f3cf670`,
    aprovado; as decisões em `0be3cece` (o apelido das linhas lê o tema e é
    protegido pela estrutura; mensagens agrupadas pela chave).
  - Regiões feitas, uma por commit: `redes` `1e67f782`, `operador.barra`
    `3e4bfdb2`, `compositor` `ed1c4483`, `status.barra` `76943da5`,
    `canal.cabecalho` com `substituir` `55ec1c16`.
  - O orçamento pelo `perfil` do ponto e o aviso com perfil, teto e tamanho:
    `a994601f` (decisão da orquestradora: `pessoa.detalhes` 4096 nós, as
    regiões 64 até a M3).
  - A bancada `regioes-do-produto.cjs` (Chromium, `index.html` real, no CI):
    nó no contêiner e em nenhum outro lugar; o produto sem mudar de marcação
    nem de tamanho; revogar devolve a região; com conteúdo grande, o núcleo
    à vista nas quatro bordas e descoberto, com o desenho do MOD deslocado
    mirando o centro de cada item.
  - Achados medidos pela bancada e consertados: o guarda de «quem aplica»
    casava com a lista do retrato; o nome do canal espremido (43 → 7 px); a
    conversa empurrada para y = 1 230; o `+` da trilha fora da vista; o
    `perfil` que ninguém lia; o `min-width`/`max-width` redundantes da barra
    de baixo; o aviso de «não coube» só com a contagem.
  - Uma correção minha: a mensagem do `3e4bfdb2` dizia que 60 linhas «não
    montavam nada»; era o executor falso da bancada sem `entregar`.
    Registrado no `a994601f`.
  - Ruling: a bancada das regiões no navegador, e não na
    `contribuicoes-e-camadas.cjs` (que roda num `vm`) como o plano dizia —
    custo se errado: uma bancada a mais no job de navegador; o ganho é medir
    a página que o MOD vê.
  - Ruling: `overflow` e `contain: paint` juntos, sabendo que cada um corta
    sozinho — custo se errado: uma linha de CSS por região.
  - `servidor.navegacao` `66a4a1ea` (a ordem de cada lista; grupos mistos na
    API 7, aprovado); a linha com o núcleo sob `pessoa.cartao` `9ca54ea1`
    (D-nucleo-1, com guarda); `pessoas.lista` `d4f35bff` (decisão (a): as
    salas continuam seções; a reversão «o MOD toma os grupos» reprova);
    `conversa.mensagem` `86bb5589` (o corpo do MOD, a busca vence, a pendente
    fica com o produto, agrupamento pela chave). **As oito regiões estão
    feitas.**
  - Achados no caminho, medidos: o subtítulo de grupo do MOD imitava o
    cabeçalho de uma sala (agora recuado e com filete); uma substituição sem
    alvo alcançava a mensagem pendente (agora não); o guarda de `.mensagem`
    num `Match` reprovava o nome do ponto `conversa.mensagem` (agora só o
    literal exato sai da busca).
  - Um tropeço meu: um script truncou `tela-sessao.js` (abriu para escrita
    antes de ler); restaurado do HEAD e as edições refeitas, antes de
    qualquer commit.
  - Pendente: os identificadores ignorados contarem na aba DIAGNÓSTICO
    (espera `camada-mods.js`, onde está o lote W da F5).
  - Steps 6 a 8: `atualizar` `4409a271`; o teto por alvo `6317df2c` (por
    alvo, MOD e modo, aceito; o caso dos dois modos `b1c9e4fd`); as recusas
    nomeadas `577e899f`; `regiao-ausente` `fb13e65e` (declarado pelo
    `CELULAR`, e não pela marca, aceito). O `status.barra` não existe no
    celular (medido pela F8, decidido): `7f5b45f0`. O 8b é da F8, com os
    meus critérios.
  - Pendente: as duas contagens na aba DIAGNÓSTICO (espera `camada-mods.js`,
    onde a F5 vai antes).
- **Tarefa 6, `pessoa.capa`** (09/10).
  - Steps 1–3 `439f05a4`: a carga do avatar virou `imagemContribuida`, a
    mesma para o avatar e a capa; a capa é uma camada atrás do conteúdo, com o
    véu e o halo no `::after` dela; o quadro parado para o movimento reduzido;
    o `midia` com ponto e alvo.
  - Step 4 `eaa994e6`: as placas na cor do fundo nativo; o guarda **medido**
    (contraste de cada texto contra uma capa branca e uma preta, calado e
    falando), e não de texto; a capa de quem usa vestia só a identidade, e o
    MUDO, o fone e o SAIR nunca ficavam sob ela (uma contradição do §4).
  - Step 4, refeito `85e58bc2`: a orquestradora recusou a identidade sozinha,
    pelo pedido do dono (08/10). A capa pinta o fundo da barra do operador
    inteira, e todo texto e controle dela fica sobre a placa do negro do
    painel. A bancada mede a cor e a borda contra **três** capas: a branca, a
    preta e a da cor da própria tinta — sem a terceira, a reversão do MUDO
    passava, porque o cinza dele ganha contraste sobre as duas pontas (4,11 →
    4,86 e 4,32). Reversões: MUDO e fone 4,11 → 1; SAIR 15,6 → 1,28.
  - Step 4b `2e1867bd`: a citação `daPessoa`, sem pedido novo e com zero
    bytes no bolso do cartão.
  - Step 5: a redução no Rust `97332b9c`; o uso e o bolso decodificado
    `7dbd5928`; seis capas animadas `b6fa329c`.
  - Esperam a F8: o «SEU PERFIL» e a capa no celular.
  - Um tropeço de processo: dois commits saíram com um guarda vermelho porque
    eu encadeei a verificação e o commit num comando só. Os dois foram
    emendados antes de qualquer integração; agora verifico e commito em
    passos separados.
  - Ruling: o teto do bolso das capas, 128 MiB decodificados por MOD — custo
    se errado: um MOD com muitas capas animadas recusado antes da medida, ou
    memória demais num celular; a M3 mede.
  - Próximo: a tarefa 7.
- **Tarefa 7, as camadas** (09/10).
  - Steps 1–2 `34535f6d`: `tipo-desconhecido` na 6, com `tipoPedido` e
    `tipos`; até a 5, o `dialogo` de antes com a linha no `seele.log`. **Um
    achado no meio:** o campo se chamava `tipo`, e `responder` espalhava a
    carga sobre `{ tipo: "resposta", n, ok }` — a resposta virava uma
    mensagem que o executor não reconhece, e o pedido do MOD esperaria para
    sempre. O caso passava, porque olhava só o campo. Agora o envelope vai por
    último, e dois casos o conferem. Seis reversões.
  - Step 3: **espera a tarefa 11** (o teto da `midia` numa superfície se
    decide com a medida).
  - Step 4 `0f10718b`: a `folha`, os tipos por API (`tiposDaApi`), a API no
    dono, a pilha de três à vista por MOD (`teto-de-folhas`), o `painel` na 6
    recusado com «use folha», e o `teto-de-superficies` com nome. Bancada de
    navegador nova, `camadas.cjs`, no CI. Sete reversões.
  - Rulings: (a) **até a 5, o tipo desconhecido segue abrindo `dialogo`** — o
    rascunho diz «na 6», e os três publicados só pedem tipos que existem;
    custo se errado: um pacote de laboratório da 5 com um tipo errado segue
    vendo um diálogo, agora com a linha no `seele.log`. (b) **a pilha de
    folhas é por MOD**, como os outros tetos; custo se errado: dois MODs
    empilham seis folhas na mesma coluna (só a de cima aparece).
  - Para a tarefa 13: o `check-api` compara `TIPOS_DE_SUPERFICIE` inteira
    com os tipos de cada versão a partir da 6; com a v6 escrita, ele acusará
    o `painel` que a tabela ainda tem, até o corte o tirar (ou até ele ler
    `tiposDaApi(6)`).
  - Achado de desenho: numa coluna de 300 px, o título de uma folha ou de
    um painel quebrava no meio da palavra («EDITA / R / PERFIL»). Consertado
    em `97cff58d`, a pedido da orquestradora; medido, a causa era o título
    espremido, e não o `overflow-wrap: anywhere` (trocá-lo não mudava as
    onze quebras): o cabeçalho quebra a linha, e o FECHAR fica na borda.
  - Rulings aceitos: (a) e (b) acima; e o `superficie.atualizar` na
    capacidade `superficies` (as 4 e 5 o veem; no contrato, desde a 6).
  - Step 5b `e886840a`: medido, a mídia por volume já ia ao `ler_imagem_mod`
    nos cinco lugares; a bancada prova, e duas reversões a fazem reprovar.
  - Step 5c `3161fc14`: `superficie.atualizar(chave, arvore)`, a ficha de
    40.002 bytes em quatro partes; `chave-desconhecida` e `orcamento` antes
    de mexer na tela.
  - Step 5d, dividido com a F8: a minha metade, a pilha de inércia que
    pergunta `CELULAR.acordadosNoModal()` e desce no ramo dos acordados,
    `682f39ba`; a dela, `d9f5fc03` (o desenho e o ajudante), validada com a
    minha numa cópia. A cena com o toque e o Tab entra quando as duas
    estiverem na `versao/0.16`.
  - Step 6 inteiro: 6a as regras declaradas `135111f3`; 6b
    `formulario.erros` `58d86c65`; 6c o sujo derivado `52a175ce`; 6d a ação
    destrutiva confirmada `70e5dd2b`. Do §5, fora da lista: o `erro` em `cor`
    e `escolha`, e os botões primário e secundário do produto.
  - **Um tropeço meu, consertado:** o contador do 6a quebrou o guarda do
    `celular-texto.css` (`tests/celular.rs`), porque a minha verificação
    rodava só o `--test frontend`. Regerado em `19239d36`. Agora verifico
    com `cargo test -p seele-app --tests`, todos os alvos.
  - Esperam: o Step 3 (a tarefa 11) e o Step 5 (`chamada.palco`, o
    `camada-mods.js` livre depois da bateria da décima).
  - 6e `d4a63221` (o `erro` na `escolha` e na `cor`) e 6f `d289e750` (os
    botões primário e secundário do produto), pedidos pela orquestradora: o
    que o §5 promete tem de existir antes de o `v6.json` congelar.
- **Tarefa 9, o núcleo de confiança** (09/10).
  - Steps 1–2 `e26e9edc`: medido, a 17e recusa o tema só de acento escuro;
    mas o §10 manda os controles lerem o produto. `nucleo.cjs` (no CI) aplica
    os temas da F8, o hostil por baixo da validação, e exige as mesmas cores
    com e sem tema: reprovava em 29 medidas (o MUDO ligado a 1,00:1 sob o
    hostil). Os `--nucleo-*` na raiz, os nomes da F8.
  - Step 3 `475f83d5`: o guarda 11, dez itens, com os dois pontos que só
    pintam fora e o motivo escrito (aceito).
  - Step 4 `0266f95e`: o apelido verificado, já da tarefa 5, reforçado com o
    nome de outra pessoa da sessão e fora da raiz do MOD.
  - Step 4b `3e89971a` e `97e47473`: medido, o modal cobria a barra; a
    orquestradora decidiu o recorte, no desktop e no celular. O véu ganha um
    furo na barra (e na porta da gaveta, abaixo de 900), o MUDO, o fone e o
    SAIR acordados, o diálogo fora do furo e rolando por dentro; o resto
    inerte e coberto. Seis reversões.
  - Espera: o Step 5 (a 7d da sala pessoal).

## 10/10

O que entrou depois do `1a401eb5`, nos dias 09 e 10, e não estava aqui.

- **O tema do servidor na casca** (09/10).
  - 17d `87b36aa9`: o núcleo guarda o `TemaMudou`, o retrato o leva
    (`tema_do_servidor`), e a casca o aplica antes de desenhar a sessão.
  - 17f.0, o servidor `747e81e6`: `persistence::tema::escolher`.
  - 17f, a casca `7df2c421` e `78fdf546`: a escolha de quem vê, o evento
    `tema` (`aplicado`, `recusado` com os pares, `nativo`), e a janela que
    sobe depois sabendo do desfecho.
  - 17g `68253601`: a prévia, só nesta máquina, com o selo, saindo com a
    superfície dona.
  - 17h `8485cacb`: `cor: { tema: 'acento' }`; medido, a folha e o diálogo já
    herdavam o tema.
  - Tarefa 10, Steps 1 a 3 `17c6890e`: `repetir: true`.
  - `ee3f4e50`: o MOD de referência exercita o `atualizar` e o
    `formulario.erros` do punho da superfície.
- **17f.0 inteira** (10/10): `68244ced` o servidor confere a porta e manda o
  `TemaMudou`; `38421122` o comando no `main.rs`, isolado; `fbd8a013` a
  pergunta na gestão (QUEM PINTA ESTE SERVIDOR), com a linha em
  `RECUSAS_NOMEADAS` da ponte (a F4 avisada, e da lacuna no guarda da
  hospedagem, que não vê `servidor_do_mod`); `f1811d9f` o `pinta` com prova
  do lado do Rust.
- **17f, a linha em QUEM DESENHA O QUE** `b2129755`, e o DIAGNÓSTICO que
  dizia «(API 3)» sobre o tema do servidor.
- **Tarefa 5, as contagens do DIAGNÓSTICO** `2af9a35a`: os identificadores
  ignorados (a medida do agora) e o lugar que a casca não tem; o contador de
  avisos passou a sair com a sessão.
- **Tarefa 7, Step 5, o `chamada.palco`** `c7c64814`, `23a5bccf`,
  `9b7e826c`, com as duas decisões da orquestradora: os modos como nos outros
  pontos, a vista alternativa que começa escondida, e o `lugar` com
  `regiao`. **Tarefa 10, Step 4** no `23a5bccf`.
- **Tropeços medidos no caminho**, todos por bancada ou reversão: a recusa
  da escolha sumia debaixo da frase do catálogo (escrita depois do
  redesenho); a reversão do filtro do DIAGNÓSTICO passou, e a seção ganhou o
  caso da contagem dupla; o guarda de `substituir` fatiava por byte e caiu
  num «ç»; a primeira medida do som contava o `ended`, que não chega com o
  áudio suspenso; e a reversão sem o `ResizeObserver` passou, porque o laço
  da chamada media a cada meio segundo — o desenho passou a dizer o tamanho
  só a quem chega.
- **A revisão do rascunho contra a casca** `e5ccf266`
  (`revisao-rascunho-contra-casca.md`), com os consertos `3235ff33` e
  `78edf0a7`, e as três decisões da orquestradora no rascunho `72ec3a5d`: o
  `ok` de `mundo.tema` é «forma válida e publicada» (os pares são da casca), e
  o `chamada.palco` e o `servidor.aparencia` ficam publicados na 6. O
  `check-api` contra o rascunho fica com 13 achados, todos da tarefa 12.
- **A revisão dos comentários desatualizados**, na área da F3, `67826a89`;
  nela, um defeito de verdade: a recusa do `decorar` mandava o avatar e a capa
  usarem `adicionar`, que eles recusam. Dois achados fora da área (a frase do
  aceite em `frases.js:702`, que quem usa lê, e o comentário do `main.rs:7738`)
  foram para a F5, pela orquestradora.
- **Tarefa 18, o quintal ao subir do Servidor 0.15 para o 1.0** `bde1e972`
  (liberada pela orquestradora: a primeira abertura da F6 está na
  `versao/0.16` desde a primeira integração). **Passa.** A pasta é escrita pelo
  código da v0.15.1 (`54f542f94`), com o quintal gravado pelo caminho de pedido
  de lá; o teste `versoes::o_quintal_sobe_da_0_15` faz a primeira abertura, a
  migração dos pacotes, o servidor sobre o banco de sempre (migrado para o 15)
  e o 4.0.0 aplicado como a gestão aplica, e a metade de servidor nova lê o
  `theme` byte a byte. Três reversões, cada uma no passo que quebra: a
  migração apagando o `mod_data`, o quintal lido pelo hash, e a primeira
  abertura levando o banco em vez de copiá-lo. As migrações 1 a 13 de hoje são
  as da v0.15.1, no SQL e na descrição (comparadas uma a uma).
- **O caso da pasta da 0.15.1 no Step 5b da tarefa 13** `b46ce9e7`, e a
  conferência da D-api-4 aprovada pela orquestradora `28cd201a`: medido com a
  pasta, a coluna `api` diz zero antes da subida do servidor (o
  `preencher_as_apis` roda dentro do `Daemon::bind`), então a conferência lê a
  API pelo pacote, numa regra só com o servidor (`api_do_pacote`).
- **A `versao/0.16` (`c89399db`) trazida** `5ddef0a7`, sem conflito; as doze
  bancadas de Node puro e as seis de navegador da F3 passam.
- **Tarefa 16, o escopo de um pedido** `dcd6f287`: num canal, o pedido de MOD
  confere a leitura e a escrita do canal (`pode_no_canal`). Medido antes, com
  o mesmo teste, na 0.16 e numa cópia da `linha/0.15` em `40066a7c`: o defeito
  estava nas duas, e nenhuma versão o alcançava — nada grava o papel mínimo de
  um canal. A 0.15.2 não leva o conserto (pendência 62). Um guarda de texto
  reprova o dia em que algo gravar. Quatro reversões; a conformidade da ponte
  e do MOD de referência verde. O rascunho (§8) e o guia (`aoPedir`) dizem o
  `write` por escopo.
- **O 8b foi feito pela F8** (`1f09c7ba` e `d1773fd6`, no `0.16/celular`): do
  lado da F3 falta o `aplicarARegiao` lendo a declaração dela e a bancada com o
  guarda 11, depois que o `0.16/celular` integrar.
- **Tarefa 15, o exemplo `servidor-com-mod.rs`** `18043d51`: instala por hash
  (`seele_server::mods::instalar`), liga o MOD antes de subir, diz as queixas
  da subida no terminal e escuta na 8390, recusando a 8384; o teste
  `exemplo_com_mod.rs` reprovou com a instalação por identificador e passa por
  hash (a reversão reprova); rodado de verdade numa porta à parte; o guia diz
  como usá-lo. O Step 3 (a prova de janela) fica para a campanha do dono.
- **Esperam:** a M3 (depois da décima segunda integração, o dono a roda da
  `versao/0.16`), as tarefas 12 (os números da M3) e 13 (o corte), o Step 5 da
  tarefa 9 (a 7d) e o Step 3 da tarefa 7 (a 11).
