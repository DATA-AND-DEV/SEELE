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
| 6 | os ADRs 0059, 0060 e 0061 | **feita**, com o 0061 proposto | `d763e9e9` |

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
