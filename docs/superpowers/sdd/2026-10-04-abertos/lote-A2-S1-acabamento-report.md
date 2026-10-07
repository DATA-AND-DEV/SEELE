# Lote A2-S1-acabamento — relatório

Base `34235fe`, branch `conserto/abertos-da-1.0`. HEAD depois do lote: `56d3bdc`. Quatro commits locais, nenhum
empurrado (sem push, tag nem release). Nenhum assunto contém uma das catorze cadeias do G1: conferi cada assunto com
`grep -F -f` contra as catorze linhas entre `CADEIAS` do plano 1B (`docs/superpowers/plans/2026-09-29-1.0-plano-1b-a-impressao-no-tls.md:2839-2852`).
Todos os commits terminam com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

| Commit | Item | O quê |
|---|---|---|
| `0646c87` | 3 | `RESERVADOS` ganha `COM0`, `LPT0`, `CONIN$` e `CONOUT$`, e o doc diz de onde vem a lista |
| `9461983` | 1 | o anexo chega num parcial (`.foto.png.seele-parcial`) e só ganha o nome final por `hard_link` depois de o hash bater; recuo sem link físico; quarentena no parcial |
| `9c05bf2` | 2 | apagar o parcial ou a reserva e falhar escreve `warn!` com o caminho e o erro; `NAO_SALVOS.Falhou` e o doc de `NotSavedReason::Falhou` dizem o que o código faz |
| `56d3bdc` | 4 | o título da recusa do nome nos dois lugares, a segunda linha da composta com 179 caracteres, e o guarda passa a medi-la |

A ordem dos commits não é a da lista: o item 3 é independente e saiu primeiro; o 2 depende do `Parcial` do 1.

## O que foi feito, por item

### 1. O arquivo parcial nunca tem o nome final

Em `crates/seele-core/src/anexo_no_disco.rs`:

- **`abrir_parcial(pasta, nome, tentativas) -> io::Result<(File, Parcial)>`**, no lugar de `abrir_sem_sobrescrever`
  (removida: uma função pública que cria o arquivo já com o nome final é o defeito esperando um chamador novo).
  Ela confere o nome com `nome_seguro` e cria o parcial com `create_new`. Se um parcial do mesmo nome já existe (outro
  anexo do mesmo nome chegando, ou um processo que morreu), ela passa a `.foto.png (2).seele-parcial` e não toca no
  que está lá, porque não dá para saber se ele ainda está sendo escrito. O nome do parcial é cortado numa fronteira
  de caractere para caber em 255 bytes: o ponto e o sufixo somam quinze.
- **O nome `.<nome>.seele-parcial`, e por quê** (está também no doc de `SUFIXO_DO_PARCIAL`):
  - o ponto na frente esconde o parcial no Finder e no `ls`. No Explorer do Windows quem esconde é um atributo, e
    não o nome; ver as preocupações;
  - o nome que veio, no meio, diz a quem achar o arquivo de que anexo ele é;
  - o sufixo troca a extensão: o parcial de um `x.exe` não é um `.exe`, e um duplo clique nele não executa nada nem
    abre metade de uma imagem;
  - «parcial» é a palavra que o servidor já usa para o mesmo papel (`SCRATCH_SUFFIX = ".parcial"` em
    `seele-server/src/persistence/attachments.rs`), e «seele» diz qual programa o deixou na pasta da pessoa.
- **`Parcial::nomear(self) -> io::Result<PathBuf>`**: tenta «foto.png», «foto (2).png»… (até o `tentativas` dado a
  `abrir_parcial`) com `std::fs::hard_link`, que falha com `AlreadyExists` se o nome existir. Então um nome tomado no
  meio-tempo passa ao sufixo seguinte, e nada é substituído. Depois o nome de parcial sai.
- **O recuo sem link físico (FAT/exFAT)**: qualquer erro do link que não seja `AlreadyExists` faz o recuo rodar,
  porque o macOS devolve ENOTSUP como `ErrorKind::Uncategorized`, e não `Unsupported` (medido; ver abaixo). O recuo,
  `reservar_e_trocar`, faz duas coisas:
  - reserva o nome final com `create_new`, que falha se o nome existir, como o link;
  - troca a reserva pelo parcial com `rename`.

  Ele nunca passa por cima de um arquivo da pessoa: o `rename` só substitui a reserva vazia que a mesma chamada
  acabou de criar. O que ele não tem é a atomicidade do link, e o doc diz isso: se outro programa trocasse a reserva
  por um arquivo dele no instante entre os dois passos, o `rename` passaria por cima; e um processo que morre entre
  os dois deixa uma reserva **vazia** com o nome final, e não truncada. Quando o recuo roda, uma linha `INFO` vai ao
  `seele.log` («o volume não aceitou link físico…», com o caminho e o erro).
  - Por que não «criar com `create_new` e copiar»: a cópia deixa o arquivo truncado com o nome final durante a cópia
    inteira, que num pendrive FAT leva segundos — justo o defeito deste item.
  - Por que não `rename` com «não substituir»: não existe no std; `renamex_np`/`renameat2` pediriam `libc` e código
    por plataforma.
- **`Drop for Parcial`**: solto sem nome, ele apaga o parcial. Isso cobre toda falha, inclusive um `?` qualquer ou
  uma tarefa largada antes do fim, sem depender de quem chama. Só um processo morto não passa por aqui.
- **A quarentena fica no arquivo final**: `receive_attachment` marca o **parcial** (`quarantine(parcial.caminho())`)
  antes de `nomear`. O link é outro nome para o mesmo inode, e a troca leva os atributos junto, então o nome final
  nunca existe sem a marca. Marcar e nomear rodam num `spawn_blocking`, porque o `xattr` é um processo.

Em `crates/seele-core/src/client.rs`:

- `Transfers::receive_attachment(attachment, arquivo, parcial: Parcial, wait, progress) -> Result<(u64, PathBuf)>`
  devolve o caminho real. Numa falha, o `?` solta o `Parcial`, e é isso que apaga o arquivo.
- `Client::download_attachment(…, destination, …)`, a porta da conformidade, também grava num parcial ao lado de
  `destination` (`abrir_parcial(pasta, nome, 1)`, só o nome exato). Um `destination` que já existe é recusado antes
  do pedido, como atalho para não baixar à toa. O doc diz que quem garante é o link, e a reversão R9 abaixo prova.

Em `crates/seele-core/src/enlace.rs`: `Comando::SalvarAnexo` abre com `abrir_parcial(…, TENTATIVAS)` antes do
`fetch_attachment`. Um nome recusado continua sem pedir byte nenhum ao servidor. `fim_do_salvar(anexo, parcial: &Path,
recebido: Result<(u64, PathBuf)>)`: o `Salvo` leva o caminho que `nomear` devolveu, e a linha da falha leva o caminho
do parcial, que diz a pasta e o nome.

A frase `ARQUIVO SALVO — <caminho real>` (`tela-sessao.js:3131`) já mostrava `transfer.path`; agora esse caminho é o
que `nomear` deu. A prova está em `um_nome_repetido_nao_substitui_o_que_ja_estava_la`, pela ponte, que recebe
`Saved { path: …/foto (2).png }`.

Em `crates/seele-ffi/src/types.rs`, o doc de `NotSavedReason::Falhou` passou a dizer que os noventa e nove nomes
tomados só aparecem na hora de dar o nome final, depois do hash.

### 2. Apagar e falhar é dito

- `apagar_dizendo(caminho, o_que)` em `anexo_no_disco.rs` escreve
  `warn!(caminho, erro, "não consegui apagar {o_que}, e ele ficou na pasta")`. Os dois `let _ =
  tokio::fs::remove_file(...)` de `client.rs` (em `receive_attachment` e `download_attachment`) já tinham virado o
  `Drop` do `Parcial` no item 1, e é ele que agora chama `apagar_dizendo`. O segundo usuário é a reserva vazia do
  recuo, o único jeito de uma falha deixar o nome final na pasta. **`NotFound` não é falha**: o que se queria era que
  o arquivo não estivesse lá, e ele não está. Isso está no doc da função.
- `NAO_SALVOS.Falhou` (`frases.js`): de «Nada foi gravado pela metade; tente de novo.» para «O SEELE apaga o que
  começou a gravar e, se não conseguir, diz no seele.log; tente de novo.» (90 caracteres na segunda linha). Confiro
  contra o código:
  - «apaga o que começou a gravar»: é o `Drop` do `Parcial` e o `inspect_err` da reserva;
  - «se não conseguir, diz no seele.log»: é o `warn!` de `apagar_dizendo`;
  - «seele.log» já é palavra pública, em `docs/alcance-pela-internet.md:310` e `docs/ponto-de-encontro.md:86`.
- Doc de `NotSavedReason::Falhou` (`types.rs`): «O que foi criado saiu» virou um parágrafo que diz o seguinte, e cada
  parte foi conferida no código:
  - os bytes chegavam num parcial e nunca tiveram o nome final;
  - o parcial é apagado, e também a reserva do recuo;
  - um apagar que falha vai para o `seele.log` com o caminho e o erro, e o que ele não apagou fica na pasta.

### 3. A lista de reservados

`RESERVADOS` passa de 28 para 32: `CONIN$`, `CONOUT$`, `COM0` e `LPT0`. Os casos novos de
`um_nome_alegado_so_vira_nome_de_arquivo_se_for_so_um_nome` são `COM0`, `lpt0`, `com0.txt`, `LPT0.tar.gz`, `CONIN$`,
`CONOUT$`, `conin$.txt` e `Conout$.log`. Dois nomes parecidos continuam passando: `conin.txt` e `lpt00.txt`.

O doc da lista diz de onde ela vem: a página «Naming Files, Paths, and Namespaces» do Win32 (CON, PRN, AUX, NUL,
COM0–9, COM¹²³, LPT0–9, LPT¹²³, também com extensão), mais `CONIN$`/`CONOUT$`, os nomes do console que o `CreateFile`
abre como dispositivo. O doc diz também o que conferi no Python 3.14.5 desta máquina:

- `ntpath.isreserved`, que cita a mesma página, recusa `CONIN$` e `conout$.log`;
- e não recusa `COM0` nem `lpt0.txt`.

Os zeros ficam assim mesmo, e o doc diz por quê: recusar um nome a mais custa a quem mandou escolher outro, e gravar
num dispositivo custa o arquivo.

### 4. O título da recusa

- Título nos dois lugares (`NAO_SALVOS.NomeRecusado` e a composta de `fraseDeNaoSalvo`): **«ESTE ARQUIVO VEIO COM UM
  NOME QUE O SEELE NÃO GRAVA NO WINDOWS, NO MAC NEM NO LINUX, E NADA FOI SALVO.»**
  - Vale para as dez regras porque descreve o que o SEELE faz, e `nome_seguro` não tem `cfg`, então a regra é a mesma
    nos três sistemas. Não acusa o nome.
  - Leva «no Windows, no Mac nem no Linux» para o título, e isso explica a quem está no Mac por que «Por quê?.pdf» cai
    numa regra do Windows.
  - Não usei «não serve no Windows, no Mac e no Linux», porque é falso para um nome com U+202E: os três sistemas
    aceitam esse nome, e ele é recusado por disfarçar.
  - «NADA FOI SALVO», e não «GRAVADO», para não dizer «NÃO GRAVA, E NADA FOI GRAVADO».
- Segunda linha da composta: «`«${claimed}»` tem um caminho, um caractere ou nome que o Windows não aceita, um
  caractere invisível que disfarça o nome, ou é comprido demais. Para gravar em `${folder}`, peça outro nome a quem
  mandou.»
  - Tem **179** caracteres fora o nome e a pasta; tinha 259. Renderizada no node com «Por quê?.pdf» e
    «/Users/x/Downloads», dá 209.
  - Mantém o nome citado, as dez regras (a tabela do guarda passa), a pasta e o que fazer. O «só grava um nome que
    sirva no Windows, no Mac e no Linux» foi para o título.
- **Guarda**: `a_recusa_do_nome_so_diz_o_que_a_regra_pode_ter_pegado` continua verde e ganhou duas cobranças.
  - A segunda linha da composta, sem os `${…}` (ajudante novo `sem_interpolacoes`), passa pela régua de
    `LIMITE_DE_FRASE`. Isso fecha a preocupação 3 da rodada 1 do A-S1, «nenhuma régua mede as compostas», para esta
    frase.
  - A composta e `NAO_SALVOS.NomeRecusado` têm de começar pelo mesmo título.
  - O comentário da régua das três linhas diz que esta composta agora também é medida.

## RED / GREEN

| Item | RED antes do conserto | GREEN |
|---|---|---|
| 3 | os casos novos contra a lista de 28: «"COM0" passou como nome de arquivo, e não é um nome que sirva no Windows, no Mac e no Linux ao mesmo tempo» | 4/4 em `anexo_no_disco` |
| 1 (ponta a ponta) | `um_download_pela_metade_nunca_tem_o_nome_final`, escrito contra o código de `34235fe`: «com 0 de 300000 bytes gravados, a pasta já tinha um «foto.png»: se o processo morresse ali, ficaria um arquivo truncado com o nome de um completo. Pasta: ["foto.png"]». O teste olha a pasta dentro do callback de andamento, que roda no meio da gravação, entre um bloco e o seguinte, e cobra que houve ao menos um ponto com `0 < feito < total` (houve). | 15/15 em `anexos` |
| 1 (unidade) | testes novos contra stubs com o comportamento antigo (o parcial criado já com o nome final): 3 vermelhos (ver o primeiro quadro abaixo) | 9/9 em `anexo_no_disco` |
| 2 | `um_parcial_que_nao_sai_da_pasta_fica_dito_no_log` e `uma_reserva_que_nao_sai_da_pasta_fica_dita_no_log`, contra o `let _`: os dois «… o `seele.log` não diz qual nem por quê … Rastro: []» | 11/11 |
| 4 | a régua nova contra a composta de antes: «a segunda linha da frase que cita o nome recusado tem 259 caracteres fora o nome e a pasta, e a régua das frases é 180» | `frontend` 259/259 |

Os três vermelhos da unidade no item 1:

- `o_parcial_nunca_tem_o_nome_final_e_so_ganha_o_nome_no_fim`: «o arquivo que ainda está chegando já tem o nome
  final…»;
- `o_parcial_de_um_nome_de_255_bytes_cabe_em_255`;
- `um_parcial_que_nao_ganhou_nome_sai_da_pasta`.

`dar_o_nome_final_nunca_substitui…` e `sem_link_fisico…` passavam contra o stub, que gravava ao lado na abertura; os
dois foram provados por reversão (R2, R3, R8).

Os testes de item 2 fazem o apagar falhar **em qualquer sistema e também como root**, sem `chmod`: o parcial, ou a
reserva, vira uma pasta, e `remove_file` não apaga pasta. Por isso eles não têm `cfg`. A reserva usa um ponto de
injeção novo, o parâmetro `trocar` de `nomear_com`, ao lado do `ligar` que já servia ao teste do recuo.

Estado final (HEAD `56d3bdc`):

- `cargo test -p seele-core --lib`: 415 ok;
- `cargo test -p seele-ffi --lib`: 120 ok, 1 ignorado de antes;
- `cargo test -p seele-conformance --test anexos`: 15 ok;
- `cargo test -p seele-app --test frontend`: 259 ok;
- `cargo xtask check-api`: verde;
- `cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets --all-features -- -D warnings`: limpos antes
  de cada commit, só com o aviso ambiental `ld: duplicate -rpath`;
- `cargo doc -p seele-core -p seele-ffi`: nenhum aviso de doc nas linhas deste lote. Os avisos que há são de antes,
  em `bomba.rs`, `conhecidos.rs` e no `//!` de `client.rs`.

## Reversões

Cada uma desfaz o conserto, roda e vê o vermelho com a frase, depois restaura da cópia e confere com `cmp`. Todas
deram «restaurado byte a byte».

| # | Reversão | Vermelho |
|---|---|---|
| R3a | sem `"COM0"` | «"COM0" passou como nome de arquivo…» |
| R3b | sem `"LPT0"` | «"lpt0" passou como nome de arquivo…» |
| R3c | sem `"CONIN$", "CONOUT$"` | «"CONIN$" passou como nome de arquivo…» |
| R1 | o parcial nasce com o nome final e fica com ele (`nome_ao_lado` no lugar de `nome_do_parcial`, e `nomear` aceita o próprio caminho) | unidade: «o arquivo que ainda está chegando já tem o nome final…»; ponta a ponta: «com 0 de 300000 bytes gravados, a pasta já tinha um «foto.png»…». Uma primeira versão de R1 (só o nome) deu vermelho por outro motivo («os 1 nomes permitidos… já existem»), e foi refeita para reproduzir o comportamento antigo. |
| R2 | `nomear` com `rename` no lugar de `hard_link` | «os anexos não foram gravados ao lado do «foto.png» que já estava na pasta»; pela ponte: «o anexo não foi salvo ao lado, ou a tela não recebeu o caminho real…» |
| R3 | o recuo troca sem reservar | «sem link físico, o anexo não foi gravado ao lado do que já estava na pasta» |
| R8 | o recuo só para `ErrorKind::Unsupported` | «sem link físico, o anexo não ganhou nome nenhum: o recuo não rodou, ou falhou: Os { code: 45, kind: Uncategorized … }». O teste injeta o erro 45 medido, e não `Unsupported`. |
| R4 | soltar o `Parcial` não apaga | unidade: «um anexo que não chegou deixou alguma coisa na pasta»; hash estragado pela ponte: «o anexo que não fechou com o hash deixou o parcial na pasta»; expirado: «um anexo que não veio deixou o parcial na pasta: [".nao-vem.bin.seele-parcial"]» |
| R5 | depois do link, o nome de parcial fica | «depois de ganhar o nome final, o parcial continua na pasta», mais as duas de ponta a ponta («…o parcial ficou para trás») |
| R6 | sem `quarantine(parcial.caminho())` | «o arquivo salvo não tem a quarentena do SEELE… No such xattr: com.apple.quarantine» |
| R7 | o nome do parcial sem o corte | «o parcial de um nome de 255 bytes não abriu: sem o corte… LongoDemais» (quem segurou foi a conferência de `nome_seguro` no candidato, antes do disco) |
| R9a | `download_attachment` sem o atalho de «já existe» | **verde**, de propósito: o link segura sozinho |
| R9b | R9a e `rename` no lugar do link | «baixar por cima de um arquivo que já existe deu certo, e só dá certo substituindo o que estava lá» |
| R10 | o `Drop` volta ao `let _ = remove_file` | «o parcial de um anexo não saiu da pasta e o `seele.log` não diz qual nem por quê… Rastro: []» |
| R11 | a reserva volta ao `let _ = remove_file` | «a reserva com o nome final não saiu da pasta e o `seele.log` não diz… Rastro: []» |
| R12 | o `warn!` sem `%erro` | o mesmo texto de R10, com a linha `WARN … caminho=…` sem `erro=` no rastro |
| R13 | a segunda linha de antes (259) | «…tem 259 caracteres fora o nome e a pasta, e a régua das frases é 180» |
| R14 | o título de antes só no dicionário | «a recusa que cita o nome e a que não cita começam por títulos diferentes…» |
| R15 | a composta nova sem «comprido demais» | o guarda de antes ainda morde: «um nome recusado por `LongoDemais` lê uma frase que não diz «comprido demais»…» |

Durante R13 e R14, um script de reversão quebrou num acento grave e deixou `frases.js` com as duas reversões
aplicadas. Reconstruí o arquivo a partir de `HEAD` mais as edições do item 4. Conferi o diff linha a linha contra o
que tinha mostrado antes, e refiz R13, R14 e R15 com um script seguro, conferindo depois que `frases.js` era igual à
cópia da intenção. O commit `56d3bdc` tem a versão certa: o guarda está verde e as reversões ficam vermelhas.

## Medido no Mac, fora da bateria

Montei duas imagens de disco no scratchpad, uma exFAT e uma FAT32 (`hdiutil attach -nobrowse`), e desmontei e apaguei
as duas no fim.

- **`std::fs::hard_link` nelas devolve `Os { code: 45, kind: Uncategorized, "Operation not supported" }`.** Não é
  `Unsupported`. Foi isso que decidiu «qualquer erro que não seja `AlreadyExists` faz o recuo rodar».
- **O caminho real do código** (`abrir_parcial` + `xattr` no parcial + `nomear`), num teste temporário que não foi
  commitado, com um `foto.png` da pessoa já na pasta, deu nos dois volumes:
  - `foto (2).png` com os bytes;
  - `foto.png` ainda «original»;
  - a quarentena `0083;0;SEELE;` no arquivo final;
  - nenhum parcial sobrando (só os `._` do AppleDouble que o macOS cria em FAT);
  - a linha `INFO` do recuo, com `erro=Operation not supported (os error 45)`.
- No APFS, a quarentena no arquivo final passando pelo `hard_link` é provada pelo teste de conformidade
  (`xattr -p`, `#[cfg(target_os = "macos")]`).

## Arquivos mudados

- `crates/seele-core/src/anexo_no_disco.rs`;
- `crates/seele-core/src/client.rs`;
- `crates/seele-core/src/enlace.rs`;
- `crates/seele-ffi/src/types.rs` (só doc);
- `crates/seele-conformance/tests/anexos.rs`;
- `apps/seele-app/ui/frases.js`;
- `apps/seele-app/tests/frontend.rs`.

Ficaram intocados o fio, `PROTOCOL_VERSION` (8), o SEELE-ENC/1, `api/*.json`, a CSP, `tauri.conf.json` e
`apps/seele-app/src/main.rs` (onde fica o conflito com o mobile/ios). Nenhuma dependência nova, nada baixado.

## O que não pôde ser compilado nem rodado aqui

- O ramo `#[cfg(target_os = "windows")]` de `quarantine` (o `Zone.Identifier`) não compila neste Mac. O código dele
  não mudou, mas agora recebe o caminho do parcial. Que o fluxo alternativo vai junto com o `CreateHardLinkW` vem de
  como o NTFS guarda um link (o mesmo registro de arquivo, com todos os fluxos), e não foi medido.
- O Windows inteiro ficou sem rodar:
  - `hard_link` no NTFS;
  - o recuo num FAT32 do Windows (o `rename` do std substitui a reserva por `MoveFileExW`/`SetFileInformationByHandle`);
  - um antivírus segurando a reserva vazia, que faria a troca falhar e o anexo cair em `Falhou`.
- No Linux, o EPERM de um vfat vem do `vfs_link` do kernel e não foi medido aqui. O código não depende do número: só
  `AlreadyExists` fica de fora do recuo.
- A bateria inteira do lote não foi rodada por mim (`cargo test --workspace`, check-versao, check-runtime,
  bancadas). Rodei só os testes pontuais acima, o clippy do workspace e o `check-api`.

## Autorrevisão

- Reli o diff inteiro (`git diff 34235fe..HEAD`) e conferi cada frase nova de doc e de tela contra o código. A
  conferência derrubou três frases:
  - «oculto» saiu do doc de `Falhou`, porque no Windows o ponto não esconde;
  - «a tarefa cancelada quando a sessão fecha» virou «uma tarefa largada antes do fim», porque nada aqui cancela a
    tarefa ao fechar a sessão;
  - o EPERM do Linux ficou marcado como vindo do kernel, e não medido.
- O primeiro desenho do recuo só rodava para `ErrorKind::Unsupported`. A medida no FAT mostrou que ele nunca rodaria
  no Mac; consertado antes do commit, e a R8 o guarda.
- O teste do recuo injetava `Unsupported`, e a R8 passaria verde com ele. Passou a injetar o erro 45 medido.
- `abrir_sem_sobrescrever` saiu da API pública, em vez de ficar ao lado de `abrir_parcial`.
- O `Drop` faz uma chamada síncrona ao disco dentro de uma tarefa assíncrona, como o `quarantine` já fazia. É um
  `unlink`.

## Preocupações

1. **O Explorer do Windows mostra o parcial.** O ponto só esconde no Mac e no Linux, e marcar
   `FILE_ATTRIBUTE_HIDDEN` pediria código `cfg(windows)` que não roda aqui. O assunto do commit `9461983` diz «parcial
   oculto», o que vale só para Mac e Linux.
2. **Um parcial de um processo morto fica para sempre.** O servidor varre os seus `.parcial` ao subir; o cliente não
   pode, porque não sabe se outro processo do SEELE (outra janela, outra conta) ainda está escrevendo nele. Ele é
   oculto no Mac e no Linux e tem nome de parcial.
3. **Os noventa e nove nomes tomados agora só aparecem depois do download.** Antes, a abertura com o nome final
   recusava antes do pedido. Um nome recusado por `nome_seguro` continua sem pedir byte nenhum.
4. **O recuo não tem a atomicidade do link.** Isso está escrito no doc de `Parcial::nomear`:
   - outro programa que trocasse a reserva por um arquivo dele no instante entre os dois passos seria substituído;
   - um processo que morre entre os dois deixa uma reserva vazia com o nome final.
5. **Queda de energia não foi tratada.** Não há `fsync` antes do link. A lista fala de processo morto, e numa queda
   de energia alguns sistemas de arquivos podem expor o nome final antes dos dados. Um `sync_data` antes de `nomear`
   resolveria, ao custo de uma descarga por anexo.
6. **O título longo.** Na tela, a composta sai com 102 caracteres na primeira linha e 209 na segunda (com «Por
   quê?.pdf» e «/Users/x/Downloads»). A segunda tem 179 fora o nome e a pasta, «perto» de 180 como a lista pede.
7. **Fora da lista e não mexido: o doc de `NotSavedReason::NomeRecusado`.** Ele diz que o nome «não serve de nome de
   arquivo no Windows, no Mac e no Linux ao mesmo tempo», e isso é falso para um nome com U+202E, que os três
   sistemas aceitam (ele é recusado por disfarçar). O título novo da tela evitou essa forma.
8. **`COM0` e `LPT0` na página da Microsoft não foram conferidos daqui**, porque não há rede. O que foi conferido
   localmente é o `ntpath` do Python; o doc diz qual parte é qual.
9. O registro em `docs/pendencias.md`, no índice e na regra do G1 fica com o lote Z, como manda a lista; este lote
   não ganha cadeia.

## Rodada 1

Base `56d3bdc`. HEAD depois da rodada: `39b1e58`. Dois commits locais, nenhum empurrado. Os dois assuntos foram
conferidos com `grep -F -f` contra as catorze cadeias do plano 1B (linhas 2839-2852): nenhuma está neles. Os dois
terminam com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

| Commit | Achados | O quê |
|---|---|---|
| `e0c1259` | I1, M1, M2, M3, M4 | «oculto» com condição; o erro do recuo diz o link; o parcial pulado vai ao log; frases condicionadas; concordância; comentário da conformidade |
| `39b1e58` | M5 | o guarda do título cobra que ele não acuse o nome |

Todos os achados foram consertados. Nenhum Minor ficou.

### I1 — «oculto» onde o Windows mostra

Em `crates/seele-core/src/anexo_no_disco.rs`:

- o doc do módulo diz que o parcial que sobra é um parcial «que o ponto na frente esconde no Mac e no Linux, mas
  não no Explorer do Windows»;
- o doc de `Parcial` diz «um arquivo com `SUFIXO_DO_PARCIAL` — oculto no Mac e no Linux, à vista no Explorer do
  Windows»;
- o doc de `apagar_dizendo` diz «o parcial, com o nome de parcial», sem «oculto»;
- a mensagem do teste `o_parcial_nunca_tem_o_nome_final_e_so_ganha_o_nome_no_fim` diz o que ele confere: «o
  parcial não começa pelo ponto que o esconde no Mac e no Linux, não tem o nome que veio ou não tem o sufixo de
  parcial».

Sobrou «oculto» sem condição só nas linhas 668 e 672, que são de antes deste lote e falam de um nome como `.bashrc`, e não do
parcial. O assunto do commit `9461983` continua dizendo «parcial oculto». Ele está três commits atrás e não foi
reescrito: mudar um assunto já feito pede rebase. Isso fica como preocupação para quem integra.

### M1 — o recuo que falha e o parcial pulado

São dois consertos com comportamento, e os dois foram feitos com TDD.

- **O erro do recuo diz o link.** Em `Parcial::nomear_com`, quando o recuo já rodou e falha com algo que não é
  `AlreadyExists`, o erro que volta fica assim:

  `io::Error::new(<tipo do erro do recuo>, "o volume não aceitou link físico (<erro do link>), e o recuo, que
  reserva o nome final e troca a reserva pelo parcial, falhou: <erro do recuo>")`

  - É esse erro que a linha «o anexo não foi salvo» de `enlace::fim_do_salvar` leva ao `seele.log`. Uma linha só
    passa a contar a história inteira.
  - O tipo do erro é o do recuo, porque é ele que impediu o nome final.
  - Sem recuo, o erro volta como veio.
  - O `# Errors` de `nomear` diz isso.
  - Escolhi pôr no erro devolvido, e não numa linha própria, para não haver duas linhas da mesma falha, uma sem a
    outra.
- **O parcial pulado vai ao log.** Em `abrir_parcial`, um `AlreadyExists` no nome de parcial escreve
  `tracing::info!(caminho, "já havia na pasta um arquivo com o nome de parcial deste anexo, …; ele ficou como
  estava, e o anexo vai para o nome de parcial seguinte")`.
  - O nível é `INFO`, e não `DEBUG`, porque o filtro padrão do `seele.log` é `seele_core=info`
    (`apps/seele-app/src/main.rs:8602`). Uma linha em `DEBUG` seria o produto sabendo e não contando.
  - No pior caso são 99 linhas, uma por nome de parcial tomado.

Os testes novos ficam no módulo de testes de `anexo_no_disco.rs`:

- `um_recuo_que_falha_diz_que_o_volume_recusou_o_link`: o link é recusado com o erro 45 medido, e a troca com
  `PermissionDenied`. O teste cobra três coisas:
  - o texto do erro tem o da troca, «link físico» e o texto do erro 45;
  - o tipo é `PermissionDenied`;
  - a pasta fica vazia.
- `um_parcial_que_ja_estava_na_pasta_fica_como_estava_e_e_dito_no_log`: põe na pasta um
  `.foto.png.seele-parcial` com «de antes» e cobra três coisas:
  - o anexo vai para `.foto.png (2).seele-parcial`;
  - o de antes fica intacto;
  - há uma linha `INFO` com o caminho dele.

`uma_reserva_que_nao_sai_da_pasta_fica_dita_no_log` comparava o erro inteiro com «a troca foi recusada pelo teste».
Ele passou a cobrar que o erro contenha essa frase, porque o texto agora vem embrulhado.

### M2 — frases sem condição

- **Quarentena** (`client.rs`, doc de `receive_attachment`): «assim o nome final nunca existe sem ela» virou «o
  arquivo com os bytes nunca tem o nome final sem ela — quando o sistema de arquivos guarda a marca». O doc
  acrescenta duas coisas:
  - um FAT ou exFAT no Windows não tem onde guardar o fluxo `Zone.Identifier`, e lá o arquivo fica sem marca;
  - no recuo, a reserva tem o nome final sem a marca por um instante, vazia.
- **Quem garante** (`client.rs`, doc de `download_attachment`): passa a dizer «o link, ou, num volume sem link
  físico, a reserva do recuo de `Parcial::nomear`». A mesma condição entrou na frase do link do mesmo doc e no doc
  do módulo `anexo_no_disco`, que só falavam do link.
- **`TENTATIVAS`**: o doc diz que `nomear` tenta o `tentativas` que `abrir_parcial` recebeu, e não a constante. O
  app passa `TENTATIVAS`, e a porta da conformidade passa um. Os nomes de parcial são sempre `TENTATIVAS`.

### M3 — concordância

O modelo do `warn!` de `apagar_dizendo` passou a «não consegui apagar {o_que}, e o arquivo ficou na pasta». Ele
concorda com os dois chamadores: «o parcial de um anexo» e «a reserva vazia com o nome final de um anexo».

### M4 — o comentário da conformidade

O comentário de `baixar_num_caminho_que_ja_existe_nao_toca_no_que_estava_la`
(`crates/seele-conformance/tests/anexos.rs`) diz agora três coisas:

- os bytes chegam num parcial ao lado;
- quem recusa este caminho é o atalho de `download_attachment`, antes de pedir byte nenhum;
- o atalho perde uma corrida, e quem garante, numa pasta com link físico como a do teste, é o link.

A R9a da rodada 0 já tinha mostrado o teste verde sem o atalho.

### M5 — o título que acusa

Em `a_recusa_do_nome_so_diz_o_que_a_regra_pode_ter_pegado` (`apps/seele-app/tests/frontend.rs`), depois da
igualdade dos dois títulos, o título de `NAO_SALVOS.NomeRecusado` não pode conter «NÃO É SÓ UM NOME». Também não
pode conter estes radicais: «TRUQUE», «DISFARÇ», «SUSPEIT», «PERIGOS», «MALICIOS», «ATAQUE» e «HOSTIL».

A lista mede só o título, porque a segunda linha diz, com razão, «um caractere invisível que disfarça o nome».
Como a igualdade roda antes, medir um título basta.

### RED / GREEN e reversões

| # | O que foi feito | Vermelho |
|---|---|---|
| RED M1a | o teste novo contra `56d3bdc` | «o recuo falhou e o erro que volta não diz que o volume recusou o link nem com que erro («Operation not supported (os error 45)»)… Erro: a troca foi recusada pelo teste» |
| RED M1b | o teste novo contra `56d3bdc` | «um parcial que já estava na pasta foi pulado e o `seele.log` não diz qual… Rastro: []» |
| RA | o erro do recuo volta sem o embrulho | a mesma frase de RED M1a |
| RA2 | o embrulho com `ErrorKind::Other`, e não o tipo do recuo | «o erro do recuo voltou com outro tipo, e quem lê o tipo deixa de ver o erro do sistema que impediu o nome final» |
| RB | `abrir_parcial` volta a pular em silêncio (`=> {}`) | a mesma frase de RED M1b |
| RC | o título de antes, «O NOME QUE VEIO COM ESTE ARQUIVO NÃO É SÓ UM NOME, E NADA FOI GRAVADO.», nos **dois** lugares de `frases.js` | «o título da recusa do nome diz «NÃO É SÓ UM NOME» e acusa o nome que veio: a mesma recusa pega «Por quê?.pdf» e «Notas 04:10.txt»…». A igualdade de antes passa nessa reversão; é a lista nova que pega. |

Cada reversão foi feita a partir de uma cópia no scratchpad e depois restaurada. As quatro deram «restaurado byte a
byte» no `cmp`.

M5 não tem RED antes do conserto: o título de hoje não acusa ninguém, e o guarda é contra a volta do título antigo.
O vermelho dele é o RC.

O primeiro desenho do teste de M1a injetava a troca com `io::Error::other`. Com isso, a cobrança do tipo passaria
mesmo com um embrulho que trocasse o tipo por `Other`. Passou a injetar `PermissionDenied`, e a RA2 prova que agora
ele morde.

I1, M2, M3 e M4 só mudam doc, mensagem e comentário, e não têm guarda novo. Um guarda que procurasse «oculto» no
texto casaria com as linhas 668 e 672, que estão certas, e seria um guarda que casa com o próprio comentário.

### Estado final (HEAD `39b1e58`)

- `cargo fmt --all -- --check`: limpo antes de cada commit.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: limpo, só com o aviso ambiental
  `ld: duplicate -rpath`.
- `cargo test -p seele-core --lib`: 417 ok (eram 415, mais os dois testes novos).
- `cargo test -p seele-conformance --test anexos`: 15 ok.
- `cargo test -p seele-app --test frontend`: 259 ok.
- `cargo xtask check-api`: verde.
- `cargo doc -p seele-core --no-deps`: nenhum aviso nas linhas mudadas.

Arquivos mudados nesta rodada:

- `crates/seele-core/src/anexo_no_disco.rs`;
- `crates/seele-core/src/client.rs` (só doc);
- `crates/seele-conformance/tests/anexos.rs` (só comentário);
- `apps/seele-app/tests/frontend.rs`.

Ficaram intocados o fio, `PROTOCOL_VERSION` (8), o SEELE-ENC/1, `api/*.json`, a CSP e `apps/seele-app/src/main.rs`.
Nenhuma dependência nova, nada baixado.

### Preocupações da rodada

1. **O assunto de `9461983` continua dizendo «parcial oculto».** Corrigi-lo pediria reescrever três commits.
2. **O FAT/exFAT do Windows sem `Zone.Identifier` não foi medido aqui.** A frase nova do doc vem de como o Windows
   guarda o fluxo alternativo, que só o NTFS tem. Este Mac não roda o ramo `cfg(windows)` de `quarantine`.
3. **O texto do erro do recuo é texto, e não cadeia de erros.** O `io::Error` embrulhado guarda o tipo, mas não o
   `raw_os_error` do recuo nem o erro como `source`. Hoje ninguém lê nenhum dos dois: `fim_do_salvar` escreve
   `%erro`, e `NaoSalvou` não leva motivo.
4. **A seção «O que foi feito» da rodada 0 ainda diz «o nome final nunca existe sem a marca»**, no item 1, sobre a
   quarentena. Vale a frase condicionada do M2 desta rodada.
