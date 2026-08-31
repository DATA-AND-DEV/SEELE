# ADR 0037 — Os candidatos do convite são tentados em paralelo, quando não dividem socket

**Estado:** aceito
**Data:** 2026-08-31

O convite carrega até quatro endereços desde a pendência nº 20, e
`Enlace::tentar_entre` os percorre **um de cada vez**. A pendência nº 26 mediu o
que isso custa:

> 9,6 s queimados em três candidatos sem chance, com o quarto respondendo em
> 358 ms.

Este ADR troca a série por uma corrida — RFC 8305, «Happy Eyeballs» — **onde a
corrida é segura**, e escreve por que ela não é segura em todo lugar.

## Contexto

O [ADR 0022](0022-alcancar-um-dogma-pela-internet.md) desenhou a escada de
alcance, e o degrau 4 dela é o furo de NAT coordenado por um ponto de encontro.
O laço que percorre os candidatos foi ganhando disciplina em cima disso: aviso
por candidato colado ao aperto de mão (porque o furo dura menos de um segundo),
prazo curto para candidato de outra casa, e duas voltas — a segunda só para quem
ficou sem tempo, conserto do commit `9750f00`.

Nada disso é gordura, e é por isso que este ADR não substitui aquele laço: ele o
divide em dois.

## Decisão

**Corre quem não divide socket. Fica em série quem divide.**

A regra é sobre o recurso e não sobre o tipo do candidato, o que evita uma
segunda taxonomia de endereços ao lado da que o `alcance` já mantém no servidor.

Três partes:

**1 · O socket compartilhado passa a ser emprestado só a quem precisa de furo.**
Hoje, havendo bilhete, toda tentativa chama `Batida::emprestar_socket`, que é um
`try_clone` do mesmo socket UDP — «o NAT mapeia por porta interna». Um
`try_clone` dá dois descritores para uma fila de recepção só, e dois `Endpoint`
do quinn lendo dali **roubam pacote um do outro**: o `Initial` de um pode ser
consumido e descartado pelo outro. Correr candidatos que dividem socket não é
lento, é **incorreto** — e falha de forma intermitente, dependente de quem ganhou
a corrida do `recv`, indistinguível de rede ruim.

Quem não precisa de furo — rede local, IPv6 em qualquer forma, endereço público
de quem não está atrás de NAT — passa a receber socket próprio e a correr, mesmo
havendo bilhete. É onde os 9,6 s estão: os três candidatos mortos da pendência nº
26 são **IPv6**, e IPv6 não tem NAT para furar. O que os bloqueia é o firewall do
roteador, que é assunto do PCP.

**2 · A corrida é o RFC 8305, com defasagem de 250 ms.** Dispara o primeiro,
dispara o próximo 250 ms depois sem cancelar os anteriores, fica com o primeiro
aperto de mão que fechar. Sem teto próprio de simultaneidade: `LIMITE_DE_ALVOS`
já é 4.

**3 · A limpeza de pin órfão dos perdedores roda depois do vencedor, e pula a
chave dele.** `desfazer_pin_orfao` apaga o pin que um aperto de mão cancelado
escreveu, e a regra «só apaga o que este aperto escreveu» é exata em série e
**falsa em paralelo**: dois candidatos podem compartilhar `chave_do_pin`, que é
`host:porta` do nome do convite, e endereços alternativos do mesmo nome colidem.
Cancelar o perdedor depois de o vencedor ter escrito encontraria
`fixado_antes == None` e `pinned() == Some`, e apagaria o pin do vencedor.

O estrago não seria uma conexão perdida: seria a confiança de primeiro contato do
[ADR 0003](0003-certificados-tofu.md) desfeita em silêncio. Sem esta parte, o ADR
trocaria oito segundos por um defeito de segurança calado.

## Alternativas

- **Correr tudo, inclusive quem divide socket.** É o que uma leitura rápida do
  RFC 8305 sugere, e é o que quase foi feito. Recusada pelo motivo da parte 1: o
  compartilhamento do socket é a condição do furo funcionar, e dois leitores na
  mesma fila se roubam. O defeito seria intermitente e sem sintoma próprio.
- **Um socket por candidato também no furo, abrindo mão do compartilhamento.**
  Cada tentativa teria porta interna diferente, e o mapeamento de NAT que o
  anfitrião furou não valeria para nenhuma delas. É desligar o degrau 4 para
  ganhar tempo num degrau que não precisa.
- **Correr, e resolver o pin dando `chave_do_pin` distinta por candidato.**
  Resolveria a colisão e quebraria o que a chave existe para fazer: dois
  endereços do mesmo servidor **devem** compartilhar pin, ou reconectar por um
  alternativo pediria confiança de novo. A colisão é a intenção; o que precisava
  de conserto era a limpeza.
- **Defasagem de 150 ms.** Ganharia ~300 ms com quatro candidatos, ao custo de
  mais apertos de mão simultâneos numa rede lenta, onde vários teriam fechado
  sozinhos. Recusada por não ter medida a favor: 250 ms é o número do RFC.

## Consequências

Três candidatos mortos e um bom deixam de custar 9,6 s e passam a custar ~1,1 s.
O caso do roteador sem hairpin — o público falha para quem está na mesma casa —
sai de brinde: em série aquela casa esperava o público morrer antes de tentar o
local; em paralelo o local fecha em milissegundos.

Nenhuma dependência nova. Os endereços já viajam no convite.

O custo é quatro `Endpoint` do quinn simultâneos por um instante, onde antes
havia um. São efêmeros e morrem com o cancelamento, mas é alocação que não
existia, e numa máquina fraca vale medir.

Este ADR **não** abre o firewall IPv6, que é a pendência nº 26 e é o PCP. Ele faz
com que endereços bloqueados parem de custar tempo. Os dois se somam: com PCP os
IPv6 respondem, e com a corrida quem não responde não cobra por isso.

## Custo de reverter

**Baixo.** A série de quem precisa de furo não foi tocada — continua com os dois
prazos, as duas voltas e a lista `merece_segunda`, e os três testes de
`crates/seele-conformance/tests/furo.rs` seguem verdes sem serem editados. Isso é
proposital, e é o que faz reverter ser apagar a corrida em vez de reconstruir o
que ela substituiu: o caminho antigo nunca deixou de existir.
