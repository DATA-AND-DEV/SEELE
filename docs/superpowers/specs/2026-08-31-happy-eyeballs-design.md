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

## 0 · Por que isto não é «trocar o `for` por um `join_all`»

O laço de hoje não é ingênuo. Ele coordena três coisas, e duas resistem a
paralelismo. Este documento existe principalmente por causa delas.

### O socket é um só, e é de propósito

Quando há bilhete de ponto de encontro, cada tentativa chama
`Batida::emprestar_socket`, que é um `try_clone` do **mesmo** socket UDP. O
comentário no fonte diz por quê:

> Preparado antes do laço porque o socket tem de ser um só — o NAT mapeia por
> porta interna.

Um `try_clone` dá dois descritores para o **mesmo** socket, com uma fila de
recepção só. Dois `Endpoint` do quinn lendo dali **roubam pacote um do outro**:
o `Initial` do candidato A pode ser consumido e descartado pelo Endpoint do
candidato B, que não sabe o que fazer com ele.

Correr candidatos que dividem socket não é lento. É **incorreto**, e o modo de
falhar é o pior possível: intermitente, dependente de quem ganhou a corrida do
`recv`, e indistinguível de rede ruim.

### O aviso e o aperto de mão saem colados

`avisar_pelo_candidato` existe porque o furo que ele provoca do outro lado dura
menos de um segundo. Isso **sobrevive** ao paralelismo — cada candidato leva o
seu aviso, e a janela do anfitrião é de sessenta por dez segundos, folgada para
quatro.

O que não sobrevive é o que existe hoje: com bilhete, o aviso sai para **todo**
candidato, inclusive os que não têm NAT nenhum para furar.

### O TOFU tem uma armadilha sob concorrência

`desfazer_pin_orfao` apaga o pin que um aperto de mão cancelado escreveu, e a
regra dele é «só apaga o que este aperto escreveu»:

```rust
if fixado_antes.is_none() && pins.pinned(chave_do_pin).is_some() {
    pins.unpin(chave_do_pin);
}
```

Em série isso é exato. **Em paralelo, não é.** Dois candidatos podem
compartilhar `chave_do_pin` — ela é `host:porta` do nome do convite, então
endereços alternativos resolvidos do mesmo nome colidem. Cancelar o perdedor
depois de o vencedor ter escrito encontraria `fixado_antes == None` e
`pinned() == Some`, e **apagaria o pin do vencedor**.

O estrago não é uma conexão perdida: é a confiança de primeiro contato sendo
desfeita em silêncio, num caminho que o ADR 0003 existe para tornar durável.
Correr sem tratar isto troca oito segundos por um defeito de segurança calado.

## 1 · A regra: o socket decide quem corre

**Corre quem não divide socket. Fica em série quem divide.**

A regra é sobre o recurso, e não sobre o tipo do candidato — o que evita uma
segunda taxonomia de endereços ao lado da que o `alcance` já tem no servidor.

Hoje `emprestar()` é chamado para todo candidato quando há bilhete. Passa a ser
chamado **só para quem precisa do furo**. A consequência é a que interessa:

- **quem não precisa de furo** — rede local, IPv6 direto, endereço público de
  quem não está atrás de NAT — ganha socket próprio e **corre**, mesmo havendo
  bilhete;
- **quem precisa** continua em série, no socket compartilhado, com os prazos e
  as duas voltas exatamente como estão.

E é justamente no primeiro grupo que os 9,6 s estão sendo queimados: os três
candidatos mortos da pendência nº 26 são endereços **IPv6**, e IPv6 não tem NAT
para furar. O que os bloqueia é o firewall do roteador, que é assunto do PCP e
não do ponto de encontro.

### Quem precisa de furo, escrito como predicado

Um candidato precisa do aviso quando é um **IPv4 público** — o caso do
anfitrião atrás de NAT, que é o degrau 4 do ADR 0022. Não precisam:

- endereços privados e CGNAT, que `e_privado` já reconhece — ou são desta rede,
  e aí não há NAT no meio, ou são de outra casa, e aí `e_de_outra_casa` já os
  trata com prazo curto porque ninguém vai responder;
- **IPv6**, em qualquer forma. Não há tradução de endereço; há firewall, e furar
  NAT não abre firewall. Avisar por um candidato IPv6 gasta janela do anfitrião
  por um caminho que o aviso não ajuda — que é a mesma frase que o código já usa
  para justificar não avisar por candidato que falhou;
- laço local.

### O que **não** muda

A série de quem precisa de furo fica intacta: os dois prazos, as duas voltas, e
a lista `merece_segunda`. Aquilo foi consertado com medida e com três testes em
`crates/seele-conformance/tests/furo.rs` no commit `9750f00`, e a corrida não a
melhora em nada — um candidato que exige furo e não responde não responde mais
rápido por ter vizinhos correndo ao lado.

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
