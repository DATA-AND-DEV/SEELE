# Por que "não conecta" de fora da sua rede

Você hospedou um Dogma, mandou o link para um amigo, e ele não entra. Este
documento existe para você não ter que descobrir sozinho por quê.

O resumo honesto: **em algumas casas o SEELE resolve isso sozinho, e em outras
ele não tem como.** As duas situações são normais, e a segunda não é defeito do
seu computador nem do seu amigo. O ADR 0022 registra a decisão por trás disto.

## O que o SEELE tenta sozinho

Quando você aperta **HOSPEDAR AQUI** (ou roda `plug --hospedar`), o SEELE sobe
uma escada e para no degrau mais alto que funcionar. Você não configura nada;
ele tenta e depois **conta o que conseguiu**, junto do link.

| Degrau | O que é | Quem alcança você |
|---|---|---|
| **3 — porta no roteador** | O SEELE pediu a porta ao seu roteador (UPnP) e ele abriu | praticamente todo mundo |
| **2 — IPv6 direto** | Sua máquina tem endereço IPv6 público; não há NAT no caminho | só quem também tiver IPv6 |
| **1 — só a rede local** | Nenhum dos dois deu | só quem estiver na sua casa |

Nada disso passa por servidor nosso nem de terceiro. O degrau 3 é uma conversa
entre o seu computador e o **seu** roteador; o degrau 2 nem isso.

## O que a tela te diz, e o que fazer com cada resposta

### "O roteador abriu a porta"

Deu certo. O link deve funcionar pela internet.

*Deve*, e não *vai*: ainda existe a chance de um firewall no caminho do seu
amigo recusar a saída. É raro. Se ele não entrar, peça para tentar de outra
rede — do celular na rede móvel, por exemplo.

### "Este link é IPv6"

O seu computador tem um endereço IPv6 público, e o link usa ele. Funciona **de
qualquer lugar, para quem também tiver IPv6**.

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

### "Este link só funciona na sua rede"

Este é o caso que dói, e é o caso em que você merece uma explicação em vez de
um link que não funciona. O SEELE te diz o motivo junto do aviso. Os motivos
possíveis são estes.

#### "Nenhum roteador respondeu ao pedido de porta"

Quase sempre é **UPnP desligado**. Muitos roteadores vêm assim de fábrica, e
alguns provedores desligam de propósito.

Entre na configuração do roteador (normalmente `192.168.0.1` ou `192.168.1.1`
no navegador) e procure por **UPnP** — pode estar escrito "UPnP", "Universal
Plug and Play", ou dentro de "NAT" / "Encaminhamento". Ligue, e hospede de novo.

Se você estiver numa rede que não é sua — trabalho, faculdade, hotel, wi-fi de
condomínio — o pedido nem chega ao roteador, e não há o que ligar. Use uma das
saídas do fim desta página.

#### "O roteador recusou abrir a porta"

Ele ouviu e disse não. Alguns roteadores só aceitam pedidos de UPnP de
dispositivos numa lista, e outros têm a função quebrada. A saída é encaminhar a
porta à mão — veja abaixo.

#### "O endereço dele não sai para a internet"

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

**Sem IPv6 e sem UPnP útil, o SEELE não tem saída aqui.** Isso está escrito no
ADR 0022 e é uma consequência de como a internet funciona hoje, não um defeito
que uma versão futura corrige de graça. O degrau 4 da escada — um ponto de
encontro que apresenta os dois lados — resolveria, e ainda não existe: ele custa
uma decisão sobre metadado que o projeto quer tomar em voz alta, e não de
passagem.

O que **não** vai acontecer: retransmitir a sua conversa pelo servidor de
outra pessoa. O ADR 0022 põe isso fora de escopo por decisão. Um produto que
existe para não ter ninguém no meio não passa a ter ninguém no meio para cobrir
os últimos casos.

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

**4. Uma VPN entre vocês.** Tailscale, WireGuard, ZeroTier. Funciona hoje,
funciona em qualquer situação, inclusive CGNAT dos dois lados. Não é a resposta
oficial do projeto porque transfere o problema para outro produto e outra conta
— mas se você tem pressa, é o que eu recomendaria.

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

A escada está em `crates/seele-server/src/alcance.rs`, e o degrau 3 em
`crates/seele-server/src/alcance/porta.rs`. Os motivos de recusa são o enum
`FalhaAoAbrir`, e cada um vira uma das frases desta página. As frases que a
pessoa lê no app estão em `apps/seele-app/ui/frases.js`, como todas as outras.
