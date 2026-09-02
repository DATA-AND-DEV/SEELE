# ADR 0042 — Nomes repetidos, e a chave que distingue

**Estado:** proposto
**Data:** 2026-09-02

Dois pessoas no mesmo servidor podem ter o mesmo apelido. A identidade continua
sendo a chave; o apelido passa a ser rótulo. O ADR 0017 é revisado nesta parte e
mantido em todo o resto.

## Contexto

O ADR 0017 diz que o apelido pertence a uma chave, e o servidor cumpre isso
recusando: chave nova pedindo nome que já é de outra chave leva `NicknameTaken`
e não entra. A razão era boa — o nome é como as pessoas se reconhecem, e deixar
alguém tomá-lo faria as mensagens antigas de uma pessoa mudarem de dono na tela
de todo mundo.

O custo apareceu em campo, duas vezes na mesma semana:

> «pessoa não autorizada entra no link, eu autorizo, e ela não consegue entrar
> porque o nome já existe»

> «fui fazer um teste com um usuário novo e ele entrou como piloto»

A primeira é a recusa doendo em quem não fez nada de errado. A segunda mostra
que o nome com que alguém chega quase nunca é escolha: é o que estava na caixa
de texto — `piloto` numa versão antiga, `pessoa` na de hoje.

**Recusar entrada por causa de um nome é caro e é raro.** Nenhum produto de
conversa faz isso hoje. O que os produtos fazem é aceitar o nome e distinguir as
pessoas por outra coisa.

## A decisão

O apelido deixa de ser único. Quem distingue é a chave, que já é a identidade em
todo o resto do sistema (ADR 0004).

**E o discriminador não aparece.** Foi o pedido de quem usa, e ele tem razão
sobre o essencial: este produto já mostra o **retrato** de cada pessoa, no
roster, nos cartões da chamada e ao lado de cada mensagem. Duas pessoas com o
mesmo nome e rostos diferentes não se confundem, e pendurar um `#1234` na cara de
todo mundo para resolver o caso raro é o que o Discord fez e depois desfez.

## Onde o nome é identidade hoje, e é o trabalho deste ADR

Não é um problema de tela: **é código que usa o nome como chave**. Quatro
lugares, achados antes de escrever isto:

1. **`schema.rs:52`** — `nickname TEXT NOT NULL UNIQUE`. É a restrição que torna
   duplicatas impossíveis, e é por onde a mudança começa: uma migração que a
   remove.

2. **`permissions::register_or_find`** — recusa quando o nome é de outra chave.
   Passa a só procurar pela chave.

3. **`permissions::rename`** — mesma recusa, e some pelo mesmo motivo.

4. **`state.rs:596`, `ssrc_of`** — resolve uma pessoa **pelo nome**, sem
   diferenciar maiúsculas, e devolve a primeira que casar. É quem `set_volume`
   usa: com dois `rafa`, abaixar o volume de um abaixa o de quem vier primeiro na
   lista. **Este é o pior dos quatro**, porque não dá erro nem aviso — ele
   simplesmente obedece à pessoa errada.

5. **`tela-chamada.js:341`** — a grade da chamada procura o cartão existente por
   `pessoa.nickname`. Dois homônimos caem no mesmo cartão, e um deles **some da
   tela**.

Os dois últimos são a razão de este ADR existir antes do código. Nenhum aparece
numa revisão de tela, e os dois são silenciosos.

## O que fica de fora, e é onde o retrato não alcança

Os diálogos de moderação listam gente como texto puro — `rafa — PONTE` — e ali
não há rosto. Com homônimos, expulsar vira sorteio.

E quando ninguém escolheu retrato, a inicial desenhada sai do próprio nome: dois
`rafa` sem foto mostram o mesmo `R`.

**A saída não é voltar a pendurar hash em todo mundo.** É o discriminador
aparecer só onde a ambiguidade existe de fato: uma lista que contém dois nomes
iguais mostra os quatro primeiros caracteres da impressão ao lado dos dois, e só
deles. Enquanto houver um `rafa` só, ele é `rafa`.

## O que **não** muda

**A chave continua sendo a identidade** (ADR 0004), e é ela que o servidor
procura ao admitir alguém. Trocar de nome continua valendo e continua barato: o
histórico acompanha, porque `persistence::messages` resolve o autor por `JOIN` e
lê o nome de agora, em vez de guardar uma cópia por mensagem.

**O apelido continua sendo por servidor**, e não global.

## Consequências

**`NicknameTaken` deixa de existir como recusa de entrada.** A frase que a
acompanha sai com ela — e ela era a única desta lista que a pessoa consertava
sozinha.

**Ninguém é barrado por causa de um nome.** Era o objetivo.

**O recurso automático fica menos importante e continua certo.** `pessoa-a3f1`,
derivado da impressão da máquina, deixa de ser necessário para evitar colisão —
mas continua sendo o que faz duas pessoas sem nome parecerem duas pessoas.

**E o custo é uma passada por todo lugar que compara nomes.** O inventário acima
é o que se conhece hoje; a implementação começa por trocar o tipo — quem hoje
recebe `nickname: String` para achar alguém passa a receber a identidade — e
deixar o compilador achar o resto.

## Alternativas

**Sufixo visível para todos**, como o Discord antigo. Resolve tudo e cobra de
todo mundo pelo problema de poucos. Recusada pelo mesmo motivo que o Discord a
desfez.

**Renomear automaticamente o segundo** — `rafa (2)`. Evita a recusa e mente sobre
o nome que a pessoa escolheu; ela passa a ser chamada de uma coisa que não pediu.

**Manter a recusa e melhorar a frase.** Foi o que se fez até aqui, e é o que este
ADR conclui não bastar: a frase melhor não muda o fato de alguém não entrar.
