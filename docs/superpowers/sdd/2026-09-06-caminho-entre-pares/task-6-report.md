# Task 6: O protocolo — a v4 e as quatro mensagens

## Status

Completa. Commit `1edf3b050a290341a34f3467d193e816a41f23ef` na worktree
`scratchpad/wt2`, branch `malha/caminho-entre-pares`.

## O que entrou

Em `crates/seele-proto/src/control.rs`, no fim de cada lista (posição importa
para o `postcard`):

- `ClientMessage::EmprestarSubida { emprestando: bool, impressao: String, locais: Vec<SocketAddr> }`
- `ClientMessage::ParFalhou { screen: ScreenId, motivo: MotivoDeFalhaDePar }`
- `ServerMessage::SirvaTelaPara { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String }`
- `ServerMessage::AssistaTelaPor { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: String }`
- `pub enum MotivoDeFalhaDePar { NaoAlcancou, ImpressaoNaoBate, CaiuNoMeio, ParouDeMandar }`

`crates/seele-proto/src/version.rs`: `PROTOCOL_VERSION` de 3 para 4, com o doc
da constante estendido no mesmo formato das entradas anteriores (2 pelo ADR
0036, 3 pelos verbos de tela), registrando as quatro variantes novas e
lembrando que a v3 já tinha saído publicada (release `v0.10.5-1`, commit
`12a6401a6`) quando esta subida aconteceu.

Também acrescentei `MAX_IMPRESSAO_LEN: usize = 64` (o tamanho fixo de um
SHA-256 em hex) e os braços correspondentes em `impl Validate for
ClientMessage` / `impl Validate for ServerMessage`, porque o `match`
exaustivo dessas duas trait impls — que já existiam neste arquivo antes desta
tarefa — não compilava mais sem cobrir as quatro variantes novas. `ParFalhou`
não valida nada (mesmo raciocínio já usado para `ScreenId` nos braços
vizinhos: quem sabe se ela existe é o servidor); `EmprestarSubida`,
`SirvaTelaPara` e `AssistaTelaPor` bounded a `impressao` no tamanho declarado
pelo próprio campo.

## TDD

Os dois testes do brief foram escritos primeiro
(`as_mensagens_do_caminho_entre_pares_atravessam_o_fio` em `control.rs` e
`a_versao_subiu_para_a_do_caminho_entre_pares` em `version.rs`), rodados e
vistos falhar por erro de compilação (variantes inexistentes,
`PROTOCOL_VERSION` ainda 3) antes de qualquer implementação. Depois da
implementação mínima, `cargo test -p seele-proto` ficou verde.

**Guarda provado por reversão (Step 5):** voltei `PROTOCOL_VERSION` para 3 e
rodei só o teste novo — falhou com `left: 3, right: 4`, exatamente o texto
esperado. Desfeito em seguida (`cargo test -p seele-proto` voltou a 187/187).

## Resumo dos testes

`cargo test --workspace`: verde do início ao fim, nenhuma falha em nenhum
crate (seele-proto 187, seele-server 353, seele-core 265, e o resto do
workspace sem regressão). `cargo fmt --all -- --check` limpo. `cargo clippy
--workspace --all-targets` sem avisos novos. `cargo doc -p seele-proto
--no-deps` emite os mesmos 2 avisos de link quebrado que já existiam antes
desta tarefa (`AlertReason::NicknameTaken`, `Signal`) — conferido revertendo
o diff (`git stash`) e rodando o doc de novo: os dois avisos apareciam
idênticos, nas linhas de antes do deslocamento. Nenhum aviso novo.

## Testes de outros arquivos que toquei, e por quê

### Dentro do próprio `seele-proto` (não são "outro crate", mas quebraram pela mesma razão)

1. **`control::o_vocabulario_e_a_versao::o_ultimo_verbo_de_cada_lista_esta_onde_esta_versao_o_deixou`**
   (`control.rs`). Este teste é o guarda que a Task de 04/09/2026 escreveu
   depois de `WatchScreen`/`UnwatchScreen` terem entrado sem subir a versão:
   ele prende o **ordinal do postcard** do último verbo de cada lista, mais
   `PROTOCOL_VERSION` ao lado. O próprio doc do teste diz: "o ordinal do
   último de cada lista dá a mesma resposta: ele só muda quando alguém
   acrescenta ou remove". Isso significa que apontar para o `UnwatchScreen` e
   o `PersonRenamed` antigos deixaria de servir de guarda assim que alguém
   acrescentasse depois deles — a posição desses dois não muda quando se
   *acrescenta ao fim*, então o teste continuaria passando sem detectar nada
   na próxima vez que a lista crescesse. Troquei os dois nomes fixados para
   os verbos que **agora são** o fim de cada lista (`ParFalhou` e
   `AssistaTelaPor`, ordinais 33 e 35, medidos rodando o teste — não
   supostos), e troquei a asserção final de `PROTOCOL_VERSION == 3` para
   `== 4`, seguindo a própria instrução do teste ("a pergunta não é como faço
   passar, é: um par da versão anterior recebe esta variante nova? Se
   recebe, `PROTOCOL_VERSION` sobe"). Ele afirma "a versão é 3" literalmente
   — não afirma compatibilidade entre versões —, então o conserto é trocar o
   número, mas eu fui além do mínimo e também recoloquei o guarda apontando
   para o alvo certo, porque um guarda que combina com seu próprio comentário
   sem de fato proteger nada é exatamente o padrão de defeito que este
   repositório registra ter custado caro.

2. **`version::tests::a_versao_anterior_continua_dentro_da_janela`**
   (`version.rs`). Este teste **afirma compatibilidade**, não só um número: a
   forma que ele prende é "aceita-se a versão atual e a anterior, recusa-se a
   seguinte" — e o próprio comentário dentro dele já registra que essa forma
   sobreviveu a uma subida anterior (`v1→v2`) trocando só os números
   concretos. Não apaguei a afirmação: atualizei os quatro números
   (`PROTOCOL_VERSION` 3→4, `oldest_supported_version()` 2→3,
   `negotiate(3)`/`negotiate(4)` passam a ser aceitos, `negotiate(2)` passa a
   ser o "anterior à janela" que devia continuar recusado antes e agora está
   dentro, `negotiate(5)` é o "futuro" recusado) e reescrevi o comentário
   para registrar por que desta vez importa mais do que da vez passada: a v3
   **já tinha saído** publicada (release `v0.10.5-1`), então existe gente de
   verdade rodando exatamente essa versão, ao contrário da v1 na subida
   anterior, que não tinha ninguém na 0.9.x. A garantia comportamental
   continua sendo testada, só a janela concreta deslizou — a mesma decisão
   que a subida 2→3 já tinha tomado.

### Em outros crates (não previstos no brief, achados ao rodar `cargo test --workspace`)

3. **`crates/seele-server/src/session.rs`** — o loop de despacho de
   `ClientMessage` faz `match message { ... }` exaustivo, e não compilava
   mais sem cobrir `EmprestarSubida`/`ParFalhou`. Não é um teste que afirma
   um número: é o `match` exaustivo do dispatcher em si recusando compilar.
   O plano mestre (`docs/superpowers/plans/2026-09-06-caminho-entre-pares.md`)
   deixa explícito que ligar esses dois verbos a `pares.rs` é a *próxima*
   tarefa (Task 7 consome `EmprestarSubida` e produz o que Task 8 usa), e o
   `progress.md` desta mesma leva já registra essa fronteira ("quem liga é a
   Task 8, e o `impressao` do `SirvaTelaPara` só existe a partir da Task 6" —
   ou seja, esta tarefa só cria a mensagem, não o comportamento). Acrescentei
   um braço mínimo, não silencioso: um `tracing::debug!` identificando a
   pessoa e dizendo que o quadro chegou antes de o despacho existir, em vez
   de um `{}` vazio ou um `todo!()` que derrubaria a sessão se algo batesse
   nesse caminho antes da Task 7. Nenhuma lógica de negócio foi escrita aqui.

4. **`crates/seele-core/src/state.rs`** — o `Room::apply(&ServerMessage)` do
   lado do cliente também é `match` exaustivo, e quebrou pela mesma razão com
   `SirvaTelaPara`/`AssistaTelaPor`. Mesmo tratamento: braço mínimo com
   `tracing::debug!`, sem lógica de negócio, documentado como andaime até a
   tarefa que vai ligar isso ao `crate::caminho`.

Nenhum outro crate do workspace referencia `PROTOCOL_VERSION` por número
literal (conferido com `grep` em todo o workspace antes de mexer); todos os
outros usos são pela constante (`seele_proto::PROTOCOL_VERSION`), que
acompanha a subida automaticamente.

## Preocupações

- Os braços de `session.rs` e `state.rs` são andaime deliberado, fora do
  escopo de arquivos que o brief da Task 6 listava (só `control.rs` e
  `version.rs`). Sem eles o workspace não compilava — o brief não podia
  prever essa colateral porque ela nasce da forma como Rust exige `match`
  exaustivo, não de uma suposição sobre versão publicada. Documentei a razão
  em comentário em cada um dos dois pontos, citando que a tarefa seguinte
  deste plano é quem liga o comportamento de verdade.
- Não escrevi nenhuma validação de formato (hex, correspondência com
  certificado) para `impressao` além do tamanho — isso é decisão de desenho
  de uma tarefa futura (o aperto de mão de verdade é quem descobre uma
  impressão errada, via `MotivoDeFalhaDePar::ImpressaoNaoBate`), e inventar
  mais aqui seria além do que a Task 6 pediu.
- `cargo doc -p seele-proto --no-deps` mostra 2 avisos de link quebrado que
  já existiam antes desta tarefa (confirmado por `git stash` + rebuild); não
  são meus e não os toquei, porque não fazem parte do escopo desta tarefa.

---

# Três consertos aplicados após Task 6

## Status

Completa. Commit `dfd6584` na worktree `scratchpad/wt2`, branch
`malha/caminho-entre-pares`.

## O que foi consertado

**1. Comentário em `crates/seele-core/src/state.rs`, linhas 1207-1211**

O comentário original citava `crate::caminho` (o módulo de banda de subida) como
dono da integração do andaime. Reescrito para citar Task 8 («O cliente pede ao
par, e cai para o servidor quando falha») como a tarefa que faz isso, e para
deixar claro que é andaime com prazo. `crate::caminho` não tem relação com o
caminho entre pares e não deve ser mencionado aqui.

**2. Elevar `tracing::debug!` para `tracing::warn!` em dois pontos**

- `crates/seele-core/src/state.rs:1213` — mensagens `SirvaTelaPara` e
  `AssistaTelaPor` descartadas
- `crates/seele-server/src/session.rs:2273` — mensagens `EmprestarSubida` e
  `ParFalhou` descartadas

Razão: `debug!` vem desligado em produção. Uma mensagem que chega e é
descartada sem resposta nenhuma é «algo que a pessoa pediu não aconteceu»,
e o nível que este repositório reserva para isso é `warn` (confirmado em
`session.rs:1488` usando `warn!` para coisa menos grave). Enquanto o andaime
existir, ele tem de ser barulhento.

**3. Doc em `crates/seele-proto/src/control.rs:230`**

Linha original: «Length of uma impressão digital em hex minúsculo.»

Reescrita em português: «Tamanho de uma impressão digital em hex minúsculo.»

Itens novos deste projeto são em português; o comentário misturava inglês e
português.

## Testes e verificações

```bash
$ cargo test --workspace
   Doc-tests seele_proto
running 1 test
test crates/seele-proto/src/version.rs - version::negotiate (line 124) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s

$ cargo clippy --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.51s

$ cargo fmt --all -- --check
(sem output — formatação correta)
```

Resultado: todos os testes passaram, clippy sem avisos novos, fmt aprovado.
