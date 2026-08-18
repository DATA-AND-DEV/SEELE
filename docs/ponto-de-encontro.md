# O ponto de encontro, e como subir o seu

O degrau 4 do ADR 0022 — furo de NAT — precisa de um serviço minúsculo que
apresente as duas máquinas uma à outra. Este documento é sobre ele: **o que ele
faz, o que ele fica sabendo, e como pôr um seu no ar.**

Se você só quer entender por que "não conecta", o documento é o outro:
[`alcance-pela-internet.md`](alcance-pela-internet.md).

## O que ele faz, em uma frase

Ele diz a quem manda um pacote qual é o endereço de onde aquele pacote veio — ou
conta isso a um terceiro endereço, quando quem manda pede.

É só isso. Não há mais nada.

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
eles se visse, porque o que passa por ali são três linhas de texto com endereços.

**Nada guardado.** Ele não tem banco, arquivo, nem tabela em memória: a decisão
inteira dele é uma função que recebe um datagrama e devolve outro
(`seele_proto::encontro::responder`, sem `self` e sem estado). Por padrão ele nem
**imprime** quem falou com quem — `--barulhento` liga isso para investigar um
problema, e avisa na saída o que passou a registrar.

**Ele não decide para onde ninguém conecta.** Quem recebe um convite nunca lê
resposta nenhuma do ponto de encontro: os endereços que tenta vieram do
`seele://`, e a impressão digital que confere também. Um ponto de encontro
hostil consegue não avisar o anfitrião. É o teto do que ele consegue.

**O link fica com o seu endereço público dentro.** O bilhete (`enc=`) carrega o
endereço do ponto de encontro e o endereço público da sua escuta de avisos —
quem tem o link aprende o seu endereço sem precisar conectar. Quem conecta
aprenderia de qualquer forma; um link é para dar a quem se convida.

## Como não usar o nosso

Uma variável de ambiente, na máquina que **hospeda**:

```sh
# usar o seu
SEELE_ENCONTRO=encontro.suacasa.exemplo:8384 plug --hospedar

# não usar nenhum: o degrau 4 deixa de existir, e nenhum pacote sai daqui
# para ponto de encontro nenhum
SEELE_ENCONTRO=nao plug --hospedar
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
bytes. Não há estado para perder porque não há estado.

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

O protocolo — três linhas de texto e uma função sem estado — está em
`crates/seele-proto/src/encontro.rs`. O serviço, em `crates/seele-encontro/`. O
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
