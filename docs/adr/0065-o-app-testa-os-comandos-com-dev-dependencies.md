# 0065 — O app testa os comandos de verdade: dev-dependencies nominais, produção intacta

Status: **aceito** · decisão do coordenador da 0.17
Data: 2026-10-25

**Emenda** o [0002](0002-regra-de-dependencia.md) (a tabela de `xtask/src/check_deps.rs`, que ganha uma exceção nominal só de dev-dependency).

## Contexto

A casca desktop (`seele-app`) vê, em produção, só a FFI, o servidor embutido, o lançador e o motor de MODs (`seele-ffi`, `seele-server`, `seele-lancador`, `seele-motor`). Os testes que precisam chamar os comandos de verdade (`varias_sessoes_testes`, `ligar` e semelhantes) só existem dentro do crate binário, em `src/`: um comando Tauri não é alcançável de um teste de integração em `tests/`. Para montar o mundo em que o comando roda, esses testes precisam construir o que a FFI esconde: o formato do fio, o dispositivo de áudio de mentira, o cliente headless e o ponto de encontro. O `cargo xtask check-deps` reprovou essas quatro arestas.

## Decisão

Uma exceção NOMINAL em `DEV_EXCEPTIONS`, válida só para `DependencyKind::Development` e só para estes crates no `seele-app`: `seele-audio`, `seele-core`, `seele-proto` e `seele-encontro`. Além disso, `seele-video` entra na lista do `seele-conformance`, que já é a exceção documentada dos testes de ponta a ponta com tudo como dev-dependency: o helper `tela_e_voz_sinteticas` gera H.264 válido pelo codificador do produto.

## Por que isto não fura a regra

- Dev-dependencies não entram no produto: com o resolver 2 não unificam features nem compilam no binário de produção. A casca de produção continua vendo só a FFI.
- A lista de produção do `seele-app` NÃO muda. A dependência normal e a de build do app em cada um dos quatro continuam reprovadas, a dev em qualquer outro crate também, e a exceção não vaza para outro pacote (testes em `check_deps.rs`).
- Fica fora desta decisão a dependência normal OPCIONAL `seele-audio` (recurso `aparelho-de-mentira`): ela sai do app por outro caminho e não ganha exceção.

## Alternativas

- Mover os testes para `seele-conformance`: rejeitado, os comandos só existem no crate binário.
- Reexportar os quatro pela FFI: rejeitado, poria o formato do fio na superfície que a casca de produção enxerga.

## Consequências

Quem quiser outra dev-dependency no app argumenta aqui, na tabela. A guarda de cada restrição foi provada tirando-a (o teste falha).
