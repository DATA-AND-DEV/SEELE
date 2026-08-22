# ADR 0033 — O vocabulário de Evangelion sai da interface; a estética fica

**Estado:** aceito
**Data:** 2026-08-21

A `specs/07-tema-evangelion.md` decidiu que o tema é **vocabulário de produto,
não skin**, e que ele vale em toda a superfície: interface, erro, log,
documentação, nome de binário. Este ADR revoga **metade** daquela frase. O tema
continua sendo o desenho do produto inteiro. Ele deixa de ser a **língua** dele.

Dito na ordem em que importa: **sai a camada de linguagem, fica o desenho.** Um
canal de voz passa a se chamar sala de voz na tela e continua sendo `Cage` no
código.

## Contexto

Três coisas, e nenhuma delas sozinha teria bastado.

**A avaliação de usabilidade.** O produto tem um vocabulário inteiro que uma
pessoa precisa aprender antes de conseguir operar a primeira tela: «inserir
plug» para entrar numa conversa, «ejetar» para sair, «A.T. Field» para o
microfone fechado, «PADRÃO: AZUL» para a sessão verificada. A própria `07` já
tinha visto o risco e escreveu a segunda regra de ouro contra ele — *o tema
nunca custa clareza* — e a solução que ela propôs foi acoplar o dado concreto ao
termo temático: `Distúrbio harmônico · perda 8,4%`. Essa regra é uma confissão.
Se o termo só funciona acompanhado da tradução, quem carrega a informação é a
tradução, e o termo é peso. Metade da barra de estado do produto era termo mais
tradução ocupando o espaço de um dado.

**O alvo.** `00` põe o produto para grupos pequenos que já vivem em terminal, e
`08` descreve o operador como «alguém confiável mas não especialista». São duas
pessoas diferentes na mesma tela: quem hospeda não é necessariamente quem
programa. Um vocabulário privado cobra dessa segunda pessoa um custo que não tem
contrapartida — ela não ganha capacidade nenhuma por saber que a sala se chama
Cage.

**Quem vem do Discord.** O README abre dizendo que o SEELE «não é um clone de
Discord com tema escuro», e isso continua verdadeiro sobre a arquitetura: o
servidor é seu, não há serviço no meio. Mas é falso como previsão de quem chega:
quase todo mundo que vai abrir este produto abriu aquele antes, e chega sabendo
o que é servidor, canal de texto, canal de voz, apelido, mudo. Recusar essas
cinco palavras não afasta o produto do Discord — a arquitetura já faz isso. Só
obriga a pessoa a reaprender cinco nomes para as mesmas cinco coisas.

## Decisão

O vocabulário da interface passa a ser o comum: **servidor, sala de voz, canal
de texto, pessoa, apelido, conectar, sair, mudo, sinal, conexão segura.** O mapa
completo de tradução vive no `docs/glossario.md`, que este ADR reescreve e que
continua sendo a autoridade única sobre qual palavra aparece na tela.

Saem também da tela, **sem substituto**, duas coisas que não eram vocabulário:

- a cartela `ゼーレ` dentro da interface e o japonês decorativo (`第3新東京市`,
  `同期率`) — a marca continua sendo `ゼーレ` onde a marca aparece, e isso é
  assunto da `docs/marca.md`, não da tela de conversa;
- o diagrama e as três luzes **MELCHIOR · BALTHASAR · CASPER** do rodapé. Elas
  nunca mediram nada. Uma luz que não mede é cenário se passando por
  instrumento, e um instrumento falso num produto de comunicação é pior que
  nenhum — ele é consultado quando algo dá errado. A telemetria que **informa**
  fica inteira.

## O que este ADR explicitamente NÃO muda

Esta seção é a metade importante. Ela existe porque a leitura preguiçosa deste
ADR é «tira Evangelion do SEELE», e não é isso.

- **Nomes de tipo, campo, função, variante e crate.** `Cage`, `CageId`, `Dogma`,
  `Pilot`, `Line`, `SyncRatio`, `at_field`, `insert_plug`, `Pattern::Blue`
  continuam exatamente como estão. O código não é interface. Renomear tipo é
  outro trabalho, com outro custo, e não é este.
- **CASPER continua sendo o banco.** `Subsystem::Casper` é uma fronteira real de
  módulo — estado persistente, histórico, configuração, migrações. O que sai é a
  **luz** dele no rodapé, que não media nada; o subsistema fica.
- **MELCHIOR continua sendo as permissões.** Identidade, autenticação, sessões,
  papéis. Mesma razão, mesma fronteira.
- **BALTHASAR continua sendo o roteamento de mídia.**
- **A marca continua SEELE**, com o katakana, o laranja, o octógono, as regras
  da `docs/marca.md` inteiras. O nome do produto não estava em discussão.
- **A estética inteira.** Cores dos tokens congelados (ADR 0014), fontes, sem
  raio, sem sombra, densidade alta, o ar de terminal. Nada disso é linguagem.
- **`id=`, `class=`, chaves de evento, referências a ADR e a spec.** São
  identificadores que outro código procura.
- **`Entry Plug` como nome interno do cliente** e o binário `plug`. É nome de
  programa, não rótulo de tela.

## Alternativas

**Manter o vocabulário e apostar na regra da `07`** — termo temático sempre com
o dado concreto ao lado. É o que estava em vigor. Falha pela aritmética de tela:
a barra de estado paga duas vezes pela mesma informação, e num produto cuja
referência é 80×24 esse é o recurso mais escasso que existe.

**Vocabulário comum na TUI e temático no app gráfico.** Rejeitada de imediato:
`07` tem razão sobre o essencial quando diz que o tema não se redesenha por
tela. Dois vocabulários é pior que qualquer um dos dois.

**Tornar o vocabulário uma preferência.** Um botão que troca «sala de voz» por
«Cage». Rejeitada porque duplica a superfície de tradução (`ADR 0012`) para
sempre, em nome de um público que este ADR não conseguiu dimensionar, e porque a
primeira tela — a que decide se a pessoa fica — é a que a preferência ainda não
alcançou.

## Consequências

**Fica mais fácil** a primeira sessão de quem nunca viu o produto, e fica mais
fácil escrever documentação, mensagem de erro e texto de convite: não há mais
uma tabela para consultar antes de nomear uma coisa comum.

**Fica mais difícil** — e este é o custo real, não uma formalidade — manter a
identidade temática viva. O tema passa a existir só no desenho, e desenho é mais
fácil de diluir que palavra: a próxima pessoa que acrescentar uma tela vai
herdar as cores sem herdar o motivo delas. A `07` continua no repositório
justamente para isso, e a nota no alto dela diz o que foi retirado.

**Perde-se o elemento assinatura como nome.** A «Taxa de Sincronização» era, por
declaração da `07`, a coisa mais visível da tela e o que nenhum concorrente
mostra. Ela **não** sai: continua sendo um percentual vivo por pessoa, agora
chamado **sinal**, com as mesmas três faixas do ADR 0024 — boa, fraca, crítica.
O que se perde é o nome, e o nome era metade do charme. Vale a troca porque a
outra metade — mostrar a qualidade de conexão de cada um, o tempo todo —
sobrevive inteira, e era ela que era útil.

**Some uma pergunta de direitos.** O `07` deixa `[EM ABERTO]` a postura sobre a
franquia, e o `docs/glossario.md` notava que o risco se concentrava em `A.T.
Field` e `Entry Plug`, que são cunhagens da obra. `A.T. Field` sai da tela por
este ADR. `Entry Plug` fica como nome interno e como forma da marca desenhada.
Isto **não** fecha aquela decisão — ela continua em aberto e continua fora do
caminho crítico — mas encolhe a superfície dela para o que se vê no repositório,
não para o que se vê no produto.

## Custo de reverter

**Alto e crescente.** O vocabulário novo entra em interface, documentação,
convite e texto de erro ao mesmo tempo, e cada leitor novo aprende o nome novo.
Reverter não é trocar strings de volta: é pedir de novo, a mais gente, o
aprendizado que este ADR decidiu não cobrar. Se alguém quiser revogar isto, o
momento é agora, antes de M4.

**Nenhum no código.** Como nenhum tipo, campo ou identificador muda, uma
reversão não toca em compilação, migração de banco ou protocolo. Foi assim de
propósito: a decisão custa caro em produto e nada em engenharia, e separar os
dois é o que permite discutir uma sem reféns da outra.
