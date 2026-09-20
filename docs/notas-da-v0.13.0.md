# Notas da versão — v0.13.0

> **Esta página é o texto para o corpo do release, e o release ainda não
> existe.** A v0.13.0 é um **candidato em validação**. As jornadas principais
> foram percorridas no aplicativo nativo com os três MODs — criar campanha e
> cena, rolar dados, gravar perfil, confirmar descarte, sair —, e a rodada
> encontrou seis defeitos, todos consertados na seção 16 de
> `docs/decisoes-da-auditoria-2026-09-20.md`. O que **não** foi percorrido
> está listado lá e no fim desta página.
>
> A tag e a publicação são passo manual de quem opera. O passo a passo é o mesmo
> da v0.12.0, em `docs/mods-api-propria/runbook-publicacao-v0.12.0.md`, com os
> números desta versão trocados.

_As mudanças de `v0.12.1` (commit `655a137`, publicada em 20/09/2026) até aqui.
O número de commits sai do `git log` na hora de marcar a tag; se este texto for
publicado depois, refaça a conta em vez de copiar uma que envelheceu._

---

## ✅ Esta versão fala com a anterior

**Protocolo 7, o mesmo da v0.12.x.** Nenhuma das duas pontas precisa atualizar
junto, e um par v0.12.1 conversa com um v0.13.0 sem nada de especial.

**E os MODs que você tem instalados continuam funcionando.** É a diferença
principal em relação à volta passada, e ela foi uma decisão: a API de MODs
passou de 3 para 4 e a conferência **deixou de ser igualdade**. Um pacote de API
3 continua sendo executado por este build.

Se você atualizou da v0.11 para a v0.12 e viu todos os MODs pararem de carregar
até instalar a versão 2.0.0 deles: isso não acontece agora.

---

## O que motivou esta versão

Uma auditoria de UX/UI feita no aplicativo nativo 0.12.1, com os três MODs
oficiais instalados e ativos, navegando por ferramentas de acessibilidade e
capturas — sem substituir cliques por chamadas internas. Ela está inteira em
`docs/auditoria-ux-ui-mods-2026-09-20.md`.

O resultado dela sobre os MODs foi direto: *«A apresentação dos MODs não atende
ao objetivo de criar experiências próprias dentro de um servidor. A faixa
inferior limita atividades diferentes ao mesmo formulário estreito.»*

Ela também encontrou uma regressão funcional que nenhum teste pegava.

---

## A MESA voltou a criar campanha

**Este é o conserto mais direto da versão, e a causa merece ser dita.**

Criar uma mesa no MESA respondia `invalid-id` e nada acontecia. O servidor do
MOD recusa toda escrita que não traga `nonce`, e toda escrita depois da criação
que não concorde com `revision`. O cliente não mandava nenhum dos dois.

A suíte estava verde porque **o teste completava os dois campos** antes de
chamar o servidor. Ele provava um caminho que o produto não percorre.

O cliente passou a mandar os dois, e o harness passou a transportar o pedido
intacto. Reverter o conserto hoje quebra catorze casos, e o primeiro diz qual
campo faltou.

---

## Os MODs deixaram de caber num rodapé

Os três MODs oficiais dividiam uma faixa de 240 px no pé da janela. Editar um
perfil, escolher a aparência do servidor ou jogar uma sessão de RPG eram a mesma
caixa estreita, com uma rolagem que pertencia às três ao mesmo tempo: rolar o
PERFIS deslocava o ESTILO e fazia a MESA sumir.

E não havia gesto de **abrir** nenhum deles. Os três desenhavam ao conectar
porque a faixa era o único lugar onde eles podiam existir — o produto decidindo,
por omissão, que as três atividades estão acontecendo o tempo todo.

**Agora um MOD pede uma superfície própria.** Uma página na área da conversa,
um painel ajustável ao lado, um diálogo com foco contido, ou um aviso curto. E
ele registra uma **entrada na navegação do servidor**, que é o gesto que faltava:
ela não ocupa altura nenhuma até alguém apertá-la.

A voz não cai quando você abre uma delas. A coluna de salas, a lista de pessoas
e a saída continuam de pé, e a saída da superfície é um botão que o **produto**
desenha — um MOD que trave no meio da montagem continua sendo uma janela de onde
se sai.

### PERFIS

Clicar numa pessoa abre o perfil dela: faixa, retrato sobreposto, nome, pronomes
e status. «Editar meu perfil» abre um diálogo com o formulário e uma **prévia ao
lado**, que mostra como você vai aparecer na lista antes de você gravar.

A cor e o efeito que você escolhe voltaram a aparecer. Eles eram guardados e não
desenhados: agora a cor é a borda do seu retrato e o efeito é a animação dele.

«Sobre mim» deixou de ser um campo de uma linha.

### ESTILO

A aparência do servidor virou uma página com abas: cores com seletor e amostra
ao lado de um pedaço de conversa desenhado com o tema, forma (densidade, tipo,
cantos, brilho) e **conjuntos prontos** como ponto de partida.

A prévia é local: o que você escolhe aparece só na sua sessão até você publicar,
e desligar a prévia devolve o tema publicado sem jogar fora o que você estava
montando. Antes, o único jeito de ver uma cor era publicá-la para todo mundo.

Quem não administra recebe o tema — a amostra e o estado —, e não um formulário
de seis cores que o servidor recusaria.

### MESA

A mesa virou um espaço com abas: tabuleiro, fichas, compêndio, iniciativa e
registro. Só a aba aberta é montada, então um tabuleiro com sessenta peças não é
desenhado enquanto você lê o compêndio.

Criar campanha voltou a ser um diálogo, com **sistema e mestre escolhidos** — os
dois estavam fixos em «livre» e «quem apertou», e isso nunca foi decisão: era o
que cabia na faixa.

O mapa da cena deixou de ser uma imagem ao lado do tabuleiro e virou o **fundo**
dele, sob as peças.

E a trilha toca. A escolha de ambientação ia ao servidor e ninguém ouvia nada; o
pacote passa a trazer três laços curtos.

---

## Um MOD pode apresentar uma pessoa, e não só comentar sobre ela

Antes, um MOD podia acrescentar uma linha **embaixo** do nome de alguém na lista
de pessoas. O nome continuava sendo o apelido do servidor, e o cartão não
recebia clique.

Agora ele pode **substituir** a apresentação. Três coisas continuam sendo do
produto, e elas são o contrato inteiro:

- **o clique vai ao ID real**, e não ao texto que o MOD desenhou. Moderar
  continua agindo sobre a pessoa certa;
- **o diagnóstico nativo recolhe**, e não some: sinal, barra e estado do
  microfone continuam a um toque de distância;
- **a origem é dita pelo produto.** Uma tela de MOD indistinguível das do SEELE
  é o que tornaria uma tela de confiança falsificável.

Se dois MODs pedirem o mesmo lugar, a gestão de MODs mostra os dois nomes, qual
está desenhando, e «usar apresentação padrão» ao lado. Nada se resolve por quem
respondeu por último.

---

## A configuração deixou de se chamar TERMINAL SERVER

Ela mistura preferências desta máquina com administração de um servidor, e o
nome antigo descrevia só a primeira metade — a outra era justamente a que
ninguém encontraria procurando por ela.

As seções passaram a ser agrupadas por **escopo**, porque trocar o microfone vale
nesta máquina e renomear o servidor vale para todo mundo que entra nele:

- **Este dispositivo** — áudio, atalhos, aparência, identidade;
- **MODs** — instalados, catálogo, armazenamento, permissões;
- **Este servidor** — a porta e o nome, para quem administra;
- **Aplicativo** — versões, conexão e atualização.

A tela deixou de caber numa caixa de 840 × 620: ela ocupa a área útil da janela.
A conversa e a voz continuam atrás, e o cabeçalho diz isso — com a sala em que
você está, o estado do microfone, e o VOLTAR, que antes ficava no fim de uma
coluna que rola.

A gestão de MODs virou quatro abas. O mesmo pacote aparecia três vezes na mesma
rolagem — em instalados, no catálogo e no disco —, e achar um MOD para ligá-lo
passava pelas outras duas.

---

## E mais vinte coisas menores que estavam erradas

- **O botão de enviar voltou.** Enviar exigia Enter, e uma tecla não é
  descobrível: o texto que a anunciava some na primeira letra digitada, e para
  quem navega por toque não há tecla nenhuma. O campo virou multilinha, com
  Shift+Enter.
- **Criar um canal agora abre o canal criado**, e põe o cursor nele.
- **Os botões de enviar imagem do PERFIS tinham texto** e apareciam vazios. O
  defeito era do renderer: `arquivo` não tinha caminho nenhum para escrever o
  rótulo. Junto vieram a finalidade — o diálogo do sistema diz para quê —, o
  filtro de tipos e o teto.
- **O cartão de um MOD some da lista ao sair da sala de voz.** Ele sumia: duas
  das três ramificações da lista não passavam os cartões ao renderer.
- **O ESTILO afirmava que o produto recusava arredondamento e brilho**, logo
  depois de aplicá-los. A frase era verdadeira quando foi escrita.
- **O estado vazio da chamada mandava usar «COMPARTILHAR, aqui em cima»**, e o
  botão está embaixo à esquerda desde que saiu do cabeçalho. O mesmo comando
  ganhou uma porta ao lado da frase.
- **A tabela de atalhos dizia CTRL+V no Mac.** Agora ela diz a tecla desta
  plataforma.
- **A tela de entrada abria com `seeled`, QUIC/TLS e curva elíptica** antes de
  «entrar» e «hospedar». O propósito vem primeiro; os detalhes continuam
  inteiros, recolhidos.
- **Fora de servidor, a identidade mostrava um travessão** e falava em «neste
  servidor». A chave é do dispositivo; o apelido é por servidor.
- **A portaria escrevia `«»`** para quem bate sem escolher apelido.
- **Um pacote que este build não executa voltou a ter nome.** Ele aparecia como
  um hash de 64 caracteres com `api-too-old`, e agora diz qual dos dois lados
  está para trás e o que fazer.
- **«Instalado, e desligado» ficava na tela** enquanto você ligava o MOD logo
  abaixo. Cada linha diz o estado dela, derivado do que é verdade agora.
- **O nome de uma pessoa na lista é maior que o número do sinal dela.** Era o
  contrário: 26 px para a medida, corpo de dado para de quem ela é.
- **«TRANSMITINDO» aparecia em quem não estava em sala nenhuma.**
- **Os rótulos da interface subiram de 10 px para 12 px**, e os botões ganharam
  alvo medido.
- **A aparência abria com a medição de um contraste já corrigido**, e agora
  oferece o caminho até a acessibilidade do sistema, que é onde a decisão mora.
- **O medidor de microfone dizia «SEM SESSÃO DE ÁUDIO» com «Fale normalmente»
  logo abaixo.**
- **A explicação sobre o aparelho padrão estava escrita duas vezes**, inteira,
  uma para o microfone e outra para a saída.
- **O resumo de alcance da rede era lido por leitor de tela como uma frase só**
  — «outro.o roteador…» —, e a porta separou «conexão externa» de «quem pode
  entrar», que são decisões de camadas diferentes.
- **O laboratório dos MODs estava quebrado e não dizia.** Ele procurava um
  prelúdio que mudou de arquivo, recortava uma função por índice de string, e
  montava um Worker de navegador — o ambiente que a API 3 tirou. O teste dele
  exigia que a região **não** tivesse botão nem campo, coisa que era verdade na
  API 2, e continuava verde porque o laboratório nunca chegava a montar nada.

---

## Para quem escreve MOD

A API 4 está congelada em `api/v4.json`, e a decisão em
`docs/adr/0052-um-mod-ganha-superficies-proprias-e-pontos-de-integracao.md`.

**Seu MOD de API 3 continua rodando.** Você não precisa fazer nada. Se quiser as
superfícies, declare `api: 4` no `mod.json` — e saiba que um pacote 4 não roda
em SEELE anterior a esta versão.

O que a 4 acrescenta:

- **`SeeleUI.superficies`** — página, painel, diálogo e aviso, com `montar`,
  `classes`, `suja`, `fechar` e `descartar`;
- **`SeeleUI.contribuicoes`** — dez pontos semânticos na interface do SEELE,
  com handle revogável, nos modos `adicionar` e `substituir`. Um MOD revoga o
  que ele registrou, e a saída da sessão limpa o resto;
- **dezenove formas novas** — caixa, pilha, grade, rolagem, formulário, abas,
  texto longo, número, deslizante, marca, interruptor, cor, retrato, distintivo
  e link, entre outras;
- **estilos declarados** — cor, gradiente, borda, raio, sombra, tipografia,
  layout, transformação, recorte, transição e animação, com estados
  (`hover`, `focus-visible`, …) e consultas **de contêiner**.

A fronteira é **alcance, e não gosto**: um estilo não alcança fora da superfície
dele, não é texto de CSS, não busca bytes na rede e não sobrevive à saída. Está
escrito em `specs/07-estetica.md`.

`SeeleUI.capacidades()` diz o que a sua versão tem, para você degradar em vez de
falhar. E o que a sua versão **não** tem simplesmente não existe no objeto —
não é um método que falha com um código, é um `TypeError` na linha que o chama.

---

## O que esta versão **não** entrega

Dito aqui porque a auditoria pede que seja dito, e porque a seção «o que
funcionou» de qualquer entrega vale menos sem ela.

- **Parte das jornadas foi percorrida no aplicativo nativo, e parte não.**
  Funcionaram na janela, com os três MODs instalados: criar campanha e cena,
  rolar dados, gravar perfil, cancelar e confirmar o descarte de alterações, e
  sair do servidor com a interface dos MODs saindo junto.

  **Continuam sem observação:** segundo participante e autorizações entre
  pessoas, ficha de terceiro, upload e recorte de retrato e faixa, arraste de
  peças, mídia, Tab e Shift+Tab em todas as combinações, memória e CPU sob
  carga, Windows e Linux. Um Escape no editor com rascunho não fechou a janela
  numa tentativa; o botão FECHAR abriu a confirmação normalmente. Isso pede uma
  verificação de teclado própria, e nenhuma causa foi demonstrada.
- **`decorar` não existe nesta versão.** O plano previa três modos de
  contribuição; a 4 entrega dois. Registrar `decorar` é recusado com a razão,
  em vez de aceito sem efeito.
- **O custo não foi medido sob carga.** Sem MOD, com três ativos, com um editor
  aberto, com a MESA em arraste, com voz. Nada aqui deve ser lido como «o custo
  cabe».
- **Windows e Linux não foram percorridos.** O código compila para os três;
  nenhum fluxo foi observado fora do macOS.
- **Não houve teste com pessoas.** A reorganização da configuração é uma
  proposta apoiada em inspeção, e a auditoria é explícita em não aprovar por
  opinião estética.

O registro inteiro das decisões desta volta — incluindo as alternativas que
foram recusadas e por quê — está em
`docs/decisoes-da-auditoria-2026-09-20.md`.
