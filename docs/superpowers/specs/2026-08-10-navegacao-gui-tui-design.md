# Navegação completa na TUI e na GUI

**Data:** 2026-08-10
**Estado:** aprovado, aguardando plano

## O problema

As duas cascas navegam pela metade, e cada uma pela metade que a outra tem.

O `plug` tem a tela de seleção, os quatro modos e o roster; não tem como sair de
um Dogma sem matar o processo, `h`/`l` não fazem nada, e o modo Busca é uma
fachada — ele entra no modo, a barra escreve `BUSCA`, o texto é acumulado em
`App::input` e descartado sem ninguém olhar. O comentário em `app.rs:376` afirma
que a busca filtra a lista de mensagens; não existe filtragem nenhuma em
`ui.rs`. É a pior categoria de lacuna, porque a interface promete em voz alta o
que não faz.

O app tem EJETAR e volta à tela de entrada; não tem lista de Dogmas visitados,
não aceita um `seele://` colado, não tem busca, e é operável só a mouse. Pior:
ele **nunca toca em `seele_core::conhecidos`** — nem lê nem grava. Acrescentar a
lista sem gravar depois de conectar entregaria uma seção permanentemente vazia.

## O que fica verdade no fim

**Paridade de composição, não de teclas.** Cada casca fica completa nos próprios
termos. A TUI cumpre o que `specs/05-cliente-tui.md` promete; a GUI ganha a tela
de entrada que falta e teclado só onde o mouse é lento. Não se persegue mapear
tecla por tecla no app — `specs/06-clientes-gui.md` pede que quem usa a TUI
saiba *onde tudo está* ao abrir o app, e isso é sobre composição.

## Onde a lógica mora

O `view.rs` já escreveu a régua que decide isto:

> *"If something in this file would be identical in the app, it is in the wrong
> crate."*

Que mensagens casam com um termo, como se dobra caixa e acento, e em que ordem
`n` e `N` andam — tudo isso seria idêntico nas duas cascas. Desce para o core.

O que fica na casca: como o casamento é pintado, como a rolagem chega até ele, e
que tecla dispara o quê.

`conhecidos` e o parsing de `seele://` já moram no lugar certo —
`seele_core::conhecidos` e `seele_core::uri`, este reexportando
`seele_proto::uri` em `lib.rs:66`. O ADR 0002 continua de pé nas duas cascas: a
GUI os alcança por comandos Tauri, e nenhuma lógica de protocolo entra em
JavaScript.

## 1 · `seele_core::search`

Módulo novo, puro: sem I/O, sem terminal, sem webview. O nome e a API saem em
inglês porque o ADR 0023 manda: a prosa que explica é português, o código que
ela explica não.

```rust
/// Onde um termo casou.
pub struct Match {
    /// Índice na lista de mensagens.
    pub message: usize,
    /// Intervalo em caracteres dentro do corpo.
    pub start: usize,
    pub end: usize,
}

pub struct Search { /* termo, casamentos, cursor */ }

impl Search {
    /// Os corpos na ordem em que a casca os desenha.
    pub fn new<S: AsRef<str>>(bodies: impl IntoIterator<Item = S>, term: &str) -> Self;
    pub fn next_match(&mut self)     -> Option<Match>;  // n
    pub fn previous_match(&mut self) -> Option<Match>;  // N
    pub fn current(&self)            -> Option<Match>;
    /// (1, 3) para desenhar "[1/3]". (0, 0) quando não casou nada.
    pub fn position(&self)           -> (usize, usize);
}

/// Colapsa espaço repetido, do jeito que a TUI desenha.
pub fn normalize(text: &str) -> String;
/// Todo lugar onde `term` ocorre em `text`, para acender um trecho já quebrado.
pub fn occurrences(text: &str, term: &str) -> Vec<(usize, usize)>;
```

**Entra por corpos, não por mensagens, e isso é decisão e não conveniência.**
`seele_ffi::types::Message` e `seele_core::state::Message` são structs
diferentes, e a TUI nem desenha nenhuma das duas: ela mostra `App::messages`,
que são `ChatLine` já projetadas com o `(editada)` colado no fim. Uma assinatura
presa a um tipo de mensagem serviria a uma casca e excluiria a outra — e a
busca não precisa saber o que é uma mensagem para achar um texto. De quebra, os
testes ficam `["olá", "sync caiu"]` em vez de fabricar mensagens inteiras.

**E cada casca entrega o texto que ela mesma desenha.** `seele-tui::ui::wrap`
quebra com `split_whitespace`, que colapsa espaço repetido, então a TUI passa os
corpos por `search::normalize` antes — sem isso, um casamento depois de um
espaço duplo apontaria para o lugar errado. O app não normaliza: o corpo é
`white-space: pre-wrap` e a janela mostra espaço duplo e quebra de linha como
eles chegaram. `normalize` é público e opcional pela mesma razão: quem desenha o
texto cru busca no texto cru.

Cada casca busca na **sua** lista, então o `Match::message` indexa o que aquela
casca desenha. Na TUI é `App::messages`; no app é `Snapshot::messages`. Buscar no
que está na tela é o que faz a busca casar com o que se vê — inclusive o
`(editada)`.

O `Match` carrega o intervalo, e não só o índice da mensagem, porque as duas
cascas precisam acender o trecho certo — e com dobramento de acento o frontend
não teria como recalcular sozinho onde o casamento começou.

**O cursor dá a volta nas duas pontas.** Num histórico, chegar ao fim e parar é
pior que voltar ao começo: quem procura trata a última ocorrência e a primeira
como vizinhas.

Três decisões, com o motivo:

- **Casa no corpo da mensagem, não no autor.** `/rei` devolvendo tudo que a rei
  já disse é um filtro disfarçado, e a escolha foi salto, não filtro. Quem
  procura uma pessoa olha o roster, que é o painel ao lado.
- **Dobra caixa e acento**, com tabela escrita à mão para os acentos do
  português (`áàâãéêíóôõúüç` e maiúsculas). Não entra
  `unicode-normalization` no core por doze caracteres. **O limite é real e vai
  escrito no doc do módulo:** acento fora do português não dobra. A tabela é
  1:1 por caractere, que é o que mantém os intervalos alinhados.
- **Reconstrói inteiro a cada tecla**, como `view::project` já faz. O histórico
  é uma página; um índice incremental que sai de passo com a lista é o defeito
  que só aparece depois de uma hora de uso.

## 2 · TUI (`plug`)

### Foco entre painéis

`Panel` ganha `prev()`. Em `App::on_normal`:

| Tecla | Efeito |
|---|---|
| `h`, ← | foco para o painel à esquerda |
| `l`, → | foco para o painel à direita |
| `Tab` | ciclo adiante (já existe) |
| `Shift+Tab` | ciclo para trás |

`Shift+Tab` exige uma variante `Key::BackTab` e o mapeamento de
`KeyCode::BackTab` na tradução de eventos em `main.rs:724`.

Nas pontas, `h`/`l` **dão a volta**, como o `Tab` já dá. A seleção dentro de um
painel continua prendendo, e a diferença é deliberada: `Panel::next` já escolheu
ciclo por ser um anel de três; `move_selection` já escolheu prender porque uma
lista que dá a volta faz `j` e `G` significarem a mesma coisa com frequência
suficiente para nenhum dos dois ser confiável.

### Busca

`App` ganha `busca: Option<Search>` e o `termo` que a produziu.

No modo Busca, digitar reconstrói ao vivo, e o contador `[1/3]` anda enquanto se
digita — é o retorno que diz se vale continuar escrevendo. `Enter` confirma e
volta ao Normal **mantendo** o destaque; `Esc` cancela e apaga. No Normal, `n` e
`N` andam entre as ocorrências; as duas teclas estão livres hoje.

O painel de mensagens rola até `Search::current`. O `ui.rs` acende o intervalo do
casamento corrente com ênfase diferente das demais ocorrências visíveis — e,
como manda `specs/05`, a distinção não pode ser só cor: o contador `[1/3]` é a
informação textual que sobrevive ao `NO_COLOR`.

```
MENSAGENS
  12:01 ayanami
    verificando harmônicos

  12:03 shinji
    o [sync] caiu aqui        ◀ 1/3

  12:04 asuka
    sync normalizou

/ sync                    [1/3]  n ▸ N ◂
```

### Ejetar sem matar o processo

`command.rs:84` hoje amontoa `q | quit | sair | ejetar` em `Command::Quit`. Isso
se separa:

- `:q`, `:quit`, `:sair` — saem do programa, como hoje.
- `:ejetar` — **novo `Command::Eject`**: volta à tela de seleção.

É mudança de comportamento de um comando existente, e é a certa: o botão do app
se chama EJETAR e faz exatamente isso. O teste que hoje afirma
`parse(":ejetar") == Command::Quit` muda junto.

O `run()` vira um laço:

```
loop {
    escolher()  →  (nada escolhido? sai)
    sessão      →  ejetou? continua : sai
}
```

`Enlace` e `Voice` são soltos no fim de cada volta e reconstruídos na seguinte.
Com `--hospedar`, ejetar também chama `Hospedagem::encerrar()`, que espera a
porta voltar — sem isso, hospedar de novo na volta seguinte falha por porta
ocupada, que é precisamente o caso que o app já tratou em `disconnect`.

**Isto não é o que a pendência #9 recusou.** Lá a ideia era trocar a conexão por
baixo de uma sessão viva. Aqui é derrubar tudo e voltar a uma tela que não tem
roster, telemetria nem áudio — o mesmo caminho que o app percorre e que
funciona.

`plug --server casa:8383` também cai na tela de seleção ao ejetar, mesmo nunca a
tendo visto. Está certo: a flag disse aonde ir no arranque, e ejetar é o pedido
explícito de ir para outro lugar.

### Fora de escopo, de propósito

`Enter` sobre um `Node::Pilot` continua no-op (`main.rs:831`). Inventar uma ação
para aquela linha agora seria desenhar interface para preencher uma lacuna, e
não porque alguém precisa dela.

## 3 · GUI (app)

### Comandos Tauri novos

```
conhecidos()            -> Vec<Conhecido>
esquecer(alvo)          -> ()
analisar_convite(link)  -> ConviteLido
buscar(termo)           -> BuscaEstado
busca_andar(adiante)    -> BuscaEstado
busca_limpar()          -> ()
```

Todos finos, sem lógica própria. `BuscaEstado { casamentos: Vec<Match>, atual:
Option<Match>, posicao, total }` — os nomes desta struct ficam em português
porque ela é do app e serializa para a página, que é escrita em português; o que
ela carrega dentro é o `Match` do core.

A busca vive em `Session` ao lado do `plug`, como `Mutex<Option<Search>>`: o
cursor é estado de sessão, e mantê-lo lá é o que impede a regra de dar-a-volta
de ser reescrita em JavaScript.

### A tela de entrada

```
┌─ tela-boot ────────────────────────┐
│           ゼーレ  SEELE            │
│                                    │
│  ONDE VOCÊ JÁ ESTEVE               │
│  ▸ casa:8383        piloto  ontem  │
│    geofront:8383    rei    3 dias  │
│                                    │
│  DOGMA   [ 127.0.0.1:8383        ] │
│  PILOTO  [ piloto                ] │
│  CONVITE [ cole um seele://…     ] │
│  [x] inserir plug com áudio        │
│                                    │
│  [ INSERIR PLUG ] [ HOSPEDAR AQUI ]│
│                                    │
│  MELCHIOR ·  BALTHASAR ·  CASPER · │
└────────────────────────────────────┘
```

A lista fica **acima** do formulário, e o formulário continua visível. Sem
Dogmas visitados a seção some inteira e a tela é exatamente a de hoje — o estado
vazio não piora, e nada fica escondido atrás de um clique.

Clicar numa linha preenche e conecta com o apelido lembrado. Cada linha tem um
*esquecer*, que é o `Resultado::Esquecer` que a TUI já oferece.

Colar um `seele://` no campo CONVITE chama `analisar_convite`, que preenche
DOGMA e guarda o resto do lado Rust. O parsing nunca encosta no JavaScript, e o
teste `the_frontend_never_names_a_protocol_concept` continua valendo — a
impressão digital em particular **não** atravessa a ponte: o frontend recebe só
um booleano dizendo que ela existe e que este app ainda não a confere
(pendência 12).

### A metade invisível

O `connect` passa a chamar `Conhecidos::registrar` **depois** de dar certo,
copiando a política que o `plug` já escreveu em `main.rs:469`:

- registra só após sucesso — guardar antes encheria a lista de endereços errados
  digitados uma vez, que é o oposto de uma lista de atalhos;
- **um Dogma hospedado aqui não entra**: `127.0.0.1` não é lugar aonde se volta,
  é o botão HOSPEDAR;
- falhar em gravar o atalho nunca derruba a conversa já de pé.

### Busca no app

Campo no cabeçalho do painel de mensagens, com o contador e dois botões para
`n`/`N`. Teclado: `/` foca, `Enter` e `Shift+Enter` andam, `Esc` limpa.

Nada de `j`/`k` no app. A escolha foi paridade de composição, e o Tab nativo da
webview já resolve o percurso do formulário sem reimplementar foco.

Com busca ativa, a chegada de um snapshot refaz o `buscar`, porque os índices
andam quando mensagem nova chega.

## 4 · Erros e bordas

O princípio que amarra quase tudo já está em `specs/05`, linha 81: o arquivo de
visitados é **conveniência**, e pode ser apagado sem consequência.

| Situação | Comportamento |
|---|---|
| `conhecidos` ilegível ou corrompido | a seção some; conectar continua funcionando |
| `seele://` inválido | frase em `#boot-erro`; formulário intacto |
| busca sem resultado | `[0/0]`, nada rola, nenhum erro |
| termo vazio | limpa a busca |
| mensagem nova durante a busca | refaz; se o casamento atual sumiu, o cursor prende no mais próximo em vez de saltar ao começo |
| ejetar hospedando | `encerrar()` é esperado; se a porta não voltar, a tela de seleção **diz o motivo** em vez de deixar o próximo HOSPEDAR falhar calado |
| ejetar durante a bateria interna | permitido; a bateria é propriedade de uma conexão que está sendo derrubada de qualquer forma |

## 5 · Testes

**`seele-core/src/search.rs`** — dobra de caixa; dobra de acento; a volta nas
duas pontas; termo vazio; zero casamentos. E um que não pode faltar: `nao`
casando `não` devolve intervalo de **três** caracteres. É exatamente aí que uma
tabela que deixasse de ser 1:1 se denunciaria.

**`seele-tui/src/app.rs`** — `h`/`l`/`Shift+Tab` movem o foco e dão a volta;
`n`/`N` andam no cursor; `Esc` apaga o destaque e `Enter` o mantém.

**`seele-tui/src/command.rs`** — `:ejetar` deixa de virar `Quit`; `:q` e `:sair`
continuam.

**`seele-conformance`** — teste novo: conecta, ejeta e conecta de novo **no
mesmo processo**. É o único jeito de provar que o teardown do `Enlace` e do
`Voice` fecha de verdade, e é a parte de risco real deste trabalho. Se ele não
passar, a decisão do laço externo estava errada, e é melhor descobrir aqui do
que dentro da pendência #9.

**`apps/seele-app/tests/frontend.rs`** — os guardas existentes cobrem os
comandos novos de graça: todo `invoke` tem que estar registrado, todo registrado
tem que ser chamado, e todo id lido tem que existir na página.

## Nota de convenção

`specs/10-convencoes.md` manda comentários de código em inglês, e o
`seele-tui/src/selecao.rs` está inteiro em português. A deriva existe e não é
assunto deste trabalho: cada arquivo tocado segue o idioma que já usa, e código
novo em `seele-core` segue o inglês do crate.

## O que este trabalho não faz

- Não implementa nenhum degrau do ADR 0022 (alcançabilidade pela internet).
- Não fecha a pendência #9: `:conectar <host>` continua avisando que não
  reconecta. O laço externo destrava o caminho, mas trocar de destino num
  comando só é trabalho à parte.
- Não toca em limitação de taxa (pendência #5), que continua bloqueando expor à
  internet.
