# Decisões tomadas ao executar a auditoria de 20/09/2026

Data: 20/09/2026. Escrito **ao final do desenvolvimento**, como o pedido exigia.

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
