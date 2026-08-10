# ADR 0022 — Alcançar um Dogma pela internet

**Estado:** proposto
**Data:** 2026-08-10

## Contexto

Hoje o `seeled` abre uma porta UDP e o cliente conecta nela. Na mesma rede
local funciona. Pela internet, só funciona se o anfitrião for alcançável de
fora — e atrás de um roteador doméstico ele não é.

É o mesmo problema do servidor de Minecraft caseiro, e vale examinar como os
mods que "resolvem" isso resolvem, porque a resposta muda o que dá para copiar.

### O que os mods de Minecraft realmente fazem

Dois mecanismos, e eles são bem diferentes entre si:

**Túnel com retransmissão** — playit.gg, ngrok, Essential. O link amigável
aponta para a infraestrutura *deles*; o tráfego entra lá e é encaminhado para a
sua máquina. Não é "sem servidor": é o servidor de outra pessoa, escondido. Todo
o áudio e todo o texto passariam por um terceiro.

**Furo de NAT com ponto de encontro** — alguns mods, e é o que o WebRTC faz. Um
serviço minúsculo diz a cada lado qual é o endereço público do outro, os dois
mandam pacotes ao mesmo tempo, os roteadores abrem o caminho, e **daí em diante
o tráfego é direto**. O terceiro participa da apresentação, não da conversa.

A diferença importa muito para este projeto. O primeiro contradiz a premissa —
seria o modelo do Discord com outro nome. O segundo é compatível: o intermediário
vê que duas pessoas se falaram e nunca vê o que disseram.

### Dá para dois clientes se enxergarem sem nenhum servidor?

**No caso geral, não.** Não é limitação do nosso código, é como o NAT funciona:

- Nenhuma das duas máquinas sabe qual é o próprio endereço público. O roteador
  reescreve isso na saída, e o interior nunca vê o resultado. Alguém de fora
  precisa contar.
- Em NAT simétrico, o mapeamento é **por destino**: o endereço público que o
  amigo veria é diferente do que qualquer outro veria. Não dá para calcular
  antes nem combinar por WhatsApp.
- Abrir o caminho exige os dois lados mandando pacote quase ao mesmo tempo.
  Isso é coordenação, e coordenação precisa de um canal.

Há **duas exceções reais**, e são as que dão "sem servidor nenhum" de verdade:

**UPnP / NAT-PMP / PCP.** O anfitrião pede ao **próprio roteador** que abra a
porta. Nenhum terceiro em lugar nenhum. Funciona numa fatia grande dos
roteadores domésticos, e falha em silêncio no resto — operadora com CGNAT,
roteador com UPnP desligado, condomínio.

**IPv6.** Sem NAT, cada máquina tem endereço roteável. Sobra só a regra de
firewall. A adoção no Brasil é alta em móvel e razoável em fixa, e cresce.
Quando os dois lados têm IPv6, o problema simplesmente não existe.

## Decisão

Tratar alcançabilidade como uma **escada**, tentada em ordem, e não como um
mecanismo único. Cada degrau vale por si e nenhum depende do seguinte.

1. **Endereço direto.** O que existe hoje. Rede local, VPS, porta encaminhada
   à mão. Continua sendo o caminho de quem hospeda a sério.
2. **IPv6.** Quando os dois lados têm, é um endereço e uma regra de firewall.
   Zero infraestrutura.
3. **UPnP / NAT-PMP.** O anfitrião pede a porta ao roteador dele. Zero
   infraestrutura, e resolve boa parte das casas.
4. **Furo de NAT com ponto de encontro.** Um serviço minúsculo apresenta os
   dois; o tráfego é direto. É o degrau que dá "manda o link e funciona".
5. **Retransmissão.** Explicitamente **fora de escopo por enquanto** — ver
   abaixo.

O `seele://` já existe e não muda de forma: muda o que vai dentro. Hoje leva
endereço e impressão digital; no degrau 4 levaria um bilhete de encontro.

## Consequências

**Os degraus 2 e 3 são baratos e não custam nada ao modelo.** UPnP é uma
biblioteca e um pedido ao roteador; IPv6 é sobretudo não atrapalhar. Os dois
juntos provavelmente cobrem a maioria dos casos de "eu e três amigos", sem
terceiro nenhum.

**O degrau 4 custa uma decisão, não só código.** O ponto de encontro aprende
**metadado**: que endereço falou com que endereço, e quando. Nunca o conteúdo —
o TOFU e o TLS 1.3 continuam ponta a ponta, e a impressão digital continua sendo
conferida contra a do Dogma. Mas é uma informação que hoje não existe em lugar
nenhum, e um projeto que se vende como "sem serviço no meio" precisa dizer isso
em voz alta em vez de descobrir depois.

Mitigações que fazem parte da decisão, se o degrau 4 for adiante: o ponto de
encontro é **opcional e trocável** — quem hospeda aponta para o seu, e o
endereço dele vai no `seele://`; ele é sem estado, não guarda nada, e não
consegue ler nem se passar por ninguém.

**O degrau 5 fica de fora.** Retransmissão resolve os últimos casos — os dois
lados em NAT simétrico — e cobra o preço inteiro: banda de terceiro, custo
recorrente, e um caminho por onde toda conversa passa. Reconstruir o Discord
para cobrir a minoria dos casos troca a premissa do produto pela conveniência
dela. Quem cair nesse caso tem o degrau 1, que sempre funcionou.

**O que isto não resolve.** CGNAT sem IPv6 e sem UPnP continua sem saída antes
do degrau 4. Vale dizer isso na documentação em vez de deixar a pessoa
descobrindo sozinha por que "não conecta".

## Alternativas consideradas

**Túnel pronto (playit.gg, ngrok, Cloudflare).** Funciona amanhã e sem código.
Recusado como padrão: põe um terceiro no caminho de toda conversa, que é
exatamente o que este produto existe para não fazer. Continua disponível para
quem quiser — não há nada a impedir, e a documentação pode mencioná-lo como o
que é.

**Exigir VPN (Tailscale, WireGuard).** Funciona hoje, zero código, e é o que eu
recomendaria a alguém com pressa. Recusado como resposta oficial porque
transfere o problema para outro produto e outra conta — "auto-hospedado, desde
que você use o serviço de outra empresa" não se sustenta.

**Combinar endereços pelo próprio link, sem ponto de encontro.** Sedutor e não
funciona: em NAT simétrico o mapeamento depende do destino, então o endereço
não existe antes de a conversa começar.

## Ordem sugerida

O degrau 3 (UPnP) primeiro: é o que tem melhor relação entre esforço e casos
resolvidos, e não pede decisão nenhuma sobre metadado. Depois o 2, que é
sobretudo conferir que nada no caminho assume IPv4. O degrau 4 só depois de
uma conversa explícita sobre o que o ponto de encontro aprende — e este ADR
existe para que essa conversa aconteça antes do código, e não depois.
