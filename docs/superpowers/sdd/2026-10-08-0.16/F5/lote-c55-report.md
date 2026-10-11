# Lote C55 da F5: a janela confere o tamanho dos campos antes de mandar — relatório

O ramo é `0.16/lotes-c55`, criado a partir de `3b1f57c8` (a ponta do
`0.16/lotes` com a P61). O worktree fica em
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-aebe15871b9aa0f8c`.

É a pendência 58, item 55: desde a P61, o núcleo recusa sem derrubar o enlace
um comando com um campo acima do teto do fio, e registra a recusa no
`seele.log`; mas a janela não ficava sabendo, e o pedido sumia calado. A
janela contava caracteres (`maxlength`), e o fio conta bytes.

**Status: feito, menos o conserto do `Hello` para um apelido já gravado
grande demais.** Esse pede uma variante nova de `ConnectionError`, e o pedido
era avisar antes: está medido e descrito abaixo, à espera da decisão.

## Os commits

| commit | o que faz |
|---|---|
| `20f1f331` | passo 1: a medida do `Hello` com o apelido grande demais (instrumento em `#[ignore]`) |
| `b485674b` | passo 2: os tetos lidos de um lugar só (`limites_dos_campos`), a ponte, o duble e o ajudante da janela |
| `4117b248` | passo 3: o apelido, na sessão e o gravado; o roteiro novo; o guarda das chaves |
| `db2c0f87` | passo 4: o nome de canal e de sala |
| `a2b1396c` | passo 5: os lugares da sala nova; o `250` escrito à mão sai |
| `ff35a7e8` | passo 6: o nome do servidor em CONFIGURAÇÕES |
| `9de4f245` | passo 7: o nome do servidor ao hospedar (`#preparar-nome`) |
| `788b3b0d` | passo 8: o motivo do banimento |
| `c5f7e847` | passo 9: o guarda dos tetos escritos à mão na `ui/` |
| `95f9945a` | o caso do nome em CONFIGURAÇÕES espera a tela abrir inteira (achado na reversão) |
| este | o relatório e o item 55 da pendência 58 |

Os arquivos que o lote mexeu:
- `crates/seele-core/src/lib.rs`: quatro reexports do `seele-proto`;
- `crates/seele-ffi/src/types.rs` e `lib.rs`: `LimitesDosCampos` e
  `Connection::limites_dos_campos`;
- `apps/seele-app/src/main.rs`: o comando, o instrumento do `Hello` e o
  guarda das chaves. Os ajudantes de teste do módulo do E54 passam a
  `pub(super)`;
- `apps/seele-app/ui/`: `base.js`, `frases.js`, `camada-nomear.js`,
  `tela-sessao.js`, `tela-server.js`, `tela-boot.js`, `camada-moderar.js`,
  `ponte-web.js` e `index.html`;
- `apps/seele-app/tests/web.rs` e `frontend.rs`;
- `tools/carga-da-casca.py` (o duble), `tools/roteiros/tetos-dos-campos.js`
  (novo), `.github/workflows/ci.yml` e `empacotar/publicar.sh`.

**O fio não mudou.** O `seele-proto` só foi lido.

**No `tela-boot.js`, só as linhas 1338 a 1347**, dentro de
`confirmarPreparar`: a conferência do `#preparar-nome` antes do
`criar_servidor`. Nada em `conectar`, em `abrirHospedagem` nem perto de onde a
F8 vai pôr o `depoisDeConectar`.

---

## Os limites, e de onde vêm

**Pela FFI, que é o caminho da casa.** O ADR 0002 não deixa o app ver o
`seele-proto`; o `seele-ffi` vê o `seele-core`, e o `seele-core` reexporta do
`seele-proto`. Já era assim com `limite_da_mensagem` (`MAX_BODY_LEN`) e com a
senha da sala (`MAX_NICKNAME_LEN`).

| campo de `LimitesDosCampos` | constante do `seele-proto` | valor |
|---|---|---|
| `apelido` | `MAX_NICKNAME_LEN` | 32 B |
| `nome_de_canal` | `MAX_CHANNEL_NAME_LEN` | 48 B |
| `lugares_da_sala` | `MAX_VOICE_ROOM_LIMIT` | 250 (o mínimo é 1) |
| `nome_do_servidor` | `MAX_CLIENT_NAME_LEN` | 64 B |
| `motivo_do_banimento` | `MAX_ALERT_TEXT_LEN` | 512 B |

- **No Rust:** `seele_ffi::Connection::limites_dos_campos()`, uma `const fn`
  no molde de `limite_da_mensagem`, e o comando `limites_dos_campos` do
  `main.rs`.
- **Na janela:** `base.js` pede os tetos uma vez (`tetosDosCampos`) e diz a
  frase de um texto que passa do teto (`passaDoTeto(campo, texto)`). O
  `bytesDe` saiu do `tela-sessao.js` para o `base.js`. Sem os tetos (a ponte
  não respondeu), nada é dito, como o `medirOCorpo` faz com a mensagem, e o
  núcleo continua recusando o que não cabe.
- **A frase** é `fraseDoTeto` (`frases.js`): «PASSA DE N BYTES — ENCURTE.» e,
  na segunda linha, quantos bytes o texto tem e por quê («uma letra acentuada
  ocupa dois, e um ideograma, três»).
- **Na ponte do navegador:** as `CONSTANTES_DO_DESKTOP` ganharam
  `limites_dos_campos`, e o guarda `as_constantes_da_ponte_sao_as_do_desktop`
  (`tests/web.rs`) as confere contra a FFI. É a única cópia dos números na
  `ui/`: o navegador não tem Rust a quem perguntar, e o perfil dele grava o
  apelido com a mesma conferência.
- **No duble da carga** (`tools/carga-da-casca.py`): os tetos são lidos do
  `control.rs`, como o `APIS_ACEITAS`.

**Os `maxlength` saíram**, os cinco, como saiu o do compositor da mensagem:
um teto em caracteres ao lado de um em bytes é o defeito que acabou de sair.
Saiu também o `250` de `lugaresDeUmaSalaNova`, em `tela-sessao.js`.

## Os sete caminhos

Cada caminho tem um caso no roteiro `tools/roteiros/tetos-dos-campos.js`,
posto nas duas listas (`ci.yml` e `publicar.sh`). Cada caso escreve um texto
acentuado que cabe na contagem de caracteres do campo antigo e passa na de
bytes, e exige três coisas:
- o comando não sai;
- a frase com o teto aparece onde se escreveu;
- um texto que cabe no teto ainda sai.

Cada caso lê só o que a janela mandou durante ele, e roda mesmo depois de um
vermelho.

| caminho | onde confere | onde diz | o caso usa |
|---|---|---|---|
| apelido na sessão (`escolher_apelido`) | `guardarOApelido` | `#perfil-erro` | 17 `á` (34 B) |
| apelido gravado (`escolher_apelido_local`, o do `Hello`) | `guardarOApelido` | `#perfil-erro` | 17 `á` (34 B) |
| nome de canal ou sala (`#nomear-valor`) | o `submit` do diálogo de nomear | `#nomear-erro` | 25 `á` (50 B), por criar canal e por renomear sala |
| lugares da sala nova | `lugaresParaASalaNova` | (ajuste, ver abaixo) | a primeira sala com 300 lugares |
| nome do servidor em CONFIGURAÇÕES (`#server-nome-valor`) | `renomearServidor` | `#server-servidor-erro` | 33 `á` (66 B) |
| nome do servidor ao hospedar (`#preparar-nome`) | `confirmarPreparar`, antes de subir | `#preparar-erro` | 33 `á` (66 B) |
| motivo do banimento (`#moderar-motivo`) | o BANIR, antes de armar | `#moderar-motivo-erro` (novo) | 171 ideogramas (513 B) |

Notas de cada um:
- **O apelido** é uma conferência só para as duas portas, antes da pergunta
  «estou numa sessão?». A pergunta do nome antes da primeira entrada
  (`perguntarONome`) passa pelo mesmo SALVAR. A caixa fica aberta, com o
  texto, e o foco volta ao campo.
- **O diálogo de nomear** confere todo nome que passa por ele: os quatro
  comandos (`criar_linha`, `criar_voice_room`, `renomear_linha` nos dois
  lugares, `renomear_voice_room`) são de canal ou de sala, com o mesmo teto.
- **O nome em CONFIGURAÇÕES** recusa sem o `atualizarServer` de depois, e com
  o foco de volta no campo. Sem isso, o redesenho de meio segundo poria o nome
  de agora no campo sem foco, e a pessoa perderia o que escreveu justo quando
  a frase manda encurtá-lo.
- **O nome ao hospedar** é conferido antes do `criar_servidor` e do
  `hospedar`. Ele só chega ao `renomear_server` depois da entrada, e sem a
  conferência o servidor subia com o nome recusado calado lá dentro.
- **O motivo do banimento** é conferido antes de armar o ato. O
  `#moderar-erro` mora na confirmação, que um motivo recusado não chega a
  abrir; daí o `#moderar-motivo-erro`, logo abaixo do campo, com
  `role="alert"`. Ideogramas porque o campo contava até 200 caracteres: 171
  cabem nele e passam dos 512 bytes.

**Os lugares da sala nova são ajustados, e não recusados.** É uma escolha, e
fica registrada aqui.
- Ninguém escreve este número: ele é copiado da primeira sala do servidor
  (`desenharCanais`), e um servidor pode estar configurado com uma sala de
  300 lugares, ou de zero.
- Uma recusa deixaria quem administra sem como criar sala nenhuma naquele
  servidor, sem campo onde consertar.
- Então a cópia que não cabe em 1 a `lugares_da_sala` vira o teto, e o ajuste
  vai ao `seele.log` («a sala nova copiaria 300 lugares da primeira sala, e o
  fio aceita de 1 a 250: ela vai com 250»). O roteiro exige essa linha.
- A cópia que cabe continua valendo, e o caso exige isso também.
- Sem sala de onde copiar, vale o teto, que é o que o `250` à mão fazia.

**Os vermelhos, antes de cada conserto:**
- «um apelido de 17 letras e 34 bytes foi gravado nesta máquina, e vai no
  Hello»;
- «um apelido de 34 bytes saiu para o servidor, e o núcleo o recusa calado»;
- «um nome de canal de 25 letras e 50 bytes saiu, e o núcleo o recusa calado»;
- «copiada de uma sala de 300 lugares, a sala nova saiu com 300, e o fio
  aceita de 1 a 250»;
- «um nome de servidor de 33 letras e 66 bytes saiu, e o núcleo o recusa
  calado»;
- «um nome de servidor de 33 letras e 66 bytes subiu o servidor, e o
  renomear_server dele o núcleo recusa calado»;
- «um motivo de 171 ideogramas e 513 bytes saiu, e o núcleo recusa o
  banimento calado».

## Os guardas

- **`nenhum_teto_do_fio_dos_campos_e_escrito_a_mao_na_ui`** (`frontend.rs`),
  com os tetos lidos da FFI. Ele reprova duas coisas:
  - um `maxlength` num dos cinco campos do `index.html`, com qualquer número;
  - um número igual a um teto numa linha de script que trata de tamanho de
    campo: que põe `maxLength`, que conta `bytesDe(`, que pede um teto
    (`passaDoTeto`, `tetosDosCampos`), que fala dos lugares de uma sala, ou
    que nomeia um dos cinco campos.

  As `CONSTANTES_DO_DESKTOP` da ponte ficam fora, porque o `web.rs` as
  confere. Há uma exceção, com o motivo escrito: o `maxLength = 48` do nome de
  um contato da agenda (`camada-sala.js`), que fica nesta máquina e não vai ao
  fio. Uma exceção que sumir também reprova.
- **`nenhuma_tela_le_um_teto_de_campo_que_nao_existe`** (`main.rs`, ao lado
  do guarda das regras do ícone). Um nome de teto que o Rust não serializa
  chega `undefined`, e `passaDoTeto` não conferiria nada, calado. Este guarda
  exige que todo nome lido exista num `LimitesDosCampos` serializado de
  verdade, e que todo teto serializado tenha uma tela que o leia.

## O `Hello`

**A medida.** O instrumento é `o_apelido_grande_demais_no_hello`, em
`main.rs` e em `#[ignore]`. Ele conecta pelo `connect` do app a um servidor
de verdade nesta máquina, na casa temporária do módulo do E54, que é a única
`SEELE_HOME` dos testes deste binário. Para refazer:

```sh
cargo test -p seele-app --bin seele-app -- o_apelido_grande_demais_no_hello --ignored --nocapture
```

Medido em `3b1f57c8`, com HOME falsa:
- **16 `á` (32 B, no teto):** `Ok(geracao 1)` em 16 ms, e o servidor a vê;
- **20 `á` (40 B):** `{"error":"Unreachable", …}` em 5 ms. A trilha é
  `Parada`, depois `Tentando` (127.0.0.1), depois `Desistiu { Unreachable }`
  aos 3 ms. Ninguém fica presente no servidor, e a janela lê «NADA RESPONDEU
  NESSE ENDEREÇO».

**Medido também na janela**, por um roteiro de medida fora do repositório.
Com o apelido de 40 B gravado, a entrada diz «NADA RESPONDEU NESSE
ENDEREÇO». A pessoa encurta o apelido no perfil e tenta de novo, e **a
tentativa seguinte leva os mesmos 40 B**: o `conectar` (`tela-boot.js`)
guarda em `ultimoApelido` o apelido da tentativa anterior, e só fechar o app
o esquece. Então «nenhuma tentativa de novo muda isso» vale mesmo depois de a
pessoa fazer a coisa certa.

**O mínimo, feito:** a janela confere o apelido antes de gravar (passo 3).
Nenhum apelido grande demais novo entra no disco.

**O conserto do que já está gravado não foi feito.** Todos os caminhos pedem
uma variante nova de `ConnectionError` ou mexem no `conectar`, e o pedido era
avisar antes. As opções, pela medida:
1. **A recomendada: o `connect` da FFI confere o apelido antes de abrir a
   conexão.** A falha é toda local e conhecida antes da rede: o apelido está
   na mão antes de resolver o endereço. O caminho de hoje abre o QUIC e o TLS
   no servidor para falhar aqui e, pela leitura (cada candidato de um convite
   manda o próprio `Hello`), numa corrida de candidatos falharia uma vez por
   candidato. Isso pede uma variante nova só na FFI, por exemplo
   `ConnectionError::ApelidoGrandeDemais { limite_bytes }`: nem o
   `ConnectError` do núcleo nem o fio mudam. Pede também a frase em
   `frases.js` e uma linha no `catch` de `conectar` que esqueça o
   `ultimoApelido` nessa recusa, como o `NicknameTaken` já faz.
2. **A falha de codificação do `Hello` ganha nome próprio**, em vez de virar
   `Unreachable`. Pede uma variante no `ConnectError` do núcleo e outra na
   FFI, e a mesma linha no `conectar`. O QUIC e o TLS continuam abrindo à toa.
3. **Sem variante nenhuma:** a janela confere o apelido em `conectar`, antes
   do `connect`, e diz a frase em `#boot-erro`. São umas dez linhas no
   `conectar`, que a F8 vai mexer para criar o `depoisDeConectar`.

As três mexem no `conectar`. Por isso o lote parou aqui. O instrumento já
está pronto para virar o vermelho do conserto escolhido.

## As reversões

Todas pelo script `reverter.py`, no rascunho do lote:
1. cópia do arquivo;
2. o mutante, com o trecho conferido como único;
3. toque no arquivo;
4. a prova;
5. a devolução pela cópia, e toque de novo;
6. `cmp` contra a cópia e contra `git show HEAD:`.

**Todas voltaram idênticas** à cópia e ao `HEAD`, e o `git status` ficou
limpo depois de cada rodada.

**Sem a conferência num campo** (o teto trocado por `null`; nos lugares, o
`limit: lugaresDeUmaSalaNova` de antes), pelo roteiro. Cada reversão reprova
a cena do seu campo, e só ela:
- **R1, o apelido:** reprovam «apelido gravado» e «apelido na sessão». É uma
  conferência só para as duas portas, e as outras cinco cenas passam;
- **R2, o nome de canal:** reprova só «nome de canal ou sala»;
- **R3, os lugares:** reprova só «lugares da sala nova»;
- **R4, o nome em CONFIGURAÇÕES:** reprova só «nome do servidor na
  configuração»;
- **R5, o nome ao hospedar:** reprova só «nome do servidor ao hospedar»;
- **R6, o motivo:** reprova só «motivo do banimento».

**O teto escrito à mão**, pelos guardas:
- **G1**, o `let lugaresDeUmaSalaNova = 250;` de volta: reprova o guarda dos
  tetos, com «tela-sessao.js: let lugaresDeUmaSalaNova = 250;»;
- **G2**, o `maxlength="48"` de volta no `#nomear-valor`: reprova o guarda
  dos tetos, com a etiqueta;
- **G3**, a conta com o número (`bytesDe(escolhido) > 32 ? fraseDoTeto(32, …)`):
  reprova o guarda dos tetos;
- **G4**, `passaDoTeto("apelidos", …)`: reprova
  `nenhuma_tela_le_um_teto_de_campo_que_nao_existe`;
- **G5**, a ponte com `"apelido": 33`: reprova
  `as_constantes_da_ponte_sao_as_do_desktop`.

**O que a reversão achou no próprio roteiro.** Na primeira rodada, R2 e R3
reprovaram também «nome do servidor na configuração» («o campo perdeu o nome
recusado»). Não era o produto: o caso escrevia o nome antes de o
`abrirServer` terminar. O `abrirTela` do fim levava o foco, e o redesenho
trocava o nome. A reversão encurtava o tempo até ali e expunha a corrida. O
caso passou a chamar a função da engrenagem e esperá-la (`95f9945a`). Depois
disso, as seis reversões reprovaram cada uma só a sua cena, e o roteiro
passou cinco vezes seguidas.

## O que rodou

No `95f9945a`:
- **`cargo test -p seele-app`**, todos os alvos, com HOME falsa, pelo script
  `testar.sh`, com o log de cada passo conferido não vazio. `--bin` deu 348
  (mais o instrumento, em `#[ignore]`), `frontend` 296, `web` 12, `celular` 8,
  `empacotamento` 22, `encerramento` 3, `fontes` 5, `manifesto_hibrido` 11,
  `marca` 9, `permissoes` 4 e `tokens` 3. A HOME falsa terminou vazia;
- **`cargo test -p seele-ffi --lib`**, com HOME falsa: 210, mais um ignorado;
- **`cargo fmt --all -- --check`**: limpo;
- **`cargo clippy -p seele-app --all-targets -- -D warnings`** e
  **`cargo clippy -p seele-ffi -p seele-core --lib -- -D warnings`**: limpos;
- **`cargo xtask check-runtime`** e **`cargo xtask check-deps`**: passam.
  `cargo test -p xtask` também, com `todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`;
- **os roteiros**, todos numa chamada, como no CI: os 36 passaram. O
  `tetos-dos-campos.js` passou cinco vezes seguidas, e gasta 1,7 s de tempo
  virtual dos 5 s do orçamento;
- **as bancadas que leem estes campos:** `ajustes-v013.cjs` passa, pelo
  Playwright já instalado em `SEELE-MOD-ESTILO`, sem baixar nada.
  `ponte-web.cjs` passa as 20 provas. Nenhuma bancada foi mexida.

Não rodou nenhuma conformidade, porque o lote não mexe no núcleo além dos
reexports. O microfone não foi aberto.

## O que ficou de fora

- **O conserto do `Hello` para um apelido já gravado grande demais**,
  descrito acima, à espera da decisão sobre a variante.
- **A senha da sala**, como o pedido mandava: a FFI já a confere, e a janela
  mostra a entrada recusada.
- **O nome de um contato da agenda** (`camada-sala.js`, `maxLength = 48`).
  Ele fica no disco desta máquina e não vai ao fio, e é a exceção nomeada no
  guarda.
- **Um achado fora do lote: a recusa de gravar o apelido some do perfil.** Em
  `guardarOApelido` (`camada-nomear.js`), o `catch` escreve a frase em
  `#perfil-erro` e chama `abrirPerfil()`, que a esconde e põe de volta no
  campo o apelido gravado. Medido com `escolher_apelido_local` recusando
  `NaoGravei`: o perfil fica aberto, sem frase, com o apelido de antes no
  lugar do que se escreveu. A conferência deste lote recusa antes desse
  `catch` e não passa por ele. O conserto não foi feito, por ser outro
  defeito. Está no item 55 da pendência 58.
