# 0030 — Quem bate à porta: TOFU aplicado a gente, e a portaria de quem hospeda

Status: proposto

Contexto: o ADR 0021 deu ao Dogma dois porteiros — convite de uso único e senha —
e o servidor sabe operar os dois: `admissao::criar_convite` e
`admissao::definir_senha`. **O app não expõe nenhum dos dois.** O único caminho
até eles é `seeled convite` e `seeled senha`, no terminal.

O público do botão HOSPEDAR AQUI é exatamente quem não abre terminal. Então, na
prática, todo Dogma hospedado pelo app é um Dogma aberto, e **não há nada em
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

| | o que checa | quando | o que a recusa custa ao Dogma |
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

**Um convite consumido não aprova ninguém.** Ele aparece no pedido, ao lado da
observação que quem hospeda escreveu ao gerá-lo — `criar_convite(casper,
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

Que o Dogma tenha portaria não é segredo que valha guardar: quem bate precisa
saber que há alguém para decidir, ou vai embora achando que o endereço está
errado.

### Ninguém espera, e é assim que a espera deixa de ser sem fim

A conexão **não fica pendurada**. Um pedido pendente cai na hora, com
`AdmissionPending`, e a frase diz o que aconteceu: o pedido ficou guardado, e dá
para tentar de novo quando quiser.

Isto dissolve a pior das três respostas em vez de tratá-la. «Ninguém atendeu» só
existe como estado se alguma coisa estiver esperando; como nada espera, o que
sobra é um pedido de pé, que quem hospeda concede quando olhar. A pessoa do outro
lado nunca fica olhando para uma barra que não anda, e o Dogma nunca segura
recurso por alguém que ainda não entrou — que é o mesmo motivo pelo qual o 0021
não quis gastar assinatura com quem nem devia estar batendo.

### E se ninguém estiver olhando

**O pedido é uma linha em SQLite, não um aviso na tela.** Ele sobrevive à janela
minimizada, ao app fechado e à máquina reiniciada. Quem hospeda vê a fila quando
abrir a portaria, com a hora de cada pedido; conceder três dias depois vale igual,
e a pessoa entra na próxima vez que tentar.

Um Dogma que só admite enquanto alguém olha a tela seria um Dogma que recusa por
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

Sobre o buraco já registrado — `banir_piloto` existe e `unban` não tem verbo de
protocolo: **este desenho não o piora, e mostra a saída.** Ele não piora porque
não acrescenta verbo nenhum de protocolo: a portaria inteira é estado local da
máquina que hospeda, decidido por quem tem o arquivo do Dogma na mão — que é
exatamente a autoridade que a frase de confirmação do banimento já nomeia como
a única que o desfaz. E mostra a saída porque estabelece a forma: uma decisão
que se desfaz é uma linha que se apaga, e `Melchior::unban` já é literalmente
`DELETE FROM bans`. Falta a ele só o caminho até a janela. Não é consertado aqui
de passagem; fica anotado.

### Onde os comandos vivem: na casa, não no fio

Toda moderação deste app hoje viaja pelo fio, como cliente, mesmo quando o
próprio app hospeda. A portaria **não**: ela fala direto com o CASPER do servidor
embutido, por acessores novos em `Hospedagem`.

Três motivos. **Fechar a porta não pode depender de estar dentro** — exigir uma
sessão autenticada para configurar a admissão faz a defesa depender do canal que
ela defende, e obriga quem hospeda a ter entrado enquanto ainda estava aberto.
**A porta se fecha antes do primeiro pacote**, no mesmo gesto de hospedar.
E **nenhum verbo novo de protocolo** significa nenhuma superfície nova exposta à
internet para uma decisão que é, por definição, de quem está na máquina.

O custo, que é real: isto não administra o Dogma de outra pessoa. Um Comandante
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

Ligado só na primeira vez: é semente de Dogma novo, não imposição a cada subida.
Quem desligar, fica desligado.

**E, ligado ou desligado, o estado da porta é dito na janela.** É a metade que
falta hoje e que nenhum padrão conserta: quem hospeda vê, no cartão de
hospedagem, se a porta está aberta, com senha, com convites, com portaria — e um
Dogma aberto que alcança além do loopback diz isso numa banda de alerta, porque
é uma porta aberta para a internet e é disso que o vermelho é reservado.

## Alternativas

1. **Lista de chaves autorizadas antes de entrar**, estilo `authorized_keys` — a
   alternativa 3 do 0021. Continua recusada pelo mesmo motivo: inverte o fluxo
   social, exigindo que a outra pessoa mande a chave antes. TOFU aplicado a gente
   é essa mesma lista, construída pelo uso em vez de antes dele.
2. **Segurar a conexão enquanto quem hospeda decide.** Melhor de ler — a pessoa
   vê «aguardando» de verdade. Recusada por três motivos que se somam: obriga um
   prazo, e um prazo cria a resposta «ninguém atendeu» que não se sabe traduzir
   em ação; segura recurso do Dogma por alguém que ainda não entrou; e não
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
- **Um Dogma hospedado pelo app deixa de ser aberto por padrão** sem que o padrão
  do `seeled` mude.
- Uma pessoa não aprovada ainda **cria conta e reserva o apelido** ao bater:
  `register_or_find` roda antes da portaria, porque é ele que transforma a chave
  provada em conta. É pequeno e é real — dá para ocupar um apelido sem nunca
  entrar. Anotado nas pendências, não consertado aqui: mexer na ordem daquele
  trecho mexe no caminho do banimento junto.
- A fila de pedidos é metadado que o Dogma passa a guardar: quem tentou entrar e
  quando, inclusive de quem nunca entrou. É informação que ele já teria no log;
  a diferença é que agora tem tabela e prazo de vida nenhum. Limpar pedidos
  antigos não está construído.
- `AdmissionPending` e `AdmissionDenied` são apensadas ao fim de
  `DisconnectReason`, como manda a compatibilidade do `postcard`: um cliente uma
  versão mais velho recusa o quadro em vez de ler outra razão no lugar.

Custo de reverter: **baixo**. Um módulo, uma migração aditiva, duas razões
apensadas e um interruptor. Desligar a portaria devolve exatamente o
comportamento de hoje, e a tabela que sobra não é lida por ninguém.
