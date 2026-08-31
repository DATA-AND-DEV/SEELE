# Happy Eyeballs — desenho, e o socket que decide quem pode correr

**Data:** 2026-08-31
**Estado:** aprovado em conversa; registrado no [ADR 0037](../../adr/0037-candidatos-do-convite-em-paralelo.md)

## O problema, medido

`docs/pendencias.md` nº 26 traz o número, e ele é brutal:

> 9,6 s queimados em três candidatos sem chance, com o quarto respondendo em
> 358 ms.

O convite leva até [`LIMITE_DE_ALVOS`] = 4 endereços desde a pendência nº 20, e
`Enlace::tentar_entre` os percorre **um de cada vez**. Cada endereço morto cobra
o prazo inteiro dele antes de o próximo começar. A pessoa espera dez segundos
por uma conexão que levaria menos de meio.

O RFC 8305 resolve isto há anos, e o nome é conhecido: dispara o primeiro
candidato, e a cada ~250 ms dispara o próximo **sem cancelar os anteriores**.
Fica com o primeiro aperto de mão que fechar.

## 0 · Por que isto não é «trocar o `for` por um `join_all`» — e por que quase virou pior

O laço de hoje não é ingênuo, e a série existe por uma razão escrita no fonte:

> Preparado antes do laço porque o socket tem de ser um só — o NAT mapeia por
> porta interna.

Cada tentativa faz `try_clone` do mesmo socket UDP, e `client.rs::local_endpoint`
constrói um `quinn::Endpoint` **novo** sobre a cópia. Dois `Endpoint` sobre o
mesmo socket dividem uma fila de recepção e **roubam pacote um do outro**: o
`Initial` de um candidato pode ser consumido e descartado pelo Endpoint de outro.
Isso não é lentidão, é incorreção, e falha de forma intermitente.

**A primeira versão deste desenho respondeu a isso dividindo os candidatos em
dois grupos**, por um predicado sobre endereço: correria quem não precisasse de
furo. Ela foi aprovada, implementada até a metade, e derrubada por dois testes de
`crates/seele-conformance/tests/furo.rs` — que não foram tocados, e que pegaram
duas coisas:

1. **a decisão já existia.** «Quem precisa de furo» é `e_publico(onde.ip())`,
   dentro de `avisar_pelo_candidato`. O predicado novo era uma segunda opinião,
   e onde as duas discordavam o aviso deixava de sair;
2. **a justificativa tinha um erro de fato.** O desenho afirmava que IPv6 nunca
   precisa de furo, «porque furar NAT não abre firewall». Firewall IPv6 doméstico
   é *stateful*: o pacote de saída que o aviso provoca abre o buraco de volta
   igual ao NAT.

Corrigido o predicado, a divisão deixava de entregar o que prometia — os
candidatos públicos, que são os lentos, ficariam em série e o ganho evaporava.

**A saída veio de conferir a premissa.** A restrição não é «uma conexão por
socket». É **um leitor por socket**, e um `quinn::Endpoint` é esse leitor: ele
dirige quantas conexões simultâneas se queira, demultiplexando por connection ID
— que é como o lado servidor do quinn atende a sala inteira com um Endpoint só.

## 1 · A decisão: um `Endpoint`, muitas conexões

O `Endpoint` sobe **uma vez**, sobre o socket compartilhado, e cada candidato é
um `Endpoint::connect` nele. Não há separação em grupos, não há predicado novo, e
`avisar_pelo_candidato` não muda — `e_publico` continua sendo a única opinião
sobre quem precisa de furo, e os avisos saem escalonados junto com os apertos de
mão que eles acompanham.

### A conta de furos muda de perfil

Hoje os avisos saem em série e param quando um candidato fecha. Correndo, os
quatro saem dentro de ~750 ms. Um candidato público custa `AVISOS_POR_CANDIDATO`
= 3 avisos, então quatro deles custam até **doze** furos quase simultâneos,
contra doze espalhados por dezesseis segundos. Cabe nos `FUROS_POR_JANELA` = 60
do anfitrião — mas o perfil deixou de ser gotejamento e virou rajada, e quem
mexer naquele teto precisa saber.

## 2 · A corrida

RFC 8305, com os nomes deste repositório:

- dispara o candidato 0 imediatamente;
- a cada `DEFASAGEM_ENTRE_CANDIDATOS` = **250 ms**, dispara o próximo, **sem
  cancelar** os que já estão no ar;
- o primeiro aperto de mão que fechar vence; os outros são cancelados;
- se todos falharem, o erro devolvido segue a regra que já existe — a primeira
  falha em que alguém respondeu vence a de quem nunca respondeu, porque ela diz
  mais.

**250 ms, e não menos**, porque é o número do RFC e porque a medição da
pendência nº 26 já o justifica: com quatro candidatos o último começa em 750 ms,
e o bom responde em 358 ms depois disso. Encurtar para 150 ms ganharia 300 ms e
poria mais apertos de mão simultâneos numa rede lenta, onde vários teriam
fechado sozinhos.

**Sem teto próprio de simultaneidade.** `LIMITE_DE_ALVOS` já é 4, então a
corrida nunca passa disso. Um segundo número para manter em dia, sem caso de uso
que o exija, é número a mais.

### Os prazos, dentro da corrida

`PRAZO_DE_CANDIDATO_DISTANTE` continua valendo para quem é de outra casa: ele
não existe para economizar tempo de parede — a corrida já faz isso — e sim para
**soltar o candidato**, encerrando a tentativa dele em vez de deixá-la pendurada
até o fim. `PRAZO_POR_CANDIDATO` vale para o resto.

`PRAZO_DA_PRIMEIRA_VOLTA` **não** entra aqui: ele existe para que a primeira
passada em série seja curta, e não há primeira passada em série. Ele continua
valendo no ramo que ficou serial.

## 3 · O conserto do TOFU

A limpeza de pin órfão dos perdedores roda **depois** de o vencedor estar
estabelecido, e **pula toda `chave_do_pin` que o vencedor escreveu**.

Concretamente: a corrida guarda, por candidato, o par
`(chave_do_pin, fixado_antes)`. Quando alguém vence, a limpeza percorre os
perdedores e ignora aquele cuja chave é a do vencedor. Quando ninguém vence,
todos são limpos, como hoje.

É uma dúzia de linhas, e sem elas ganhar a corrida apagaria a própria confiança.

## 4 · O que se ganha

| | hoje | depois |
|---|---|---|
| Três candidatos mortos + um bom (pendência nº 26) | **9,6 s** | ~**1,1 s** |
| Roteador sem hairpin | falha até chegar ao candidato certo | coberto, de graça |
| Dependência nova | — | nenhuma |

O caso do hairpin sai de brinde e vale ser dito: um roteador que não devolve o
tráfego de dentro para o próprio endereço público faz o candidato público falhar
para quem está na mesma casa. Em série, essa casa espera o público morrer antes
de tentar o local. Em paralelo, o local fecha em milissegundos e ninguém repara.

## 5 · O que este desenho não faz

- **Não abre firewall IPv6.** É a pendência nº 26 e é o PCP; este desenho apenas
  para de *esperar* por endereços que o firewall bloqueia. Os dois se somam: com
  PCP os IPv6 passam a responder, e com a corrida quem não responde não custa.
- **Não mexe na série de quem precisa de furo**, pelas razões da seção 1.
- **Não muda o convite.** Os endereços já viajam desde a pendência nº 20.
- **Não toca no `alcance` do servidor.** Quem monta a lista de candidatos
  continua como está; o que muda é como quem recebe a percorre.

## 6 · Como se prova

- **A escolha de quem corre**, pura: dado um conjunto de candidatos, quais vão
  para a corrida e quais para a série. É a função que carrega a regra da seção 1
  e é testável sem socket nenhum.
- **A defasagem**, com relógio de teste: que o segundo candidato começa 250 ms
  depois do primeiro e **não** espera o primeiro terminar.
- **O primeiro que fecha vence**, com um candidato lento e um rápido em ordem
  invertida — o lento primeiro na lista, e ainda assim o rápido ganha.
- **Ninguém morre por causa do vencedor**: as tentativas perdedoras são
  canceladas, e nenhuma delas continua escrevendo em lugar nenhum.
- **O pin do vencedor sobrevive**, com dois candidatos de mesma `chave_do_pin`.
  É o teste que mais importa aqui: sem o conserto da seção 3 ele reprova, e o
  que ele guarda é a confiança de primeiro contato do ADR 0003.
- **A série de quem precisa de furo continua intacta**: os três testes de
  `furo.rs` continuam verdes sem serem tocados. Se algum precisar mudar, a regra
  da seção 1 foi violada.

## 7 · Riscos

- **A defasagem de 250 ms não foi medida nesta rede.** Vem do RFC. O que a
  confirma é a mesma medição da pendência nº 26, refeita com a corrida no lugar.
- **Quatro apertos de mão simultâneos custam quatro `Endpoint`** do quinn por um
  instante. São efêmeros e morrem com o cancelamento, mas é alocação que antes
  não existia, e numa máquina fraca vale medir.
- **A regra da seção 1 é um predicado sobre endereço**, e predicados sobre
  endereço erram em rede exótica — um `/16` à mão, uma VPN capturando a rota. O
  erro é benigno nos dois sentidos: quem for classificado como «precisa de furo»
  sem precisar apenas fica em série, como hoje; quem for classificado ao
  contrário perde o aviso e falha como falharia sem bilhete nenhum. Nenhum dos
  dois é regressão sobre o estado atual daquele candidato.
