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
| 3 | o `PONTO_PADRAO` no `seele-proto` | **espera o G-servidor** | |
| 4 | o quarto assinado | feita | `830dd365` |
| 5 | o limite por prefixo | feita | `0d273738` |
| 6 | o ponto fala as duas versões | feita | `50488d25` |
| 7 | o `sondar` aprende o `/2` | feita | `f3e1bff8` |
| 8 | o anfitrião assina e diz | **espera o G-servidor** | |
| 9 | quem liga pergunta pelo `QUEM2` | feita | `7871d563` |
| 10 | de ponta a ponta | espera a 8 | |
| 11 | o ENC/2 no fuzz | feita | `ff203557` |
| 12 | o ADR 0062 e as frases públicas | feita | `3c12e223` |
| 13 | o portão da VPS | feita | `e9b31450` (o `publicar.sh`, à parte; a F6 simulou o merge com o `36274663` e ele entra limpo), `8f006d8c` (a receita) |

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
