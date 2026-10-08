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

## Decisões para o dono (pela orquestradora)

- **D-enc-1** — o limite e o teto. Proposta no plano, com a conta: balde de 64
  que enche a 8/s por `/32` e `/64`, só no ENC/2 (60 SEELE abertos atrás de um
  IP passam para sempre); teto de 16 384 marcas no quarto assinado, separado das
  4096 do ENC/1; quem passa ouve `RECUSA taxa` uma vez por segundo, e o produto
  diz.
- **D-enc-2** — o `publicar.sh` rodar `sondar --exige-enc2` antes de publicar.
  A receita sai de qualquer jeito (Task 13).

## Tarefas

| # | tarefa | estado | commit |
|---|---|---|---|
| 1 | o fio do ENC/2 no `seele-proto` | a fazer | |
| 2 | a mensagem que a chave assina no `MORO2` | a fazer | |
| 3 | o `PONTO_PADRAO` no `seele-proto` | a fazer | |
| 4 | o quarto assinado | a fazer | |
| 5 | o limite por prefixo | a fazer | |
| 6 | o ponto fala as duas versões | a fazer | |
| 7 | o `sondar` aprende o `/2` | a fazer | |
| 8 | o anfitrião assina e diz | a fazer | |
| 9 | quem liga pergunta pelo `QUEM2` | a fazer | |
| 10 | de ponta a ponta | a fazer | |
| 11 | o ENC/2 no fuzz | a fazer | |
| 12 | o ADR 0062 e as frases públicas | a fazer | |
| 13 | o portão da VPS | a fazer | |
