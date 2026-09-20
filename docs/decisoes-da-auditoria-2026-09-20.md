# Decisões tomadas ao executar a auditoria de 20/09/2026

Data: 20/09/2026. Escrito **ao final do desenvolvimento**, como o pedido exigia.

> **Esta entrega é um candidato em validação, e não uma API concluída.** A
> [revisão](revisao-entrega-api4-2026-09-20.md) do checkout `7ca66cc` encontrou
> seis defeitos reproduzidos além do que este arquivo declarava pendente, e
> recusou — com razão — a frase «todos os 33 achados concluídos». A revisão
> seguinte, do checkout `26ad0c2`, encontrou três caminhos do mesmo contrato
> que a bancada não alcançava.
>
> A validação nativa do checkout `d96d71a` percorreu as jornadas na janela e
> encontrou mais seis. O reteste de `c4fe3ea` confirmou os consertos e
> **recusou a aparência** dos três MODs oficiais; o que essa volta mudou está
> em [aparencia-dos-mods-2026-09-20.md](aparencia-dos-mods-2026-09-20.md),
> inclusive a causa que estava embaixo das outras — as classes de um MOD nunca
> desenharam nada neste produto, porque a CSP descarta um `<style>` criado por
> script.
>
> A seção 12 registra os seis primeiros, a 13 é a matriz **implementado /
> parcial / pendente**, a 15 registra os três seguintes — inclusive uma
> afirmação da seção 12 que estava errada — e a 16 registra os da validação
> nativa.
>
> A conclusão da revisão continua valendo para o que ainda não aconteceu: nada
> aqui foi observado no aplicativo nativo com as três atividades juntas.

Este arquivo registra o que foi decidido enquanto a
[auditoria de UX/UI](auditoria-ux-ui-mods-2026-09-20.md) e o
[plano da API de criação de interfaces](plano-api-mods-criacao-de-interfaces.md)
eram executados. Ele não repete o que a auditoria diz nem o que os commits
dizem: registra **as escolhas que tiveram alternativa**, o que cada uma custa, e
— no fim — o que ficou por fazer, com o motivo.

Partida conferida e não suposta: `v0.12.1`, commit `655a137`, publicada em
20/09/2026 às 04:40 UTC. Confirmada pelo comando do `CLAUDE.md` antes de
qualquer diagnóstico.

---

## 1. Escopo: o que foi tratado como «tudo que está descrito na auditoria»

A auditoria tem 33 achados de produto (U01–U33), cinco defeitos funcionais com
causa nomeada, uma proposta de reorganização das configurações, seis riscos «a
reproduzir» e oito jornadas de aceite.

**Decisão:** tratar como entregável tudo o que é acionável por código neste
repositório e nos irmãos, e declarar explicitamente o que não é. O que não é
está na seção 9, com o motivo de cada um.

**Alternativa recusada:** entregar só as etapas A e B do §14 do plano e chamar
de pronto. O §15 do plano proíbe isso por escrito — «Não chamar etapas A/B de
"API completa"» —, e o pedido era executar até o fim.

---

## 2. A decisão de arquitetura: API 4 como conjunto, e não como ruptura

O §13 do plano pedia «API 4 com compatibilidade explícita de leitura/execução da
API 3». A conferência de manifesto era igualdade:
`manifest.api == MOD_API_VERSION`.

**Decisão:** `APIS_ACEITAS = [4, 3]`, com capacidades por versão
(`capacidades_da_api`), aplicadas pelo prelúdio do executor.

**Por quê.** A API 3 foi uma ruptura porque ela **tirou** coisas — o ADR 0049
explica —, e ali a igualdade estava certa. A API 4 não tira nada: ela acrescenta
superfícies, contribuições, composição e estilos sobre o mesmo executor, a mesma
ponte e o mesmo renderer. Subir a constante sozinha recusaria todo pacote
publicado no mesmo instante, inclusive os três oficiais na máquina de quem já os
tinha instalado.

**O que isso obrigou.** O indexador precisou acompanhar: `VERSAO_DA_API = 4` e
`APIS_ACEITAS = (4, 3)`, e o catálogo passou a publicar o conjunto além do teto.
Um cliente que lesse só `api_oferecida` concluiria que um pacote de API 3 na
lista é velho demais, e ele não é.

**Como a degradação foi resolvida.** Ausência, e não recusa em tempo de
execução. Um pacote de API 3 recebe um `SeeleUI` em que `superficies`
simplesmente **não existe**. A alternativa — um método que existisse e sempre
falhasse — faria o autor descobrir o problema dentro de um `catch`, com uma
string, em produção. Um método ausente é um `TypeError` na primeira linha que o
chama, com pilha.

---

## 3. Superfícies: por que não bastava aumentar a faixa

**Decisão:** quatro tipos — `pagina`, `painel`, `dialogo`, `aviso` — com ciclo de
vida próprio, num módulo novo (`mods-superficies.js`).

**Alternativa recusada:** aumentar `max-height` da faixa. O §15 do plano a
nomeia: «Aumentar `max-height` da faixa não cria uma API de aplicações.» O que
faltava não era espaço — era título, foco, rota, estado de alteração e saída.

**Onde a página mora, e o que isso custa.** Na mesma célula de grade da
conversa, por cima dela. A alternativa era uma quinta coluna, e a grade de
`.paineis` é declarada uma a uma: um quinto filho cairia numa segunda linha —
a mesma armadilha que fez a faixa dos MODs nascer fora de `<main>`. O custo
está registrado no ADR 0052: **um MOD pode cobrir a conversa**. A mitigação é o
cabeçalho persistente com o voltar do produto, e ela é mitigação, não
impossibilidade.

**A saída é do produto, e isso não é negociável.** O botão, o texto e o comando
são deste lado. Um MOD que não desenhe botão nenhum, que trave no meio da
montagem ou que sature a fila continua sendo uma superfície de onde se sai.

**Modal torna o resto inerte, e não só escuro.** `inert` nos irmãos, e não só
um véu. Escurecer sem tornar inerte é a armadilha clássica: parece modal e
responde a Tab.

---

## 4. Contribuições: pontos semânticos, nunca seletores

**Decisão:** `SeeleUI.contribuicoes.registrar({ ponto, modo, alvo, prioridade })`
sobre dez pontos nomeados, com handle revogável.

**Alternativa recusada:** dar ao MOD um caminho até o documento — um
`querySelector` restrito, um slot por nome de classe. Um seletor é um contrato
que ninguém escreveu: no dia em que a conversa renomear `.roster-linha`, todo MOD
publicado quebra junto, e nenhum dos dois lados combinou isso.

**A disputa foi resolvida por decisão visível, e não por ordem de chegada.** Duas
substituições do mesmo ponto se resolvem por prioridade declarada e, empatadas,
por ordem de registro; a escolha de quem administra ganha de qualquer número —
uma pessoa decidiu, e um número não. A gestão de MODs **mostra** a disputa, com
os dois nomes e «usar apresentação padrão» ao lado. Só a escolha seria um botão
para um problema que ninguém consegue ver; só a disputa seria contar um problema
sem dar a saída.

**A preferência mora na máquina, e não no servidor.** `localStorage`, por
destino. A alternativa seria o servidor decidir a apresentação da tela de quem
entra nele, e isso é uma decisão sobre a tela de outra pessoa.

**O limite do cartão subiu de 24 para 64 nós.** Um cartão que **substitui** a
identidade precisa de faixa, retrato sobreposto, nome, pronomes, status e
distintivos; vinte e quatro nós não chegam lá, e o pacote descobria isso como
uma recusa sem explicação. O que continua de fora é tudo que recebe foco — a
linha do roster já tem um botão do produto.

---

## 5. Estilos: a fronteira dita pelo que ela é

**Decisão:** propriedades declaradas por categoria, validadas uma a uma, montadas
de partes conferidas. Classes compiladas numa folha presa à raiz da superfície,
com escopo que o produto escreve. Estados e consultas **de contêiner**.

**Alternativa recusada, e é a que mais importa:** uma função que receba CSS e
tente limpá-lo. Limpeza por substituição de texto é uma corrida contra o
analisador do navegador, e quem escreve o analisador não sabe que esta corrida
existe. Não há uma função dessas neste código, e não deve passar a haver.

**Alternativa recusada 2:** Shadow DOM como fronteira. O plano já avisa —
«Shadow DOM é encapsulamento visual, não sandbox de execução» —, e adotá-lo
agora traria contrato novo (`:host`, herança, popovers, fontes) sem resolver
nada que o escopo de folha não resolva. Fica em aberto para quando um protótipo
medir o custo nas plataformas.

**`specs/07-estetica.md` foi emendado, e não afrouxado.** A regra de canto reto
e ausência de sombra é sobre as telas do SEELE e continua inteira. O que ela não
é — e estava sendo lida como se fosse — é uma regra sobre o que um MOD desenha
na superfície dele. A emenda escreve a fronteira em quatro cláusulas: não
alcança fora, não é texto de CSS, não busca bytes na rede, não sobrevive à saída.

---

## 6. Os três MODs: o que cada um deixou de ser

**MESA.** O defeito funcional (U25) era o contrato de escrita: `escrever` não
mandava `nonce` nem `revision`, e o servidor recusa os dois antes de qualquer
escrita. **A decisão que importa não foi o conserto — foi o teste.**
`world().call` completava os dois campos antes de chamar o servidor, então a
suíte provava um caminho que o produto não percorre. O harness passou a
transportar o pedido intacto, e o que semeia estado se chama `escritaDireta`.
Reverter o conserto do cliente hoje quebra catorze casos com o diagnóstico
certo; isso foi verificado.

O mapa deixou de ser mídia ao lado do canvas e virou o **fundo** dele. A
alternativa era alinhar dois planos por coordenada, e ninguém os alinhava.

A trilha passou a tocar. O pacote traz três laços curtos de 30 s a 11 kHz,
gerados e declarados no manifesto. **Alternativa recusada:** dizer que a trilha
«aguarda suporte» — era o que a versão anterior fazia, e é o defeito que este
repositório nomeia como o mais caro. **Custo aceito:** 1,9 MB no pacote, e três
arquivos de áudio sintético que ninguém vai chamar de trilha sonora. Eles são
ambientação audível, e é o que o achado pedia.

Junto veio um defeito que a mudança revelou: `package.mjs` copiava só três
arquivos e ignorava `arquivos` do manifesto. O pacote teria sido publicado com a
trilha declarada e ausente.

**PERFIS.** `accent` e `effect` eram lidos, gravados e não apareciam na árvore
visual. Agora a cor é a borda do retrato e o efeito é a animação dele, e o
cartão **substitui** a apresentação. O editor virou diálogo com prévia lado a
lado, e a prévia usa **o mesmo desenho do cartão** — duas funções de desenho
seriam duas verdades sobre o mesmo cartão.

O rascunho passou a ser por entidade (U26). **Decisão:** fechar guarda, e só
`DESCARTAR` — explícito e confirmado uma vez — joga fora. A alternativa
(perguntar ao fechar) treina a apertar sem ler.

**ESTILO.** A afirmação obsoleta (U24) saiu, e no lugar dela entrou o
**resultado** da aplicação, com o erro quando há um. A prévia local separada de
publicar (U23) é a metade que faltava: antes o único jeito de ver uma cor era
publicá-la para todo mundo.

---

## 7. O laboratório: consertado porque ele ensina

A auditoria foi explícita — «Corrigir o laboratório é parte da entrega da API,
pois é o que o criador de MOD usa para aprender e validar».

Ele estava obsoleto e não dizia: procurava `PRELUDIO_DO_MOD` em `base.js`, que
mudou de arquivo com o ADR 0049; recortava uma função por índice de string, com
assinatura que também mudou; e montava um `Worker` de navegador, que é o
ambiente que aquele ADR tirou.

**Decisão:** o prelúdio é lido de `executor.rs`, e uma falha em encontrá-lo é um
erro com o caminho — não um `undefined` que vira Worker sem API. O renderer
servido são os arquivos reais do produto. O que ele **não** prova está escrito no
cabeçalho dele.

**E um exercício novo, que não existia:** o cliente de cada MOD passou a subir no
**QuickJS de verdade**, com o prelúdio recortado de `executor.rs`, sob o mesmo
teto de 8 MiB. Ele confere o ambiente antes do pacote — `document`, `window`,
`fetch`, `require`, `process` e mais cinco têm de estar ausentes — e confere que
o MOD chegou a falar com o servidor dele. Um MOD que só pergunta o retrato e
para não desenhou nada.

O teste de navegador deixou de exigir que a região **não** tivesse botão nem
campo. Isso era verdade na API 2; desde a 3 a região tem controles de propósito,
e a asserção continuava verde porque o laboratório ao lado nunca chegava a
montar nada.

---

## 8. As decisões de UX que tiveram alternativa

**U08 — o medidor de microfone.** A auditoria ofereceu duas saídas: um teste
local de microfone independente de servidor, ou uma instrução coerente.
**Escolhida a segunda.** A primeira exige captura de áudio fora de sessão, que
este build não tem; prometê-la num texto seria repetir o defeito noutro lugar. As
duas frases passam a sair do mesmo estado, então não podem voltar a discordar.

**U17 — o botão de enviar.** A comp da 0.9.0 diz «sem botão de enviar», e o
argumento era bom: ele é a segunda maneira de fazer o que a tecla já faz.
**A auditoria ganhou**, e o motivo é medido: uma tecla não é descobrível, o
`placeholder` some na primeira letra digitada, e para quem navega por toque não
há tecla nenhuma. O botão é pequeno e secundário — a tecla continua sendo o
caminho rápido.

**U29 — a configuração.** A auditoria recomendou «página ampla + painel rápido
de áudio». **Entregue como camada de área útil, e não como rota.** A razão de ser
camada continua valendo e está escrita em `index.html`: trocar de microfone no
meio de uma conversa é o caso de uso, e uma tela que substitui a sessão esconde
o que se está consertando. O que mudou foi o tamanho — de 840×620 para a área
útil — e a faixa de contexto no cabeçalho, que diz sala, microfone e que a
conversa continua atrás. **O painel rápido de áudio separado não foi feito**;
ver a seção 9.

**U30 e o achado B da auditoria de áudio, em conflito.** U30 pede que a
explicação longa sobre o aparelho padrão não se repita; o achado B anterior
exige que o aviso esteja ao lado das duas listas — «quem usa a tela de
configuração fica pior do que quem não usa». **Resolvido por corte, e não por
escolha entre os dois:** o aviso encurtou e ficou nos dois lugares; o parágrafo
que o explica está uma vez, recolhido. Recolher o aviso junto teria escondido a
informação que faz a escolha ser informada.

**U04 — a escala.** `--seele-t-rotulo` subiu de 10px para 12px. Registrado em
ADR 0014 com a ordem de sempre (design, ADR, cópia). O adendo diz o que o número
**não** é: a WCAG não fixa tamanho de texto, e a auditoria é explícita — «não
declarar conformidade sem medida». Os alvos de botão são 40px e 32px, medidos, e
o comentário no CSS é a medida.

**U15 — a hierarquia.** O sinal era 26px peso 900 e o nome era o corpo de dado.
**Decisão:** o número desce para um `<details>` recolhível, com o valor no
resumo. Recolher não é tirar — quem precisa do número continua a um toque dele —,
e o detalhe nasce **aberto** quando há o que dizer: mudo, isolado ou fora da
faixa nominal não são repouso.

---

## 9. O que **não** foi feito, e por quê

Esta seção existe porque a auditoria pede que ela exista: «Não apresentar como
aprovados» o que não foi executado.

| O que | Por que não |
|---|---|
| **Publicar a v0.13.0 e os pacotes 3.0.0** | Exige a chave de produção (`~/.minisign/mods.key`) e a decisão de publicar, que é de quem responde pelo projeto. O runbook está escrito; a ordem é aplicativo compatível primeiro, pacotes depois. |
| **As oito jornadas de aceite do §14** | Elas terminam em observação nativa, com administrador **e** participante, dados existentes e servidor novo. Uma delas exige duas máquinas. Nada aqui as substitui, e contagem de suítes verdes menos ainda. |
| **A medição de custo do §10** | «Medir o app inteiro: sem MOD; três MODs ativos com UI fechada; um editor aberto; perfil com imagens; MESA carregado e em arraste; mesma cena com voz.» Não foi feita. O que existe são os tetos declarados e o descarte registrado; o que não existe é a linha de base. Nada nesta entrega deve ser lido como «o custo cabe». |
| **Windows e Linux** | A auditoria não os percorreu, e esta entrega tampouco. O código compila para os três; nenhum fluxo foi observado fora do macOS. |
| **Teste com pessoas (critério 6 das configurações)** | «Validar protótipo com pessoas sem familiaridade com SEELE.» Não foi feito, e não pode ser feito daqui. |
| **O painel rápido de áudio** | A auditoria o pede junto da página de configurações. A camada de área útil preserva a voz e diz isso na faixa de contexto, o que cobre a necessidade que o painel atendia; o painel em si, ao lado do controle de microfone, não foi construído. |
| **Teste local de microfone fora de sessão** | Ver seção 8, U08. Exige captura de áudio sem `Connection`, que este build não tem. |
| **Zoom textual, leitor de tela completo, latência e memória sob carga** | A auditoria já os listava como não executados, e continuam. |
| **Shadow DOM para as superfícies** | Ver seção 5. Em aberto, e não recusado — falta o protótipo que meça o custo nas plataformas. |
| **Três dos cinco riscos «a reproduzir»** | Árvore de ficha acima dos tetos (32 campos, 512 nós, 12 KiB), reconciliação das linhas nativas sob atualização frequente, e validação de mídia real (formatos, orientação, recorte, bytes retidos). Continuam sem reprodução, e a auditoria manda não apresentá-los como medidos. Os outros dois estão fechados — ver seção 10. |

---

## 10. Os guardas que foram provados contra a regressão de verdade

O `CLAUDE.md` deste repositório exige isto: «Provar um guarda contra a regressão
de verdade — revertendo o conserto e vendo-o falhar — é o que separa os dois.»

Foi feito para quatro, e o resultado está registrado:

| Guarda | Revertendo o conserto |
|---|---|
| MESA: `nonce` e `revision` no pedido | 14 casos falham, e o primeiro diz «o pedido de criação saiu sem `nonce`, e o servidor recusa com invalid-id» |
| ESTILO: a afirmação obsoleta | falha com «a afirmação obsoleta de recusa voltou depois de o produto ter aplicado» |
| PERFIS: o rascunho sobrevive a fechar | falha com «fechar a ficha apagou o que tinha sido escrito e não gravado» |
| `check-api` enxergando a janela | renomear `declararClasses` em todos os arquivos reprova, nomeando o símbolo |

| PERFIS: o arquivo volta quando o envio falha | falha com «o envio falhou e o arquivo ficou preso no produto até a saída da sessão» |
| MESA: a edição não atravessa a troca de canal | falha com «o nome de cena de uma mesa apareceu noutro canal» |

Os dois últimos são **dois dos cinco riscos que a auditoria mandou reproduzir**,
e são os dois que tinham conserto acionável:

- *«MESA chama `soltar` apenas depois do laço bem-sucedido. Verificar liberação
  em `finally`.»* Vale igual para o PERFIS, e os dois foram consertados;
- *«Rascunhos e respostas em voo atravessando mudança de canal […] Prender
  abertura/edição à entidade e ao canal de origem.»* O PERFIS já ganhou isso com
  U26; a MESA ganhou aqui.

O segundo caso precisou de uma segunda escrita para medir alguma coisa: com o
canal de destino vazio, ele passava sem tocar no que existe para pegar. Está
registrado no commit.

Os demais guardas acrescentados nesta volta — compatibilidade de API, escopo da
configuração, substituição de cartão, mapa como fundo, trilha do pacote, aviso
de fixação — não passaram por essa prova. Eles são afirmações sobre o estado
atual; dizer que são guardas provados seria a coisa que este arquivo existe para
não fazer.

---

## 11. O que foi observado de verdade, e onde

| Verificação | Evidência |
|---|---|
| Suíte do SEELE | 20 suítes, todas verdes; `cargo clippy --all-targets` sem aviso; `cargo build --release` limpo |
| Suítes dos três MODs | MESA 46, PERFIS 37, ESTILO 21 — verdes |
| Suíte do indexador | 200 casos, verdes |
| Renderer real num navegador real | `npm run test:ui` nos três: renderer do produto, prelúdio de `executor.rs`, superfícies, saída e reconexão |
| Cliente no QuickJS real | os três sobem sob 8 MiB, sem `document`, `window`, `fetch`, `require` nem `process`, e falam com o servidor deles |
| Fachada de API | `cargo xtask check-api`: quatro versões, toda a superfície aponta para algo |

**O que nenhuma dessas linhas prova:** que uma pessoa consegue usar o que foi
construído. Isso são as oito jornadas, e elas estão na seção 9.

---

## 12. A revisão do checkout `7ca66cc`: o que cada defeito virou

A [revisão](revisao-entrega-api4-2026-09-20.md) trouxe seis defeitos
reproduzidos com os métodos reais, mais o guia. Ela também trouxe a frase que
resume por que eles passaram: **o laboratório usa uma estrutura diferente e não
captura o problema.**

Essa é a forma de «existir não é funcionar» que este repositório paga mais caro,
e ela apareceu três vezes nesta volta, no mesmo lugar: o palco das camadas
estava direto no `<body>` do laboratório e dentro de `section#tela-sessao` no
produto; o `registrar` de mentira devolvia o tamanho do vetor onde o do produto
devolve um descartador; e a revogação do laboratório não passava o dono, então
ele aceitava o que o SEELE recusa. **As três foram consertadas no laboratório
antes do conserto do produto**, porque um laboratório mais permissivo que o
produto ensina a escrever o que o produto recusa.

### R1 — o modal tornava inerte o próprio ancestral

`prenderFoco` percorria os filhos de `document.body` e excetuava apenas
`palcos.camadas`. No `index.html`, esse palco é filho de `section#tela-sessao`,
então a seção inteira — com o diálogo dentro dela — ficava inerte.

**Conserto:** `inertarFora(dentro)` sobe o ramo ativo até o documento,
adormecendo **os irmãos de cada nível** e nunca um ancestral. Ele também
preserva o estado anterior: um nó que já era inerte não entra na lista e não é
despertado no fim, o que faz dois modais em qualquer ordem se desfazerem sem um
acordar o que o outro adormeceu.

**A confirmação do produto precisou das duas coisas que a revisão pediu.**
Acima — `z-index` da `.camada-moderar` foi de 30 para 80, passando os diálogos
de MOD (60) e os avisos (70) — e **interativa**: `confirmarDescarte` acorda
`#moderar` enquanto a pergunta está de pé e o devolve ao estado anterior nos
dois desfechos, por um `readormecer` que `abrirConfirmacao` passou a aceitar
como quinto argumento.

### R2 — a revogação não conferia o dono, e a API 3 alcançava a 4

Duas metades, e as duas no roteador.

`revogar(handle, dono)` agora exige que os três coincidam: identificador,
instância e geração. Os três, e não só o identificador — um MOD recarregado
dentro da mesma sessão é outra instância, e a contribuição da anterior não é
dele. O `dono` vem do roteador, nunca de um campo da mensagem: um MOD não
nomeia a própria versão nem a própria identidade.

A segunda metade é `podePedir(mod, tipo)`, conferido **antes** do `switch`, com
a tabela `CAPACIDADES_POR_API`. O prelúdio omitir o método nunca foi a
fronteira: ele roda dentro do contexto do MOD, e `seele.postar` continua lá. A
recusa nomeia a razão — «não existe na API 3, que é a que este pacote declara» —
em vez de dizer que a mensagem é desconhecida.

### R3 — oito pontos aceitos e não aplicados

O mais caro dos seis, e o que exigiu decisão de escopo.

**Oito foram ligados** a componentes reais: `pessoa.identidade` e
`pessoa.cartao` (modo `adicionar`) na linha do roster, `pessoa.detalhes` no
recolhível do perfil, `pessoa.acoes` no rodapé do cartão, `canal.item` no botão
do canal, `canal.cabecalho` e `compositor.ferramentas` na barra e no
compositor, `sala.acoes` no cabeçalho do grupo de sala, e `servidor.aparencia`
passou a decidir se o tema de um MOD se aplica. O caminho é único —
`montarContribuicao` monta o `conteudo` declarado pelo renderer de sempre, com
o perfil do cartão — e não mais `SeeleUI.cartoes` legado.

**E um modo foi retirado.** `decorar` estava na tabela de dois pontos e não
tinha aplicação nenhuma. Ele não foi ligado, e a razão não é esforço: as
propriedades que um MOD já pode declarar dentro da própria raiz —
`opacidade: 0`, `escalar: 0.1`, `mover: -512` — são escolhas estéticas sobre o
desenho **dele**, e as mesmas, aplicadas a um nó do produto, somem com o nome
que abre a moderação. O cabeçalho de `mods-estilos.js` diz que personalização
estética «não pode encobrir uma confirmação de confiança», e um `decorar`
honesto precisa do seu próprio subconjunto de estilo, provado contra o
encobrimento.

**Decisão:** suspender `decorar` na API 4, **recusá-lo pelo nome** com a razão
junto, e dizê-lo em `api/v4.json`, no guia e no erro que o MOD recebe. A
alternativa — mantê-lo na tabela até o subconjunto existir — é exatamente a
falha silenciosa que a revisão encontrou.

A revisão avisa: «O objetivo solicitado continua sendo completar a integração,
não apenas retirar nomes da documentação.» Oito pontos foram completados; um
modo foi retirado, com a razão escrita e um guarda que impede que ele volte à
tabela sem quem o aplique.

### R4 — mil ciclos, mil descartadores retidos

`InstanciaDeMod.registrar` passou a **devolver como esquecer**: um descartador
idempotente que tira a entrada da lista de recursos e chama o descarte. A
contribuição guarda esse retorno em `esquecer`, e `revogar` passa por ele em vez
de chamar `tirar` direto — chamar `tirar` é o que deixava o descartador para
trás.

A medida da revisão repetida na bancada nova: mil ciclos terminam com zero
contribuições vivas **e zero descartadores retidos**, e revogar o mesmo punho
duas vezes não desconta duas vezes.

### R5 — «usar apresentação padrão» voltava ao automático

Havia dois estados onde precisavam existir três. Agora: `""` é automático (a
prioridade decide), um `id` é aquele provedor, e `:nativo` — um valor
reservado, que não colide com nenhum `autor/nome` — é o nativo explícito.
`escolherSubstituicao` devolve `{ escolhida, preteridas, nativa, ausente }`, e
a gestão desenha os três estados e os três botões.

`ausente` é o quarto desfecho e não é o terceiro: um provedor escolhido que saiu
desenha o nativo **e diz que o escolhido não está de pé**, em vez de cair no
automático em silêncio — que é justamente o que a pessoa acabou de recusar.

A chave da preferência passou a ter destino (`chaveDaPreferencia(ponto)` sobre o
par `alvo\0ponto`), como o registro de decisões já dizia e o código não fazia.

### R6 — visibilidade e fechamento tinham contratos incompatíveis

`mostrar` chamava `aplicarVisibilidade` e devolvia sucesso com o nó fora do
documento. Agora ele chama `abrir`, que é idempotente. `ocultar` passou a soltar
o foco e o estado inerte — uma superfície invisível com a aplicação inerte atrás
dela é uma janela travada sem nada na tela para explicar. E uma superfície
descartada **recusa** em vez de responder sucesso.

`get montada` foi acrescentado porque a diferença entre «fechada» e «oculta» não
tinha como ser perguntada, nem pelo produto nem por um teste.

### R7 — o guia continuava ensinando a API 3

Aqui a revisão está certa sobre um fato desconfortável: o commit `21553ab` do
indexador se chama «O guia passa a descrever a API 4» e mudou **quatro linhas**,
todas de etiqueta. A mensagem dele descreve três seções novas que não existiam
no diff. O guia-fonte continuava inteiro na API 3.

O que foi feito nesta volta, em `docs/guia-criacao-mods.md` do indexador:

- a afirmação de igualdade exata de versão (linha 310) saiu, e no lugar entrou
  a conferência **por conjunto**, com a matriz dos cinco números;
- a seção «Aceitar a API 3 não dá à API 3 o que a 4 tem», com o `TypeError` que
  um pacote de API 3 recebe e a recusa do anfitrião à mensagem direta;
- um capítulo novo — `referencia/api4` — com superfícies (os quatro tipos e o
  ciclo de vida inteiro), contribuições (os dez pontos, os modos de cada um, as
  três garantias e a suspensão de `decorar`), estilos (categorias, estados,
  consultas de contêiner e a fronteira dita pelo que ela é), a migração da 3
  para a 4 e os limites da seção;
- a tabela de funções passou a dizer **em que API** cada membro existe;
- o manifesto passou a dizer que um MOD só de servidor não ganha nada
  declarando 4.

**E um defeito do gerador foi encontrado no caminho:** o slug de capítulo era
`[a-z-]+`, sem dígito, e um `## [referencia:api4]` que a expressão não reconhece
**não reprova** — ele cai no corpo do capítulo anterior e sai como parágrafo. A
seção inteira seria publicada presente no HTML e ausente do sumário e da busca.
`chapters()` passou a aceitar dígito e a **reprovar** quando um `##` não vira
capítulo.

---

## 13. Matriz do escopo acordado

«Implementado» quer dizer: existe, tem quem o aplique, e há guarda ou bancada
que reprova se ele sair. «Parcial» quer dizer que existe e não está completo, e
o que falta está escrito. «Pendente» quer dizer que não foi feito.

### A API 4

| Item | Estado | O que sustenta, ou o que falta |
|---|---|---|
| Conjunto de APIs aceitas (4 e 3) | Implementado | `a_api_dos_mods_nao_diverge`, `check-api`, e o indexador publica o conjunto |
| Capacidades por versão, conferidas no anfitrião | Implementado | `podePedir` antes do `switch`; bancada R2 |
| Superfícies: `dialogo`, `pagina`, `painel`, `aviso` | Implementado | `npm run test:ui` nos três MODs; bancada R6 |
| Ciclo de vida: criar/ocultar/mostrar/fechar/descartar | Implementado | Bancada R6, transições idempotentes e recusa depois de descartada |
| Foco contido, inércia e retorno ao acionador | Parcial | A atribuição de inércia é medida na bancada, no `index.html` real, com reabertura e ordem inversa. **Tab e Shift+Tab em todas as combinações continuam não observados**, e a validação nativa anotou um Escape que não fechou o editor com rascunho — sem causa demonstrada |
| Confirmação de descarte alcançável e por cima | Implementado | Observada no aplicativo nativo: por cima do modal do MOD e interativa nos dois desfechos |
| A superfície cabe no transporte | Implementado | Medida em bytes a cada `npm test` dos três pacotes (`cabeNaPonte`); mensagem grande é recusada pelo nome, e não como fila cheia |
| A atividade começa por um gesto | Implementado | Nenhum dos três pinta a faixa permanente na API 4; medido no laboratório dos três |
| Contribuições: 10 pontos aplicados | Implementado | `todo_ponto_de_contribuicao_anunciado_tem_quem_o_aplique` e o vetor de referência registra nos dez |
| Modo `substituir` consultado em todo ponto que o anuncia | Implementado | `todo_ponto_que_anuncia_substituir_e_consultado_por_escolher_substituicao` |
| Modo `decorar` | **Pendente** | Recusado pelo nome. Falta **escrever o contrato**: quais propriedades, em quais pontos, sobre quais nós. Cor, tipografia, fundo e borda não exigem oferecer deslocamento nem opacidade sobre um controle nativo |
| Isolamento entre MODs na revogação | Implementado | Bancada R2, com os dois lados: B não revoga de A, e A revoga de A |
| Descarte sem retenção | Implementado | Bancada R4: mil ciclos, zero retidos |
| Três estados de apresentação | Implementado | Bancada R5, incluindo a distinção entre nativo e provedor ausente |
| Estilos declarados e validados | Implementado | `mods-estilos.js`; ADR 0052 e a emenda de `specs/07` |
| Classes com estados e consultas de contêiner | Implementado | Por superfície; uma região não tem folha própria, e o guia diz isso |
| Composição e contratos de estado do §12 do plano | **Pendente** | Nada foi feito; não estava na auditoria, e a revisão o nomeia como redução de escopo |
| Keyframes livres | **Pendente, por decisão** | Presets em vez de `@keyframes`: um keyframe declarado por um MOD é texto que vira regra, e a lista do que uma regra aceita não é nossa |
| Fontes | **Pendente** | O que foi decidido é que **fonte arbitrária de rede** não entra — é busca de bytes de terceiro na janela de quem conversa. Fonte **empacotada no MOD** ou escolhida de um conjunto gerenciado pelo produto é outra coisa, e não foi decidida nem implementada. Tratar as duas como o mesmo acesso foi um erro de redação desta linha |

### A auditoria de UX/UI

| Item | Estado | Observação |
|---|---|---|
| U01–U33 | Implementado em código | Cada um tem commit e, na maioria, guarda. **Nenhum foi observado por uma pessoa usando o produto** |
| Os cinco defeitos funcionais | Implementado | Com guarda; dois deles provados revertendo o conserto |
| Reorganização das configurações | Implementado | Camada ampla com cabeçalho e agrupamento |
| Painel rápido de áudio | **Pendente, por decisão** | Registrado na seção 9; a revisão observa, com razão, que registrar a decisão não equivale a entregar a recomendação |
| Cinco riscos «a reproduzir» | Parcial | Dois reproduzidos e consertados com guarda provado; três continuam sem reprodução |
| As jornadas de aceite | Parcial | Percorridas no aplicativo nativo com os três pacotes (validação de `d96d71a`): criar campanha e cena, rolar dados, gravar perfil, cancelar e confirmar descarte, sair. Seis defeitos encontrados e consertados na seção 16. **Continuam pendentes** segundo participante, ficha de terceiro, retrato/faixa, arraste de peças, mídia, Tab em todas as combinações, memória sob carga, Windows e Linux |

### O guia e o laboratório

| Item | Estado | Observação |
|---|---|---|
| Guia-fonte na API 4 | Implementado | Capítulo novo, matriz de versões, migração |
| Site gerado | Implementado | 25 capítulos; o gerador passou a reprovar um `##` que não vira capítulo |
| Um exemplo por ponto de integração | Parcial | O vetor de referência registra nos dez e o guia aponta para ele; **o guia não traz dez trechos, traz um por forma de ponto** |
| Laboratório com a estrutura do produto | Implementado | Palcos dentro de `#tela-sessao`; instância de mentira devolvendo descartador; revogação com dono |
| `api/v4.json` | Implementado | Modos por ponto, e a suspensão de `decorar` escrita em `fora` |

---

## 14. O que **ainda** não foi observado, e o que é preciso para observar

A revisão recusou a justificativa de «precisa de duas máquinas» para adiar
**todas** as jornadas, e ela está certa: criar campanha, editar perfil, navegar,
confirmar descarte, repetir abertura/fechamento e verificar saída precisam de um
cliente só.

O que foi feito com esse recado nesta volta:

- **as jornadas de cada MOD** foram percorridas no laboratório dos três, num
  navegador de verdade, com o renderer do produto, o prelúdio de
  `executor.rs` e o código de servidor real de cada pacote: desenho, controles,
  dados autorizados, superfície com a saída do produto, **saída soltando tudo**
  e reconexão. Os três passam, e foi essa bateria que encontrou o `esquecer`
  que o laboratório não devolvia;
- **as jornadas do produto** — a inércia com a hierarquia do `index.html`, a
  confirmação alcançável, o ciclo de vida da superfície, os três estados de
  apresentação, o isolamento e o descarte — foram medidas na bancada
  `contribuicoes-e-camadas.cjs`, contra o HTML real e o roteador real;
- **o aplicativo nativo foi construído e subiu**: `cargo build -p seele-app`
  limpo, o binário abre e fica de pé sem uma linha em `stderr`, e encerra.

**O que continua sem observação, e é preciso ser honesto sobre por quê:** não
consigo operar a janela do aplicativo a partir daqui — não há como apertar um
botão, digitar num campo ou ler o que a tela mostra. Isso não é a limitação de
«duas máquinas», que era falsa; é a de que nenhuma das ferramentas desta sessão
alcança a janela de um aplicativo nativo do macOS.

Então o que falta é uma sessão de teclado, com um cliente só, e estes passos:

1. subir o aplicativo, criar um servidor local e entrar nele;
2. instalar os três pacotes candidatos e aceitar o conjunto;
3. **MESA**: criar campanha, criar ficha, abrir a ficha de outra pessoa e
   confirmar que o que é privado não aparece;
4. **PERFIS**: editar o perfil, gravar, fechar com alteração não gravada e
   conferir que a pergunta do produto aparece **por cima** e responde;
5. **ESTILO**: prévia e publicação do tema; conferir que `servidor.aparencia`
   com «usar apresentação padrão» devolve o tema do SEELE;
6. abrir uma superfície, dar Tab e Shift+Tab até a borda e conferir que o foco
   não sai dela; Escape; conferir que o foco volta ao que a abriu;
7. abrir dois modais — o de um MOD e a confirmação por cima — e fechá-los **em
   ordem trocada**;
8. revogar uma contribuição pela gestão e conferir que a apresentação nativa
   volta;
9. sair do servidor e conferir que nada de MOD sobrou na tela;
10. medir memória sem MOD, com três ativos, com uma UI aberta e depois de ciclos
    de saída.

Os passos 1–9 são de um cliente. O que continua exigindo dois é sincronização
entre participantes, autorização de outra pessoa e qualidade de voz — e só isso.

---

## 15. A segunda revisão (checkout `26ad0c2`): três caminhos que ficaram fora

A revisão reconheceu os consertos e encontrou três caminhos do mesmo contrato
que a bancada não alcançava. Os três eram atingíveis por API pública, e os três
estão fechados.

### 1. A inércia era de cada diálogo, e precisava ser da pilha

Dois sintomas, uma causa. `prenderFoco` guardava a própria lista de
adormecidos, e `inertarFora` pula quem já está inerte:

- **reabrir** passava por lá de novo — `criar` com o mesmo `id` chama
  `mostrar`, que desde o conserto de R6 chama `abrir` — e a segunda chamada
  não adormecia ninguém, **substituindo** a lista pela vazia. Fechar não
  devolvia nada, e a aplicação ficava inerte sem nada na tela para explicar.
  Havia ainda um `abrir()` redundante em `SuperficiesDoMod.criar`, logo depois
  do `mostrar()`, que era o caminho público mais curto até isso;
- **dois diálogos**: B não reivindicava o que A já tinha adormecido, então
  fechar A acordava o fundo com B ainda aberto.

Preservar o booleano anterior de cada nó — o que a volta passada fez — resolve
um nó que **já era** inerte antes de nós. Não resolve dois donos simultâneos
querendo o mesmo nó adormecido, porque esse é um problema de propriedade.

**Decisão:** uma pilha de camadas em `mods-superficies.js`. A inércia é do
**topo**, e só dele; a cada entrada e a cada saída, o que nós adormecemos é
devolvido inteiro e recalculado. Entrar é idempotente e traz a camada para a
frente; sair funciona do meio da pilha, que é o fechamento fora de ordem.

A confirmação do produto não entra na pilha — ela é da moderação, não de um MOD
—, e por isso ganhou um conjunto explícito de «acordados por pedido», que o
recálculo respeita. Sem ele, bastava um MOD abrir outra superfície enquanto
alguém lia a pergunta para a pergunta adormecer com ela na tela.

### 2. O descarte estava consertado para quem não desenhava

Duas metades, e as duas com a mesma forma: **criar o descartador e não usar o
retorno**.

`montarContribuicao` registrava `soltarMontagem` e `tirar` nunca o chamava:
revogar soltava o registro lógico e deixava o renderer, o nó e o descartador
retidos. A medida anterior não pegava porque mil ciclos de uma contribuição
**sem conteúdo montado** não passam por esse caminho — a revisão nomeia isso
exatamente.

O construtor de `SuperficieDeMod` ignorava o retorno de `registrar`: mil
superfícies criadas e descartadas deixavam mil entradas na instância.

**Decisão:** `InstanciaDeMod.registrar` passou a devolver um descartador com
`.esquecer()` ao lado. Quem já está no meio do próprio descarte chama
`esquecer` — sair da lista sem pedir o descarte de volta. A alternativa era
chamar o descartador e parar na guarda de idempotência do outro lado: funciona,
e é uma recursão que só não é infinita por causa da ordem de duas linhas em
outro arquivo.

A bancada passou a medir **com desenho montado**, e a de superfícies passou a
existir. Os dois casos reprovam com os números da reprodução quando o conserto
é revertido.

### 3. A substituição de cartão ainda dependia do legado

`cartaoDeContribuicao` procurava só `cartoesDosMods.get(mod).cartaoDe(id)`. Um
autor que seguisse o guia — registrar `pessoa.cartao/substituir` com `conteudo`
— via a linha nativa continuar no lugar, sem recusa e sem explicação. O MOD
oficial usa `SeeleUI.cartoes` e por isso o caminho funcionava sem revelar a
falha. A frase «não mais `SeeleUI.cartoes` legado» da seção 12 estava errada, e
fica corrigida aqui.

**Decisão:** o conteúdo genérico primeiro; `SeeleUI.cartoes` como alternativa,
porque um pacote de API 3 é executado e é assim que ele desenha.

E **uma montagem por destino**. Uma contribuição sem `alvo` vale para todo
mundo, e um nó só, devolvido para duas pessoas, não aparece nas duas: `append`
move, e a segunda linha rouba o nó da primeira. `montarContribuicao` passou a
receber o destino e a guardar um nó, um renderer e um descartador por destino —
o mesmo defeito existia em `conteudoDasContribuicoes` para toda contribuição
geral, e saiu junto.

### O que isto mudou no que este arquivo afirma

A seção 13 continua valendo, com três linhas corrigidas: `decorar` é
**pendente** porque o contrato dele não foi escrito — não porque decorar seja
incompatível com segurança; fontes empacotadas ou gerenciadas não são o mesmo
acesso que fonte arbitrária de rede, e tratá-las como uma coisa só foi erro de
redação; e a afirmação sobre o caminho legado do cartão estava errada.

**A validação nativa continua sendo a condição de término**, e ela não passou
por aqui. A sessão que escreveu isto não alcança a janela de um aplicativo do
macOS; a revisão informa que a sessão dela alcança. Os dez passos continuam na
seção 14.

---

## 16. A validação nativa: seis defeitos que só a janela mostrava

A [validação nativa](validacao-nativa-api4-d96d71a.md) do checkout `d96d71a`
percorreu as jornadas no macOS, com os três pacotes instalados pela interface.
Criar campanha e cena, rolar dados, gravar perfil, cancelar e confirmar
descarte e sair do servidor funcionaram. Seis defeitos concretos não.

**O que esta rodada corrige sobre o que a seção 14 dizia:** ela listava dez
passos de teclado como pendentes e eles foram percorridos. O que ficou aberto
está no fim daquele relatório, e não é o que estava aqui.

### N1 — o ESTILO não cabia no transporte

14.164 bytes num `superficie-montar`; o teto por mensagem é 12.288. Duas
metades.

**No produto:** `postar` devolve `false` nos dois casos — mensagem grande e
fila cheia —, e o prelúdio traduzia qualquer `false` para `fila-cheia`. A
frase na tela falava de saturação, que não era o que estava acontecendo, e
mandava procurar no lugar errado. O anfitrião passa a entregar o teto ao
prelúdio (`__seeleTetoDaMensagem`, como as capacidades), que mede **antes** de
postar e recusa com `mensagem-grande`, o tamanho, o teto e o tipo. A
conferência em Rust continua: o prelúdio roda dentro do contexto do MOD.

E a gestão deixou de dizer «o código não carregou» sobre um MOD que carregou:
`aoFalhar` separa a subida da volta, e a frase nova não manda reconectar para
resolver o que reconectar não resolve.

**No pacote:** as três abas eram montadas de uma vez. O renderer do produto
**já** desenha só o painel escolhido — mandar o conteúdo das outras duas era
pagar a ponte por algo que nem seria montado. Agora só a aba aberta atravessa:
4.853 bytes.

**E a medida virou guarda**, em `test/cliente-api3.test.cjs` dos três pacotes.
Foi ela que encontrou um segundo caso que a rodada nativa não viu: o diretório
do PERFIS com setenta pessoas dava **52.862 bytes**. Ele não apareceu na
validação porque o servidor de teste tinha uma pessoa. O diretório passou a
paginar, com o total e a faixa escritos na tela.

### N2 — a faixa permanente dos três

`iniciar`, na casca compartilhada, chamava `desenhar` — e portanto `ui.regiao`
— a cada volta do relógio, ligado ou não. Com os três instalados, a sessão
perdia cerca de 230 px de altura permanentemente, sem nenhuma atividade
aberta.

**Decisão:** num pacote de API 4, a casca não pinta a região. A atividade mora
numa superfície que abre por um gesto; o recado curto — «não foi possível
atualizar» — vira **aviso**, que sai sozinho. Num pacote de API 3 a região
continua exatamente como era: ela é o único lugar onde aquele MOD existe, e a
degradação não pode ser um caminho pior.

O que **não** mudou é que a falha é dita. Trocar uma faixa que incomoda por um
erro que ninguém vê seria o defeito que este repositório mais paga.

O aceite virou teste nos três laboratórios: com o MOD ligado e nada aberto, a
faixa mede zero.

### N3 — o PERFIS substituía sem ter conteúdo

O PERFIS registra a apresentação de todo mundo ao subir e devolve nada para
quem ainda não preencheu o perfil. A linha ficava com um alvo de clique vazio e
um `DETALHES` recolhido no lugar do nome: o produto apagando a identidade
nativa em troca de nada.

**Decisão, e ela é do produto:** o **conteúdo** decide, não a existência de um
provedor. `linhaDoRoster` pede o cartão antes de escolher o desenho. Uma
substituição sem conteúdo desenha a linha nativa — e **mantém o caminho até o
MOD**, porque quem não tem perfil é exatamente quem precisa abrir o editor.

O nome acessível passou a nomear a pessoa sempre. Ele era
`nomeAcessivel || pessoa.nome`, e um provedor com um rótulo genérico dava a
vinte linhas o mesmo nome — quem navega por leitor de tela ouvia vinte vezes a
mesma coisa sem saber em quem estava.

### N4 — a escolha nativa não alcançava o caminho legado

`SeeleUI.cartoes` é o outro caminho do mesmo provedor para o mesmo ponto.
«Usar apresentação do SEELE» devolvia o nome nativo e deixava o cartão do MOD
pendurado abaixo dele.

**Decisão:** a preferência é do **ponto**, e vale nos dois caminhos. Nativo
explícito: nenhum cartão de MOD. Um provedor escolhido: só os dele. Automático:
todos. Preservar a compatibilidade da API 3 é executar o caminho antigo, não
ignorar o que a pessoa escolheu.

### N5 — criar campanha deixava o diálogo aberto e vazio

O sucesso limpava o rascunho e redesenhava. O que ficava na tela era um
formulário com o nome apagado e o botão desligado — a aparência exata de uma
criação que **não** aconteceu.

Agora a criação da campanha descarta o diálogo e abre a mesa, com um aviso. Só
ela: cena, ficha e peça acontecem dentro de uma tela que continua sendo usada
para a seguinte.

### N6 — o compositor virou um controle do navegador

`index.html` trocou o `<input>` por `<textarea>` quando Shift+Enter passou a
quebrar parágrafo, e a folha continuou selecionando `.compor input`. O campo
saiu com a aparência padrão do WebKit — fundo branco, largura de umas poucas
dezenas de pixels — dentro de uma moldura escura larga.

O guarda de classes não pegava porque **não há classe**: o seletor é por
etiqueta, e trocar a etiqueta é trocar o seletor sem tocar na folha. O guarda
novo lê a etiqueta de `#campo-mensagem` na própria página e exige regra para
ela, com fonte, cor e largura.

### As configurações

Saíram do menu a frase de introdução — os quatro cabeçalhos de grupo já dizem
o mesmo, cada um com a ressalva certa — e as cinco notas por seção, que
repetiam o `data-sub` que o painel já mostra ao abrir. A régua dos grupos e das
seções apertou, sem tirar alvo de toque. Na área de áudio, as duas instruções
sobre a mesma decisão viraram uma frase, e o espaçamento entre blocos caiu de
24 para 16.

Trocar de seção passa a rolar o painel para o topo: a rolagem é do painel
inteiro e ele não é recriado, então a seção nova nascia no meio de si mesma.

E o operador deixou de dizer «microfone aberto» com o modo em TECLA. São três
coisas — o aparelho, como ele abre, e se está indo ao ar —, e a frase dizia as
duas primeiras ao mesmo tempo. «Aberto» sem ressalva passou a querer dizer só o
caso em que ele está aberto o tempo todo.
