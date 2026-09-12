# ADR 0045 — Toda versão continua de pé: o app vira launcher

**Estado:** proposto
**Data:** 2026-09-05

**Substitui o [ADR 0026](0026-duas-assinaturas-e-um-botao-de-atualizar.md) na
parte do botão de atualizar.** As duas assinaturas daquele ADR ficam inteiras e
são pré-requisito desta página.

Esta decisão existe por causa do [ADR 0044](0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md),
e sozinha não teria motivo. Ela está numa página separada porque tem custo,
alternativas e custo de reverter próprios — e porque quem for ler o 0026
procurando «por que o botão virou um seletor» precisa achar isto, não um ADR
sobre MODs.

## Contexto

O pedido do dono, depois de o desenho de MODs estar fechado:

> «MODs são feitos pensados numa versão específica. Ou seja, se tiver uma
> atualização, pode impactar diretamente no MOD. Por isso: servidores rodarão as
> versões que quiserem, a la Minecraft. O host quer usar mod x e mod y só que
> esses mods foram feitos para uma versão anterior a latest. Quando o host for
> hospedar, pode escolher a versão do server que é compatível.»

### A tolerância corre para o lado errado deste pedido

Há uma janela de compatibilidade no protocolo, e ela é assimétrica.
`crates/seele-proto/src/version.rs`: `PROTOCOL_VERSION = 3`,
`COMPATIBILITY_WINDOW = 1`. O comentário do próprio módulo diz o resultado:

> **«um cliente v3 não alcança mais um servidor v2»** — o servidor faz
> `negotiate(3)`, vê 3 acima do que ele fala, e recusa com `Incompatible`.

**Cliente velho alcança servidor novo; cliente novo não alcança servidor velho.**
Um host que fixasse o servidor numa versão anterior por causa de MOD trancaria do
lado de fora exatamente quem está atualizado — que é todo mundo, porque o ADR
0026 pôs um botão de atualizar no app.

E não é um `if` a corrigir. Uma linha acima está o motivo: **o `postcard` indexa
variante por posição e não é autodescritivo** — uma variante desconhecida não é
ignorada, *«ela desloca a leitura do fluxo para sempre»*. Falar duas versões do
protocolo significa **manter dois conjuntos de tipos vivos no código**, não
tolerar um campo a mais. O `SCREEN_HEADER_VERSION` já mostrou o preço disso uma
vez, herdando esta janela sem precisar dela.

### E não há migração reversa

`persistence/schema.rs`: *«Append only once shipped»*, e
`no_migration_contains_a_down_step` reprova `DROP TABLE` e `DROP COLUMN`, com o
motivo escrito — *«a recuperação segura de uma migração ruim é um backup, não uma
migração reversa que ninguém testou»*. Um servidor velho contra um banco que um
servidor novo já migrou não tem volta.

## Decisão

**O app vira launcher. Cliente e servidor são versionados juntos, e ao conectar o
cliente roda a versão daquele servidor. Toda versão publicada continua hospedável
e alcançável, para sempre, exceto as revogadas.**

Foi escolhido com o custo na mesa duas vezes, e o custo está escrito abaixo.

### Uma versão do produto é uma unidade

Cliente, servidor, protocolo e API de MOD sobem juntos e são identificados pelo
mesmo número. **Não há matriz de compatibilidade**, e essa é a propriedade que
torna a decisão sustentável: a única pergunta que alguém precisa responder é
«qual versão», nunca «qual combinação».

É o que permite ao ADR 0044 abandonar a camada de compatibilidade dentro do
servidor. Um MOD roda contra a versão para a qual foi escrito, e nenhuma versão
precisa fingir ser outra.

### Um diretório de dados por versão

Porque não há migração reversa. Descer de versão não converte banco: abre outro,
como um mundo de Minecraft.

**Isto vai na tela, não numa nota de release:** descer de versão não leva as
conversas junto. Um produto que descobre isso pelo silêncio é o defeito que o
`CLAUDE.md` chama de *«o produto sabe e não conta»*.

### Cada build assinada, e o manifesto continua no release

A chave `minisign` do ADR 0026 assina cada versão, e a conferência acontece
**antes de qualquer arquivo ser tocado** — a propriedade que aquele ADR já
comprou (*«falha no meio da atualização não deixa meia instalação»*) vale sem
emenda aqui.

O `latest.json` vira um **manifesto de versões**, e continua morando em
`releases/latest/download/`, no release do GitHub. Nenhum serviço novo a
hospedar para isso valer — que era metade do orgulho do 0026 e sobrevive.

### A exceção: revogação

«Toda versão para sempre» tem um dia ruim e ele é previsível: uma versão velha
com falha de segurança continua rodável. Guarda-se tudo **menos as com falha
grave**, e isso exige uma peça que hoje não existe:

- Uma **lista de revogação assinada**, dentro do manifesto de versões.
- Uma versão revogada **não é hospedável e não é alcançável**, e a recusa diz
  qual é o defeito e qual versão o corrige — nunca genérica, que é o que
  `specs/02-protocolo.md` já exige de toda razão.
- A lista é conferida **ao hospedar** e **ao entrar**, que são atos, e **nunca ao
  abrir o app**. É o que preserva a regra do 0026: *«num produto cujo argumento é
  que o servidor é seu, um app que fala com o github.com a cada arranque
  contradiz o argumento»*.

**Esta é a parte cara, e ela reintroduz uma consulta que o 0026 tinha orgulho de
não ter.** Amarrá-la a atos em vez de ao arranque é a redução mais barata que
existe aqui, e ela não zera o custo.

A mesma peça serve à remoção de um MOD do indexador (ADR 0044), e as duas devem
ser a mesma coisa — duas listas de revogação seriam duas chances de esquecer uma.

> **Emenda, 10/09/2026 — «a mesma coisa» é o código, não o arquivo.** «Uma lista
> só» não cabe junto com a chave separada que o ADR 0044 exige do indexador: ou
> tirar um MOD do ar passaria a precisar da chave que autoriza instalar
> programa, ou a chave do indexador passaria a poder impedir um programa de
> rodar. O que ficou: **uma implementação, dois documentos, e cada fato num
> lugar só** — versões revogadas no manifesto, com a chave do atualizador; MODs
> removidos no catálogo, com a chave do indexador. O argumento inteiro, o
> formato e o procedimento estão em
> [`docs/versoes-lado-a-lado.md`](../versoes-lado-a-lado.md), que é também
> onde mora o contrato do manifesto e a lista do que falta publicar.

### O que o ADR 0026 perde, e o que ele mantém

**Mantém:** as duas assinaturas, o manifesto no release, «quem decide é a
pessoa», nenhuma consulta ao abrir, e a conferência antes de tocar arquivo.

**Perde:** «um app, uma versão instalada». O botão de atualizar vira um seletor
de versão, e o argumento do 0026 contra atualização silenciosa — *«num app de
conversa, um binário trocado sem aviso é intrusivo»* — sobrevive reforçado: um
seletor é ainda mais explícito que um botão.

**Passa a existir:** disco ocupado por versões guardadas, e uma tela que diz
quanto e permite apagar.

## Alternativas consideradas

1. **Só a fachada congelada do ADR 0044, sem multi-versão.** Um número próprio
   para a API de MOD, e o servidor mais novo continua rodando MOD antigo pela
   fachada. Foi a recomendação inicial e foi recusada pelo dono depois de duas
   apresentações. O que ela comprava: nada de launcher, protocolo e instalador
   intactos, ninguém trancado do lado de fora. O que ela não cobre — e é o motivo
   real da recusa — é **mudança de comportamento**: nome é barato de preservar,
   semântica não é. Se «expulsar» mudar de significado, nenhuma fachada segura um
   MOD que dependia do significado antigo.

2. **Alargar a janela de compatibilidade do protocolo.** Trocar `N−1` por `N−5` e
   deixar um cliente falar com servidores velhos. Recusada pelo `postcard`: sem
   autodescrição, isso é manter cinco conjuntos de tipos vivos, e o custo cresce
   com cada variante nova para sempre. Some-se que a UI teria de ter uma cara por
   versão — 15 mil linhas de JavaScript que ninguém versiona hoje.

3. **Versionar só o servidor, e manter um cliente só.** O host escolhe a versão do
   servidor; o cliente é sempre o mais novo. Mais barato, e quebra pela metade: um
   MOD de cliente — que é metade dos exemplos que o dono deu — não teria versão
   contra a qual ser escrito, e o autor viraria refém de toda atualização.

4. **Nada: uma versão só, e MOD acompanha ou morre.** É o que existe hoje e é de
   graça. Recusada pelo pedido, e por uma consequência que o pedido antecipa: um
   catálogo cujos MODs quebram a cada release é um catálogo que apodrece sozinho,
   e o indexador do 0044 seria infraestrutura mantida para nada.

## Consequências

- **O ADR 0026 é substituído na parte do botão**, e as duas assinaturas dele
  viram pré-requisito em vez de decisão isolada.
- **Cada release vira artefato permanente.** É um compromisso sem fim conhecido,
  em disco nosso e no de quem instala.
- **O instalador ganha um conceito que não tinha:** versões lado a lado. O ADR
  0043 acabou de decidir que o instalador do Windows é nosso, o que torna isto
  possível de fazer bem — e o torna trabalho nosso, não do NSIS.
- **A negociação de versão do protocolo deixa de ser a defesa principal** e passa
  a ser uma conferência de coerência: os dois lados já sabem a versão antes de
  falar. O `negotiate` fica onde está, porque a primeira coisa que toca um byte
  de socket não confiável continua tendo de ser total.
- **Um segundo lugar onde o produto consulta a rede por obrigação** — a lista de
  revogação —, além do indexador de MODs.
- **`SCREEN_HEADER_VERSION` deixa de herdar uma janela que não era dele**, porque
  a versão passa a ser do produto inteiro.

## O que fica sem saída

**Guardar toda versão para sempre é um compromisso que não tem data de fim**, e
quem o assume hoje não é quem vai pagá-lo daqui a três anos.

**A revogação é a válvula e ela é imperfeita.** Uma máquina que nunca consulta a
lista roda a versão furada, e amarrar a consulta a atos — em vez de ao arranque —
é exatamente o que abre essa brecha. Foi escolhido assim de propósito, porque a
alternativa contradiz o argumento do produto.

**Um seletor de versão é uma pergunta a mais para quem só queria conversar.** O
custo de dar escolha é que ela tem de ser feita, e a maioria das pessoas não sabe
responder «qual versão». O padrão tem de ser a mais nova, e o seletor tem de
aparecer só quando ele importa — que é ao hospedar, e ao entrar num servidor que
não é a mais nova.

**Descer de versão perde as conversas**, e não há versão desta decisão em que não
perca, porque não há migração reversa e escrever uma seria escrever a coisa que
`no_migration_contains_a_down_step` existe para proibir.

## Custo de reverter

**Alto, e assimétrico.** Voltar a «um app, uma versão» é fácil no código e
impossível na promessa: a partir do dia em que alguém hospedar numa versão velha
com MODs escritos para ela, desfazer isto tranca esse servidor. É a mesma classe
da API congelada do ADR 0044 — a promessa é o produto, não a implementação dela.

**Baixo enquanto ninguém tiver hospedado numa versão que não a mais nova.** Até
lá, isto é um seletor com uma opção.
