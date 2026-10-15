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
| — | merge da `versao/0.16` (`c8b3cd47`), a sétima integração | feito; a árvore é a mesma da `c8b3cd47` | `52d015f4` |
| 22 | 4b, a identidade por máquina: a metade da FFI | **feita** (porte do `9ff9b73c` da 0.15); a semente da sala e o SEU CÓDIGO esperam a casca | `58552de8` |
| — | os testes da corrida de volta (tirados por engano em `b4f5ef94`) | feito | `b6280079` |
| 14b | a pendência 55: a metade de núcleo (o quarto junto com a corrida) | **feita** | `2b2bf744` |
| — | o teste da janela de furos que caía sob carga (pendência 29, causa 3), pedido pela orquestradora | consertado | `7b2bbb96` |
| — | merges da `versao/0.16` (`f7fb8ab9`, `ffd8e4fb`): a oitava e a nona integração, com a `ui/` e o `main.rs` liberados | feitos | `23aed9df`, `72089800` |
| 18 | P2a, as hospedagens e o `onde_estou`: a metade do `main.rs` | **feita**; a da casca espera a vez na `tela-sessao.js` e na `tela-boot.js` | `eb3f1be1` |
| 21 | P2d, a saída do app | **feita**, menos a confirmação ao fechar, que é de tela | `76af8287`, `2205fc46` |
| 31 | os contadores da campainha no `seele.log` | **feita** | `a87ba4f9` |
| — | merge da `versao/0.16` (`e3970b14`), a décima, com o lote V da F5 na FFI | feito | `e2267f6c` |
| 14b | a pendência 55 inteira: a ponte pergunta ao quarto, e o `connect` deixa de perguntar antes | **feita** | `80ac45d2` |

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

## A sétima integração e a metade da FFI da tarefa 22 (09/10)

A orquestradora avisou que a P8 está parada, sem previsão, esperando o dono, e
que a casca espera as ondas 5 e 6 da F5. Pediu o que não dependesse das duas.

- **O merge** de `c8b3cd47` (`52d015f4`): a árvore é idêntica à da integração
  verde.
- **Tarefa 22, a metade da FFI** (`58552de8`). O launcher já passa
  `SEELE_IDENTIDADE` e põe `SEELE_HOME` em `dados/servidor-<linha>`. A FFI da
  0.16 lia `<home>/identity.key`, e uma 0.16 aberta por um launcher criaria uma
  chave nova: a pessoa seria outra.
  - O porte é o `9ff9b73c` da `linha/0.15`, com o mesmo contrato e a mesma
    forma. A regra ficou na FFI, e não na casca como o plano dizia: nenhuma
    assinatura mudou, e o `main.rs` não foi tocado.
  - **O guarda da 0.15 não servia aqui**, e isso foi medido. Ele lê só o
    prefixo antes do primeiro `#[cfg(test)]`. Na 0.16 a produção continua
    depois dos módulos de teste, e um leitor escondido ali passava. O guarda
    novo tira cada módulo de teste e lê o resto, e tem um guarda do guarda.
  - **A `linha/0.15` tem a mesma forma de arquivo**: produção depois do primeiro
    `#[cfg(test)]`, com dois `impl Connection`. O guarda de lá tem o mesmo
    buraco para uma regressão futura. **O código de lá está certo hoje**, com
    só os dois leitores, os dois pelo montador. Avisado à orquestradora, sem
    mexer naquela linha.
  - Quatro reversões medidas, todas reprovando. A segunda (vazia valendo)
    criou uma chave nova em `crates/seele-ffi/identity.key`: o caminho vira
    relativo, e o teste roda na pasta do crate. Era artefato da reversão, nunca
    entrou no git, e foi apagado.

## A metade de núcleo da 14b (09/10)

A orquestradora liberou com a régua alta, porque toda conexão passa pelo
`correr`. Pediu primeiro o teste que mostra o candidato tardio perdido hoje,
depois a prova de que a ordem de hoje se mantém, a conformidade de quem passa
pelo enlace, e reversões.

- **Achado: a corrida estava sem teste desde 31/08.** Os cinco testes do
  `correr` (`e09d537a`) saíram no revert `b4f5ef94`, que tirava outra coisa. O
  doc continuava dizendo que a corrida era genérica para ter teste. Eles
  voltaram antes de qualquer mudança (`b6280079`), com um sexto que escreve os
  instantes de hoje (0, 250 e 500 ms). Reversões: a corrida em série e a
  defasagem pela metade.
- **O candidato tardio perdido hoje, medido**: com o lote ignorado (a corrida
  de antes), os dois testes do tardio reprovaram, e todos os outros passaram.
- **A corrida com tardios** (`2b2bf744`): o lote entra na frente de quem espera
  a vez, um lote vazio não mexe em nada, a corrida o espera, quem já ganhava
  não espera, e a posição é a ordem em que cada um começou. **O teste do lote
  vazio pegou um defeito do primeiro desenho**: o `sleep` da defasagem era
  refeito a cada volta, e um lote vazio empurrava a vez de todo mundo em
  100 ms. A vez virou um instante.
- **Só quem precisa de furo espera o quarto.** A pergunta é uma só
  (`pede_furo`, a mesma do `avisar_pelo_candidato`). Quem espera avisa a escuta
  de hoje (`Batida::com_aviso`). O endereço de hoje entra logo depois do alvo,
  e a trilha conta mais um candidato quando ele chega.
- **A conformidade nova** é `quarto_em_paralelo.rs`, com 3 testes:
  - quem está na mesma casa entra antes do prazo de um quarto mudo;
  - um servidor que mudou de endereço entra pelo que o quarto deu, registrado
    pelo `Hospedagem::iniciar` de verdade;
  - a trilha de uma volta que não entrou acaba em `Desistiu` no candidato que
    chegou depois.
- Dez reversões medidas, listadas no commit.
- **Ligar na casca espera a `ui/`**: a FFI monta o `Quarto`, e o `main.rs` deixa
  de perguntar antes.
- **A rodada inteira da conformidade está pendente.** Ela começou depois do
  commit e foi parada no quarto de 52 arquivos: a orquestradora avisou que a
  bateria da oitava integração estava começando, e a porta 8384 é uma só. Os
  três que rodaram passaram. O arquivo novo e os que a 14b toca já tinham
  passado sozinhos antes: `quarto_em_paralelo`, 3 de 3, com as reversões. Fica
  para quando a orquestradora liberar.

## O teste da janela de furos que caía sob carga (09/10)

Pedido pela orquestradora: `a_janela_fecha_dentro_do_atender_e_nao_so_no_auxiliar`
(`seele-server --lib`) caiu uma vez na rodada paralela da F3.

- **O que ele mede.** 61 avisos legítimos (um acima do teto) produzem
  exatamente 60 furos: o teto vale dentro do `atender`, e não só no auxiliar.
- **Por que caía.** A contagem lia com 600 ms por leitura e parava no primeiro
  silêncio. Um soluço da linha do teste com uma leitura pendente fazia o prazo
  vencer antes de o furo seguinte ser lido. É o «53 contra 60» da causa 3 da
  pendência 29, que já estava registrada.
- **Como foi medido.**
  - Carga de CPU não reproduz, como a pendência 29 já dizia. Com 120 processos
    em `nice 20`, os sessenta furos saíram em 7,20 s, com o maior intervalo em
    134 ms. Um tropeço meu: devia ter lido a pendência antes de carregar a
    máquina.
  - Um soluço de 800 ms provocado no meio da fila encerrou a conta em 9 furos,
    3 de 3.
- **O conserto** (`7b2bbb96`): `contar_furos` conta até os esperados sem prazo
  por leitura, e então espera um a mais. `um_soluco_no_meio` provoca o soluço
  em toda rodada. O vizinho, `a_janela_cabe_uma_entrada_legitima_inteira`,
  tinha o mesmo laço e foi consertado junto. A folga da janela ficou escrita.
- Reversões:
  - sem o teto, reprova com 61;
  - com o prazo por leitura de volta, os dois reprovam com 3, em toda rodada;
  - sem o teto e sem a espera de um a mais, passa: o controle.
- **Um erro meu durante a medida**: o shell de uma rodada de carga morreu antes
  de matar os processos. 120 `yes` em `nice 20` ficaram vivos por uns seis
  minutos, com a carga média em ~77, durante a bateria da oitava integração.
  Foram mortos (`pkill -x yes`, eram exatamente os 120). Dito à orquestradora.
  Da próxima vez, cada processo de carga se mata sozinho com prazo.

## A casca liberada: as tarefas 18 e 21 (09/10)

- **Tarefa 18, a metade do `main.rs`** (`eb3f1be1`):
  - `Session.hospedagens { guardado, pessoal }` no lugar de `hospedagem` e de
    `servidor_no_ar`;
  - o comando `onde_estou()`, que fica em `AGUARDANDO_TELA` até a
    `tela-sessao.js` e a `tela-boot.js` perguntarem a ele;
  - o `hospedado_aqui` saiu. A pergunta da lista de visitados virou
    `endereco_desta_maquina`, com a mesma leitura do endereço.
- **Tarefa 21** (`76af8287`, `2205fc46`):
  - a sala que fecha diz `ChamadaEncerrada`, e o guardado continua dizendo
    `ServerShuttingDown`, mudança localizada no `hospedagem.rs`;
  - fechar o app segura a saída, despede as duas hospedagens ao mesmo tempo
    (1 s de teto) e sai. A saída é segurada **só com `code: None`**: o runtime
    manda esse evento do `Destroyed` da última janela, com a página já fora.
    Medido no `tauri-runtime-wry` 2.11.4.
  - **Um guarda da F5 mudou**: o `a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`
    (lote R) proibia todo `prevent_exit`. Passou a permitir só essa forma, e
    continua proibindo `prevent_close` e `CloseRequested`. A reversão «segurar
    qualquer saída» reprova nele.
  - Fica para a tela: a confirmação ao fechar, que vale só para o guardado.

## A 14b inteira (10/10)

- **A ponte liga o quarto à `Chegada`** quando há impressão conferida
  (`com_o_quarto`, que só pergunta com bilhete). O endereço de hoje sai de
  `candidato_do_quarto`, montado como os outros.
- **O `connect` do app deixa de perguntar.** Nada do que o quarto dá volta para
  ele, e é isso que mantém a lista de conhecidos limpa.
- As regras que eram do `connect` moram na `Chegada` (a resposta que já corre,
  a escuta só no IP do servidor) e na ponte (a chave canônica do endereço de
  hoje), com testes de comportamento. Os casos do `ja_esta_na_corrida` foram
  para os testes de `candidato_do_quarto`. O `ja_esta_na_corrida` e a
  reexportação de `bilhete_desta_volta` saíram: eram segunda cópia.
- Os três guardas da casca sobre o quarto viraram um. O
  `ocupante_da_escuta.rs` passou a percorrer o caminho de verdade, e o
  controle dele reprova com a ponte sem o quarto.
- **A conformidade de quem passa pelo enlace**: 16 de 17 numa rodada, arquivo
  por arquivo. O `acceptance_m3` reprovou um teste,
  `a_restarted_server_keeps_its_accounts`, com `SemResposta` em 24 s. É a
  assinatura registrada da pendência 29, com a receita na 43: carga, e um teste
  que usa o `Client::connect` direto, que a 14b não tocou. Sozinho, o arquivo
  deu 3 de 3, 8 de 8 cada, em 4 s.


## A sala viva, no `0.16/sala-viva` (10/10)

O conjunto (tarefas 23, 24, 26, 27 e 29) num ramo à parte, para entrar inteiro:
não há sala que toca sem ninguém atender. A décima primeira integração veio
para o ramo em `97ae94a7`, com um conflito só, no `AGUARDANDO_TELA`, e os dois
lados mantidos.

**A FFI** (`6be6d49a`, `d0a97fba` no núcleo):
- `semente_da_sala` e `codigo_desta_maquina` saem da pasta da identidade, pela
  mesma chave da impressão. A `SementeDaSala` não é `Serialize` nem `Clone`, e o
  `Debug` não a imprime;
- `ConnectConfig::codigo_esperado`: o ATENDER confere `Esperada::Codigo`.
  `esperada_de` decide num lugar só, e com o código o quarto não é perguntado;
- a agenda, a leitura da entrada, o `Codigo`, o `Toque` (que toca na saída
  padrão quando a escolhida sumiu) e o `ponto_de_encontro`;
- **achado no caminho, e consertado no núcleo:** um primeiro contato pelo
  código saía `FirstContact`, o cego. O código prova a chave dentro do TLS, e
  agora sai `FirstContactVerified`. A `atender_pela_ponte.rs` o prova de ponta a
  ponta.

**O Rust do app:**

| tarefa | o que | commits |
|---|---|---|
| 23 | a sala sobe sozinha no `setup` e anuncia o estado por `seele://sala`; a semente nunca atravessa a webview (guarda); o ponto é o de quem liga; «receber ligações» desliga e religa; a sala que termina de subir depois do desligar desce | `3f3c3aff`, `7546c9e8` |
| 24 | o toque por `seele://toque`, com pedido de atenção ao sistema e nunca `set_focus`; o alarme para no último toque **e quando a sala fecha**, provado com a sala de verdade; cada toque é anotado na agenda | `fad0662a` |
| 27 | `atender`, `recusar` e `encerrar_a_ligacao`; o `connect` confere a sala desta máquina pelo código; `onde_estou_na`, com o nome e a forma do lote X da F5 | `fcbc38cd` |
| 29 | `meu_codigo`, a agenda e o `bloquear` ao vivo; **os bloqueados valem desde a subida da sala** | `17966fba` |

**A casca:**

| peça | commits |
|---|---|
| as frases da sala (isolado) e o guarda delas | `f2c57c54`, `0838a73c` |
| o `ejetar(soOCliente)`, que desmonta só o cliente (isolado) | `58926889` |
| a faixa `#toque`, ATENDER e RECUSAR, com a parte da ponte do navegador | `a53c1349` |
| o SEU CÓDIGO, a agenda, «receber ligações» e a linha da sala, com a outra parte da ponte | `086082d0` |

- Toda peça da sala leva `data-sala-pessoal`, que as cascas do navegador
  escondem (combinado com a F8). Os lados na ponte são os do
  `F4/ponte-no-pc.md`. Escrevi as linhas a pedido da F4, junto com a peça que
  chama cada comando: o guarda dela reprova uma ponte que lista o que a casca
  ainda não chama.
- Os roteiros novos `toque.js` e `sala.js` só valem no desktop. A F4 os tira da
  rodada da casca larga.

**Medido no caminho:**
- o desligar precisava do `encerrar`: com o `despedir`, a 8385 ficava presa, e
  religar não subia;
- a primeira reversão do bloqueio ao vivo **travou** a suíte em vez de
  reprovar: sem rede, nenhuma sessão vence o toque. Toda espera de decisão dos
  testes da sala ganhou prazo;
- este Chromium sem cabeça não roda `requestAnimationFrame`: o `#anuncio` fica
  vazio com a frase dita, e o roteiro mede a chamada do `anunciar`;
- sob carga de 21 a 54 (outras sessões compilando), três coisas reprovaram uma
  vez e passaram sozinhas: duas provas do enlace com prazo de 20 s, o
  `discar_pelo_codigo` e o `texto-maior-390` da bancada do celular. Nenhuma
  passa pelo que mudou; é a assinatura da pendência 29.

**Como está:** app 694/694; carga limpa; 31/31 roteiros; `ponte-sem-gesto`
limpa nas duas cascas; a bancada do celular inteira (72, e o G3 igual nos dois
motores). A conformidade da sala (`atender_pela_ponte`, `sala_pessoal`,
`ligar_pelo_codigo`, `discar_pelo_codigo`, `quarto_em_paralelo`) passou arquivo
por arquivo depois da décima primeira integração; desde o aviso da
orquestradora, não rodo conformidade.

**O que ficou de fora, dito para não ficar calado:**
- **a metade da casca da 18**: o `estado_da_porta().hospedando` da
  `tela-sessao.js` e o `nossoServidorEm` da `tela-boot.js` ainda respondem «estou
  no meu servidor?» pelo critério deles. O `onde_estou` saiu da
  `AGUARDANDO_TELA` quando a faixa passou a chamá-lo, e o guarda já não cobra
  isso;
- as tarefas 19 e 20 (o `disconnect` que desmonta só o cliente, e VOLTAR AO MEU
  SERVIDOR), a confirmação ao fechar (21, só o guardado) e as 28 e 30 (LIGAR pelo
  código), que esperam a 16 da F1;
- o lembrete de atenção do macOS (`Informational` a cada 4 s) não tem teste de
  relógio; a lógica do alarme tem;
- a agenda é um arquivo regravado inteiro. Um nome dado no instante em que um
  toque termina pode perder uma das duas escritas. Raro, e da conveniência e não
  da segurança;
- **depois do encontro com o lote X da F5:** o teste da sala com
  `codigo_esperado` provando `onde_estou_na == Pessoal` pela chave.

## A sala viva na décima segunda (10/10)

- A sala viva entrou na `versao/0.16` (`6e845e0d`), com o lote X da F5 integrado
  antes. Trazida para o `0.16/sala-viva` em `9f170f3f`, sem conflito.
- **O encontro com o X, provado** (`daf25ba9`):
  `a_sessao_na_propria_sala_e_pessoal_pela_chave_e_nao_pelo_endereco`. Quem
  recebe entra na própria sala pela ponte, com `codigo_esperado`. A impressão
  gravada é a da sala, e o `onde_estou_na` diz `Pessoal` com o endereço
  `127.0.0.1` e também com um nome de fora. Reprovam: o `onde_estou_na` sem a
  impressão da conexão, e a ponte sem `gravar_impressao`.
- **Dois ajustes da integração me tocaram:**
  - a tabela dos neutros da bancada `ponte-web.cjs` não tinha o `estado_da_sala`
    nem o `toques_agora` que eu pus na ponte (`8e7fcd37`). Eu rodei a
    `ponte-sem-gesto.cjs` e não a `ponte-web.cjs`, que confere a resposta de cada
    neutro. **Quem põe um neutro na ponte roda as duas;**
  - o `connect` passou a ler a hospedagem para conferir a sala, e a marca
    `sala::` da F4 o pegou. Ele entrou em `leem_e_seguem_a_lista` (`f7d53427`).
- A VPS foi reimplantada com o ENC/2: a sala pessoal acha o ponto de produção.

## As tarefas 19 e 20: sair não derruba o servidor (10/10)

Desenhadas durante a pausa da M3, só lendo (`fecfa5d8`, `ceb2a490`), e
compiladas depois dela. No `0.16/sala-viva`, cada etapa verde sozinha:

| etapa | commit |
|---|---|
| o Rust e a ponte: o `disconnect` só da sessão, `parar_de_hospedar`, `meu_servidor`; a `ATENDIDOS_APESAR_DA_HOSPEDAGEM` sai com o ramo no guarda da F4; o guarda novo da 19 | `8b82f0eb` |
| a `tela-chamada.js` (isolado): o SAIR sai sem caixa | `b200830a` |
| a `tela-sessao.js`: as frases dizem que o servidor continua; o `ejetar()`; a pergunta ao `onde_estou` (a metade da `tela-sessao.js` da 18); o VOLTAR na trilha | `755e33ce` |
| o VOLTAR AO MEU SERVIDOR e o PARAR DE HOSPEDAR na entrada (`camada-hospedagem.js`), com o roteiro `meu-servidor.js` | `620f1d8b` |
| a `tela-fim.js`, e o `desmontar_a_sessao` que ninguém mais chama | `d46dcbb4` |

**Medido no caminho:**
- a ordem que eu tinha anotado quebrava o SAIR entre dois commits: a
  `tela-chamada.js` chamava o `hospedandoAqui`, que a `tela-sessao.js` renomeia.
  Ela entrou antes;
- a `tela-fim.js` não pôde ficar isolada: sem ela, o `desmontar_a_sessao` ficou
  sem chamador, e o guarda dos comandos sem chamador reprova. Ele saiu no mesmo
  commit;
- o guarda do estado de topo da sessão pegou o `meuServidorNaTrilha`, que eu não
  tinha esquecido ao sair.

**Como está:** app 711/711; 32/32 roteiros; as duas bancadas da ponte limpas; a
bancada do celular passa (85, e o G3 igual nos dois motores). Cada guarda e cada
passo do roteiro reprova na reversão.

**O que ficou de fora:**
- o teste de um MOD do guardado que continua respondendo enquanto o dono visita
  outro servidor. Ele vale por construção (o servidor não cai mais), e ainda não
  tem prova;
- com o guardado no ar, o HOSPEDAR AQUI da entrada ainda responde «JÁ ESTOU
  HOSPEDANDO NESTA JANELA». O bloco do VOLTAR fica logo acima dele. Fazer o
  botão voltar é da `tela-boot.js`, na fila da F5, com a metade dela da 18;
- **a confirmação ao fechar (21)** pede decisão da orquestradora: o guarda de
  credencial da F5 proíbe vetar o fechamento da janela, e no macOS o botão do
  sistema e o Cmd+Q passam sem pergunta.

## Tarefa 21: a confirmação ao fechar (10/10)

Decidida pela orquestradora em 10/10, no molde da exceção do `beforeunload`.
No `0.16/sala-viva`, em três commits, cada um verde sozinho:

| etapa | commit |
|---|---|
| a `esquecerARecusaDoApelido()` da F5, por `cherry-pick` do `ca2589f9` (`0.16/lotes`), com o mesmo `patch-id` | `b276e569` |
| a exceção no guarda do lote R (isolado) | `a1423896` |
| o ouvinte, o duble e o roteiro `fechar.js` | `730048aa` |

- **Um ouvinte só, no `camada-hospedagem.js`**: o `onCloseRequested` cobre o
  botão vermelho do macOS, o Alt+F4 e o botão da casca (o `close()` também pede
  o fechar). A parte 1 aprovada, uma pergunta no `#janela-fechar` com a linha
  do `base.js` num commit isolado, ficou desnecessária.
- **Antes de qualquer veto, as credenciais da tentativa somem**:
  `fecharAceiteDeMods()` e `esquecerARecusaDoApelido()`. A segunda é da F5:
  o guarda dela proíbe outro script de nomear o `recusaDoApelido`.
- **Só o guardado pergunta.** Sem servidor no ar (`meu_servidor` devolve
  `null`), fecha direto. A sala pessoal não pergunta: ela se despede sozinha.
- **A exceção no guarda do lote R** (`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`,
  parte 4), revisada pela F5:
  - o `close-requested` cru continua proibido;
  - cada `onCloseRequested(` é cobrado, e o trecho vai até o parêntese que
    fecha a chamada, não até o primeiro `.catch(`;
  - antes do `preventDefault`: as duas credenciais, e `invoke("meu_servidor")`
    antes do `if (!meu) return;`;
  - um corte cedo (um `")"` num texto) reprova dizendo que o guarda não leu o
    ouvinte, em vez de passar calado. Os dois últimos itens vieram da revisão
    da F5.

**Reversões, cada uma reprovando:** segurar antes de esquecer; não esquecer o
apelido; segurar sem perguntar pelo servidor; o `close-requested` cru; um
`.catch(` antes do veto; um `")"` num texto antes do veto; o `return` antes da
pergunta. As duas últimas passavam caladas sem os ajustes da F5.

**O roteiro `fechar.js`:** sem servidor, o pedido passa e a recusa do apelido
é esquecida; com o guardado no ar, o pedido é segurado e a caixa cita o
servidor, sem fechar antes do sim; depois do sim, a janela fecha e o pedido
seguinte passa.

**Como está:** cada etapa sozinha passa o `--test frontend` (292), a carga, os 33
roteiros e as duas bancadas da ponte; sem o ouvinte, o `fechar.js` reprova («a
casca não ouve o pedido de fechar a janela»). Com a (a) e a (b) na árvore: app
718/718 e a bancada do celular passa.

**O que ficou de fora — o Cmd+Q.** Medido no código (tauri 2.11.5,
tauri-runtime-wry 2.11.4, tao 0.35.3, muda 0.19.3): o item Quit do macOS manda
o `terminate:`, o `tao` não implementa `applicationShouldTerminate:`, e o
`applicationWillTerminate:` vai direto ao `RunEvent::Exit`. O Cmd+Q, o Sair do
Dock e o logout passam ao largo do `onCloseRequested` **e** do `ExitRequested`:
não perguntam, não despedem as hospedagens e **não despedem nem a própria
sessão** (o `despedir_se` da pendência 50, que na `v0.15.1` publicada também só
roda no `ExitRequested`). Leitura de código, ainda sem medida em tempo de
execução. A orquestradora deu o sim para os dois consertos, escritos na pausa da M3:
(a) o Sair nosso no menu do macOS, que fecha a janela e passa pela pergunta;
(b) a despedida de melhor esforço no `RunEvent::Exit`, com teto de 1 s. Os dois
estão escritos, com 5 testes e 2 guardas, cada um reprovando na reversão (11),
e **ficam sem commit até a medida no app de verdade**, que a orquestradora roda
(a tecla e o clique são dela): se o runtime não entregar a despedida no
`RunEvent::Exit`, a (b) sai. O desenho e a medida estão no plano, na tarefa 21.

## A 21 medida no app: a janela que não fechava, a (a) e a (b) (10/10)

A orquestradora rodou duas medidas no app de verdade, com builds `.app` de
nome próprio (`SEELE-sem-ab` e `SEELE-com-ab`), o `seeled` numa porta só dele
como medida de quem ficou, e um segundo cliente.

**A primeira medida achou um defeito da 21**: o botão vermelho, o Sair do menu
e o sim da pergunta não fechavam nada. O processo seguia vivo, sem janela à
vista, dentro do servidor por minutos, sem uma linha de saída no `seele.log`.

- **A causa, medida antes do conserto.** Com um ouvinte de `onCloseRequested`
  de pé, o Tauri segura o fechar no Rust e entrega a decisão à página; sem
  `preventDefault`, quem termina de fechar é o embrulho do `@tauri-apps/api`,
  pelo `destroy()`. A capacidade `janela` dava o `close` e não o `destroy`:
  «window.destroy not allowed», numa promise que ninguém via (a página não tem
  ouvinte de `unhandledrejection`). Reproduzido pelo runtime falso contra a ACL
  gerada.
- **O conserto** (`ba9772c9`): o `core:window:allow-destroy`, com o esquema
  gerado, e o guarda `a_pagina_que_ouve_o_fechar_consegue_terminar_de_fechar`
  (`tests/permissoes.rs`), que reprova sem a permissão.
- **Por que o roteiro não pegou** (`1e0a2a8a`): o duble aceitava tudo. Agora ele
  fecha como o Tauri — o `close()` vira pedido quando há ouvinte, sem veto o
  embrulho chama o `destroy()`, e as permissões vêm do `janela.json` — e o
  `fechar.js` exige a janela destruída. Sem a permissão, ele reprova com a
  frase do Tauri.

**A segunda medida**, com o conserto e com diagnóstico que não foi a commit
(a página dizendo ao `seele.log` quando recebe o Cmd+Q; todo evento do menu):

| saída | sem a (a) e a (b) | com a (a) e a (b) |
|---|---|---|
| botão vermelho | sai em ~1 s; o servidor vê na hora | igual, e «a despedida já tinha rodado» no fim |
| item Sair do menu | — | `seele-sair` → «Sair do menu…» → a saída; o servidor vê na hora |
| Cmd+Q pela tecla | a página recebe o keydown, o app sai pelo `terminate:`, **o servidor só vê 20 s depois** | a página recebe, o menu dispara o `seele-sair`, a saída; o servidor vê na hora |
| `quit app` (Dock, logout) | o servidor só vê 20 s depois | «se despediram levou_ms=96»; o servidor vê na hora |

Com o guardado no ar (`--hospedar`, um segundo cliente dentro): o Cmd+Q mostra
«FECHAR O SEELE»; CANCELAR deixa tudo de pé; o sim fecha em ~1 s, e quem estava
dentro lê «O SERVIDOR ESTÁ ENCERRANDO».

**Os commits:** a (b) em `ef364aa9` (a despedida do fim no `RunEvent::Exit`,
teto de 1 s) e a (a) em `f835b52d` (o Sair nosso no menu do macOS). App
719/719; 33/33 roteiros; as duas bancadas da ponte.

**Decidido e registrado:**
- **o «Quit and Keep Windows» (Option+Cmd+Q) sai do menu.** É o alternativo que
  o AppKit só põe ao lado de um Sair com `terminate:`. O SEELE não restaura
  janelas na reabertura (o `tao` não usa a restauração de estado do AppKit), e
  ele só fazia o que o Cmd+Q antigo fazia: sair sem perguntar. Recuperá-lo
  reabriria esse atalho.

**O que ficou sem resposta ou de fora:**
- **o Cmd+Q da primeira medida não se explica pela ACL**: a linha «Sair do
  menu» sai antes do `close()`, e naquela rodada ela não saiu. A segunda medida,
  com o mesmo envio, não reproduziu. Fica como observado uma vez, sem causa;
- o Cmd+Q com a janela minimizada não foi medido (o `AXMinimized` não pegou);
- **as listas do RT** (`0.16/lotes`): o `fechar.js` tem de entrar na lista do
  job `bancadas` do `.github/workflows/ci.yml` e na `roteiros_da_casca` do
  `empacotar/publicar.sh`, entre o `escolher-transmissao.js` e o `hospedar.js`.
  As listas não existem no `0.16/sala-viva`: entram no encontro da 14ª;
- a página sem ouvinte global de `unhandledrejection`, que deixou este defeito
  mudo, é do `base.js` e ficou com a orquestradora.

## A 21 quebrou a bancada do celular na 14ª (11/10)

**O que aconteceu.** O `1e0a2a8a` pôs no duble da `carga-da-casca.py` o
marcador `__PERMISSOES_DA_JANELA__`. A `tools/bancada-do-celular.py` monta o
mesmo duble por conta própria e só preenchia os marcadores antigos: a página
morria no primeiro script («is not defined»), e 46 casos reprovaram na bateria
da 14ª, inclusive o G3. A orquestradora achou por bisseção (verde em
`538ce467`, vermelho em `d9e31226`) e consertou na integração (`5c5dc9e0`): o
marcador preenchido, e a bancada recusa pelo nome qualquer marcador que sobrar.
Reproduzido aqui em `d9e31226`; verde depois do avanço para a `versao/0.16`
(`a400c4eb`).

**O erro foi meu**: rodei a bancada do celular antes de mexer no duble, e não
depois. A verificação que eu rodava (a carga, os roteiros e as duas bancadas
da ponte) não a incluía.

**A regra, da orquestradora, daqui em diante:** toda mexida no duble, na `ui/`
ou na carga roda a bancada do celular antes da entrega, junto com os roteiros
e as bancadas da ponte. Ela usa o duble da carga, então qualquer mudança nele a
alcança. A minha verificação passou a rodá-la.

## A metade da casca da tarefa 18 (11/10)

Sobre a `versao/0.16` da 15ª (`a400c4eb`), com o W2, o W3, o E54 e o
`depoisDeConectar` da F8 já dentro. Cada etapa verde sozinha, verificada do
zero (a carga, os 37 roteiros, as duas bancadas da ponte e a do celular; o
`--test frontend`):

| etapa | commit |
|---|---|
| o duble responde o `onde_estou` pela sessão, como o Rust (Guardado no endereço do servidor hospedado, Outro fora dele, Nenhum sem sessão) | `714fbeb5` |
| a dispensa de conferir a própria chave vem do `onde_estou` depois do `connect`, e não do endereço; o `nossoServidorEm` sai; os guardas; o passo 6 do `meu-servidor.js` | `c4317f0b` |
| HOSPEDAR AQUI, com o guardado no ar, volta a ele; o passo 7 | `aada60a5` |

- **«Estou no meu servidor?» tem uma resposta só.** A última régua própria era
  o `nossoServidorEm` da entrada: o endereço que a janela hospedou. Agora o
  `conectar` pergunta ao `onde_estou` depois do `connect`, e só «Guardado» (a
  chave do aperto de mão é a do servidor daqui) dispensa a `#tela-auth`. O mesmo
  endereço com outra chave confere.
- **O desenho da F5 e da F8 não mudou**: a conferência da tentativa logo depois
  do `connect` (o guarda do lote E54 reprovou a minha primeira versão, que
  perguntava antes dela), e o `depoisDeConectar` logo depois de uma
  conferência. A pergunta é uma espera e fica entre duas conferências, como
  depois de cada espera da entrada (lote W2).
- **O guarda da dispensa mudou de nome**:
  `a_dispensa_de_conferir_a_propria_chave_vem_da_chave_desta_conexao`, no
  lugar de `the_flag_that_skips_the_key_check_is_cleared_before_the_attempt_and_not_after`.
- **Reversões, cada uma reprovando:** o código de antes (passo 6); a pergunta
  antes da primeira conferência; sem a segunda conferência; «!== "Nenhum"»; o
  endereço de volta; a `tela-sessao` pelo `estado_da_porta`; sem o conserto do
  HOSPEDAR AQUI (passo 7).
- **Medido no caminho:** uma rodada da bancada do celular reprovou o
  `aba-servidor-390-papel` com a máquina carregada e um roteiro meu rodando
  junto; isolado ele passou 3 de 3, e as rodadas inteiras seguintes passaram.
  E duas vezes mexi em arquivo com a verificação rodando: a rodada não valeu
  como prova de etapa nenhuma, e cada etapa foi verificada de novo, sozinha.

App 727/727 na ponta.

## A 21 quebrou duas bancadas da janela (11/10)

A F5 achou, na `versao/0.16` pura (`57fd0841`), a `conversa-com-estado` caindo
inteira e a `ajustes-v013` reprovando por «nenhum erro na página», as duas com
«Cannot read properties of undefined (reading 'catch')» em
`camada-hospedagem.js:137`. Os dubles de janela das bancadas têm
`onCloseRequested() {}`, que devolve `undefined`, e o `.catch` do ouvinte da 21
estourava na carga, levando junto o VOLTAR e o PARAR do mesmo arquivo.

**O conserto** (`07c6ae0f`): o registro vai em
`Promise.resolve(janelaDoApp?.onCloseRequested?.(…))`. O guarda da exceção do
fechar passou a ler a chamada direta **e** a opcional, e a reprovar qualquer
outra menção ao método: com a forma nova, o guarda antigo não achava ouvinte
nenhum, e um veto sem as condições passaria calado (provado por reversão).

**O erro foi meu, e é o segundo do mesmo tipo** depois da bancada do celular:
a verificação rodava a carga, os roteiros, as bancadas da ponte e a do
celular, e não as 34 bancadas da janela. Agora roda todas. Com o conserto, 33
de 34 passam; a do executor no navegador pede o motor com o wasi-sdk, que não
baixei, e não carrega a camada.

**A regra, daqui em diante, para toda mexida na `ui/`, no duble ou na carga:**
a carga, os roteiros, as duas bancadas da ponte, a do celular **e todas as
bancadas da janela** (`apps/seele-app/bancada/*.cjs`), antes da entrega.

## D-sala-2 decidida, e a conferência da D-sala-1 no Windows (15/10)

- **D-sala-2 (do dono):** a sala pessoal sobe sozinha quando o app abre. É o
  que a tarefa 23 já faz (`3f3c3aff`, `7546c9e8`), e nada muda no código.
- **D-sala-1** continua «até o dono rever». O dono liberou a orquestradora para
  conferir no PC de testes pelo SSH. O roteiro, sem tela e sem mexer no
  firewall à mão, está em `conferencia-firewall-windows.md`. Ele já aponta,
  pela leitura do código:
  - a frase da sala fala da porta 8383, e a sala escuta na 8385;
  - o app só pergunta se existe uma regra chamada `SEELE`, sem conferir se ela
    permite ou bloqueia;
  - a casca tem uma linha no PERFIL, e não uma pergunta ao abrir;
  - o `alcance::firewall` do servidor não é chamado por ninguém.

## A faixa do toque e os consertos da D-sala-1 (15/10)

**A faixa do toque cobria o veredito** (achado da F3, pelo guarda 11): a 1024,
o centro do texto de um primeiro contato caía embaixo dela; reproduzido aqui, e
a 1440 ela também cruzava parte do texto. Agora ela **empurra a tela em vez de
cobri-la** (`58b597b4`): acesa, a `.tela` à vista abre no topo o espaço até o pé
dela. O passo 5 do `toque.js` mede retângulo contra retângulo.

**A conferência no PC de testes** (a orquestradora, pelo SSH, só lendo; os
passos que instalam ficaram com o dono): a v0.15.1 instalada, a regra `SEELE`
certa (entrada, permite, qualquer perfil, UDP, pelo programa), e um par TCP/UDP
`seele-app.exe` do aviso do Windows, só no perfil público, em minúsculas. O
passo 6 confirmou no app de verdade o ramo «outra cópia». O instalador e o
portátil da 0.16 saíram de `95ff2689`.

**Os consertos pedidos pela orquestradora** (os achados 1, 2 e 4):

| conserto | commit |
|---|---|
| 2: o app lê a regra num lugar só (`src/firewall.rs`), pelo PowerShell — ligada, de entrada, UDP, do caminho inteiro do programa que roda, no perfil da rede ativa, com o bloqueio vencendo | `1daa487a` |
| 4: o `alcance::firewall` do `seele-server` sai (sem chamador, só o nome do arquivo) | `d7c51e41` |
| 1: os textos do instalador e do ADR 0043 dizem o que a regra é; o comentário da caixa «desmarcada» sai | `4f6ca33e` |

- **Os testes usam a saída de verdade do PC**, e não amostras inventadas. O
  leitor que roda no Windows tem um teste `cfg(windows)` (o CI e o PC), e um
  gêmeo diz «PARCIAL» fora dele.
- **Reversões, cada uma reprovando:** o bloqueio ignorado; só o nome do arquivo;
  o perfil ignorado; e a leitura de antes («existe uma regra chamada SEELE»),
  que deixa passar um bloqueio com esse nome.
- **Fica com o dono** (o achado 3): a D-sala-1 diz «pergunta», e a casca tem uma
  linha no PERFIL com um botão.
- **Fica anotado:** o botão «ABRIR A PORTA AGORA» (o da sala, meu, e o da tela
  de hospedar, da F5) cria a regra do SEELE **instalado**; para a cópia
  portátil e para um bloqueio do aviso do Windows ele não resolve nada, e hoje
  aparece igual. Pedir a mudança passa pela orquestradora.

**Medido no caminho:**
- o scratchpad em `/private/tmp` foi limpo entre 11 e 15/10, com os scripts de
  verificação e as pastas de comparação que eu tinha passado à F8; recriados;
- a bancada do celular reprovou de forma intermitente o `entrada-desfechos-390`
  («PONTE_NO_LUGAR_DO_TAURI is not defined»), sem relação com a faixa; foi à F8
  com a pasta guardada.

**Como está a ponta** (`4f6ca33e`, sobre a `versao/0.16` `95ff2689`): app
743/743; os 40 roteiros; as duas bancadas da ponte; a do celular; e 33 das 34
bancadas da janela (a do executor pede o motor com o wasi-sdk). A
`voz-no-navegador` reprovou uma vez com «Could not start audio source» e passou
na seguinte, com o `.wasm` recompilado: a minha verificação usava um `.wasm` de
antes da integração, e passou a recompilá-lo sempre.

## O motivo do firewall na tela, e o botão só quando resolve (15/10)

**Provado no PC antes** (a orquestradora, na ponta `3ffdd92b`): o
`cargo test -p seele-app --bin seele-app -- firewall::` deu 12 de 12, com a
leitura de verdade pelo PowerShell (`no_windows_a_leitura_de_verdade_chega_ao_fim`,
1,27 s), e o `cargo clippy -p seele-app -- -D warnings` saiu 0.

**Pedido pela orquestradora:** um botão que aparece igual quando resolve e
quando não resolve é o produto dizendo uma coisa que não é verdade. O ABRIR A
PORTA AGORA cria a regra do SEELE **instalado**, em UDP e em qualquer perfil:
resolve a falta de regra e o perfil não coberto, e só quando quem roda é o
instalado, com o instalador ao lado.

| etapa | commit |
|---|---|
| o `NaTela { motivo, frase, abrir_resolve }` no Rust; o estado da sala com ele; a linha da sala com o botão só quando resolve | `e0e48744` |
| a parede da tela de hospedar lê o mesmo motivo (isolado, `tela-boot.js` da F5) | `3cdcafc7` |
| o guarda da parede cobra o botão só quando resolve (isolado, `frontend.rs`) | `bf2624ab` |
| sai o `firewall_nao_cobre` da resposta do `hospedar` | `cb3b4520` |

- **A frase diz o que fazer:** com o botão, o Windows pergunta uma vez; na
  cópia portátil, abrir o SEELE instalado; no bloqueio, onde ele sai (Regras de
  Entrada); no perfil, que a rede ativa não está coberta; sem o instalado,
  instalar.
- **No perfil não coberto o botão também aparece**, quando quem roda é o
  instalado: a regra que ele cria vale em qualquer perfil e soma às outras.
  Fora da lista que a orquestradora escreveu, mas dentro da regra dela.
- **Depois da regra criada**, um reanúncio do estado da sala (feito a cada
  mudança do registro, com a leitura de quando ela subiu) não devolve o botão
  nem a frase de antes, até a sala descer e subir com uma leitura nova.
- **Reversões, cada uma reprovando:** o `abrir_resolve` sempre verdadeiro; o
  botão em todos os motivos na linha da sala e na parede de hospedar; sem a
  marca da regra criada; a parede sem a frase do motivo.
- **Fica para o PC:** o `pasta_instalada` e o ramo do Windows do
  `firewall_deste_executavel` não compilam no Mac.

**Como está a ponta** (`cb3b4520`): app 744/744; os 40 roteiros; as duas
bancadas da ponte; a do celular; e 33 das 34 da janela (a do executor pede o
motor com o wasi-sdk). Com a máquina a 17–26 de carga, o `texto-maior-390` do
celular e a `diagnostico-de-mods` reprovaram por tempo; com e sem esta mudança,
sob carga de 10 a 14, passaram. A F5 revisou e aprovou a parede da tela dela.

## A 65 provada no Windows, os guardas da spec «Testes» 8 e as caixas do plano (15/10)

**A pendência 65, provada no PC de testes** (a orquestradora; o conserto é a
`c8bb813c`, e a pendência está em `4ed24ee0`):

- **vermelho antes** (`95271bc4`): o `sala::a_sala_pessoal::a_porta_ocupada_e_dita`
  deu «left: None, right: Some(PortaOcupada)»;
- **verde depois** (`4ed24ee0`): `alcance::testes` 44 de 44, com os dois novos;
  `sala::` 22 de 22; o clippy `-p seele-server -p seele-app -D warnings` limpo;
- **as reversões lá, uma por família:** sem o `ninguem_na_porta(…)`, o
  `a_porta_ocupada_por_qualquer_ocupante_e_porta_ocupada` reprova no ocupante só
  IPv4 e para ali; tirando só a sonda `[::]`, ele reprova no ocupante só IPv6, e
  o `as_sondas_acham_o_ocupante_que_o_socket_duplo_nao_ve` reprova junto.

A `4ed24ee0` entra na integração 18.

**A tarefa 31, a metade dos guardas** (`fe01d33d`, isolado no `frontend.rs`,
depois de trazer a `versao/0.16` em `52b8278b`, no merge `6b9107df`). Dos cinco
itens da casca na spec, dois já tinham guarda: o `focus()` da faixa (`a53c1349`)
e a `media-src` sem `data:` (`f97dcd4f`). Os três que faltavam leem o Rust do
outro lado, para não casarem com o próprio texto:

- **o toque por evento:** o canal que a faixa ouve é o `CANAL_DO_TOQUE`; cada
  variante do `AvisoDoToque` tem o seu ramo; a faixa não tem `setInterval`; e o
  `toques_agora` é uma pergunta só, na carga, que nenhum outro script faz;
- **o apelido como alegação:** o `ligando` do `frases.js` põe todo apelido entre
  aspas e mostra o código, e o script da faixa não lê `.apelido` por conta própria;
- **«atender sai de Y»:** vale para toda variante do `OndeEstou` menos `Nenhum` e
  `Pessoal`, e uma variante nova reprova até alguém decidir de que lado ela fica.
  O roteiro `toque.js` prova só o caminho do `Guardado`, e a spec fala do `Outro`.

As reversões, dez, uma por vez, reprovam cada uma pela sua frase:

- o canal renomeado, um relógio de 5 s e o `Parou` sem ramo;
- o apelido sem aspas, o apelido solto no anúncio e a frase sem o código;
- o «sai de» só no `Outro`, ou também no `Pessoal`, ou sem perguntar ao
  `onde_estou`, ou com uma variante nova no Rust.

No `frontend.rs`, 299 de 299, e o clippy limpo. Como a mudança não toca a `ui/`
nem o duble, a bateria da casca não se aplica.

**As caixas do plano.** Marquei as da 23, 24, 25, 26 e 31, cada uma com o guarda
e o commit. A 25 está feita desde a `fad0662a`: o `AlarmeDaJanela` toca na saída
escolhida e para com o toque.

**A D-sala-3** (15/10, pela recomendação da F7) já está no código desde a
`e0e48744`. No `SoNoPerfil`, o botão aparece quando quem roda é o instalado, e o
`firewall::o_botao_so_aparece_quando_resolve` prova isso.

**Uma reprovação velha do celular, que não se repete.** A notificação de uma
rodada das 11:27 chegou depois. O caso era o `entrada-desfechos-390`, com
«Cannot access 'pedidoDeInstalar' before initialization» e
«PONTE_NO_LUGAR_DO_TAURI is not defined». A pasta guardada mostra a ponta
`a416f17f` com a árvore no meio da faixa do toque (`camada-toque.css`,
`camada-toque.js` e `roteiros/toque.js` modificados). Não achei a causa. O que a
medida diz:

- as três rodadas inteiras depois dela passaram;
- na ponta `4ed24ee0`, três rodadas só do `entrada-desfechos` deram 6 de 6.

**O teste que faltava da 20** (`511aa0bc`): um MOD do guardado continua
respondendo enquanto a dona visita outro servidor. O teste passa pelo `connect`
e pelo `disconnect` de verdade, no runtime falso do Tauri, com o MOD de
referência:

- com a dona dentro, ele responde 1;
- com ela noutro servidor (`Outro`), ele responde 2, pelo servidor que os
  comandos de MOD do app acham.

Três reversões reprovam, cada uma no ponto dela: o `disconnect` voltando a
derrubar o guardado; os comandos de MOD seguindo a sessão; e a saída desligando
o MOD.

Para chegar lá:

- o `disconnect` e o `desmontar_o_cliente` ficaram genéricos no runtime, como o
  `connect`;
- o guarda da revogação no `frontend.rs` passou a aceitar a assinatura genérica
  (`da9580eb`, isolado);
- o guarda do ponto de encontro do `main.rs` (lote G-servidor da F5) passou a
  ler só a produção (`845e20e3`). Ele reprovava o `Hospedagem::iniciar` curto do
  teste, que o próprio doc dele chama de padrão seguro dos testes. Três
  reversões de produção ainda reprovam.

App 748/748 e o clippy limpo em todos os alvos. Como a mudança não toca a `ui/`
nem o duble, a bateria da casca não se aplica.

## A P8 no ramo, e a tarefa 16: o `Tocando` no cliente (15/10)

**A P8 no ramo.** A orquestradora liberou a 16 com a P8 fechada. Trouxe a
`0410a462` (`20b6c281`) e, a pedido dela, a ponta `05e417df`, com o `OutraLinha`
da F1 (`f9da6c11`), as duas com `--no-ff` e sem conflito. Na árvore integrada
passaram o seele-core, o seele-ffi, o seele-sessao, o seele-app, o `outra_linha`
da conformidade e o clippy. Na bateria inteira da casca passaram: a carga, os
42 roteiros, as duas pontes, a bancada do celular (o `entrada-desfechos`
inclusive) e 33 das 34 bancadas da janela.

**Um achado da P8, levado à orquestradora:** o
`dois_produtores_de_snapshot::cada_campo_do_snapshot_tem_um_lado` reprova na
própria `05e417df`. O `fora_do_ar_revision` entrou no `Snapshot` com a
`eae03135` (a P6 da F1), e o guarda não o classifica. O conserto é da F1.

| passo | o que | commit |
|---|---|---|
| 1 | o aperto de mão em dois prazos: 10 s até o `Tocando`, e o prazo do toque mais 5 s depois dele | `721fbe07` |
| 2 | quem toca vence a corrida: ninguém mais começa, quem corria cai, e o prazo do candidato se solta, sem segundo toque | `df17b0d9` |
| — | a linha provisória do `Tocando` no `ETAPAS` (isolado, `frases.js`) | `75c7ee70` |
| 3 | a `Etapa::Tocando` do núcleo à FFI, e o `caminho()` que lê quem tocou | `c0eda87a` |

- **O que o cliente fazia:** o `Tocando` virava `ProtocolViolation` na hora,
  porque o aperto de mão lia a resposta do `Response` esperando `Session`. A
  recusa e o «ninguém atendeu» nem chegavam a ser lidos.
- **A corrida e o segundo toque:** a sala anuncia a mesma pessoa por outro
  candidato como outro `Tocou`, e fecha calado o toque de antes. O segundo
  candidato saía aos 250 ms e chegava lá. E o prazo de cada candidato (4 s)
  cortava a ligação que estava chamando.
- **O `caminho()`:** ele lia a última `Tentando` antes de `Dentro` como a
  vencedora; com o toque, ela pode ser de quem foi largado.
- **Os testes:** `seele-conformance/tests/tocando.rs`, contra a sala de
  verdade (seis), e os da corrida sem socket (quatro), da chegada (dois) e da
  FFI (um). As reversões, dezessete, reprovam cada uma pela sua frase.

**Uma lição de verificação, cara de aprender:** a conformidade inteira mede o
que está no disco enquanto roda, e não só o que compilou no começo.
- Um `cargo test --test tocando` rodado no meio dela recompila o binário do
  `tocando` no mesmo `target/`, e a rodada roda o novo.
- O `estados.rs` lê o `chegada.rs` do disco: com o passo seguinte editado, ele
  comparou o fonte novo com o binário velho e reprovou.
Durante a conformidade inteira, nenhum `cargo test` dela e nenhuma edição de
fonte.

**Como está a ponta** (`c0eda87a`):
- seele-core e FFI, 689 de 689; app, 752 de 752; clippy limpo;
- a conformidade inteira: 310 passaram, e só reprova o guarda da P8;
- a bateria da casca: a carga, os 42 roteiros, as duas pontes, o celular e
  33 das 34 da janela.

**Falta da 16 o `cancelar_ligacao`.** Ele espera o sim da orquestradora: toca
o `ConnectionError` da FFI e o `frases.js`. O desenho proposto é uma porta nova
na FFI com um `Cancelamento`, sem mexer no `ConnectConfig`, e uma
`Etapa::Cancelada` terminal. Um `Desistiu(Cancelada)` afrouxaria duas arestas
que hoje são requisito escrito.
