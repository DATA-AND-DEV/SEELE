# 0030 — Quem bate à porta: TOFU aplicado a gente, e a portaria de quem hospeda

Status: aceito

**Construído em 2026-08-18**, em `crates/seele-server/src/portaria.rs`, que abre
citando este ADR. O estado ficou `proposto` por duas semanas depois de o código
existir; a data acima é a do commit `0f68dc0`, não a de hoje.

O que está de pé, conferido no código e não suposto: as três camadas conjuntivas,
o pedido durável em SQLite (`portaria::bater`, `decidir`, `revogar`, `pedidos`),
as duas razões `AdmissionPending` (índice 12) e `AdmissionDenied` (índice 13)
apensadas ao fim de `DisconnectReason`, a portaria ligada por semente no botão
HOSPEDAR AQUI (`portaria::semear_ligada`) e desligada por padrão no `seeled`, e a
fila na janela em `apps/seele-app/ui/camada-portaria.js`.

O que continua **não** construído, como o próprio ADR previu: a notificação do
sistema quando a janela não está em foco, e a limpeza de pedidos antigos.

**Vocabulário:** este texto foi escrito antes do ADR 0035 e falava `Dogma`,
`piloto` e `CASPER`. Foi trazido para a língua de hoje — servidor, pessoa,
PERSISTENCE — porque descreve o sistema que está no ar. O pedido do dono citado
no contexto fica **verbatim**: é testemunho, não descrição.

Contexto: o ADR 0021 deu ao servidor dois porteiros — convite de uso único e senha —
e o servidor sabe operar os dois: `admissao::criar_convite` e
`admissao::definir_senha`. **O app não expõe nenhum dos dois.** O único caminho
até eles é `seeled convite` e `seeled senha`, no terminal.

O público do botão HOSPEDAR AQUI é exatamente quem não abre terminal. Então, na
prática, todo servidor hospedado pelo app é um servidor aberto, e **não há nada em
lugar nenhum da janela que o feche** — nem para fechá-lo, nem para dizer que ele
está aberto. O aviso que o `seeled` dá em voz alta ao subir sem porteiro
(`admissao.rs:27`) vai para um log que ninguém que apertou aquele botão vai ler.

O pedido do dono foi «uma tela de permissão para usuários não registrados nos
dogmas, assim não entra qualquer um». Este ADR **estende** o 0021 — não o emenda.
Nada que o 0021 decidiu deixa de valer: os dois mecanismos continuam sendo o que
são, na ordem em que são, e o padrão aberto do `seeled` continua de pé pelo motivo
que ele deu. O que se acrescenta é uma terceira camada, de outra natureza, e o
caminho até a janela para as três.

## Decisão

**Uma portaria: TOFU aplicado a gente.** É o mesmo mecanismo que o ADR 0003 já
usa para servidores, virado do avesso. Lá, quem entra fixa a chave de quem
hospeda no primeiro contato. Aqui, quem hospeda decide sobre a chave de quem
entra no primeiro contato — e, decidido uma vez, não se pergunta de novo.

Isto só é possível porque o SEELE já tem identidade durável por pessoa: chave
Ed25519 em disco (ADR 0004), apelido preso à chave (ADR 0017). «É a primeira vez
que esta pessoa aparece?» é uma pergunta que este produto já sabe responder, sem
cadastro, sem conta, sem senha de usuário.

### As três camadas, e como convivem

Elas são **conjuntivas**: passar por uma não dispensa as outras. Um segredo abre
o portão da rua; a portaria decide se a porta da casa abre. Elas checam coisas
diferentes, em momentos diferentes, e as duas ordens importam.

| | o que checa | quando | o que a recusa custa ao servidor |
|---|---|---|---|
| Senha / convite (0021) | um segredo | no `Hello`, **antes** da assinatura | nada — nem uma verificação de assinatura |
| Portaria (este) | uma chave **provada** | depois do desafio-resposta | uma verificação de assinatura |
| Banimento | uma conta | depois da chave virar conta | uma consulta |

A portaria vem **depois** do desafio-resposta, e este é o ponto do desenho.
Fixar uma impressão digital que ninguém provou não é TOFU, é fixar um palpite:
qualquer um poderia encher a fila de pedidos com chaves alheias, e quem hospeda
aprovaria uma pessoa e admitiria outra. Só faz sentido decidir sobre uma
identidade depois que ela foi demonstrada.

O preço disso é que uma chave desconhecida custa uma verificação de assinatura,
que é justamente o que o 0021 recusou pagar na camada dele. Está certo que
custe: quem chega à portaria já passou pelo segredo, ou não havia segredo a
passar. E quem não passou por nenhum dos dois é contido antes, pelos dois baldes
do ADR 0025 — o balde por endereço, que existe desde antes do `Hello`, é o que
impede que gerar chaves novas vire uma fila de pedidos infinita. As três camadas
mais o balde compõem: cada uma cobre o flanco que a anterior deliberadamente
deixou aberto.

**O convite passa a ser gasto por quem entra, não por quem bate.** Isto é uma
correção dentro do 0021, encontrada ao construir isto e não antes: o convite era
consumido na camada do segredo, então um handshake que morresse depois dela —
assinatura ruim, pessoa banida, a rede caindo entre dois quadros — queimava o
convite de alguém que nunca entrou. Era raro o bastante para nunca ter aparecido.
Com portaria deixa de ser raro e passa a ser **o caso normal**: uma batida
pendente é o caminho projetado, a pessoa é mandada tentar de novo, e o convite
que ela tinha já não valia nada — aprovada ou não, ficava para fora para sempre.

A conferência e o gasto viraram duas metades. A proteção contra dois clientes
com o mesmo convite no mesmo instante não estava na conferência e sim no
`UPDATE ... WHERE usado_em IS NULL`, que continua sendo uma operação só: os dois
conferem com sucesso, e só um vê linha alterada.

**Um convite conferido não aprova ninguém.** Ele aparece no pedido, ao lado da
observação que quem hospeda escreveu ao gerá-lo — `criar_convite(persistence,
observacao)` já guarda esse campo e nada o lia. «Chegou com o convite *para o
Rafael*» é a melhor prova que existe do outro lado, e ainda assim é prova, não
decisão: um link se encaminha. Quem decide continua sendo quem hospeda.

### A recusa não é uniforme aqui, e isto é deliberado

O 0021 exige falha uniforme e tem razão: na camada do segredo, distinguir «senha
errada» de «convite gasto» conta a quem está adivinhando qual palpite chegou
mais perto. **Na portaria não há palpite.** Quem chegou até aqui provou uma chave
que tem em mãos, e a resposta é sobre ele mesmo, não sobre um segredo que ele
tentou acertar. Não há oráculo para proteger. Duas razões novas, apensadas ao fim
de `DisconnectReason`:

- **`AdmissionPending`** — o pedido chegou e ainda não foi decidido.
- **`AdmissionDenied`** — quem hospeda decidiu que não.

Que o servidor tenha portaria não é segredo que valha guardar: quem bate precisa
saber que há alguém para decidir, ou vai embora achando que o endereço está
errado.

### Ninguém espera, e é assim que a espera deixa de ser sem fim

A conexão **não fica pendurada**. Um pedido pendente cai na hora, com
`AdmissionPending`, e a frase diz o que aconteceu: o pedido ficou guardado, e dá
para tentar de novo quando quiser.

Isto dissolve a pior das três respostas em vez de tratá-la. «Ninguém atendeu» só
existe como estado se alguma coisa estiver esperando; como nada espera, o que
sobra é um pedido de pé, que quem hospeda concede quando olhar. A pessoa do outro
lado nunca fica olhando para uma barra que não anda, e o servidor nunca segura
recurso por alguém que ainda não entrou — que é o mesmo motivo pelo qual o 0021
não quis gastar assinatura com quem nem devia estar batendo.

### E se ninguém estiver olhando

**O pedido é uma linha em SQLite, não um aviso na tela.** Ele sobrevive à janela
minimizada, ao app fechado e à máquina reiniciada. Quem hospeda vê a fila quando
abrir a portaria, com a hora de cada pedido; conceder três dias depois vale igual,
e a pessoa entra na próxima vez que tentar.

Um servidor que só admite enquanto alguém olha a tela seria um servidor que recusa por
omissão. Este recusa nada por omissão: ele adia, e o adiamento é durável. O que
falta é o toque no ombro — uma notificação do sistema quando a janela não está em
foco. Fica de fora deste ADR e vai para as pendências: é melhoria de latência da
decisão, não a diferença entre decidir e não decidir.

### O que quem hospeda vê, e o que ele não deve confundir com identidade

A `NOTAS-DE-RELEASE` deste produto separa «o arquivo chegou inteiro» de «este
arquivo é bom». O cartão de um pedido faz o mesmo corte, e a ordem visual carrega
o corte:

1. **A impressão digital**, em primeiro lugar e por extenso — SHA-256 da chave
   pública, o mesmo formato que o ADR 0003 mostra no primeiro contato com um
   servidor. É a identidade. É o que se confere por outro canal, e é a única
   coisa nesse cartão que outra pessoa não pode escolher.
2. **O apelido pedido**, abaixo, entre aspas e apresentado como afirmação: *diz
   chamar-se «Rafael»*. Nunca como título do cartão. Título é do que a pessoa é,
   e ela ainda não é nada aqui.
3. **Com que segredo chegou**, e a observação do convite quando houve um.
4. **Quando bateu.**

O ADR 0017 já impede o ataque óbvio: pedir um apelido que é de outra chave é
recusado, e continua sendo. O que ele não impede — e nenhum código impede — é o
parecido: `Rafae1` ao lado de `Rafael`. Contra isso não há verificação, só o
hábito de ler a linha de cima. Por isso a linha de cima é a de cima.

### Desfazer

Aprovar grava uma linha. **Revogar apaga a mesma linha**, e a pessoa volta a ser
desconhecida: na próxima vez que bater, pergunta-se de novo.

**Revogar não é banir, e as duas não se encostam.** Revogar diz «pergunte-me
outra vez»; banir diz «nunca». Revogar também **não derruba quem está dentro** —
vale para a próxima batida. Derrubar já tem verbo, chama-se expulsar, e fazer um
ato brando ter consequência violenta é como uma interface ensina a não apertar
nada.

Sobre o buraco já registrado — `Permissions::ban` existe e `unban` não tem verbo de
protocolo: **este desenho não o piora, e mostra a saída.** Ele não piora porque
não acrescenta verbo nenhum de protocolo: a portaria inteira é estado local da
máquina que hospeda, decidido por quem tem o arquivo do servidor na mão — que é
exatamente a autoridade que a frase de confirmação do banimento já nomeia como
a única que o desfaz. E mostra a saída porque estabelece a forma: uma decisão
que se desfaz é uma linha que se apaga, e `Permissions::unban` já é literalmente
`DELETE FROM bans`. Falta a ele só o caminho até a janela. Não é consertado aqui
de passagem; fica anotado.

### Onde os comandos vivem: na casa, não no fio

Toda moderação deste app hoje viaja pelo fio, como cliente, mesmo quando o
próprio app hospeda. A portaria **não**: ela fala direto com o PERSISTENCE do servidor
embutido, por acessores novos em `Hospedagem`.

Três motivos. **Fechar a porta não pode depender de estar dentro** — exigir uma
sessão autenticada para configurar a admissão faz a defesa depender do canal que
ela defende, e obriga quem hospeda a ter entrado enquanto ainda estava aberto.
**A porta se fecha antes do primeiro pacote**, no mesmo gesto de hospedar.
E **nenhum verbo novo de protocolo** significa nenhuma superfície nova exposta à
internet para uma decisão que é, por definição, de quem está na máquina.

O custo, que é real: isto não administra o servidor de outra pessoa. Um Comandante
remoto continua sem fechar a porta da casa alheia — que é onde o 0021 já tinha
deixado a administração de verdade (alternativa 3), e continua lá.

### O padrão fica aberto no `seeled`, e fechado no botão

O 0021 mantém o padrão aberto porque é o que faz o teste em rede local funcionar
sem cerimônia. **Continua valendo, e não se mexe nele.** O `seeled` sobe aberto e
avisa, como sempre.

**O que muda é só o botão HOSPEDAR AQUI: ele sobe com a portaria ligada.** O
argumento é que os dois têm públicos diferentes. Quem digita `seeled` aceitou
cerimônia; quem apertou um botão não. E a portaria custa quase nada onde o padrão
aberto era defensável — numa rede local entre duas máquinas, é um clique — mas é
a diferença entre aberto e fechado no dia em que a porta do roteador abre, que é
o dia que o 0021 escreveu como o dia em que o padrão deixa de servir.

Ligado só na primeira vez: é semente de servidor novo, não imposição a cada subida.
Quem desligar, fica desligado.

**E, ligado ou desligado, o estado da porta é dito na janela.** É a metade que
falta hoje e que nenhum padrão conserta: quem hospeda vê, no cartão de
hospedagem, se a porta está aberta, com senha, com convites, com portaria — e um
servidor aberto que alcança além do loopback diz isso numa banda de alerta, porque
é uma porta aberta para a internet e é disso que o vermelho é reservado.

## Alternativas

1. **Lista de chaves autorizadas antes de entrar**, estilo `authorized_keys` — a
   alternativa 3 do 0021. Continua recusada pelo mesmo motivo: inverte o fluxo
   social, exigindo que a outra pessoa mande a chave antes. TOFU aplicado a gente
   é essa mesma lista, construída pelo uso em vez de antes dele.
2. **Segurar a conexão enquanto quem hospeda decide.** Melhor de ler — a pessoa
   vê «aguardando» de verdade. Recusada por três motivos que se somam: obriga um
   prazo, e um prazo cria a resposta «ninguém atendeu» que não se sabe traduzir
   em ação; segura recurso do servidor por alguém que ainda não entrou; e não
   melhora o caso que importa, o da janela minimizada, onde o prazo vence de
   qualquer jeito. Um pedido durável é uma promessa mais forte que uma barra
   girando.
3. **Aprovar automaticamente quem chegou com convite válido.** Tentador — quem
   hospeda gerou aquele convite com as próprias mãos. Recusada porque um link se
   encaminha, e porque transforma a camada mais forte na mais fraca: bastaria
   vazar um convite não usado para atravessar a portaria também. O convite vira
   **prova exibida**, que é todo o valor dele sem nenhum do risco.
4. **Fechar o padrão do `seeled` também.** Recusada: reverteria uma decisão do
   0021 de carona numa mudança que não precisa dela. O problema relatado é do
   botão, e a correção fica no botão.
5. **Verbos de protocolo para a portaria**, como o resto da moderação. Recusada
   acima; e ela custaria expor à internet a decisão sobre quem entra pela
   internet.

## Consequências

- Quem hospeda pelo botão passa a ter, na janela, as três camadas: fechar com
  senha, gerar convite, e decidir quem entra. Era o buraco relatado.
- **Um servidor hospedado pelo app deixa de ser aberto por padrão** sem que o padrão
  do `seeled` mude.
- **Quem não passa pela portaria não vira conta.** Ela roda antes de
  `register_or_find`, e essa ordem é o que impede uma batida recusada de reservar
  um apelido para sempre — o ADR 0017 prende o nome à chave, e um pedido negado
  não deve deixar um nome ocupado por alguém que jamais entrou. O custo é que
  quem está banido **e** desconhecido é respondido como pendente em vez de
  banido, e aparece na fila: quem hospeda vê e recusa. Barulho pequeno, e do lado
  certo — a decisão continua com quem hospeda.
- Uma pessoa aprovada cujo apelido tenha sido tomado por outra chave nesse meio
  tempo é recusada por `register_or_find`, e não pela portaria. É a regra do ADR
  0017 valendo como sempre valeu; a aprovação continua de pé para quando ela
  escolher outro nome.
- A fila de pedidos é metadado que o servidor passa a guardar: quem tentou entrar e
  quando, inclusive de quem nunca entrou. É informação que ele já teria no log;
  a diferença é que agora tem tabela e prazo de vida nenhum. Limpar pedidos
  antigos não está construído.
- `AdmissionPending` e `AdmissionDenied` são apensadas ao fim de
  `DisconnectReason`, como manda a compatibilidade do `postcard`: um cliente uma
  versão mais velho recusa o quadro em vez de ler outra razão no lugar.

Custo de reverter: **baixo**. Um módulo, uma migração aditiva, duas razões
apensadas e um interruptor. Desligar a portaria devolve exatamente o
comportamento de hoje, e a tabela que sobra não é lida por ninguém.

## Adendo — a espera do cliente passa a insistir, enquanto alguém olha (22/08/2026)

A alternativa 2 acima foi recusada, e continua recusada: **o servidor não segura a
conexão** enquanto quem hospeda decide. O que mudou é do outro lado do fio.

O relato de campo foi «quando o usuário vai entrar num servidor, ele precisa
ficar clicando repetidas vezes num mesmo lugar», e era literal: o botão da tela
de aperto de mão tinha três passos — conferir, entrar, tentar de novo — e a
primeira vez de qualquer pessoa passa pela terceira, porque a portaria deste ADR
recusa quem ainda não foi liberado. Uma pessoa esperando aprovação ficava
apertando um botão de minuto em minuto para descobrir se já.

Agora a tela insiste sozinha, a cada quinze segundos. Havia um guarda que
proibia exatamente isso, e ele citava este ADR e o 0025. Duas das três razões
dele não sobreviveram ao exame:

- **não é o que este ADR recusou.** O que se recusou foi segurar a conexão. Uma
  batida do cliente conecta, é recusada e desconecta: não segura recurso do
  servidor, não obriga prazo nenhum, e não fabrica a resposta «ninguém atendeu» —
  a resposta continua sendo a mesma recusa durável de sempre;
- **não estoura o balde do ADR 0025.** Quinze segundos são quatro batidas por
  minuto, e o balde de antes de autenticar repõe trinta. A bateria de reconexão
  que já existe bate mais forte: vinte e quatro tentativas em cinco minutos por
  cliente, e ninguém chamou aquilo de inundação. O intervalo não é número
  inventado — é o `MAX_BACKOFF` daquela bateria.

A terceira razão sobreviveu, e é a que este adendo endereça: **o para sempre.**
Uma janela minimizada batendo por horas, sem ninguém para ler a resposta, é
gasto no servidor de um estranho por uma espera que ninguém está esperando. A
alternativa 2 já nomeava esse caso — «o caso que importa, o da janela
minimizada» — e a resposta agora é a mesma coisa dita como regra: **a espera
acompanha o olho.** Com a janela visível, insiste; escondida, para e diz que
parou; de volta, retoma. Sair pela porta encerra, e não pausa.

O guarda não foi apagado: foi reescrito para cobrar a propriedade nova, mais
estreita — «só bate enquanto alguém está olhando» —, incluindo que a pergunta
venha **antes** de o relógio ser armado, ou a janela escondida ainda bateria uma
vez. Conferido por mutação.
