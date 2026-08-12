# Conferência de impressão digital nas duas cascas

**Data:** 2026-08-11
**Estado:** aprovado, aguardando plano

Primeira das duas travas que precisam cair antes de alcançar um Dogma pela
internet. A outra — limitação de taxa (pendência #5) — é subsistema separado,
com spec e plano próprios.

## O problema

O `seele://` carrega `fp=`, e `crates/seele-proto/src/uri.rs` chama isso de "o
motivo principal de isto existir": o ADR 0006 inventou o link para transformar
primeiro contato **cego** em primeiro contato **verificado**. Hoje isso não
acontece por inteiro em casca nenhuma.

**O app não confere nada.** Ele aceita um `seele://` colado, lê a impressão
digital e a descarta — `ConnectConfig` não tem campo por onde ela passe. Pior, o
app **nunca anuncia primeiro contato**: o `plug` diz `PRIMEIRO CONTATO — CHAVE
FIXADA <fp>` (`crates/seele-tui/src/main.rs:529`), e o app fixa chaves em
silêncio. `crates/seele-core/src/tofu.rs:38-40` é explícito sobre isso — *"the
shell should say what it just trusted"*.

**O `plug` confere em dois dos três ramos.** Em `main.rs:504-516`:

```rust
let oferecida = match client.pin_decision() {
    PinDecision::FirstContact { fingerprint } => fingerprint.clone(),
    PinDecision::Matches => esperada.clone(),   // devolve o que veio do link
    PinDecision::Changed { offered, .. } => offered.clone(),
};
if !oferecida.eq_ignore_ascii_case(esperada) { /* recusa */ }
```

No ramo `Matches`, `oferecida` **é** `esperada`, então o teste seguinte é
`esperada == esperada` — verdadeiro sempre. A causa é que
`PinDecision::Matches` não carrega impressão nenhuma (`tofu.rs:46`): não havia o
que comparar, e o código pegou o único valor à mão.

Consequência: um Dogma já fixado com a impressão A, e um link prometendo B,
conecta calado. O comentário logo acima afirma ser "o que transforma o primeiro
contato de cego em verificado", e naquele ramo não transforma.

Não é catastrófico — se o host já está fixado, o TOFU garante que é o mesmo
servidor de ontem. Mas o link discordar do pin é informação real: significa que
o convite é de outro Dogma, está velho, ou foi forjado. Hoje ela é engolida.

## O que fica verdade no fim

Uma regra, escrita uma vez, valendo nas duas cascas. O core decide **o que
aconteceu**; a casca decide **como dizer**. É a mesma régua que
`seele_core::search` já segue.

## A política

Os dois ramos significam coisas diferentes e são tratados como tal:

| Situação | O que se faz | Por quê |
|---|---|---|
| Sem pin, convite bate | conecta, **verificado** | é o que o ADR 0006 existe para produzir |
| Sem pin, sem convite | conecta, **diz o que fixou** | TOFU cego, e `specs/08` quer isso declarado |
| Sem pin, convite discorda | **recusa** | sem pin anterior, o convite era a única prova, e falhou |
| Com pin, convite bate ou ausente | conecta, calado | nada novo a dizer |
| Com pin, convite discorda | conecta, **avisa** | o TOFU já provou que é o mesmo servidor; o defeito está no link |
| Pin diferente (`Changed`) | recusa | já é assim, e o convite não muda nada |

A última linha é do TOFU e continua sendo erro de conexão, não veredito.

**Recusar no já-fixado seria a troca errada.** Um link velho ou digitado errado
trancaria a pessoa fora de um Dogma que ela usa, e o caminho de volta — apagar o
campo de convite — não é adivinhável.

### Recusar tem de desfixar

`PinDecision::FirstContact` significa que a chave **já foi gravada** —
`tofu.rs:36-37`: *"The key has now been recorded"*. Uma recusa que deixasse o
pin no disco seria teatro: a conexão seguinte, sem link, veria `Known` e entraria
sem hesitar no servidor que acabou de ser rejeitado.

Então `InviteRefused` **apaga o pin que acabou de ser escrito**. Quem tinha um
link velho e o remove volta a um primeiro contato limpo, que é o estado correto:
nunca houve decisão de confiar naquele servidor.

O `plug` tem este buraco hoje, e ele sai junto — é a mesma correção, no mesmo
lugar.

## 1 · `seele-core`

`PinDecision::Matches` passa a carregar a impressão que continua valendo. Sem
isso não há o que comparar, e foi por isso que o `plug` acabou comparando um
valor consigo mesmo.

`Destino` (em `enlace.rs`, arquivo português por decisão do ADR 0023) ganha
`impressao_esperada: Option<String>`.

O veredito nasce em `tofu.rs`, que é arquivo inglês:

```rust
/// O que a conferência concluiu, já decidido — a casca só desenha.
pub enum Verdict {
    /// Nada estava fixado e não havia convite. Fixado agora, às cegas.
    FirstContact { fingerprint: String },
    /// Nada estava fixado, e o convite confirmou.
    FirstContactVerified { fingerprint: String },
    /// O pin bate e nada o contradiz.
    Known,
    /// Primeiro contato, e o convite discorda. A conexão foi recusada.
    InviteRefused { expected: String, offered: String },
    /// O pin é o de sempre, mas o convite promete outra coisa.
    /// A conexão seguiu; o defeito está no link.
    InviteDisagrees { expected: String, offered: String },
}
```

Cinco variantes porque são cinco coisas distintas a dizer. O par
`FirstContact` / `FirstContactVerified` é o que dá sentido ao trabalho: hoje as
duas situações são indistinguíveis, e a segunda é a única que o link foi
inventado para criar.

A comparação usa `eq_ignore_ascii_case`, como o `plug` já fazia, e `uri.rs` já
normaliza a impressão do link para minúsculas — com teste.

## 2 · `seele-ffi`

`ConnectConfig` ganha `expected_fingerprint: Option<String>`.

**O veredito volta do `connect`, não por evento.** Isto não é preferência:
`Trust::FirstContact` já existe e é emitido de dentro do `Plug::connect`
**antes** de a casca assinar os eventos (`crates/seele-ffi/src/lib.rs:271`),
então o app não tem como recebê-lo. Devolver na volta da chamada contorna a
ordem inteira em vez de tentar consertá-la.

`Trust` passa a espelhar o `Verdict` do core. Hoje ele tem duas variantes onde o
core terá cinco, e traduzir cinco em duas jogaria fora exatamente a informação
nova.

## 3 · As cascas

**`plug`.** As doze linhas de `main.rs:504-516` somem; `args.expected_fingerprint`
passa a entrar no `Destino`. Os cinco vereditos são desenhados com o que já
existe: `Alert` para os três informativos, `Screen::Lost` para a recusa. O
layout de duas impressões lado a lado continua, e agora sai alinhado — o
`render_lost` passou a preservar quebras de linha no commit `c70bf08`.

**app.** O `connect` passa a impressão guardada em `Session.convite`:

| Veredito | O que aparece |
|---|---|
| `FirstContact` | aviso laranja com a impressão: fixamos isto, confira por outro canal |
| `FirstContactVerified` | confirmação discreta — o link bateu |
| `Known` | nada |
| `InviteDisagrees` | aviso laranja: o link não corresponde a este Dogma; você está no que já conhecia |
| `InviteRefused` | a conexão falha, com as duas impressões em `#boot-erro` |

O `#boot-aviso` laranja com `role="status"` já existe desde a tela de entrada —
é reúso. O tratamento vermelho continua reservado ao que impede de entrar, como
`specs/08-seguranca.md` quer e como `tokens.css` anota no próprio token.

### Uma decisão anterior se inverte

O trabalho da tela de entrada manteve a impressão digital **fora** da ponte de
propósito: só `conferencia_pendente: bool` atravessava. A razão era boa — o app
não sabia conferir, e mandar o valor convidaria a comparação a ser reescrita em
JavaScript.

Agora o veredito chega pronto do Rust, e a string atravessa para ser **lida por
uma pessoa**. `specs/06-clientes-gui.md:19` proíbe lógica de protocolo no
frontend; exibir um texto que o usuário deve conferir por outro canal não é
lógica. A comparação continua inteiramente em Rust, e é isso que mantém a regra
de pé.

## 4 · Erros e bordas

| Situação | Comportamento |
|---|---|
| convite sem `fp=` | nada a conferir; `expected_fingerprint` é `None` e nada muda |
| caixa alta no link | `uri.rs` normaliza, e a comparação ignora caixa |
| `PinDecision::Changed` | o TOFU recusa antes, com ou sem convite |
| reconectar sem recolar | `Session.convite` passa a ser limpo no `disconnect`/`ejetar` |
| `plug --server` sem link | `impressao_esperada` é `None`; caminho intocado |

A limpeza do `Session.convite` era item estacionado do trabalho anterior, inerte
enquanto o app não conferia nada. Deixou de ser inerte.

## 5 · Testes

**`tofu.rs`** — a matriz inteira: (sem pin / com pin) × (sem convite / convite
bate / convite discorda). Seis células, cinco vereditos.

**O buraco do `Matches`, nomeado** — fixa um servidor, conecta com um convite
que discorda, e afirma `InviteDisagrees` **e que a conexão fica de pé**. Este
teste reprova contra o código de hoje; é para isso que ele existe.

**A recusa desfixa** — primeiro contato com convite que discorda, afirma a
recusa, e então afirma que **não sobrou pin**: reconectar sem link tem de dar
`FirstContact` de novo, e não `Known`. Sem esta segunda metade o teste passaria
com o pin intacto, e a recusa seria decorativa.

**Conformidade** — contra um Dogma de verdade: impressão certa no link →
`FirstContactVerified`; errada → conexão recusada, com as duas no motivo.

**`frontend.rs`** — cada veredito tem mensagem própria, e a comparação não
aparece em JavaScript. Os guardas existentes já cobrem comandos e ids.

## O que este trabalho não faz

- **Não implementa limitação de taxa** (pendência #5). É a outra trava, e tem
  spec própria: onde o balde vive, qual a chave, e o que acontece ao estourar
  são decisões que não cabem aqui.
- **Não implementa nenhum degrau do ADR 0022.** Alcançar um Dogma pela internet
  continua sendo endereço direto, VPS ou porta encaminhada à mão.
- **Não registra o esquema `seele://` no sistema operacional** (pendência #10).
  Quando isso acontecer, o cliente terá de perguntar antes de conectar — e este
  trabalho é justamente o que torna essa pergunta capaz de dizer algo útil.
