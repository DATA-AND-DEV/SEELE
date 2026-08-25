# Por que "não conecta" de fora da sua rede

Você hospedou um servidor, mandou o link para um amigo, e ele não entra. Este
documento existe para você não ter que descobrir sozinho por quê.

O resumo honesto: **em algumas casas o SEELE resolve isso sozinho, e em outras
ele não tem como.** As duas situações são normais, e a segunda não é defeito do
seu computador nem do seu amigo. O ADR 0022 registra a decisão por trás disto.

## O que o SEELE tenta sozinho

Quando você aperta **HOSPEDAR AQUI** (ou roda `connection --hospedar`), o SEELE sobe
uma escada e para no degrau mais alto que funcionar. Você não configura nada;
ele tenta e depois **conta o que conseguiu**, junto do link.

| Degrau | O que é | Quem alcança você |
|---|---|---|
| **3 — porta no roteador** | O SEELE pediu a porta ao seu roteador (UPnP) e ele abriu | praticamente todo mundo |
| **4 — furo de NAT** | Nenhum caminho direto, e um ponto de encontro apresentou as duas máquinas | quase todo mundo, e há casos em que não abre |
| **2 — IPv6 direto** | Sua máquina tem endereço IPv6 público; não há NAT no caminho | só quem também tiver IPv6 |
| **1 — rede local, ou a mesma VPN** | Nenhum dos outros deu, e o único endereço que sai daqui é de uma VPN | quem estiver na sua casa, ou na mesma VPN |
| **1 — só a rede local** | Nenhum dos outros deu | só quem estiver na sua casa |

Os degraus 3, 2 e 1 não passam por servidor nosso nem de terceiro: o 3 é uma
conversa entre o seu computador e o **seu** roteador, e o 2 nem isso.

**O degrau 4 é o único com um terceiro no meio**, e é por isso que ele é o
último a ser tentado — só quando nenhum dos outros resolveu. Ele é o que faz
"manda o link e funciona" valer numa casa com CGNAT ou com UPnP desligado, e
custa uma informação que antes não existia em lugar nenhum. A seção
**[O que o ponto de encontro fica sabendo](#o-que-o-ponto-de-encontro-fica-sabendo)**
diz exatamente o quê, e como não usar o nosso.

## O link leva mais de um endereço, de propósito

A sua máquina tem vários endereços, e **nenhum deles serve para todo mundo**: o
da sua rede não é alcançável de fora, e o público que o roteador abriu quase
sempre não volta para dentro da sua própria casa (a maioria dos roteadores
domésticos não faz isso — chama-se *hairpin*, e é a explicação de "funciona
para o meu amigo do outro estado e não para quem está na sala ao lado").

Por isso o link carrega todos, na ordem em que valem a pena: **o da sua rede
primeiro**, depois o global, depois a porta do roteador, e uma VPN por último.
Quem recebe tenta um de cada vez e para no primeiro que atender. Na mesma casa a
conexão sai imediatamente, sem esperar por um caminho que não volta.

Isto mudou na versão seguinte à 0.5.0, e vale saber por quê: a 0.5.0 punha no
link **só** o endereço do degrau mais alto, e com isso quebrou o caso que sempre
funcionou — os dois na mesma rede. Um link antigo, de um endereço só, continua
sendo aceito, e um cliente antigo lê o primeiro endereço do link novo e ignora o
resto.

## O que a tela te diz, e o que fazer com cada resposta

A tela diz **uma frase**, e uma segunda só quando ela muda o que você faz. É de
propósito: um aviso de três parágrafos embaixo de um link é um aviso que ninguém
lê. O porquê de cada resposta, as marcas de roteador e de VPN, e as saídas que
não cabem numa frase estão aqui — esta página é a versão longa, e ela existe
justamente para a tela poder ser curta.

### "O roteador abriu a porta"

Deu certo. O link deve funcionar pela internet, e também na sua rede: ele leva os
dois endereços, e a tela não gasta uma linha dizendo isso porque quem está perto
entra sem precisar ler nada.

*Deve*, e não *vai*: ainda existe a chance de um firewall no caminho do seu
amigo recusar a saída. É raro. Se ele não entrar, peça para tentar de outra
rede — do celular na rede móvel, por exemplo.

### "Um ponto de encontro abriu o caminho"

O degrau 4. Você está numa rede em que o roteador não abriu a porta — CGNAT, dois
roteadores, UPnP desligado —, e o SEELE conseguiu assim mesmo: um serviço
minúsculo contou a cada lado o endereço do outro, os dois mandaram pacote ao
mesmo tempo, os roteadores abriram o caminho, e **daí em diante o tráfego é
direto**. É o mesmo mecanismo do WebRTC, que é como funciona chamada de vídeo no
navegador.

*Deve funcionar*, e não *vai funcionar*, e desta vez o motivo é específico:
quando as duas redes fazem **NAT simétrico** — o tipo que muda o endereço a cada
destino —, o furo não abre. Não há o que ajustar na sua máquina; é o tipo do
roteador dos dois lados. Se não entrar, as saídas são as do fim desta página:
encaminhar a porta à mão, ou uma VPN de rede entre vocês.

O link continua levando o endereço da sua rede junto, então quem estiver na sua
casa entra pelo caminho de sempre, sem passar por ponto de encontro nenhum.

### "Este link leva um endereço IPv6"

O seu computador tem um endereço IPv6 público, e o link usa ele. Funciona **de
qualquer lugar, para quem também tiver IPv6**. O endereço da sua rede vai junto,
como em todos os degraus, então quem está na sua casa entra do mesmo jeito.

No Brasil isso cobre boa parte da internet móvel e uma fatia crescente da fixa.
Se o seu amigo não tiver IPv6, ele vai precisar estar na sua rede, ou você vai
precisar de uma das saídas da seção seguinte.

Duas coisas podem atrapalhar mesmo com IPv6 dos dois lados:

- **O firewall do seu roteador.** IPv6 não tem NAT, mas tem firewall, e muitos
  roteadores bloqueiam conexões de entrada por padrão. Procure por "IPv6
  firewall" ou "filtro de entrada IPv6" nas configurações e libere a porta
  **8383/UDP**.
- **Endereços que mudam.** Muitos provedores trocam o prefixo IPv6 de tempos em
  tempos. Um link gerado ontem pode não valer hoje. Gere um novo.

### "Este link só alcança a sua rede, ou quem estiver na mesma VPN"

Você está com uma **VPN ligada**, e ela é o único caminho que sai desta máquina.

Isto não é a VPN atrapalhando por acaso: é o que uma VPN faz. Ela captura o
tráfego de saída, e o endereço que ela te dá é dela, não da sua casa. VPN de
navegação — Cloudflare WARP, Proton, Nord, Mullvad — **não aceita conexão de
entrada**: você alcança o mundo por ela, o mundo não te alcança.

Pior: o endereço IPv6 que uma dessas VPNs te dá é um endereço global de
verdade, e por fora não há como distinguir. Até a versão seguinte à 0.5.0 o
SEELE olhava para ele, concluía "degrau 2" e escrevia "alcança de qualquer
lugar" embaixo de um link que não aceitava ninguém. Era exatamente o silêncio
que este documento existe para não deixar acontecer — só que com uma frase
confiante em cima.

O que fazer:

- **Se quem vai entrar está na sua rede**, nada. O link leva também o endereço
  da sua casa, e é o primeiro que ele tenta.
- **Se quem vai entrar está longe**, desligue a VPN e hospede de novo — aí o
  SEELE volta a poder pedir a porta ao roteador, e o degrau 3 entra em jogo.
- **Ou ponham os dois na mesma VPN**, e aí o endereço da VPN passa a servir. É
  a diferença entre uma VPN de navegação e uma **VPN de rede** (Tailscale,
  WireGuard, ZeroTier): a segunda existe para os dois lados se enxergarem, e
  funciona inclusive com CGNAT dos dois lados.

Um detalhe que morde na hora de conferir: com VPN ligada, o endereço que o
comando de rede da sua máquina mostra como "o meu" costuma ser o do túnel. O
endereço que interessa é o da placa de rede — o `192.168.x.x` ou `10.x.x.x`. O
SEELE agora procura pelas interfaces, e não pelo caminho de saída, exatamente
por isso.

### "Este link só funciona na sua rede"

Este é o caso que dói, e é o caso em que você merece uma explicação em vez de
um link que não funciona. O SEELE te diz o motivo junto do aviso. Os motivos
possíveis são estes.

#### "Nenhum roteador respondeu ao pedido de porta"

Quase sempre é **UPnP desligado** — é o que a frase manda ligar. Muitos
roteadores vêm assim de fábrica, e alguns provedores desligam de propósito.

Entre na configuração do roteador (normalmente `192.168.0.1` ou `192.168.1.1`
no navegador) e procure por **UPnP** — pode estar escrito "UPnP", "Universal
Connection and Play", ou dentro de "NAT" / "Encaminhamento". Ligue, e hospede de novo.

Se você estiver numa rede que não é sua — trabalho, faculdade, hotel, wi-fi de
condomínio — o pedido nem chega ao roteador, e não há o que ligar. Use uma das
saídas do fim desta página.

#### "O roteador recusou abrir a porta"

Ele ouviu e disse não. Alguns roteadores só aceitam pedidos de UPnP de
dispositivos numa lista, e outros têm a função quebrada. A saída é encaminhar a
porta à mão — veja abaixo.

#### "Nenhum endereço desta máquina está na rede dele"

O roteador respondeu — de `192.168.x.x`, digamos — e o único endereço que esta
máquina tem é o de um túnel, `172.16.x.x`. **Costuma ser uma VPN ligada**: ela
fica com todo o tráfego de saída e deixa a placa de rede sem uso, então a
máquina não tem endereço nenhum na rede em que o roteador encaminharia.

Pedir o mapeamento assim mesmo abriria a porta apontando para um endereço que
não existe naquela rede — e o roteador diria `Ok`. É outro sucesso mentiroso, da
mesma família do CGNAT, e é por isso que a pergunta vem antes do pedido.

A saída é a mesma da seção sobre VPN, acima: desligue a VPN e hospede de novo.

#### "O endereço do roteador não sai para a internet"

Este é o caso sem saída fácil, e vale entender por quê.

O seu roteador abriria a porta de boa vontade — mas o endereço que **ele**
tem na internet também é um endereço privado. Existe outro NAT acima dele.
Abrir a porta no seu roteador funcionaria e não adiantaria nada, porque quem
vem de fora nunca chega até ele. (É por isso que o SEELE pergunta antes de
pedir: um "sucesso" aqui seria mentira.)

Duas causas:

- **CGNAT.** A sua operadora não te dá um endereço IPv4 só seu; ela divide um
  entre vários clientes. É cada vez mais comum, principalmente em internet via
  rádio, 4G/5G e alguns provedores regionais.
- **Dois roteadores.** Você tem um roteador seu ligado no roteador do
  provedor, ou mora num prédio que distribui internet por um roteador central.

**É para este caso que o degrau 4 existe.** Quando um ponto de encontro está
alcançável, o SEELE apresenta as duas máquinas e o furo de NAT costuma resolver
justamente esta situação — que era, até ele existir, a que não tinha saída
nenhuma. Se a tela disser "só funciona na sua rede" mesmo assim, o ponto de
encontro não respondeu (fora do ar, ou uma rede que não deixa UDP sair) ou as
duas redes são do tipo que não deixa furar.

Não é mágica e não é garantia: leia
**[O que o ponto de encontro fica sabendo](#o-que-o-ponto-de-encontro-fica-sabendo)**
antes de decidir se você quer esse degrau ligado.

O que **não** vai acontecer: retransmitir a sua conversa pelo servidor de
outra pessoa. O ADR 0022 põe isso fora de escopo por decisão. Um produto que
existe para não ter ninguém no meio não passa a ter ninguém no meio para cobrir
os últimos casos.

## O que o ponto de encontro fica sabendo

Esta é a parte que este projeto não quer que você descubra depois.

O degrau 4 põe um serviço no meio — só na **apresentação**, nunca na conversa —,
e um serviço no meio aprende alguma coisa. O que ele aprende é isto:

| Ele fica sabendo | Ele **não** fica sabendo |
|---|---|
| que o seu endereço falou com o endereço de outra pessoa, e quando | o que foi dito, em texto ou em voz |
| o endereço público das duas máquinas | quem são vocês, que Server é, quais salas existem |
| que houve uma tentativa de conexão | se ela deu certo |

O conteúdo continua ponta a ponta: o TLS 1.3 e a impressão digital do ADR 0003
são conferidos entre as duas máquinas, e o ponto de encontro não tem por onde ler
nem por onde se passar por ninguém — quem entra nem lê resposta dele, porque os
endereços que tenta vieram todos do link.

Três coisas fazem parte da decisão, e não são promessas soltas:

- **Ele não guarda nada.** Não há banco nem arquivo: a decisão dele é uma função
  que recebe um datagrama e devolve outro. Por padrão ele nem imprime quem falou
  com quem.
- **Ele é opcional.** `SEELE_ENCONTRO=nao` na máquina que hospeda desliga o
  degrau 4, e nenhum pacote sai dali para ponto de encontro nenhum. Tudo o que
  funcionava continua igual.
- **Ele é trocável.** O endereço do ponto de encontro viaja **dentro do link**,
  então apontar para o seu não exige versão nova de nada nem que a outra pessoa
  saiba que ele mudou. Subir um é uma linha de comando numa VPS barata:
  [`ponto-de-encontro.md`](ponto-de-encontro.md).

Uma última coisa que vale saber sobre o link: quando ele traz um bilhete de
encontro, **quem tem o link aprende o seu endereço público sem precisar
conectar**. Quem conecta aprenderia de qualquer forma, e um link é para dar a
quem se convida — mas é um motivo a mais para não colar um `seele://` em lugar
público.

## O firewall da sua máquina — o Windows, principalmente

Tudo acima é sobre o caminho até a sua máquina. Falta a última porta, que é a
da própria máquina, e no Windows ela é fechada por padrão.

**A caixa "Permitir que este aplicativo se comunique nas redes" pode nunca
aparecer.** Se você está esperando por ela, pare de esperar: para escuta UDP de
programa de console ela não aparece mesmo, e o pacote é descartado sem aviso
nenhum, dos dois lados. Foi assim num caso real, e o dono da máquina teve de
abrir a porta à mão.

São duas coisas, e as duas precisam estar certas:

**1. A rede tem de ser "Particular".** Uma rede marcada como "Pública" bloqueia
praticamente toda entrada, e o Windows marca assim toda rede nova. Confira e
corrija no PowerShell, como administrador:

```powershell
Get-NetConnectionProfile
Set-NetConnectionProfile -Name "<o nome da sua rede>" -NetworkCategory Private
```

**2. A porta 8383/UDP tem de ter regra de entrada.** Também no PowerShell, como
administrador:

```powershell
New-NetFirewallRule -DisplayName "SEELE server" -Direction Inbound `
  -Protocol UDP -LocalPort 8383 -Action Allow -Profile Private
```

Para conferir que o servidor está mesmo escutando, e **em quê**:

```powershell
Get-NetUDPEndpoint -LocalPort 8383
```

Se aparecer `0.0.0.0`, ele está atendendo só em IPv4 nesta máquina — o Windows
abre socket IPv6 sem IPv4 junto por padrão, e o SEELE recua para IPv4 quando a
pilha dupla não sai. Isso é esperado, e a partir da versão seguinte à 0.5.0 o
link deixa de anunciar endereço IPv6 quando isso acontece (anunciava, e ninguém
entrava por ele: não havia nada escutando ali).

No macOS e no Linux o caso é bem mais raro. No macOS, se o firewall estiver
ligado, ele pergunta na primeira execução e basta permitir; no Linux, `ufw allow
8383/udp` ou o equivalente do seu firewall.

## As saídas que sempre funcionam

Em ordem de trabalho, do menos ao mais:

**1. Pedir IPv6 à operadora.** Se você está em CGNAT, IPv6 é a saída mais
provável, e muitas operadoras já entregam — às vezes só falta ligar no roteador.
Procure "IPv6" na configuração dele. Resolvido isso, o SEELE usa o degrau 2
sozinho.

**2. Pedir um IPv4 público.** Muitas operadoras tiram o cliente do CGNAT se
você ligar e pedir. Costuma ser gratuito. Pergunte por "IP público" ou "IP fixo".

**3. Encaminhar a porta à mão.** Se o roteador é seu e você tem endereço
público, entre na configuração dele, procure "Encaminhamento de portas" (*port
forwarding*) e crie uma regra:

- protocolo: **UDP**
- porta externa: **8383**
- porta interna: **8383**
- destino: o endereço da sua máquina na rede (o SEELE mostra qual é)

Depois disso o link do degrau 1 passa a funcionar de fora — é o degrau 1 do
ADR 0022, o que sempre funcionou.

**4. Uma VPN de rede entre vocês.** Tailscale, WireGuard, ZeroTier. Funciona
hoje, funciona em qualquer situação, inclusive CGNAT dos dois lados. Não é a
resposta oficial do projeto porque transfere o problema para outro produto e
outra conta — mas se você tem pressa, é o que eu recomendaria.

**Não confunda com VPN de navegação** (WARP, Proton, Nord): aquelas escondem
você e não deixam ninguém entrar, e ligadas na máquina que hospeda elas
**atrapalham** — veja a seção sobre isso acima. As duas coisas se chamam "VPN" e
fazem trabalhos opostos aqui.

**5. Hospedar numa VPS.** Uma máquina virtual barata tem endereço público e
nenhum NAT. É o caminho de quem hospeda a sério, e é para isso que o `seeled`
existe.

## Como saber se você está em CGNAT, sem esperar dar errado

Compare dois números:

1. O endereço que o **seu roteador** diz ter na internet (na tela de status
   dele, procure "WAN" ou "Internet").
2. O endereço que um site como `meuip.com.br` mostra.

Se forem **diferentes**, há outro NAT acima do seu roteador — CGNAT ou um
segundo roteador. Se o número em (1) começar com `100.64` até `100.127`, é
CGNAT com certeza: essa faixa existe só para isso.

## Para quem for mexer no código

A escada está em `crates/seele-server/src/alcance.rs`, o degrau 3 em
`crates/seele-server/src/alcance/porta.rs`, o degrau 4 em
`crates/seele-server/src/alcance/encontro.rs` (quem hospeda),
`crates/seele-core/src/encontro.rs` (quem entra) e `crates/seele-encontro/` (o
serviço), e a descoberta de endereços — a que pergunta às interfaces em vez de à
rota padrão, por causa da VPN — em
`crates/seele-server/src/alcance/interfaces.rs`. A lista de endereços dentro do
link é o `alt=` do `crates/seele-proto/src/uri.rs`, e quem tenta um de cada vez
é `Enlace::conectar_entre`, em `crates/seele-core/src/enlace.rs`. Os motivos de recusa são o enum
`FalhaAoAbrir`, e cada um vira uma das frases desta página. As frases que a
pessoa lê no app estão em `apps/seele-app/ui/frases.js`, como todas as outras.
