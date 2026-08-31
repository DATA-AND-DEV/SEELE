# ADR 0037 — Um `Endpoint`, muitas conexões: os candidatos do convite correm juntos

**Estado:** aceito
**Data:** 2026-08-31
**Reescrito em 2026-08-31**, sobre uma primeira redação que continha um erro de
fato. Ver «O que a primeira redação dizia, e por que estava errada», no fim.

O convite carrega até quatro endereços desde a pendência nº 20, e
`Enlace::tentar_entre` os percorre **um de cada vez**. A pendência nº 26 mediu o
que isso custa:

> 9,6 s queimados em três candidatos sem chance, com o quarto respondendo em
> 358 ms.

Este ADR troca a série por uma corrida — RFC 8305, «Happy Eyeballs».

## Contexto

O [ADR 0022](0022-alcancar-um-dogma-pela-internet.md) desenhou a escada de
alcance, e o degrau 4 dela é o furo de NAT coordenado por um ponto de encontro.
O laço que percorre os candidatos foi ganhando disciplina em cima disso: aviso
por candidato colado ao aperto de mão (porque o furo dura menos de um segundo),
prazo curto para candidato de outra casa, e duas voltas — a segunda só para quem
ficou sem tempo, conserto do commit `9750f00`.

A pergunta que este ADR teve de responder foi: **por que a série existe?** Não
por preguiça — havia uma razão escrita no fonte, em `crate::encontro`:

> Preparado antes do laço porque o socket tem de ser um só — o NAT mapeia por
> porta interna.

Cada tentativa chama `Batida::emprestar_socket`, que é um `try_clone` do mesmo
socket UDP, e `client.rs::local_endpoint` constrói um `quinn::Endpoint` **novo**
sobre essa cópia. Dois `Endpoint` sobre o mesmo socket têm uma fila de recepção
só e roubam pacote um do outro: o `Initial` de um candidato pode ser consumido e
descartado pelo Endpoint de outro.

## Decisão

**Um `Endpoint` sobre o socket compartilhado, e todas as conexões correm nele.**

A restrição real nunca foi «uma tentativa por vez». Era «um leitor por socket» —
e um `quinn::Endpoint` **é** esse leitor. Um Endpoint dirige quantas conexões
simultâneas se queira: ele demultiplexa por connection ID, que é exatamente como
o lado servidor do quinn atende a sala inteira com um Endpoint só.

Então a separação em grupos não precisa existir. Não há predicado novo, não há
«quem corre e quem espera». O Endpoint sobe uma vez, e `Endpoint::connect` é
chamado para cada candidato com a defasagem do RFC 8305:

- dispara o candidato 0 imediatamente;
- a cada `DEFASAGEM_ENTRE_CANDIDATOS` = **250 ms**, dispara o próximo, sem
  cancelar os anteriores;
- o primeiro aperto de mão que fechar vence; os outros são cancelados.

**250 ms**, que é o número do RFC. Com quatro candidatos o último começa em
750 ms, e a medição da nº 26 diz que o bom responde em 358 ms depois disso.

**Sem teto próprio de simultaneidade:** `LIMITE_DE_ALVOS` já é 4.

### O aviso continua por candidato, e quem decide continua sendo `e_publico`

`avisar_pelo_candidato` não muda, e o predicado dele — `e_publico(onde.ip())` —
continua sendo a **única** opinião sobre quem precisa de furo. Correndo, os
avisos saem escalonados junto com os apertos de mão que eles existem para
acompanhar, que é a propriedade que o commit da coordenação comprou e que este
ADR não pode desfazer.

### A conta de furos muda de perfil, e isso precisa estar escrito

Hoje os avisos saem em série e param assim que um candidato fecha. Correndo, os
quatro saem dentro de ~750 ms. O comentário em `avisar_pelo_candidato` já conta
que um candidato público custa `AVISOS_POR_CANDIDATO` = 3 avisos, não um, e que
`FUROS_POR_JANELA` subiu de 20 para 60 do lado do anfitrião por causa disso.

Quatro candidatos públicos correndo custam até **doze** furos quase simultâneos,
contra doze espalhados por dezesseis segundos. Cabe nos 60 da janela, e a janela
é por dez segundos — mas o perfil deixou de ser «gotejando» e passou a ser
«rajada», e quem for mexer no teto do anfitrião precisa saber disso.

### A limpeza de pin órfão passa a depender do vencedor

`desfazer_pin_orfao` apaga o pin que um aperto de mão cancelado escreveu, e a
regra «só apaga o que este aperto escreveu» é exata em série e **falsa em
paralelo**: dois candidatos podem compartilhar `chave_do_pin`, que é
`host:porta` do nome do convite, e endereços alternativos do mesmo nome colidem.
Cancelar o perdedor depois de o vencedor ter escrito encontraria
`fixado_antes == None` e `pinned() == Some`, e apagaria o pin do vencedor.

A limpeza dos perdedores roda **depois** do vencedor e **pula a chave dele**. O
estrago que isso evita não é uma conexão perdida: é a confiança de primeiro
contato do [ADR 0003](0003-certificados-tofu.md) desfeita em silêncio.

## Alternativas

- **Um `Endpoint` por candidato, todos sobre cópias do mesmo socket.** É o que a
  primeira redação deste ADR quis evitar dividindo os candidatos em grupos.
  Recusada porque o defeito é real e o remédio é errado: dois leitores na mesma
  fila se roubam, e a resposta certa é ter um leitor, não ter menos corredores.
- **Separar candidatos em «corre» e «não corre» por um predicado sobre
  endereço.** Foi a primeira redação. Recusada por duas razões, e a segunda é a
  que mata: ela duplicava a decisão que `e_publico` já toma, e — com o predicado
  correto — deixaria em série justamente os candidatos públicos, que são os
  lentos. O ganho evaporava.
- **Defasagem de 150 ms.** Ganharia ~300 ms com quatro candidatos, ao custo de
  mais apertos de mão e mais avisos simultâneos. Recusada por não ter medida a
  favor; 250 ms é o número do RFC.

## Consequências

Três candidatos mortos e um bom deixam de custar 9,6 s e passam a custar ~1,1 s.
O caso do roteador sem hairpin sai de brinde: em série, a casa cujo endereço
público não volta para dentro esperava ele morrer antes de tentar o local; em
paralelo o local fecha em milissegundos.

Nenhuma dependência nova: `tokio::task::JoinSet` já vem com `features = ["full"]`.

O custo é o perfil de furos virando rajada, escrito acima, e quatro conexões
QUIC simultâneas por um instante onde antes havia uma. Elas morrem com o
cancelamento.

Este ADR **não** abre firewall IPv6 — isso é a pendência nº 26 e é o PCP. Ele faz
com que endereços que não respondem parem de custar tempo.

## O que a primeira redação dizia, e por que estava errada

Fica registrado porque o erro é instrutivo e porque o ADR foi aprovado sobre ele.

A primeira redação dividia os candidatos em dois grupos e afirmava:

> **IPv6, em qualquer forma, não precisa de furo.** Não há tradução de endereço;
> há firewall, e furar NAT não abre firewall.

**A segunda metade é falsa.** Firewall IPv6 doméstico é *stateful*: o pacote de
saída que o anfitrião manda ao ser avisado abre o buraco de volta exatamente como
o NAT abre. O aviso serve para IPv6 tanto quanto para IPv4 — e é por isso que
`e_publico` sempre incluiu os dois, sem distinguir família.

O erro tinha duas consequências, e as duas foram pegas por
`crates/seele-conformance/tests/furo.rs` sem que nenhum teste fosse tocado:
o aviso deixava de sair para candidatos que precisavam dele, e a decisão sobre
quem precisa passava a existir em dois lugares que discordavam.

Corrigido o predicado, a divisão em grupos deixava de entregar o que prometia:
os públicos — os lentos — ficariam em série. Foi ao procurar uma saída para isso
que a premissa da divisão foi conferida e caiu: a restrição era um Endpoint por
socket, não uma conexão por socket.

## Custo de reverter

**Baixo.** A série continua existindo no arquivo para o caso de um candidato só,
e os prazos, as duas voltas e a lista `merece_segunda` não mudaram de forma.
Reverter é voltar a chamar o laço serial para a lista inteira.
