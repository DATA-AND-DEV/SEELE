# 07 — Estética

> **Nota — 2026-08-31.** Este documento se chamava `07-tema-evangelion.md` e
> carregava, além da estética, um glossário de vocabulário temático apresentado
> como «obrigatório em toda a superfície do produto».
>
> Aquele vocabulário saiu da interface no [ADR 0033](../docs/adr/0033-o-vocabulario-sai-da-interface-a-estetica-fica.md),
> saiu da marca no [ADR 0034](../docs/adr/0034-a-marca-abandona-as-duas-citacoes-do-anime.md)
> e saiu do código no [ADR 0035](../docs/adr/0035-o-codigo-deixa-de-falar-evangelion.md).
> A tabela ficou aqui congelada «como registro do que o produto foi» — e
> continuou dizendo que aqueles termos eram obrigatórios, o que havia deixado de
> ser verdade em três lugares.
>
> **Um documento que descreve como vigente uma decisão revogada é pior que um
> documento que não existe**: o primeiro manda alguém obedecer, o segundo só não
> ajuda. A tabela saiu; a história dela está nos três ADRs, que é onde história
> se guarda. **A autoridade sobre a palavra que a pessoa lê é
> [`docs/glossario.md`](../docs/glossario.md).**
>
> O que sobrou aqui é o que sempre foi o mais deste documento, e continua
> valendo inteiro: a regra de ouro da densidade, a hierarquia de comando, os
> tokens de cor, a tipografia, o canto reto, a ausência de sombra e de gradiente.
> **Saiu a língua; ficou o desenho.**

A estética é decidida aqui uma vez e aplicada em todo lugar: interface,
mensagens de erro, logs, documentação. Não se redesenha por tela.

## Regra de ouro

A referência não é «laranja e preto com fonte futurista». É a **densidade de
informação** e a **hierarquia de comando** de um console de operação: muito
rótulo pequeno, muitos números vivos, quase nenhuma decoração, e a sensação de
que tudo está sendo medido.

Segunda regra: **a estética nunca custa clareza**. Se alguém precisa decifrar um
rótulo para resolver um problema, a interface falhou. Nome vem sempre
acompanhado do dado concreto — `perda · 8,4%`, e não só `perda`.

## O elemento assinatura

**O sinal, por pessoa.** Cada pessoa na lista tem um valor vivo derivado do RTT,
do jitter e da perda daquela conexão. Nenhum concorrente mostra isso; aqui é a
coisa mais visível da tela. É a medida que dá caráter ao produto e, não por
acaso, é genuinamente útil — quando alguém fica difícil de entender, todo mundo
já sabe por quê.

As faixas são três, e estão no [ADR 0024](../docs/adr/0024-faixas-de-sincronia-em-tres-e-a-media-no-core.md).

## A queda não fecha a tela

Quando a conexão cai, o cliente **não fecha e não mostra um spinner**. A
interface esmaece, conta um período de graça de cinco minutos, lista as
tentativas de reconexão, e o histórico continua ali para leitura.

Isso é uma faixa **sobre** a sessão, e não uma tela que substitui o que estava
escrito — a distinção é a regra, não o detalhe de implementação. Uma tela de
«sessão encerrada» existe e é outro momento: ela aparece quando o período de
graça acaba, e oferece reconectar.

Funcionalmente é um período de graça sustentado pela migração de conexão do
QUIC (ver `01`). É o melhor casamento entre desenho e engenharia no projeto —
proteger de simplificações.

## Tokens de cor

Valores definitivos saem do trabalho de design; estas são as **restrições** que
aquele trabalho precisa respeitar:

| Papel | Regra |
|---|---|
| Fundo | Preto quase absoluto, nunca cinza-carvão neutro |
| Acento primário | Laranja institucional — cor de identidade, **não** cor de sucesso |
| Alerta | Vermelho, uso exclusivo para erro e queda. Se aparece, algo está errado |
| Nominal / telemetria | Verde de fósforo |
| Identidade verificada | Azul |
| Texto corrido | Off-white levemente amarelado. Branco puro é errado |

Faixas do sinal: **≥ 85** nominal (fósforo) · **60–84** degradado (laranja) ·
**< 60** crítico (vermelho).

Eram quatro — `≥ 90` nominal, `70–89` aceitável em off-white, `40–69`
degradado, `< 40` crítico. O comp v2 banda o mesmo número em três, corta em 85 e
60, e **não usa osso em escala nenhuma**; o comp é posterior a esta tabela e o
dono decidiu que ele vence. A consequência que importa: 80 lia-se como «fora do
nominal, mas tudo bem» e agora se lê como degradado — laranja, a cor de ir
olhar. É o objetivo da mudança, não um efeito colateral dela.

## Tipografia

Monoespaçada para todo dado, número, endereço e log. Display condensada e
pesada, caixa alta e tracking apertado, para cabeçalhos e cartelas de alerta.

**O japonês decorativo saiu.** Ele era acento tipográfico e nunca carregou
informação necessária para operar o produto — foi por isso que pôde sair sem
que nada se perdesse. O ADR 0034 tirou as duas citações que restavam na marca, e
o resto saiu com elas. Um guarda em `crates/seele-tui/src/ui.rs` afirma que o
katakana não volta à tela.

## Canto reto e ausência de sombra

O recuo do produto é **canto reto e sem sombra**: `--seele-raio` vale `0` e
`--seele-sombra` vale `none` em `tokens.css`, e é assim que toda tela do SEELE
abre. Não é falta de um recurso; é a escolha que dá o ar de terminal, e ela vale
para a interface, para as camadas e para a TUI.

**Uma exceção nomeada: o tema de servidor.** Quem administra um servidor pode
levantar os dois pelo MOD de tema, e o efeito vale **só naquela sessão** — é
escrito no contêiner da sessão e sai com ela, sem tocar em preferência nenhuma
de quem usa. Fora da sessão, e em toda instalação que não entrou num servidor
com esse tema, o recuo continua sendo o de cima.

Três coisas tornam a exceção admissível, e as três são obrigatórias:

1. **O arredondamento é inteiro entre 0 e 24**, conferido pelo produto. Um
   número livre aceita `9999`, que transforma cada painel num comprimido.
2. **O brilho é sim ou não**, e a sombra que ele liga é montada pelo produto a
   partir de um token do produto. Aceitar a sombra escrita por um MOD faria a
   API de tema virar a porta de entrada de qualquer `box-shadow`, inclusive um
   que desenhe fora do painel e cubra a tela.
3. **A marca nunca acompanha.** `docs/marca.md`, regra 1, continua valendo
   inteira: o símbolo não ganha raio nem sombra em situação nenhuma, e um tema
   de servidor não o alcança.

Abrir uma segunda exceção — um raio que sobreviva à sessão, uma sombra de valor
livre, um gradiente — exige emendar este parágrafo, e não um argumento de que
«o token já existia».

### Emenda de 20/09/2026 — o que um MOD desenha **dentro do que é dele**

A regra acima é sobre **as telas do SEELE**: a conversa, as camadas, a
configuração, a TUI. Ela continua inteira, e nada aqui a afrouxa.

O que a auditoria de UX/UI de 20/09/2026 encontrou foi outra coisa, e o achado
U04 a nomeia: a mesma regra estava sendo lida como se valesse também para o que
um MOD desenha na superfície **dele**, e o resultado era «não limitar o visual
de um perfil a seis cores e dois enums». Um MOD de perfis que queira um cartão
com faixa, retrato sobreposto e cor própria não está mexendo na estética do
SEELE — ele está desenhando o conteúdo que ele existe para desenhar.

**A fronteira é alcance, e não gosto.** Um estilo de MOD:

1. **não alcança fora da superfície dele.** As regras são compiladas numa folha
   presa àquela raiz, com um escopo que o produto escreve. Uma classe `cartao`
   de um MOD não encontra `.roster-linha`, não encontra a portaria e não
   encontra a superfície de outro MOD;
2. **não é texto de CSS.** Toda propriedade é declarada por nome, validada por
   categoria e montada de partes conferidas — `sombra: { x, y, desfoque, cor }`,
   e nunca `box-shadow: <texto>`. Não existe, e não deve passar a existir, uma
   função que receba CSS e tente limpá-lo: limpeza por substituição de texto é
   uma corrida contra o analisador do navegador, e quem o escreve não sabe que
   ela existe;
3. **não busca bytes na rede.** `url(...)`, `@import` e família de tipo livre
   ficam de fora. Recursos visuais entram por identificador, do pacote declarado
   ou da metade de servidor do próprio MOD;
4. **não sobrevive à saída.** A folha é um recurso com dono, e ela sai com a
   superfície.

As telas de confiança do produto continuam fora de alcance por construção: um
MOD não contribui para a entrada, para a portaria, para a confirmação de
descarte nem para o aceite de conjunto. Personalização estética pode substituir
a faixa de sinal; não pode falsificar autorização, bloquear saída ou encobrir
uma confirmação.

**A marca continua sem exceção nenhuma.** A regra 1 de `docs/marca.md` vale
igual: o símbolo não ganha raio, sombra, gradiente nem animação, e nenhuma
superfície de MOD o alcança.

O contrato está em `api/v4.json`, e a validação em
`apps/seele-app/ui/mods-estilos.js`. Ver ADR 0052.

## Movimento

Só a sequência de boot é generosa. No resto, movimento é diagnóstico: a barra do
sinal respira, o indicador de fala pulsa com a voz, a contagem da queda desce.
Sem transição decorativa, com **uma exceção nomeada**: a varredura — a faixa que
desce sobre a scanline, herdada do comp v2 e aceita em M5 pelo
[ADR 0014](../docs/adr/0014-palheta-v2-canonica.md). Ela não diagnostica nada, e
o que a torna admissível é ser inofensiva: `pointer-events: none`,
`aria-hidden`, e sob `prefers-reduced-motion` ela para sem sumir. É a única;
qualquer outra volta a ser erro, e abrir a segunda exige emendar este parágrafo
de novo. `prefers-reduced-motion` respeitado, e a TUI oferece desligar animação
por completo.

Nenhuma animação pode atrasar quem usa. Se a conexão fecha em 200 ms, o boot
dura 200 ms.

## Voz da interface

Operacional, fria, factual. A interface **reporta**; não pede desculpa, não é
simpática, não usa primeira pessoa.

- Certo: `CONEXÃO SEGURA NÃO ESTABELECIDA · credencial rejeitada`
- Certo: `SALA-02 vazia. Conecte para iniciar.`
- Errado: `Ops! Não conseguimos te conectar 😥`
- Errado: `Nenhuma mensagem ainda!`

Erro sempre diz **o que aconteceu** e **o que fazer**. Tela vazia é convite à
ação, não piada.

## Direitos

Fechado pelos ADRs 0033, 0034 e 0035, e sem `[EM ABERTO]` a resolver: o produto
não usa vocabulário, logotipo, arte, trilha ou nome de personagem de obra de
terceiro. O que a estética herda — densidade, hierarquia de console, cartela de
alerta, monoespaçada em caixa alta — é linguagem visual de interface industrial,
que não pertence a ninguém.

O nome **SEELE** fica, e a razão está no ADR 0034: a palavra é alemã, quer dizer
«alma», e não é marca de terceiro. O que saiu foi a escrita dela em katakana.
