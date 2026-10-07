# Os lotes dos abertos da 1.0

Saíram do inventário verificado (`inventario.json`): 65 pontos, um cético por fonte, e uma síntese. A ordem abaixo é a de
execução, um lote de cada vez neste worktree (a bateria inteira de um lote ocupa a porta UDP 8384).

Raiz de todo caminho relativo: /Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0.

## Lote A-S1 — S1: o Rust decide onde o anexo grava, e nada que já existe é substituído ou apagado

Ordem 1. Itens: S1. Por que juntos: É o único item bloqueante de segurança, e sai sozinho e primeiro. Ele atravessa seele-core, seele-ffi, a casca, a tela, as frases e o frontend.rs. Por isso nenhum lote que toque client.rs, enlace.rs, seele-ffi, apps/seele-app/src/main.rs, tela-sessao.js, frases.js ou frontend.rs começa antes de ele entrar. B, K1, L1 e L2 não dividem arquivo com ele e correm na mesma onda.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/anexo_no_disco.rs (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/client.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/enlace.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/types.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-sessao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/frases.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-proto/src/attachment.rs (só comentários)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/anexos.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/frontend.rs`

Ordem dos commits: (1) o módulo puro, com o teste vermelho antes; (2) a gravação com create_new; (3) FFI, casca e tela; (4) as provas de ponta a ponta e o guarda da tela; (5) os comentários.

1. crates/seele-core/src/anexo_no_disco.rs (novo, com `pub mod anexo_no_disco;` em seele-core/src/lib.rs). `pub fn nome_seguro(alegado: &str) -> Result<&str, NomeRecusado>` trabalha no nível do texto, e nunca com `Path::components` ou `file_name`: assim a regra é a mesma no Mac e no Windows, e um teste no Mac prova o caso do Windows. Recusa:
- `/` e `\`; `:` (unidade e fluxo alternativo `x:Zone.Identifier`); `< > " | ? *`;
- o que fica vazio depois do trim; `.`, `..` e nomes só de pontos;
- `char::is_control`, com o NUL incluso;
- os caracteres de formatação que disfarçam a extensão, por lista explícita, porque a std não expõe a categoria Cf e não entra crate: U+061C, U+200B–U+200F, U+202A–U+202E, U+2060–U+2064, U+2066–U+2069 e U+FEFF;
- os nomes reservados do Windows (o radical antes do primeiro ponto, sem caixa, com qualquer extensão): CON, PRN, AUX, NUL, COM1–COM9, LPT1–LPT9, e também COM¹ COM² COM³ LPT¹ LPT² LPT³;
- ponto ou espaço no fim;
- mais de 255 bytes (`MAX_FILE_NAME_LEN`, seele-proto/src/control.rs:132).
`NomeRecusado` diz qual regra pegou, para o seele.log, e não para a tela.
`pub fn abrir_sem_sobrescrever(pasta: &Path, nome: &str) -> io::Result<(File, PathBuf)>` usa `OpenOptions::new().write(true).create_new(true)` e tenta «foto.png», «foto (2).png»… até «foto (99).png». A extensão é o que vem depois do último ponto que não é o primeiro caractere, então «.bashrc» vira «.bashrc (2)». O sufixo corta o radical numa fronteira de caractere, para o nome final caber em 255 bytes. `AlreadyExists` passa ao próximo nome, e qualquer outro erro volta.
Teste, escrito antes: `um_nome_alegado_so_vira_nome_de_arquivo_se_for_so_um_nome`.
- Tem de dar Err: «../.zshrc», «..\..\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\x.bat», «/etc/passwd», «C:\x.bat», «C:x.bat», «a.txt:Zone.Identifier», «..», «.», «CON», «nul.txt», «x.bat.», «x.bat », «a\u{0}b», «a\nb», «a?b», «a<b» e «foto\u{202E}gnp.exe».
- Tem de dar Ok: «foto.png», «relatório final.pdf» e «.bashrc».
- Uma colisão com um nome de 255 bytes devolve um nome de até 255 bytes, e três colisões seguidas dão « (2)», « (3)» e « (4)».
Reversão: trocar a regra por `Path::new(nome).file_name()` deixa passar a linha com `..\` e o RLO, e o teste fica vermelho.

2. crates/seele-core/src/client.rs: `Transfers::receive_attachment` deixa de usar `tokio::fs::File::create` (:2351). Ele recebe o arquivo que `abrir_sem_sobrescrever` já abriu, com o caminho real, ou abre ele mesmo com `create_new`. O `remove_file` do hash errado (:2373) só apaga o que esta chamada criou. `quarantine` e o retorno usam o caminho real. `Client::download_attachment(…, destination, …)` continua existindo para a conformidade, agora com `create_new`: os testes de anexos.rs usam caminhos novos (:181, :306, :540, :664).

3. crates/seele-core/src/enlace.rs: `Enlace::salvar_anexo` e `Comando::SalvarAnexo` passam a levar a pasta e o nome já validado. O motor abre com `abrir_sem_sobrescrever` e devolve o caminho real no evento de salvo.

4. crates/seele-ffi: `Connection::save_attachment(attachment, pasta: String)` deixa de receber nome. O nome é lido do histórico local, como `declared_type_of` (lib.rs:4888-4899) já faz, e nunca vem da janela. Um anexo fora do histórico carregado é recusado com motivo próprio, sem cair num nome de fora. `Transfer::NotSaved` (types.rs) ganha `reason`: `NomeRecusado`, `AnexoDesconhecido`, `SemPasta` ou `Falhou`. O salvo leva o caminho real.

5. apps/seele-app/src/main.rs:
- `salvar_anexo(anexo)` deixa de receber `destino`. Hoje :3011-3017 aceita qualquer caminho absoluto da janela, que é o agravante.
- A pasta é calculada em Rust: `app.path().download_dir()`, com recuo para `home_dir()`, sem crate novo. Sem nenhuma das duas, a resposta é `SemPasta`.
- Comando novo, `destino_do_anexo(anexo) -> Result<{pasta, nome}, Recusa>`, que serve só para a frase. Registrá-lo no `generate_handler!`. Na hora de gravar, o Rust deriva tudo de novo.
- O comentário de :3060-3062 («sem um seletor de arquivos nativo — que custaria um crate novo») está desatualizado, porque o `tauri-plugin-dialog` já está na árvore (:2931, `pick_file`). Ele passa a dizer que o destino é a pasta de downloads, escolhida em Rust.

6. apps/seele-app/ui/tela-sessao.js, em `salvarAnexo`:
- chama `invoke('destino_do_anexo', { anexo })` antes de `abrirConfirmacao`;
- não monta caminho nenhum: sai o `${pastaDeDestino}/${nome}` de :3353;
- manda `invoke('salvar_anexo', { anexo })`, sem destino.
Frases em ui/frases.js:
- confirmação: «Grava «foto.png» em /Users/x/Downloads.\nSe já houver um arquivo com esse nome lá, o SEELE grava ao lado, como «foto (2).png», e não substitui nada.\n», seguida das linhas de quarentena e antivírus que já existem;
- recusa: «O NOME QUE VEIO COM ESTE ARQUIVO NÃO É SÓ UM NOME.\n«../.zshrc» gravaria fora de /Users/x/Downloads, ou num nome que o Windows reserva, e o SEELE só grava dentro da pasta de downloads.\nNada foi gravado. Peça a quem mandou que mande de novo com outro nome.».
Depois de salvar continua `ARQUIVO SALVO — <caminho real>`, que já mostra o sufixo.

7. Provas de ponta a ponta.
- Em crates/seele-conformance/tests/anexos.rs, `um_nome_com_caminho_nao_grava_fora_da_pasta` passa por `seele_ffi::Connection::save_attachment(id, pasta)`, para o nome vir do histórico como em produção. O remetente sobe «../escapou.txt», e quem recebe salva em `casa/pasta`. O teste afirma que `casa/escapou.txt` não existe e que chegou `NotSaved { reason: NomeRecusado }`. Reverter para `pasta.join(nome)` faz o arquivo aparecer, e o teste fica vermelho.
- `um_nome_repetido_nao_substitui_o_que_ja_estava_la`: `pasta/foto.png` com «original» continua intacto, e o novo vai para `foto (2).png`. Variante: um anexo com hash corrompido não apaga o `foto.png` que já existia. Reverter `create_new` para `File::create` sobrescreve o original, e o teste fica vermelho.
- Em apps/seele-app/tests/frontend.rs, reescrever `saving_never_writes_to_a_path_this_window_cannot_name` (:8795), que hoje exige `${pastaDeDestino}`. `salvarAnexo` não pode conter `${pastaDeDestino}/` nem `destino`, chama `destino_do_anexo` antes de `abrirConfirmacao`, e `salvar_anexo` vai sem destino. Reverter a tela para montar o caminho reprova o guarda. `a_preview_is_not_an_open_and_is_not_a_save` (:7997) continua verde.

8. Comentários. attachment.rs:75-80 e :297-301 (o teste com `../../etc/passwd` diz «the name never reaches the filesystem», o que é falso no recebedor) e os doc-comments «where the receiver chose» (ffi:1458, enlace:1943, client.rs:2304) passam a dizer que a regra mora em `seele_core::anexo_no_disco`.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core anexo_no_disco`, `… -p seele-conformance --test anexos` e `… -p seele-app --test frontend`. Depois, `cargo fmt --all` e `cargo clippy --workspace --all-targets --all-features -- -D warnings`.

NÃO fazer:
- não mudar o fio nem a validação de attachment.rs: o fio continua levando qualquer nome, e quem decide é quem recebe;
- não aceitar caminho vindo da janela em comando nenhum;
- não usar `Path::components` nem `file_name` na regra;
- não abrir o diálogo nativo de salvar, que seria uma decisão de produto que ninguém pediu;
- não deixar `File::create` em nenhum caminho do anexo;
- não acrescentar crate;
- não editar docs/pendencias.md, o índice nem o plano 1B. Quem registra é o lote Z, e este lote não ganha cadeia no G1.

**Riscos.** Fio, API de MODs e CSP: nada muda, porque `Transfer` não está em api/*.json. Uma 0.15.0 continua mandando o nome que quiser, e o anexo com nome hostil passa a ser recusado na hora de salvar, que é o conserto. Há comportamento novo também no Mac: nomes antes aceitos (CON.txt, «a:b», «a?b») passam a ser recusados. A gravação real no NTFS (reservados, ponto no fim, Zone.Identifier) só se exercita no Windows, pela bateria Windows do publicar.sh, e isso fica fora. A árvore principal (branch mobile/ios, sem commit) está movendo apps/seele-app/src/main.rs para lib.rs, e quem fizer o merge por último resolve a mudança de nome.

## Lote B-servidor — A portaria fecha quando o banco falha, e o seele.log de quem hospeda deixa de aceitar linha forjada pelo fio

Ordem 2. Itens: F1-portaria, P51-27, P51-26. Por que juntos: Os três são segurança do lado de quem hospeda e moram só no seele-server. O primeiro é a portaria que abre por falha. Os outros dois são a linha do seele.log que um cliente autenticado forja pelo id do pedido (P51-26, regressão do 6800d18 que não está na v0.15.0) e o texto do MOD que passa a entrar no mesmo `%error` (P51-27): o P51-26 só fica seguro com o Display escapado do P51-27. Nenhum arquivo é do S1, e por isso este lote corre na mesma onda que ele.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/admissao.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/mods/mod.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/mods/despacho.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/mods/pedidos.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/lib.rs (só se o fim do Copy de Falha pedir)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/como-se-faz-um-mod.md`

Ordem: o P51-27 primeiro, depois o P51-26. O F1-portaria é independente dos dois.

1. P51-27 (mods/mod.rs e despacho.rs).
- `Falha::Lancou` e `Falha::NaoCarregou` passam a carregar o texto da exceção e a primeira linha da pilha, lidos com `ctx.catch()` dentro do `contexto.with`. O molde é `Lancado::da_excecao` (apps/seele-app/src/executor.rs:2146-2160), copiado e não importado.
- A leitura de `name` e `stack` pode rodar um getter do MOD, e por isso roda sob o mesmo teto de passos, sem zerá-lo entre a chamada e a leitura. Em `PassouDoTempo` a exceção também é tirada com `ctx.catch()`.
- O texto é cortado em 512 pelo tamanho que ele terá depois de escapado, porque em `Debug` um caractere chega a dez. A regra é a de `ate_o_teto_do_registro`, reescrita aqui.
- O `Display` usa `{texto:?}`: «mod threw: "ReferenceError: console is not defined" at …». Um throw no topo diz «mod threw while loading: …».
- `Falha` deixa de ser `Copy` e passa a `Clone + PartialEq`. As cerca de 25 linhas com `Falha::` se ajustam, e os `assert_eq!(…, Err(Falha::Lancou))` (mod.rs:776, :846 e :1083) passam a `matches!(…, Err(Falha::Lancou { .. }))`. Conferir se lib.rs:667 e despacho.rs:155 compilam como estão.
- Guia (docs/como-se-faz-um-mod.md:196-200): uma frase dizendo que a linha de quem hospeda traz o que o MOD lançou.
Testes:
- `um_mod_que_lanca_nao_leva_o_vizinho_junto` (despacho.rs:804) passa a exigir `Error: eu` em `falharam[0].1.to_string()`. Hoje dá «mod threw».
- Em mod.rs: `aoPedir` com `console.log(1)` contém `ReferenceError`; `throw new Error('topo')` no topo contém `topo`; um texto com `\n` não quebra a linha do Display; `'\u{100000}'.repeat(2048)` cabe no teto.
- Reversões: voltar a `map_err(|_| Falha::Lancou)` deixa os testes vermelhos, e trocar `{texto:?}` por `{texto}` deixa vermelho o do `\n`.

2. P51-26, a metade do servidor (mods/pedidos.rs:53, dentro de `executar`). Esse ponto cobre `ClientMessage::ModRequest` e o `PedidoDeImagem::Ler` do volume (volume.rs:606-607).
- Id vazio: `warn!(catalogo = true, %error, …)`, sem `mod_id=`.
- Senão: `mod_id = %id_seguro(id)`. `id_seguro` é uma função local que corta em 128 caracteres e tira os de controle e os de bidi, como o `id_no_registro` do app, sem depender dele.
A grafia `mod_id=autor/nome` que o grep do guia procura não muda.
Testes, ao lado de `a_recusa_do_pedido_escreve_o_mod_id_que_o_grep_do_guia_acha` (:372) e com `servidor_que_recusa()`:
- id vazio: a linha traz `catalogo=true` e nenhum `mod_id=`;
- id `a/b` seguido de uma quebra de linha e de `WARN seele_server: forjada`: o registro fica com uma linha só.
Os dois falham no HEAD, voltar a `%id` deixa os dois vermelhos, e o teste do grep continua verde.
A metade do app (`mod_request`, main.rs:2736 e :2741) vai no lote C, junto com o P51-10, porque o main.rs está com o S1 nesta onda.

3. F1-portaria (admissao.rs).
- `use rusqlite::OptionalExtension;`.
- Em `carregar` (:85-101): `let senha_hash: Option<String> = conexao.query_row(…).optional()?;` e a contagem de convites com `?`, sem `.unwrap_or(0)`.
- `conferir_convite` (:355-361) passa a `.optional()?`, e o erro de banco vai ao log do operador como erro, e não como senha inválida.
- Os três chamadores já falham fechado e ficam como estão: session.rs:507-510 (CredentialRejected), `Daemon::politica_de_admissao` com o seeled main.rs:123-128 (false) e apps/seele-app/src/main.rs:7851 (BancoNaoRespondeu).
Teste `uma_falha_ao_ler_a_politica_fecha_a_porta_em_vez_de_abrir`, com `persistence.connection()` (pub(crate), persistence/mod.rs:220):
- (1) `definir_senha(Some("x"))`, depois `DROP TABLE configuracao`: `Politica::carregar` é Err;
- (2) só convites, depois `DROP TABLE convites`: é Err.
No HEAD os dois dão Ok com `aberto() == true`. Voltar a `.ok()` ou a `.unwrap_or(0)` deixa os dois vermelhos.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server admissao` e `… -p seele-server mods`, mais fmt e clippy -D warnings.

NÃO fazer:
- não tocar em apps/seele-app/src/main.rs: ele é do S1 nesta onda, e a metade do app do P51-26 é do lote C;
- não fazer o seele-server depender do seele-app;
- não mudar a resposta `bridge-refused` ao cliente nem nada no fio (`Falha` não atravessa);
- não editar pendências nem o índice;
- sem cadeia no G1.

**Riscos.** Nada no fio, na API ou na CSP. A portaria só muda quando o banco falha: antes entrava qualquer um, agora entram nenhum, com o motivo no log. O texto do MOD passa a entrar no seele.log de quem hospeda, e por isso o Display em Debug e o corte pelo tamanho escapado são obrigatórios: sem eles, o P51-27 reabre a injeção que o P51-26 fecha. O P51-26 só é registrado como fechado depois do lote C.

## Lote K1-registro-da-janela — O que a janela manda ao registro chega inteiro e legível, e o caminho até o invoke passa a ser exercitado

Ordem 3. Itens: P51-04, P51-06, P51-13. Por que juntos: Os três são o mesmo caminho, `registrarNoAnfitriao` em base.js até o invoke `registrar_da_janela`: o 4 corrige o que chega, o 6 corrige o texto que some, e o 13 exercita o elo. A prova (a) do 6 e o bloco novo do 13 são o mesmo bloco de bancada. O lote só toca JavaScript da janela e bancadas, e corre junto com o S1.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/base.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/mods-regiao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/contribuicoes-e-camadas.cjs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/regiao-do-mod.cjs`

1. P51-04 (base.js).
- Em :2060, a recusa de `podePedir`: antes do `responder(false, …)`, `registrarNoAnfitriao('atender-mod', `${mod.id}: «${m.tipo}» não existe na API ${mod?.api ?? '?'}`, 'aviso', mod.id)`.
- Em :2188, o tipo desconhecido: `registrarNoAnfitriao('atender-mod', `${mod.id}: a API de MODs não conhece «${m.tipo}»`, 'aviso', mod.id)`.
- No `catch` (:2190-2192), o registro passa a usar `motivoDaFalha(falha)` (mods-regiao.js:417, global porque é carregado antes de base.js), com `JSON.stringify(falha)` quando ele devolver «[object Object]». `fraseDeErro` fica só no `responder`.

2. P51-06.
- Em `registrarNoAnfitriao` (base.js:441-453): `const bem = (s) => String(s).replace(/[\uD800-\uDFFF]/gu, '�')`, aplicado a `o_que` e a `onde`.
- Para `modId`: `modId == null ? null : bem(modId)`. `bem(null)` daria o texto «null», e o Rust escreveria `mod_id=null`.
- Não usar `toWellFormed()`: ele exige o Safari 16.4, e o `minimumSystemVersion` é 11.0.
- Em mods-regiao.js:809: `[...plano.no.chave].slice(0, 120).join('')`, sem trocar substituto.

3. Bancada contribuicoes-e-camadas.cjs, para o P51-04.
- Carregar `ui/frases.js` inteiro no `contexto` por `vm.runInContext`, para `fraseDeErro` existir como no app. Conferir que nenhum bloco seguinte contava com a ausência dele.
- Exigir `!texto.includes('ALGO FALHOU')` na linha da contribuição recusada.
- Mover a troca de `contexto.registrarNoAnfitriao` para antes da chamada de API 3 (hoje ela começa por volta de :1805) e exigir `dita('contribuir', 'API 3')`.
- Depois de um `tipo: 'inexistente'`, exigir `dita('não conhece')`.

4. Bloco novo na mesma bancada, num `vm.createContext` PRÓPRIO, como o `aba` de :1167-1173. O `contexto` compartilhado tem `registrarNoAnfitriao() {}` (:207) e um `donoDaRegiao` de mentira (:756), que os blocos seguintes usam, e recortar as declarações de verdade nele as trocaria.
- Recortar de base.js `function registrarNoAnfitriao(` e `function donoDaRegiao(` até o primeiro `\n}\n`, como o recorte de `desenharARegiaoDoMod` (:1806-1810), com um `confere` que falha alto se não achar.
- No contexto: um `invoke` de mentira que anota {cmd, args} e devolve `Promise.resolve()`, `modsCarregados` (Map), `geracaoDaSessao` e uma instância com `admite: () => true`.
O bloco confere:
- (1) com a instância no mapa, `dono.anotarRecusa('x')` gera exatamente um `registrar_da_janela` com {nivel:'aviso', onde:'recusa-de-mod', oQue:'mod/a: x', modId:'mod/a'};
- (2) com o mapa vazio, ou com outra instância nele, não sai invoke nenhum;
- (3) `registrarNoAnfitriao('o','t','erro','mod/a')` repassa `modId` e `nivel`;
- (P51-06 a) com «avatar» seguido de `\uD800`, `args.oQue.isWellFormed()`; sem modId, `args.modId === null`.

5. regiao-do-mod.cjs (P51-06 b): um campo com a chave `'a'.repeat(119) + '😀'` exige `dataset.chaveDoMod.isWellFormed()` e que a chave termine em 😀.

Reversões, cada uma rodada antes do commit:
- tirar `modId` do objeto do invoke (base.js:449) reprova (1) e (3);
- inverter `if (!meu()) return;` (base.js:790) reprova (1) e (2);
- devolver `fraseDeErro` ao registro reprova a do «ALGO FALHOU»;
- tirar uma das duas linhas novas reprova o caso dela;
- tirar o `replace` reprova (a), e voltar ao `slice` reprova (b).

Comando: `cargo xtask check-runtime`, que roda as bancadas por nome (xtask/src/check_runtime.rs:85-90).

NÃO fazer:
- não mexer no Rust do registro, que é do lote C;
- não usar `toWellFormed`;
- não rodar recorte de base.js no contexto compartilhado;
- não criar evento novo na API de MODs;
- não editar pendências nem o índice.

**Riscos.** Só JavaScript da janela e bancada. Nada no fio, na API congelada ou na CSP. As duas linhas novas podem ser provocadas por um MOD em laço, como a do catch já pode, e o teto de vazão delas no Rust é o P51-08, do lote C. Carregar frases.js no contexto compartilhado pode mudar o que um bloco seguinte mede: confira que cada bloco continua verde pelo motivo certo. O WKWebView e o WebView2 não se medem aqui.

## Lote L1-o-que-a-release-leva — O classificador para de afirmar o que não sabe, o corpo diz se a bateria rodou, a CI ganha deny e fuzz, e os instaladores baixam de onde se publica

Ordem 4. Itens: F1-classificador, F1-ci-aqui, F1-install. Por que juntos: Os três mexem no caminho que publica e são guardados no mesmo xtask/tests/empacotamento.rs. O classificador e a linha da bateria moram no mesmo publicar.sh. Nada aqui é código do produto. O lote corre na onda 1: o L2 põe os guardas dele num arquivo novo, para não dividir o empacotamento.rs com este.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/empacotar/publicar.sh`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/xtask/tests/empacotamento.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/.github/workflows/ci.yml`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/fuzz/Cargo.toml`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/fuzz/fuzz_targets/uri.rs (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/fuzz/fuzz_targets/datagrama_do_encontro.rs (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/fuzz/sementes/ (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-proto/tests/sementes_do_fuzz.rs (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/install.sh`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/install.ps1`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/assinatura-e-atualizacao.md`

1. F1-classificador (publicar.sh:513-603, `notas_das_mudancas`).
- Um `case` com a lista explícita dos prefixos de papel, com e sem escopo: docs, test, chore, refactor, ci, build e style. É `case` e não regex, porque o `case` POSIX não exprime o padrão.
- Um assunto que não é feat, fix, perf nem prefixo de papel (sem prefixo, ou com um prefixo desconhecido como «wip:») é contado à parte.
- Com produto e ferramenta vazios e a conta à parte maior que zero, a saída é «_Os commits desta faixa não dizem pelo prefixo se mudam o produto, e este resumo não adivinha. A lista inteira está logo abaixo._». Ela não promete notas no topo, porque `empacotar/notas/$VERSAO.md` é opcional (:1887-1890).
- Um `aviso` a quem publica diz quantos assuntos ficaram sem classificar e sugere o arquivo de notas.
- «Nenhuma mudança de produto» fica só para quando todos os assuntos têm prefixo de papel.
Testes em empacotamento.rs:
- (a) `um_assunto_sem_prefixo_nao_vira_nenhuma_mudanca_de_produto`, com a entrada «O guarda do vetor deixa o console de fora» seguida de «docs: papel». A saída não pode conter «nenhuma mudança de produto» e tem de conter «não dizem pelo prefixo».
- (b) o mesmo com «wip: algo».
- `sem_feat_nem_fix_na_faixa_nao_se_inventa_secao` (:1781) continua verde. Reverter o publicar.sh deixa (a) e (b) vermelhos.
O índice (:109-119) não é tocado aqui: quem o atualiza é o lote Z.

2. F1-ci-aqui.
- (1) ci.yml ganha dois jobs, sob o `workflow_dispatch` que já existe:
  - `deny`: `cargo install cargo-deny --locked` no runner, depois `cargo deny check advisories licenses bans sources` sobre o deny.toml;
  - `fuzz-curto`: no nightly, `cargo install cargo-fuzz --locked`, e para cada alvo `cargo fuzz run <alvo> fuzz/sementes/<alvo> -- -max_total_time=60`.
  Não tocar no gatilho (:40-44), que é do dono.
- (2) Corpus-semente versionado em `fuzz/sementes/<alvo>/`, fora do `/fuzz/corpus/` que o .gitignore:27 ignora. As sementes são geradas e conferidas por crates/seele-proto/tests/sementes_do_fuzz.rs, que codifica quadros válidos do protocolo 8 com o `encode` real e compara com o que está no disco; com `SEELE_REGERAR_SEMENTES=1` ele os reescreve. Esse teste é a prova (b), `as_sementes_do_fuzz_estao_na_versao_do_fio`: todo arquivo de `control_frame` começa com `PROTOCOL_VERSION`, lido da constante de verdade. Ele mora no seele-proto porque o xtask não depende do seele-proto e não deve passar a depender.
- (3) Dois alvos novos, com `[[bin]]` em fuzz/Cargo.toml: `uri` (`seele_proto::uri::analisar`) e `datagrama_do_encontro` (`seele_proto::encontro::analisar` e `ler_aqui`). O fuzz/Cargo.lock não muda: confira com `git diff`.
- (4) publicar.sh, em «Como esta versão foi montada» (:1910-1931): a linha da bateria sai de uma função testável por `--linha-da-bateria`, como o `--notas`. Ela diz «Bateria: rodou aqui (fmt, clippy, testes, cargo deny)», com « e no Windows» só quando houver `pedido windows`. Com SEM_BATERIA=sim, diz «**Esta versão saiu com --sem-bateria: não foi testada antes de publicar.**».
Testes em empacotamento.rs:
- (a) `o_ci_roda_deny_e_fuzz`: exige no ci.yml um job com `cargo deny check` e um `cargo fuzz run` para cada `[[bin]]` de fuzz/Cargo.toml. Hoje reprova, e apagar o job reprova de novo.
- (c) `--linha-da-bateria` com e sem SEM_BATERIA, e o corpo de uma `Bancada` com `--sem-bateria` contendo «saiu com --sem-bateria». Reverter a linha deixa o teste vermelho.
Conferência local, sem rede:
- `cargo deny check --disable-fetch`;
- `cargo +nightly fuzz run <alvo> fuzz/sementes/<alvo> -- -max_total_time=10` para cada alvo novo (cargo-fuzz e o nightly já estão nesta máquina).

3. F1-install. Medido em 04/10: a v0.15.0 de SEELE-RELEASES publica SHA256SUMS, seele-cli-0.15.0-macos.tar.gz e o -windows-x86_64.zip, sem pacote Linux.
No install.sh:
- separar `REPO_DO_CODIGO="DATA-AND-DEV/SEELE"`, usado só na dica de `git clone`, de `REPO_DAS_VERSOES="DATA-AND-DEV/SEELE-RELEASES"`, usado em :72 e :87;
- baixar o SHA256SUMS ANTES do pacote. Se ele não listar `$PACOTE`, parar com: «a versão $VERSAO não publica o servidor para $SISTEMA. Compile do código-fonte: git clone https://github.com/DATA-AND-DEV/SEELE && cargo build --release --bin seeled»;
- no macOS, aceitar só Apple Silicon, conferido por `sysctl -in hw.optional.arm64` = 1 (e não por `uname -m`, que diz x86_64 sob Rosetta). Um Mac Intel recebe a mesma frase de compilar;
- corrigir o comentário de :55 (o tar é só da arquitetura de quem compila, empacotar/macos.sh:40 e :189).
No install.ps1: `$repoDasVersoes = 'DATA-AND-DEV/SEELE-RELEASES'`. A conferência da soma (:80-109) já existe e fica como está.
Em docs/assinatura-e-atualizacao.md:127, o exemplo de `endpoints` mostra SEELE-RELEASES.
Testes em empacotamento.rs:
- (a) `os_instaladores_de_uma_linha_baixam_de_onde_o_publicar_publica`: `REPO_DAS_VERSOES` e `$repoDasVersoes` estão nos `REPOS` do publicar.sh (mesmo recorte de :2054). Voltar a DATA-AND-DEV/SEELE reprova.
- (b) `#[cfg(unix)]`, sem rede: rodar o install.sh com SEELE_VERSION=v9.9.9, SEELE_BASE=file://<tempdir>, SEELE_BIN=<tempdir> e um SHA256SUMS sem o pacote do sistema. Espera saída diferente de 0 com «não publica o servidor para». Reverter a ordem (o pacote antes da soma) dá «não consegui baixar», e o teste fica vermelho.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p xtask --test empacotamento` e `… -p seele-proto --test sementes_do_fuzz`.

NÃO fazer:
- não tirar o segundo endpoint do tauri.conf.json nem o guarda `o_endereco_antigo_continua_na_lista_enquanto_a_migracao_dura` (:2150): o F1-endpoint-antigo é decisão do dono;
- não mudar o gatilho do ci.yml;
- não pôr cargo-deny nem cargo-fuzz em nenhum Cargo.toml;
- não editar o índice nem as pendências;
- não publicar, não dar push, não disparar Actions.

**Riscos.** Nada no fio, na API ou na CSP. O runner baixa cargo-deny e cargo-fuzz, como já baixa o playwright, e isso é ferramenta de CI, não dependência do produto. O nightly do fuzz pode quebrar sem aviso. O install.ps1 só se exercita no Windows, e aqui fica só o guarda de texto. O efeito do install.sh só chega a quem usa o curl do README depois do push do dono. O case novo tem de rodar no sh do macOS e no bash do Linux de quem publica. O ci.yml volta a ser editado pelo K3 (um comentário), que espera este lote.

## Lote L2-textos-publicos — Nenhum texto publicado manda rodar o `connection`, e nenhum promete que quem hospeda não ouve

Ordem 5. Itens: F1-connection, F1-texto-audio-em-claro. Por que juntos: Os dois corrigem frases públicas do README e de docs que contradizem o código. O README é editado pelos dois, e os guardas de ambos vão para o mesmo arquivo novo do xtask. O código que muda é só o binário seeled (crates/seele-server/src/main.rs), que nenhum outro lote toca.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/README.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/.github/NOTAS-DE-RELEASE.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/windows.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/teste-duas-maquinas.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/ponto-de-encontro.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/como-testar.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/alcance-pela-internet.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/specs/01-arquitetura.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/xtask/tests/textos_publicos.rs (novo)`

1. seeled (crates/seele-server/src/main.rs:88-101).
- Extrair a função pura `para_a_outra_maquina(porta, lan: Option<IpAddr>, global: Option<Ipv6Addr>, impressao: &str) -> Vec<String>`.
- Os links são montados com `seele_proto::uri::Convite::novo(SocketAddr::new(ip, porta).to_string()).com_impressao_digital(fp)`, o mesmo construtor que `criar_convite` usa (:200-203). Saem sem token e com a impressão, que é conferida dentro do TLS.
- A saída fica: «na outra máquina, cole no SEELE:», e depois uma linha com «seele://[2001:…]:8383?fp=…   (pela internet, se o firewall do roteador deixar entrar)» e outra com «seele://192.168.0.7:8383?fp=…   (na mesma rede)».
Teste no módulo de testes do bin (:342): `o_que_o_seeled_manda_colar_e_um_link_com_a_impressao`. Nenhuma linha contém «connection», e cada link passa em `seele_proto::uri::analisar` (uri.rs:421) com a impressão dada. Reverter para o `println!` antigo deixa o teste vermelho.

2. README.
- Trocar `connection --server …` (:140, :184, :188) por «abra o SEELE na outra máquina e cole o link que o seeled imprimiu».
- `cargo build --bin seeled`, sem `--bin connection` (:179). O `ln -sf` cita só o seeled. :208 e :255 (`connection --url`) saem.
- Tirar `.deb` e Linux do anúncio do instalador (:140, :191).
- Dizer que o compartilhamento de tela existe (:267; ele está em crates/seele-video).
- Trocar o «botão direito → Abrir» (:148) por Ajustes → Privacidade e Segurança → «Abrir assim mesmo», o caminho do macOS 15.
- Tirar o exemplo de :314.
.github/NOTAS-DE-RELEASE.md (:18-21 e :28): «dois programas: SEELE … e seeled …».

3. Docs.
- windows.md (:5, :141, :188, :209, :215), teste-duas-maquinas.md (:20, :45, :79, :99, :110, :292), como-testar.md (:3-50) e alcance-pela-internet.md:12 trocam o `connection` pelo app.
- ponto-de-encontro.md:113 e :117: `connection --hospedar` vira «HOSPEDAR AQUI no app», com `SEELE_ENCONTRO=…` no ambiente, porque o degrau 4 é da Hospedagem e não do seeled.
- As medidas do roteiro de duas máquinas que eram relatadas pelo connection passam a sair da telemetria do app ou do seele.log.

4. F1-texto-audio-em-claro.
- specs/01-arquitetura.md:55 passa a: «O servidor encaminha o Opus sem decodificar nem reescrever (specs/02), mas o recebe em claro: quem hospeda pode, em tese, gravar a voz. Por não tocar no payload, E2EE é um incremento, e não uma reescrita (specs/08).»
- README.md:108 passa a: «O TLS vai de cada pessoa até o servidor de quem hospeda, e o ponto de encontro não está nele. Quem hospeda recebe a voz e o texto em claro, como diz a specs/08.»

5. Guardas num arquivo NOVO, xtask/tests/textos_publicos.rs (o xtask descobre tests/*.rs sozinho). Ele não fica em empacotamento.rs porque o L1 edita esse arquivo na mesma onda.
- `nenhum_texto_publicado_manda_rodar_um_programa_que_nao_existe` varre README.md, .github/NOTAS-DE-RELEASE.md, install.sh, install.ps1, crates/seele-server/src/main.rs, windows.md, teste-duas-maquinas.md, ponto-de-encontro.md, como-testar.md e alcance-pela-internet.md atrás de «`connection`», «connection --» e «--bin connection», enquanto nenhum Cargo.toml declarar esse binário (ler os `[[bin]]`). Análises, planos e ADRs ficam fora, porque são histórico.
- `nenhum_texto_promete_que_quem_hospeda_nao_ouve`: README.md e specs/*.md não contêm «nunca vê áudio em claro», e no README não sobra «ponta a ponta» sem o qualificador «até o servidor».
- Reversões: voltar a linha :208 do README ou a :141 do windows.md deixa o primeiro vermelho, e voltar a frase da specs/01 deixa o segundo.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server --bin seeled` e `… -p xtask --test textos_publicos`.

NÃO fazer:
- não reescrever as frases do ponto de encontro e do pino que as cadeias F-Docs e F-Fecho do G1 sustentam (o link que discorda num alvo de LAN, a escuta que NÃO é conferida no TLS, a abertura de ponto-de-encontro.md sobre os 60 s). Troque só o que o item pede e releia cada parágrafo inteiro contra o adendo do ADR 0003;
- não editar a página publicada da v0.15.0, que é do dono;
- não tocar em xtask/tests/empacotamento.rs;
- não editar pendências nem o índice.

**Riscos.** Nada no fio, na API ou na CSP. A saída do seeled muda de texto, e nenhum script a lê (git grep vazio). O roteiro de duas máquinas é reescrito em torno do app, e algumas linhas da tabela de medidas mudam de fonte. O README e os docs só chegam ao GitHub com o push do dono. docs/ponto-de-encontro.md e docs/alcance-pela-internet.md voltam a ser editados pelo M2 (P51-20), que espera este lote.

## Lote C-registro-da-casca — As portas do seele.log da casca: a frase da janela num campo, um teto de vazão por MOD, o id sempre escapado e a recusa de carga por geração

Ordem 6. Itens: P51-05, P51-08, P51-10, P51-12. Por que juntos: Os quatro mexem só em apps/seele-app/src/main.rs, e no `BaldeDoConsole` de executor.rs, nas mesmas funções do registro: `registrar_da_janela` (5 e 8), as recusas de mídia (8 e 10), `mod_request` (10 e a metade do app do 26) e as recusas de carga (12). A pendência pede o 5 junto com o 8, e o 10 junto com o 26. O lote leva também a metade do app do P51-26, que o lote B não pôde fazer porque o main.rs estava com o S1.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/executor.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-plano-1d-mods-na-casca.md`

Ordem: P51-05, P51-08, P51-10 (com a metade do app do P51-26) e P51-12.

1. P51-05.
- Em `registrar_da_janela` (main.rs:2414-2418), nos três braços (aviso, erro e DEBUG): `tracing::warn!(onde = %onde, mod_id, frase = ?o_que, "a janela disse")`, e o mesmo com error! e debug!. O `mod_id` verdadeiro passa a vir antes, e um forjado fica entre aspas e escapado.
- O corte muda junto. O `.take(TETO_DA_FRASE_NO_REGISTRO)` (:2396-2400) passa a `executor::ate_o_teto_do_registro`, com o aviso do corte, como o console faz: em `?`, um caractere chega a ocupar dez.
- O doc de `TETO_DA_FRASE_NO_REGISTRO` (:2284-2303) passa a dizer que a janela também escapa. A troca por espaço de `quebra_ou_inverte_a_linha` fica, como segunda defesa.
- No plano 1D, acertar só a descrição da forma da linha (:3876). Os greps de :157 e :3874 continuam casando.
Testes, em `mod o_registro_da_janela` (:11597):
- `o_que = "x onde=forjado mod_id=vitima/mod"` com `mod_id = Some("a/b")`: o primeiro `mod_id=` da linha é `mod_id=a/b`. No HEAD ele é `vitima/mod`;
- `registrar_da_janela` entra em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas` (:12188), com `frase="…"` do tamanho dos outros campos;
- ajustar `o_aviso_da_janela_sai_como_warn_com_o_id_do_mod_em_campo_proprio` para achar a frase dentro de `frase="…"`.
Reversões: voltar ao `"{o_que}"` deixa vermelho o primeiro, e voltar ao `.take(512)` deixa vermelho o do pior caso.

2. P51-08.
- `Session.baldes_do_registro: Mutex<HashMap<String, BaldeDoConsole>>`, com a chave por `id_no_registro(id)` e uma chave fixa para a linha sem id (o catálogo).
- O mapa tem no máximo 64 entradas. Cheio, um id novo usa um balde de transbordo compartilhado, e nunca passa sem teto.
- Extrair `registrar_da_janela_em(&baldes, agora: Instant, nivel, onde, o_que, mod_id)`, com o relógio passado como parâmetro. O comando passa a receber `State<Session>`, sem mudar os argumentos do invoke.
- A regra única para as outras portas: `recusa_de_midia_dita` e `midia_servida_dita` também recebem `&baldes` e `agora`, e os comandos passam `&session.baldes_do_registro` e `Instant::now()`.
- Aplicar `admitir(agora)` nas três portas, levando `suprimidas` num campo da linha seguinte, como `registrar_console_do_mod` faz.
- Em `Session::revogar`, antes de esvaziar o mapa, dizer a conta pendente de cada balde, uma linha por MOD. Se o balde não expõe isso, acrescente em executor.rs um `pendentes()` sobre a lógica de `pendentes_se_couber` (:526). O lote D vai mexer no mesmo struct depois.
Testes, com o relógio fixo:
- 200 chamadas para `a/b` no mesmo instante deixam exatamente `RAJADA_DO_CONSOLE` linhas;
- avançando 250 ms, a linha seguinte traz `suprimidas=168`;
- `c/d` não é segurado pelo balde de `a/b`;
- com o mapa cheio, um id novo passa pelo transbordo;
- `revogar` com conta pendente escreve a linha da conta;
- o mesmo para `recusa_de_midia_dita`.
Os testes de `o_registro_da_janela` passam a chamar a versão `_em`. Reversão: tirar o `admitir` deixa os testes vermelhos (no HEAD saem 200 linhas).

3. P51-10, com a metade do app do P51-26.
- O doc de `motivo_no_registro` (:5320-5323) passa a: «o formatador escapa só a mensagem, e só ANSI; um campo em `%` sai cru, e quem protege a linha são as aspas e o escape desta função».
- `mod_id = %id_no_registro(&id)` em :2736, :2741, :5610 e :5918.
- O doc de `id_no_registro` lista as portas por nome, sem número de linha.
- Extrair `pedido_recusado_na_saida_dito(id, request, erro)` de `mod_request`: com id vazio, `catalogo = true` e nenhum `mod_id=` (a metade do app do P51-26); senão, `mod_id = %id_no_registro(&id)`.
Testes:
- com `id = "a/b\nWARN seele_app: forjada"`, sai uma linha só (no HEAD saem duas);
- id vazio dá `catalogo=true` sem `mod_id=`;
- um guarda de fonte exige que nenhum `mod_id = %id` use o parâmetro cru do comando em `midia_em_bytes` e `escolher_para_o_mod`.
Reversão: voltar a `%id` deixa os testes vermelhos.

4. P51-12.
- `RecusasDeCarga = Mutex<HashMap<String, (u64, String)>>`.
- `recusa_de_carga_dita` e `carga_dita` recebem `geracao` (que `codigo_do_mod` e `mod_nativo_reservar` já têm). A recusa é dita quando a entrada não existe, quando a geração é outra ou quando o motivo mudou.
- Atualizar o texto que `os_dois_comandos_de_carga_passam_pela_recusa_dita` (:11562-11586) espera, e tirar a ressalva do doc (:3685-3690).
Teste novo, perto de :11380: `revogar()`, depois a recusa atrasada da geração 1, depois a mesma recusa na geração 2. Têm de sair duas linhas WARN. Para o vermelho de verdade: primeiro a assinatura nova, comparando só o motivo (o comportamento do HEAD), e o teste fica vermelho; depois a comparação pela geração.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app o_registro_da_janela`, `… -p seele-app recusa`, e a suíte da lib do seele-app inteira antes do commit. Depois fmt e clippy -D warnings.

NÃO fazer:
- não manter o `.take(512)`;
- não usar `Instant::now()` dentro dos testes;
- não mudar os argumentos do invoke `registrar_da_janela`, porque a janela não muda;
- não mexer no prelúdio nem no laço do executor (lote D);
- não mexer nas conferências `!session.geracao_vale(` (lote E);
- não editar pendências nem o índice.

**Riscos.** Muda a forma das linhas no seele.log, que é texto e não fio. O grep do guia (`mod_id=…`, docs/como-se-faz-um-mod.md:237) e o do roteiro do 1D continuam casando. Segurar linhas pode esconder um diagnóstico, e por isso a conta (`suprimidas`, mais a do revogar) é obrigatória e tem teste. Sem risco de CSP nem de API. O P51-14 (lote D) escreve testes sobre `registrar_da_janela_em`, e eles devem procurar campos, e não a forma inteira da linha.

## Lote H-recusa-legivel — A volta recusada diz o motivo de verdade, e o impostor que não prova a chave deixa de parecer «versão incompatível»

Ordem 7. Itens: P51-03, P51-02. Por que juntos: São acoplados pelo caminho da bateria. Com `vale_insistir = false`, o P51-02 sozinho trocaria «ENLACE PERDIDO» por «CREDENCIAL RECUSADA»: o P51-03 tem de vir antes, ou junto. Os dois mexem em `vale_insistir` e `alguem_respondeu` (enlace.rs), em `fim_da_recusa` e nos tipos da FFI, e nas frases. O lote não toca o main.rs e corre junto com o C.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/client.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/enlace.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/types.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/frases.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/frontend.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/bateria_interna.rs`

Ordem: o P51-03 inteiro, e só depois o P51-02.

1. P51-03.
- (1) `Motivo::Recusado(String)` passa a `Motivo::Recusado(ConnectError)` (enlace.rs:300 e :2916). `ConnectError` já é `Clone + PartialEq + Eq` (client.rs:59), e `Motivo` mantém os traços que deriva.
- (2) Em enlace.rs:2916, antes do `encerrar`: `tracing::warn!(?erro, "a volta da bateria foi recusada, e a sessão acaba")`. Hoje uma volta recusada acaba sem uma linha no seele.log.
- (3) Na FFI, `fn fim_da_recusa(&ConnectError) -> EndReason`, com `match` exaustivo e SEM braço `_`, pela regra de enlace.rs:5355-5363:
  - `Refused { reason }` vira `(*reason).into()`;
  - `InviteMismatch` vira `EndReason::ServidorTrocouNaVolta`, que é nova;
  - `PinChanged` vira `EndReason::ChaveMudouNaVolta`, que é nova;
  - `ModsNaoAceitos` vira `ModsMudaram`, que já existe;
  - LocalEndpoint, Unreachable, TlsRefused, HandshakeTimeout, SemResposta e ProtocolViolation viram `LinkLost`, cada uma nomeada no `match`.
  As variantes novas não levam impressões, porque `EndReason` é `Copy + Hash`. O braço `Motivo::Recusado(_) => CredentialRejected` (seele-ffi lib.rs:4150) passa a chamar `fim_da_recusa`.
- (4) Em frases.js, MOTIVOS ganha «QUEM ATENDEU NA VOLTA NÃO É O SERVIDOR DO CONVITE.\nA sessão acabou sem mandar nada a ele. Confirme o link com quem o mandou.» e «A CHAVE DO SERVIDOR MUDOU DURANTE A SESSÃO.\nConfirme por outro canal antes de entrar de novo.».
- (5) Os dois testes que liam o texto passam a ler o tipo. Em enlace.rs:6876: `matches!(aviso, Aviso::Encerrado(Motivo::Recusado(ConnectError::InviteMismatch { .. })))`. Em bateria_interna.rs:553: `Recusado(ConnectError::InviteMismatch { expected, offered })`, conferindo `expected == real` e `offered == ofertada`.
Prova, com o vermelho de verdade:
- primeiro o tipo muda, com `fim_da_recusa` devolvendo `CredentialRejected` para tudo (o HEAD), e o teste novo de seele-ffi fica vermelho nos quatro casos: InviteMismatch dá ServidorTrocouNaVolta, Refused{ServerFull} dá ServerFull, PinChanged dá ChaveMudouNaVolta, e ModsNaoAceitos dá ModsMudaram;
- depois vem o mapeamento;
- um teste de rastro em enlace.rs (`rastro_de_teste`) exige a linha WARN com a variante;
- o guarda frontend.rs:7618 (`every_reason_a_session_can_end_with_has_a_sentence_in_the_page`) obriga as frases novas.
Reversões: voltar o braço a `CredentialRejected` deixa o teste vermelho, e tirar o `warn!` deixa vermelho o de rastro.

2. P51-02.
- (1) `ConnectError::ChaveNaoProvada` em client.rs.
- (2) Em `classify_connection_error`: com a decisão `Some(Matches|FirstContact)` e um `TransportError(e)` cujo `e.code == quinn::TransportErrorCode::crypto(51)` (decrypt_error), devolve essa variante. `None` e os outros `TransportError` continuam `TlsRefused`. O precedente é `motivo_da_recusa` (par.rs:1118-1141).
- (3) `alguem_respondeu` (:5365) dá true, e `vale_insistir` (:5391) dá false. `desfazer_o_pin_deste_aperto` não muda.
- (4) `fim_da_recusa(ChaveNaoProvada)` vira `ServidorTrocouNaVolta`.
- (5) `ConnectionError::ChaveNaoProvada`, variante unitária, em types.rs:1864, traduzida em `classify_connect_failure` (lib.rs:4993, exaustivo).
- (6) FRASES, em frases.js, ganha `ChaveNaoProvada`: «QUEM ATENDEU NÃO PROVOU SER ESTE SERVIDOR.\nEle mostrou o certificado do servidor sem a chave que o assina. O convite, a senha e o apelido não chegaram a ele.».
- (7) Acertar o doc de `TlsRefused` (client.rs:66-77) e o parágrafo de enlace.rs:4880-4889.
Prova:
- enlace.rs:6018, o impostor de loopback (`impostor_que_repete_o_certificado`, :6949), passa a exigir `Some(ConnectError::ChaveNaoProvada)` (no HEAD dá TlsRefused). A asserção de :6041 procura `ChaveNaoProvada` na linha do pin desfeito;
- em client.rs, `sem_recusa_do_verificador_a_classificacao_e_a_de_sempre` continua verde, e um caso novo prende que um alerta diferente de 51 (por exemplo, 120) continua `TlsRefused`;
- em frontend.rs, um guarda novo de `ConnectionError` contra FRASES (hoje só há o de EndReason, :7641, e o de FalhaAo*, :2220) exige `ChaveNaoProvada:` com «NÃO PROVOU».
Reversões: tirar o braço do alerta 51 deixa vermelho o teste do impostor, e tirar a frase deixa vermelho o de frontend.rs.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core enlace`, `… -p seele-core client`, `… -p seele-ffi fim_da_recusa`, `… -p seele-conformance --test bateria_interna`, `… -p seele-conformance --test acceptance_m5 --test estados` (eles usam `matches!` e devem compilar sem ajuste) e `… -p seele-app --test frontend`.

NÃO fazer:
- não pôr impressões dentro de `EndReason`;
- não usar braço `_` em `fim_da_recusa`;
- não classificar o caso sem decisão do verificador (`None`) como ChaveNaoProvada;
- não entregar o P51-02 sem o P51-03 no mesmo lote;
- não mudar o fio;
- não editar pendências nem o índice.

**Riscos.** Nada no fio nem na CSP. A FFI ganha variantes internas à casca. `EndReason` chega ao Snapshot (`ended`), que os MODs leem: os valores novos são aditivos, e nenhum api/*.json enumera EndReason. A classificação depende do número do alerta que o rustls 0.23.45 manda. Se uma versão nova mudar o mapeamento, o teste do impostor reprova alto, sem ficar calado. A 0.15.0 continua conversando, porque um servidor legítimo nunca provoca decrypt_error.

## Lote K2-midia-provada-e-contada — Uma imagem que não decodifica é recusada e dita, e a aba DIAGNÓSTICO conta a mídia da região, das páginas e do fundo de tela

Ordem 8. Itens: P51-07, P49-5. Por que juntos: Os dois reescrevem a mesma função, `buscarFundoDaTela` (mods-regiao.js:1927-1978). O P51-07 troca o `onload` por `decode()`, e o P49-5 acrescenta `anotarMidia`, a recusa e o esquecimento. Os dois mexem também nas anotações de mídia e nas mesmas duas bancadas. Feitos juntos, o fundo de tela tem um caminho só de recusa.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/mods-regiao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/base.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/camada-mods.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/regiao-do-mod.cjs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/contribuicoes-e-camadas.cjs`

1. P51-07.
- Criar `provarImagem(uri)`: `const i = new Image(); i.src = uri; return i.decode().catch(() => { throw new Error('a imagem não decodificou'); })`.
- Nos quatro caminhos (retrato, mods-regiao.js:1549-1556; fundo de mídia, :1491-1495; fundo de tela, :1959-1966; avatar contribuído, base.js:1197-1201), o `decode()` é encadeado DENTRO do `then` que já existe (`return provarImagem(midia.uri).then(…)`). Assim a recusa cai no `.catch` de cada um, que já anota, fala ao MOD com o evento que a API já tem (`midia`/`falhou` e `tela`/`fundo-falhou`) e diz ao registro (mods-regiao.js:1496-1502, :1563-1569, :1968-1979; base.js:1203-1213).
- Depois do `await`, conferir de novo `cancelado`, `this.solta` e `podeFalar()` antes de aplicar.
- Somar `this[bolso.conta] += midia.bytes` (retrato e fundo de mídia) e `estado.bytes` (avatar) só depois do `decode()`.
- No retrato, um ouvinte de `error` no `<img>` leva ao mesmo caminho de recusa.
- No fundo de tela, o `img.onload` vira o `decode()` do próprio `img`.

2. P49-5.
- Em `buscarFundoDaTela`: «carregando» ao pedir; «pronta» depois do `decode()`; «recusada» nas três saídas (papel errado, `.catch` e a recusa do `decode()`), pela mesma assinatura `anotarMidia(elem, situacao, motivo)` (:2627). `esquecerMidia(elem)` quando a declaração tira o fundo (`!declarado`). O `onerror` que o item pedia fica coberto pela recusa do `decode()` do P51-07.
- Em base.js, a função pura `midiasDaRegiaoEDasPaginas(regioes, superficies)` soma `midiasAnotadas()` da região de cada MOD (`regioesDosMods`, :680) e da região de cada superfície (`superficiesDosMods`, :1096), por `regiao.id`, no formato `{carregando, pronta, recusada, motivos}`. Ela deixa de fora as anotações `cartao: true`, que `midiasDoPonto('pessoa.cartao')` já conta (:1416-1421).
- A aba (camada-mods.js, junto de `quemPintaCadaPonto`, :880-891) ganha o bloco «a região e as páginas de cada MOD», com frases como «a região e as páginas de mod/x: 1 recusada — <motivo>».
- O doc de `anotarMidia` (mods-regiao.js:2607-2618) deixa de dizer «ninguém as lê».

3. Provas.
- regiao-do-mod.cjs: um `Image` de mentira cujo `decode()` recusa para um uri marcado. Com esse uri, retrato, fundo de mídia e tela exigem o estado «recusada», o evento ao MOD (`midia`/`falhou` e `tela`/`fundo-falhou`), a linha em `anotarRecusa` e a conta de bytes sem a imagem. No HEAD os três ficam «pronta» e calados.
- No mesmo arquivo, no laço «fundo de tela» (:1660-1700, casos «… que não é imagem» e «… que o Rust recusou»): `regiao.midiasAnotadas()` tem a anotação `recusada` com o motivo. Um caso novo, em que a declaração tira o fundo, faz a anotação sumir.
- contribuicoes-e-camadas.cjs: o avatar contribuído com o `Image` de mentira no contexto. Com o renderer de mentira (:1204), uma região com um som recusado, uma superfície com um fundo recusado e uma anotação `cartao: true` recusada: a função devolve `recusada: 2` com os dois motivos, e não 3.
- Reversões: tirar o `decode()` de um caminho deixa o caso dele vermelho; tirar o `anotarMidia` do fundo deixa vermelho o caso do laço; tirar a soma da superfície dá 1, e tirar o filtro de cartão dá 3.

Comando: `cargo xtask check-runtime`. O desenho do bloco no Chromium é opcional, e roda sem baixar nada com `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright node apps/seele-app/bancada/diagnostico-de-mods.cjs`.

NÃO fazer:
- não criar evento novo na API de MODs;
- não afrouxar `img-src`;
- não somar bytes antes do `decode()`;
- não contar duas vezes as anotações de cartão;
- não instalar Playwright pela rede;
- não editar pendências.

**Riscos.** Nada no fio, na API congelada nem na CSP: é o mesmo uri, sob o mesmo `img-src`. A imagem passa a ficar «pronta» um pouco mais tarde, só depois de decodificada. O `decode()` com bytes corrompidos no WKWebView, no WebView2 e no WebKitGTK não se mede aqui (fora), só com o Image de mentira no Node.

## Lote N-guardas-e-guia — Os guardas que provavam menos do que diziam (check-api e vetor de referência), o .gitattributes e o guia da metade de cliente pela API 5

Ordem 9. Itens: P51-15, P51-21, P51-28, P51-22. Por que juntos: São quatro itens pequenos e independentes, cada um num arquivo que nenhum outro lote toca, a não ser o guia: docs/como-se-faz-um-mod.md é editado pelo lote B (P51-27) antes e pelo D (frase opcional) depois. Juntá-los num lote só fecha de uma vez o que é de teste e de texto puro.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/xtask/src/check_api.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/.gitattributes`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/mod_de_referencia.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/como-se-faz-um-mod.md`

1. P51-15 (xtask/src/check_api.rs). Duas partes já fecharam: o «panicked» no stderr não sai mais, e a etapa do check-api no publicar.sh já existe (:1005-1019). Quem as registra é o lote Z.
- No mod de testes, com o `pasta_com` (:904):
  - (a) `v7.json` com o conteúdo `{` faz `conferir_api` voltar Err com uma linha que contém «não é json» e o caminho;
  - (b) `Ok(dir.join("v8.json"))`, sem o arquivo, volta Err com «não deu para ler».
- O braço `Err(falhas)` de `run` (:478-483) sai para `fn relatar(resultado, n_momentos, n_eventos, saida: &mut impl Write, erros: &mut impl Write) -> ExitCode`, com teste: Err dá FAILURE e escreve cada falha com o prefixo «check-api:»; Ok dá SUCCESS e escreve o resumo.
- Reversões: apagar o push de :392 reprova (a), apagar o de :388 reprova (b), e trocar o FAILURE de `relatar` por SUCCESS reprova o teste dele.

2. P51-21 (.gitattributes:26-28), só comentário: «A regra de cima bastaria para os vetores que só chegam pelo checkout; para um arquivo escrito numa máquina Windows e conferido pelo hash da cópia de trabalho (`api/*.json`, abaixo), não basta. Nos dois casos, esta sobrevive a alguém afrouxá-la.». O guarda de catalogo.rs:833-956 ignora as linhas com `#` e continua verde: confira com `cargo test -p seele-app catalogo`.

3. P51-28, só o guarda (crates/seele-conformance/tests/mod_de_referencia.rs:242-266). O vetor, o prelúdio e api/v5.json não mudam.
- A leitura de `oferecidos` passa a achar `nome: (` e `nome: async (`, inclusive dentro de um `{ nome: (` de espalhamento condicional. É o equivalente, por linha, de `(?:^|\{\s*)([A-Za-z0-9]+): (?:async )?\(`, feito à mão com `split`, sem dependência. Ela não pega `pedir('enviar-imagem', { arquivo, token })` nem `{ arquivo, inicio: Number(`.
- Lista nomeada `NAO_EXERCITADOS_PELO_VETOR: &[(&str, &str)] = &[("enviar", "pede volume.esperar na metade de servidor e um arquivo escolhido; o vetor fica igual a 3d50b21 até a medida de campo da pendência 49")]`.
- O guarda (a) cobra do vetor todo oferecido fora da lista; (b) reprova se um nome da lista sumir do prelúdio; (c) reprova se o vetor passar a chamar `.nome(` de um nome da lista, porque a exceção venceu; (d) exige `criar` e `enviar` entre os oferecidos.
- Reversões: no HEAD, (d) reprova; voltar à leitura de hoje reprova (d); tirar `enviar` da lista reprova (a).

4. P51-22 (docs/como-se-faz-um-mod.md, «A metade de cliente», :55-69). Este lote vem depois do B, que pôs uma frase em :196-200.
- Reescrever pela API 5. O código roda num executor QuickJS fora da janela, sem `document` nem `window`. A janela recebe uma região declarada (`SeeleUI.regiao`), tema, cartões, eventos (`SeeleUI.aoEvento`), superfícies e contribuições atrás das capacidades, e `SeeleMods.request`/`snapshot`. Os nomes têm de bater com o prelúdio (executor.rs:1614 e :1675).
- Apontar `api/README.md` e `api/v5.json` como referência, e `apps/seele-app/testes/mod-de-referencia/cliente/main.js` como exemplo vivo.
- No `mod.json` de exemplo (:30, :43): `"api": 5`, um `reach` dos que o vetor usa, e a tabela apontando `api/v5.json`.
- Não apontar o guia externo, que ainda diz API 4 (P51-24, fora).
- Prova: o grep da pendência (:8086) deixa de achar «acesso à janela inteira», `document.documentElement.style` e `"api": 1`.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p xtask check_api`, `… -p seele-conformance --test mod_de_referencia` e `… -p seele-app catalogo`.

NÃO fazer:
- não mexer no vetor de referência nem no prelúdio: a medida de campo da pendência 49 compara o vetor byte a byte com 3d50b21, e exercitá-lo é do dono;
- não apontar o guia externo;
- não editar pendências nem o índice.

**Riscos.** Nenhum no código do produto. O guia é frase pública, e pela regra que o lote Z escreve ele não ganha cadeia. O recorte de `oferecidos` quebra se o prelúdio mudar de forma, e por isso falha alto. O lote D muda `emTexto` no prelúdio depois deste, e quem fizer o D roda este teste de novo.

## Lote D-console-do-executor — O console do MOD: a promessa rejeitada só é dita depois da volta e com teto, a pilha e o aninhado têm prova, e a conta do balde sai sem esperar o MOD parar

Ordem 10. Itens: P51-01, P51-14. Por que juntos: Os dois mexem no laço de `rodar` do executor.rs. O P51-01 muda `dizer_as_rejeitadas`, chamada em `escoar_jobs` e na parada, e o P51-14 muda a espera do laço e o `BaldeDoConsole`. Mexem também no prelúdio (`emTexto`) e nos testes do registro em main.rs, que dependem da forma que o lote C deixou.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/executor.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/como-se-faz-um-mod.md (opcional, :217-218)`

1. P51-01.
- (a) Tirar o `avisar(…, ParaOFora::Falhou(…))` do caminho normal do rastreador (executor.rs:1163-1167). Ele fica no braço `Err(_)` do `try_borrow_mut` (:1176-1181), onde a linha já sai na hora: sem isso, a janela deixaria de saber.
- (b) `RejeicoesDaVolta::guardar` guarda, junto da `LinhaCortada`, o texto do Falhou já cortado por `cortar_linha_do_console`. A conta `alem` continua só número.
- (c) `dizer_as_rejeitadas` (:2089) recebe `fila: &Arc<Fila>`. Para cada rejeição que ninguém pegou, manda a linha pelo balde e DEPOIS `avisar(manda, fila, ParaOFora::Falhou("promessa rejeitada sem tratamento: <texto cortado>"))`. Com `alem > 0`, um Falhou só, com a conta. Os três chamadores passam `fila`: `escoar_jobs` (:1953), a parada (:1401) e o teste (:4026).
- (d) `tratada` continua tirando a rejeição da espera.
- (e) O doc de `REJEICOES_GUARDADAS` perde o parágrafo «O Falhou que vai à janela não passa…» (:1985-1995).
Testes:
- `uma_rejeicao_pega_logo_depois_nao_vira_linha_de_erro` (main.rs:10962) ganha `!deixou.falas.iter().any(|f| matches!(f, ParaOFora::Falhou(t) if t.contains("pega logo depois")))`. Falha no HEAD;
- `o_falhou_da_rejeicao_tem_o_teto_do_registro`: `Promise.reject(new Error('x'.repeat(100000)))` exige o Falhou dentro do teto (512 mais o aviso do corte);
- executor.rs:4003 continua lendo o primeiro `try_recv()` como Console e ganha a asserção de que o segundo é o Falhou cortado;
- `uma_promessa_rejeitada_sem_tratamento_nao_morre_calada` (:3908) e `…chega_ao_registro_com_o_texto` (main.rs:10912) continuam verdes.
Reversão: devolver o `avisar` ao caminho normal deixa vermelha a primeira asserção.

2. P51-14.
- m8: exigir a pilha na linha do temporizador (main.rs:11246) e na do ouvinte (:11283) com `r"\n    at "`, porque no seele.log a quebra sai escapada. No teste do executor (:2553), que lê o texto cru, exigir a quebra de verdade. Reversão: tirar o trecho de executor.rs:1560 reprova os três.
- m9: o lote C já pôs `registrar_da_janela_em` em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas`. Confira e cite isso no relatório. Se não estiver lá, escreva o teste com `"x".repeat(2048)`, exigindo o que `ate_o_teto_do_registro` promete (o teto escapado e o aviso), e não «512 x» exatos.
- T3 M2: dois testes por `registrar_da_janela_em`: nivel «erro» com `Some("a/b")` exige ERROR e `mod_id=a/b`; nivel «outro» exige DEBUG e `mod_id=a/b` (a captura já é TRACE, :10568). Procure campos, e não a forma inteira da linha. Reversão: tirar `mod_id` do braço «erro» reprova.
- T1, em `emTexto` (executor.rs:1544-1567):
  - `semMentir` dá `name: message` para um Error aninhado, `[função nome]` para uma função e `String(v)` para um símbolo;
  - quando o primeiro `JSON.stringify` lança (ciclo), uma segunda passada troca por «[cíclico]» só o que é ancestral do valor de agora, com a pilha de ancestrais tirada do `this` do substituto. Um WeakSet de tudo o que já foi visto marcaria como cíclica uma referência repetida.
  - Teste: `console.log({e:new Error('x'), f(){}, s:Symbol('s')})` dá «Error: x», «[função f]» e «Symbol(s)»; `o.o=o; console.log(o)` contém «cíclico» e não «[object Object]».
  - O teste `uma_parte_que_nao_vira_texto_nao_derruba_a_linha_do_console` (:2508-2525) troca de exemplo: um objeto de protótipo nulo com um getter que lança. O comentário de :1550-1552 acompanha.
- T2 N1:
  - `BaldeDoConsole` ganha `ate_a_proxima_ficha(agora) -> Option<Duration>`, que só é Some com `suprimidas > 0`;
  - no laço de `rodar` (:1278-1287), a espera passa a `recv_timeout(min(proximo_vencimento, ate_a_proxima_ficha))`, com o `recv()` sem prazo só quando os dois são None. O ramo do timeout (:1324) já chama `dizer_as_seguradas`;
  - o doc de :580-582 é corrigido;
  - teste: `for (let i=0;i<100;i++) console.log(i);`, sem temporizador e sem Encerrar, recebe em até 1 s `Console{texto: CONTA_DO_BALDE, suprimidas>=60}`. No HEAD a conta só vem no Encerrar;
  - `uma_rajada_no_console_seguida_de_silencio_tem_a_conta_dita_ao_parar` (:2937) continua verde;
  - opcional: o guia (:217-218) ganha «ou, se o MOD se calar, uma linha do produto logo depois».
- T2 m1 já fechou em 517afaa, e não há nada a fazer.

Depois de mexer no prelúdio, rode `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-conformance --test mod_de_referencia`: o guarda dele acha onde o console começa e termina no prelúdio, e o lote N mudou a leitura dele. Rode também `… -p seele-app executor` e `… -p seele-app o_registro_da_janela`, e depois fmt e clippy -D warnings.

NÃO fazer:
- não mandar o Falhou por `manda.send` direto: ele passa por `avisar`, por causa da cota AVISOS_NA_FILA;
- não mandar o Falhou antes da linha, ou o teste de :4026 quebra pela ordem;
- não usar WeakSet para detectar ciclo;
- não mexer nas portas do registro (lote C) nem nas gerações (lote E);
- não editar pendências nem o índice.

**Riscos.** Nada no fio nem na API: o `console` não está em vN.json nenhum. O T1 muda o texto de objetos aninhados no seele.log. O T2 N1 acorda a thread do motor no máximo uma vez por ficha (≤250 ms), e só enquanto há linha segurada. A gestão passa a saber da rejeição no fim da volta, e não no meio dela.

## Lote M1-consulta-ao-quarto — A consulta ao quarto e as marcas: o que os testes do 1A e do 1B diziam provar passa a ser provado, e o que o 1A deixou de duplicação sai

Ordem 11. Itens: P51-17, P51-19. Por que juntos: O T5a, o T5c e o T6f do 17 e o M2 do 19 mexem na mesma `resolver_ponto` e no mesmo `saidas == 0` de seele-core/src/encontro.rs, e a pendência pede que se façam juntos. O T1c do 17 (os campos de `Marcas` privados) e o C4 e o T5e do 19 mexem no mesmo alcance/encontro.rs do seele-server. É um lote pesado (G mais M), mas separá-lo partiria a mesma função entre dois implementadores.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/encontro.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-proto/src/uri.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-proto/src/encontro.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/alcance/encontro.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/quarto.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/quarto_pelo_link.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/ocupante_da_escuta.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/volta_pela_trilha.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/enlace.rs (só o módulo de testes)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-plano-1a-o-quarto.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-plano-1e-a-api-congelada.md`

Ordem sugerida: a consulta (T5a, T5c, M2, T6f, C1, T5d), depois as marcas (T1a, T1b, T1c, T2c/T3c/M3), depois os testes de conformidade (T4a, T7a, T8b/T8d, 1B), e por fim o 19 (C4, T5e, T3d/T3e, T7b e os planos).

1. A consulta, em seele-core/src/encontro.rs.
- T5a e T5c: extrair de `resolver_ponto` a função `primeiro_do_ponto(achados: Vec<SocketAddr>, ponto) -> Option<SocketAddr>`, com IPv4 primeiro e a linha «nenhum endereço» quando vazio. Teste: [[::1]:8384, 127.0.0.1:8384] dá 127.0.0.1, e [] dá None com a linha. Apagar o `sort_by_key` (:362) ou a linha do vazio reprova.
- M2 do 19: se nenhuma pergunta sair pela família do primeiro endereço, a consulta tenta o seguinte dentro do mesmo `ate`, com teste da função de escolha. Ele reprova com a escolha de hoje.
- T6f: a decisão de depois do envio vira uma função pura, e `saidas == 0` (:829-831) só sai do laço com `enviados == 0`. Depois da volta 1, a volta recusada conta como volta (`enviados += 1`, `proxima_volta += INTERVALO_DOS_ENVIOS`, :832-833), para não virar laço quente. Testes: a função pura reprova com o `break` de hoje; um teste de `consultar_por` com o envio recusado na volta 2 e a resposta tardia à volta 1 dá Achado em vez de PontoMudo.
- C1: testes unitários de `teto_depois_da_resposta` (:406). O piso: 1 ms de ida e volta dá `primeira + 100 ms`. O dobro: 200 ms dá `primeira + 400 ms`. A segunda marca dentro do teto e o ONDE perdido ganham testes de `consultar_por` com um ponto falso, no molde de :1046.
- NÃO mexer no doc de `onde_mora_hoje` (:666-667): o lote M2 (P51-18) o reescreve com o comportamento que este lote deixar.
- T5d, em seele-proto/src/uri.rs: `separar_ponto("")` passa a recusar com `EnderecoInvalido`, com teste. `Bilhete::ler` já recusa ponto vazio (:289).

2. As marcas, em seele-proto/src/encontro.rs.
- T1a: `do_servidor` passa a conferir o texto inteiro, letra e número, como o doc de :270-271 já promete. O teste pede None para `0123456789abcdef!!!`, que o HEAD aceita.
- T1b: um teste com um `aviso` de 16 caracteres.
- T1c: os campos de `Marcas` passam a privados, com `aviso()`, `escuta()` e `servidor()`, sem construtor de fora. Isso alcança 6 leituras em seele-core e 14 no seele-server. O `Marcas { .. }` do teste de alcance/encontro.rs:1563 passa a sair de `do_servidor`, porque um construtor `cfg(test)` não atravessa crates. O seele-ffi só chama `do_servidor` (lib.rs:8054) e não muda.
- No laço de `a_escuta_nunca_tem_a_marca_do_aviso` (:859-885), `{impressao}` na mensagem de cada asserção.
- T2c, T3c e M3: testes em alcance/encontro.rs que capturam os pedidos da subida (MORO, ONDE e LEVE, :500-501 e :528) e conferem a marca de cada um.

3. Conformidade.
- T4a: quarto_pelo_link.rs:268-276 guarda o último erro e o põe no `panic!`.
- T7a: um caso com um ponto que responde lixo, que tem de dar PontoMudo (quarto.rs:104), ou o teste muda de nome para o que ele mede.
- T8b e T8d: distinguir a escuta None da FFI da do anfitrião, e ler de volta o `ponto/aviso` do `enc=` (quarto_pelo_link.rs:391).
- 1B:
  - uma montagem de certificado compartilhada no módulo de testes de enlace.rs, no lugar de `ponta_com_certificado_proprio` (:6307) e `servidor_que_aperta_a_mao_e_cai` (:6624);
  - `ponto_que_conta` (volta_pela_trilha.rs:88-100) passa a contar só os datagramas que `seele_proto::encontro::analisar` lê como `Pedido::Leve` com a marca do aviso esperado. Trocar a marca do LEVE em `Batida::preparar` reprova a asserção de :228, que hoje passa;
  - `expect` com motivo no lugar de `.ok()?` (:74-75, :89-90, :134-140, :303-304).

4. P51-19.
- T5e: em alcance/encontro.rs:560, `seele_proto::uri::separar_ponto(texto).ok().map(|a| (a.maquina.to_owned(), a.porta))`, com o comportamento de antes. O comentário de :557-559 deixa de falar do `Bilhete`.
- C4: `mandar_pelo_server` (:729) devolve um erro tipado, com `passageira` para `ErrorKind::WouldBlock`. `registrar` (:881-896) o repassa, e `anotar` (:914-931) não muda de estado numa falha passageira. Teste com captura: Err(passageira) seguido de Ok não deixa linha nenhuma; no HEAD saem duas linhas info.
- T3d e T3e: constantes de teste compartilhadas.
- T7b: um ADENDO no ADR 0047, no molde do ADR 0003 (9f6f90e, «## Adendo — 2026-09-29»), dizendo que `onde_mora` virou `onde_mora_hoje` e onde ele mora hoje. A linha :83 não muda, porque :85 diz «como ele está no commit».
- Planos 1A (:23, :923, :1076, :1223, :2749) e 1E (:57, :124, :291, :543, :546-551): uma nota no topo de cada seção citada dizendo o que a execução trocou (o G1 de hoje é o do índice e do plano 1B; `fn restaurar` virou `AmbienteDoEncontro`; `PUBLICADAS_ANTES_DO_GUARDA` e `$tmp` viraram o que o Ruling da Task 1 do 1E diz), sem reescrever o histórico.
- O T5f fica de fora: o resto dele fechou em 27f86af, e o Espiao do seele-ffi é opcional e mexeria noutro crate.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core --lib encontro`, `… -p seele-proto`, `… -p seele-server --lib alcance`, `… -p seele-conformance --test quarto --test quarto_pelo_link --test ocupante_da_escuta --test volta_pela_trilha`, mais fmt e clippy -D warnings. Rode a reversão de cada guarda novo antes do commit.

NÃO fazer:
- não mudar o fio: as marcas continuam saindo dos mesmos 16 caracteres, e o SEELE-ENC/1 não muda;
- não criar construtor de `Marcas` para teste entre crates;
- não reescrever a linha :83 do ADR;
- não mexer no doc de `onde_mora_hoje`, em hospedagem.rs nem em lib.rs do seele-server (lote M2);
- não editar pendências nem o índice.

**Riscos.** T5d, T6f e T1a são código de produto no seele-proto e no seele-core, mas só mudam a aceitação local, e um cliente 0.15.0 não muda. O T6f muda o tempo da consulta só quando um envio é recusado depois da volta 1: em vez de PontoMudo na hora, ela espera a resposta tardia até a volta seguinte, o teto ou o prazo. O M2 muda o custo da consulta só quando a primeira família não tem rota. O T1c alcança quatro crates. O lote é grande: se não fechar numa sessão, a divisão natural é parar depois do passo 2 e deixar o 3 e o 4 para uma segunda sessão do mesmo lote, antes do M2.

## Lote K3-a-gestao-diz — A gestão de MODs diz quem desenha de verdade, o modo de desenvolvedor diz o que cortou, e o polimento do 1C e do 1D

Ordem 12. Itens: P49-9, P49-10, P51-23. Por que juntos: Os três mexem em camada-mods.js, na frase da gestão (`oQueAGestaoDiz`) e no desenho dos contornos, e são provados na mesma bancada contribuicoes-e-camadas.cjs. O P51-23 também toca base.js, mods-regiao.js, frontend.rs e ci.yml: por isso o lote vem depois do K2 (base.js), do H (frontend.rs) e do L1 (ci.yml).

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/mods-contribuicoes.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/camada-mods.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/base.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/mods-regiao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/contribuicoes-e-camadas.cjs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/diagnostico-de-mods.cjs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/frontend.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/.github/workflows/ci.yml`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-plano-1c-mods-no-log.md`

1. P49-9.
- Com a escolha automática e alguém substituindo o alvo vazio, `resumo` (mods-contribuicoes.js:504-530) passa a somar `quemPinta(ponto, '', estaDePe)`.
- O campo novo `tambemDesenham` são os que vencem noutro alvo, fora o escolhido. `preteridos` junta a disputa do alvo vazio com `perderam`.
- `oQueAGestaoDiz` (camada-mods.js:750-830) diz «automática: mod/a para todos, mod/b para quem declarou», com a nota «Também pediram este lugar e não o receberam: mod/e. Escolha abaixo qual deve desenhar.».
- Prova: na bancada, que já roda `oQueAGestaoDiz` sobre o registro de verdade (:1181 e :1482), os casos de mod/a, mod/b e mod/e (o comando da pendência, pendencias.md:7495-7529). Tirar `tambemDesenham` traz de volta «apresentado por mod/a (escolha automática)», e o caso fica vermelho. O caso parcial de :1400-1421 não muda.

2. P49-10. No ramo `linha.escolhidoNaoSubstitui` (camada-mods.js:773-774), com o ponto `pessoa.cartao` e `cartoesDosMods.has(id)`, a frase passa a «você escolheu mod/d: o cartão é do SEELE, e os cartões de mod/d (API 3) valem sozinhos». Prova: um caso com mod/d da API 3. Sem o ramo, a frase volta a «… — o SEELE desenha», e o caso fica vermelho.

3. P51-23.
- Contornos: em `desenharContornos` (camada-mods.js:1169), contar os nós que passam de `TETO_DE_CONTORNOS` em vez do `break` de :1194. Havendo, pôr ao lado do aviso de :1231-1235 a nota «N pontos não contornados (teto de 300)».
- Recorte vazio: o critério é a interseção com o recorte, e não o tamanho zero da caixa, porque o contêiner vazio é contornado de propósito (:1195-1197 e camada-mods.css:434-435). Um nó com área cuja interseção com o recorte tem largura ou altura zero não ganha caixa. O `r.bottom < cima` de :1215 passa a excluir a igualdade.
- Recusas do pedido: os quatro `registrarNoAnfitriao('pedido-de-mod', `${id}: …`, 'aviso', id)` (base.js:542, :546, :552, :558) passam por um ajudante. NO MESMO COMMIT, o guarda frontend.rs:11848 passa a conferir o ajudante (o id na quarta posição e 'aviso' na terceira) e que `pedirAoServidor` o chama. O guarda de hoje reprova com «deixou de chamar».
- ci.yml:334: «as bancadas de navegador», sem número.
- Plano 1C (:1458): o nome de hoje do guarda, `os_quatro_comandos_de_midia_passam_pela_recusa_dita`.
- Docs de `LIMITES_DO_CARTAO` (mods-regiao.js:128-130): numa contribuição, os bytes valem por destino.
- Os outros subitens (as duas definições de «classe que o script aplica», a frase «nenhum script cria» em frontend.rs:15221/:15227, a anotação de bytes sem números, os comentários da bancada e da aba, Docs-m5/m7) seguem o texto de pendencias.md:8090-8108.
- Prova dos contornos e do recorte, em diagnostico-de-mods.cjs (`oModoDeDesenvolvedorContornaSemTomarNada`, :840): com mais de 300 contêineres, a nota diz quantos ficaram de fora; um contêiner com área recortado a zero não ganha caixa; um vazio dentro do recorte continua ganhando. Essa bancada é de navegador e não está no check-runtime. Rode-a aqui, sem baixar nada, com `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright node apps/seele-app/bancada/diagnostico-de-mods.cjs`, inclusive a reversão de cada caso, antes do commit.

Comandos: `cargo xtask check-runtime`, `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --test frontend` e a bancada de navegador acima.

NÃO fazer:
- não apagar o contorno do contêiner vazio;
- não trocar as chamadas pelo ajudante sem reescrever o guarda no mesmo commit;
- não mexer em nada do ci.yml além do comentário de :334;
- não instalar Playwright pela rede;
- não editar pendências nem o índice.

**Riscos.** A nota dos contornos é frase de tela, só no modo de desenvolvedor. A prova dos contornos não roda em nada automático, só no job manual `bancadas` (fora) e aqui à mão. Nenhum par de MODs publicados provoca o caso do P49-9 hoje. Nada no fio, na API ou na CSP.

## Lote K4-som-do-pacote — O som do pacote atravessa a ponte uma vez só, e um som grande demais é recusado antes de decodificar, como «recusada», com os números

Ordem 13. Itens: P49-7b, P49-7a. Por que juntos: Os dois mexem nas mesmas funções de som do main.rs (`midia_declarada_do_mod`, `som_declarado_do_mod`, `ler_midia_do_servidor`) e em `montarSom`, em mods-regiao.js. O 7b é mais simples e limpa o caminho que o 7a passa a conferir. O lote mexe no main.rs e em base.js/mods-regiao.js, e por isso espera o D e o K3.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/base.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/mods-regiao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/regiao-do-mod.cjs`

1. P49-7b.
- `midia_declarada_do_mod` (main.rs:5398-5416) confere o tipo por `ler_tipo` (seele-core/src/mods.rs:347) antes de montar o `data:`.
- Com o papel `som`, devolve o papel e o tamanho, sem `uri`, depois das mesmas conferências de pacote, de arquivo declarado e de teto. O campo vira `Option<String>`, omitido na serialização. Só a imagem passa por `ler_midia`.
- Saem os dois comentários «atravessa duas vezes» (base.js:953 e mods-regiao.js:2289). O doc de `midia_do_mod` (main.rs:5429-5430) deixa de citar `recursosDoMod`, que não existe.
- Teste: com o pacote de teste que os testes de som já montam (main.rs:12395, `som/toque.wav`), a resposta para um WAV não tem `uri`, e a de uma imagem tem. Voltar o `uri` do som deixa o teste vermelho. A regiao-do-mod.cjs continua verde.

2. P49-7a.
- Junto de `som_declarado_do_mod`, a função pura `bytes_decodificados_minimos(bytes) -> Option<u64>`. Ela devolve um PISO do que o som ocupa decodificado a 48 kHz em float32 (quadros × 48000/taxa × canais × 4), e nunca uma estimativa que possa passar do real. O parse do cabeçalho é feito à mão, sem crate:
  - WAV: exato, pelo `fmt ` e pelo `data`;
  - OGG: pela granule da última página e pela taxa e canais do cabeçalho de identificação Vorbis. No Opus a granule já está a 48 kHz, menos o pre-skip;
  - MP3: exato pelo quadro Xing/Info quando houver; senão, o piso pela taxa máxima da versão e camada do primeiro quadro (320 kbps no MPEG-1 camada III) e pelos canais dele.
- Acima de 64 MiB, a recusa sai antes de servir, com o motivo `som-grande-demais-decodificado` e os números, pelo mesmo `recusa_de_midia_dita`. Isso vale em `som_declarado_do_mod` e, para o papel `som`, em `ler_midia_do_servidor` (:5629), porque o som do servidor tem o mesmo pico (`midia_em_bytes` → `bufferDaUri`).
- A janela diz essa recusa como o teto, e não como falha de carga. Em `montarSom` (mods-regiao.js:2363-2369), esse motivo segue o caminho do teto: figura «cheia», `recusada` ao MOD com os números no `porque`, anotação e registro. É o que o api/README.md:71-76 promete e o que diz o comentário de mods-regiao.js:2385-2389.
- Um guarda lê mods-regiao.js e falha se `TAXA_DA_DECODIFICACAO` (:3034) e `bytesDeSomDecodificado` (:89 e :149) divergirem das constantes repetidas no Rust.
- O doc de `LIMITES_DA_REGIAO` (:83-86) passa a dizer que o pico é cortado antes, no Rust, quando o cabeçalho permite.
- Testes em main.rs, um por braço, com bytes sintéticos montados no teste:
  - um WAV de 8 bits, mono, 8 kHz, do tamanho do teto de arquivo, é recusado (piso ≈ 250 MB), e um curto passa;
  - um OGG Vorbis com granule de três minutos em estéreo é recusado;
  - um MP3 com quadro Xing de muitos quadros é recusado, e um MP3 mono sem Xing, do tamanho do teto, passa.
  Reversão por braço: devolver None naquele formato deixa passar o caso grande dele.
- Na regiao-do-mod.cjs: `bytesDoSom` rejeitando com `{ Recusado: { motivo: 'som-grande-demais-decodificado …' } }` faz o MOD ouvir `recusada` (e não `falhou`), e a figura fica «cheia». Sem o mapeamento, o MOD ouve `falhou`, e o caso fica vermelho.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app som`, `cargo xtask check-runtime`, mais fmt e clippy -D warnings.

NÃO fazer:
- não acrescentar crate de áudio;
- não recusar por estimativa que possa passar do real: só o piso recusa;
- não mudar a promessa pública do api/README (recusada, com os números);
- não mexer na lógica de som de contribuição (lote K5);
- não mudar o nome do comando `midia_do_mod`, que o api/v5.json mapeia;
- não editar pendências.

**Riscos.** Nos formatos comprimidos só há estimativa. Por isso só o piso recusa, e um som que o piso não alcança segue como hoje, decodificado e conferido na janela. A ponte interna da casca muda (`uri` some do som), e nenhum leitor de `uri` de som do pacote foi achado em ui/. Não muda o fio nem a API congelada. O WebView2 e o WKWebView não se testam aqui.

## Lote M2-encontro-ambiente-e-textos — O ambiente do encontro nos testes devolvido com trava, a linha do quarto com levou=, e os docs do 1B que dizem outra coisa

Ordem 14. Itens: P51-16, P51-18, P51-20. Por que juntos: Os três mexem no mesmo bairro do M1 (alcance/encontro.rs, seele-core/encontro.rs, seele-proto/encontro.rs, enlace.rs, volta_pela_trilha.rs) e por isso vêm depois dele. O P51-18 é o dono do doc de `onde_mora_hoje`, que tem de descrever o que o T6f do M1 deixou. O P51-16 e o P51-20 tocam hospedagem.rs, lib.rs do seele-server e types.rs, que o G e o I editam depois.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/hospedagem.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/alcance/encontro.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/encontro.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/enlace.rs (só doc)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/types.rs (só doc)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-proto/src/encontro.rs (só doc)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-encontro/src/lib.rs (só doc)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/alcance-pela-internet.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/ponto-de-encontro.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/volta_pela_trilha.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-plano-1b-a-impressao-no-tls.md (só :99)`

1. P51-16.
- Em seele-server/src/lib.rs, `#[cfg(test)] pub(crate) mod ambiente_de_teste` com `AmbienteDoEncontro` (guardar, pedir, e um Drop que devolve), no molde de quarto_pelo_link.rs:141-165.
- Ele segura, COMO CAMPO, um `MutexGuard<'static, ()>` de um `static TRAVA: Mutex<()>`, tomado com `unwrap_or_else(PoisonError::into_inner)`.
- Tem de ser campo, e não um `let _t = TRAVA.lock()` solto: o teste de hospedagem.rs é `#[tokio::test] async` e segura o guarda através de `.await`. O clippy do CI (`-D warnings`, ci.yml:120) reprova um guarda solto por `await_holding_lock` e não reprova o struct (medido com o clippy 1.97 desta máquina).
- hospedagem.rs:539-570 e alcance/encontro.rs:1286-1324 passam a usá-lo. O comentário de :1294-1295 deixa de prometer a serialização.
- Teste com `catch_unwind`, no molde de quarto_pelo_link.rs:169-196: esvaziar o Drop reprova. Para a corrida: `cargo test -p seele-server --lib -- --test-threads=8` em laço.

2. P51-18.
- Em `onde_mora_hoje` (seele-core/src/encontro.rs:681): `let comecou = tokio::time::Instant::now();`, e a linha final ganha `levou = ?comecou.elapsed()`.
- O doc (:666-667, e o custo de cada caso) é reescrito com o comportamento que o M1 deixou, T6f inclusive: «como PontoMudo se nada respondeu antes; se o ponto já respondeu, como NinguemMora, ou Achado com o que chegou».
- Prova: em `um_nome_que_nao_resolve_no_prazo_diz_no_rastro_que_foi_o_prazo` (:1404), exigir uma linha com «onde o anfitrião mora hoje» e «levou=». No HEAD ela reprova.

3. P51-20 (texto).
- C-M3 (seele-ffi/src/types.rs:1923): num alvo de LAN, a promessa pode estar certa e quem atendeu ser outro servidor no mesmo endereço.
- C-M4 (seele-proto/src/encontro.rs:186): «guardou a impressão que a conexão aceitou».
- C-M5 (plano 1B:99): a prova é do teste da FFI, e o fixture de convite.rs passa `true` à mão.
- C-R1-m1, em seele-encontro/src/lib.rs:30, docs/alcance-pela-internet.md:287 e :325 e docs/ponto-de-encontro.md:18 e :48: «até o quarto encher e chegar uma marca nova (`Quarto::morar`)».
- T4 (volta_pela_trilha.rs:352): só a frase «dezenas de milissegundos» vira «microssegundos (medido: 4 µs)». A frase dos 20,0 s do mesmo parágrafo é do lote F (P50-1).
- Pino: o doc de `desfazer_pin_orfao` (enlace.rs:4911-4923) diz o segundo resíduo (dois candidatos na mesma chave, e a linha do pin órfão levando a falha do vizinho) e que os dois falham para o lado seguro.
- Prova: o bloco de greps da pendência (:8056-8063) deixa de achar as frases.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server --lib`, `… -p seele-core --lib encontro`, `cargo doc -p seele-proto --no-deps`, mais fmt e clippy -D warnings.

NÃO fazer:
- no plano 1B, só a região de :99 (o lote Z edita :2875-:2899);
- não mexer na abertura de ponto-de-encontro.md sobre os 60 s, que é a cadeia do Lote C no G1;
- não pôr trava nos outros 18 testes que chamam `Hospedagem::iniciar`: fica registrado como risco;
- não editar pendências nem o índice.

**Riscos.** Os outros testes do seele-server que chamam `Hospedagem::iniciar` também leem `$SEELE_ENCONTRO` e continuam sem trava: 20 chamadas, 18 fora do teste que troca a variável. Eles podem pagar o PRAZO na janela do BURACO_NEGRO. O guarda torna o futuro do teste `!Send`, o que o `#[tokio::test]` de uma thread aceita. P51-18 e P51-20 são texto e uma linha INFO. Nada toca o fio.

## Lote E-geracao-morta — Todo comando de uma geração morta é contado, e ler_imagem_mod diz cada recusa

Ordem 15. Itens: P51-09, P51-11. Por que juntos: O P51-11 usa a `recusar_se_morta` que o P51-09 cria, nas duas conferências de geração de `ler_imagem_mod`, e a pendência pede os dois juntos. Os dois mexem só no main.rs, depois do lote C, que mudou `recusa_de_midia_dita`.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`

1. P51-09.
- Criar `Session::recusar_se_morta(&self, geracao) -> Result<(), FalhaNoMod>`, que confere, conta em `comandos_de_geracao_morta`, escreve a linha DEBUG de `confere_geracao` e devolve `sessao-encerrada`.
- Trocar por ela as TREZE conferências `if !session.geracao_vale(geracao) { … }` dos comandos: as sete sem conta (mod_nativo_colher :4230, mod_nativo_entregar :4279, enviar_imagem_mod :5682 e :5710, ler_imagem_mod :5727 e :5737, e a segunda de escolher_para_o_mod :5899) e as seis que contam à mão (:5182, :5444, :5521, :5600, :5793, :5945). As linhas são do HEAD e mudam depois do C: procure pelo texto.
- `liberar_reserva` recebe `mortos: &AtomicU64`, como `registrar_reserva`, e `mod_nativo_ativar` passa `&session.comandos_de_geracao_morta`.
- Tirar a ressalva do doc em :3936-3940. A frase da tela (camada-mods.js:1641) fica a mesma.
Testes:
- (a) `liberar_reserva` com `geracao_vale = || false` faz `mortos` subir de 0 para 1. Primeiro a assinatura muda sem o `fetch_add`, e o teste fica vermelho; depois entra o `fetch_add`;
- (b) guarda de fonte, sem comentários, como `os_quatro_comandos_de_midia_passam_pela_recusa_dita`: nenhum corpo de `#[tauri::command]` tem `!session.geracao_vale(`. No HEAD ele aponta treze;
- (c) `mod_nativo_colher` e `mod_nativo_entregar` com a geração morta fazem o contador subir.
Reversões: voltar uma conferência ao `if` cru reprova (b), e tirar o `fetch_add` de `liberar_reserva` reprova (a).

2. P51-11.
- Extrair a parte de depois do `await`: `fn imagem_lida_dita(id: &str, lida: Result<Vec<u8>, String>) -> Result<MidiaDoMod, FalhaNoMod>`. As duas recusas, a do servidor e a de `formato-desconhecido`, passam por `recusa_de_midia_dita`, com os baldes e o instante que o lote C pôs nela.
- As duas conferências de geração passam por `recusar_se_morta`.
- A `connection()` que falha (:5730-5732) passa por `recusa_de_midia_dita` com `sessao-encerrada`, na linha DEBUG «a sessão acabou». Ela só conta se a geração estiver morta (`recusar_se_morta` antes dela): uma conexão ausente com a geração de pé não é comando de sessão encerrada.
- O guarda de fonte (:12274-12301) passa a exigir `imagem_lida_dita(` em `ler_imagem_mod`.
Teste de rastro de `imagem_lida_dita`:
- `Err("Prazo esgotado")` dá um WARN com `mod_id=` e `motivo="Prazo esgotado"`;
- `Ok(b"so texto")` dá um WARN com `motivo=formato-desconhecido`;
- `Ok(PNG)` dá Ok sem linha.
Tirar uma das duas chamadas deixa o caso dela vermelho, que é o que o guarda de hoje não pega.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app geracao`, `… -p seele-app imagem`, mais a suíte da lib do seele-app, fmt e clippy -D warnings.

NÃO fazer:
- não contar como geração morta a conexão ausente com a geração de pé;
- não mudar a frase da tela;
- não mexer nas portas do registro (C) nem no som (K4);
- não editar pendências nem o índice.

**Riscos.** Nenhum no fio, na API ou na CSP. O contador passa a subir mais, e só aparece no modo de desenvolvedor. A rede fica fora dos testes, porque a função extraída é pura.

## Lote K5-som-da-contribuicao — O som de uma contribuição só decodifica quando o MOD manda tocar, e muda de destino quando quem o tomou sai da tela

Ordem 16. Itens: P49-7c, P49-8. Por que juntos: Os dois mexem na mesma marca de sons da contribuição (`sonsDaContribuicao.tomados`, `tomouOsSons`) e em `montarSom`. O 7c adia a decodificação até o `tocando`, e o 8 devolve a marca e remonta noutro destino. Feitos juntos, há uma regra só de quem toma e quando decodifica. O lote vem depois do K4, que mexeu em `montarSom`.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/mods-regiao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/base.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/bancada/regiao-do-mod.cjs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/api/README.md`

1. P49-7c, a parte daqui. O teto por MOD fica com o dono.
- Uma contribuição declarada com `tocando: false` não decodifica até o MOD declarar `tocando: true`. Numa contribuição não há botão.
- `montarSom` toma a marca como hoje, antes de qualquer `await`, mas adia o pedido a `som_do_mod` e o `decodeAudioData`. A figura assenta, e a anotação diz «pronta» com o tamanho do arquivo, porque o Rust já conferiu papel e teto de arquivo.
- Quando o pedido chega, a decodificação roda e o teto do bolso vale ali, com a mesma recusa: figura «cheia» e `recusada` com os números.
- Docs:
  - `LIMITES_DA_REGIAO` (mods-regiao.js:67) diz que cada superfície tem o seu bolso;
  - `LIMITES_DO_CARTAO` diz que o som de uma contribuição só é decodificado quando o MOD o manda tocar;
  - api/README.md:71-76 ganha que, numa contribuição, a recusa pelo teto chega quando o MOD pede para tocar.
- Prova: na regiao-do-mod.cjs, com o `sonsDaContribuicao` que a bancada já monta (:2398-2401), uma contribuição com `tocando: false` não chama `bytesDoSom` nem `decodeAudioData`; depois de um `tocando: true`, chama. Decodificar na montagem deixa o caso vermelho.

2. P49-8.
- Quando o destino que tomou os sons sai da tela (o nó dele desconectado, visto em `calarOsSonsQueSairamDaTela`, base.js:1479-1489) enquanto a contribuição ainda desenha noutro destino conectado, o renderer dele solta os tocadores, devolve ao bolso o que o som decodificado ocupava, esquece as anotações desses sons e devolve a marca (`sonsDaContribuicao.tomados = false`, `tomouOsSons = false`).
- Redesenhar não basta, porque a reconciliação reaproveita a figura que assentou sem tocador (base.js:1259-1261). O renderer que assentou um som sem tocador guarda esse adiamento, e um destino conectado o remonta, na mesma varredura, quando a marca volta: toma a marca e pede os bytes, ou, pelo 7c, só quando houver `tocando`.
- A recusa `SOM_FORA_DA_TELA` (mods-regiao.js:2532-2534) deixa de sair nesse caso.
- Quando a pessoa do primeiro destino volta, o nó dele reaparece sem tocador.
- Prova: dois renderers com o mesmo `sonsDaContribuicao`. O primeiro toma o som; o nó dele sai do documento e a varredura roda; um `tocando: true` religado toca no segundo, sem recusa, e só existe um tocador. Num segundo passo, o nó do primeiro volta, e continua um tocador só. Reversões: não devolver a marca traz de volta a recusa de `SOM_FORA_DA_TELA`; devolver a marca sem remontar no segundo deixa o `tocando` sem tocador.

3. Opcional, a parte daqui do P51-25: em mods-regiao.js:2364-2366, quando `typeof this.dono.bytesDoSom !== 'function'`, o `falhou` diz «o dono desta região não oferece bytesDoSom» no lugar do TypeError genérico, com um caso na regiao-do-mod.cjs.

Comando: `cargo xtask check-runtime`.

NÃO fazer:
- não criar teto de som por MOD: escolher quanto som um MOD segura e mudar a frase pública é do dono;
- não mudar a API congelada;
- não decodificar na montagem;
- não editar pendências.

**Riscos.** O primeiro toque de uma contribuição passa a ter a latência da decodificação, e a recusa pelo teto passa a chegar no pedido de tocar, e não na montagem. Uma troca de tocador no meio de um som o faz recomeçar. O pior caso (muitas contribuições tocando ao mesmo tempo) continua sem teto por MOD, e isso fica com o dono. A frase do api/README é pública, mas pela regra que o lote Z escreve não ganha cadeia.

## Lote F-saida-com-despedida — Quem sai pelo app some do roster dos outros em menos de dois segundos, e a saída do app para os MODs e diz isso no seele.log

Ordem 17. Itens: P50-1, P50-2, P50-3. Por que juntos: É o mesmo caminho de saída. O P50-1 faz o `Enlace::sair` esperar a despedida. O P50-2 faz a casca esperar a thread da conexão, no lugar da espera cega de 150 ms. O P50-3 põe os MODs parando antes, na mesma `despedir_se`. Os três mudam `despedir_se` e o guarda de texto apps/seele-app/tests/encerramento.rs, que nenhum inventário citou e que reprova quando o `sleep` sai.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/enlace.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/encerramento.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/saida_com_despedida.rs (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/tela_por_um_par.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/volta_pela_trilha.rs (só o doc de :346-353)`

1. P50-1 (seele-core/src/enlace.rs).
- Constante `ESPERA_DA_DESPEDIDA` (300 ms), com doc: o CONNECTION_CLOSE sai na primeira volta do driver, e a espera só limita o escoamento (3×PTO).
- Os dois braços `Some(Comando::Sair) | None => return self.encerrar(Motivo::Pedido)` (:2544 e :2553) passam a `return self.despedir().await`.
- `async fn despedir(&mut self)` faz, nesta ordem:
  - guarda `self.cliente.as_ref().map(Client::endpoint)`;
  - cancela todas as `TarefasDePar` e espera, com teto de cerca de 100 ms, que as alças abortadas devolvam. Um `abort` não interrompe um poll que já corre noutra thread, e as baterias rodam em `multi_thread`;
  - chama `self.encerrar(Motivo::Pedido)`;
  - espera `tokio::time::timeout(ESPERA_DA_DESPEDIDA, ponta.wait_idle())`.
- `Enlace::sair` passa a `&mut self`: manda o Sair, espera a tarefa do motor com teto de cerca de 1 s e aborta se vencer. A alça vira `Option<JoinHandle>` com `take()`, e o Drop aborta a que sobrar. Os três chamadores já têm `mut` (lib.rs:3939, bateria_interna.rs:436, tela_por_um_par.rs:2752).
- `drive` (seele-ffi lib.rs:4197) não muda.
- O doc de `esperar_o_servidor_fechar` (volta_pela_trilha.rs:346-353) deixa de dizer que o `disconnect` custa 20,0 s. A frase dos «microssegundos» já veio do M2.
Provas, em crates/seele-conformance/tests/saida_com_despedida.rs (novo, com `vaga::minha()`):
- (a) `quem_sai_pela_ffi_some_do_roster_dos_outros_em_menos_de_dois_segundos`: um Daemon em memória e um observador `Enlace` B. A entra por `seele_ffi::Connection::connect` (audio: false) em `spawn_blocking`, como `conectar` (volta_pela_trilha.rs:172-178). Depois de `a.disconnect(); drop(a)`, B recebe `PersonGone` e `presentes` perde A em menos de 2 s;
- (b) `o_enlace_que_sai_num_runtime_que_cai_em_seguida_ainda_avisa`: o Enlace numa `std::thread` com runtime current_thread, que termina logo depois de `sair().await`, e o roster cai em menos de 2 s;
- (c) em tela_por_um_par.rs, depois de `sair().await`, ler a fila com `proximo()` em prazo zero até o `Encerrado` e, SEM drenar, exigir que nenhum `TelaQuadro` venha depois dele. O teste de hoje drena 300 ms (:2341, :2350-2365) e não prova a ordem.
Reversões: voltar os braços a `self.encerrar(...)` e tirar a espera de `sair` faz (a) e (b) voltarem aos ~20 s; tirar o cancelamento de antes do aviso reprova (c).
O teste da outra sessão (worktree eloquent-blackburn-a0bf2c, `medida_do_disconnect.rs`, sem commit) tem um relé UDP que conta datagramas e pode reforçar a prova. Só ler, não copiar sem rodar.

2. P50-2.
- Em seele-ffi/src/lib.rs, a thread da conexão (:1346-1366) avisa por um `std::sync::mpsc::Sender<()>` logo depois de `runtime.block_on(drive(..))` voltar, e antes de o runtime cair.
- O `Receiver` mora na `Connection` num `Mutex<Option<Receiver<()>>>`.
- `pub fn encerrar_e_esperar(&self, teto: Duration) -> bool` chama `disconnect()` e faz `recv_timeout(teto)`: `Ok` ou `Disconnected` dão true, e só `Timeout` dá false.
- Prova: `encerrar_e_esperar_volta_com_o_aviso_ja_dado` roda o próprio binário como filho (padrão de seele-audio/tests/realtime_safety.rs:168-190). O filho conecta pela FFI ao servidor do pai, faz `assert!(a.encerrar_e_esperar(500 ms))` e `std::process::exit(0)` na linha seguinte. O pai exige que `presentes` perca a pessoa em menos de 2 s. Uma `encerrar_e_esperar` que volte sem esperar o sinal reprova.

3. P50-3, em main.rs.
- `despedir_da_sessao(sessao: &Session, teto)`, sem AppHandle:
  - `sessao.revogar()` primeiro, como `desmontar_o_cliente`, com `tracing::info!(mods = n, "saída do app: MODs nativos mandados parar")`;
  - espera com teto de cerca de 200 ms, sem segurar o cadeado de `mods_nativos` entre as conferências, que `encerrando` esvazie;
  - por fim, `encerrar_e_esperar(Duration::from_millis(500))`. Se ele voltar false, `tracing::warn!("a despedida não terminou em 500 ms: quem ficou vai notar a saída pelo prazo de ociosidade, 20 s")`.
- `despedir_se` (:8889-8898) passa a chamá-la. O doc dela descreve o que acontece de verdade.
- Em `assentar_fala` (:4018-4047), o `Parou` sobe de debug para `info!` com «MOD nativo parou» (mod_id, instancia, geracao).
- Testes: (1) com `Session::default()` e uma instância nativa reservada (`instancia(geracao)`, :9451), depois de `despedir_da_sessao` a instância está em `encerrando`, e a captura (`capturar`, :10566) tem a linha info. (2) `assentar_fala` com `ParaOFora::Parou` tem INFO e «MOD nativo parou». Reversões: tirar o `revogar()` reprova (1), e voltar a `debug!` reprova (2).

4. apps/seele-app/tests/encerramento.rs (guarda de texto).
- `fechar_a_janela_avisa_o_servidor` mantém `RunEvent::ExitRequested` antes de `despedir_se(handle)`, e troca a exigência de `connection.disconnect()` por `despedir_da_sessao(` e `encerrar_e_esperar(`.
- `a_despedida_espera_o_quadro_sair` troca o `sleep` por `encerrar_e_esperar(` com um teto.
- O doc do módulo diz que a prova de comportamento agora mora em seele-conformance/tests/saida_com_despedida.rs.
- Reversão: voltar `despedir_se` ao `disconnect()` com `sleep` reprova os dois.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-conformance --test saida_com_despedida -- --test-threads=1`, o mesmo com `--test tela_por_um_par` e `--test bateria_interna` (`sair_encerra_sem_esperar_a_bateria`), `… -p seele-app --test encerramento`, `… -p seele-app despedir`, mais fmt e clippy -D warnings.

NÃO fazer:
- não mudar o fio: o CONNECTION_CLOSE é do QUIC, com o EJECTED de `Client::disconnect`;
- não sinalizar depois do `drop(runtime)`, que espera a piscina de bloqueio sem prazo;
- não segurar o cadeado de `mods_nativos` enquanto espera;
- não apagar os guardas de encerramento.rs: reescreva-os;
- não mexer na hospedagem (lote G);
- não editar pendências: o lote Z fecha a pendência 50 com estes commits.

**Riscos.** O fio não muda, e um servidor 0.15.0 já trata o fechamento do par. A saída voluntária segura a thread da conexão por até cerca de 400 ms, e o ExitRequested a thread principal por até cerca de 700 ms (200 dos MODs e 500 da conexão; com o G, mais 800). Windows e Linux não se testam aqui. Que o roster cai em menos de 2 s foi deduzido da leitura do quinn 0.11.11: quem mede é o teste novo. Os MODs da janela continuam morrendo com o WebView, como hoje.

## Lote J-primeiro-uso — A tecla de falar com nome, o aviso de fone e o nome pedido no primeiro uso

Ordem 18. Itens: F1-tecla-fone-nome. Por que juntos: É um item só, e de tela, mas espalhado por oito arquivos de ui/ e pelo frontend.rs. Ele divide frases.js, tela-sessao.js e frontend.rs com o G e o I, e base.js com os K. Por isso fica sozinho, na onda em que esses arquivos estão livres.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-sessao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-server.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/base.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/frases.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/camada-ajuda.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/camada-nomear.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-boot.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/index.html`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/frontend.rs`

1. A tecla.
- `nomeDaTecla` (tela-server.js:1523-1545) é movida para base.js, que carrega antes das duas telas (index.html:4317), e usada nos dois lugares. O comentário dela (:1515) continua dizendo que é o único lugar que nomeia teclas.
- As linhas de estado (tela-sessao.js:886-891 e tela-server.js:830-834) dizem «microfone abre enquanto segura ESPAÇO», com `nomeDaTecla(teclaDeFalar)`.
- Na ajuda (index.html:3945-3946), o span fica `<span id="ajuda-tecla-falar" class="ajuda-tecla">ESPAÇO</span>`: o id antes da classe, e o texto padrão continua «ESPAÇO», para o guarda `a_ajuda_so_promete_teclas_que_a_janela_atende` (frontend.rs:9044) seguir lendo `class="ajuda-tecla">ESPAÇO<`. camada-ajuda.js escreve `nomeDaTecla(teclaDeFalar)` nele ao abrir.
- A linha de baixo passa a «Segure para falar, no modo tecla. Vale com a janela do SEELE na frente e fora da caixa de mensagem.».

2. O fone.
- Na primeira entrada numa sala de voz de cada execução, uma faixa que se fecha: «USE FONE. O SEELE ainda não cancela eco: sem fone, quem fala com você se ouve de volta.».
- A mesma frase entra na ajuda e em CONFIGURAÇÕES · MICROFONE E SOM.
- Se a marca de «já avisei nesta execução» for um Map ou Set de topo em tela-sessao.js, deixe um comentário dizendo que ela é por execução, e não por sessão: o guarda de mapas de topo do lote I vai pedir esse motivo.

3. O nome.
- Em `conectar()` (tela-boot.js:122): se o apelido continua vazio depois de `apelido_local` (:146-152), abrir `abrirPerfil` (camada-nomear.js:171) com o título «COMO OS OUTROS VÃO TE CHAMAR?» antes de chamar `connect`. Gravar com `escolher_apelido_local` e seguir.
- Fechar sem nome segue como hoje, mas dizendo: «Você vai entrar como pessoa-3f2a, um nome tirado da sua chave. Dá para trocar no perfil.».

Provas, em frontend.rs:
- (a) `a_linha_de_estado_nomeia_a_tecla_de_falar`: as duas tabelas `comoAbre` chamam `nomeDaTecla(` e não contêm «abre na tecla» sozinho;
- (b) `a_ajuda_escreve_a_tecla_de_falar_que_esta_gravada`: o index.html tem o span com `id="ajuda-tecla-falar"`, e camada-ajuda.js escreve nele com `nomeDaTecla(teclaDeFalar)`;
- (c) `ha_um_aviso_de_fone_em_algum_lugar_que_a_pessoa_le`: frases.js tem a frase, e a tela a desenha ao entrar numa sala;
- (d) `sem_nome_o_primeiro_conectar_pergunta_antes`: em `conectar`, a checagem de vazio e a abertura do perfil vêm antes de `invoke('connect'`.
Cada um fica vermelho quando se reverte a sua parte, e o guarda de :9044 continua verde.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --test frontend` e `cargo xtask check-runtime`.

NÃO fazer:
- não tirar «ESPAÇO» do HTML da ajuda, porque quebra o guarda de :9044;
- não pedir nome a quem já tem apelido;
- não mexer no Rust do apelido;
- não editar pendências nem o índice.

**Riscos.** Nada no fio, na API ou na CSP. É UX: o diálogo de nome no primeiro uso acrescenta um passo a quem entra por link. A medida em tela de verdade (WKWebView e WebView2) é manual e fica fora.

## Lote G-quem-hospeda-fecha-avisando — Fechar o SEELE enquanto se hospeda pergunta antes, avisa quem está dentro com ServerShuttingDown terminal e desce a porta do roteador

Ordem 19. Itens: P50-4, F1-fechar. Por que juntos: Os dois são o mesmo conserto visto de dois inventários. O F1-fechar quer a pergunta, o `ServerShuttingDown` difundido e terminal, e o UPnP descido. O P50-4 quer que o fechamento do processo encerre a hospedagem com aviso, mesmo sem pergunta. Os dois mexem em `Hospedagem`, no ExitRequested e no `despedir_se` que o lote F deixou. O índice já descreve o desenho («com ServerShuttingDown terminal e frase própria»), e o pedido de resolver o que não depende do dono cobre a aprovação que faltava.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/server.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/session.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-server/src/hospedagem.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/enlace.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/frases.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-sessao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-fim.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/frontend.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/encerramento.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/bateria_interna.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-conformance/tests/quem_hospeda_fecha.rs (novo)`

1. Servidor.
- Uma variante interna de difusão em `Event` (server.rs), fora do fio, por exemplo `Event::ServidorEncerrando`. Toda sessão a trata como o braço da expulsão (session.rs:3340): escreve `Disconnecting { reason: ServerShuttingDown }` e passa por `despedir`. Acerte os `match` exaustivos de Event (por exemplo, :4926).
- `Daemon::encerrar_avisando(&self, prazo).await`: difunde, espera as sessões acabarem até o prazo e só então chama `shutdown()`. `Daemon::shutdown` continua síncrono, para o Drop.
- `Hospedagem::encerrar` (o caminho do SAIR, hospedagem.rs:342-358): depois de `escada.descer()`, `encerrar_avisando(cerca de 500 ms)` no lugar do `shutdown()` cru. O resto continua como está.
- `Hospedagem::despedir(self, teto)` (o caminho da saída do app):
  - `encerrar_avisando(teto/2)`;
  - `timeout(teto/4, wait_idle())`;
  - `timeout(teto/4, escada.descer())`.
  É a ordem inversa da de `encerrar`, porque na saída o aviso vale mais que o roteador.
- A variante `ServerShuttingDown` já existe no protocolo 8 (control.rs:570), então o fio não muda.

2. Cliente.
- `a_sessao_acabou_aqui` (enlace.rs:341-346) inclui `ServerShuttingDown`. A `ScheduledMaintenance` continua sendo o «volto já» recuperável.
- Em frases.js, MOTIVOS: `ServerShuttingDown` passa a «QUEM HOSPEDA FECHOU O SERVIDOR.\nQuando ele abrir de novo, entre pela lista de servidores.», e a tela de fim (tela-fim.js) a alcança.

3. Casca (main.rs).
- `on_window_event` com `WindowEvent::CloseRequested { api }`: com `session.hospedagem` presente e sem confirmação, `api.prevent_close()` e emite `pedido-de-fechar` com o nome do servidor e quantas pessoas estão dentro.
- A janela (tela-sessao.js) escuta e chama `abrirConfirmacao("FECHAR O SEELE", «Você está hospedando «Mesa de RPG», com 3 pessoas dentro. Fechar a janela desliga o servidor e todos saem; eles vão ler que quem hospeda fechou.», "FECHAR E DESLIGAR", () => invoke("fechar_de_vez"))`.
- Comando `fechar_de_vez`: marca a confirmação (um AtomicBool em Session), tira a hospedagem do slot e roda `despedir(800 ms)`. Depois, `app.exit(0)` SEMPRE, mesmo se `despedir` falhar ou vencer o teto.
- `RunEvent::ExitRequested { code, api }`: com `code.is_none()` (saída por interação), hospedagem presente e sem confirmação, `api.prevent_exit()` e o mesmo `pedido-de-fechar`. Em qualquer outro caso, `despedir_se`, que passa a: MODs (P50-3), depois `hospedagem.take()` com `tauri::async_runtime::block_on(h.despedir(800 ms))` e `tracing::info!("saída do app: a hospedagem foi encerrada com aviso")`, depois a conexão (P50-2).
- Nunca prevenir com `code` Some: o `app.exit` do `fechar_de_vez` e o `app.restart()` do atualizador (main.rs:8370). O tauri 2.11 já ignora o prevent no RESTART_EXIT_CODE.
- Sem hospedar, nada muda.

4. Provas.
- crates/seele-conformance/tests/quem_hospeda_fecha.rs (novo):
  - `encerrar_avisa_cada_sessao_que_o_servidor_fechou`: `Hospedagem::iniciar` (como ejetar.rs:321) com um `Enlace` dentro; `encerrar()` faz o cliente ler `Disconnecting{ServerShuttingDown}`, e não um erro de transporte. Remover a difusão reprova;
  - `a_despedida_avisa_mesmo_com_o_runtime_caindo_em_seguida`: a Hospedagem numa `std::thread` com runtime current_thread, que espera um oneshot, roda `despedir(800 ms)` e deixa o runtime cair na linha seguinte. O cliente vê o aviso, e `closed()`, em menos de 1 s. Tirar o aviso ou o `wait_idle` faz o cliente cair só pelos 20 s.
- enlace.rs: `toda_despedida_do_protocolo_escolhe_um_lado` (:5446-5507) espera true para `ServerShuttingDown`. Em bateria_interna.rs, o par de `uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao` (:265) injeta `SessionEnded { reason: ServerShuttingDown }` e afirma que a sessão acaba sem reconectar. Reverter `a_sessao_acabou_aqui` deixa os dois vermelhos.
- frontend.rs: main.rs tem `CloseRequested` com `prevent_close` condicionado à hospedagem; a tela escuta `pedido-de-fechar` e chama `abrirConfirmacao`; a frase existe e a tela de fim a alcança.
- encerramento.rs: o ExitRequested continua chamando `despedir_se(handle)`, e o `prevent_exit` só aparece sob `code.is_none()`.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server hospedagem`, `… -p seele-conformance --test quem_hospeda_fecha --test bateria_interna`, `… -p seele-core enlace`, `… -p seele-app --test frontend --test encerramento`, mais fmt e clippy -D warnings.

NÃO fazer:
- não pôr nada novo no fio;
- não tornar a ScheduledMaintenance terminal;
- não prevenir a saída sem hospedagem, nem com `code` Some;
- não deixar a confirmação prender a janela: a saída é garantida;
- não tratar sinal no seeled avulso, que é outro item;
- não editar pendências nem o índice.

**Riscos.** Fio: nada, porque a variante já existe no v8. Um cliente 0.15.0 recebe `ServerShuttingDown`, trata como recuperável e reconecta por 5 minutos, como hoje. O Cmd+Q do macOS e o Alt+F4/X do Windows passam por caminhos diferentes no Tauri 2. Se o Cmd+Q não for segurável, ele cai no `despedir_se`, que ainda avisa e desce o UPnP. A conferência à mão no Mac e no Windows fica com o dono. Um logout do macOS com hospedagem pode ser cancelado pela pergunta, como num editor com arquivo não salvo. O pior caso de fechar hospedando com MOD nativo passa a cerca de 1,5 s. O roteador real não se testa aqui.

## Lote I-audio — O áudio que não abre não prende a conexão, um pânico deixa linha, o microfone mudo pelo macOS é dito, e o volume e os rascunhos não vazam entre servidores

Ordem 20. Itens: F1-audio-panico, F1-microfone-macos, F1-vazamento. Por que juntos: Os três mexem no `Shared` da voz no seele-ffi (o prazo e a reabertura, a telemetria do silêncio, os volumes guardados), no Snapshot de types.rs, no main.rs e na tela da sessão com as frases. Juntos, o Snapshot muda uma vez, com o mesmo cuidado: nada da máquina e nada de preferência entre pessoas vai aos MODs.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/lib.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-ffi/src/types.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/src/main.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/frases.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-sessao.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-chamada.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/ui/tela-fim.js`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/apps/seele-app/tests/frontend.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-core/src/voice.rs`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/crates/seele-audio/src/device.rs (só se o detector morar lá)`

1. F1-audio-panico.
- (1) Abrir o áudio numa thread própria com nome (`std::thread::Builder::new().name("seele-abre-audio")`), devolvendo por `tokio::sync::oneshot`. Esperar com `tokio::time::timeout(PRAZO_DO_AUDIO, rx)` ANTES de `ready.send` (seele-ffi lib.rs:3955-3984), como hoje.
  - O prazo: `PRAZO_DO_AUDIO` = 10 s, com um doc dizendo que ele cobre o driver são, e que a primeira abertura com o pedido do TCC e o WASAPI são medidas do dono (fora).
  - Por que esperar antes do ready: `set_muted`, `set_voice_mode`, `set_total_isolation` e `set_supressao_de_ruido` (:2082-2113, :2421-2437) pulam com `shared.voice == None`, e um mudo pedido logo depois do `connect` se perderia.
  - Por que thread própria, e não `spawn_blocking`: uma tarefa bloqueante presa no driver seguraria o `drop` do runtime.
  - Ao vencer o prazo: `tracing::warn!("o áudio não abriu em {PRAZO}s; a sessão segue só com texto")`. A voz que chegar tarde é descartada na própria thread, e o `drop` fecha o aparelho.
  - O Snapshot ganha `audio: EstadoDoAudio` (`Abriu`, `SemAparelho`, `NaoAbriuNoPrazo`), sem tirar `audio_available`, que os MODs podem ler. A tela (tela-sessao.js:2433) mostra «O ÁUDIO NÃO ABRIU A TEMPO. A conversa por escrito já funciona; para tentar a voz de novo, escolha o microfone em CONFIGURAÇÕES.».
  - A reabertura da reconexão (:4139-4143) usa a mesma espera, sem segurar o Mutex de `shared.voice` enquanto o aparelho abre.
- (2) Pânico: `instalar_gancho_de_panico()`, chamado uma vez (`Once`), logo depois do `tracing_subscriber…init()` em `main()`. Ele guarda o gancho anterior, escreve `tracing::error!(target: "seele_app", thread = nome, local = %info.location(), "pânico: {mensagem}")` e chama o anterior.
- Provas:
  - (a) `um_panico_numa_thread_deixa_linha_no_log`: instala o gancho e cria uma `std::thread` com nome. Dentro dela, `tracing::subscriber::with_default(captura, || panic!("x"))`, porque o subscriber é por thread. Depois do `join`, exige «pânico:», a mensagem e o nome da thread;
  - (b) `esperar_o_audio(abrir: impl FnOnce() -> Result<V> + Send + 'static, prazo) -> Result<V, Atraso>`, genérica como `reabrir_voz_na_reconexao`: um `abrir` que dorme 2 s, com prazo de 200 ms, volta em menos de 1 s com `Atraso`. A versão síncrona leva 2 s e reprova pela asserção de tempo, sem pendurar a bateria;
  - (c) a reabertura da reconexão passa pela mesma espera.

2. F1-microfone-macos, só a parte (1).
- No laço de captura (seele-core/src/voice.rs), uma função pura conta quadros de silêncio DIGITAL (amostras exatamente 0.0) com o microfone aberto e não mudo. Passados 3 s de zero exato, a telemetria marca `microfone_entrega_silencio_digital: bool`.
- Só no macOS a tela diz «O MACOS PODE ESTAR BLOQUEANDO O MICROFONE.\nAjustes → Privacidade e Segurança → Microfone → ligue o SEELE, e entre de novo.». No Windows, o consentimento já é lido.
- Provas:
  - (a) teste do detector com quadros sintéticos: 3 s de zeros exatos marcam; ruído a -90 dBFS, ou uma única amostra diferente de 0, não marcam. Reverter o detector deixa o teste vermelho;
  - (b) em frontend.rs, a frase existe em frases.js e a tela a desenha a partir do campo.
- A medida com o TCC negado e a via AVFoundation (2) ficam com o dono.

3. F1-vazamento.
- (1) Em tela-sessao.js, `esquecerOEstadoDaSessao()` faz `volumes.clear()` (enquanto existir), `rascunhos.clear()`, `previas.clear()`, `previasFechadas.clear()` e `previasDeLink.clear()`, e chama `esquecerRetratos()`. Ela é chamada em `ejetar` (:3683-3750) e em `limparSessaoEncerrada` (tela-fim.js:160-190).
  - O RECONECTAR da tela de fim passa por `limparSessaoEncerrada` (tela-fim.js:124-127). Um rascunho não sobrevive ao fim de uma sessão, o que é o «por sessão» que o doc de `rascunhos` (tela-sessao.js:2627) já diz: atualize o doc para dizer isso. A reconexão automática da bateria não passa por aqui e não perde nada.
- (2) O volume vai para o Rust.
  - `Shared` guarda `volumes: Mutex<HashMap<String, u16>>`, por apelido. `set_volume` (lib.rs:2477-2494) grava ali e aplica.
  - A função pura `ganhos_a_aplicar(volumes, room) -> Vec<(ssrc, ganho)>` (no molde de `reabrir_voz_na_reconexao`, :4212-4240) é chamada pelo laço que dobra eventos em `Room` sempre que o SSRC de um apelido muda, a reconexão inclusive.
  - O valor chega à janela por um comando próprio, `volumes_locais()`, registrado no `generate_handler!`, e NUNCA pelo Snapshot, que os MODs leem (base.js:2072-2079, api/v5.json `reads.retrato`).
  - O mapa `volumes` da janela deixa de existir, também em tela-chamada.js:525, :819 e :829.
- Provas:
  - (a) `todo_mapa_de_topo_da_sessao_e_esquecido_ao_sair`, em frontend.rs: lista todo `const|let X = new Map()/new Set()` de topo em tela-sessao.js e exige o `X.clear()` (ou a chamada nomeada: `esquecerRetratos()` para `retratos`, `encerrarBusca` para `casamentosPorMensagem`) dentro de `esquecerOEstadoDaSessao`, e que ela seja chamada nos dois lugares. Uma exceção só entra com o motivo escrito no teste, como a marca do fone do lote J, se ela for por execução. Tirar qualquer `.clear()` reprova;
  - (b) `o_volume_volta_quando_o_ssrc_muda`: 150 para «rafa», depois um SSRC novo para «rafa», e a função devolve (novo, 1.5). Um segundo teste confere que o laço a chama na troca.

Comandos: `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-ffi audio`, `… -p seele-ffi volume`, `… -p seele-core voice`, `… -p seele-app panico`, `… -p seele-app --test frontend`, mais fmt e clippy -D warnings.

NÃO fazer:
- não pôr o volume, nem nada que identifique a máquina, no Snapshot. Os campos novos são booleano e estado;
- não usar `spawn_blocking`;
- não mandar o `ready` antes da espera da voz;
- não abrir exceção de `unsafe` nem usar `objc2-avf-audio`;
- não rodar `tccutil`, que muda um ajuste de privacidade do sistema;
- não editar pendências nem o índice.

**Riscos.** Nada no fio nem na CSP. O Snapshot ganha dois campos aditivos, sem dado da máquina. A thread presa no driver continua viva até o driver soltar, mas não segura nada. O detector pode dar falso positivo com interfaces USB que entregam zero exato com o ganho no mínimo, e por isso a frase diz «pode estar». Um prazo de 10 s pode cortar a voz na primeira abertura se o pedido do TCC bloquear a abertura: a medida é do dono, e a frase ensina a tentar de novo. Um rascunho escrito antes do fim de uma sessão some no RECONECTAR. O WASAPI e o WebView2 não se testam aqui.

## Lote Z-registro-e-release — docs: as pendências 49, 50 e 51 dizem o que fechou e em qual commit, o índice perde o que foi feito, e a release fica preparada (o placeholder do G1, a regra de quem ganha cadeia, o rascunho das notas)

Ordem 21. Itens: P49-6, R-placeholder, R-regra-G1, R-notas-rascunho, R-notas-actions. Por que juntos: É o lote de registro, e vem por último porque registra o que os outros fecharam, com os hashes deles. Todas as edições de docs/pendencias.md e do índice da onda moram aqui, para nenhum lote de código dividir esses arquivos. A regra «Quem ganha cadeia» e o placeholder do G1 moram no mesmo parágrafo do plano 1B, e a regra é o que torna coerente que nenhum lote desta onda tenha ganhado cadeia.

Arquivos: `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/pendencias.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-indice.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/docs/superpowers/plans/2026-09-29-1.0-plano-1b-a-impressao-no-tls.md`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/empacotar/notas/rascunho.md (novo)`, `/Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0/xtask/tests/empacotamento.rs`

Três commits, todos com assunto «docs: …», menos o do guarda.

1. R-regra-G1 e R-placeholder, só no plano 1B.
- :2875 troca «O último do Lote F-Fecho, este:» por «O último do Lote F-Fecho (`994e750`):».
- :2879 troca «Rode, trocando `<commit do Lote F-Fecho>` pelo commit cujo assunto é a última cadeia acima:» por «Rode:».
- :2884 fica `for faixa in d96a222..63113a6 c52f9d9..994e750; do`.
- :2895 fica: «`c52f9d9..994e750` é a correção da revisão final (`c52f9d9` é o último commit antes do Lote F-Seg, e `994e750` o último do Lote F-Fecho)».
- Em :2899, o começo do parágrafo, até «…e acrescenta o lote à linha do G1 no índice.», é substituído por:
«**Quem ganha cadeia, e o que o G1 não prova.** O G1 confere o que a lista dele no índice pede em commits: que a Task 5 do 1A não sai sem as Tarefas 1 a 5 e 7 do 1B, sem a correção da revisão ampla do 1B, sem a lista sem endereço morto da correção da revisão ampla do 1A e sem a correção da revisão final do Plano 1 (as outras duas coisas da lista, a medida de campo e a linha nas notas, não são commits). Por isso só ganha cadeia uma correção desse conjunto — uma rodada de revisão nova que conserte o que um deles entregou — que entra antes da release e muda código do produto ou uma frase pública (o `README.md`, as `specs/`, `docs/ponto-de-encontro.md`, `docs/alcance-pela-internet.md`, `api/README.md`, `docs/como-se-faz-um-mod.md`, as frases de tela). Ela acrescenta à lista de cima a cadeia do seu último commit, com o porquê dela; atualiza a contagem no `echo` da receita e no parágrafo das releases cortadas antes; troca o hash do bullet anterior; leva o fim da segunda faixa da receita da história escolhida a dedo (hoje `994e750`) para o último commit do seu lote; e acrescenta o lote à linha do G1 no índice. Um conserto de outro assunto — o S1, uma pendência (a 49, a 50, a 51), uma correção de outro plano — não ganha cadeia aqui, mesmo mudando código do produto ou uma frase pública: ele não fecha nada que o G1 confere, e uma cadeia dele faria do portão um inventário do que a release leva, que é trabalho das notas e do histórico. Se um conserto desses tiver de segurar a release, o portão dele se escreve no plano dele.»
- A frase antiga dos commits só de docs vira «Uma correção cujos commits só mexem em `docs/superpowers/`, em `docs/pendencias.md`, em testes ou em ferramenta (`crates/*/tests/`, `xtask/`, `apps/seele-app/bancada/`) não muda o que a release entrega, e também não ganha cadeia», mantendo o exemplo do Lote F-Registro. «Que o último commit do lote…» fica como está.
- O fim do parágrafo («O G1 confere que os commits estão na release, e não que as frases são verdadeiras…») ganha «e um conserto de fora do G1 que torne falsa uma frase que uma cadeia sustenta também passa por ele».
- Conferência, com /bin/bash, sobre o HEAD final da onda:
  - a receita das cadeias imprime «G1: as catorze cadeias estão na release, e nenhuma foi revertida»;
  - a da história escolhida a dedo, com 994e750, sai vazia;
  - sobre 89499c2, sai com 6 «G1 reprova»;
  - `grep -n "só ganha cadeia uma correção desse conjunto"` e `grep -n "lista sem endereço morto"` acham a regra.

2. R-notas-rascunho e R-notas-actions.
- Criar e commitar empacotar/notas/rascunho.md. O nome nunca casa a regra de versão (publicar.sh:762), e o publicar.sh:1119-1123 recusa trabalho não commitado sob empacotar/. O texto vem do plano 1B, «O que as notas da release dizem» (:2990-3002), em duas citações para quem baixa:
  - «### Se um servidor da lista não abrir, cole o link de novo», com: a 0.15 pode ter guardado a chave de um link que discordava; no endereço da internet a lista se corrige sozinha na primeira volta; no da rede de casa, ou quando só responde um endereço que você não escolheu, a volta é recusada com «ESTE NÃO É O SERVIDOR QUE A LISTA GUARDOU»; o remédio é colar de novo o link de quem hospeda;
  - «### O que passa a ser recusado, e antes entrava com aviso», com os dois casos e o porquê.
  As frases da tela têm de bater com ui/frases.js:463-473.
- Guarda em xtask/tests/empacotamento.rs, com a Bancada que já roda o publicar.sh num repositório falso (:570-650): com `empacotar/notas/rascunho.md` contendo uma marca, o corpo de uma versão X.Y.Z não a contém, e com o mesmo conteúdo em `X.Y.Z.md`, contém. Ele prende o comportamento de hoje: reprova se a leitura de `$VERSAO.md` virar um glob.
- No índice, o item 2 de «Antes de cortar a release» (:111-119):
  - o rascunho mora em `empacotar/notas/rascunho.md`, e quem publica faz `git mv` para `<versão>.md` no commit da versão e acrescenta o resultado da medida de campo;
  - sem as notas, a página diz a frase nova do classificador (L1: «Os commits desta faixa não dizem pelo prefixo se mudam o produto…») no lugar de «nenhuma mudança de produto»;
  - pelo release.yml as notas da versão não entram: cole o conteúdo de `empacotar/notas/<versão>.md` no rascunho antes de publicar (o release.yml:823-829 usa só .github/NOTAS-DE-RELEASE.md).

3. Registro.
- Índice:
  - a linha do Plano 6 (:22) troca «o m-4 da pendência 49 (`repetir`, ou o contrato de `tocando` escrito na API)» por «o contrato de `tocando` (api/README.md, 563bc37) entra no texto da v6; `repetir` é opção, não condição»;
  - «Fora desta especificação, e ainda sem aprovação» perde o que foi feito (S1, portaria, install, connection, classificador, fechar, tecla, vazamento, áudio, os jobs deny e fuzz) e fica só com o que é do dono (o corpo da v0.15.0, o gatilho de push da CI), apontando a pendência 52.
- Pendência 49:
  - os itens 5, 7a, 7b, 8, 9 e 10 dizem «fechado em <data>, `<hash>` (lote …)»;
  - o 7c diz o que fechou (o som adiado) e o que fica com o dono (o teto por MOD);
  - o 6 diz «cumprido em 563bc37: o contrato de `tocando` está em api/README.md, e o comportamento é guardado pela regiao-do-mod.cjs (`oSomQueTerminouVoltaComOTocandoDeclarado`); `repetir` fica como opção para quando a API 6 for desenhada, e o contrato entra no texto da v6 quando ela for congelada»;
  - os itens 1 a 4 ficam como estão.
- Pendência 50 fecha:
  - o título passa a «Consertada em <data>»;
  - o conserto do P50-1 e do P50-2, com os hashes do lote F e a medida do teste saida_com_despedida.rs (o tempo que ele mediu);
  - o vizinho do 1C (P50-3) e a hospedagem (P50-4), com os hashes do F e do G;
  - que a medida da sessão separada (medida_do_disconnect.rs) não foi commitada.
- Pendência 51, item por item, com o commit e o lote:
  - 1 a 23, 26 e 27 fechados. O 26 fecha só depois do B e do C;
  - o 28 com a parte 1 fechada, e o vetor fica com o dono;
  - 24 e 25 ficam (outros repositórios);
  - os parciais que já estavam fechados antes: o T2 m1 do 14 em 517afaa; no 15, o «panicked» e a etapa do publicar.sh (:1005-1019); o T5f do 19 em 27f86af, com o Espiao do seele-ffi sobrando; a 8384 do 16 como escolha documentada.
- Pendência 52 nova, «O que a onda de 04/10 deixou com quem opera». Uma entrada por item de «fora», com o comando de cada um, e os textos prontos:
  - o corpo de substituição da v0.15.0: o «o que mudou» tirado de empacotar/notas/0.15.0.md e da faixa v0.14.2..v0.15.0 (protocolo 8), e «dois programas: SEELE e seeled»;
  - os dois textos do F1-ao-lado: largar a promessa (index.html:545-546 dizendo que só a versão mais nova é baixada, e que no Windows não há duas lado a lado) ou mantê-la (num plano próprio);
  - o argumento técnico do F1-endpoint-antigo;
  - as medidas manuais: TCC do microfone, primeira abertura do áudio, fechar no Mac e no Windows, S1 no NTFS, WebViews.

NÃO fazer:
- não acrescentar cadeia ao G1 por nenhum lote desta onda;
- não renomear o rascunho para um número de versão;
- não mexer no release.yml (a montagem do corpo ali só se prova com um disparo, e ele publica em DATA-AND-DEV/SEELE);
- não escrever «fechado» sem o hash do commit que fecha;
- não reescrever as linhas históricas das pendências: acrescentar.

**Riscos.** Se o dono quiser o G1 como inventário de tudo o que a release leva, basta não aplicar a regra. Aí cada lote de código desta onda teria de ganhar uma cadeia, e a faixa teria de ser movida para o último deles. Nenhuma release pode ser cortada entre o primeiro lote de código e este lote. Se ninguém renomear o rascunho, a página da release sai sem a linha obrigatória, como hoje, e o índice passa a dizer isso. Nada no código.

## O que fica com o dono

- **F1-endpoint-antigo.** O guarda `o_endereco_antigo_continua_na_lista_enquanto_a_migracao_dura` (xtask/tests/empacotamento.rs:2150) registra a remoção como decisão de quem opera. Fica com o dono decidir se o segundo endpoint (tauri.conf.json:60) sai. O lote Z escreve o argumento técnico na pendência 52: a lista de uma build nova não alcança quem está em 0.9.x; a ponte é a release v0.10.0, que tem de continuar publicada; numa build atual, a reserva devolve a v0.10.0 e transforma uma falha do manifesto novo em «não há versão nova», calada. Se ele decidir tirar: remover a linha, apagar o guarda com a decisão escrita no commit, reescrever publicar.sh:150-153 e escrever `todo_endpoint_do_atualizador_e_uma_casa_do_publicar`.
- **F1-release-corpo.** Editar o corpo publicado da v0.15.0 em SEELE-RELEASES é ação externa, com o token de quem publica. A página ainda diz «nenhuma mudança de produto» e «três programas», e não foi editada desde 2026-09-23T00:12:13Z (medido hoje). O texto de substituição sai pronto na pendência 52, pelo lote Z. O texto das próximas releases fica certo pelo L1 (classificador) e pelo L2 (NOTAS-DE-RELEASE.md).
- **F1-ci-dono.** Ficam com o dono: devolver `push`/`pull_request` ao ci.yml (as linhas estão no cabeçalho, :40-42, e o arquivo registra a escolha como dele); ler os logs dos cinco jobs vermelhos (clippy em macOS e Windows, test em Linux, macOS e Windows), que pedem login; provar um run verde, que pede push ou disparo; e decidir se o repositório continua público. Os jobs deny e fuzz ficam prontos pelo lote L1.
- **F1-ao-lado.** Decisão de produto (anexo :820): manter a promessa de guardar a 0.14.2 ao lado, o que é construir a escolha de versão e o pacote do Windows, ou largá-la e corrigir index.html:545-546. As notas publicadas da v0.15.0 também prometem. O lote Z deixa os dois textos prontos na pendência 52. Escolhido o caminho, o conserto do texto é P e cabe numa onda seguinte.
- **P51-24.** O guia externo (mods.seele.app.br/guia) diz `APIS_ACEITAS = [4, 3]` e `MOD_API_VERSION = 4`. O conserto é no repositório SEELE-MODS-INDEXER e na publicação do site, que são de quem opera. Do lado daqui, o lote N faz o guia do repositório deixar de apontar o externo enquanto ele estiver atrás.
- **P51-25.** Os laboratórios de SEELE-MOD-MESA, SEELE-MOD-PERFIS e SEELE-MOD-ESTILO (preview.cjs, o `media-src data:` e o test-ui) são outros repositórios e outras publicações. A melhoria opcional daqui, uma frase própria quando o dono da região não oferece `bytesDoSom`, entra no lote K5.
- **P49-1.** Medida num Mac com tela e ouvido: o «antes» com o app de 3d50b21, escrito em docs/evidencias/som-de-mod-wkwebview/registro.md (roteiro em pendencias.md:7284-7304). Não há código a fazer.
- **P49-2.** Medida em hardware: o «depois» no WKWebView com o app de verdade, o `pmset -g assertions` nos momentos do roteiro, e o macOS 11.0 a 11.2 se houver máquina (pendencias.md:7306-7346).
- **P49-3.** A metade WebView2 da mesma medição precisa de uma máquina Windows e de `powercfg /requests` como administrador (pendencias.md:7348-7358).
- **P49-4.** O disparo manual do ci.yml (job `bancadas`) antes de cada publicação é ação no Actions (`gh workflow run ci.yml --ref <ramo>`). Localmente dá para rodar as bancadas de navegador com `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright`, como conferência prévia, mas isso não substitui o disparo.
- **F1-install (parte do dono).** O conserto de código é do lote L1. Ficam com o dono: o push do `main`, porque o curl do README serve o `main` remoto, que hoje está em 70bae9d, e sem push o conserto não chega a ninguém; e decidir se publica um pacote Linux. Hoje o install.sh passa a mandar compilar do código no Linux e no Mac Intel.
- **F1-connection e F1-texto-audio-em-claro (parte do dono).** O README e os docs corrigidos pelo lote L2 só chegam ao GitHub com o push do dono. A página publicada da v0.15.0 só muda pelo F1-release-corpo.
- **F1-microfone-macos (parte do dono).** Ficam com o dono: (c) a medida com o pedido do TCC negado por alguém na tela, que confirma se o CoreAudio entrega zeros (resetar a permissão do microfone com `tccutil` é ajuste de privacidade do sistema, e não é ação para um implementador); e (2) a via exata por `AVAudioApplication.recordPermission` (objc2-avf-audio), que pede abrir exceção ao `forbid(unsafe_code)` do workspace, uma decisão de arquitetura. O detector de silêncio digital, a parte (1), é do lote I.
- **F1-audio-panico (parte do dono).** Medir quanto tempo leva, e se bloqueia, a primeira abertura com o pedido de permissão do macOS, e a abertura pelo WASAPI no Windows, para calibrar o `PRAZO_DO_AUDIO` (o lote I fixa 10 s, com doc).
- **F1-fechar (parte do dono).** A conferência à mão do fechamento enquanto se hospeda: no Mac (botão vermelho e Cmd+Q, inclusive se o Cmd+Q é segurável) e no Windows (X e Alt+F4). Precisa de uma pessoa na tela, e o Windows não se testa aqui. O código e as provas automáticas são do lote G.
- **S1 (parte do dono).** A gravação real no NTFS (nomes reservados, ponto no fim, `Zone.Identifier` com `create_new`) só se exercita no Windows, pela bateria Windows do publicar.sh, que roda quem publica. As regras de texto são provadas no Mac pelo lote A.
- **P49-7c (parte do dono).** Um teto de som decodificado por MOD, somando todos os bolsos (até 12 superfícies e 128 contribuições), escolhe quanto som um MOD pode segurar e muda a frase pública do api/README.md:71-76. O adiamento do som de contribuição com `tocando: false` é do lote K5.
- **P51-28 (parte do dono).** Mexer no vetor de referência (exercitar o `enviar` ou escrever no cabeçalho por que não) espera a medida de campo da pendência 49, que compara o vetor byte a byte com 3d50b21. A parte 1, o guarda, é do lote N.
- **R-notas-rascunho (parte do dono).** O número da versão, o `git mv` de empacotar/notas/rascunho.md para `<versão>.md` no commit da versão, e a linha da medida de campo (plano 1B:2903) são de quem publica. O rascunho e o guarda são do lote Z.
- **R-notas-actions (parte do dono).** Montar o corpo da release no release.yml (as notas da versão, `---` e NOTAS-DE-RELEASE.md) só se prova com um disparo do release.yml. Além disso, o release.yml publica em DATA-AND-DEV/SEELE, e não em SEELE-RELEASES (análise :564-565), e nunca passou. O lote Z escreve só a instrução no índice.
- **Antes de cortar a release (índice, itens 1, 4 e 5).** Continuam com o dono: a medida de campo obrigatória do G1 (as quatro linhas de resultado, plano 1B, «A medida de campo»); a decisão escrita sobre as Tasks 1 e 9 do 1D e a metade WebView2 (se seguram a release); o G2 registrado; o corte da release; e o merge desta onda no `main` sem squash, porque a receita do G1 lê os assuntos da história.
- **Confirmações em WebView de verdade (P51-06, P51-07, P49-5, F1-tecla-fone-nome).** O comportamento no WKWebView, no WebView2 e no WebKitGTK (substituto UTF-16 no invoke, `decode()` com bytes corrompidos, o bloco novo da aba DIAGNÓSTICO, a tecla e o aviso de fone na tela) só se confirma com o app em cada plataforma. As bancadas de Node provam a lógica, e a confirmação em tela é opcional e do dono.

## Observações da síntese

- Raiz de todos os caminhos relativos: /Users/dev-alexandre/SEELE/.claude/worktrees/abertos-da-1.0 (branch conserto/abertos-da-1.0, HEAD 3685a64, árvore limpa). A última release publicada, conferida hoje pela consulta do CLAUDE.md, é a v0.15.0 de SEELE-RELEASES (commit e2fac4dab, 2026-09-23). Ela publica latest.json, os pacotes do app, seele-cli-0.15.0-macos.tar.gz, seele-cli-0.15.0-windows-x86_64.zip e SHA256SUMS, sem pacote Linux.
- Ondas, para rodar em paralelo. Cada lote roda numa árvore própria, criada a partir do main com os lotes anteriores já integrados, e dois lotes da mesma onda nunca dividem arquivo:
- onda 1: A-S1 (1), B-servidor (2), K1 (3), L1 (4), L2 (5);
- onda 2: C (6, depois de A), H (7, depois de A), K2 (8, depois de K1), N (9, depois de B);
- onda 3: D (10, depois de C e N), M1 (11, depois de H), K3 (12, depois de K2, H e L1);
- onda 4: K4 (13, depois de D e K3), M2 (14, depois de M1, L2 e H);
- onda 5: E (15, depois de K4), K5 (16, depois de K4);
- onda 6: F (17, depois de E, M2 e H), J (18, depois de K5, K3 e H);
- onda 7: G (19, depois de F, J e M2);
- onda 8: I (20, depois de G);
- onda 9: Z (21, depois de todos).
O gargalo é apps/seele-app/src/main.rs, que oito lotes editam (A, C, D, K4, E, F, G, I), e por isso a fila mínima tem nove ondas.
- Regras comuns a todo lote de código:
- o teste que falha antes é escrito antes, e cada guarda novo é provado por reversão (desfazer o conserto, ver o vermelho, restaurar), como o CLAUDE.md pede;
- testes sempre com `perl -e 'alarm 600; exec @ARGV' cargo test -p <crate> …`;
- antes de cada commit, `cargo fmt --all` e `cargo clippy --workspace --all-targets --all-features -- -D warnings`, e `cargo xtask check-runtime` quando mexer em ui/ ou em bancada/;
- commits em português, com o assunto descritivo, como os do repositório, terminando com a linha Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>;
- nenhum lote de código edita docs/pendencias.md nem o índice: o relatório de cada lote diz, por item, o hash do commit que o fecha, e o lote Z registra;
- o fio (PROTOCOL_VERSION 8, SEELE-ENC/1), api/v3..v5.json, a CSP e as dependências não mudam em lote nenhum.
- G1: pela redação nova de «Quem ganha cadeia» (R-regra-G1, lote Z), nenhum lote desta onda ganha cadeia, porque nenhum é correção do conjunto que o G1 confere: S1, Fase 1 e pendências 49, 50 e 51 são outro assunto. Pela letra da regra de hoje, todos os lotes que mudam código ou frase pública ganhariam. Por isso nenhuma release pode ser cortada entre o primeiro lote de código e o commit da regra. O commit da regra só divide arquivo com o M2 (plano 1B:99), e pode subir antes do Z, se for conveniente, depois do M2.
- Achado fora dos inventários, que muda o F: apps/seele-app/tests/encerramento.rs prende por texto o `connection.disconnect()` e o `sleep` dentro de `despedir_se`. O P50-2 troca o sleep por `encerrar_e_esperar`, e esse guarda reprova se não for reescrito, o que entrou na especificação do F. O doc do arquivo dizia que não havia prova de comportamento porque «exigiria subir dois processos», e o P50-2 passa a ter exatamente essa prova.
- Achado fora dos inventários, que muda o G: no tauri 2.11, `ExitRequestApi::prevent_exit` é ignorado no RESTART_EXIT_CODE, e o ExitRequested chega com `code` Some num `app.exit` ou `app.restart` programático (o atualizador chama `app.restart()`, main.rs:8370). A especificação do G só segura a saída com `code.is_none()`.
- Achado fora dos inventários, que muda o I: o RECONECTAR da tela de fim passa por `limparSessaoEncerrada` (tela-fim.js:124-127). Limpar os rascunhos ali faz um rascunho não sobreviver ao fim de uma sessão. O doc de `rascunhos` (tela-sessao.js:2627) já diz «só em memória, por sessão», e o Snapshot não tem a impressão do servidor para chavear por ela. A especificação escolhe limpar e atualizar o doc; a reconexão automática da bateria não passa por ali e não perde nada.
- Reconciliação entre inventários. O F1-fechar dizia «nada depende do dono», e o P50-4 dizia que perguntar antes de fechar e tornar o ServerShuttingDown terminal era decisão dele (índice, «ainda sem aprovação»). O índice já descreve o desenho («com `ServerShuttingDown` terminal e frase própria»), e o pedido de resolver o que não depende do dono é a aprovação que faltava. Por isso os dois entraram juntos no lote G, sobrando para o dono só a conferência à mão no Mac e no Windows.
- Reconciliação entre inventários. A metade do app do P51-26 (`mod_request`) saiu do lote B para o C, junto com o P51-10, que mexe nas mesmas duas linhas. Assim a metade do servidor, a urgente (um cliente autenticado forja linha no seele.log de quem hospeda, regressão não publicada do 6800d18), sai na onda 1, junto com o S1. O P51-26 só é registrado fechado depois do C.
- Reconciliação entre inventários. O doc de `onde_mora_hoje` tem um dono só, o P51-18 no M2, que o reescreve com o comportamento que o T6f do M1 deixar. O parágrafo de `esperar_o_servidor_fechar` (volta_pela_trilha.rs:346-353) é editado em sequência: o M2 troca «dezenas de milissegundos», e o F troca a frase dos 20,0 s.
- Atenção para quem integra: a árvore principal (/Users/dev-alexandre/SEELE, branch mobile/ios, sem commit) está movendo apps/seele-app/src/main.rs para lib.rs e mexendo em frontend.rs, encerramento.rs, permissoes.rs, apps/seele-app/tests/empacotamento.rs, index.html e no Cargo.toml do app. Oito lotes desta onda editam o main.rs, e seis o frontend.rs. Quem integrar por último resolve a mudança de nome: a semelhança é quase total, e o git costuma seguir o rename. Que trabalho entra primeiro é decisão do dono.
- O Playwright não está no node_modules deste repositório, mas está em /Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright, e os Chromium estão no cache. A bancada aceita a variável `PLAYWRIGHT` (apps/seele-app/bancada/playwright.cjs), então o K2 e o K3 rodam as bancadas de navegador aqui sem baixar nada.
- R-check-deps não está aberto: o `cargo xtask check-deps` não usa banco de avisos e roda sem rede, e o `cargo deny --offline check advisories` passa com o banco local de 01/10. Não entrou em lote nenhum.
- Itens fechados por dentro de itens abertos, que o lote Z só registra: o T2 m1 do P51-14 (517afaa); o «panicked» e a etapa do check-api no publicar.sh, do P51-15; o T5f do P51-19 (27f86af), sobrando o Espiao do seele-ffi; a porta 8384 do P51-16, como escolha documentada; e o m-4 da pendência 49 (P49-6), cumprido em 563bc37.
- Lotes pesados, que podem não fechar numa sessão: o M1 (P51-17 é G, mais o P51-19) e o K4 (P49-7a é G, mais o P49-7b). A especificação de cada um diz onde parar sem deixar a mesma função pela metade. O A-S1 é M, mas atravessa doze arquivos.

## Lote A2-S1-acabamento — o que a revisão do A-S1 deixou: o parcial nunca tem o nome final, a lista do Windows completa, e o que falha ao apagar é dito

Vem da revisão do lote A-S1 (3685a64..ec5134d, aprovado com a bateria verde) e das preocupações do implementador
(lote-A-S1-report.md). Roda depois do B-servidor.

1. **O arquivo parcial nunca tem o nome final** (preocupação 6). Hoje o arquivo é criado com o nome final antes do
   download. Se o processo morrer no meio, fica na pasta um arquivo truncado com cara de completo.
   - O download vai para um nome temporário, criado com `create_new` na mesma pasta, por exemplo
     `.<nome>.seele-parcial`. Justifique o nome no relatório.
   - Só depois de o hash conferir, o arquivo passa ao nome final, sem substituir nada:
     - um `hard_link` para o nome final, que falha se o nome já existir, seguido de apagar o temporário, e com o
       sufixo seguinte se o nome tiver sido tomado no meio-tempo;
     - ou outra forma sem sobrescrita que você justificar.
   - Num volume sem link físico (FAT/exFAT), use um caminho de recuo que também não sobrescreva, e diga qual é.
   - A quarentena (`com.apple.quarantine`, `Zone.Identifier`) fica no arquivo final.
   - A frase `ARQUIVO SALVO — <caminho real>` mostra o nome que ficou.
   - Toda falha apaga o temporário.
   - Teste com TDD: um download interrompido, por exemplo com o hash errado ou uma falha injetada no meio, deixa a
     pasta sem nenhum arquivo com o nome final. Prove por reversão.
2. **R9 e preocupação 12: apagar e falhar é dito.** O `let _ = tokio::fs::remove_file(...)` de
   `crates/seele-core/src/client.rs`, e o do temporário novo, passam a escrever um `warn!` com o caminho e o erro
   quando falham. As frases que hoje prometem sem condição passam a dizer o que o código faz:
   - «Nada foi gravado pela metade», em `apps/seele-app/ui/frases.js`;
   - «O que foi criado saiu», no doc de `NotSavedReason::Falhou` em `crates/seele-ffi/src/types.rs`.
3. **R8 e preocupação 7: a lista de reservados.** Entram `COM0`, `LPT0`, `CONIN$` e `CONOUT$` em `RESERVADOS`
   (`crates/seele-core/src/anexo_no_disco.rs`), com casos no teste
   `um_nome_alegado_so_vira_nome_de_arquivo_se_for_so_um_nome`, inclusive com extensão e em minúsculas. O doc da lista
   diz de onde ela vem.
4. **R7: o título da recusa.** «O NOME QUE VEIO COM ESTE ARQUIVO NÃO É SÓ UM NOME» sugere truque num nome comum como
   «Por quê?.pdf».
   - Troque, nos dois lugares de `frases.js` (`NAO_SALVOS.NomeRecusado` e a composta de `fraseDeNaoSalvo`), por um
     título que valha para todas as regras sem acusar o nome, por exemplo «ESTE ARQUIVO VEIO COM UM NOME QUE O SEELE
     NÃO GRAVA».
   - Encurte a segunda linha da composta para perto do `LIMITE_DE_FRASE` (180), sem perder o que ela diz.
   - O guarda `a_recusa_do_nome_so_diz_o_que_a_regra_pode_ter_pegado` continua verde.

NÃO fazer: mudar o fio, a API de MODs ou a CSP; editar pendências e índice (é o lote Z).

## Lote V-captura-na-tela-parada — o teste da captura separa «a tela não mudou» de «a captura travou» e de «um quadro foi jogado fora»

Vem da bateria do lote K1. O teste
`captura::macos::testes::a_captura_entrega_quadros_ao_longo_do_tempo` (`crates/seele-video/src/captura/macos.rs`
~:1087-1128) reprova nesta máquina, e reprova também na base `39b1e58`, com o `seele-video` intocado desde `3685a64`:

```
a captura entregou 17 quadros em três segundos ... escritos pelo sistema: 17
```

O que foi medido:
- o primeiro monitor de `fontes()` é um externo vertical de 1080×1920 parado, e deu `pegos=17`, `escritos=17` e
  `sem_conteudo=73`, com escritos por segundo `[17, 0, 0]`;
- as 90 amostras chegaram, e 73 vieram sem conteúdo porque a tela não mudou;
- no monitor principal, que tinha atividade, a mesma captura dá 88 quadros.

A frase do teste («A tela parada não explica: o ScreenCaptureKit reentrega o mesmo quadro») é falsa aqui.

**O cuidado que manda neste lote.** O comentário em `macos.rs` ~:539-546 conta o defeito real que a captura já teve:
um `frame_status()` desconhecido (`None`) tratado como «sem pixels» descartou 145 quadros que tinham imagem, sem uma
linha de erro. Se o teste novo só passar a contar `sem_conteudo` como entrega, esse defeito volta e passa calado.
Afrouxar não vale.

1. **A decisão vira uma função pura, com teste de unidade.** A escolha entre «sem conteúdo» e «converter», feita hoje
   no tratamento da amostra (~:536-569), sai para uma função sem ScreenCaptureKit, por exemplo
   `classificar(estado: Option<…>, tem_imagem: bool) -> …`, chamada pelo tratamento real.
   - O teste de unidade roda em qualquer Mac, sem permissão de gravação de tela, e prova:
     - estado desconhecido com imagem → converter, e nunca «sem conteúdo»;
     - estado `Idle`, `Blank` ou `Suspended` → sem conteúdo;
     - estado com conteúdo → converter.
   - Reversão: reintroduza o defeito antigo (`None` → «sem conteúdo») e veja o teste vermelho com uma frase que diga
     o que quebra.
   - Se o tipo do estado da `screencapturekit` não puder ser construído num teste, use um enum próprio que espelhe o
     do crate, com uma conversão testada. Justifique no relatório.
2. **O teste ao longo do tempo cobra o que pode provar.**
   - As amostras continuam chegando: `escritos + sem_conteudo`, medido por segundo, com um mínimo em cada um dos três
     segundos. Uma captura que trava depois das primeiras reprova, com a tela parada ou não.
   - Pelo menos um quadro com imagem chega, como hoje.
   - Quando a tela ficou parada (poucos `pegos` e muitos `sem_conteudo`), o teste passa e diz isso em voz alta no
     `stderr`, com os números.
   - A mensagem de falha diz o que foi medido, sem a frase falsa.
   - Escolher o monitor principal em vez do primeiro da lista é opcional. Se escolher, diga como ele é achado.
   - Reversão: um `tomar()` que para de entregar depois do primeiro segundo, num teste ou num simulacro, reprova.
3. Rode `cargo test -p seele-video` várias vezes nesta máquina: verde com o monitor externo parado. Escreva no
   relatório os números de cada rodada.

NÃO fazer: mudar o código de produção além de extrair a função; tirar o teste; marcar `#[ignore]`.

## Acréscimos aos lotes, vindos das revisões da onda 1

### Acréscimo ao lote H-recusa-legivel (vem do K1)

- **K1-m1** (apps/seele-app/tests/frontend.rs:8165-8167 (doc de o_dono_da_regiao_leva_cada_recusa_de_midia_ao_registro)). O doc de um guarda do frontend.rs ficou desatualizado depois do 4599e14. O doc diz que «as seis bancadas de node do cargo xtask check-runtime [...] ficam verdes com o dono sem anotarRecusa (medido em 01/10/2026)». Desde 4599e14, a contribuicoes-e-camadas reprova nesse caso. Medi numa cópia em scratchpad, trocando `anotarRecusa:` por `anotarRecusaX:` em base.js: a bancada saiu com 1 e com «o elo · anotarRecusa: o dono de `base.js` não tem `anotarRecusa`…». A frase tem data, então ainda é verdade como registro histórico, mas quem a ler hoje vai achar que esse caso só é guardado pelo frontend.rs e pela bancada de navegador. O frontend.rs não está na lista de arquivos do K1, e o relatório já aponta isso (preocupação 3). Conserto: Quem mexer depois no frontend.rs (lote C ou Z) acrescenta que, desde 4599e14, o bloco «o elo» da contribuicoes-e-camadas também reprova com o dono sem `anotarRecusa`, e com o `meu()` invertido.

### Acréscimo ao lote K2-midia-provada-e-contada (vem do K1)

- **K1-m2** (apps/seele-app/ui/mods-regiao.js:402 (`pedidoDeArquivo`, `finalidade.slice(0, 200)`) → base.js:905 `invoke("escolher_para_o_mod", …)` → main.rs:5834 `finalidade: Option<String>`). Outro texto de MOD que vai a um invoke ainda é cortado por índice (fora do escopo). O defeito é o mesmo do P51-06. Se o caractere da posição 200 for um par substituto, o corte o parte ao meio, e o serde_json 1.0.151 (read.rs:913/959, conferido) recusa a cadeia: o pedido de arquivo inteiro volta recusado. Não é calado para o MOD, porque o `.catch` da região manda `resultado: "falhou"` com o motivo. O lote limitou o item à linha 809, e o implementador o listou como preocupação 6, sem medir. Conserto: Entra no inventário como item de um lote futuro: cortar a `finalidade` por ponto de código, como a 809 e a 1442, com uma prova em regiao-do-mod.cjs pelo mesmo molde da `aChaveDoCampoNaoPartePar`.
  Medido pelo corretor do K1: com a finalidade "a"×199 + 😀 cortada em 200, o JSON.stringify deixa \ud83d solto, e o serde_json 1.0.151 recusa o corpo inteiro (unexpected end of hex escape). O conserto do K1 para o registro (ef707c7) é o modelo.
