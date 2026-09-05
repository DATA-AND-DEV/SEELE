# ADR 0032 — Personalização de um servidor: o nome no fio, a cor numa placa, e nenhuma imagem por enquanto

**Estado:** aceito, construído em parte
**Data:** 2026-08-18 · **construído** entre 2026-08-19 e 2026-09-02

**O nome e o ícone existem. A cor não.** O corpo abaixo é o texto de 18/08 com o
vocabulário trazido para a língua do ADR 0035 — `Dogma` era `servidor`, `Cage`
era sala de voz, `Linha` era canal, `CASPER` era `PERSISTENCE`. Ele **continua
dizendo «imagem, não»**, e isso deixou de ser verdade: o adendo no fim registra
o que reverteu aquela recusa, com que medida, e as duas coisas que o desenho
construído fez diferente do que esta página projetou.

O que está de pé, conferido no código:

| | estado | onde |
|---|---|---|
| Nome, escrito pela janela | construído | `RenameServer` · `ServerRenamed` · `renomear_server` |
| Nome anunciado com o servidor no ar | construído | `crates/seele-server/tests/personalizacao.rs` |
| Ícone | **construído**, contra a recusa desta página | `SetServerIcon` · `ServerIconChanged` |
| Cor da placa | **não construída** | — |
| Sigla derivada do nome | construída | `siglaDoAlvo` · `tela-sessao.js` · `camada-servidores.js` |
| Coluna de preferência local de cor | não construída | — |

O literal `"Casa"` **não saiu** de `apps/seele-app/src/main.rs`: ele virou o nome
de partida de um servidor que agora pode ser renomeado, em vez do único nome
possível. É menos do que esta página pediu e resolve o defeito que ela relatou.

**E a cor deixou de estar bloqueada, sem que ninguém a construísse.** A seção
«Qual dos dois vem primeiro» condiciona a cor ao ADR 0031, com um argumento que
não era de conveniência: sem a trilha não existe superfície pequena o bastante
para uma cor escolhida por outra pessoa. **A trilha existe desde 22/08** — o
mesmo commit do nome — e a placa de 56 px com a sigla está desenhada. O 0031
continua proposto pela decisão dele, que é ter várias sessões; a dependência que
esta página declarou era da **superfície**, e essa chegou. Quem for construir a
cor não precisa mais esperar por aquele ADR.

Pedido do dono, nas palavras dele: **«Personalização do Dogma (nome, cor,
ícone, etc)»**. Este ADR divide as três, porque elas não são a mesma coisa: uma
já está construída pela metade, uma é a colisão que o ADR 0029 já enfrentou uma
vez, e a terceira é uma imagem de terceiro sendo desenhada na janela de alguém
que não pediu para vê-la.

Ele **compartilha uma pergunta** com o ADR 0031 — o que pertence a um servidor e o
que pertence a esta máquina — e a resposta está lá, escrita uma vez. O que este
acrescenta é o outro lado: o que um servidor pode dizer sobre si mesmo, e até onde
isso alcança dentro da janela de quem entra. A seção «Qual dos dois vem
primeiro», no fim, diz o que depende do quê.

## Contexto

Há um ADR que resolveu esta colisão para o caso de dentro de casa, e ele é para
ser lido antes deste: **o ADR 0029**, que decidiu que um MOD **declara pares
nome→valor e nunca uma folha**, que o produto **mede antes de aplicar**, que o
**papel é intransferível e o tom é negociável**, e — a parte que este ADR tem de
encarar — que **um MOD não acompanha um servidor**: a sala recomenda, e instalar
continua sendo um ato.

**A diferença que muda tudo é uma frase: a escolha é de outra pessoa.** Um MOD
você instala, numa tela sua, com as medidas na frente. A cor de um servidor chega
junto com o convite de alguém, sem tela, sem ato e sem leitura. Toda a defesa do
0029 repousa numa pessoa decidindo; aqui não há a pessoa, e é por isso que este
ADR é mais restritivo que aquele e não menos.

O que existe hoje, e que condiciona todo o desenho abaixo:

- **O nome já viaja e já é desenhado.** `ServerMessage::Session` carrega
  `server: String` (`crates/seele-proto/src/control.rs`), conferido contra
  `MAX_CLIENT_NAME_LEN` = 64; `seele_ffi::Snapshot` o repassa como `server`
  (`crates/seele-ffi/src/types.rs`); e a casca o escreve no cabeçalho. Esta
  metade está pronta.
- **A outra metade não existe: não há como nomear um servidor pela janela.**
  `ServerConfig::name` é campo de struct montada no `main.rs`
  (`crates/seele-server/src/lib.rs`), e o botão HOSPEDAR AQUI passa a string
  literal `"Casa"` — `apps/seele-app/src/main.rs`. Todo servidor hospedado pelo
  app do mundo se chama Casa.
- **Existe onde guardar.** A tabela `configuracao`
  (`crates/seele-server/src/persistence/schema.rs`) nasceu na migração 2 para o
  ADR 0021 e é descrita como «configuração do servidor que não cabe num arquivo,
  porque muda em tempo de execução e precisa sobreviver a reinício». O ADR 0027
  usou exatamente esse critério para o teto de anexos
  (`persistence/attachments.rs`), e ele vale igual aqui.
- **A palheta é congelada e as garantias dela são medidas.** ADR 0014;
  `apps/seele-app/tests/tokens.rs` recusa literal de cor em qualquer folha
  (`the_stylesheet_uses_no_colour_the_tokens_do_not_define`, `tokens.rs:69`) e
  refaz aritmética de contraste WCAG em Rust.
- **O vermelho é exclusivo de alerta e de queda**, e os guardas que o protegem
  não olham para cor nenhuma: eles perguntam se uma **regra nomeia** o token.
  São quatro em `apps/seele-app/tests/frontend.rs`. O quinto vivia em
  `crates/seele-tui/src/theme.rs` e saiu com a TUI (ADR 0039). O ADR 0029 mediu
  o resto e achou o número desconfortável: **`laranja-nerv` ×
  `vermelho-alerta` = ΔE76 19,00**, o par mais próximo entre as seis cores que
  carregam significado, por mais que o dobro.
  A palheta não se sustenta em distância cromática; sustenta-se nos guardas.
- **Nenhuma informação transmitida só por cor.** `specs/06-clientes-gui.md:143`.
  É por isso que a marca de bloco `█ ▒ ░` acompanha o sinal em toda palheta.
- **A CSP é restritiva**, e a linha inteira está em
  `apps/seele-app/tauri.conf.json:22`. Para este ADR importa uma diretiva:
  `img-src 'self' data:`. Uma imagem embutida como `data:` **é permitida hoje**,
  sem mexer em nada — o que significa que o freio contra imagem de terceiro não
  pode ser a CSP, porque a CSP não freia.
- **A prévia embutida de imagem não está construída**, e o ADR 0027 diz por quê,
  no alto dele: desenhar imagem exige «baixar os bytes e olhar os primeiros deles
  antes de escolher um decodificador», e enquanto essa conferência não existir
  **todo** anexo é um arquivo com nome e tamanho, sem prévia. Isto é de um anexo
  que alguém escolheu abrir.
- **O quadro de controle tem teto de 16 KiB.** `MAX_FRAME_LEN`
  (`crates/seele-proto/src/control.rs:49`), e o `Session` já carrega salas de voz,
  canais, papéis e permissões dentro dele.
- **O que esta máquina lembra de cada servidor já tem arquivo.**
  `crates/seele-core/src/conhecidos.rs`: uma linha por servidor, com endereço,
  apelido, última sala de voz e quando foi a última visita. O cabeçalho do módulo diz
  que ele é a conveniência e que «pode ser apagado sem consequência», ao
  contrário do `pins`.
- **Glifo foi recusado no esquema de MODs**, e o motivo vale aqui:
  `ui/glifos.js` é geometria SVG dimensionada contra a Plex Mono embarcada, e no
  terminal os mesmos papéis são caracteres de verdade — «uma capacidade `glifo`
  teria de querer dizer as duas coisas, e não quer».

## Decisão

**O nome é do servidor e viaja no fio, como já viaja. A cor é uma escolha dentro de
uma lista fechada de nomes, e ela pinta uma placa de 56 px e mais nada. Imagem,
não — ainda, e com a saída nomeada.**

### O nome é o caso fácil, e vale dizer que é fácil

Metade está construída: o campo existe no protocolo, existe no `Snapshot`,
existe na tela. O que falta é **quem escreve**, e não há decisão de arquitetura
nenhuma escondida aí.

- **Onde mora:** na tabela `configuracao`, pelo critério que a própria migração 2
  escreveu e que o ADR 0027 já reusou — muda em tempo de execução e precisa
  sobreviver a reinício. Não num arquivo TOML que continua não existindo.
- **Quem escreve:** quem hospeda, na mesma tela em que o ADR 0030 pôs a portaria,
  e pelo mesmo caminho — acessor no `Hospedagem`, falando direto com o PERSISTENCE da
  máquina, **sem verbo novo de protocolo**. O argumento do 0030 vale sem
  emenda: nomear o servidor é decisão de quem tem o arquivo dele na mão, e expor
  isso à internet seria superfície nova para uma decisão que não é remota.
- **O literal `"Casa"` sai**, e é o conserto mais barato desta página. Enquanto
  não houver nome escolhido, o padrão continua sendo um nosso — um padrão é
  melhor que um campo vazio —, mas ele deixa de ser a única coisa possível.
- **Renomear com o servidor no ar precisa de um aviso**, ou quem está dentro
  continua lendo o nome velho até reconectar. É um evento a mais, no fim da
  enumeração, como manda a compatibilidade do `postcard` — a mesma regra que o
  ADR 0030 seguiu com `AdmissionPending` e `AdmissionDenied`. **Se o custo desse
  aviso não se pagar, a alternativa honesta é o nome só valer na próxima
  conexão**, e dizer isso na tela; o que não pode acontecer é a tela de quem
  renomeou mostrar o nome novo e a de todo mundo mostrar o velho.
- **O teto é o que já existe:** 64 bytes, conferido no `control.rs`, e a mesma
  validação de nome em branco que a sessão já faz
  (`a_blank_name_never_leaves_the_server_inside_a_session`).

Não há mais nada. Registrar isto como fácil é útil: um ADR que tratasse as três
personalizações como um problema só empurraria a mais barata para trás da mais
cara.

### Um servidor não repinta a janela de quem entra

**Não.** É a pergunta central do pedido e a resposta é a mesma do ADR 0029 —
com um agravante que a torna mais firme aqui, não menos.

O 0029 recusou que um MOD acompanhasse um servidor por três motivos, e os três
sobrevivem inteiros a esta página:

- **Toda defesa daquele desenho repousa numa pessoa decidindo.** A conferência
  mede, a tela mostra, o arquivo é legível, a assinatura confere. Aplicar por
  entrar numa sala remove a pessoa de todo o desenho e deixa só a conferência de
  pé — «e a conferência é a metade fraca, porque ela sabe medir contraste e não
  sabe medir intenção».
- **Um servidor não é necessariamente entre amigos.** O ADR 0021 deixa um servidor sem
  convite e sem senha aberto por padrão, de propósito.
- **A tela mudaria enquanto a pessoa está nela**, que é o movimento menos
  diagnóstico que este produto poderia ter.

E o agravante: **aqui não há nem o ato de instalar em que pendurar o
consentimento.** No 0029 a recomendação vinha pelo `seele://`, e a pessoa ia a
uma tela, lia o arquivo inteiro, via a diferença pintada lado a lado e as
medidas, e aí instalava. Uma cor de servidor não tem tela, não tem arquivo, não tem
diferença para ler: ela chega no `Session` junto com a lista de salas de voz. Se a
regra do 0029 já era «recomendação é dado; aplicação é consentimento», então uma
personalização que repintasse a janela seria aplicação **sem sequer a
recomendação**.

Então a regra é a mesma, e mais estreita: **um servidor nunca dá valor a um token,
nunca alcança `:root`, nunca aparece numa folha e nunca toca um seletor.** Os
quatro guardas do vermelho, o teste de literal de cor, o teste de contraste da
varredura e a ordem de `acessibilidade.css` continuam valendo sem que nenhum
deles precise ser reescrito — pela mesma razão mecânica do 0029: o que muda não é
folha e não tem seletor.

### O que ele ganha: uma placa, e o tamanho dela é a decisão

O que um servidor personaliza é **o botão dele na trilha** — 56 px no comp v3, o
elemento que o ADR 0031 tira do desabilitado. Um elemento, um `<button>`, e nada
mais na janela.

Isto não é um consolo: é o desenho inteiro. Uma cor que alcança 56 px não pode
esconder uma queda de enlace, não pode repintar uma faixa de veredito, não pode
mudar a hierarquia da tela às três da manhã. **A pergunta «um servidor pode repintar
a janela alheia» vira respondível justamente porque existe um lugar pequeno o
bastante para dizer sim.** Sem a trilha do 0031, o único lugar onde uma cor de
servidor caberia seria a janela inteira, e aí a resposta teria de ser não e ponto —
que é o que a seção «Qual dos dois vem primeiro» conclui.

**Como é aplicada:** por CSSOM, num nó, e não no `documentElement`. É o mesmo
mecanismo do 0029 — `style.setProperty`, que a CSP não cobre porque não é folha e
não é `<style>` — com o alcance reduzido de uma raiz para um botão. Nenhum
arquivo entra em `ui/`, então
`the_page_loads_only_files_that_are_shipped` continua valendo.

### A cor: um nome de uma lista fechada, e nunca um valor

Aqui este ADR **aperta** o 0029 em um grau, e o grau é exato.

| | quem escolhe | o que declara | quem mede | quem lê a medida |
|---|---|---|---|---|
| MOD (0029) | você | par nome→valor | o produto, na instalação | você, antes de consentir |
| servidor (este) | outra pessoa | **um nome, e só** | ninguém, porque não há o que medir | — |

Um MOD pode declarar valor porque quem instala escolheu, e porque há uma tela em
que a medida é lida por alguém antes de valer. Um servidor declara **nome**, de uma
lista que o produto publica, porque a única forma de «esta cor já foi medida»
sobreviver sem leitor é a cor já ser nossa.

**A lista, e por que ela é curta.** As candidatas são as cores que carregam
significado e já têm contraste anotado: `laranja-nerv`, `fosforo`, `padrao-azul`
e `osso`. São **quatro**, e o vermelho não está entre elas.

**O vermelho fica de fora, e não é negociável.** Não porque seja feio numa
placa, mas porque `vermelho-alerta` é a única cor com que qualquer superfície de
alerta ou de queda se pinta, e uma placa vermelha de identidade encostaria na
placa vermelha de estado a ΔE76 19,00 do laranja e a zero de si mesma. O papel é
intransferível, e o 0029 já escreveu essa frase.

**Quatro é pouco, e isso é dito em vez de escondido.** Cinco servidores e duas placas
repetem a cor. O que torna isso um custo estético e não funcional é uma regra
que já existe: **a placa carrega a sigla**, que é texto, e
`specs/06-clientes-gui.md:143` proíbe informação transmitida só por cor. A cor
nunca é o que distingue um servidor; ela é o que torna a distinção rápida. Acabar a
lista custa velocidade de leitura, e não a leitura.

**Estado vence identidade na placa.** Uma placa cujo servidor caiu **perde a cor do
servidor** e assume a forma de queda, que é vermelha e é do produto. Não há um
instante em que uma cor de identidade e uma cor de estado dividam a mesma coluna
disputando o significado do vermelho — a identidade se cala enquanto o estado é
anormal. É a mesma disciplina dos quatro guardas, aplicada onde eles não
alcançam sozinhos.

### E a pessoa pode discordar, no arquivo que já existe para isso

A metade que devolve a decisão a quem olha: **a cor que um servidor escolhe é uma
proposta, e esta máquina lembra o que a pessoa preferiu.** Uma coluna a mais no
`conhecidos`, que é exatamente o arquivo do que este cliente sabe sobre cada
servidor onde já esteve, e que o próprio módulo descreve como a conveniência que
«pode ser apagado sem consequência».

É o «recomendar, e instalar continua sendo um ato» do ADR 0029, na menor escala
em que ele cabe: o servidor sugere, a máquina guarda a escolha de quem está na
frente da tela, e a preferência local vence sempre. E é barato porque não há
medida nenhuma a refazer — as duas cores em disputa são nossas.

O custo é o que o ADR 0017 já avisou sobre formato de arquivo em disco:
`conhecidos` ganha uma coluna, e uma linha de quatro campos passa a ter cinco. É
o arquivo mais fácil de migrar que este projeto tem — linhas antigas leem com a
coluna ausente, e o pior caso é uma placa com a cor padrão.

### O ícone: não, em v1, e a saída está escrita

**Um servidor não manda imagem.** É a recusa mais dura desta página, e ela tem três
razões que se somam:

**1. A prévia de um anexo que alguém pediu ainda não existe.** O ADR 0027
registra, no alto dele, que desenhar imagem embutida exige olhar os primeiros
bytes antes de escolher um decodificador, e que enquanto isso não existir todo
anexo é nome e tamanho. Uma imagem que um servidor empurra é **o mesmo gênero de
problema com a agravante de ninguém ter pedido para vê-la**. Construir o caso
não pedido antes do caso pedido seria construir a ordem ao contrário.

**2. A CSP não freia isto, então dizer que ela freia seria falso.**
`img-src 'self' data:` já aceita imagem embutida. O freio tem de ser uma
decisão escrita, e é esta — o que é uma diferença importante em relação ao 0029,
onde a CSP fazia metade do trabalho de graça.

**3. O quadro do aperto de mão não tem essa folga.** `MAX_FRAME_LEN` é 16 KiB e
o `Session` já carrega salas de voz, canais, papéis e permissões. Um ícone dentro dele
disputaria espaço com o conteúdo da sala — e um servidor grande que deixasse de
entrar por causa de um ícone seria o pior defeito possível: uma decoração
custando a conexão.

**O que existe no lugar, e existe no dia um:** a **sigla**, derivada do nome do
servidor por nós, deterministicamente. O comp v3 já a usa — `TÓQUIO-3` vira `T3`
porque a coluna tem 60 px —, e o `index.html` já registra que ela é «abreviação
de desenho e não um dado». Toda placa tem ícone desde o primeiro dia, e nenhum
byte dele vem do fio.

**A saída, para quando ela for querida**, escrita aqui para não ser inventada com
pressa depois:

- **Não no aperto de mão.** Buscado sob demanda e endereçado por conteúdo, como
  o ADR 0027 já faz com anexo — o `Session` carrega no máximo um hash, e a
  imagem vem por fluxo próprio, uma vez, guardada por máquina.
- **Teto pequeno e declarado:** um quadrado, **128×128 e 16 KiB**, recusado por
  razão enumerada acima disso — números escolhidos para caber numa placa de
  56 px em tela de alta densidade e para que o pior caso de cache por servidor seja
  conhecido, na mesma disciplina do teto do 0027.
- **Uma lista curta de tipos, e só quando os bytes concordam com a alegação** —
  a conferência do 0027, a mesma, e **nunca antes dela existir**.
- **Sem animação.** `specs/07` diz que movimento é diagnóstico, com uma exceção
  nomeada que já foi gasta pela varredura (ADR 0014, revisão em M5). Um GIF
  piscando na trilha seria a segunda, e uma exceção nomeada não abre precedente
  para a próxima.
- **Nunca fora da placa.** Nem no cabeçalho, nem na tela de entrada, nem numa
  notificação.

## Qual dos dois vem primeiro

O dono pediu para não propor construir os dois de uma vez porque «são
parecidos», e para dizer qual é pré-requisito de qual. São três respostas, e não
uma:

**O nome não depende de nada.** É um campo em `configuracao`, um acessor no
`Hospedagem`, uma tela ao lado da portaria e um literal que sai do `main.rs`.
Ele pode ser construído hoje, sozinho, e melhora o produto de hoje: todo servidor
hospedado pelo app se chama Casa, e é assim que ele aparece na tela de quem
entra.

**A cor depende do ADR 0031, e a dependência não é de conveniência.** Sem a
trilha, não existe superfície pequena o bastante para uma cor escolhida por
outra pessoa. A pergunta «um servidor pode repintar a janela alheia» tem duas
respostas conforme o que existe na tela: com uma sessão por vez, a única
resposta é não, porque a janela inteira é aquele servidor; com a trilha do 0031, a
resposta é «pode pintar a placa dele», que é uma resposta útil. **O 0031 não
torna a cor mais fácil de construir: ele torna a cor possível de aceitar.**

**O ícone não depende de nenhum dos dois** — depende da conferência de bytes que
o ADR 0027 deixou anotada como não construída, e é por isso que ele está fora de
v1 aqui.

Então a ordem é: **nome agora, cor depois do 0031, ícone depois do 0027.** E os
três não devem ser construídos juntos, porque juntá-los esconde o mais barato
atrás do mais caro — que é exatamente o que aconteceria se «personalização»
virasse um trabalho só.

## Alternativas consideradas

1. **O servidor declara valores hexadecimais, e o produto mede na conexão como o
   0029 mede na instalação.** É a leitura mais literal do 0029 e a que mais se
   parece com o que foi pedido. Recusada por um motivo estrutural: **no 0029 a
   medida tem leitor.** Uma pessoa lê o número, vê a cor recusada, e decide. Uma
   medida na conexão não tem leitor nem decisão — o único desfecho possível é cair
   para a nossa cor em silêncio, que é a «instalação parcial silenciosa» que o
   0029 recusa nominalmente. Uma conferência cujo único resultado é um silêncio é
   uma conferência que não vale o código que custa.

2. **O servidor acompanha um MOD inteiro.** Já recusada pelo ADR 0029, na seção «Um
   MOD não acompanha um servidor», com os motivos inteiros. Este ADR não a reabre;
   ele apenas registra que a versão fraca — um nome de cor para uma placa — não é
   a mesma coisa, porque não alcança token, não alcança seletor, e não alcança
   nada além de 56 px.

3. **Nada: um servidor é o nome dele.** É o que existe, é de graça e funciona.
   Recusada por uma razão que só aparece depois do ADR 0031: uma trilha de quatro
   placas iguais com duas letras cada é uma coluna que ninguém varre com o olho, e
   a trilha existe justamente para ser varrida com o olho. Antes do 0031 esta
   alternativa era a certa, e é por isso que ela foi a certa até hoje.

4. **A pessoa pinta as placas, e o servidor não opina.** Segura, e quase adotada. O
   que ela perde é o único caso que o pedido descreve — quem hospeda quer que o
   servidor dele **seja** de um jeito, e a placa é onde isso apareceria para os
   outros. **Ela não foi recusada: virou a segunda metade da decisão**, como
   preferência local que vence a proposta do servidor. Adotar só ela seria responder
   metade do pedido e chamar de resposta.

5. **Ícone como `data:` no `Session`.** Custa zero fluxo novo e a CSP já aceita.
   Recusada pelas três razões da seção do ícone, e a que decide é a terceira: o
   quadro do aperto de mão tem 16 KiB e já carrega a sala inteira.

## Consequências

- **O literal `"Casa"` sai do `main.rs`**, e todo servidor hospedado pelo botão
  deixa de ter o mesmo nome.
- **A tabela `configuracao` ganha o segundo consumidor**, depois do teto de
  anexos do ADR 0027. O critério com que ela foi criada continua sendo o critério.
- **Um evento novo no fim da enumeração**, se renomear com o servidor no ar for
  construído — com a mesma regra de compatibilidade do `postcard` que o 0030 já
  seguiu. Um cliente uma versão atrás recusa o quadro em vez de ler outra coisa.
- **Um campo novo no `Session`**, também no fim, para o nome de cor. É um
  identificador curto de uma lista fechada, e um nome desconhecido é **ignorado**
  e não recusado — a mesma regra que o ADR 0006 usa para parâmetro desconhecido
  no `seele://`, e que permitiu ao `alt=` ser compatível com clientes velhos. Um
  servidor que escolhe uma cor que este cliente não conhece desenha a placa padrão.
- **`conhecidos` ganha uma coluna**, e passa a guardar uma preferência além de
  uma conveniência. Continua sendo o arquivo que pode ser apagado sem
  consequência.
- **A CSP não muda, o conjunto de arquivos de `ui/` não muda, `tokens.css` não
  muda e os quatro guardas do vermelho não mudam.** Se alguma das quatro
  precisasse mudar, o desenho estaria errado — é o mesmo critério que o ADR 0029
  escreveu para si.
- **Nenhuma dependência nova**, e nenhuma exceção nova no `deny.toml`.
- **A aritmética de contraste não ganha consumidor aqui.** O ADR 0029 a promove a
  código de produção porque um MOD declara valores; este ADR não precisa dela,
  porque a lista fechada já foi medida. Se um dia a alternativa 1 for adotada,
  esta consequência se inverte.

## O que fica sem saída

**Quatro cores é pouco, e não há mais.** A palheta congelada tem catorze cores,
seis carregam significado, uma é o vermelho e não sai do lugar, e as de
superfície não servem para distinguir nada porque a função delas é não aparecer.
Não há versão desta decisão em que caibam doze servidores com doze cores distintas
sem abrir a palheta, e abrir a palheta é a alternativa 1.

**A sigla é derivada, e derivação erra.** Dois servidores chamados «Casa»
e «Torre 3» viram `T3` os dois, e nenhuma regra automática conserta isso sem
inventar uma exceção por caso. É o mesmo gênero do `Rafae1` ao lado de `Rafael`
que o ADR 0030 nomeia e não resolve: contra parecido não há verificação, só o
hábito de ler o nome inteiro, que está escrito a dois centímetros dali.

**A cor não diz nada e por isso não protege nada.** Uma pessoa que aprende a
achar um servidor pela placa laranja acha o próximo servidor laranja do mesmo jeito.
Um servidor hostil que escolhe a cor de um servidor conhecido é uma imitação barata, e
a única defesa é a mesma de sempre — a impressão digital, que é a única coisa na
tela que outra pessoa não pode escolher (ADR 0003, e o cartão do ADR 0030).

**Um nome é escolhido por outra pessoa, e nomes ofendem.** 64 bytes escritos por
quem hospeda aparecem no cabeçalho de quem entra e na trilha, e não há moderação
disso nem deve haver: quem entra num servidor escolheu entrar. O que o produto pode
fazer é o que já faz com o apelido — teto de tamanho e recusa de branco — e o que
ele não pode é julgar texto.

## Custo de reverter

**Baixo para o nome.** Um campo na `configuracao` que ninguém lê mais, e o
literal de volta. Nada no protocolo se não houver o evento de renomeação; com
ele, uma variante que servidores velhos nunca emitem.

**Baixo para a cor, e a propriedade não é acidente:** apagar a coluna do
`conhecidos` e ignorar o campo do `Session` devolve a trilha em cor única,
exatamente, sempre — porque a cor nunca escreveu num token, nunca entrou numa
folha e nunca teve estado além de uma linha em disco. O estado sem cor não é um
estado de recuperação: é a placa padrão, com um campo a menos.

**Nenhum, para o ícone**, porque ele não é construído. É a única parte deste ADR
cujo custo de reverter é zero, e é a única que não entrega nada.

## Adendo — a imagem veio, e a recusa desta página caiu por medida (2026-08-22)

A seção «O ícone: não, em v1» tinha três razões. **Duas continuam verdadeiras e
uma foi respondida por desenho, não por pressa.** O que ficou construído está em
`SetServerIcon` · `ServerIconChanged`, com o formulário em
`escolher_icone_do_server` e a suíte em
`crates/seele-server/tests/personalizacao.rs`.

**A terceira razão — o quadro do aperto de mão — era a que decidia, e é a que
foi resolvida.** Esta página recusou o ícone porque `Session` já carrega as salas
de voz, os canais, os papéis e as permissões dentro de `MAX_FRAME_LEN`, e uma
decoração não pode custar a conexão. A saída construída é exatamente a que a
seção «A saída, para quando ela for querida» nomeou: **quadro próprio, nunca
dentro do `Session`**, mandado uma vez por sessão logo depois dele quando há
imagem, e silêncio quando não há. O comentário de `ServerIconChanged` cita esta
página pela razão certa.

**Duas medidas ficaram diferentes do que esta página escreveu, e as duas são mais
apertadas:**

| | esta página, em 18/08 | construído |
|---|---|---|
| teto de bytes | 16 KiB | **8 KiB** (`MAX_SERVER_ICON_LEN`) |
| lado máximo | 128 px | **256 px** (`MAX_SERVER_ICON_SIDE`) |
| formatos | «uma lista curta de tipos» | **PNG e só PNG** |

O teto caiu pela metade por uma propriedade que esta página queria e não sabia
comprar: a 8 KiB a mensagem fica na metade do teto do quadro, e **um ícone grande
demais é recusado como ícone, e nunca como um quadro que ninguém sabe
explicar**. O segundo número existe porque os bytes não medem a imagem — um PNG
de 8 KiB pode declarar 20 000 × 20 000 e custar 1,6 GB a quem o decodifica —, e
é a conferência do cabeçalho que impede uma decoração de virar um estouro de
memória em toda máquina que a desenha.

**«Só PNG» é o que substitui a conferência de bytes que o ADR 0027 deixou
anotada como não construída**, e é mais barato que ela: o tipo é fixado pela
mensagem em vez de declarado ao lado dos bytes, então **não há alegação para o
conteúdo contradizer**. GIF e SVG ficam recusados por construção — o primeiro
animaria numa placa, o segundo é um documento com script e rede dentro.

**A razão 1 continua de pé**: a prévia embutida de anexo continua não
construída, e este adendo não a antecipa. A razão 2 também: a CSP não freia, e
o freio continua sendo decisão escrita.

**O que a imagem ainda custa, dito em vez de descoberto depois:** os bytes
cruzam uma vez por sessão em vez de uma por máquina. Endereçar a imagem pelo
hash do conteúdo é o desenho mais barato e precisa de um fluxo próprio; a 8 KiB
a economia é de 8 KiB por reconexão, e ainda não paga um segundo caminho de
transferência. E trocar o ícone escreve uma linha e manda para toda sessão
conectada: a ~50 pessoas, uma troca custa ≈ 400 KiB de uma subida doméstica — o
que é um soluço, e é o motivo de haver um número aqui em vez de um encolher de
ombros.

## Adendo — o nome passou pelo fio, e a razão contradiz esta página (2026-08-22)

Esta página decidiu que o nome seria escrito por **acessor no `Hospedagem`, sem
verbo novo de protocolo**, com o argumento do ADR 0030: nomear um servidor é
decisão de quem tem o arquivo dele na mão.

**O construído tem `RenameServer` no protocolo**, e o comentário em `control.rs`
diz por quê, sem contornar o argumento: ele é sólido e **responde outra
pergunta**. Ele cobre o app que hospeda *neste processo*, e deixa um servidor
rodando por `seeled` numa VPS sem forma nenhuma de ser nomeado por quem o
administra.

O que mantém o caminho pelo fio tão estreito quanto o local é a permissão:
`RenameServer` e `SetServerIcon` exigem `Permission::AdministerServer` — e
**não** o `ManageVoiceRooms` dos quatro verbos de sala. A migração 1 semeia
`AdministerServer` só no Comandante, e `Permissions::seat_the_arrival` dá o
Comandante a quem conectar primeiro no próprio servidor: ele alcança exatamente
quem apertou o botão, mais quem essa pessoa promoveu de propósito.

Os dois caminhos escrevem a mesma linha. O ADR 0030 continua valendo onde ele
foi escrito — a portaria, que é decisão sobre gente e não sobre o nome de uma
casa, continua sem verbo de protocolo nenhum.
