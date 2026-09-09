# Onda única de consertos da revisão final — caminho entre pares

Você está numa branch (`malha/caminho-entre-pares`) que acabou de passar por uma
revisão da branch inteira. O relatório completo está em
`.superpowers/sdd/2026-09-06-caminho-entre-pares/revisao-final.md` — **leia-o
primeiro e inteiro**. Ele cita arquivo e linha em cada achado, e a citação é
confiável: o revisor conferiu no fonte.

A spec que manda é `docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md`.

Você conserta **doze itens**, listados abaixo na ordem em que devem ser feitos.
Nenhum outro. Se você achar um décimo terceiro problema, **não conserte** —
escreva no relatório.

## Regras da casa (não negociáveis)

- **TDD.** Para todo item que muda comportamento: escreve o teste, **roda e vê
  falhar pelo motivo certo**, depois conserta, depois vê passar. Um teste que
  passou de primeira não prova nada.
- **Prova por reversão.** Para cada guarda novo que você escrever: desfaz o
  conserto, roda o teste, confirma que **só ele** falha, restaura. Registra o
  resultado no relatório. «Existir não é funcionar» é a regra desta casa.
- **Um commit por item**, com a mensagem dizendo o que o sistema passa a fazer.
- Ao final: `cargo test --workspace`, `cargo clippy --workspace --all-targets`,
  `cargo fmt --check` e `cargo xtask check-deps` — todos limpos. Cole os números.
- **Vocabulário**: servidor, sala de voz, pessoa, PERSISTENCE. Nunca Dogma,
  Cage, Pilot, Linha, CASPER. Comentários e docs em português.
- **Motivo de erro é sempre variante enumerada**, nunca `String` (ADR 0012).
- **Não despache subagentes.** A revisão vem de fora, depois do seu relatório.

---

## 1 · I5 — o contador de cópias para de contar em silêncio

`crates/seele-conformance/tests/tela_por_um_par.rs:417`

`while let Ok(evento) = eventos.recv().await` sai do laço em
`broadcast::error::RecvError::Lagged`, e `agora()` congela. Se congelar em `1`,
a asserção central da branch (`copias.agora() == 1`, trinta vezes) passa mesmo
com o servidor voltando a subir a cópia. É o mesmo falso-verde que fundou o
Critical da Task 10, por outra porta.

Conserta: `Err(Lagged(_)) => continue`, `Err(Closed) => break`. Faz primeiro,
porque todos os testes que você escrever depois dependem deste contador.

## 2 · C2 — vazamento de conteúdo entre salas de voz

`crates/seele-server/src/pares.rs:186-198`, `crates/seele-server/src/session.rs:3041-3046`

`Server.pares` é global ao daemon e `escolher` não filtra por sala. Alguém que
empresta na sala B pode ser apontado para servir a tela da sala A — e o cliente
dele repassa o que **ele** está recebendo, que é a tela da sala B. Quem assiste
na sala A recebe, decodifica e o `seele-ffi` entrega sem filtro nenhum.

Isto contradiz por escrito o §5 da spec.

Conserta: `escolher` passa a exigir que o par candidato esteja **na mesma sala
de voz** que a tela sendo servida. O servidor já sabe quem está em qual sala —
procure por `Presentes` / `Occupancy` em `crates/seele-server/src/server.rs` e
use o que já existe em vez de duplicar o estado dentro de `Pares`. Se você
precisar guardar a sala em `QuemDeclarou`, lembre que pessoas trocam de sala:
um campo que só é escrito na declaração fica velho. Prefira consultar a fonte
viva.

Teste: dois pares em salas diferentes, um deles emprestando; o de outra sala
**não** pode ser escolhido. Prova por reversão obrigatória.

## 3 · C3 — fim limpo do repasse não avisa ninguém

`crates/seele-core/src/enlace.rs:3438` (`Ok(None) => break`), `:3264-3272`, `:3228-3234`

`escoar_tela_alheia` só manda `ParFalhou` em **erro**. Um fluxo de par que
termina limpo emite `TelaFechou` e cala — e quem assiste já saiu do cano do
servidor por `TelaParouDeAssistir`. Resultado: tela em branco permanente.

Três caminhos chegam lá com a transmissão ainda no ar: contrapressão
(`PEDACOS_A_ESPERA_DO_PAR` cheio zera o destino), quem empresta reconectando ao
servidor, e quem empresta saindo da sala.

Conserta: no `Ok(None)`, **sempre** mandar `ParFalhou` com
`MotivoDeFalhaDePar::ParouDeMandar` antes de decidir qualquer outra coisa. Quem
assiste não tem como distinguir «a tela acabou» de «o par calou» — então
reporta, e deixa o **servidor** adjudicar: ele sabe se a transmissão ainda
existe.

**Antes de escrever o conserto, confira o outro lado**: `session.rs:2418-2436`
é o braço que trata `ParFalhou`. Ele religa o cano de quem assiste. Verifique o
que ele faz quando a tela citada **não existe mais** (o caso «a transmissão
acabou de verdade»). Se ele reclamar ou entrar em estado ruim, conserte lá
também — sem inventar variante nova: use o que já existe.

Conserta também a doc de `PEDACOS_A_ESPERA_DO_PAR` (`enlace.rs:3169-3182`), que
hoje afirma com todas as letras uma recuperação que não existe: «Quem assiste
nota a falta de imagem e cai para o servidor pelo caminho de sempre.» Depois do
conserto ela passa a ser verdade — reescreva dizendo **por qual mecanismo**.

Teste: o repasse termina limpo com a transmissão viva, e quem assiste volta a
receber imagem do servidor. Prova por reversão obrigatória.

O teste existente `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`
cobre só o `drop` bruto, e o comentário dele (`:735-739`) descarta o caso limpo
como «o caso fácil». Esse comentário está errado — corrija-o.

## 4 · C1 (proporcional) — o repasse recusa a segunda tela em vez de fundi-la

`crates/seele-core/src/enlace.rs:3211-3217` (struct), `:2024`, `:2118-2125`, `:3221`, `:3264`

Há **um** `RepasseDeTela` por `Motor`, sem identidade de tela, e
`escoar_tela_alheia` é gerado uma vez **por fluxo**. Com duas telas no ar na
mesma sala — que é literalmente o cenário do §0 da spec — a segunda `abriu()`
sobrescreve a primeira, os quadros das duas entram intercalados no mesmo fluxo
do par, e o primeiro `fechou()` mata o repasse da outra. Quem recebe rotula
tudo com o `screen` do fluxo e vê duas telas fundidas numa só, **sem erro em
lugar nenhum**.

O conserto completo é per-tela no fio do par — mudança de protocolo, e fundação
do subprojeto B. **Não é o que você faz.**

O que você faz é o guarda honesto: `RepasseDeTela` passa a guardar de qual tela
é o repasse em curso (dono + `ScreenId`), e:

- `abriu()` de uma segunda tela **não assume** — recusa, e deixa um `warn!`
  dizendo que só uma tela por vez é repassada hoje;
- `pedaco()` e `fechou()` só agem quando a identidade bate, para o `fechou()`
  de uma não derrubar o repasse da outra.

Quem assiste a tela recusada continua recebendo do servidor, que é o caminho de
sempre — nenhuma imagem se perde, e nada se funde em silêncio.

Deixa um comentário no struct dizendo, em uma frase, que a versão per-tela é do
subprojeto B e por quê.

Teste: duas telas na mesma sala, a segunda não é repassada e a primeira
sobrevive ao fim da segunda. Prova por reversão obrigatória.

## 5 · I1 — trava de versão nas duas mensagens v4 do servidor

`crates/seele-server/src/session.rs:2667-2671`

O `match entende` cobre `UplinkLoss` e nada mais; `SirvaTelaPara` e
`AssistaTelaPor` caem no `_ => true`. Um cliente v3 que as recebesse teria o
fluxo de controle deslocado para sempre.

Hoje ele não as recebe, mas **por acidente** — um invariante de outra tarefa. A
promessa da spec precisa de uma linha que a afirme: dois braços `>= 4`.

Enquanto está aí: `session.protocol_version >= 2` no mesmo `match` já é vácuo,
porque `oldest_supported_version()` é 3. Conserte ou remova, com um comentário
dizendo o que aconteceu.

## 6 · I2 — cada repasse bem-sucedido queima um par para sempre

`crates/seele-server/src/pares.rs:232` (`desapontou`), único chamador em `session.rs:2411`

`desapontou` só é chamada por `ParFalhou`. Quando o repasse termina **bem** —
`UnwatchScreen`, quem compartilha para, quem assiste sai da sala — a nomeação
fica, e `ja_servindo()` conta aquele par como ocupado pelo resto da sessão do
daemon. Do lado do cliente `atendendo_pares` já foi devolvido: os dois lados
discordam em silêncio, e a malha degrada para a estrela sem um rastro.

Conserta: todo caminho que encerra o repasse chama `desapontou`. Procure os
caminhos de saída (fim de `WatchScreen`, fim da transmissão, saída da sala) em
`session.rs` e `voice_room.rs`.

Teste: depois de um repasse encerrado normalmente, o mesmo par pode ser
escolhido de novo. Prova por reversão obrigatória.

## 7 · I3 — quem empresta desiste de atender quando a própria discagem falha

`crates/seele-core/src/enlace.rs:3081-3084`

```rust
let resultado = tokio::select! {
    atendido = atende => atendido,
    discado = disca => discado.ok(),
};
```

Quem assiste **nunca** chama `atender` — só disca. Então a discagem de quem
empresta existe apenas como o furo de NAT desta ponta; ela não pode fechar. Mas
o `select!` trata o fim dela como resposta: se `ligar` devolver `Err` cedo
(família de endereço incompatível, `connect_with` recusando na hora, todos os
candidatos falhando rápido), `atender` é **cancelado** e quem empresta desiste
de servir alguém que estava chegando.

Conserta: o braço da discagem não decide nada. Espere `atender`; a discagem
segue viva só pelo efeito de abrir o mapeamento. Mantenha o prazo global.

O §3.3 da spec descreve «os dois discam», implicando que qualquer um pode
aceitar — o código não faz isso. **Corrija a spec** para descrever o desenho
real (um disca, o outro atende, e a discagem do que atende é só o furo),
marcando a data da emenda como as outras emendas do arquivo fazem.

Teste: a discagem falha imediatamente e `atender` ainda completa. Prova por
reversão obrigatória.

## 8 · M1 — o `# Errors` de `ligar` mente

`crates/seele-core/src/par.rs:563-567`. Documenta `ImpressaoNaoBate` e
`NaoAlcancou`; a função também devolve `RecusadoDepoisDeLigar`,
`ConfirmacaoNaoChegouATempo` e `Escuta`. Numa casa em que motivo enumerado é
contrato de casca, uma lista incompleta é uma promessa quebrada.

## 9 · M2 — a impressão digital confere só o teto

`crates/seele-proto/src/control.rs:2189`. `MAX_IMPRESSAO_LEN` aceita `impressao`
vazia. A spec diz «exatamente 64 caracteres»; a validação diz `<= 64`. Um
cliente que declare lixo é escolhido, ocupa vaga em `ja_servindo` e custa
segundos de tela parada a cada `WatchScreen`.

Conserta para exigir exatamente 64, e que sejam dígitos hexadecimais. Teste com
`""`, com 63, com 65 e com 64 caracteres não-hex.

## 10 · Ressalva de `stable_id` no doc de `QuemDeclarou::id_da_conexao`

`crates/seele-server/src/pares.rs`. O `id_da_conexao` vem de `stable_id()` do
`quinn`, que é um endereço de alocação. Ele passou de decoração a **carregar a
correção** de uma rodada de conserto: se a suposição de estabilidade cair,
`saiu` apaga a declaração viva de outra conexão. A ressalva tem de estar escrita
onde a invariante mora — uma frase no doc do campo, dizendo o que quebra se a
suposição falhar.

## 11 · `locais_a_publicar(true, ..)` não tem caminho de produção

`crates/seele-core/src/enlace.rs`. `locais_a_publicar` e `locais_de_pares` estão
inertes: nada em `apps/` nem no `seele-ffi` liga o empréstimo. O documento
`docs/teste-duas-maquinas.md` já diz isso; o código não. Duas linhas de doc em
cada, para a próxima pessoa não concluir que o caminho de LAN foi exercitado.

## 12 · M5 — a spec se contradiz sobre o tipo da impressão

`docs/superpowers/specs/2026-09-05-caminho-entre-pares-design.md`, §4. As
assinaturas dizem `impressao: [u8; 32]`; o parágrafo seguinte e o código dizem
`String`. O código está certo. Corrija a spec, marcando a emenda.

---

## Relatório

Escreva em `.superpowers/sdd/2026-09-06-caminho-entre-pares/onda-final-report.md`:

- item a item: o que mudou, em qual commit, e **o resultado da prova por
  reversão** (qual teste falhou, com que mensagem);
- para o item 3: o que você encontrou em `session.rs:2418-2436` quando a tela
  não existe mais, e o que fez a respeito;
- para o item 2: como você descobriu a sala de cada pessoa, e por que essa
  fonte não fica velha;
- qualquer coisa que você achou e **não** consertou;
- os quatro comandos finais com os números.

Devolva no chat só: status, a lista de commits, uma linha de testes, e as
preocupações. O corpo vai no arquivo.
