# F6 — o launcher por linha de servidor — andamento

**Branch:** `0.16/launcher`, a partir de `versao/0.16`.
- O pedido dizia «em `c0337b2`», mas o `versao/0.16` estava em `d4eb6f1`.
- A diferença é um commit de documentação: o achado do ESTILO no spec-mestre.

**Plano:** `docs/superpowers/plans/2026-10-08-0.16-F6-launcher.md`, com 17
tarefas. As 11 do levantamento foram partidas assim:
- a tarefa 1 do levantamento virou as tarefas 1 a 5;
- a 5 virou as 9 e 10.

A 0.15.2 é a tarefa 8, no branch `linha/0.15`.

**Linha de base, medida em 08/10:** `cargo test -p seele-lancador` dá 91
testes de unidade e 2 de integração, verdes.

## Medido antes de desenhar

- **O Tauri 2.11.6 cria as janelas do `tauri.conf.json` antes do `setup` do
  app** (`tauri-2.11.6/src/app.rs:2521-2531`). Por isso, na primeira abertura
  (tarefa 12):
  - a janela passa a `create: false`;
  - a janela é criada no `setup`, **depois** de copiar os dados da 0.15.
- **O `RemoteRelease` do `tauri-plugin-updater` 2.10.1 é público** e exige
  `url` e `signature` em todo alvo do topo (`updater.rs:1383-1430`). Ele serve
  ao teste do manifesto (tarefa 6).
- **O esquema do banco da 0.15.1 é a versão 13.** Lido com
  `git show 54f542f:crates/seele-server/src/persistence/schema.rs`.

## Decisões técnicas deste plano

Para a orquestradora conferir:

1. **O texto canônico da linha.**
   - É `0.N` para antes das duas numerações e `M` para M ≥ 1: os Servidores
     1.0 e 1.3 são da linha `1`.
   - A pasta de dados da linha 1 é `dados/servidor-1`.
   - Na 0.16 só a `servidor-0.15` é usada de fato: a linha 1 é o próprio app,
     na pasta dele.
2. **A ordem de `versions[]`:** da linha mais nova para a mais velha e, dentro
   da linha, por publicação.
   - Um conserto da 0.15 publicado depois da 0.16 não toma o topo.
   - O topo do Tauri continua sendo o da linha mais nova.
3. **O sinal de «aberta pelo launcher»:** `SEELE_LANCADOR=1`.
   - A 0.15.2 o lê, e a 0.16 também: uma 2.0 abrirá a 1.x pelo mesmo caminho.
4. **A linha legada só é baixada na primeira abertura onde houve 0.15**, e uma
   vez só.
   - Quem nunca teve 0.15 a baixa quando precisar (ao entrar ou hospedar).
   - Assim não se fala com a rede a cada arranque.
5. **O `manifesto.py` lê a unidade do código** (`PROTOCOL_VERSION`,
   `MOD_API_VERSION`, `APIS_ACEITAS` e `VERSAO_DO_SERVIDOR`).
   - Um guarda confere essa leitura contra as constantes compiladas.
   - Uma 0.16 sem `VERSAO_DO_SERVIDOR` não sai.
6. **O caminho do instalador:** o pedido fala em `crates/seele-instalador`, e o
   crate está em `apps/seele-instalador`.
   - O pacote portátil não precisa dele: é a mesma carga do instalador, num
     `tar.gz`.

## Pedidos à F1 (pela orquestradora)

- (a) `pub const VERSAO_DO_SERVIDOR: &str = "1.0";` em
  `crates/seele-proto/src/version.rs`, nesta forma literal.
- (b) `seele_proto::version::linha_do_protocolo(u8) -> Option<&'static str>`,
  com `8 → "0.15"` e `9.. → "1"`.
- (c) O `v=` do link passando a ser a versão do servidor (item 22).
- (d) A recusa de outra linha chegando ao app como erro tipado, com o byte ou a
  linha do par (item 23).
- (e) Se não existir: `pub use seele_proto::version::PROTOCOL_VERSION;` em
  `seele-ffi`, para o guarda da unidade do manifesto.

## Arquivos de outras frentes que a F6 toca (ganchos mínimos)

| arquivo | gancho | tarefa |
|---|---|---|
| `main.rs` | comandos finos da F6; `setup` (a primeira abertura e a janela); revogação no começo de `hospedar` e de `connect`; `Abertura.pelo_lancador` | 9 a 15 |
| `frases.js` | a frase da versão revogada, num commit próprio | 10 |
| `tela-boot.js` | o seletor de hospedar por linha | 14 |
| `camada-servidores.js` | uma chamada | 13 |
| `tela-server.js` | esconder a atualização quando aberta pelo launcher | 11 |
| `index.html` | `#boot-linha-legada`, `#versoes-confirmar` e as notas | 12 a 15 |
| `tauri.conf.json` | `create: false`; sai o segundo endereço | 12, 16 |

## Tarefas

**O `seele-lancador` depois das tarefas 1 a 5:** 113 testes de unidade e 2 de
integração, verdes, e o clippy limpo.

**Merge da F1:** o `aa8574e` (`0.16/protocolo`) entrou em `96abc57`, com
`VERSAO_DO_SERVIDOR`, `linha_do_protocolo` e o reexport no `seele-ffi` — os
pedidos (a), (b) e (e). Ali o `PROTOCOL_VERSION` ainda é 8; o 9 vem no commit
seguinte da F1. Os pedidos (c) e (d) ficam para depois: o número de quem recusa
já chega em `ControlError::UnsupportedVersion { found }`.

**Um tropeço registrado.** O macOS não tem `timeout`, e as duas primeiras
tentativas de reversão da tarefa 4 falharam no comando, sem rodar teste
nenhum. Foram refeitas com `perl -e 'alarm …'` e aí provaram.

| # | tarefa | estado | commits |
|---|---|---|---|
| — | plano | feito | |
| 1 | a linha e a unidade no manifesto | feito, com reversão (a regra legada e a ordem das linhas) | `e2ead14` |
| 2 | `Pedido::MaisNovaDaLinha`; dados da linha | feito, com reversão; os 7 testes da casca (`o_launcher_ligado_ao_produto`), que o levantamento dava como nunca rodados, rodam e passam | `3a1b7f6` |
| 3 | marcador com linha e soma; poda | feito, com reversão (`skip(1)` e a recusa sem linha) | `5e8933c` |
| 4 | dados por linha; `semear_linha` | feito, com reversão (a recusa do destino dentro da origem e o «deixar de fora» da cópia) | `3ac83dc` |
| 5 | tamanho e remoção | feito, com reversão (`symlink_metadata`) | `62b7c5b` |
| 6 | `manifesto.py` híbrido e acumulado | — | |
| 7 | pacote portátil do Windows | — | |
| 8 | a 0.15.2 (`linha/0.15`) | — | |
| 9 | a casca pelo `Resolvedor` | — | |
| 10 | revogação nos atos do próprio processo | — | |
| 11 | a guarda da linha legada | — | |
| 12 | a primeira abertura | — | |
| 13 | entrar por linha | — | (a recusa espera a F1) |
| 14 | hospedar por linha | — | |
| 15 | tela de disco mínima | — | |
| 16 | documentos; sai o segundo endereço | — | |
| 17 | medida de ponta a ponta (o Windows via orquestradora) | — | |
