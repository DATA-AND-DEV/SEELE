# Backport 0.16.x — confirmar antes de atualizar para outra versão do servidor, e escolher as saídas dos MODs antigos sem converter o banco antes da hora

Patch: [`0.16.x-confirmar-travessia.patch`](0.16.x-confirmar-travessia.patch).
Evidências: [`evidencias/`](evidencias/).

## O que mudou nesta revisão (25/10)

A primeira versão deste patch (24/10) cobria só o atualizador (§3). Esta
**substitui** aquele arquivo, com o mesmo nome, e acrescenta o conserto do
§2 de `revisao-astra-travessia-api5-6.md`: na 0.16.0, escolher ATUALIZAR ou
DESLIGAR para um MOD de API antiga **já convertia e gravava o banco** no
clique, MOD a MOD, sem cópia do estado de antes e sem desfazer. Agora:

- as escolhas ficam **estagiadas na tela**, com DESFAZER e CANCELAR, e nada
  vai ao Rust até o botão final;
- o botão final («FAZER A CÓPIA DE SEGURANÇA, APLICAR E HOSPEDAR») só vale
  com uma escolha para cada MOD; ele baixa as versões novas (uso «obter»,
  que não mexe em servidor), e um comando só,
  `aplicar_saidas_dos_mods_antigos`, faz **a cópia de segurança verificada,
  depois a conversão e as escolhas**;
- **cancelar não altera o banco**: medido byte a byte com a fixture da 0.15.1;
- a API aceita continua a **6** no Servidor 1.0: nada aqui faz a linha 1
  rodar MOD da API 5, e nada da API 7 da 0.17 foi portado.

O detalhe está em §3b e os testes em §4.

**Nada disto foi publicado.** Não há tag, release, rascunho nem mudança no
SEELE-RELEASES. É um patch revisável para quem decidir cortar uma 0.16.x.

## 1. A base

- Commit: `8faa3379b` («publicar: o corpo do release é medido antes de ir ao
  GitHub…»).
- Conferido pela API do GitHub em 2026-10-24 (o comando do `CLAUDE.md`): o
  corpo da `v0.16.0`, publicada em 2026-10-24T18:16:47Z, diz `commit 8faa3379b`.
- O `latest.json` servido hoje por `releases/latest/download/latest.json`
  (baixado em 2026-10-24) tem topo `0.16.0`, `schema: 2` e `versions[]` com
  `0.16.0 → unit.server "1.0"` e `0.15.2 → unit.server "0.15"`. É desse campo
  que o patch lê a versão do servidor da oferta.

## 2. Como aplicar

Num ramo próprio, cortado de `8faa3379b` (e **não** do `main`, que já é
Servidor 2.0):

```sh
cp docs/backports/0.16.x-confirmar-travessia.patch /tmp/   # o arquivo some com o switch
git switch -c release/0.16.x 8faa3379b
git apply --check /tmp/0.16.x-confirmar-travessia.patch
git apply /tmp/0.16.x-confirmar-travessia.patch
```

O `.patch` é um `git diff --binary` contra a árvore de `8faa3379b` (blob git
do arquivo, revisão de 25/10 com o Perfis:
`00c4b1602d1d31a166c810089401f82899409122`; antes dela, `e214795d…` (sem o
Perfis) e `32738b27…` (24/10, só o atualizador), ambos substituídos); aplica
com `git apply`. Não é `format-patch`: não há commit dentro, e o commit fica
para quem revisar. São 26 arquivos, +3127/−152, todos em `apps/seele-app/`; os
novos (`src/atualizador.rs`, `src/copia_do_banco.rs`,
`src/perfis_api5_testes.rs`, `bancada/travessia-de-versao.cjs` e os 8 da
fixture `testes/pasta-da-0.15.1-perfis/`) entram pelo próprio patch.

**Versão.** O patch não muda número de versão nenhum: o workspace é `0.0.0` de
propósito, e quem carimba o número é o empacotamento a partir da tag
(`empacotar/macos.sh`, `linux.sh`, `windows.ps1`). A 0.16.x sai com a tag
(por exemplo `v0.16.1`). Notas opcionais em `empacotar/notas/0.16.1.md`.

## 3. O que muda: o atualizador

Arquivos tocados, todos em `apps/seele-app/`:

| arquivo | mudança |
|---|---|
| `src/atualizador.rs` (novo) | a decisão: `Fronteira { Mesma, Cruza{de,para}, NaoSabe }` pela `versions[].unit.server` do `latest.json` contra a linha deste app (`versoes::linha_deste_app()`, «1»); `pode_instalar`; adiar/esquecer o adiamento (arquivo `atualizacao-adiada` ao lado das preferências). Mesmo desenho do `apps/seele-app/src/atualizador.rs` da 0.17, com os testes refeitos para 0.16→0.17. |
| `src/main.rs` | `mod atualizador;`; `VersaoNova` ganha `fronteira`, `pede_aviso`, `adiada`; `FalhaAoAtualizar::TravessiaNaoConfirmada`; `instalar_atualizacao(confirmada: Option<String>)` recusa a travessia não confirmada **antes do `download`** e sem consumir a oferta; comando novo `escolher_atualizacao(escolha)` para cancelar ou adiar; registro no `generate_handler!`. |
| `ui/index.html` | a camada `#travessia` (`role="alertdialog"`, `aria-modal`, `aria-labelledby`, `aria-describedby`, três botões) e a frase `#atualizacao-travessia` no painel, antes do botão. |
| `ui/tela-server.js` | INSTALAR passa por `aoApertarInstalar`: com `pede_aviso` abre a confirmação; sem, instala como antes. Cancelar/adiar chamam `escolher_atualizacao`; prosseguir chama `instalar_atualizacao` com a versão mostrada. Foco inicial em CANCELAR, Tab preso nas três saídas, Escape e clique fora cancelam, o foco volta a quem abriu. |
| `ui/frases.js` | `frasesDaTravessia(nova)`, `DEPOIS_DA_TRAVESSIA` e a frase de `TravessiaNaoConfirmada`. Sem a palavra «linha» (ADR 0033): o texto diz «Servidor 1.0» e «Servidor 2.0». |
| `ui/tela-server.css` | a frase do painel e a caixa da camada (reusa `camada-nomear.css`). |
| `ui/ponte-web.js` | `escolher_atualizacao` recusado na página, como os outros dois comandos do atualizador. |
| `ui/celular-texto.css` | 4 linhas geradas para os tamanhos de letra novos (o teste `celular` exige o arquivo em dia). |
| `tests/frontend.rs` | quatro guardas estáticos (ver §4). |
| `bancada/travessia-de-versao.cjs` (novo) | a bancada Playwright do diálogo. |

**O que não muda**: nada de WebRTC, nenhum crate de transporte, nenhuma linha
de `seele-proto` (protocolo continua 9, Servidor 1.0, API de MOD 6), nenhum
fio, nada do `seele-server`, do launcher, do `manifesto.py` ou do
`publicar.sh`. O patch fica inteiro dentro de `apps/seele-app/`.

## 3b. As saídas dos MODs de API antiga (revisão de 25/10)

O defeito, lido no código de `8faa3379b`: `ui/tela-boot.js` chamava
`resolver_mod_de_api_antiga` a cada clique em ATUALIZAR ou DESLIGAR, e
`src/mods.rs` abria o banco com `Persistence::open` (que migra) e gravava a
troca na hora. Com vários MODs antigos, o banco era convertido no primeiro
clique, sem cópia e sem desfazer.

| arquivo | mudança |
|---|---|
| `src/copia_do_banco.rs` (novo) | a cópia de segurança verificada, portada da regra do `antes_de_hospedar::preparar_copia` da 0.17, sem o resto daquela tela: trava (`BEGIN IMMEDIATE`), contagem por tabela, `VACUUM INTO` num nome provisório, `integrity_check` = `ok`, mesmas tabelas e contagens, mesmo esquema; só então o nome final `copias-de-seguranca/<servidor\|padrao>/esquema-<n>-<ms>.db` (o lugar e o nome que a 0.17 lista). Uma cópia que não passa é apagada. 3 testes. |
| `src/mods.rs` | `resolver_mod_de_api_antiga` (por MOD, convertia na hora) dá lugar a `aplicar_saidas_dos_mods_antigos(config, servidor, escolhas, no_ar)`, em lote, nesta ordem: servidor parado; servidor guardado; **uma escolha para cada MOD que hospedar recusaria, e só para eles** (`FaltaEscolher`, `NaoEDeApiAntiga`); os bytes de cada versão nova (`PacoteNaoServe`); **a cópia verificada** (`CopiaNaoFeita`); só então `Persistence::open` e as escolhas. Novos: `EscolhaDoModAntigo`, `FalhaAoResolverModAntigo::{FaltaEscolher, CopiaNaoFeita}`. 3 testes novos com a fixture da 0.15.1; os 6 de antes passam pelo botão final. |
| `src/main.rs` | o comando Tauri `resolver_mod_de_api_antiga` sai; entra `aplicar_saidas_dos_mods_antigos(servidor, escolhas)`, que devolve onde ficou a cópia. `mod copia_do_banco;`. |
| `src/versoes.rs`, `src/servidores.rs` | as duas provas de ponta a ponta (saída 1 e saída 3, com o servidor real subindo depois) passam pelo comando novo; um comentário. |
| `ui/tela-boot.js` | ATUALIZAR e DESLIGAR só estagiam (`aria-pressed`, «ESCOLHIDO: …», DESFAZER por MOD); CANCELAR esquece tudo sem chamar o Rust; o botão final só vale com todas escolhidas, baixa pelo «obter», chama o comando novo e hospeda de novo. Uma recusa é dita e as escolhas ficam. |
| `ui/frases.js` | frases de `FaltaEscolher` e `CopiaNaoFeita`; a de `BancoNaoRespondeu` deixa de prometer «o MOD continua como estava» (ela pode vir no meio da conversão) e aponta a cópia de antes; a de `PacoteNaoServe` diz «Nada foi mudado no servidor». |
| `ui/ponte-web.js` | o comando novo recusado na página, no lugar do antigo. |
| `tests/frontend.rs` | guarda novo: o único `invoke` que grava é o do botão final; escolher, DESFAZER e CANCELAR não chamam o Rust; o comando da 0.16.0 não volta. |
| `bancada/saidas-de-quem-hospeda.cjs`, `bancada/hospedar-com-mod-antigo.cjs` | o fluxo estagiado (abaixo) e as frases novas. |
| `src/perfis_api5_testes.rs` (novo; declarado em `mods.rs` com `#[path]`, porque o recorte de produção de `main.rs` não aceita `#[cfg(test)] mod x;` sem corpo, e o rustfmt não deixa a declaração numa linha só) e `testes/pasta-da-0.15.1-perfis/` (cópia sem edição da fixture do compat na 0.17, conferida com `diff -r`) | o caso relatado, Perfis 3.3.1 (API 5) → 4.0.0 (API 6), de ponta a ponta: um teste com os pacotes substitutos da fixture (roda sempre) e um com os **pacotes oficiais** (`#[ignore]`, lê a pasta deles de `SEELE_PERFIS_OFICIAIS`; ver §4). |

**O que fica igual**: a recusa de hospedar com MOD de API antiga
(`mods_de_api_antiga`, a API mínima 6), a saída «HOSPEDAR PELO SERVIDOR 0.15»
e a regra de dados (`enable`/`disable` não tocam `mod_data`, ADR 0045).

### O texto que a pessoa lê (0.16.x → 0.17.0)

> **ESTA ATUALIZAÇÃO MUDA COM QUEM VOCÊ CONVERSA**
> A 0.17.0 fala o Servidor 2.0, e esta 0.16.x fala o Servidor 1.0. Os dois não se entendem.
> - A 0.17 fala WebRTC, um transporte novo, e não conversa com a 0.16 nem com versões anteriores.
> - Depois de atualizar, você não entra mais nos servidores de quem continuar na 0.16.x.
> - E quem continuar na 0.16.x não entra nos servidores da 0.17.0, nem nos que você hospedar nela.
> - A versão de agora continua disponível pelo launcher do SEELE, para abrir quando precisar entrar num servidor antigo.
>
> [CANCELAR] [ADIAR] [ATUALIZAR MESMO ASSIM]

A frase do WebRTC só aparece quando a oferta é Servidor 2.0. Se o manifesto
não disser a versão do servidor da oferta (`nao_sabe`), a camada também abre,
com «NÃO SEI COM QUEM A VERSÃO NOVA CONVERSA». Numa atualização 0.16 → 0.16.x
(as duas Servidor 1.0) não há camada nem frase: o clique é o de sempre.

A afirmação «continua disponível pelo launcher» é a da spec/ADR 0046 e do
contrato E4; não foi medida numa máquina com a 0.17 instalada.

## 4. Testes rodados e resultados

Todos numa árvore exportada de `8faa3379b` (`git archive`), com o patch aplicado
e um `--target-dir` próprio fora dela, no macOS (Darwin 27). A tabela é da
revisão de 25/10 (atualizador + saídas dos MODs), rodada inteira de novo.

| o quê | comando | resultado |
|---|---|---|
| formatação | `cargo fmt --manifest-path apps/seele-app/Cargo.toml -- --check` | limpo |
| lint | `cargo clippy --manifest-path apps/seele-app/Cargo.toml --all-targets -- -D warnings` | limpo |
| testes do app | `cargo test --manifest-path apps/seele-app/Cargo.toml --config 'env.SEELE_PERFIS_OFICIAIS="…"' --no-fail-fast -- --include-ignored` | **826 passaram, 0 falharam**: unidade 427 (com os dois do Perfis, o oficial incluído), `celular` 11, `empacotamento` 22, `encerramento` 5, `fontes` 5, `frontend` 312, `manifesto_hibrido` 11, `marca` 9, `permissoes` 5, `tokens` 3, `web` 16. Sem a pasta dos oficiais, 825 e 1 ignorado. (24/10, só o atualizador: 817) |
| a bancada do atualizador | `node apps/seele-app/bancada/travessia-de-versao.cjs <pasta>` (Playwright Chromium headless, ponte simulada) | 10 cenas, todas passaram |
| a bancada das saídas | `node apps/seele-app/bancada/saidas-de-quem-hospeda.cjs <pasta>` (idem) | passou: escolher/DESFAZER/CANCELAR sem chamar o Rust; o botão final com as três escolhas; a recusa dita com as escolhas mantidas |
| as frases das recusas | `node apps/seele-app/bancada/hospedar-com-mod-antigo.cjs` | passou, com `FaltaEscolher` e `CopiaNaoFeita` |
| bancadas vizinhas | `camadas.cjs`, `ponte-sem-gesto.cjs`, `ponte-web.cjs` | passaram («as 21 provas passam» na ponte web) |
| aplica na base | `git apply --check` numa segunda exportação de `8faa3379b` | aplica (os 26 arquivos) |

**Os testes das saídas, com a fixture da 0.15.1** (`testes/pasta-da-0.15.1`,
banco no esquema 13 com o `seele/estilo` 3.1.0 da API 4 ligado e o quintal
dele), em `src/mods.rs`, módulo `saidas_do_mod_antigo`:

- `antes_da_confirmacao_o_banco_da_0_15_1_fica_byte_identico` — o que a tela
  lê antes do botão final, e cada botão final recusado (lista vazia →
  `FaltaEscolher`; MOD que não é antigo e escolha repetida →
  `NaoEDeApiAntiga`; pacote de outro MOD → `PacoteNaoServe`; servidor no ar →
  `ServidorNoAr`): o SHA-256 de `seele.db`, `-wal`, `-shm` e `-journal` igual
  ao de antes, e nenhuma cópia criada. Cancelar e DESFAZER, na tela, não
  chamam o Rust (bancada e guarda abaixo), então também não tocam o banco;
- `sem_a_copia_verificada_nada_e_convertido` — com a pasta das cópias
  impossível de criar, `CopiaNaoFeita`, e o banco byte a byte como estava, no
  esquema 13;
- `o_botao_final_copia_o_estado_de_antes_e_so_entao_converte` — a cópia tem o
  esquema 13 e o **mesmo conteúdo lógico** do banco de antes (toda tabela,
  toda linha, todo valor), com o 3.1.0; o banco passa ao esquema deste build
  com o 4.0.0, e o `mod_data` do estilo fica igual;
- os 6 testes de antes (desligar, atualizar, servidor no ar, servidor
  desconhecido, MOD que não é antigo, pacote de outro MOD) passam pelo botão
  final; e as duas provas de ponta a ponta em `versoes.rs` (saída 1 e saída 3,
  com o servidor real subindo e o 4.0.0 lendo o quintal da 0.15) também;
- `copia_do_banco::testes` (3): a cópia sai conferida e a fonte não muda; uma
  cópia estragada entre escrever e verificar não passa e não fica; um arquivo
  que não é banco não é copiado.

**O caso Perfis 3.3.1 (API 5) → 4.0.0 (API 6)** (`src/perfis_api5_testes.rs`).
A mesma sequência nas duas medidas, pelas funções de verdade: o HOSPEDAR
recusaria com `("seele/perfis", Some(5))`; antes da confirmação os botões
finais recusados (sem escolha → `FaltaEscolher`, servidor no ar) deixam o
SHA-256 de `seele.db`/`-wal`/`-shm`/`-journal` igual, o esquema em 13 e
nenhuma cópia; o botão final faz **uma** cópia, no esquema 13, com a linha do
3.3.1 e o `mod_data` de antes, e só então converte (esquema 17) e liga o 4.0.0;
o `mod_data` do Perfis fica byte a byte igual; hospedar não recusaria mais; e
o servidor real sobe e a metade de servidor do 4.0.0 responde a um `view` de
quem hospeda. CANCELAR e DESFAZER não chamam o Rust (guarda e bancada acima),
então não há o que medir no banco por eles além do «antes» acima.

| medida | pacotes | `mod_data` | resultado |
|---|---|---|---|
| `com_os_substitutos_da_fixture` (roda sempre) | os **substitutos** de `testes/pasta-da-0.15.1-perfis/` (id, versão e API dos oficiais; não os bytes) | `perfil:1`, gravado pelo 3.3.1 substituto **rodando no código da v0.15.1** (`54f542f94`), segundo o `LEIA.md` da fixture | passou; o 4.0.0 substituto leu o perfil byte a byte |
| `com_os_pacotes_oficiais` (`#[ignore]`) | os **oficiais** 3.3.1 e 4.0.0 de uma pasta local `pacotes-oficiais-perfis/` (copiados pelo Codex; **não versionada**, é preciso ter os pacotes oficiais à mão), sem alteração: o hash do 3.3.1 calculado sobre a pasta e o do 4.0.0 devolvido pelo `instalar_de` **conferem com o catálogo** (`ec9fb794…` e `6c5ecf4e…`) | `profiles` = `{"1": {revision 1, displayName, pronouns, bio, status, accent, effect "aurora", avatar/banner null}}`, **a forma que o servidor do 3.3.1 oficial grava** (`blank()`/`validate()` dele); escrito pelo teste no banco da fixture, **não** pelo 3.3.1 rodando numa 0.15.1 | passou; o 4.0.0 **oficial** de pé devolveu `ok: true` e `profiles["1"]` igual ao gravado |

O `LEIA.md` da fixture, copiado sem edição, cita o teste dela na 0.17
(`travessia_api5_testes`); no backport o teste é `mods::perfis_api5_testes`.

A medida oficial rodou assim (o `--config` do cargo põe a variável, sem
prefixo no shell):

```sh
cargo test --manifest-path apps/seele-app/Cargo.toml \
  --config 'env.SEELE_PERFIS_OFICIAIS="<pasta com os pacotes oficiais 3.3.1 e 4.0.0>"' \
  -- --include-ignored perfis_api5_testes
```

**Revertido, reprova** (o Perfis): com a cópia feita depois da conversão, os
dois testes do Perfis reprovam («a cópia não é do estado de antes»: esquema 17
em vez de 13). Restaurado, passam.

**Revertido, reprova** (as saídas):

- `tela-boot.js` de volta ao da 0.16.0 (convertia a cada clique): reprovam o
  guarda `as_saidas_dos_mods_antigos_so_escolhem_e_quem_converte_e_o_botao_final`
  e a bancada das saídas;
- o «escolher» chamando o Rust (`invoke` no clique): reprovam o mesmo guarda
  («escolher uma saída chama o Rust») e a bancada;
- a cópia depois da conversão, em `aplicar_saidas_dos_mods_antigos`: reprovam
  `sem_a_copia_verificada_nada_e_convertido` (o SHA-256 do banco mudou) e
  `o_botao_final_copia_o_estado_de_antes…` (a cópia saiu no esquema 17);
- sem a exigência de uma escolha por MOD: reprova
  `antes_da_confirmacao_o_banco_da_0_15_1_fica_byte_identico` (a lista vazia
  converteu e fez cópia).

A bancada usa o Playwright pelo `bancada/playwright.cjs` (variável
`PLAYWRIGHT` ou `node_modules`); aqui ela rodou por um invólucro que aponta a
instalação local.

**Os testes da decisão** (`src/atualizador.rs`, módulo `testes`, 8):

- `esta_versao_e_da_linha_1` — a 0.16 diz ser da linha «1» (`linha_deste_app`);
- `da_0_16_para_a_0_17_avisa` — legado → nova: `Cruza{de:"1", para:"2"}`,
  e prosseguir sem confirmação é `TravessiaNaoConfirmada`;
- `da_0_16_para_a_0_16_x_nao_avisa` — mesma linha: `Mesma`, sem aviso, instala
  sem confirmação;
- `sem_a_linha_no_manifesto_avisa` — sem entrada, `server` ilegível, manifesto
  vazio: `NaoSabe`, que avisa;
- `cancelar_nao_instala`, `adiar_nao_instala_e_lembra_a_versao`;
- `prosseguir_pede_a_confirmacao_desta_versao` — a confirmação de outra versão
  não serve;
- `a_fronteira_vai_para_a_tela_com_o_tipo` — a forma JSON que a tela lê.

**Os guardas em `tests/frontend.rs`** (4 novos, e o contador de
`FalhaAoAtualizar` passou de 8 para 9 com a frase nova):
`instalar_uma_travessia_e_recusado_antes_de_baixar_sem_a_confirmacao_desta_versao`,
`o_clique_em_instalar_passa_pela_confirmacao_quando_a_oferta_pede`,
`a_confirmacao_da_travessia_e_um_alertdialog_com_as_tres_saidas`,
`a_falha_da_travessia_nao_confirmada_tem_frase`.

**Provados contra a regressão**, revertendo o conserto e vendo falhar:

- ligar o botão INSTALAR direto a `instalarAtualizacao(null)`: reprovam o guarda
  `o_clique_em_instalar_passa_pela_confirmacao_quando_a_oferta_pede` e a
  bancada («INSTALAR numa travessia não abriu a confirmação»);
- tirar a conferência `pode_instalar` de `instalar_atualizacao`: reprova o
  guarda `instalar_uma_travessia_e_recusado_antes_de_baixar…`, e o clippy com
  `-D warnings` acusa `TravessiaNaoConfirmada` e `pode_instalar` sem uso.

Restaurado o conserto, tudo volta a passar (a tabela acima é da árvore
restaurada).

**`ui/celular-texto.css`** é gerado; as 4 linhas novas foram escritas à mão no
formato do gerador e o teste `o_texto_do_sistema_alcanca_todo_tamanho_do_desktop`
confirma que o arquivo é idêntico à saída dele.

**Capturas** (`evidencias/`, Chromium 1280×860 e 720×860):
`01-painel-com-a-travessia.png`, `02-confirmacao-0.16-para-0.17.png`,
`03-depois-de-cancelar.png`, `04-confirmacao-ja-adiada.png`,
`05-confirmacao-nao-sabe.png`, `06-confirmacao-720px.png`; e as saídas dos
MODs com as três escolhas estagiadas e o botão final,
`07-saidas-estagiadas.png`.

## 5. A sequência de distribuição

O atualizador de **toda** 0.16 lê um só endereço:
`releases/latest/download/latest.json` do SEELE-RELEASES, ou seja, o
`latest.json` do release marcado «latest». Daí a ordem.

1. **Cortar e publicar a 0.16.x** (por exemplo `v0.16.1`) a partir de
   `8faa3379b` + este patch, **como latest** (sem `--linha-legada`). Para quem
   está na 0.16.0 ela é Servidor 1.0 → mesma versão do servidor → atualização
   comum, sem aviso; o `notes` mostrado no «O QUE MUDA» é o que o
   `manifesto.py` escrever.
2. **Deixá-la como latest por um tempo**, o bastante para quem usa a 0.16.0
   apertar PROCURAR ATUALIZAÇÃO e instalá-la. A consulta é só manual (ADR 0026,
   «não há consulta automática ao abrir»): ninguém recebe a 0.16.x sem apertar
   o botão. Não há medida de quantos atualizaram — o produto não telefona para
   casa; o tempo é decisão de quem publica.
3. **Só depois publicar a 0.17.0.** O `publicar.sh` acumula o manifesto a partir
   do `latest.json` publicado, então o da 0.17.0 trará `0.17.0 → "2.0"` e
   `0.16.x → "1.0"` em `versions[]`. Uma 0.16.x que procurar atualização vê a
   0.17.0 no topo, lê `unit.server "2.0"`, compara com a sua «1» e abre a
   confirmação antes de baixar.

**Por que a ordem não pode inverter.** No `manifesto.py` de `8faa3379b`
(`montar`, linhas 323–329), um release de uma linha **mais velha** que a mais
nova do manifesto não vira o topo («Um conserto de uma linha mais velha não
vira a atualização de quem está na mais nova»), e o `publicar.sh` tem
`--linha-legada` para publicá-lo com `make_latest=false`. Uma 0.16.x publicada
**depois** da 0.17 seria, portanto, legada: não chega a ninguém pelo botão de
atualizar da 0.16.0, que passaria a ver a 0.17 no topo.

### Alternativa: publicar a 0.17 com `make_latest=false`

- Publicar a 0.17.0 **sem** marcá-la latest mantém o `latest.json` da 0.16.x
  como o servido; nenhuma 0.16 recebe a oferta da 0.17 pelo botão.
- O `publicar.sh` de hoje só manda `make_latest=false` com `--linha-legada`,
  que foi feito para conserto de linha velha; usar isso na linha mais nova, ou
  desmarcar «Set as the latest release» na tela do GitHub, é decisão de quem
  publica e não foi exercitado.
- Custos, lidos no código e **não medidos**:
  - a 0.17 também procura em `releases/latest/…`; quem já está na 0.17 só vê a
    0.17.1 quando um release da linha 2 virar latest;
  - o launcher da 0.16 lê o mesmo `latest.json`: sem a entrada `server "2.0"`
    nele, um link `v=2.0` não acha a versão para baixar, e a tela HOSPEDAR da
    0.16 não oferece a linha 2.
- Ela pode ser combinada com a sequência acima (publicar a 0.17 não-latest
  primeiro, e marcá-la latest depois do intervalo da 0.16.x).

### Quem não instalar a 0.16.x

- **Nenhuma promessa retroativa.** A 0.16.0 já instalada não tem este código e
  não ganha confirmação nenhuma. Antes de instalar a 0.17, ela mostra só a
  frase «HÁ VERSÃO NOVA: da 0.16.0 para a 0.17.0», o texto do `notes` do
  `latest.json` no bloco «O QUE MUDA» (com as quebras de linha) e o aviso fixo
  «ANTES DE INSTALAR» sobre fechar e reabrir. INSTALAR E REABRIR continua
  sendo **um clique**.
- O que dá para fazer por esse lado sem tocar na 0.16.0 é escrever o aviso da
  quebra no `notes` da 0.17.0: o bloco `AVISO-DO-ATUALIZADOR` de
  `empacotar/notas/0.17.0.md`, que o `empacotar/manifesto.py` leva ao `latest.json`.
  Dizer, sim; bloquear, não.
- Quem pular a 0.16.x e instalar a 0.17 continua podendo abrir a 0.16 pelo
  launcher (ADR 0046) — não medido aqui.

## 6. Limites do que foi medido

- A decisão e a recusa antes do download são testadas em Rust (unidade e
  guarda de ordem no fonte). **Não** houve uma atualização real ponta a ponta:
  nenhum `latest.json` de 0.17 foi servido a uma 0.16.x empacotada e
  assinada, e nada foi baixado ou instalado.
- O diálogo foi medido no Chromium headless do Playwright com a ponte Tauri
  simulada (a forma da `VersaoNova` é a que o Rust serializa, conferida pelo
  teste `a_fronteira_vai_para_a_tela_com_o_tipo`). WebKit (o WebView do macOS),
  WebView2 e WebKitGTK não foram exercitados.
- Leitor de tela real não foi usado; o que se mediu são os atributos ARIA, o
  foco inicial e o Tab preso.
- **As saídas dos MODs** foram medidas em Rust com as fixtures da 0.15.1 (o
  `seele/estilo` 3.1.0, API 4; e o Perfis 3.3.1, API 5) e na bancada com a
  ponte simulada. O Perfis foi medido com os **pacotes oficiais** e com os
  substitutos (§4); com os oficiais, o `mod_data` foi escrito pelo teste na
  forma do 3.3.1, e não gravado por ele numa 0.15.1. Medido só o lado do
  servidor (`view` respondido pelo 4.0.0): a **superfície de cliente** do
  Perfis não foi aberta numa janela, e o app empacotado não foi rodado com um
  banco real.
- O quintal (`mod_data`) fica igual depois da troca, como antes; isso não prova
  que a versão nova de cada MOD entende os dados antigos (só a do estilo, nas
  provas de ponta a ponta de `versoes.rs`).
- A janela entre a cópia e a conversão: a trava sai logo antes de
  `Persistence::open`; um processo de fora escrevendo naquele instante não é
  impedido (o mesmo limite que a 0.17 registra). Não medido.
- Uma falha do banco **depois** da cópia (`BancoNaoRespondeu`) pode deixar a
  conversão ou as escolhas pela metade; a frase aponta a cópia de antes, e
  restaurá-la é manual. Não exercitado.
