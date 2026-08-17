# ADR 0022 — Alcançar um Dogma pela internet

**Estado:** aceito
**Data:** 2026-08-10
**Degraus 2 e 3 implementados:** 2026-08-17 — ver "O que a implementação
ensinou", no fim, e "O que a primeira máquina de outra pessoa ensinou", logo
depois: três decisões deste ADR foram corrigidas por um relato de campo, e a
maior delas é que o convite não pode carregar um endereço só. Os degraus 4 e 5 continuam exatamente como estão escritos
aqui: o 4 esperando a conversa sobre metadado, o 5 fora de escopo por decisão.

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

## O que a implementação ensinou

Escrito depois de construir os degraus 2 e 3, e guardado porque as três coisas
mais úteis daqui são justamente as que este texto tinha errado.

### O degrau 2 não era "sobretudo não atrapalhar"

A "Ordem sugerida" acima chama o degrau 2 de "sobretudo conferir que nada no
caminho assume IPv4". A conferência foi rápida e a conclusão foi que **tudo** no
caminho assumia IPv4, em três lugares independentes, e nenhum se conserta
mexendo no outro:

1. **A escuta** era `0.0.0.0`. Um socket IPv4 não atende IPv6, ponto.
2. **O cliente** ligava em `0.0.0.0:0`. Um socket IPv4 **não envia** para
   destino IPv6, então nenhum trabalho no servidor consertaria isto.
3. **As duas cascas** separavam `host:porta` com `rsplit_once(':')`.

Por isso a ordem acabou invertida: o degrau 2 veio primeiro, porque sem ele o
convite gerado pelo degrau 3 não teria como carregar um endereço IPv6.

E "trocar o endereço para `[::]`" não bastava. Se um socket IPv6 atende IPv4
junto depende do `IPV6_V6ONLY`, cujo padrão muda de sistema para sistema.
Medido, porque a primeira versão do comentário no código estava errada: Linux e
macOS vêm em pilha dupla; **Windows e os BSD vêm em IPv6 puro**. E como nos dois
primeiros o padrão é um `sysctl`, nem lá dá para confiar nele. A opção passou a
ser escrita à mão e conferida de volta.

### O `:` do IPv6 é o mesmo `:` da porta

O ADR diz que o `seele://` "não muda de forma", e está certo: a forma não mudou.
O que ninguém tinha olhado é que o separador de porta e o separador de um IPv6
são o mesmo caractere. `[2001:db8::1]:8383` virava uma máquina com colchetes,
que nenhum resolvedor aceita; `2001:db8::1` virava a máquina `2001:db8:` na
porta `1`, que não existe.

E a lição maior foi sobre a **outra ponta**: o `seeled` montava o alvo do
convite com `format!("{ip}:{porta}")`, que num IPv6 escreve `2001:db8::1:8383`
— exatamente a forma que o cliente passou a recusar. Recusar educadamente um
link torto que nós mesmos geramos seria pôr uma frase bonita em cima de um
defeito nosso. Conferir quem **gera** vale tanto quanto conferir quem lê.

### CGNAT não faz o UPnP falhar — faz ele dar certo à toa

Esta é a correção que mais importa. O texto acima trata CGNAT como um dos casos
em que "UPnP falha em silêncio". Não é isso que acontece.

Atrás de CGNAT — ou de um segundo roteador, o caso do condomínio — o roteador
da casa **atende o pedido e abre a porta**. O `AddPortMapping` devolve sucesso.
O mapeamento existe de verdade. Só que ele abre uma porta na WAN daquele
roteador, que é ela mesma um endereço privado, e ninguém de fora chega lá.

Não é um erro que se possa mostrar: é um sucesso mentiroso, que é uma forma
pior do silêncio que este ADR manda evitar — um erro pelo menos aparece. A
saída foi perguntar ao roteador qual é o endereço externo dele **antes** de
pedir o mapeamento, e conferir contra as faixas que não roteiam, inclusive a
`100.64.0.0/10` que a RFC 6598 reservou para CGNAT.

Não é hipótese: na primeira rede real em que o código rodou, o roteador
respondeu WAN `192.168.0.30`. A conferência pegou de primeira.

### Um mapeamento tem prazo, e o ADR não falava nisso

"UPnP é uma biblioteca e um pedido ao roteador" subestima o fim da história. Um
mapeamento tem validade, e as três decisões que apareceram não são óbvias:

- **Prazo, e não permanente.** Um mapeamento permanente sobrevive ao processo
  que o pediu: um `kill -9` deixa a porta aberta apontando para uma máquina que
  não atende mais, e ninguém nunca mais a fecha. Uma hora de validade faz o pior
  caso se consertar sozinho.
- **Alguém tem de renovar.** A cada 20 minutos, com folga para várias
  tentativas perdidas. Sem isso a porta fecha no meio de uma conversa — e o
  sintoma é o pior possível: funcionou, e parou sem ninguém mexer em nada.
- **Devolver ao fechar.** `Drop` não pode esperar, então a devolução é
  explícita, no mesmo desenho do `Hospedagem::encerrar` que já existia.

### Do degrau 3 saiu o UPnP, e não o NAT-PMP

O degrau 3 se chama "UPnP / NAT-PMP" e chegou só com UPnP, via `igd-next`. O
motivo é que o custo do NAT-PMP não está onde parece: o `crab_nat` custaria
**um** crate, mas não descobre o endereço do gateway, e quem descobre — o
`netdev` — arrasta uma pilha de enumeração de interfaces para dentro de um
daemon que tem de caber em 1 vCPU. Pagar isso para cobrir a fatia de roteadores
que fala PCP e **não** fala UPnP não se paga hoje.

Não é discordância: o degrau 3 é "o anfitrião pede a porta ao roteador dele", e
é o que acontece. Fica anotado como o próximo passo barato se aparecer
descoberta de gateway sem esse custo.

Vale dizer também o que o `igd-next` cobra, porque foi escolhido sabendo:
`attohttpc` é dependência não opcional dele, então um segundo cliente HTTP entra
mesmo usando só o caminho assíncrono, e o `hyper` vem com `http2` fixado para
falar um protocolo que é HTTP/1.1. Seis crates novos, contra 31 do `portmapper`.

### O que continua sem saída, e agora está escrito

CGNAT sem IPv6 e sem UPnP continua sem saída antes do degrau 4 — como este ADR
já dizia. O que mudou é que deixou de ser uma descoberta solitária: a escada não
falha em silêncio, cada recusa é uma variante nomeada com frase própria, ela
aparece **junto do link** e não numa tela de diagnóstico, e
`docs/alcance-pela-internet.md` explica caso a caso o que fazer.

O motivo de aparecer junto do link é concreto: um link que só funciona na rede
de casa e um link que funciona pela internet são o **mesmo texto**.

## O que a primeira máquina de outra pessoa ensinou

Escrito em 2026-08-17, depois de o primeiro relato de campo chegar: um Windows
hospedando, um Mac na mesma casa, e o Mac não entrava. Três coisas decididas
aqui estavam erradas, e vale nomear cada uma.

### "O degrau mais alto vai no convite" era o erro

Está escrito acima que o `seele://` "não muda de forma: muda o que vai dentro", e
a implementação leu isso como *um* endereço, o do degrau mais alto alcançado.
Isso **tirou um caso que já funcionava**: antes desta escada existir, o convite
levava o endereço da rede local, e os dois na mesma casa entravam sempre.

A escada continua sendo a coisa certa para decidir **o que dizer** a quem
hospeda. O que ela não pode fazer é decidir o que **descartar**: os endereços dos
degraus mais baixos continuam valendo, para outras pessoas. O convite passou a
levar todos, na ordem em que valem a pena, e o ADR 0006 registra a forma. A
escada ficou com o papel que só ela tem — a frase.

### Um degrau só pode ser declarado se a escuta o servir

O degrau 2 é "a máquina tem IPv6 global". Faltava a outra metade da conjunção: o
**socket** também precisa atender em IPv6. Na máquina do relato a pilha dupla
falhou e o Dogma recuou para IPv4 — o comportamento que este ADR já previa e que
funcionou —, e a escada, que recebia só a porta, seguiu perguntando pelo IPv6
global da máquina e declarando o degrau 2. O convite anunciava um endereço onde
nada escutava.

Não é um esquecimento pontual: era o tipo que permitia. `Pilha::alcanca_ipv6`
existia, e nada obrigava a perguntar. Agora a escada recebe a escuta inteira e
todo endereço passa por um construtor privado que confere — a afirmação e a
prova da afirmação viraram a mesma chamada.

### "Descobrir o próprio endereço" não é perguntar pela rota padrão

Os dois degraus daqui precisam saber o endereço da máquina, e os dois usavam o
mesmo truque: abrir um socket UDP, `connect` num endereço de documentação, ler o
`local_addr`. Isso responde "qual endereço meu o sistema usaria para sair", que
**não** é a pergunta — e uma VPN captura exatamente essa resposta.

Com Cloudflare WARP ligado, o endereço devolvido era o do túnel. O convite saía
com ele, e o UPnP mandava o roteador encaminhar a porta para um endereço que não
existe naquela rede. O conserto foi enumerar interfaces, o que custou um crate
(`if-addrs`) neste daemon que tem de caber em 1 vCPU — e é uma conta que o
próprio degrau 3 já tinha feito ao recusar o `portmapper` por 31 crates. Aqui é
um, com `libc` como única dependência, e paga um caso medido.

Junto veio um degrau novo, o do endereço de VPN. Um IPv6 de túnel é um unicast
global de verdade e não há como distingui-lo pela faixa; quem sabe é a interface.
Sem essa distinção a escada escrevia "alcança de qualquer lugar" embaixo de um
link que não aceita entrada nenhuma — a forma mais convincente do silêncio que
este ADR existe para não produzir, porque vem com uma frase confiante em cima.
