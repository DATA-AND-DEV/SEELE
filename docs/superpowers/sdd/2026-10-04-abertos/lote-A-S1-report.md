# Lote A-S1 — relatório

Base `3685a64`, branch `conserto/abertos-da-1.0`. HEAD depois do lote: `90e9a27`. Sete commits locais, nenhum
empurrado. Nenhum assunto contém uma das catorze cadeias do G1 (conferido com as linhas entre `CADEIAS` da receita do
plano 1B contra `git log --format=%s 3685a64..HEAD`). Todos terminam com
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

| Commit | O quê |
|---|---|
| `545b7d7` | (1) o módulo puro `seele_core::anexo_no_disco`, com o teste vermelho antes |
| `60575f7` | (2) a gravação com `create_new`: `receive_attachment` recebe o arquivo aberto, `download_attachment` cria com `create_new`, o motor cria antes de pedir |
| `bc734f4` | (3) enlace com pasta + nome, FFI, casca, tela, frases, e o guarda da tela reescrito (ver «Desvios») |
| `2e30417` | (4) as provas de ponta a ponta em `anexos.rs`, pela ponte |
| `773a164` | (5) os comentários de `attachment.rs` (e de `control.rs`, ver «Desvios») |
| `cdb4867` | testes de unidade de `destino_de` na ponte (guardas que o (3) trouxe e que nenhum teste provava) |
| `90e9a27` | o caso da extensão que sozinha ocupa o limite, com teste e o comentário corrigido |

## O que foi feito, por item

1. **`crates/seele-core/src/anexo_no_disco.rs` (novo) e `pub mod anexo_no_disco;` em `lib.rs`.**
   - `nome_seguro(&str) -> Result<&str, NomeRecusado>`, no nível do texto (nunca `Path::components` nem
     `file_name`). Recusa: vazio após `trim`; só pontos (`.`, `..`, `...`); `/` e `\`; `:`; `< > " | ? *`;
     `char::is_control` (NUL incluso); os caracteres de formatação por lista explícita (U+061C, U+200B–U+200F,
     U+202A–U+202E, U+2060–U+2064, U+2066–U+2069, U+FEFF); os reservados do Windows pelo radical antes do primeiro
     ponto, sem caixa (CON, PRN, AUX, NUL, COM1–9, LPT1–9, COM¹²³, LPT¹²³); ponto ou espaço no fim; mais de
     `MAX_FILE_NAME_LEN` (255, de `seele_proto::control`).
   - `NomeRecusado` (dez variantes, `thiserror`) diz qual regra pegou — vai para o log, não para a tela.
   - `abrir_sem_sobrescrever(pasta, nome) -> io::Result<(File, PathBuf)>`: `OpenOptions::new().write(true)
     .create_new(true)`, de «foto.png» a «foto (99).png» (`TENTATIVAS = 99`); `AlreadyExists` passa ao próximo,
     outro erro volta. Confere o nome de novo com `nome_seguro` (última porta antes do disco) e confere cada
     candidato.
   - `nome_ao_lado(nome, vez)`: extensão = depois do último ponto que não é o primeiro caractere («.bashrc» →
     «.bashrc (2)»); o radical é cortado numa fronteira de caractere para caber em 255 bytes; se extensão + sufixo
     já ocupam o limite, o nome todo vira radical.
   - `para_mostrar(alegado)`: escreve controle e formatação como `\u{XXXX}`, para a frase de recusa poder citar o
     nome sem que um U+202E inverta a própria frase.
2. **`client.rs`.** `Transfers::receive_attachment(attachment, arquivo: std::fs::File, caminho: &Path, wait,
   progress)`: recebe o arquivo já criado; o miolo (`receber_no_arquivo`) foi separado para que **toda** falha
   (prazo, cabeçalho, disco, hash) passe por um ponto só na volta, que apaga **o arquivo que esta chamada recebeu** e
   nada mais. `quarantine` e o retorno usam o caminho real. `Client::download_attachment(…, destination, …)` cria
   com `create_new` **antes** do pedido (um caminho que existe é recusado) e apaga o que criou se o pedido falhar.
   Não sobrou `File::create` em caminho nenhum do anexo.
3. **`enlace.rs`.** `Enlace::salvar_anexo(anexo, pasta, nome)` e `Comando::SalvarAnexo { anexo, pasta, nome }`.
   O motor abre com `abrir_sem_sobrescrever` num `spawn_blocking`, **antes** do `fetch_attachment` (um nome
   recusado ou tomado 99 vezes não pede byte nenhum ao servidor); falha ao abrir vira `NaoSalvou`;
   `Transferencia::Salvo` leva o caminho real.
4. **seele-ffi.** `Connection::save_attachment(attachment, folder: String)` sem nome. O nome é lido do histórico em
   `run_command` (`file_name_of`, em todas as linhas, como `declared_type_of`) e passa por `destino_de`, que
   devolve `(pasta, nome)` ou `SaveRefused { reason, claimed, folder }`. `Transfer::NotSaved` ganhou
   `reason: NotSavedReason` (`NomeRecusado`, `AnexoDesconhecido`, `SemPasta`, `Falhou`). Uma recusa vira
   `Event::TransferChanged { NotSaved { reason } }` sem nada pedido ao servidor; a falha do motor vira `Falhou`.
   Pasta vazia **ou relativa** → `SemPasta`. Novo `Connection::save_destination(attachment, folder) ->
   Result<SaveDestination, SaveRefused>` (mesmo `destino_de`), só para a frase. Tipos novos exportados:
   `NotSavedReason`, `SaveDestination { folder, file_name, beside }`, `SaveRefused`.
5. **`apps/seele-app/src/main.rs`.** `pasta_dos_anexos(&AppHandle) -> String` (`download_dir()` com recuo para
   `home_dir()`, sem crate novo; caminho não-UTF-8 vira vazio → `SemPasta`). `salvar_anexo(app, session, anexo)`
   sem `destino`. Comando novo `destino_do_anexo(app, session, anexo) -> Result<SaveDestination, SaveRefused>`,
   registrado no `generate_handler!` (sem sessão → `AnexoDesconhecido`). `pasta_de_downloads` usa o mesmo
   `pasta_dos_anexos`, e o comentário desatualizado sobre «seletor nativo que custaria um crate novo» passa a dizer
   que o `tauri-plugin-dialog` já está na árvore (é o de `escolher_arquivo`) e que o destino é decisão do Rust.
6. **Tela.** `salvarAnexo(anexo)` é `async`, chama `invoke("destino_do_anexo", { anexo })` antes de
   `abrirConfirmacao`, não monta caminho nenhum, e manda `invoke("salvar_anexo", { anexo })`. A recusa vai para
   `abrirRecusa` com `fraseDeNaoSalvo(recusa)`. O botão deixou de carregar `data-anexo-nome`. `transferenciaAndou`
   escreve `fraseDeNaoSalvo(transfer.reason)` no `NotSaved`; o `Saved` continua `ARQUIVO SALVO — <caminho real>`.
   Em `frases.js`: dicionário novo `NAO_SALVOS` (quatro frases, medidas pelas réguas), `fraseDeSalvar(onde)` (a
   confirmação da lista, seguida das duas linhas de quarentena e antivírus que já existiam) e
   `fraseDeNaoSalvo(recusa)` (cita o nome e a pasta quando a recusa os traz).
7. **Provas de ponta a ponta** (`crates/seele-conformance/tests/anexos.rs`, todas com `let _vaga = vaga::minha();`
   na primeira linha, pela `seele_ffi::Connection`, com o nome vindo do histórico):
   - `um_nome_com_caminho_nao_grava_fora_da_pasta`: o remetente sobe «../escapou.txt», quem recebe salva em
     `casa/pasta`; `casa/escapou.txt` não existe, a pasta fica vazia, e chega `NotSaved { reason: NomeRecusado }`.
   - `um_nome_repetido_nao_substitui_o_que_ja_estava_la`: `pasta/foto.png` = «original» fica intacto, chega
     `Saved { path: pasta/foto (2).png }`, e `foto (2).png` tem os bytes.
   - `um_anexo_que_nao_fecha_com_o_hash_nao_apaga_o_que_ja_estava_la` (a variante): o blob é estragado no disco do
     servidor com o mesmo tamanho; chega `NotSaved { Falhou }`, `foto.png` continua «original» e `foto (2).png` não
     fica.
   - Também: `baixar_num_caminho_que_ja_existe_nao_toca_no_que_estava_la` (a porta da conformidade com
     `create_new`), e no teste do expirado uma asserção de que o pedido que não veio não deixa arquivo vazio.
   - Caminhos novos onde o teste antigo baixava por cima do arquivo de origem (`panorama.png` →
     `panorama-salvo.png`; os de :181, :306 e :540 já eram novos).
   - **Guarda da tela** (`apps/seele-app/tests/frontend.rs`): `saving_never_writes_to_a_path_this_window_cannot_name`
     reescrito — `salvarAnexo` sem `${pastaDeDestino}` e sem `destino` (tirado `destino_do_anexo`), com
     `invoke("destino_do_anexo", { anexo })` antes de `abrirConfirmacao(`, `invoke("salvar_anexo", { anexo })`,
     `abrirRecusa(`; e, do lado do Rust, `command_parameters(main.rs)` de `salvar_anexo` e `destino_do_anexo` é
     exatamente `["anexo"]`. Guarda novo `um_anexo_que_nao_foi_salvo_diz_por_que` (toda variante de
     `NotSavedReason` tem frase em `NAO_SALVOS`; a de `NomeRecusado` não manda «tente de novo» e fala em «outro
     nome»; o ramo `"NotSaved"` de `transferenciaAndou` usa `fraseDeNaoSalvo(transfer.reason)`).
     `a_preview_is_not_an_open_and_is_not_a_save` continua verde.
8. **Comentários.** `attachment.rs` (o doc de `file_name` e o teste com `../../etc/passwd`) passam a dizer que no
   servidor o nome não chega ao disco, que em quem recebe ele vira arquivo ao salvar, e que a regra mora em
   `seele_core::anexo_no_disco`; o fio continua levando o nome como veio. Os doc-comments «where the receiver chose»
   de ffi, enlace e client.rs foram reescritos nos commits (2) e (3), junto com o código que eles descrevem; o de
   `preview_attachment` («in a place they picked») foi ajustado no (5).

## RED / GREEN

- (1) Com os corpos-stub (`nome_seguro` devolvendo `Ok`, `para_mostrar` identidade, `abrir_sem_sobrescrever` com
  `create(true).truncate(true)`): **4 vermelhos** — «"../.zshrc" passou como nome de arquivo…», «a frase que recusa
  o nome citaria o U+202E…», «a colisão gravou no mesmo nome», «três colisões seguidas não deram « (2)», « (3)» e
  « (4)»» (`left: ["foto.png", "foto.png", "foto.png", "foto.png"]`). Implementado: 4 verdes.
- (2) `baixar_num_caminho_que_ja_existe_nao_toca_no_que_estava_la` escrito antes: vermelho contra o código de
  `3685a64` («baixar por cima de um arquivo que já existe deu certo, e só dá certo substituindo o que estava lá»).
  Depois do conserto: 11 de 11 em `anexos`, com o de `panorama` passado a um caminho novo (ele ficou vermelho com
  `File exists (os error 17)`, o sinal esperado de que o teste baixava por cima do próprio arquivo de origem).
- (3) Guardas da tela reescritos antes da tela: **5 vermelhos** contra a tela antiga
  (`saving_never…`: «`salvarAnexo` voltou a montar um destino na janela…»; `saving_says_out_loud…`: «a confirmação
  de salvar não é a frase de `ui/frases.js`…»; `um_anexo_que_nao_foi_salvo_diz_por_que`: «a ponte não diz mais por
  que um anexo não foi salvo»; as duas réguas: «`NAO_SALVOS` is gone from ui/frases.js»). Depois de enlace, FFI,
  casca, tela e frases: `frontend` 258 de 258.
- (4) e os testes de `destino_de`/borda (commits 6 e 7) foram escritos depois do código que guardam: provados por
  reversão abaixo.

Estado final (HEAD `90e9a27`): `cargo test -p seele-core --lib` 407 ok; `-p seele-ffi --lib` 120 ok (1 ignorado, de
antes); `-p seele-conformance --test anexos` 14 ok; `-p seele-app --test frontend` 258 ok; `-p seele-proto --lib`
257 ok; `-p seele-app` inteiro (bin + 8 binários de teste) verde no commit (3). `cargo xtask check-api` verde.
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets --all-features -- -D warnings` limpos antes de
cada commit (só o aviso ambiental `ld: duplicate -rpath`).

## Reversões (cada uma: desfazer, ver vermelho com a frase, restaurar da cópia e `cmp` byte a byte — todas
«restaurado byte a byte»)

| # | Reversão | Vermelho |
|---|---|---|
| 1 | `nome_seguro` trocado pela regra `Path::new(nome).file_name() == nome` | `um_nome_alegado…`: «"..\\..\\AppData\\…\\x.bat" passou como nome de arquivo…». Um `rustc` à parte confirmou que a mesma regra aceita no Mac `foto\u{202E}gnp.exe`, `C:x.bat`, `a?b` e `CON`. |
| 2 | `create_new` → `File::create` em `abrir_sem_sobrescrever` | `um_nome_repetido_grava_ao_lado…` (`left: ["foto.png" ×4]`) e `um_nome_de_255_bytes…` («a colisão gravou no mesmo nome») |
| 3 | sem o corte do radical em `nome_ao_lado` | `um_nome_de_255_bytes…`: «o nome ao lado de um nome de 255 bytes não abriu: sem o corte do radical… `InvalidInput`, `LongoDemais`» |
| 4 | `download_attachment` com `tokio::fs::File::create` | `baixar_num_caminho…`: «baixar por cima de um arquivo que já existe deu certo…» |
| 5 | `receive_attachment` sem o `remove_file` da falha | `o_server_enche…`: «um anexo que não veio deixou um arquivo vazio onde ia ser gravado» |
| 6 | a tela volta a montar `${pastaDeDestino}/${onde.file_name}` e manda `{ anexo, destino }` | `saving_never…`: «`salvarAnexo` voltou a montar um destino na janela…» |
| 7 | `salvar_anexo` da casca volta a aceitar `destino: String` | `saving_never…`: «`salvar_anexo` aceita da janela mais que o anexo…» (`left: ["anexo", "destino"]`) |
| 8 | `transferenciaAndou` escreve `NAO_SALVOS.Falhou` sem ler o motivo | **a primeira versão do guarda ficou verde** (os ramos da recusa do servidor também leem `transfer.reason`). O guarda foi estreitado ao ramo `"NotSaved"` e refeito: «a tela não lê o motivo de um anexo não salvo…» |
| 9 | tira `AnexoDesconhecido` de `NAO_SALVOS` e «e não substitui nada» de `fraseDeSalvar` | «a ponte pode dizer `AnexoDesconhecido` e `NAO_SALVOS` não tem frase…» e «a confirmação não diz que um arquivo que já está lá não é substituído…» |
| b1 | a ponte pula `nome_seguro` em `destino_de` | `um_nome_com_caminho…`: `left: NotSaved { reason: Falhou }`, `right: NomeRecusado`. O arquivo **não** escapou: a segunda conferência, dentro de `abrir_sem_sobrescrever`, segurou. |
| b2 | b1 **e** o motor gravando em `pasta.join(nome)` com `File::create` (a reversão da lista) | os três de ponta a ponta: «o nome `../escapou.txt` virou caminho e o arquivo foi gravado fora da pasta escolhida», «o anexo não foi salvo ao lado…», «o anexo que não fechou com o hash levou junto o `foto.png`…» |
| c | só `create_new` → `File::create` em `abrir_sem_sobrescrever` | `um_nome_repetido…`: `Saved { path: …/pasta/foto.png }` em vez de `foto (2).png`; a variante do hash: `left: None` (o `foto.png` da pessoa foi **apagado**), `right: Some("original")` |
| d1 | `destino_de` sem `!pasta.is_absolute()` | `sem_pasta_absoluta_nada_grava`: «uma pasta vazia ou relativa foi aceita para gravar: ("Downloads", "foto.png")» |
| d2 | `claimed` sem `para_mostrar` | `um_nome_que_nao_e_so_um_nome…`: «a recusa de "foto\u{202e}gnp.exe" não diz o motivo, ou cita o nome de um jeito que a própria frase não consegue mostrar» |
| d3 | `file_name_of` só na linha 1 | `um_nome_que_e_so_um_nome_grava_na_pasta_dada`: `SaveRefused { reason: AnexoDesconhecido, … }` |
| e | `nome_ao_lado` sem o recuo da extensão enorme | `um_nome_de_255_bytes…`: «…não cabe em 255 bytes ou não é mais só um nome: 258 bytes» |

(Uma primeira tentativa de d1–d3 juntas foi descartada: a d3 mascarava as outras, e as três foram refeitas uma a uma.)

## Na tela, de verdade

Uma bancada de rascunho (fora do repositório, em scratchpad) serviu `ui/` com a ponte simulada, pelo `telas.cjs` e
o Playwright de `SEELE-MOD-PERFIS`, sem baixar nada: SALVAR num «foto.png» mostrou a confirmação da lista com
«/Users/x/Downloads» e «foto (2).png»; CONFIRMAR chamou `salvar_anexo` com `{ anexo: 11 }` e nada mais; SALVAR num
«../.zshrc» (o mock rejeita com `NomeRecusado`) abriu a recusa e não chamou `salvar_anexo`; `NotSaved/NomeRecusado`
e `Saved` com «foto (2).png» escreveram as frases certas no bloco. Nenhum erro de página. A cena `anexos` do
`telas.cjs` roda com os mesmos dois erros que já tinha em `3685a64` (MODs e qualidade da voz, conferido rodando a
árvore de `3685a64` extraída com `git archive`).

## Arquivos mudados

`crates/seele-core/src/anexo_no_disco.rs` (novo), `crates/seele-core/src/lib.rs`, `crates/seele-core/src/client.rs`,
`crates/seele-core/src/enlace.rs`, `crates/seele-ffi/src/lib.rs`, `crates/seele-ffi/src/types.rs`,
`apps/seele-app/src/main.rs`, `apps/seele-app/ui/tela-sessao.js`, `apps/seele-app/ui/frases.js`,
`crates/seele-proto/src/attachment.rs` (só comentários), `crates/seele-proto/src/control.rs` (só comentário),
`crates/seele-conformance/tests/anexos.rs`, `apps/seele-app/tests/frontend.rs`. Fio, `PROTOCOL_VERSION`,
SEELE-ENC/1, `api/*.json`, CSP e `tauri.conf.json` intocados; nenhuma dependência nova.

## O que não pôde ser compilado nem rodado aqui

- O ramo `#[cfg(target_os = "windows")]` de `quarantine` (o `Zone.Identifier`, agora com o caminho real) não
  compila neste Mac; o código dele não mudou.
- A gravação real no NTFS — nomes reservados, ponto/espaço no fim, fluxo alternativo, `create_new` diante de um nome
  curto 8.3 ou de colisão de caixa — só se exercita na bateria Windows do `publicar.sh`. Aqui a regra é provada no
  texto (o teste roda no Mac e cobre `\`, `C:`, `:Zone.Identifier`, `CON`, `nul.txt`, `COM¹.txt`).
- Os comandos Tauri `destino_do_anexo` e `salvar_anexo` não rodam de ponta a ponta num runtime do Tauri: a forma
  deles é cobrada pelo `frontend.rs`, e o que eles chamam pela FFI e pela conformidade.
- A bateria inteira do lote (cargo test --workspace, check-versao, check-runtime, todas as bancadas) não foi rodada
  por mim: só os testes pontuais, o clippy do workspace, o `check-api` e a cena `anexos`.

## Desvios da lista, e por quê

1. **A frase da recusa com o nome tem duas linhas, não três**, e diz «num nome que o Windows reserva ou com
   caracteres que disfarçam o nome, e o SEELE só grava dentro dessa pasta». Duas linhas porque `frontend.rs` diz que
   as frases compostas de `frases.js` ficam em duas «por construção» (o guarda da terceira linha). «Disfarçam o nome»
   porque a frase da lista seria falsa para o RLO e os caracteres de controle, que não gravam fora nem são
   reservados. «Dessa pasta» e não «da pasta de downloads» porque a pasta pode ser a pessoal (o recuo de
   `home_dir()`), e ela está escrita na própria frase. A confirmação ficou como a lista, com as duas linhas antigas.
2. **`SaveDestination` tem um terceiro campo, `beside`** («foto (2).png»), calculado em Rust por `nome_ao_lado`,
   a mesma função que grava: sem ele a janela teria de reescrever a regra da extensão para montar o exemplo.
3. **Um dicionário novo, `NAO_SALVOS`**, com as quatro frases do `NotSaved` (o motivo sem frase seria o produto
   sabendo e não contando: «tente de novo» para um nome que vai ser recusado de novo). `TRANSFERENCIAS.NotSaved`
   virou `NAO_SALVOS.Falhou`. Entrou em `DICIONARIOS` do `frontend.rs`, então as duas réguas o medem.
4. **O guarda da tela foi para o commit (3), não para o (4)**: o guarda antigo exigia `${pastaDeDestino}`, e a tela
   nova o deixaria vermelho no commit que a muda. Foi reescrito antes da tela (TDD), no mesmo commit.
   `saving_says_out_loud_what_this_product_does_not_promise` também mudou: ele lia «não varre vírus», «quarentena»,
   «chegou inteiro» e `destino` dentro de `salvarAnexo`; com a frase em `frases.js` ele passa a ler `fraseDeSalvar`,
   cobra que `salvarAnexo` a use, e troca o `contains("destino")` (que contradiria o guarda novo) por
   `.folder`/`.file_name`/`.beside` e «não substitui nada».
5. **`control.rs:128`** (`MAX_FILE_NAME_LEN`) repetia o mesmo «the name never reaches the filesystem»; só o
   comentário mudou, no commit (5).
6. **A regra dos reservados tira os espaços do fim do radical** antes de comparar (`CON .txt`), além da lista.
   COM0 e LPT0 **não** entraram (a lista diz 1–9); fica anotado.
7. **Testes além da lista**: o de `download_attachment` com `create_new`, a asserção do expirado, os quatro de
   `destino_de` na ponte (provam `SemPasta` com pasta relativa, `AnexoDesconhecido`, a busca em todas as linhas e a
   citação por extenso), e o caso da extensão enorme. Daí sete commits e não cinco.

## Autorrevisão

- Releitura do diff inteiro (`git diff 3685a64`). Cada frase nova de comentário e de tela foi conferida contra o
  código: «o fio continua levando o nome como veio» (`attachment.rs` só recusa vazio, NUL e tamanho); «255 bytes
  cabem nos três» (medido no APFS pelo teste de 255 bytes; ext4 é 255 bytes; NTFS é 255 unidades UTF-16, que nunca
  são mais que os bytes UTF-8); «nada é pedido ao servidor» numa recusa (o motor e a ponte recusam antes do
  `fetch_attachment`); «nada foi gravado pela metade» no `Falhou` (o arquivo criado sai em toda falha de
  `receive_attachment`).
- O guarda 8 nasceu verde contra a reversão e foi consertado — é o caso do «existir não é funcionar».
- Defesa em profundidade: o nome é conferido na ponte (para o motivo certo) e de novo em `abrir_sem_sobrescrever`
  (para nenhum chamador novo gravar um nome hostil); a reversão b1 mostra a segunda segurando sozinha.

## Preocupações

1. **Conflito de merge com o branch `mobile/ios`**, que move `apps/seele-app/src/main.rs` para `lib.rs` sem commit
   na árvore principal. Quem integrar por último resolve: as mudanças daqui em `main.rs` são `pasta_dos_anexos`,
   `salvar_anexo`, `destino_do_anexo`, `pasta_de_downloads`, o `use seele_ffi::{…}` e o `generate_handler!`.
2. **O arquivo existe, vazio ou pela metade, durante o download**, com o nome final (é criado antes do pedido para
   o `create_new` responder antes de qualquer byte). Toda falha o apaga; um processo morto no meio deixa o parcial,
   como antes deixava o `File::create`.
3. **`Falhou` continua dizendo «tente de novo»** (a frase de antes). Para um blob estragado no servidor ou para 99
   nomes tomados, tentar de novo dá no mesmo. Separar exigiria um motivo a mais no core (`NaoSalvou` não tem
   motivo hoje); não fiz, por não estar na lista.
4. **Sem sessão, `destino_do_anexo` responde `AnexoDesconhecido`** («este arquivo não está na conversa que esta
   janela carregou») — verdadeiro, mas sem motivo próprio.
5. **Comportamento novo também no Mac**: nomes antes aceitos (`CON.txt`, `a:b`, `a?b`, `x.`) passam a ser
   recusados na hora de salvar. Uma 0.15.0 continua mandando qualquer nome; o fio não mudou.
6. **COM0/LPT0 e `CONIN$`/`CONOUT$`** não estão na regra (fora da lista). O que o `create_new` faz no Windows
   diante de um desses nomes, numa versão que os trate como dispositivo, não foi medido; acrescentá-los à lista
   `RESERVADOS` custa uma linha e um caso no teste, se o controlador quiser.
7. O registro em `docs/pendencias.md`, no índice e no plano 1B fica com o lote Z, como manda a lista; este lote não
   ganha cadeia no G1.

## Rodada 1

Base `90e9a27`. HEAD depois da rodada: `ec5134d`. Três commits locais, nenhum empurrado; nenhum assunto contém uma
das catorze cadeias do G1 (conferido com as linhas entre `CADEIAS` do plano 1B contra o assunto de cada um), e todos
terminam com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

| Commit | Achados |
|---|---|
| `cd89d40` | A-S1-R1 (frase, docs, guarda novo) e A-S1-R3 (comentários das réguas) |
| `1c5432e` | A-S1-R4 (a linha do log) |
| `ec5134d` | A-S1-R2, A-S1-R6, e o doc de `Falhou` (registro do A-S1-R5) |

### O que foi feito, por achado

- **A-S1-R1 (Important) — consertado.** A segunda linha da composta de `fraseDeNaoSalvo` passa a ser:
  «`«X»` tem um caminho, um caractere ou um nome que o Windows não aceita, um caractere invisível que disfarça o nome,
  ou é comprido demais, e o SEELE só grava em `<pasta>` um nome que sirva no Windows, no Mac e no Linux. Peça a quem
  mandou que mande de novo com outro nome.» Continua em duas linhas. Em relação ao texto sugerido, entrou «ou é
  comprido demais» (a regra `LongoDemais` não é «um nome que o Windows não aceita»: 300 bytes de «é» cabem nas 255
  unidades UTF-16 do NTFS) e «três sistemas» virou «no Windows, no Mac e no Linux», para quem lê saber quais são.
  Renderizada no node com «Notas 04:10.txt», a frase sai como esperado. O doc de `NotSavedReason::NomeRecusado`
  (types.rs) e o de `AttachmentHeader::file_name` (attachment.rs) dizem o mesmo, e o primeiro cita «Notas 04:10.txt»
  e «Por quê?.pdf» como nomes comuns que caem ali. A mensagem da asserção de
  `um_nome_alegado_so_vira_nome_de_arquivo_se_for_so_um_nome` («gravaria fora da pasta, num dispositivo do Windows ou
  com a extensão disfarçada») tinha a mesma enumeração incompleta, para `a?b`, `a<b`, `x.bat.`, e foi junto.
  **Guarda novo** em `frontend.rs`, `a_recusa_do_nome_so_diz_o_que_a_regra_pode_ter_pegado`. Ele lê as variantes
  de `NomeRecusado` do `anexo_no_disco.rs` (sem comentários) e, por uma tabela regra → pedaço da frase, cobra que a
  frase diga o pedaço de cada uma. Uma variante nova sem linha na tabela reprova. Cobra também que a frase não diga
  «gravaria», que só vale para um nome com caminho, e que a composta tenha duas linhas, coisa que nenhuma régua
  media. Para ler a literal de crase, entrou um ajudante `texto_das_literais`, porque a `literals_in` lê só aspas.
- **A-S1-R2 (Minor) — consertado.** O comentário do apagar em `receive_attachment` não promete mais que o arquivo já
  está fechado e não diz mais que o Windows não apaga arquivo aberto. Agora ele diz que soltar o `tokio::fs::File` não
  espera uma escrita ainda em voo e que isso não impede o apagar: no Mac e no Linux o nome sai na hora, e no Windows
  o std abre com `FILE_SHARE_DELETE`, de modo que o arquivo sai no máximo quando a escrita solta o último handle.
- **A-S1-R3 (Minor) — consertado.** O doc de `DICIONARIOS` nomeia `fraseDeSalvar` como a exceção à regra do ato
  irreversível escrito ao lado do ato. Ela mora em `frases.js` por decisão do lote, tem quatro linhas e nenhuma régua
  a mede. O que ela diz é cobrado por `saving_says_out_loud_what_this_product_does_not_promise`, e o tamanho não é
  cobrado por ninguém. O comentário da régua das três linhas diz que agora as compostas de duas linhas são três, e
  que a de `fraseDeNaoSalvo` é contada pelo guarda novo.
- **A-S1-R4 (Minor) — consertado.** O fim do salvar foi para `fim_do_salvar(anexo, caminho, recebido)`, fora do
  `tokio::spawn`, e no `Err` escreve `warn!(%anexo, caminho, %erro, "o anexo não foi salvo")`. Tirei a parte «o
  arquivo criado foi apagado» da mensagem sugerida, porque `receive_attachment` ignora o resultado do `remove_file`
  (ver preocupações). As duas linhas de quando o arquivo não pôde ser criado ganharam `%anexo`. Teste:
  `um_anexo_que_nao_foi_salvo_deixa_o_porque_no_log` (enlace.rs), com o `Rastro` de `rastro_de_teste`.
- **A-S1-R5 (Minor) — registrado, sem mudar a frase**, como o achado manda. O doc de `NotSavedReason::Falhou`
  dizia «a gravação começou e não terminou», o que era falso quando a pasta não deixava criar o arquivo. Agora ele
  nomeia esse caso (sem permissão, pasta que não existe, os noventa e nove nomes tomados). Também diz que em várias
  causas tentar de novo dá no mesmo enquanto a tela manda tentar, e que separá-las pede um motivo no `NaoSalvou` do
  core.
- **A-S1-R6 (Minor) — consertado.** O doc do módulo `anexo_no_disco` diz que o fio recusa o que é vazio ou só
  espaço, o que tem NUL e o que passa de `MAX_FILE_NAME_LEN` bytes. O doc de `SaveRefused.folder` diz que a pasta
  volta como a casca passou, vazia ou relativa. Em `nome_ao_lado`, saiu `!radical.is_empty()`: o ramo `ponto > 0`
  nunca dá radical vazio, e com `nome` vazio os dois lados do `if` dão `("", "")`.

### RED / GREEN

- R1: o guarda novo foi escrito antes da frase. Vermelho contra `90e9a27`: «a frase que cita o nome recusado diz que
  ele gravaria fora da pasta, e isso só vale para um nome com caminho — não para «Notas 04:10.txt» nem para «Por
  quê?.pdf»». Com a frase nova: verde.
- R4: primeiro a extração pura de `fim_do_salvar`, sem o log, e o teste. Vermelho: «um anexo que não foi salvo não
  deixou no `seele.log` qual anexo foi nem por quê, e o motor era o único que sabia. Rastro: []». Com o `warn!`:
  verde.
- R2, R3, R5 e R6 só mexem em comentário e doc, fora a condição redundante (que não muda resultado nenhum). Para
  esses, a prova é a bateria pontual continuar verde.

Estado final (HEAD `ec5134d`): `cargo test -p seele-app --test frontend` 259 ok; `-p seele-core --lib` 408 ok;
`-p seele-ffi --lib` 120 ok (1 ignorado, de antes); `-p seele-conformance --test anexos` 14 ok; `-p seele-proto --lib`
257 ok; `cargo xtask check-api` verde. `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets --all-features -- -D warnings` limpos antes de cada commit, só com o aviso
ambiental `ld: duplicate -rpath`.

### Reversões (desfazer, ver vermelho, restaurar da cópia e conferir com `cmp`: todas «restaurado byte a byte»)

| # | Reversão | Vermelho |
|---|---|---|
| R1a | `frases.js` de volta ao de `90e9a27` | «a frase que cita o nome recusado diz que ele gravaria fora da pasta…» |
| R1b | a frase nova sem «ou é comprido demais» | «um nome recusado por `LongoDemais` lê uma frase que não diz «comprido demais», e o motivo que ela dá é o de outra regra» |
| R1c | uma terceira linha (`\n` antes de «Peça a quem mandou») | a asserção das duas linhas (`left: 3`, `right: 2`) |
| R1d | uma variante `RegraNova` acrescentada a `NomeRecusado` | «`nome_seguro` ganhou a regra `RegraNova`, e ninguém disse que pedaço da frase de recusa a cobre…» |
| R4 | `Err(_) => Transferencia::NaoSalvou { anexo }` sem o `warn!` | «um anexo que não foi salvo não deixou no `seele.log` qual anexo foi nem por quê… Rastro: []» |

### Arquivos mudados

`apps/seele-app/ui/frases.js`, `apps/seele-app/tests/frontend.rs`, `crates/seele-core/src/anexo_no_disco.rs`,
`crates/seele-core/src/client.rs` (só comentário), `crates/seele-core/src/enlace.rs`, `crates/seele-ffi/src/types.rs`
(só doc), `crates/seele-proto/src/attachment.rs` (só doc). Ficaram intocados o fio, `PROTOCOL_VERSION`, o
SEELE-ENC/1, `api/*.json`, a CSP e `tauri.conf.json`. Nenhuma dependência nova.

### Preocupações desta rodada

1. **O guarda do R4 prova a função, não a chamada.** `fim_do_salvar` tem uma chamada só, em `Comando::SalvarAnexo`.
   Um teste que lesse o log pelo caminho da ponte precisaria de um `Subscriber` global no binário `anexos`, e o
   `seele-conformance` não tem `tracing` nas dependências. Não acrescentei.
2. **`receive_attachment` ignora a falha do `remove_file`** (`let _ = …`, de antes). Se o apagar falhar, o arquivo
   parcial fica na pasta sem nenhuma linha no log, e duas frases passam a ser falsas: «Nada foi gravado pela metade»,
   na tela, e «O que foi criado saiu», no doc de `Falhou`. É raro (o arquivo foi criado por esta mesma chamada, e o
   apagar só falha se a pasta mudar de permissão no meio). Não consertei porque nenhum teste daqui alcança esse
   caminho sem servidor. Fica registrado para o lote que der motivo ao `NaoSalvou` (o do A-S1-R5).
3. **A composta do nome recusado ficou mais longa**: a segunda linha tem 259 caracteres fora o nome e a pasta
   (medido no node), contra 184 da frase de `90e9a27`. Nenhuma régua mede as compostas, e o guarda novo cobra a
   cobertura e as duas linhas, não o tamanho.
4. **A frase é ampla de propósito.** Ela cobre as dez regras sem dizer qual pegou, e por isso diz a quem recebe «um
   caminho, um caractere ou um nome…» e não a regra exata. Dizer a regra exata pediria que a variante de
   `NomeRecusado` atravessasse a FFI, e a lista decidiu que ela vai só para o log.
