# ADR 0039 — O produto passa a ter uma casca só

**Estado:** aceito
**Data:** 2026-08-31

O `seele-tui` — 10.286 linhas, o cliente de terminal — sai do repositório. O
produto passa a ser a casca gráfica e o `seeled`.

## Contexto

A TUI foi a casca **de referência**. O README abria com «voz e texto
auto-hospedados, com o terminal em primeiro lugar» e dizia que «a interface de
referência é aquela que cabe em 80×24»; a `specs/05-cliente-tui.md` fechava
critérios de aceite sobre SSH e dezesseis cores; a `specs/09-roadmap.md` a
entregou no M4.

**Ela deixou de ser distribuída no ADR 0035**, e o commit `c8b7e57` a tirou do
pacote de instalação. Desde então nenhum script de empacotamento a compila —
nem o `macos.sh`, nem o `windows.ps1`, nem o workflow de release. O
`docs/glossario.md` registra a linha «`seele` — não distribuído desde o ADR
0035».

O que ficou foi o pior dos dois mundos: um crate mantido, compilado a cada
`cargo test`, citado como referência de desenho em cem lugares — e que ninguém
podia instalar. O README continuou vendendo-o como o cliente principal por mais
de um mês.

## Decisão

**O `seele-tui` é removido, e o produto passa a ter uma casca só.**

O que sai junto:

- **`crates/seele-conformance/tests/acceptance_m4.rs`.** Ele é a suíte de aceite
  da TUI — «funciona por SSH em terminal de dezesseis cores sem perder
  informação», «do lançamento até pronto para falar em menos de 1,5 s». São
  critérios sobre o terminal, e morrem com ele. Nenhuma cobertura do núcleo se
  perde: as regras de renderização que ele exercitava eram do crate removido.
- **`specs/05-cliente-tui.md`**, mas **não inteira** — ver abaixo.

## O que a spec 05 tinha e não era sobre o terminal

Esta é a parte que quase se perdeu, e é a razão de este ADR ser mais longo que a
remoção que descreve.

Ao apagar a spec 05, cento e noventa e seis citações espalhadas pelo código
ficariam órfãs — e a amostragem delas mostrou que a maioria **não citava o
terminal**: citava acessibilidade. *«Proíbe carregar por cor sozinha»*,
*«legível sem depender de cor nenhuma»*. Regras que valem em qualquer superfície
e que a casca gráfica obedece hoje.

Elas foram para a `specs/06-clientes-gui.md`, que passa a ser a spec da única
casca:

- modo alto contraste e modo sem cor;
- **nenhuma informação transmitida só por cor** — a mais citada de dentro do
  código, e a que mais silenciosamente se perde: uma cor a mais é fácil de
  acrescentar, e ninguém percebe que ela virou a única fonte;
- leitor de tela, que continua `[EM ABERTO]` — mas por escopo, e não mais por
  viabilidade: numa TUI o caminho não existia, numa casca web existe (ARIA).

O que **não** foi: renderização em células, `NO_COLOR`, terminal mínimo de
80×24, e os critérios de aceite por SSH. Aquilo era sobre o terminal.

## Sobre as citações que sobraram

Cerca de cem comentários no código e nos ADRs continuam nomeando `seele-tui` —
quase todos dizendo que a casca gráfica **copiou** dela: o padrão de projetar o
snapshot inteiro (ADR 0019), as três faixas do sinal (ADR 0024), a colapsagem de
espaço na busca.

**Elas ficam.** São verdadeiras, e apagá-las apagaria a razão de a interface
gráfica ser como é. Uma referência a um crate que saiu é um convite a olhar o
histórico; uma explicação removida não é convite a nada.

## Alternativas

- **Manter o crate sem distribuir**, como estava. Recusada porque era o estado
  atual e ele custava: compilação em toda rodada, cem citações vivas para um
  binário que ninguém instala, e um README mentindo por um mês.
- **Voltar a distribuí-la.** Recusada pelo dono do produto, e a decisão é dele:
  duas cascas custam paridade em toda funcionalidade nova, e o
  `docs/glossario.md` já registrava três coisas que a gráfica tinha e o terminal
  não.
- **Apagar a spec 05 inteira.** Era o plano, e teria orfanado as regras de
  acessibilidade que o produto ainda obedece. Ver acima.

## Consequências

Menos 10.286 linhas de crate, menos uma suíte de aceite, menos uma spec. Uma
casca a menos para manter em paridade.

E uma perda que vale nomear: a conformidade tinha **duas cascas independentes**
exercitando o mesmo núcleo, e um defeito que aparecesse só numa era um sinal
barato de que a lógica estava na casca errada. Fica uma. O
`nenhuma_casca_reabre_a_voz_jogando_fora_os_controles` continua escrito como
lista sobre cascas, e não como conferência sobre um arquivo — para que a casca
móvel do M6 herde a cobrança sem que ninguém precise reescrever o teste.

## Custo de reverter

**Alto.** O crate está no histórico do git e volta com um `git revert`, mas
tudo que foi escrito depois desta data desconhece a existência dele: nenhuma
funcionalidade nova terá paridade em terminal, e a spec 05 não existe mais para
dizer o que aquela paridade seria.
