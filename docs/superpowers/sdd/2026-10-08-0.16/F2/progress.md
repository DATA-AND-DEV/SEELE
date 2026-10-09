# F2 — o SEELE-ENC/2, o ponto de encontro assinado (Plano 4) — andamento

**Branch:** `0.16/enc2`, criado de `versao/0.16` em `de1aee67` (um commit de docs
depois do `dc001a0a`: só `integracao.md`).

**Plano:** `docs/superpowers/plans/2026-10-08-0.16-F2-enc2.md`, com 13 tarefas.

## Conferido antes de escrever (08/10)

- **O publicado:** `v0.15.1 · 54f542f94` é o último release de SEELE-RELEASES.
  Nenhuma versão publicada fala o ENC/2, nem o ponto em produção.
- **O balde do ponto lê 96 bytes** (`crates/seele-encontro/src/lib.rs:284`): num
  Linux um datagrama maior chega truncado sem erro e é analisado. O balde de
  `TAMANHO2 + 1` da Task 6 fecha isso.
- **Medido:** o `SocketAddr` aceita escopo (`[fe80::1%3]:80`) e o `u64` aceita
  `+1` e `007`. O fio exige a forma canônica de cada campo.
- **Medido:** `base64` 0.22.1 `URL_SAFE_NO_PAD` recusa `=` e bits sobrando;
  `ed25519-dalek` 3.0.0 tem `verify_strict`.
- **O nightly e o `cargo-fuzz` 0.13.2 estão instalados.**

## Decisões do dono (08/10, pela orquestradora)

- **D-enc-1, decidida como proposta:** um balde de 64 que enche a 8/s, por `/32`
  e `/64`, só no ENC/2; o teto do quarto assinado é de 16 384 marcas, separado
  das 4096 do ENC/1. Quem passa ouve `RECUSA taxa` no máximo uma vez por segundo
  por prefixo, e o log diz que o limite é por endereço público. O limite por
  `/48` fica como segundo degrau.
- **D-enc-2, sim:** o `conferir_ponto_de_encontro` entra no `publicar.sh` num
  commit à parte, perto do `conferir_condicoes_das_notas` da F6 (avisar a F6). Uma
  opção de pular, se existir, deixa dito no corpo do release que o ponto não foi
  conferido, como o `--sem-bateria`.
- As duas vão para o ADR 0062.

## A ordem mudou (aviso da orquestradora)

A F5, no lote G-servidor, muda a `Convocacao::do_ambiente`: sem
`$SEELE_ENCONTRO`, nenhum ponto. **A Task 3 (o `PONTO_PADRAO` no `seele-proto`)
e a Task 8 (o assinante na `Convocacao`) esperam o G-servidor em
`versao/0.16`**, e são feitas em cima dele. Conferido: o `0.16/lotes-g-servidor`
não toca no `seele-core/src/encontro.rs`, então a Task 9 não espera.

## Medidas e achados da execução

- **O macOS também trunca sem erro.** Com o balde do ponto do tamanho exato, um
  `ONDE` com um byte a mais (97) era respondido: o `recv_from` devolve 96 e o
  pedaço forma um pedido. O balde de `TAMANHO2 + 1` fecha isso nas duas versões
  (Task 6), e `um_datagrama_maior_que_o_do_protocolo_e_calado` prende.
- **O `verify_strict` ganhou teste.** O plano dizia que nenhum teste o separava
  do `verify`. A chave identidade (ordem pequena) com `R = identidade, s = 0`
  confere com qualquer mensagem no `verify`, e
  `uma_chave_fraca_nao_registra_com_a_assinatura_que_vale_para_tudo` reprova na
  troca (Task 4).
- **Um defeito no balde do plano.** Contar o enchimento a partir de agora, em
  milissegundos inteiros, nunca enchia com pedidos mais próximos que 1 ms. O balde
  avança só o que contou (Task 5), e um teste prende.

## Tarefas

| # | tarefa | estado | commit |
|---|---|---|---|
| 1 | o fio do ENC/2 no `seele-proto` | feita | `d7d8a92b` |
| 2 | a mensagem que a chave assina no `MORO2` | feita | `c7b2048e` |
| 3 | o `PONTO_PADRAO` no `seele-proto` | feita, em cima do G-servidor | `26e0cd80` |
| 4 | o quarto assinado | feita | `830dd365` |
| 5 | o limite por prefixo | feita | `0d273738` |
| 6 | o ponto fala as duas versões | feita | `50488d25` |
| 7 | o `sondar` aprende o `/2` | feita | `f3e1bff8` |
| 8 | o anfitrião assina e diz | feita, em cima do G-servidor | `0c6ff5f0` |
| 9 | quem liga pergunta pelo `QUEM2` | feita | `7871d563` |
| 10 | de ponta a ponta | feita | `057854d8` (e o guarda do `quarto.rs`, `76107d38`) |
| 11 | o ENC/2 no fuzz | feita | `ff203557` |
| 12 | o ADR 0062 e as frases públicas | feita | `3c12e223` |
| 13 | o portão da VPS | feita | `e9b31450` (o `publicar.sh`, à parte; a F6 simulou o merge com o `36274663` e ele entra limpo), `8f006d8c` (a receita) |
| 14–16 | o cookie (D-enc-4) e o teto por prefixo (D-enc-3) | feitas | `86caf16e`, `47233d9a` |
| 17–18 | o `sondar` com o `DESAFIO`, o fuzz, os docs e o ADR | feitas | `86caf16e`, `ba8b7b85` |
| — | o cookie pelo HMAC do RustCrypto (pedido da orquestradora) | feita | `04181a99` |

## A revisão independente (08/10) e os consertos

Um revisor sem o histórico desta sessão leu `6e3d8a23..3c12e223`, com três
medidas suas, e deu «com consertos». Os cinco itens importantes eram reais
(conferidos no código antes de mexer):

| item | conserto | commit |
|---|---|---|
| a conta do D-enc-1 errada em 5–10×: um registro só lembrado (carimbo até +300 s) segurava vaga por ~600 s | o teto conta os vivos; os só lembrados saem antes do «cheio»; Mutex envenenado por `into_inner` | `e5b58616` |
| a taxa como alavanca: `QUEM2` de um vizinho, ou forjado com o IP da vítima, calava os `MORO2` da rede | um balde por classe (registro e pergunta); a fiação da taxa ganha teste pelo socket | `8b30365a` |
| quem é limitado pela taxa ouvia «ponto anterior ao ENC/2» | o cliente pergunta de novo enquanto só a sonda respondeu; o caso calado vira `Enc2Calado`; teste com o limite de verdade | `2e808964` |
| frases públicas prometendo mais que o código | reescritas, com «o que sobra na sala» | `cf1d6964` |
| o corpo do release dizia «produção» com outro ponto conferido | a linha diz qual ponto; fora do padrão, em negrito | `d1e78879` |

Os menores também: o rastro deixa de gravar o código de quem se procura
(`2e808964`); o `sondar` repete os pedidos e o portão reprova relógio de ponto a
mais de 60 s (`87479823`); um erro de um datagrama só (`WSAEMSGSIZE`, reset por
ICMP) não mata o `servir` (`07b6b702`, o classificador provado aqui; o `servir`
num Windows de verdade não foi medido); a folga do datagrama não promete mais
campo novo sem versão nova (`cf1d6964`).

**Decididas pelo dono (08/10), e feitas:**
- **D-enc-4, o cookie de ida e volta** (`86caf16e`): o `MORO2` leva o cookie no
  último campo, o ponto responde `DESAFIO` sem guardar nada e sem gastar o
  balde, e o `MOROU` leva o cookie corrente. HMAC-SHA256 sobre o `sha2` (os
  vetores da RFC 4231), janela de 120 s, preso ao IP; o segredo do `getrandom`,
  só em memória. Cinco reversões, uma delas medida pelas duas famílias do
  loopback (o cookie do `::1` não vale no `127.0.0.1`). O fuzz: 4,4 milhões de
  execuções em 31 s, sem falha.
- **D-enc-3, 128 marcas vivas por prefixo** (`47233d9a`).
- Os docs e o ADR (`ba8b7b85`): o segredo do ponto, e «o que sobra» sem a origem
  forjada calando uma sala.

**O fio do ENC/2 está completo** para a integração: `MORO2` (com o cookie),
`QUEM2`, `MOROU`, `DESAFIO`, `AQUI2`, `NINGUEM` e `RECUSA`.

**Os docs falam no presente do que a Task 8 faz** (o anfitrião dizendo o
relógio torto e a recusa no `seele.log`). A Task 8 tem de entrar na mesma 0.16.

## A Task 3 depois do G-servidor

O G-servidor (`8a4bd619`, no `0.16/lotes-g-servidor`) dá dois sentidos à
ausência de `$SEELE_ENCONTRO`: no `do_ambiente`, nenhum ponto; no
`com_o_ponto_padrao` (o HOSPEDAR do app), o `PONTO_PADRAO`. Então a função pura
da Task 3 passa a ser `ponto_escolhido(variavel, sem_a_variavel)`, o
`do_ambiente_ou` do G-servidor passa a chamá-la, e o `ponto_de_quem_liga` do
cliente é o caminho de produção (sem a variável, o padrão). Proposta, a levar à
orquestradora: o guarda que o G-servidor pôs no `resolver` do anfitrião
(nenhum teste fala com a produção) ganha um gêmeo no `resolver_ponto` do
cliente, quando o `PONTO_PADRAO` chegar ao `seele-proto`; hoje um teste que
chamasse `localizar(ponto_de_quem_liga()…)` sem ambiente perguntaria à VPS.

**Marco: o lado do ponto está pronto** (Tasks 1, 2, 4, 5, 6 e 7). Depois do merge,
o binário do `seele-encontro` deste branch é o que vai para a VPS, e o
`sondar --exige-enc2` confere de fora.

## As Tasks 3, 8 e 10 (08/10, depois do merge do G-servidor)

O `versao/0.16` em `a2b9f9d0` entrou no `0.16/enc2` pelo `14ab08ba`.

- **Task 3** (`26e0cd80`): o `PONTO_PADRAO`, o `REDE_DO_PADRAO`, o `VARIAVEL`, o
  `ponto_escolhido(variavel, sem_a_variavel)` e o `aponta_para_a_producao` vão
  para o `seele-proto::encontro`; o `do_ambiente_ou` do G-servidor e o guarda do
  anfitrião passam a chamá-los, sem mudar de sentido. Quem liga ganha o
  `ponto_de_quem_liga()` e o gêmeo do guarda: o `resolver_ponto` e o `resolver`
  da batida recusam a VPS de produção antes do DNS, pelo `cfg(test)` e pela
  feature `recusar-encontro-de-producao` do `seele-core`, que só a
  `[dev-dependencies]` do `seele-conformance` pede (e um `compile_error!` a
  recusa num build otimizado).
- **Task 8** (`0c6ff5f0`): `Convocacao::para_sala(socket, escuta, chave,
  ponto)` assina o `MORO2` da escuta e o do servidor a cada reavivamento, com o
  cookie do `DESAFIO`/`MOROU` e as dicas das interfaces físicas de rede local.
  O `EstadoDoRegistro` (`Esperando`, `Registrado`, `RelogioTorto`, `Recusado`,
  `CookieRecusado`, `SemResposta`) sai por um `watch`, lido por
  `Hospedagem::registro_no_quarto()`, e cada mudança vira uma linha no
  `seele.log`. Um servidor guardado (`do_ambiente`) continua só no ENC/1.
- **Task 10** (`057854d8`): um ponto de verdade no `127.0.0.1`, uma sala que
  chega a `Registrado` passando pelo `DESAFIO`, e quem liga achando pelo
  `localizar` o servidor, a escuta e as dicas. Ao lado: a sala sem registro é
  `NinguemMora`, e o servidor guardado é achado pelo ENC/1 no mesmo ponto. Três
  reversões (o ponto calando o `MORO2`, a sala mandando o `MORO` da versão 1, a
  sala esquecendo o cookie) reprovam.

**Um guarda que eu quebrei e não vi.** A refatoração da Task 9 (`7871d563`) levou
o `timeout_at(ate, resolver_ponto(ponto))` do `onde_mora_hoje` para o
`consultar_o_ponto`, e o guarda de fonte
`quarto.rs::resolver_o_nome_acontece_dentro_do_prazo_e_nao_antes_dele` passou a
reprovar. Ninguém viu porque a conformidade estava bloqueada pela bateria da
integração. Conserto em `76107d38`: o guarda segue a consulta até o
`consultar_o_ponto` e confere que o `onde_mora_hoje` e o `localizar` passam por
ele. Provado por duas reversões.

**Rodado, um arquivo de cada vez:** `enc2`, `quarto`, `quarto_pelo_link`,
`ocupante_da_escuta`, `furo`, `volta_pela_trilha`, `estados`, `ejetar` e
`tela_por_um_par` do `seele-conformance`, e os testes de unidade do encontro em
`seele-encontro`, `seele-proto`, `seele-core` e `seele-server`. `cargo fmt` e
`clippy -D warnings --all-targets` limpos nos cinco crates. **Não rodado:** a
bateria inteira, que é da integração.

**Marco: a F2 está completa** para a próxima integração. O `0.16/enc2` entra
limpo no `versao/0.16` de hoje (`4ef902ed`, conferido pelo `git merge-tree`).
**O que fica com o dono:** subir o binário do `seele-encontro` deste branch na
VPS. Até isso, o `publicar.sh` reprova a 0.16 no portão, como deve.
