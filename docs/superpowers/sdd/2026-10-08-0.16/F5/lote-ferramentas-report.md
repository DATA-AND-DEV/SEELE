# Lote ferramentas (F5, onda 1): relatório

Branch `0.16/lotes-ferramentas`, a partir de `0.16/lotes` em `fbeff8e`. Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a51a5962c49b997af`.
Brief: `.superpowers/sdd/lotes/lote-ferramentas-brief.md`. Nada aqui é código do produto, nada tocou o fio, a API de MODs nem os arquivos de outros lotes.

## O que foi feito, item por item

| Item | Commit | O que mudou |
|---|---|---|
| P51-15 | `1345e0e` | `xtask/src/check_api.rs`: dois testes para os `push` sem teste de `conferir_api` e a função `relatar`, tirada do braço `Ok`/`Err` do `run`, com três testes |
| P51-21 | `d7ecb3e` | `.gitattributes`: o comentário do `-text` deixa de dizer que a regra de cima «já bastaria» |
| L1-m2 | `19e04ce` | `empacotar/publicar.sh`: `faixa_da_versao` e `conferir_notas`, chamada por último nas conferências; teste em `empacotamento.rs` |
| L1-m3 (lock) | `f9c2a94` | `fuzz/Cargo.lock` = o do `1277d85` da F1, byte a byte |
| L1-m3 (CI e guardas) | `92783b5` | `.github/workflows/ci.yml` (job `fuzz-curto`) e dois guardas em `empacotamento.rs` |
| L1-m4 | `02c720d` | `install.sh`: `/usr/sbin:/sbin` no fim do PATH da chamada do `sysctl`; um teste comportamental e um de texto |

### P51-15, o check-api

- `um_json_de_api_que_nao_e_json_reprova_o_check_api_dizendo_qual`: um `v7.json` com `{` volta `Err` com uma linha que tem «não é json» e o caminho.
- `um_json_de_api_que_nao_se_abre_reprova_o_check_api_dizendo_qual`: `Ok(dir.join("v8.json"))` sem o arquivo volta `Err` com «não deu para ler» e o caminho.
- `relatar(resultado, n_momentos, n_eventos, saida: &mut impl Write, erros: &mut impl Write) -> ExitCode`. O `run` só a chama, com `stdout` e `stderr`.
  - `o_check_api_que_reprova_sai_com_falha_e_diz_cada_falha_no_erro`: `FAILURE`, cada falha numa linha do erro com o prefixo `check-api:`, e a saída padrão **vazia** (a linha de sucesso não sai junto com uma falha).
  - `o_check_api_que_aprova_sai_com_sucesso_e_diz_o_resumo`: `SUCCESS`, o resumo na saída (versões, momentos, eventos, cobradas) e o erro vazio.
  - `o_check_api_que_nao_consegue_dizer_o_resumo_nao_sai_com_sucesso`: um terceiro teste que o brief não pedia. Escrever o resumo num cano quebrado dá `FAILURE`. O `println!` de antes entrava em pânico, e o código nunca era 0. Sem esse teste, o `is_ok()` do `relatar` seria código que ninguém executa.
- A mensagem de sucesso é a mesma de antes (conferida rodando `cargo run -p xtask -- check-api`: «5 versão(ões) … 5 momento(s) … 17 evento(s) … 0 versão(ões) a partir da 6»).

### P51-21, o `.gitattributes`

Só comentário. O trecho passou a: «A regra de cima bastaria para os vetores que só chegam pelo checkout; para um arquivo escrito numa máquina Windows e conferido pelo hash da cópia de trabalho (`api/*.json`, abaixo), não basta. Nos dois casos, esta sobrevive a alguém afrouxá-la.». O guarda `todo_vetor_cujos_bytes_sao_o_contrato_fica_fora_do_conversor_de_fim_de_linha` ignora as linhas com `#` e continua verde (`-p seele-app --bin seele-app catalogo`: 12 passed).

### L1-m2, o aviso antes da bateria

- `faixa_da_versao` define `ANTERIOR` e `FAIXA` (era o bloco solto do fim do script). `conferir_notas` a chama e roda `git log --no-merges --format='%s' "$FAIXA" | notas_das_mudancas >/dev/null`: o corpo vai para o ralo e **só os avisos** (stderr) saem. Sem tag anterior, diz que é a primeira versão e volta.
- Chamada **por último** nas conferências, logo antes de «--- tudo conferido ---»: o aviso é a última coisa que quem publica lê antes do veredito, e não uma linha perdida entre o Docker e o Windows.
- O corpo continua montado no fim, como hoje, e o fim chama `faixa_da_versao` de novo em vez de depender de as conferências terem rodado.
- Teste: `o_conferir_avisa_dos_assuntos_sem_classe_sem_chegar_a_bateria`. Cria a tag `v1.2.2` e dois commits (um sem prefixo, um `feat(telepatia): …`), roda `["1.2.3", "--conferir"]` e exige os dois avisos («não diz pelo prefixo» e ««telepatia» é um escopo que a tabela») **e** que o diário não tenha nenhum `cargo test`, que é o que prova que a bateria não rodou. Para isso a `Bancada` ganhou o método `git`.
- O guarda do conteúdo, `quem_publica_fica_sabendo_quantos_assuntos_o_resumo_nao_classificou`, continua verde.

### L1-m3, o fuzz com o lock conferido e o crash guardado

- **De onde veio o lock:** pela mensagem da orquestradora, o `fuzz/Cargo.lock` é o do `1277d85` da F1 (`git checkout 1277d85 -- fuzz/Cargo.lock`), num commit só dele. `git diff 1277d85 HEAD -- fuzz/Cargo.lock` sai vazio. Antes de receber a mensagem eu já tinha gerado um por `cargo metadata --offline --manifest-path fuzz/Cargo.toml --format-version 1`, sem rede. Ele é **idêntico** ao do `1277d85` (mesmo sha1 `ae8eefe3…`): entram só `itoa 1.0.18`, `memchr 2.8.3`, `serde_json 1.0.151` e `zmij 1.0.23`, com versão e checksum iguais aos do `Cargo.lock` do workspace, e o `seele-proto` ganha `serde_json`. Os blocos `seele-fuzz` e `seele-proto` só mudam de lugar na ordenação.
- `ci.yml`, job `fuzz-curto`:
  - a justificativa «Sem `--locked`, e de propósito…» saiu, e no lugar ficou a razão do passo novo e o conserto sem rede;
  - antes do primeiro alvo: `cargo +nightly metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1 > /dev/null`;
  - no fim: `actions/upload-artifact@v4`, `if: failure()`, `name: fuzz-artifacts`, `path: fuzz/artifacts/`, `if-no-files-found: ignore`. **Na rodada 1 virou `warn`**: o `ignore` calava o sinal de um `path:` errado (ver «Rodada 1 de conserto», item 5).
  - O gatilho (`on:`) não foi tocado.
- Guardas em `empacotamento.rs`:
  - `o_lock_do_fuzz_lista_o_que_o_seele_proto_declara` compara os nomes de `[dependencies]` de `crates/seele-proto/Cargo.toml` com a lista do pacote `seele-proto` em `fuzz/Cargo.lock` (igualdade, nos dois sentidos);
  - `o_ci_roda_deny_e_fuzz` passou a exigir (a) o passo `cargo +nightly metadata --locked --manifest-path fuzz/Cargo.toml` antes do primeiro `fuzz run`, e (b) o upload, lido por `passos_do_job_do_ci("fuzz-curto")`: `uses: actions/upload-artifact@v4`, um `if:` com `failure()`, `path` = `fuzz/artifacts` e posição depois do último passo `fuzz run`.

### L1-m4, o `sysctl` fora do PATH

- `install.sh`: `$(PATH="$PATH:/usr/sbin:/sbin" sysctl -in hw.optional.arm64 2>/dev/null || true)`. No **fim** e sem caminho absoluto, para o dublê dos testes continuar interceptando.
- `um_mac_apple_silicon_sem_o_sysctl_no_caminho_ainda_e_apple_silicon` (`#[cfg(all(target_os = "macos", target_arch = "aarch64"))]`): `uname` dublê (Darwin arm64), PATH `ferramentas:/usr/bin:/bin` (sem `sysctl`), e o `sysctl` de verdade, em `/usr/sbin`, responde. Exige que não diga «este Mac é Intel», que saia com 0 e que o `seeled` instalado seja o do pacote. Antes, confere que `/usr/bin/sysctl` e `/bin/sysctl` não existem e que `/usr/sbin/sysctl` existe, para o teste não passar a provar nada. `Instalador` ganhou `executar` (o fim do `rodar_contra`, agora compartilhado) e `rodar_num_mac_sem_o_sysctl_no_caminho`, este com o mesmo `cfg`.
- `a_chamada_do_sysctl_no_instalador_alcanca_o_usr_sbin_depois_do_path_da_pessoa` (toda máquina): a linha do `sysctl` no código (sem comentário) tem `/usr/sbin`, não tem `/usr/sbin/sysctl` e tem o `$PATH` antes do `/usr/sbin`.

## Evidência de TDD

Saídas guardadas no scratchpad da sessão (`.../scratchpad/ferramentas/`) e aqui os trechos.

### P51-15

- Os testes (a) e (b) exercitam `push` que **já existiam**, então passam desde que são escritos; a prova deles é só a reversão (abaixo). Isso é dito de propósito: não há vermelho de TDD para código que já estava lá.
- `relatar`, vermelho 1: `cargo test -p xtask check_api` não compilava:
  `error[E0425]: cannot find function \`relatar\` in this scope` (três vezes, `check_api.rs:1103`, `:1143`, `:1197`).
- `relatar`, vermelho 2: com um *stub* que não escreve nada e sai com sucesso, os três testes falham pelo motivo certo:
  - `o_check_api_que_reprova_…`: «o \`check-api\` achou duas falhas em \`api/\` e saiu com 0 …» (`left: ExitCode(unix_exit_status(0))`, `right: …(1)`);
  - `o_check_api_que_aprova_…`: «o resumo do \`check-api\` perdeu «check-api: toda a superfície de MOD ainda aponta para algo» … Saiu \"\"»;
  - `o_check_api_que_nao_consegue_dizer_o_resumo_…`: «… saiu com 0: o CI ficaria verde sem o log dizer o que foi conferido».
- Verde: `cargo test -p xtask --bin xtask check_api` → `34 passed; 0 failed`.

### P51-21

Sem vermelho (só comentário). Verde antes e depois: `cargo test -p seele-app --bin seele-app catalogo` → `12 passed`.

### L1-m2

- Vermelho (o `--conferir` chega a «tudo conferido» com exit 0 e sem o aviso):
  `o --conferir não avisou do assunto sem prefixo da faixa: quem publica só ficaria sabendo depois da bateria e dos builds …` (`empacotamento.rs:1026`).
- Verde: `1 passed`; depois, `cargo test -p xtask --test empacotamento` inteiro → 89 passed (na época).

### L1-m3

- Lock, vermelho: `o fuzz/Cargo.lock está atrasado em relação ao \`seele-proto\`: faltam ["serde_json"] e sobram []. O job \`fuzz curto\` roda \`cargo metadata --locked\` e reprovaria antes do primeiro alvo.` Verde com o lock novo.
- CI, vermelho (antes de editar o `ci.yml`): `o ci.yml não confere o \`fuzz/Cargo.lock\` com \`cargo +nightly metadata --locked --manifest-path fuzz/Cargo.toml\` …` (`empacotamento.rs:3409`). Verde depois de editar.
- Fora do teste, o passo do CI com o lock velho: `CARGO_NET_OFFLINE=true cargo +nightly metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1` → `error: cannot update the lock file … because --locked was passed to prevent this`, exit 101. Com o lock novo → exit 0, e o sha1 do arquivo não muda.

### L1-m4

Vermelho (os dois, com a linha de hoje):
- `a chamada do \`sysctl\` não alcança /usr/sbin, onde ele mora no macOS: com um PATH que não o tem, o comando não é achado, o \`|| true\` engole o erro e um Mac Apple Silicon recebe «este Mac é Intel»: if [ "$(sysctl -in hw.optional.arm64 2>/dev/null || true)" != 1 ]; then`
- `o install.sh chamou de Intel um Mac Apple Silicon cujo PATH não tem o \`sysctl\`: o comando não foi achado, o erro foi engolido e o vazio virou a resposta de um Intel: … Compile do código-fonte: …`

Verde com o conserto: `2 passed`.

## Provas por reversão

Em cada uma, o conserto foi desfeito, o teste rodou, e o arquivo foi restaurado de uma cópia e conferido por `shasum` (igual ao de antes da reversão).

| Reversão | Teste que ficou vermelho | Frase |
|---|---|---|
| apagar o `push` «não é json» | `um_json_de_api_que_nao_e_json_…` | «um \`v7.json\` que contém só \`{\` foi aprovado … Veio Ok(Resumo { versoes: 1, cobradas: 0 })» (só ele falhou: 33 passed, 1 failed) |
| apagar o `push` «não deu para ler» | `um_json_de_api_que_nao_se_abre_…` | «um \`v8.json\` que o \`read_dir\` listou e que não existe foi aprovado …» (só ele) |
| `FAILURE` para `SUCCESS` no braço `Err` do `relatar` | `o_check_api_que_reprova_…` | «achou duas falhas em \`api/\` e saiu com 0: o CI ficaria verde com a API quebrada» (só ele) |
| `if escrito.is_ok()` para `… \|\| true` | `o_check_api_que_nao_consegue_dizer_o_resumo_…` | «não conseguiu escrever o resumo e saiu com 0» (só ele) |
| tirar a chamada de `conferir_notas` | `o_conferir_avisa_dos_assuntos_…` | «o --conferir não avisou do assunto sem prefixo da faixa …» |
| `fuzz/Cargo.lock` do `HEAD` anterior (`git checkout HEAD -- fuzz/Cargo.lock`) | `o_lock_do_fuzz_lista_…` | «faltam ["serde_json"] e sobram []»; e o `cargo metadata --locked` do CI sai 101 |
| tirar o passo `metadata --locked` do `ci.yml` | `o_ci_roda_deny_e_fuzz` | «o ci.yml não confere o \`fuzz/Cargo.lock\` …» |
| tirar só o `--locked` do passo | `o_ci_roda_deny_e_fuzz` | a mesma |
| tirar o passo do `upload-artifact` | `o_ci_roda_deny_e_fuzz` | «o job \`fuzz-curto\` do ci.yml não guarda o que o fuzz acha …» |
| tirar o `if: failure()` do upload | `o_ci_roda_deny_e_fuzz` | «o upload do fuzz não tem \`if: failure()\` …» |
| `path: fuzz/corpus/` no upload | `o_ci_roda_deny_e_fuzz` | «o upload do fuzz não guarda \`fuzz/artifacts/\` …» |
| `sysctl -in` sem o `PATH=…` (a linha de hoje) | **os dois** do L1-m4 | as duas frases acima |
| `/usr/sbin/sysctl -in` (caminho absoluto) | só o de texto | «a chamada do \`sysctl\` usa o caminho absoluto, e o dublê dos testes deixa de interceptá-la» |
| `PATH="/usr/sbin:/sbin:$PATH"` (no começo) | só o de texto | «o /usr/sbin vem antes do \`$PATH\` da pessoa …» |

O teste comportamental, como esperado, passa nas duas últimas: o `sysctl` de verdade responde de qualquer jeito. Quem guarda a ordem é o de texto.

## Arquivos mudados

- `xtask/src/check_api.rs` (P51-15)
- `.gitattributes` (P51-21)
- `empacotar/publicar.sh` (L1-m2)
- `fuzz/Cargo.lock`, `.github/workflows/ci.yml` (L1-m3)
- `install.sh` (L1-m4)
- `xtask/tests/empacotamento.rs` (L1-m2, L1-m3, L1-m4)
- este relatório

Verificação final: `cargo fmt --all -- --check` limpo; `cargo clippy -p xtask --all-targets --all-features -- -D warnings` limpo; `cargo test -p xtask` → 50 (bin) + 4 + **92** (`empacotamento`; 96 depois da rodada 1) + 1 + 1 + 2 passed, 0 falhas. `cargo run -p xtask -- check-api` passa. Nenhum `cargo test --workspace`. O aviso `ld: duplicate -rpath` é ambiental.

## O que não rodou aqui, e por quê

- **O job `fuzz-curto` do CI.** O `ci.yml` só dispara pela aba Actions, que é do dono, e eu não disparo nada. O que se provou aqui: o YAML carrega (`yaml.safe_load` do PyYAML, 10 passos, o último é o upload) e o comando do passo novo, `cargo +nightly metadata --locked …`, rodou localmente (offline) e sai 0 com o lock novo e 101 com o velho. O `upload-artifact@v4` com `if: failure()` e o efeito de um `crash-*` de verdade só se provam num disparo; aqui ficam os guardas de texto.
- **O teste do Apple Silicon fora do Apple Silicon.** `um_mac_apple_silicon_sem_o_sysctl_no_caminho_ainda_e_apple_silicon` só compila e roda em macOS arm64. Aqui roda (esta máquina é essa) e passou. No Linux, no Windows e num Mac Intel ele não existe, e quem guarda é o teste de texto. Não pude compilar o `xtask` nessas plataformas: o `cfg` do método `rodar_num_mac_sem_o_sysctl_no_caminho` é o mesmo do teste, para não gerar `dead_code` onde ele não é usado.
- **O efeito do `install.sh`** só chega a quem usa o `curl` do README depois do push do `main`.
- **O fuzz de verdade, com o lock novo.** Rodei, por 10 s, offline: `CARGO_NET_OFFLINE=true cargo +nightly fuzz run uri fuzz/corpus/uri fuzz/sementes/uri -- -max_total_time=10 -create_missing_dirs=1` → «Done 1757207 runs in 11 second(s)», sem crash, e o `fuzz/Cargo.lock` continuou com o mesmo sha1. Depois apaguei `fuzz/target` e `fuzz/corpus` (ignorados pelo git) e conferi que o lock está idêntico ao do `1277d85`.
- Não rodei `check-runtime` nem bancadas de navegador: nada em `apps/seele-app/ui/` nem em `bancada/` mudou.

## Preocupações

1. **O aviso agora sai duas vezes numa publicação de verdade:** na fase de conferências e de novo quando o corpo do release é montado, porque o brief manda manter a classificação no fim, «como hoje». O `--conferir` o mostra uma vez. Se a repetição incomodar, o fim pode chamar `notas_das_mudancas` com uma variável que cale os avisos; não fiz porque o brief não pediu e mudaria uma função que outros testes exercitam.
2. **O comando opcional do brief escreve na pasta versionada.** `cargo +nightly fuzz run uri fuzz/sementes/uri -- -max_total_time=10` (sem a pasta de corpus na frente) faria o libFuzzer gravar o que acha em `fuzz/sementes/uri`, que `sementes_do_fuzz.rs` confere byte a byte. Usei a forma do CI, com `fuzz/corpus/uri` na frente. **Nota para corrigir o plano depois:** a forma certa é a do CI, `cargo +nightly fuzz run uri fuzz/corpus/uri fuzz/sementes/uri -- -max_total_time=10 -create_missing_dirs=1`, e o `lote-ferramentas-brief.md` (e o `lotes.md` que o originou) devem trocá-la.
3. **O guarda do lock é raso de propósito:** compara só a lista de dependências diretas de `seele-proto` com a do lock. Não confere o fecho transitivo nem o pacote `seele-fuzz`; quem confere isso inteiro é o `cargo metadata --locked` do CI. O leitor de `[dependencies]` entende uma dependência por linha; uma tabela de várias linhas viraria nomes falsos e o guarda reprovaria com «sobram» (falha alta, não silenciosa).
4. **`relatar` muda um detalhe de comportamento:** se o resumo de sucesso não puder ser escrito, sai com falha (antes, o `println!` entrava em pânico, que também não era 0). Na rodada 1 (item 4), essa falha passou a ser dita em `erros`; as falhas de `api/` que não puderem ser escritas continuam ignoradas, porque o código já é de falha.
5. **(Resolvida na rodada 1, item 2.)** Antes, o `conferir_notas` herdava a tolerância de antes: um `git log` ou um `git tag` que falhasse passava calado. Agora as conferências param dizendo, e o fim do script avisa.
6. **O scratchpad da sessão é compartilhado** entre os lotes: um `trocar.py` na raiz dele foi reescrito por outro agente enquanto eu trabalhava. Passei a usar um subdiretório só meu (`ferramentas/`). Nada do repositório foi afetado.
7. **Para a v6:** nada. Nenhuma promessa de API nova, nenhuma frase em `api/README.md`.

## Rodada 1 de conserto

A revisão aprovou o lote, sem crítico nem importante, e apontou seis menores. Todos foram consertados no mesmo branch (`0.16/lotes-ferramentas`), um commit por item, com o teste antes do conserto quando o conserto muda código. As reversões restauraram o arquivo de uma cópia e conferiram o `shasum` depois de cada uma.

| Item | Commit | Arquivos |
|---|---|---|
| 1. o teste do aviso casava com o próprio dado | `c9cf97f` | `xtask/tests/empacotamento.rs` |
| 2. o git que falha em `conferir_notas` | `731164b` | `empacotar/publicar.sh`, `xtask/tests/empacotamento.rs` |
| 4. o `relatar` que saía com falha calada | `5f4e1bb` | `xtask/src/check_api.rs` |
| 6. os guardas do `ci.yml` que aceitavam a forma errada | `dfef595` | `xtask/tests/empacotamento.rs` |
| 5. `if-no-files-found: warn` | `dc93fef` | `.github/workflows/ci.yml`, `xtask/tests/empacotamento.rs` |
| 3. os comentários com o histórico medido | `54b5820` | `.github/workflows/ci.yml`, `xtask/tests/empacotamento.rs` |

### 1. O teste do L1-m2 casava com o próprio dado de entrada

- **O que mudou.** O commit da bancada se chamava «uma mudança que não diz pelo prefixo o que é», e a asserção procurava «não diz pelo prefixo»: se o script ecoasse o assunto, o teste passaria sem o aviso. O assunto virou «arruma a fila do servidor», e a asserção exige o texto do aviso de verdade: «1 assunto desta faixa não diz pelo prefixo se muda o produto» (um assunto sem classe; o outro commit tem prefixo com escopo desconhecido).
- **Teste:** `o_conferir_avisa_dos_assuntos_sem_classe_sem_chegar_a_bateria`. Comando: `cargo test -p xtask --test empacotamento o_conferir_avisa_dos_assuntos` → `1 passed`.
- **Reversão:** tirar a chamada de `conferir_notas` das conferências. Saída: `o --conferir não avisou do assunto sem prefixo da faixa: quem publica só ficaria sabendo depois da bateria e dos builds …` (`1 failed`). Restaurado, `shasum` `597eb989…` igual ao de antes.

### 2. `conferir_notas` não falhava alto quando o git falhava

- **O que mudou (`publicar.sh`).**
  - `faixa_da_versao` lê o `git tag --list` numa substituição de comando com o status testado, zera `ANTERIOR` e `FAIXA` e volta 1 se o git falhou. Antes, o status se perdia no pipe para `tag_anterior`, e o `--conferir` dizia «é a primeira versão publicada».
  - `conferir_notas` morre (`morrer`) se `faixa_da_versao` falha («não consegui listar as tags do repositório») e se o `git log` da faixa falha («não consegui ler os commits de <faixa>»). Antes, o status do pipe era o do `notas_das_mudancas`, e o passo dizia «classificadas» sem ter lido um commit.
  - Uma função nova, `resumir_assuntos`, passa os assuntos já lidos a `notas_das_mudancas`, com o `printf '%s\n'` e só quando há o que ler: `$(…)` tira a quebra do fim, e uma faixa sem commits tem de ser entrada vazia, e não uma linha vazia, que contaria como um assunto sem classe.
  - No fim, onde o corpo é montado, só um `aviso`: «não consegui listar as tags de novo ao montar o corpo do release» (o corpo, nesse caso, diz «primeira versão», o que o aviso adverte) e «não consegui ler os commits de <faixa> ao montar o corpo do release», caso em que o resumo não é gerado (a frase «nenhuma mudança de produto» que o `notas_das_mudancas` diria a uma entrada vazia seria falsa).
- **Provado com a bancada.** Deu: `Bancada::git_que_falha(falha, deixa_passar)` põe na frente do PATH do script um `git` que repassa tudo ao de verdade, menos as chamadas que levam o argumento dado (`--list` só pega o `git tag --list`; `--format=%s` só pega o `git log` do resumo, e não o da lista de commits), e que a partir da N-ésima chamada sai com 128. A configuração fica em arquivos ao lado do dublê, e `rodar` não mudou de assinatura.
- **Testes novos (4):**
  - `o_conferir_para_dizendo_quando_o_git_nao_lista_as_tags`;
  - `o_conferir_para_dizendo_quando_o_git_nao_le_os_commits_da_faixa`;
  - `o_corpo_do_release_avisa_quando_o_git_nao_lista_as_tags_na_hora_de_montar` (o dublê deixa passar a 1ª chamada, a das conferências, e falha a 2ª);
  - `o_corpo_do_release_avisa_quando_o_git_nao_le_os_commits_na_hora_de_montar` (idem; confere também que o corpo chegou ao dublê do GitHub, «As mudan…», e não afirma «nenhuma mudan…»; os dois pedaços são ASCII porque o `json.dump` do script escapa o resto).
- **Vermelho (antes do conserto), os quatro.** Com o `git tag` falhando, o `--conferir` saía 0 e imprimia «v1.2.3 é a primeira versão publicada: não há mudanças a classificar … tudo conferido» (`assertion left != right failed: o git não listou as tags e o --conferir passou`). Os avisos do fim não existiam (`o git não listou as tags ao montar o corpo do release, e o script não disse …`; `… não leu os commits ao montar …`). Comando: `cargo test -p xtask --test empacotamento git_nao` → `4 failed`. Verde depois: `4 passed`.
- **Efeito num guarda de texto.** `o_corpo_do_release_lista_os_commits_da_mesma_faixa_que_resume` procurava o pipe `git … log … "$FAIXA" | notas_das_mudancas`, e casava também com a linha de `conferir_notas`, o que o tornava mais fraco do que dizia. Passou a exigir exatamente duas leituras `log --no-merges --format='%s' "$FAIXA")`, cada uma em `$(…)` e entregue ao `resumir_assuntos`.
- **Reversões (em cada uma, só os testes que a cobrem ficaram vermelhos):**

| Reversão | Vermelhos |
|---|---|
| `faixa_da_versao` volta a engolir o status do `git tag` | `…_nao_lista_as_tags` (conferir) e `…_nao_lista_as_tags_na_hora_de_montar` |
| `conferir_notas` volta a ler o `git log` num pipe | `…_nao_le_os_commits_da_faixa` |
| o fim volta a chamar `faixa_da_versao` sem testar o status | `…_nao_lista_as_tags_na_hora_de_montar` |
| o fim trata a falha do `git log` como entrada vazia | `…_nao_le_os_commits_na_hora_de_montar` («a seção de mudanças sairia dizendo que não há nenhuma») |
| o corpo resume `"$ANTERIOR"` em vez de `"$FAIXA"` | `o_corpo_do_release_lista_os_commits_da_mesma_faixa_que_resume` |

  Depois de cada uma, `publicar.sh` restaurado e `shasum` `eff1205f…` igual. Verde final: `cargo test -p xtask --test empacotamento` → `96 passed`.

### 3. Os comentários contavam uma história não medida

- **O que mudou.** O `ci.yml` («ficou sem o serde_json … por um mês, com o job verde») e o `o_lock_do_fuzz_lista_o_que_o_seele_proto_declara` («e o job passava: o defeito era invisível») foram reescritos com o histórico medido, por `git log -S` e pelas datas dos commits:
  - o `serde_json` entrou em `seele-proto` em `f4e3183` (05/09);
  - o `fuzz/Cargo.lock` ficou como estava desde `8df5da6` (09/08) até o `1277d85` (08/10), em que um `cargo fuzz build` o reescreveu (segundo a mensagem do commit da F1);
  - o job `fuzz-curto` nasceu em `62e3f7e` (05/10), um mês depois, já sem `--locked` e já dizendo no comentário que o lock estava atrás;
  - **não se mediu** se o job rodou, nem com que resultado, entre 05/10 e 08/10, e os comentários dizem isso em vez de afirmar que passava verde.
- **Teste:** só comentário. `cargo test -p xtask --test empacotamento fuzz` → `2 passed` (o lock e o ci); o YAML carrega (`yaml.safe_load`, 10 passos). Sem reversão: não há o que reverter num comentário. Nenhum outro lugar do repositório que o lote tocou (`ci.yml`, `empacotamento.rs`, `xtask/src`, este relatório) mantém a frase antiga (`grep` por «por um mês», «com o job verde», «invisível»).

### 4. `relatar` saía com falha calada

- **O que mudou (`check_api.rs`).** Se o resumo de sucesso não se escreve, `relatar` escreve em `erros` `check-api: não deu para escrever o resumo ({erro})` e sai com `FAILURE`. Se nem o erro se escreve, o código de falha é o que sobra, e a doc de `relatar` diz isso.
- **Teste:** `o_check_api_que_nao_consegue_dizer_o_resumo_nao_sai_com_sucesso` passou a exigir `erros` que comece por `check-api: não deu para escrever o resumo` e traga o motivo («cano quebrado provocado pelo teste»).
- **Vermelho (antes do conserto):** `o check-api saiu com falha por não conseguir escrever o resumo e não disse isso nem o porquê no erro … Saiu ""`. Verde depois: `cargo test -p xtask --bin xtask check_api` → `34 passed`.
- **Reversão:** voltar o braço `Err` ao silêncio (sem o `writeln!`) → o mesmo teste volta ao vermelho com a mesma frase, e só ele (`33 passed, 1 failed`). `shasum` `bb5c848d…` igual depois.

### 5. `if-no-files-found: ignore` calava o sinal de caminho errado

- **O que mudou.** `if-no-files-found: warn` no upload do `fuzz-curto`, e o guarda `o_ci_roda_deny_e_fuzz` passou a exigir a linha exata `if-no-files-found: warn`. Um vermelho que não é crash ganha uma anotação de «nenhum arquivo achado», que é verdade e não custa nada; um `path:` errado deixa sinal.
- **Vermelho (guarda novo contra o `ignore` que estava lá):** `o upload do fuzz não tem \`if-no-files-found: warn\`: com \`ignore\` (ou \`error\`) um \`path:\` errado não deixaria sinal nenhum … O passo: […, "if-no-files-found: ignore"]`. Verde depois da troca no `ci.yml`; o YAML carrega e o último passo traz `'if-no-files-found': 'warn'`. A reversão é o próprio estado anterior (`ignore`), que reprova.

### 6. Os guardas do `ci.yml` aceitavam formas erradas

- **O que mudou (`o_ci_roda_deny_e_fuzz`).**
  - A condição do upload é lida inteira, normalizada (tira o `${{ }}` que pode envolvê-la) e comparada com `failure()` por igualdade. `if: failure()` e `if: ${{ failure() }}` passam; `${{ !failure() }}` e `failure() && false`, que contêm «failure()», reprovam.
  - O passo do `cargo +nightly metadata --locked --manifest-path fuzz/Cargo.toml` é procurado nos passos do job `fuzz-curto` (`passos_do_job_do_ci`), e não nos `run:` do `ci.yml` inteiro, e a posição dele é comparada com a do primeiro `fuzz run` do mesmo job. Helpers novos: `comando_do_passo` e `condicao_do_passo`.
- **A fraqueza, mostrada antes de consertar** (guarda velho contra o `ci.yml` com a forma errada, `1 passed` nos dois): `if: ${{ !failure() }}` passava; o passo do `--locked` no job `deny`, fora do `fuzz-curto`, passava.
- **Reversões contra o guarda novo (cada uma restaura o `ci.yml` de uma cópia, `shasum` `c28c31dc…` igual depois):**

| Forma errada posta no `ci.yml` | Resultado |
|---|---|
| `if: ${{ !failure() }}` | vermelho: `left: Some("!failure()")`, `right: Some("failure()")` |
| `if: failure() && false` | vermelho: `left: Some("failure() && false")` |
| o passo do `--locked` no job `deny`, fora do `fuzz-curto` | vermelho: «o job `fuzz-curto` do ci.yml não confere o `fuzz/Cargo.lock` …» |
| o passo do `--locked` depois dos alvos | vermelho: «… confere o lock do fuzz depois de rodar o primeiro alvo …» |
| `if: ${{ failure() }}` (a mesma condição, escrita de outro jeito) | **verde**, como deve |

  Os vermelhos de antes (tirar o passo, tirar só o `--locked`, tirar o upload, tirar o `if`, trocar o `path:`) continuam valendo: o guarda lê pelos mesmos passos.

### Verificação desta rodada

`cargo fmt --all -- --check` limpo; `cargo clippy -p xtask --all-targets --all-features -- -D warnings` limpo; `cargo test -p xtask` → 50 (bin; `check_api`: 34) + 4 + **96** (`empacotamento`) + 1 + 1 + 2 passed, 0 falhas; `cargo test -p seele-app --bin seele-app catalogo` → 12 passed. O `fuzz/Cargo.lock` continua idêntico ao do `1277d85`. Árvore limpa antes do commit deste relatório.

### O que continua não rodando aqui

Igual à primeira rodada: o job `fuzz-curto` no Actions (o `if: failure()`, o `warn` e o efeito de um `crash-*` de verdade só se provam num disparo do dono; aqui estão os guardas de texto, que agora leem a forma inteira) e o teste do Apple Silicon fora do arm64. O git que falha só foi encenado com um dublê na bancada (`git_que_falha`), e não com um repositório de verdade quebrado (sem permissão, por exemplo): um repositório sem permissão derrubaria também `conferir_arvore` antes de `conferir_notas`, e o dublê é o único jeito de fazer falhar só o `git tag --list` ou só o `git log` da classificação.
