# Lote L1-o-que-a-release-leva — relatório

Base `96d3a42`. Seis commits locais, nada empurrado (sem push, tag, release ou Actions):

| commit | item |
|---|---|
| `d53921d` | F1-classificador (e o `aviso` de várias linhas, achado no caminho) |
| `88a91ea` | F1-ci-aqui (4): a linha da bateria no corpo do release |
| `8882910` | F1-ci-aqui (2) e (3): os alvos `uri` e `datagrama_do_encontro`, as sementes e o teste delas |
| `62e3f7e` | F1-ci-aqui (1): os jobs `deny` e `fuzz-curto` no `ci.yml` |
| `57ea9b9` | F1-install |
| `60f6a21` | só comentários: a faixa certa da v0.15.0 e as datas das medidas (correção da autorrevisão) |

HEAD: `60f6a21`. Nenhum assunto contém uma das catorze cadeias do G1 (conferido com a lista do plano 1B,
`grep -F` cadeia por cadeia). Todos terminam com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

Fio, API e CSP: `git diff 96d3a42 -- api/ crates/seele-proto/src/ apps/seele-app/tauri.conf.json Cargo.lock fuzz/Cargo.lock`
volta vazio. `PROTOCOL_VERSION` continua 8; `cargo xtask check-api` e `check-versao` verdes. Nenhuma dependência nova.

## Item 1 — F1-classificador (`d53921d`)

**O que fiz** em `empacotar/publicar.sh`, `notas_das_mudancas`:

- Um braço de `case` com os prefixos de papel por extenso, com e sem escopo (`docs`, `test`, `chore`, `refactor`, `ci`,
  `build`, `style`), que fica de fora como antes.
- O `*)` final conta (`ndm_sem_classe`) em vez de descartar: sem prefixo, ou com prefixo desconhecido (`wip:`).
- Seções vazias e conta > 0: «_Os commits desta faixa não dizem pelo prefixo se mudam o produto, e este resumo não
  adivinha. A lista inteira está logo abaixo._» (o texto da lista, em duas linhas de markdown). Não promete notas no topo.
- «Nenhuma mudança de produto» só quando a conta é zero e as seções estão vazias.
- Um `aviso` a quem publica com a conta (singular e plural) e o arquivo `empacotar/notas/<versão>.md`, que sai no topo.
- **Além da lista:** com as seções cheias e conta > 0, uma linha depois delas: «_Além destes, N commits desta faixa não
  dizem pelo prefixo…_». Sem ela, um `fix` e cem assuntos sem prefixo davam uma página que resume a versão por uma
  linha só. O defeito é o mesmo do item, mais brando.
- **Achado no caminho, consertado porque o aviso novo depende dele:** `aviso()` imprimia só o `$1`. As chamadas que
  mandavam o conserto na segunda linha («Classifique-o em ESCOPOS_DE_PRODUTO…», o `git push origin v…` da tag) o
  perdiam. Agora imprime todas as linhas, com recuo.
- O comentário de cima («Só `feat` e `fix` entram…») foi reescrito; o índice (:109-119) não foi tocado (é do Z).

Medido com o script da base: a faixa `v0.14.2..v0.15.0` tem quatro assuntos, nenhum com prefixo, um deles com 85
arquivos (`14d9c30`), e o `--notas` dava «nenhuma mudança de produto»; `e2fac4dab..96d3a42` hoje tem 197 assuntos, 23
`docs:`, e dava a mesma frase.

O `case` novo rodou igual em `/bin/sh` (bash 3.2 do macOS), `bash`, `dash` e `ksh` desta máquina.

**Testes (xtask/tests/empacotamento.rs):**

| teste | RED antes | GREEN depois |
|---|---|---|
| (a) `um_assunto_sem_prefixo_nao_vira_nenhuma_mudanca_de_produto` | «um assunto sem prefixo pode ser mudança de produto, e a página afirmou que não há nenhuma» | ok |
| (b) `um_prefixo_desconhecido_tambem_nao_vira_nenhuma_mudanca_de_produto` | «um prefixo que o resumo não conhece virou «nenhuma mudança de produto»» | ok |
| `quem_publica_fica_sabendo_quantos_assuntos_o_resumo_nao_classificou` | «o aviso não diz quantos assuntos ficaram sem classificar» | ok |
| `com_secoes_cheias_o_que_ficou_sem_classificar_tambem_e_dito` | «a página resumiu a versão pelo único assunto que se classificou, e calou…» | ok |
| `um_aviso_de_varias_linhas_chega_inteiro_a_quem_publica` | «a segunda linha do aviso do escopo desconhecido não chegou» | ok |
| `os_prefixos_de_papel_sao_todos_reconhecidos_com_e_sem_escopo` | passava antes (o código velho descartava tudo calado); é guarda da lista nova, provado pela reversão R2 | ok |
| `sem_feat_nem_fix_na_faixa_nao_se_inventa_secao` (:1781, existente) | — | continua verde |

**Reversões** (cada uma: desfazer, ver vermelho com a frase, restaurar, `cmp` byte a byte com a cópia boa):

- R1: o `publicar.sh` inteiro da base → (a), (b), conta, seções cheias e aviso de várias linhas vermelhos (5 FAILED).
- R2: `ci:` e `ci(…):` fora do `case` → `os_prefixos_de_papel…` vermelho: «com todo assunto de papel, a versão é só de
  papel e a página tem que dizer isso».
- R3: `aviso()` de uma linha só → «a segunda linha do aviso do escopo desconhecido não chegou».
- R4: sem o bloco «Além destes» → «a página resumiu a versão pelo único assunto que se classificou, e calou sobre o que
  não se classificou».

## Item 2 — F1-ci-aqui

### (4) A linha da bateria (`88a91ea`)

`linha_da_bateria <sem_bateria> <pedidos>` é texto puro, exposta por
`publicar.sh --linha-da-bateria "<pedidos>" [--sem-bateria]` (molde do `--decidir`). No corpo, entra em «Como esta
versão foi montada», logo depois do parágrafo do commit, com `$SEM_BATERIA` e `$PEDIDOS`.

- Sem `--sem-bateria`: «Bateria: rodou aqui (fmt, clippy, testes, cargo deny).»
- Com o Windows pedido: «Bateria: rodou aqui (fmt, clippy, testes, cargo deny) e no Windows (testes).» — **desvio
  pequeno da lista**, que dizia só « e no Windows»: a bateria de lá roda só `cargo test --workspace` (publicar.sh,
  bloco `bw_saida`), e sem o «(testes)» a frase leria como se fmt, clippy e deny também tivessem rodado lá.
- Com `--sem-bateria`: «**Esta versão saiu com --sem-bateria: não foi testada antes de publicar.**»

Testes: `a_linha_da_bateria_diz_o_que_rodou_e_onde` (RED: «a linha da bateria não saiu», a opção não existia) e
`o_corpo_de_uma_versao_sem_bateria_diz_que_ela_nao_foi_testada` (Bancada com `--sem-bateria`; RED: «o corpo do release
de uma versão sem bateria não diz que ela não foi testada»). GREEN os dois.

Reversões:
- R5: tirar a linha do corpo → o teste da Bancada vermelho, com a frase acima.
- R6: tirar o ramo do `--sem-bateria` da função → os dois vermelhos («uma versão que saiu sem bateria não diz isso na
  página»).
- R7: o Windows sempre na linha → «sem o Windows pedido, a bateria de lá não roda, e a linha disse que rodou».

O comentário do `ci.yml` (:26-29) que diz que o `--sem-bateria` «pula tudo isso, dizendo que pulou» ganhou «no terminal
de quem publica e no corpo do release».

### (2) e (3) Sementes e alvos novos (`8882910`)

- `fuzz/fuzz_targets/uri.rs`: `seele_proto::uri::analisar`; num convite aceito, `alternativos.len() < LIMITE_DE_ALVOS`
  e a ida e volta `analisar(convite.to_string()) == convite`.
- `fuzz/fuzz_targets/datagrama_do_encontro.rs`: `encontro::analisar` e `ler_aqui`; aceito ⇒ `TAMANHO` bytes; um `AQUI`
  nunca é lido como pedido; a marca do `AQUI` é alfanumérica e ≤ `LIMITE_DA_MARCA`; `ONDE`, `MORO` e `QUEM` remontam
  iguais pelos montadores (o `LEVE` não, porque o `Display` de `SocketAddr` reescreve o endereço).
- Dois `[[bin]]` em `fuzz/Cargo.toml`. Comentários dos alvos em português.
- `fuzz/sementes/<alvo>/` para os cinco alvos (21 arquivos): 9 quadros de controle dos dois sentidos, 2 datagramas de
  mídia, 2 versões, 2 convites, 5 datagramas do encontro.
- `crates/seele-proto/tests/sementes_do_fuzz.rs`, cinco testes:
  - `as_sementes_do_fuzz_estao_na_versao_do_fio` — a prova (b): todo arquivo de `control_frame` **e** de `media_header`
    começa com `PROTOCOL_VERSION` (o datagrama de mídia também a carrega no primeiro byte);
  - `as_sementes_no_disco_sao_as_que_o_codigo_gera` — compara byte a byte com o que `encode`, `encode_datagram`, o
    `Display` do convite e os montadores do encontro geram; falta, diferença e sobra são ditas por nome;
  - `toda_semente_e_uma_entrada_que_o_alvo_aceita`;
  - `todo_alvo_do_fuzz_tem_sementes` — todo `[[bin]]` de `fuzz/Cargo.toml` tem sementes em `sementes()`;
  - `as_sementes_ficam_fora_do_conversor_de_fim_de_linha` — exige a linha nova do `.gitattributes` (abaixo).
  - Com `SEELE_REGERAR_SEMENTES=1`, escreve uma vez por processo (`OnceLock`, antes de qualquer leitura) e não apaga
    nada.
- **Fora da lista de arquivos: `.gitattributes`** ganhou `fuzz/sementes/** -text`. As sementes são conferidas byte a
  byte e o `* text=auto eol=lf` normaliza CRLF de quem o Git tomar por texto. Medido: quatro dos nove quadros de
  controle não têm byte nulo, e o `git ls-files --eol` dava `i/lf` para `cliente-ping` antes da linha. É uma linha nova
  depois do bloco `*.base64` (a linha 59); o lote N mexe nas linhas 26-28, sem sobreposição. O guarda do
  `catalogo.rs` (`todo_vetor_cujos_bytes_sao_o_contrato…`) continua verde.

RED: sem a pasta, `as_sementes_do_fuzz…` («fuzz/sementes/control_frame está vazia ou não existe…»), a conferência e a
do `.gitattributes` vermelhas. GREEN depois de `SEELE_REGERAR_SEMENTES=1` e da linha. Conferi que cada blob comitado é
igual ao arquivo (`git show HEAD:… | cmp`).

Reversões:
- R8: `PROTOCOL_VERSION` 8 → 9 em `version.rs` (só para a prova) → `as_sementes_do_fuzz…` vermelho: «fuzz/sementes/
  control_frame/cliente-entrar-na-sala não começa com a versão do fio (9)…», e a conferência lista cada quadro como
  «difere do que o código gera hoje». `version.rs` restaurado e `git diff --quiet` nele.
- R9: um byte a mais em `uri/minimo` → «fuzz/sementes/uri/minimo difere do que o código gera hoje».
- R10: um arquivo a mais (`uri/sobra`) → «…está no disco e o código não a gera».
- R11: sem a linha do `.gitattributes` → «o .gitattributes não tem a linha `fuzz/sementes/** -text`…».
- R12: `uri` fora de `sementes()` → «o alvo de fuzz «uri» não tem sementes em `sementes()`…».
- R12b: a semente `uri/minimo` trocada por `http://…` → «a semente uri/minimo não é um convite que o `analisar`
  aceite».
- Alvos de fuzz: com a invariante falsificada (`alternativos.len() < 1` no `uri`; `aqui.is_none()` no do encontro), as
  próprias sementes derrubam o alvo em menos de um segundo («panicked … reversão», `deadly signal`). Prova que as
  sementes chegam às invariantes. Os `crash-*` gerados ficaram em `fuzz/artifacts/` (ignorado) e foram apagados.

**Conferência local, sem rede** (`CARGO_NET_OFFLINE=true`): os cinco alvos, 10 s cada, a partir das sementes, sem
nenhum pânico (uri 1,9 M execuções; encontro 2,9 M; control_frame 1,5 M; media_header 2,8 M; version_negotiation
24 M). Medido também o peso das sementes (10 s, `-seed=1`, `cov:`): uri 86 → 601, encontro 52 → 168, control_frame
3 820 → 3 828. Os números estão no doc do teste das sementes.

**O `fuzz/Cargo.lock` não muda no commit, mas qualquer build do fuzz o reescreve.** Ele está atrás do `seele-proto`: não
tem `serde_json`, `itoa`, `memchr` e `zmij`. Medi com o `fuzz/Cargo.toml` da base (sem os alvos novos) e o diff do lock
foi idêntico ao com os alvos novos. Depois de cada build local, devolvi o lock ao da base com a cópia salva, e
`git diff --quiet fuzz/Cargo.lock` passou antes de cada commit.

### (1) Os jobs do `ci.yml` (`62e3f7e`)

- `deny`: checkout, `Swatinem/rust-cache@v2` (chave `ci-deny`), `cargo install cargo-deny --locked`,
  `cargo deny check advisories licenses bans sources`.
- `fuzz-curto`: `rustup toolchain install nightly --profile minimal`, `cargo install cargo-fuzz --locked`, e um passo
  por alvo: `cargo +nightly fuzz run <alvo> fuzz/corpus/<alvo> fuzz/sementes/<alvo> -- -max_total_time=60
  -create_missing_dirs=1`, cada um com `if: ${{ !cancelled() }}` e `timeout-minutes: 15`.
- O gatilho (`on: workflow_dispatch`, :43-44) não mudou; os dois jobs foram acrescentados no fim, depois de
  `bancadas-de-mod`, e os testes do `check_runtime.rs` que leem o `ci.yml` continuam verdes (`cargo test -p xtask`).

**Desvios da lista no comando do fuzz, medidos:**
- `fuzz/corpus/<alvo>` antes de `fuzz/sementes/<alvo>`: o libFuzzer grava o que acha na **primeira** pasta. Rodando o
  comando da lista (`… run uri <cópia das sementes> -- -max_total_time=5`), 2 sementes viraram 658 arquivos. Na máquina
  de quem roda a conferência local, isso reprovaria a conferência das sementes; no runner, seria só lixo.
- `-create_missing_dirs=1`: sem ele, o libFuzzer recusa a pasta de corpus que não existe («ERROR: The required
  directory … does not exist», medido).
- `cargo +nightly fuzz run`: o `rust-toolchain.toml` fixa 1.97.1, e o `cargo fuzz` precisa do nightly.
- Sem `--locked` no fuzz, por causa do lock atrasado (acima).

Teste `o_ci_roda_deny_e_fuzz` (RED: «o ci.yml não roda `cargo deny check`…»; GREEN). Ele exige um `run:` com
`cargo deny … check` cobrindo as quatro conferências (ou o `check` sem nome), e, para cada `[[bin]]` de
`fuzz/Cargo.toml`, um `cargo … fuzz run <alvo>` com `fuzz/sementes/<alvo>` na linha e fora do primeiro lugar.

Reversões:
- R13: sem o passo do deny → «o ci.yml não roda `cargo deny check`, e um aviso de segurança novo…».
- R15: `cargo deny check licenses` → «o `cargo deny` do ci.yml deixou de conferir `advisories`…».
- R14: sem o passo do `uri` → «o ci.yml não roda o alvo de fuzz «uri» a partir de fuzz/sementes/uri…».
- R16: as sementes como primeira pasta do `media_header` → «o fuzz de «media_header» grava o que acha em
  fuzz/sementes/media_header…».
- R17: sem o job `fuzz-curto` inteiro → «o ci.yml não roda o alvo de fuzz «version_negotiation»…» (o primeiro
  `[[bin]]` de `fuzz/Cargo.toml`).

**Conferência local do deny:** `cargo deny check --disable-fetch` não existe no cargo-deny 0.20.2 desta máquina
(«unexpected argument '--disable-fetch'»); o equivalente é a opção global `--offline`.
`cargo deny --offline check advisories licenses bans sources` → «advisories ok, bans ok, licenses ok, sources ok»,
saída 0, com o banco de avisos local de 01/10/2026 (`3461c0d`).

## Item 3 — F1-install (`57ea9b9`)

`install.sh`:
- `REPO_DAS_VERSOES="DATA-AND-DEV/SEELE-RELEASES"` na API (`releases/latest`) e no `BASE`;
  `REPO_DO_CODIGO="DATA-AND-DEV/SEELE"` só na dica de compilar.
- O `SHA256SUMS` vem antes do pacote. Sem `$PACOTE` nele: «a versão $VERSAO não publica o servidor para $SISTEMA.»
  e, na linha seguinte, «Compile do código-fonte: git clone https://github.com/DATA-AND-DEV/SEELE && cd SEELE && cargo
  build --release --bin seeled». **Desvio:** a lista não tinha o `cd SEELE`, e sem ele o `cargo build` roda fora do
  clone (o script da base já tinha o `cd SEELE` na mensagem de «não achei nenhuma versão»).
- A arquitetura é conferida **depois** da lista de somas e antes do pacote: macOS só com `sysctl -in hw.optional.arm64`
  = 1 (Intel recebe a frase de compilar), Linux só x86_64 (a mesma frase). A ordem faz a lista responder primeiro, e
  deixa o teste (b) independente da máquina.
- O comentário de :55 saiu; o novo diz que o pacote do macOS é da arquitetura de quem compila (`empacotar/macos.sh`
  empacota o `seeled` do alvo daquela máquina) e que quem publica compila num Apple Silicon.
- O cabeçalho diz de onde baixa e em que ordem; a mensagem de «não achei nenhuma versão publicada» nomeia o repositório
  e usa a mesma dica de compilar.

`install.ps1`: `$repoDasVersoes = 'DATA-AND-DEV/SEELE-RELEASES'` na API e no `$base`, com o porquê num comentário; a
conferência da soma ficou como estava. `docs/assinatura-e-atualizacao.md:127`: o exemplo de `endpoints` mostra
SEELE-RELEASES (só ele: a seção é a de quem monta a chave do zero, e a ponte antiga é decisão do dono).

Testes:

| teste | RED antes | GREEN depois |
|---|---|---|
| (a) `os_instaladores_de_uma_linha_baixam_de_onde_o_publicar_publica` | «install.sh deixou de declarar REPO_DAS_VERSOES…» | ok |
| (b) `o_instalador_de_uma_linha_confere_a_lista_de_somas_antes_de_baixar_o_pacote` (`cfg(unix)`, sem rede, `file://`) | saída «erro: não consegui baixar seele-cli-9.9.9-macos.tar.gz.» | ok |
| `o_instalador_de_uma_linha_instala_quando_a_soma_confere` (`cfg(unix)`, `uname`/`sysctl` dublês: Apple Silicon nativo e sob Rosetta) | passava antes (guarda do caminho bom) | ok |
| `uma_maquina_sem_pacote_da_arquitetura_dela_recebe_a_frase_de_compilar` (`cfg(unix)`: Mac Intel e Linux aarch64 de mentira) | «Darwin x86_64: o install.sh instalou um pacote de outra arquitetura» | ok |

O (a) compara por igualdade exata com os `REPOS` do `publicar.sh` (o mesmo recorte de :2054), porque
`DATA-AND-DEV/SEELE` é prefixo de `…-RELEASES`; e exige que toda linha com `api.github.com/repos/` ou
`/releases/download/` use a variável.

Reversões:
- R18: `REPO_DAS_VERSOES="DATA-AND-DEV/SEELE"` → «install.sh baixa de «DATA-AND-DEV/SEELE», e o publicar.sh publica em
  ["DATA-AND-DEV/SEELE-RELEASES"]…».
- R19: o mesmo no `install.ps1` → a mesma frase com `install.ps1`.
- R20: o download montado com `$REPO_DO_CODIGO` → «install.sh monta um endereço de versão sem REPO_DAS_VERSOES».
- R21: o pacote baixado antes da lista → (b) vermelho com «erro: não consegui baixar seele-cli-9.9.9-macos.tar.gz.»
- R22: sem a conferência do Apple Silicon → «Darwin x86_64: o install.sh instalou um pacote de outra arquitetura».
- R23: conferência pelo `uname -m` → «num Mac Apple Silicon sob Rosetta, com o pacote publicado e a soma certa, o
  install.sh não instalou».

A recusa do (b) rodou igual em `dash`, `bash` e `ksh`.

## Arquivos mudados

`empacotar/publicar.sh`, `xtask/tests/empacotamento.rs`, `.github/workflows/ci.yml`, `fuzz/Cargo.toml`,
`fuzz/fuzz_targets/uri.rs` (novo), `fuzz/fuzz_targets/datagrama_do_encontro.rs` (novo), `fuzz/sementes/**` (novo, 21
arquivos), `crates/seele-proto/tests/sementes_do_fuzz.rs` (novo), `install.sh`, `install.ps1`,
`docs/assinatura-e-atualizacao.md`, e **`.gitattributes`** (fora da lista; ver item 2).

## O que rodou aqui, e o que não pôde

Rodou: `cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets --all-features -- -D warnings` limpos
antes de cada commit (só o aviso ambiental do linker); `rustfmt --check` nos alvos de fuzz (fora do workspace);
`cargo test -p xtask` (empacotamento 87 = 74 + 13 novos; o resto verde); `cargo test -p seele-proto` (inclusive as 5
das sementes); `cargo test -p seele-app catalogo`; `cargo xtask check-api` e `check-versao`; o deny e o fuzz locais
acima.

Não rodou aqui:
- **Os dois jobs no Actions.** O disparo é do dono. Não conferi que `cargo install cargo-deny --locked` e
  `cargo install cargo-fuzz --locked` compilam com o 1.97.1 fixado no runner, nem o nightly do dia, nem o deny com o
  banco de avisos atualizado — este pode trazer aviso novo, e é para isso que o job existe.
- **`install.ps1`**: só no Windows; aqui fica o guarda de texto (a).
- O ramo Linux do `install.sh` rodou só com o `uname` dublê; os três testes `cfg(unix)` não rodam no Windows.
- O caso positivo da linha da bateria pela `Bancada` (bateria inteira com dublês) não foi escrito: custa ~50 s por
  causa do relógio de 5 s por etapa, e a lista pedia o positivo pelo `--linha-da-bateria`.

## Autorrevisão

- Cada frase nova de comentário foi conferida contra o código ou medida. A autorrevisão achou duas atribuições erradas
  minhas e as corrigiu em `60f6a21`: eu tinha posto os «169 assuntos, 146 sem prefixo» na faixa da v0.15.0, quando são
  da faixa seguinte (medido: a da v0.15.0 tinha quatro assuntos sem prefixo); e o doc das sementes dizia que um quadro
  de controle velho seria recusado no primeiro byte, o que é falso com a janela de compatibilidade 1 (o de mídia é).
  Também tirei do doc das sementes uma frase não medida sobre o controle «mal ser tocado» sem semente — a medida
  mostrou o contrário (3 820 × 3 828).
- As afirmações sobre a página publicada da v0.15.0 (sem «bateria», com «nenhuma mudança de produto») e sobre a v0.10.0
  ser a última do repositório do código vêm das medidas do inventário de 04/10/2026, e estão datadas assim; não as medi
  de novo para não ir à rede.
- `alvos_do_fuzz` existe duas vezes (no teste do xtask e no do seele-proto), porque o xtask não depende do seele-proto e
  os dois são crates de teste separados.

## Preocupações

1. **`fuzz/Cargo.lock` atrasado.** Qualquer `cargo fuzz` o reescreve (acrescenta `serde_json` e três dependências dele).
   A lista mandava não mudá-lo, e não mudou; mas quem rodar o fuzz localmente vai ver o arquivo sujo, e o job do CI
   não pode usar `--locked`. Regenerá-lo é uma linha num lote futuro, ou decisão de quem integra.
2. **Desvios da lista**, todos medidos e ditos acima: a pasta de corpus antes das sementes e o
   `-create_missing_dirs=1` no fuzz; `cargo deny --offline` no lugar do `--disable-fetch`; o `cd SEELE` na dica de
   compilar; «e no Windows (testes)»; a linha «Além destes…»; a ordem «lista de somas → arquitetura → pacote»; o
   `aviso()` de várias linhas; a linha do `.gitattributes`.
3. O `aviso()` de várias linhas muda a saída de todos os avisos antigos do `publicar.sh` (agora com as linhas que
   faltavam). Nenhum teste dependia do corte.
4. O efeito do `install.sh` só chega a quem usa o curl do README depois do push do `main` pelo dono, e o Linux e o Mac
   Intel passam a receber a frase de compilar (não há pacote para eles).
5. Divergência pequena no inventário, sem efeito aqui: o anexo diz que a v0.10.0 fala o protocolo 3; a tag local
   `v0.10.0` tem `PROTOCOL_VERSION = 2`. Nenhum texto meu cita o número.
6. Para o lote Z registrar: F1-classificador fecha em `d53921d` (+ `60f6a21`), F1-ci-aqui em `88a91ea`, `8882910` e
   `62e3f7e`, F1-install em `57ea9b9`. O índice (:109-119) e as pendências não foram tocados.
