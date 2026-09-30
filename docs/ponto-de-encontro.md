# O ponto de encontro, e como subir o seu

O degrau 4 do ADR 0022 — furo de NAT — precisa de um serviço minúsculo que
apresente as duas máquinas uma à outra. Este documento é sobre ele: **o que ele
faz, o que ele fica sabendo, e como pôr um seu no ar.**

Se você só quer entender por que "não conecta", o documento é o outro:
[`alcance-pela-internet.md`](alcance-pela-internet.md).

## O que ele faz, em uma frase

Ele diz a quem manda um pacote qual é o endereço de onde aquele pacote veio — ou
conta isso a um terceiro endereço, quando quem manda pede.

E guarda em memória o endereço em que cada anfitrião disse morar, para dizê-lo a
quem perguntar por ele: é **o quarto**, descrito abaixo. Esse endereço vale 60
segundos desde o último aviso do anfitrião; vencido, deixa de ser dito a quem
pergunta, mas não sai da memória na hora: fica lá até o quarto encher, até um
aviso novo em nome do mesmo anfitrião o substituir, ou até o ponto reiniciar.
Não há mais nada.

Nenhuma máquina atrás de NAT sabe o próprio endereço público: o roteador
reescreve isso na saída e o interior nunca vê o resultado. Alguém de fora precisa
contar, e furar um NAT é os dois lados mandando pacote ao mesmo tempo depois de
saberem para onde. O ponto de encontro é esse "alguém de fora".

Quando a conexão sobe, ele já não participa de nada: **o áudio e o texto nunca
passam por ele**, e o TLS 1.3 e o TOFU do ADR 0003 continuam ponta a ponta.

## O que ele fica sabendo

**Metadado.** Que endereço falou com que endereço, e quando. Isso é real, é o
custo que o ADR 0022 nomeia em voz alta, e não há como ter o degrau 4 sem ele.

**Nada do que é dito.** Ele não vê conteúdo nem chave; não teria o que fazer com
eles se visse, porque o que passa por ali são linhas curtas de texto com marcas
e endereços.

**Quase nada guardado.** Não há banco nem arquivo. A resposta a `ONDE` e a `LEVE`
continua sendo uma função que recebe um datagrama e devolve outro
(`seele_proto::encontro::responder`, sem `self` e sem estado). `MORO` e `QUEM`
passam por **o quarto**, que existe desde 2026-09-03: um mapa de
`marca → endereço`, em memória, com prazo de 60 segundos e teto de 4096 marcas
(ADR 0022, «O quarto, e por que a recusa foi revista»). O `MORO` escreve nele, e
a resposta a `QUEM` sai dele; a função sem estado, sozinha, cala o `QUEM` e
responde o `MORO` como um `ONDE`. Os 60 segundos são a validade de uma entrada,
contados do último `MORO`, e não o tempo que ela leva para sumir: vencida, ela
deixa de responder ao `QUEM`, e continua na memória até o quarto encher (aí o
que venceu é varrido), um `MORO` novo da mesma marca a substituir, ou o ponto
reiniciar. Nada sobrevive a um reinício, e nada vai a disco em nenhum momento.

O que o operador do ponto de encontro consegue **ler** por causa do quarto: que
uma marca está no ar, ou esteve até a última varredura, e em que endereço. Nada
além disso — a marca são os 16 primeiros dos 64 caracteres da impressão digital
do servidor (e uma letra), não um nome nem um endereço de e-mail, e quem falou
com quem não passa por ali, porque a conversa nunca passou. Por padrão o serviço
nem **imprime** isto — `--barulhento` liga a impressão para investigar um
problema, e avisa na saída o que passou a registrar, e mesmo com ele ligado o
quarto não é impresso.

**Ele não decide em quem se confia.** Quem chega com o bilhete do ponto de
encontro (o do link, ou o que a lista de conhecidos guardou) **e** com uma
impressão digital a conferir (a do link, ou a da lista) pergunta ao quarto onde
o servidor mora hoje, e lê a resposta: o endereço que ela traz entra na frente
dos guardados. Faltando uma das duas, não há pergunta. Um ponto de encontro
hostil, ou quem ocupou a marca, consegue mandar essa conexão para o endereço
errado, ou não avisar o anfitrião. O que ele não escolhe é a impressão digital:
a esperada, a do link ou a da lista, é conferida dentro do aperto de mão TLS,
antes de qualquer `Hello`. Se a chave não confere, o aperto falha ali, e o
convite, a senha e o apelido não chegam a quem atendeu com a chave errada. O
prejuízo é não entrar. Isso vale também onde esta máquina já fixou uma chave
naquele endereço: o endereço que o quarto devolve, quando entra na corrida, é
um candidato que a pessoa não escolheu, e ali um pino que confere não passa por
cima da impressão esperada (o adendo de 2026-09-29 ao ADR 0003). Se a chave
fixada ali mudou, a conexão é recusada do mesmo jeito. É o teto do que ele
consegue.

**O link fica com o seu endereço público dentro.** O bilhete (`enc=`) carrega o
endereço do ponto de encontro e o endereço público da sua escuta de avisos —
quem tem o link aprende o seu endereço sem precisar conectar. Quem conecta
aprenderia de qualquer forma; um link é para dar a quem se convida.

## Como não usar o nosso

Uma variável de ambiente, na máquina que **hospeda**:

```sh
# usar o seu
SEELE_ENCONTRO=encontro.suacasa.exemplo:8384 connection --hospedar

# não usar nenhum: o degrau 4 deixa de existir, e nenhum pacote sai daqui
# para ponto de encontro nenhum
SEELE_ENCONTRO=nao connection --hospedar
```

Quem entra não configura nada: o endereço do ponto de encontro viaja no próprio
`seele://`, dentro do `enc=`. É isso que faz o serviço ser trocável de verdade —
apontar para o seu não exige versão nova de nada, nem que a outra pessoa saiba
que ele mudou.

Com `SEELE_ENCONTRO=nao`, tudo o que funcionava continua funcionando: rede local,
IPv6 e porta no roteador não passam por ponto de encontro nenhum.

## Subir o seu

Precisa de uma máquina com **endereço público** — uma VPS de dez reais serve, e
sobra. Não precisa de banco, de disco, nem de domínio (um endereço IP no
`SEELE_ENCONTRO` funciona igual).

```sh
cargo build --release -p seele-encontro
./target/release/seele-encontro
```

Ele abre a porta **8384/UDP** em IPv4 e IPv6 e fica ali. Opções:

```
--porta N       em que porta atender (padrão 8384)
--rede-local    também apresentar endereços de rede local (só para experimentar)
--barulhento    imprimir quem falou com quem (é metadado; desligue depois)
```

Libere a porta no firewall da máquina:

```sh
# Linux com ufw
sudo ufw allow 8384/udp
```

E, para ele subir junto com a máquina, um serviço de systemd de nove linhas:

```ini
[Unit]
Description=SEELE — ponto de encontro
After=network.target

[Service]
ExecStart=/opt/seele/seele-encontro
Restart=always
DynamicUser=yes

[Install]
WantedBy=multi-user.target
```

`DynamicUser=yes` porque ele não precisa de usuário, de casa, nem de permissão
de escrita em lugar nenhum — não há o que persistir.

Reiniciá-lo no meio de uma apresentação custa a repetição de um datagrama de 96
bytes, e esvazia o quarto, que é o único estado dele. Cada anfitrião volta ao
quarto no reavivamento seguinte, a cada quinze segundos; até lá, quem pergunta
por ele não o acha no quarto e tenta os endereços que já tinha.

### Conferir que ele está mesmo no ar

`systemctl status` responde «o processo está de pé», que é uma pergunta
diferente de «alguém de fora alcança esta porta». Entre as duas há o firewall do
provedor, o firewall da máquina e a regra da porta — e cada um deles já quebrou
isto numa máquina de verdade.

De **outra** máquina:

```sh
cargo run -p seele-encontro --example sondar -- <endereço>:8384
```

Ele fala o protocolo mesmo, e não um `ping`: um serviço que responde a ICMP e
recusa datagrama de 96 bytes está tão quebrado quanto um desligado, e só isto
distingue os dois. As duas famílias são tentadas em separado, porque um ponto de
encontro que atende IPv4 e não IPv6 apresenta mal justamente os pares que mais
precisam dele.

Quando dá certo, ele imprime **o seu endereço visto de fora** — que é
literalmente o serviço que este ponto de encontro presta.

## O que ele **não** resolve

**NAT simétrico dos dois lados não fura.** Nesse caso o mapeamento do roteador
muda a cada destino, então o endereço que o ponto de encontro viu não é o
endereço por onde o outro lado chegaria. É por isso que a frase do degrau 4 diz
"deve funcionar" e não "funciona", e por isso a escada continua caindo para os
degraus de baixo com as saídas de sempre — encaminhar a porta à mão, ou uma VPN
de rede entre os dois.

A resposta a esse caso seria **retransmissão**: o tráfego passando pela máquina
de um terceiro. O ADR 0022 põe isso fora de escopo por decisão, e não é falta de
tempo — é o que separa este produto do que ele existe para não ser.

## Se você for mexer no código

O protocolo — quatro verbos que chegam ao ponto (`ONDE`, `LEVE`, `MORO` e
`QUEM`), a resposta `AQUI`, e uma função sem estado que decide o `ONDE` e o
`LEVE` — está em `crates/seele-proto/src/encontro.rs`. O quarto, que guarda o
`MORO` e responde ao `QUEM`, e o serviço estão em `crates/seele-encontro/`. O
lado de quem hospeda, em `crates/seele-server/src/alcance/encontro.rs`; o de quem
entra, em `crates/seele-core/src/encontro.rs`. O bilhete que viaja no link é o
`enc=` de `crates/seele-proto/src/uri.rs`.

Duas propriedades que o código guarda de propósito, e que valem ser mantidas se
alguém mexer:

- **O ponto de encontro nunca copia bytes de um pedido para uma resposta.** A
  resposta é montada campo a campo. O único pedaço do pedido que reaparece é a
  marca, que é alfanumérica e curta justamente por isso.
- **Todo datagrama tem 96 bytes, pedido e resposta.** Um refletor que responde
  mais do que recebe é uma arma apontada para quem nunca ouviu falar deste
  projeto. O enchimento é o que mantém o ganho em 1:1.
