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
| — | plano | escrito, mandado à orquestradora | — |
| 1 | 4e, a janela do Comandante | a fazer | — |
| 2 | 4a, o certificado Ed25519 | a fazer | — |
| 3 | 4c, a campainha | a fazer | — |
| 4 | 7g-núcleo, a agenda | a fazer | — |
| 5 | 7c-áudio, o toque só de saída | a fazer | — |
| 6 | os ADRs 0059, 0060 e 0061 | a fazer | — |
