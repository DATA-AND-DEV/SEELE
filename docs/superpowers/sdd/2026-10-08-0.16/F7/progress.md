# F7 — a sala pessoal (Plano 5) e as hospedagens desacopladas (Plano 2) — andamento

**Branch:** `0.16/sala`, criado de `versao/0.16` em `4fc31e9c` (a primeira
integração, com a bateria verde de 2912 testes).

**Plano:** `docs/superpowers/plans/2026-10-08-0.16-F7-sala.md`, com 31 tarefas:
- as 22 do levantamento, 4a–4g, 6a–6g e 7a–7h. A 4f, a 6b, a 7c e a 7g foram partidas onde uma metade não espera ninguém;
- o Plano 2 em cinco: a 4e e P2a–P2d;
- os ADRs.

## Conferido antes de escrever (08/10)

- **O publicado:** `v0.15.1 · 54f542f94` é o último release de SEELE-RELEASES. A
  0.15.2 **não** foi publicada.
- **O M2 da F5 ainda não começou:** `0.16/lotes-m2` só tem docs sobre
  `versao/0.16`. Então a mudança da 4e em `hospedagem.rs` entra antes dele.
- **As três razões da sala já são terminais no cliente** (`enlace.rs:350-364`,
  da F1). A 6f encolhe para a bateria de reconexão da ligação.
- **Todas as construções de `ServerConfig` usam `..`**, então o campo
  `identidade` da 4a não quebra nenhuma.
- **O `rcgen` 0.14.8 tem `from_pkcs8_der_and_sign_algo` com `PKCS_ED25519`**, e o
  `rustls` 0.23.45 com o `ring` lê PKCS#8 v1 de Ed25519
  (`from_pkcs8_maybe_unchecked`, `crypto/ring/sign.rs:368`).
- **Os números de ADR:** o mais alto em todos os branches `0.16/*`,
  `versao/0.16` e `linha/0.15` é o 0058. Esta frente usa o 0059, o 0060 e o 0061,
  e o 0062 fica reservado ao ENC/2.

## Achados para a orquestradora

Os quatro estão no plano, na seção «Achados desta leitura»:
- **A1:** a linha legada reabre o S2 e deixa um servidor 0.15 atender as ligações
  da pessoa. A 0.15.2 precisa só assinar nonce de 32 bytes. Medido em
  `linha/0.15`: o cliente assina até 256 bytes, e todo servidor 0.15 manda 32.
- **A2:** na sala, o primeiro atendido viraria Comandante. O gancho da sala cria
  a conta do dono.
- **A3:** o DER da sala vira formato congelado.
- **A4:** um pedido à F1, o `NOME_DA_SALA` no `seele-proto`.

## Tarefas

| # | tarefa | estado | commit |
|---|---|---|---|
| — | plano | escrito, mandado à orquestradora | `906e326b` |
| 1 | 4e, a janela do Comandante | **feita** | `29d66937` |
| 2 | 4a, o certificado Ed25519 | **feita** | `43d81c31` |
| 3 | 4c, a campainha | **feita** | `77207424` |
| 4 | 7g-núcleo, a agenda | **feita** | `6f28dd41` |
| 5 | 7c-áudio, o toque só de saída | **feita**, com duas medidas pendentes do dono | `d1f8a947` |
| 6 | os ADRs 0059, 0060 e 0061 | **feita**; o 0061 aceito depois da decisão do dono | `d763e9e9` e o seguinte |
| — | merge da `versao/0.16` (`dc001a0a`) | limpo, e as provas da F7 verdes em cima dele | `fd372ca0` |
| 7 | 4f-porta, o UPnP por papel | **feita** | `efaada91` |
| 8 | 6b-verificador, o `ConfereCodigo` | **feita** | `3197550a` |
| — | merge da `versao/0.16` (`a2b9f9d0`) e a reexportação do `NOME_DA_SALA` | feitos | `273a7df0`, `023bf2f8` |
| 9 | 4g, a recepção (a campainha viva) | **feita** | `9890e4eb`, `3fc72adb` |
| 12a | 6b, o `Destino` com `Esperada` | **feita** | `a892f14a` |
| — | merge da P1 e da P2 da F1 (`bf559478`) | feito | `28003af4` |
| 10 | 4d, o aperto de mão em três fases | **feita** | `312ec92e` |
| 12b | 6b, o ramo do código no `connect_por` | **feita** | `1fb62a9a` |
| — | merge da `versao/0.16` (`bbb0ed9c`): o servidor da F1 e o ENC/2 | feito | `8f19ddd2` |
| 11 | 4f, a hospedagem da sala | **feita**; a rodada inteira da conformidade espera o fim da 5ª integração | `c3483884` |
| 13 | 6a, `ler_entrada` | **feita** | `86edd3e4` |
| — | merge da `versao/0.16` (`bd58a080`): o F da F5 e a P9 da F1 | feito; a rodada da 4f, 46 de 46 | `d3c1c596` |
| 14a + 15 | 6c e 6d, discar pelo código | **feitas** | `1f1a1b35` |
| — | a linha provisória de `ETAPAS` (`Localizando`) | isolada, até a 7f | `e3e32bfe` |
| 17 | 6f, a bateria da ligação | **feita**, menos a volta do convidado sem tocar, que espera a 16 | `0d320448` |
| — | merge da `versao/0.16` (`cb426b19`), a sexta integração | feito; a árvore é a mesma da `cb426b19` (`38c21202`), verde na bateria da integração | `87fd6973` |

## Tarefa 1 — a janela do Comandante (08/10)

- **A prova**, `crates/seele-server/tests/janela_do_comandante.rs`, roda em
  `Location::File` (o guardado) e em `Location::Memory` (a forma da sala). Os
  dois casos passam.
- **A reversão foi medida.** Com o bloco do gancho posto depois do
  `tokio::spawn` do laço, os dois casos reprovaram com «uma conexão foi aceita
  enquanto quem hospeda ainda semeava a portaria». Restaurado e conferido com
  `cmp`.
- **Rodados:**
  - `seele-server --test janela_do_comandante` (2);
  - `seele-server --lib hospedagem` (9);
  - `seele-conformance --test ejetar` (3);
  - `cargo check -p seele-app`;
  - fmt e o clippy de `seele-server`, `seele-app` e `seele-conformance`.
- **Não rodado:** o `quarto_pelo_link` só compilou (`--no-run`). Ele usa a
  8384, e a F5 estava ocupada. A mudança nele é mecânica: o quinto argumento
  `|_| Ok(())`.
- **Fronteira:** o `main.rs` foi tocado só no `async fn hospedar`, porque a
  assinatura nova não compila sem isso.
- **Um deslize corrigido:** a cópia de segurança da reversão foi gravada por um
  instante em `$TMPDIR`, fora do worktree. Apagada logo em seguida. Daqui em
  diante, só o scratchpad.

## Tarefa 2 — o certificado Ed25519 da sala (08/10)

- **A medida que vinha primeiro:** o Ed25519 aperta a mão no QUIC deste
  repositório, com `rustls` 0.23.45, `ring` 0.17.14 e `quinn` 0.11.11. O teste é
  `o_ed25519_aperta_a_mao_no_quic_deste_repositorio`.
- **O vetor congelado:** o SHA-256 do DER da semente de zeros é
  `82547b3cfe6c9b6f4a2eb295889c4b079b4d5550fbf48f9149cfa6afdc491d02`.
  - **Conferido fora do Rust:** o `openssl` do sistema (LibreSSL 3.3) mostra
    `Serial Number: 1`, `CN=seele-sala`, validade de 23/09/2026 a 31/12/9999,
    Ed25519 e `DNS:seele-sala`.
  - Ele não extrai chave Ed25519, então a chave foi conferida com a
    `cryptography` 49 do Python: é a da semente de zeros
    (`3b6a27bc…8b59da29`), e a autoassinatura confere.
- **As reversões:**
  - um par sorteado no lugar do par da semente: 5 testes reprovam;
  - ignorar a identidade injetada: reprova com «não apresentou a identidade»;
  - gravá-la no banco: reprova com «a chave da pessoa foi gravada no banco»;
  - o número de série `None`: **o rcgen 0.14 não sorteia, deriva da chave**.
    Só o vetor reprovou. O plano esperava outra coisa, e foi corrigido;
  - um `Debug` que vaza a chave: **a primeira versão do teste passou**. Ela
    procurava a semente entre colchetes, e no vazamento a semente vem no meio
    da lista do PKCS#8. O teste agora procura a sequência sem colchetes e em
    hex, e reprova.
- **Rodados:**
  - `seele-server --test identidade_da_sala` (7);
  - `seele-server --lib tls` (5);
  - o clippy de `seele-server`, `seele-conformance` e `seele-app`, porque o
    `ServerConfig` mudou.

## Tarefa 3 — a campainha (08/10)

- `crates/seele-server/src/campainha.rs`, pura, com o tempo injetado. Tem 14
  testes, e 13 deles começaram vermelhos (`not yet implemented`). O das razões
  passa desde o esqueleto, porque o mapeamento para o fio nasce com os tipos.
- **Sete reversões medidas**, cada uma reprovando só o teste dela:
  - a deduplicação;
  - o `>=` dos pendentes;
  - o `1 +` do teto;
  - os bloqueados;
  - o desligar que não apaga;
  - a espera que recusar não anota;
  - a tolerância que saiu não abre.
- **A leitura de «a sala esvazia»:** é quem recebe sair, que é o desligar. Vai
  para o ADR 0059.

## Tarefa 4 — a agenda (08/10)

- `crates/seele-core/src/agenda.rs`. Tem 6 testes, e todos começaram vermelhos.
- O `escrever_privado` de `conhecidos.rs` passou a `pub(crate)`, e os 16 testes
  de `conhecidos` continuam verdes.
- **Três reversões medidas:**
  - limpar o apelido;
  - o nome que não se troca;
  - o modo 0600.

## Tarefa 5 — o toque só de saída (08/10)

- `crates/seele-audio/src/toque.rs` tem 6 testes, e todos começaram vermelhos.
  `o_toque_nao_aloca` está em `realtime_safety.rs`.
- **As reversões medidas:**
  - sem o envelope: um degrau de 0,163 contra o teto de 0,029, e
    `nao_ha_degrau_na_forma_de_onda` reprova;
  - uma alocação no `preencher`: o processo aborta, «memory allocation …
    failed», SIGABRT. É como o guarda de tempo real reprova, e o controle
    negativo do próprio arquivo documenta isso.
- **Medido neste Mac:** `o_toque_toca_sem_abrir_nenhum_fluxo_de_entrada` passa.
  A saída padrão abriu, muda, e o contador de fluxos de entrada não subiu.
- **PENDENTE, com o dono presente:** os dois passos abaixo abrem o microfone,
  e o macOS pode mostrar um pedido de permissão. Não disparei isso sem aviso.
  - o controle negativo, `abrir_o_par_inteiro_conta_um_fluxo_de_entrada`;
  - a reversão da medida, o toque aberto pelo `open` inteiro.

  **Sem eles, a medida ainda não prova que o contador não é inerte.** O comando:
  `cargo test -p seele-audio --test toque_so_de_saida -- --ignored --test-threads=1`.
- **Rodado:** `cargo test -p seele-audio`, com 253 testes de unidade e as
  integrações, verdes. O `laco_por_processo` (`cfg(windows)`) não compila neste
  Mac. Os dependentes (`seele-core`, `seele-ffi` e `seele-app`) passam no
  `cargo check`, e nenhum faz `match` sobre `DeviceError`.

## Tarefa 6 — os ADRs (08/10)

- **0059**, aceito: a sala segura a conexão. **0060**, aceito: o código.
  **0061**, proposto: a identidade por máquina.
- O 0017 e o 0046 ganharam só notas de cabeçalho, e o índice ganhou as três
  linhas.
- **O guarda `xtask/tests/adr_index.rs` reprovou a primeira versão do índice.**
  As linhas do 0017 e do 0046 diziam «0061 (proposto)», e o guarda lê a palavra
  em qualquer lugar da linha. Elas agora citam o 0061 sem o status. O
  `textos_publicos` também passa.
- **O 0061 espera o dono** em duas coisas: a opção (a) ou (b) da linha legada,
  e a regra dos 32 bytes na 0.15.2, que é condição para aceitá-lo.

## Onde a frente está

As seis tarefas [AGORA] estão feitas. Pendências:
- **as duas medidas do toque que abrem o microfone**, com o dono presente;
- **a decisão do dono** sobre a A1 e o 0061;
- **a liberação da orquestradora** para as tarefas 7 (UPnP por papel) e 8
  (`ConfereCodigo`), que o plano marca como [AGORA, se liberada].

O resto espera o G-servidor, a F2, a casca ou a F1.

## As decisões do dono (08/10, pela orquestradora)

- **A1:** a 0.15.2 só assina desafios de exatamente 32 bytes. Quem escreve é a
  F6, no `linha/0.15`, com o teste e a reversão.
- **O ADR 0061: a opção (a).** A 0.15.2 lê `$SEELE_IDENTIDADE`, e as cópias
  acabam. O 0061 passou a **aceito**, com «O contrato da variável» escrito nele,
  e as notas do 0017 e do 0046 e o índice foram atualizados. O `adr_index` e o
  `textos_publicos` passam.
- **A divisão:**
  - o lado do launcher (o `Lancamento` e a primeira abertura sem copiar a
    chave) e a 0.15.2 são da F6;
  - a 4b, a 0.16 lendo a pasta da identidade, é desta frente (tarefa 22, que
    espera a casca).
- **A2 e A3** ficam como propostos, nas tarefas 4f e 4a. **A4** foi repassado à
  F1. **Os ADRs** 0059 a 0061 são desta frente, e o 0062 fica reservado ao ENC/2.
- **A orquestradora está integrando `0.16/sala` até `85879154`.** Nada de
  conformidade nem de bateria até ela avisar.

## A segunda integração e as tarefas 7 e 8 (08/10)

- **Merge de `versao/0.16` em `dc001a0a`** (`fd372ca0`), limpo. Rodados em cima
  dele, todos verdes:
  - `janela_do_comandante` e `identidade_da_sala`;
  - `seele-server --lib` (576);
  - a agenda e o toque.
- **Tarefa 7, o UPnP por papel** (`efaada91`), em `alcance/porta.rs`:
  - `Papel::Servidor` tem a âncora 8383; `Papel::Sala` não tem âncora, não
    recua para a 8383 e não avisa de porta;
  - as regras se chamam «SEELE servidor» e «SEELE sala»;
  - o `fechar` só apaga a regra que aponta para este endereço. O `igd-next` 0.17
    só lista por índice, então a lista é percorrida até achar a porta, com
    teto de 128. Sem lista, a regra fica para o prazo.
  - A escada ainda passa sempre `Papel::Servidor` (`alcance.rs`); a 4f passa o
    da sala.
  - 7 testes novos e 4 reversões medidas. `seele-server --lib`: 583.
- **Tarefa 8, o `ConfereCodigo`** (`3197550a`), em
  `crates/seele-core/src/confere_codigo.rs`:
  - a chave Ed25519, o código, o pino `@CÓDIGO` e a prova de posse com o `ring`;
  - o motivo tipado sai por um `RelatoDoCodigo`.
  - Os testes fazem um QUIC real com o certificado da sala. 3 reversões
    medidas. `seele-core --lib`: 444.
- **Quando o `NOME_DA_SALA` da F1 (`7e123ee5`) chegar à `versao/0.16`:** o
  `tls.rs` passa a reexportá-lo, e os testes que escrevem `"seele-sala"` passam
  a usá-lo.

## As tarefas 9, 12a e 4d (08/10)

- **Tarefa 9, a recepção** (`crates/seele-server/src/recepcao.rs`): a campainha
  viva. A sessão que toca recebe um canal de decisão, o app assina os avisos, e
  o desligar vai pelo `Event::ServidorEncerrando` do G-servidor. O log leva 4
  símbolos do código.
  - Seis reversões medidas.
  - **O achado do `tracing`:** o teste do log reprovava junto com o módulo e
    passava sozinho. Medido no `tracing-core` 0.1.36 (`has_just_one`): 3/3
    vermelhos sem conserto, 10/10 com o `rebuild_interest_cache`, 20/20 verdes
    com um segundo despachante vivo. É o mesmo conserto do
    `segundo_despachante` do app (`1405210b`). Virou a pendência 56, com o
    texto desta frente, e o conserto dos outros testes ficou no lote Z da F5.
- **Tarefa 12a, o `Esperada`** (`a892f14a`): `Destino.esperada:
  Option<Esperada>` com `Certificado(String)` e `Codigo(Codigo)`.
  - Um método `impressao_esperada()` mantém os usos de produção numa linha
    cada, para o encontro com o lote F no `enlace.rs` ser só de texto.
  - Os 16 arquivos de conformidade tocados e o `quarto_pelo_link` rodaram,
    verdes.
- **Tarefa 10, a 4d** (`312ec92e`): o `handshake` foi partido em três fases.
  - A fase 1 vai até a assinatura, em 10 s.
  - O `tocar`, só na sala, tem o prazo dele e não segura a trava do banco.
  - O `admitir` leva a conta e a sessão, em 10 s.
  - A sala é dita por `ServerConfig.sala`, e a recepção mora em
    `Server::recepcao`. O prazo do toque saiu de constante para a
    configuração.
  - Os 10 testes de `tests/sala_toca.rs` passam, e as 4 reversões reprovam o
    teste de cada uma.
  - O aperto de todo servidor mudou de forma, então rodei o `seele-server`
    inteiro e os 42 arquivos de conformidade, um por vez: 42 de 42 verdes.
- **Anotado para a 4f** (a hospedagem da sala):
  - a sala não manda `Membros`, `MembroNovo` nem `MembroBanido` (a P5 da F1,
    `7a9591f0`), com um teste;
  - a API do ENC/2 da F2 (`0.16/enc2`, `a2181ad6`): `Convocacao::para_sala`,
    `Hospedagem::registro_no_quarto` e, para quem liga, `localizar`.

## Tarefa 12b — ligar pelo código (08/10)

- **No `connect_por`, quando se espera um código:**
  - o `ConfereCodigo` entra no lugar do `TofuVerifier`;
  - o SNI é sempre o `NOME_DA_SALA`;
  - o pino `@CÓDIGO` é lido antes de discar e gravado depois do `Session`;
  - a recusa vem tipada (`ConnectError::CodigoNaoConfere`), lida do relato.
- **Na FFI:** `CodigoNaoBate { esperado, veio }` e `SePassouPorCodigo`. **As
  frases são da casca (tarefa 7f)**: nada no app disca um código ainda, então
  essas recusas não chegam à tela antes dela.
- **No `enlace.rs`:** as duas chamadas do `connect_por` e um braço em
  `alguem_respondeu` e outro em `vale_insistir`. Na mensagem da 4d eu tinha dito
  à orquestradora que a 12b não tocaria o `enlace.rs`. Estava errado, e foi
  corrigido na mensagem seguinte.
- **A prova** é `seele-conformance/tests/ligar_pelo_codigo.rs`, com 4 testes, e
  as 4 reversões medidas reprovam.
- **Rodados:** `seele-core --lib` (445), `seele-ffi --lib` (124) e os 43
  arquivos de conformidade, um por vez, todos verdes.

## Onde a frente está (08/10, fim da tarde)

Feitas: 1–10 e 12, menos a 11. Esperando:
- **17** (a reconexão de 60 s): o lote F da F5, que mexe no `enlace.rs`. A
  orquestradora avisa;
- **16** (o `Tocando` no cliente): a P8 da F1, no `client.rs`;
- **11** (a hospedagem da sala): a P10 da F1 e o ENC/2 da F2 na `versao/0.16`;
- **13** (`ler_entrada`): a P9 da F1;
- **14 e 15**: a F2;
- **18 a 31**: a casca;
- **as duas medidas do toque que abrem o microfone**, com o dono.

## As tarefas 11 e 13 (09/10)

- **Tarefa 11, a hospedagem da sala** (`c3483884`):
  - `Hospedagem::iniciar_sala(porta, semente, apelido, ponto)`, com o gancho que
    cria a conta de quem recebe (o achado A2), a `Convocacao::para_sala` da F2
    e `PORTA_DA_SALA = 8385`.
  - Na escada, três decisões puras pela `Convocacao::assinada()`: o degrau 4
    sempre, a porta como `Papel::Sala` e o furo nunca largado.
  - A sala não manda `Membros`, `MembroNovo` nem `MembroBanido`.
  - A prova é `seele-conformance/tests/sala_pessoal.rs`, com 4 testes, mais 3
    testes de unidade na escada.
  - **A reversão do furo largado passou na prova de ponta a ponta**: o ponto
    guarda o registro até o prazo dele. A regra ganhou um teste de unidade, que
    reprova.
  - **A rodada dos outros arquivos de conformidade foi interrompida** a pedido
    da orquestradora (a 5ª integração). O `seele-server` inteiro rodou: 609 na
    lib e as integrações.
- **Tarefa 13, `ler_entrada`** (`86edd3e4`): as quatro regras da spec, em
  `crates/seele-core/src/entrada.rs`.
  - A guarda dos caracteres de endereço era redundante, medido pela reversão:
    a leitura do código já os recusa. Ela saiu, e um teste prende o
    comportamento.
  - Uma entrada de teste que prometia o O e o I/L sem os ter virou um teste de
    verdade.
- **A 14, a 15 e a 17 esperam a 5ª integração (o F)**: as três mexem no
  `enlace.rs` e na `chegada.rs`.

## A quinta integração, a 14a e a 15 (09/10)

- **O merge** de `bd58a080` entrou limpo (`d3c1c596`). A rodada inteira da 4f,
  que tinha ficado parada, passou nele: `seele-server`, `seele-core`,
  `seele-ffi` e `seele-audio` verdes, e a conformidade 46 de 46, arquivo por
  arquivo.
- **14a + 15, discar pelo código** (`1f1a1b35`):
  - `Chegada::para_o_codigo(codigo, ponto, apelido)` nasce parada com zero
    candidatos. `Etapa::Localizando{ponto}` pergunta ao quarto
    (`encontro::localizar`, da F2). A resposta vira os candidatos e o bilhete:
    as dicas primeiro, o servidor depois, sem repetir, todos com
    `Esperada::Codigo`.
  - As arestas são `Parada{0} → Localizando` e `Localizando → Avisando |
    Tentando | Desistiu`. Localizar pode reprovar, ao contrário do aviso.
  - `ConnectError::NaoLocalizou(FalhaAoLocalizar)`, com sete motivos. Na FFI,
    `ConnectionError::NaoLocalizou{motivo}` e `ConnectStage::Localizando`.
  - `Destino::marca_do_aviso`: a `Batida` recebe a marca, e não a impressão.
    **O achado**: um destino de código não preparava batida nenhuma, nem na
    entrada nem na volta da bateria. A ligação não avisava a escuta, e o furo
    do lado de quem recebe nunca abria.
  - A prova de ponta a ponta é `seele-conformance/tests/discar_pelo_codigo.rs`,
    com 4 testes. Um deles é o «sem *hairpin*»: o ponto só conhece um endereço
    público morto, e a ligação entra pela dica.
  - Nove reversões medidas, listadas no commit.
- **A linha provisória de `ETAPAS`** (`e3e32bfe`): «LOCALIZANDO PELO PONTO DE
  ENCONTRO», num commit isolado e marcada para a 7f trocar. Sem ela, o guarda
  `every_stage_of_an_arrival_has_a_sentence_in_the_page` fica vermelho, e foi
  visto vermelho antes da linha.
- **A rodada depois dos dois commits**: `seele-server` verde, e a conformidade
  47 de 47, arquivo por arquivo, com o arquivo novo. A `Batida` mudou para toda
  conexão que traz `enc=`, e por isso a rodada foi inteira. `seele-core`,
  `seele-ffi` e o `seele-app` inteiro passaram antes do commit.

## Tarefa 17 — a bateria da ligação (09/10)

- `Battery::da_ligacao` dura um minuto (`CARGA_DA_LIGACAO`). O motor a escolhe
  quando o destino espera um código (`bateria_de`). A carga virou campo da
  bateria.
- **A volta já era ao `IP:porta` que atendeu**: o motor nasce por candidato,
  com o destino de quem venceu. A prova é de ponta a ponta. A ligação entra
  pela segunda dica, porque a primeira e o público estão mortos. A sala cai
  sem despedida e volta na mesma porta, e a ligação volta sozinha, com a
  bateria começando em no máximo um minuto.
- Um guarda novo prende a carga dentro da `TOLERANCIA_DE_RECONEXAO` da
  campainha. Uma bateria mais longa voltaria tocando.
- Reversões: o motor com cinco minutos, e a carga em 61 s. A volta ao
  `IP:porta` não tem reversão direta: o motor não guarda lista a que voltar.
  A disposição do teste é conferida nele.
- `porta.rs` da conformidade passou a dizer três binários de porta fixa.
- **Espera a 16**: a volta de um convidado sem tocar de novo. Ela precisa do
  cliente que lê o `Tocando` para a primeira entrada. A frase «A LIGAÇÃO CAIU»
  é da 7f.
- **A rodada inteira depois da 17** (`39faee3d`, liberada de novo pela
  orquestradora depois da bateria do `linha/0.15`): 46 de 47.
  - O vermelho foi `tela_por_um_par::retirar_o_consentimento_de_assistir_por_par_devolve_a_tela_ao_servidor`,
    com `Error: SemResposta`. Enquanto ele falhava,
    `a_porta_tomada_por_um_instante_nao_reprova_a_volta_do_servidor`, que
    disputa a porta de propósito, já corria havia mais de 60 s.
  - Rodado de novo: o teste sozinho passou 3 de 3, e o arquivo inteiro 19 de
    19.
  - **Por leitura, e não por medida**, nada da 14a nem da 17 está no caminho
    dele: ele não usa bilhete nem destino de código. São os dois únicos lugares
    em que as duas tarefas mudaram o comportamento de uma conexão.
  - O arquivo tem uma família intermitente conhecida, a pendência 41. Lá o
    sintoma registrado é «a paciência acabou», e não `SemResposta`: **não
    afirmo que é a mesma**. Separar de vez pediria rodadas intercaladas contra
    a base, como as que a pendência 41 fez.

## Onde a frente está (09/10, depois da sexta integração)

Feitas: 1–15 e 17, menos a volta do convidado sem tocar, que é da 17 e espera
a 16. Esperando:
- **16** (o `Tocando` no cliente): a P8 da F1;
- **14b e 18 a 31**: a casca;
- **as duas medidas do toque que abrem o microfone**: ficam com o dono, fora
  do «fechar tudo» (a orquestradora as pôs na lista dele).

O dono pediu, pela orquestradora, que a frente feche sem chamá-lo. Quando a P8
e a casca forem liberadas, a F7 segue sem confirmação a cada passo, com as
regras de sempre.

