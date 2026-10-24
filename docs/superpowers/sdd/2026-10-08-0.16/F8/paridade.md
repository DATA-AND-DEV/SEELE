# A matriz de paridade: o celular e o desktop

**Frente:** F8 da 0.16, a tela do celular (D-celular-1). **Data:** 2026-10-09.
**Base:** `versao/0.16` em `f7bc902b`. O código de `apps/seele-app/ui/` lido aqui é o de
`f7bc902b`. As âncoras `arquivo:linha` são desse commit, com `apps/seele-app/` e `ui/`
implícitos. O publicado é a v0.15.1 (`54f542f94`), conferido pela API de releases.
**Spec:** [`docs/superpowers/specs/2026-10-09-0.16-celular-design.md`](../../../specs/2026-10-09-0.16-celular-design.md).

## O que esta matriz é

Cada tela, camada, estado e ação do desktop, com o que ele vira no celular. «A par do
desktop» quer dizer **tudo o que o escopo do ADR 0057 permite, com a mesma
informação**, e o que fica de fora é dito na tela. O produto não esconde o que não
faz.

O levantamento foi feito em 09/10 por sete leituras do código, uma por área, e
juntado aqui. Cada linha leva a âncora do desktop. O inventário das ações são os
**159** `invoke` da `ui/`, e não os 154 do spec-mestre: os 154 contavam a `main`
`54f542f`, e as ondas da F5 acrescentaram cinco. A §8 lista os 159, um por linha.

## Como ler

Cada linha tem um destes estados:

- **igual** — o celular faz o mesmo, com a mesma informação; muda o lugar ou o
  tamanho, e a linha diz onde ele mora;
- **adaptado** — faz a mesma coisa de outro jeito, e a linha diz como;
- **fora na 0.16** — o celular não faz, e a linha diz **a frase que a tela diz** e
  **onde** ela aparece;
- **novo** — um caso que o desktop não tem e que o celular precisa (a entrada pelo
  link, por exemplo).

**Os lugares do celular** (o desenho está no spec):
- as quatro abas no pé, na ordem das colunas do desktop: **SERVIDOR · CONVERSA ·
  PESSOAS · AJUSTES**;
- **a faixa de voz**, encaixada acima das abas quando a pessoa está numa sala, com
  MUDO, SURDO e SAIR;
- **a CHAMADA**, em tela cheia, aberta por um toque na faixa, com RECOLHER;
- **as folhas de baixo**: a da pessoa, a da mensagem, anexar, senha de sala e
  confirmação;
- **as subpáginas de AJUSTES**;
- a **tela de início** do web app, a **ENTRAR EM** do link, a **espera** da portaria
  e o **fim**.

**Contagem por área:**

| área | igual | adaptado | fora na 0.16 | novo | linhas |
|---|---|---|---|---|---|
| 1. A entrada e o fim | 16 | 38 | 17 | 14 | 85 |
| 2. A moldura da sessão | 22 | 47 | 6 | 0 | 75 |
| 3. A conversa | 45 | 27 | 2 | 0 | 74 |
| 4. A chamada e a tela | 30 | 30 | 9 | 0 | 69 |
| 5. Configurações, perfil e ajuda | 9 | 37 | 9 | 0 | 55 |
| 6. Moderação, confirmações, portaria e aceite de MODs | 15 | 18 | 29 | 0 | 62 |
| 7. Os MODs | 17 | 12 | 20 | 0 | 49 |
| **total** | **154** | **209** | **92** | **14** | **469** |

A ponte **atende** 91 e **recusa com nome** 51 dos 159; 17 voltam com frase.

## As decisões que a matriz aplica

Do dono e da orquestradora:
- **ADR 0057:** o celular só entra, e nunca hospeda; não compartilha a própria tela;
  não administra; não liga pelo código na 0.16.
- **D-web-1:** os MODs rodam no celular.
- **D-estilo-1:** a aparência do servidor se edita no celular, pelo diálogo do ESTILO
  numa coluna, em tela cheia e com VER NA SESSÃO. Por isso ela não entra em NO
  COMPUTADOR.
- **D-52:** o áudio abre só ao entrar numa sala de voz.
- **D-capa-1:** a capa animada vai no cartão da chamada e nos dois cabeçalhos de
  perfil.
- **D-nucleo-1:** o apelido verificado fica sempre à vista.

Da orquestradora, em 09/10:
- a navegação e o rótulo SERVIDOR;
- corte seco, sem animação de lugar;
- o tema alcança tudo, menos o núcleo;
- o que fica fora é dito no lugar do controle e numa seção de AJUSTES (aqui com o nome novo, pela D-web-3);
- SEGURAR PARA FALAR, com VOZ por padrão;
- `operador.barra` no alto de AJUSTES (aceito pela F3);
- a «ENTRAR EM» mostra o endereço e a impressão, e não o nome do servidor;
- o apelido tomado não gasta o convite.

Da F8, ao juntar as seções:
- **Sala lotada:** o cartão diz **LOTADA** (a palavra do `frases.js`) e o ENTRAR
  continua. Quem recusa é o servidor, como no desktop.
- **Entrar mudo:** fora de sala não há faixa de voz, então o cartão da sala tem
  **ENTRAR MUDO** ao lado de ENTRAR NA SALA.
- **A telemetria fora de sala:** ATRASO, JITTER, PERDA, CODEC, CAMINHO e UPTIME
  ficam completos em AJUSTES › ESTE SERVIDOR › CONEXÃO. Na sala, eles também estão na
  fileira da CHAMADA.
- **PERDA, e não DISTÚRBIO:** é a palavra do glossário canônico. O rodapé do desktop
  ainda diz DISTÚRBIO.
- **A saída do tema é do núcleo.** No desktop, CONFIGURAÇÕES fica fora do tema, e
  quem pegou um tema ruim sempre chega a «o SEELE desenha». No celular, AJUSTES é
  uma aba da sessão. Por isso a aba AJUSTES, a linha MODS DESTE SERVIDOR e a página
  dela leem `--nucleo-*`, como o MUDO.
- **Nenhuma superfície de MOD cobre a faixa de voz nem as abas.** O diálogo, a folha
  e a página imersiva moram acima delas. Achado no protótipo: o diálogo do ESTILO em
  tela cheia cobria o MUDO e o caminho para AJUSTES.

## O PC no navegador (D-web-3)

O web app também serve no computador, com **a casca de desktop de hoje**, pela ponte
da F4. A casca é escolhida na carga, pela marca `data-casca`:
- **`celular`** com `(pointer: coarse)` ou largura abaixo de 900 px;
- **`larga`** no resto.

Cada linha tem a coluna **PC (navegador)**, com o mesmo critério. O padrão é
**igual**: a casca é a mesma.
- **Fica de fora:**
  - hospedar, e o que é só de quem hospeda: a portaria do próprio servidor, e
    instalar, ligar e aplicar MODs no servidor;
  - as versões lado a lado;
  - ligar pelo código;
  - o caminho entre pares (o navegador não abre UDP nem aceita conexão de
    entrada).
- **Adapta-se** o que é da casca do Tauri: a janela, o atualizador, o seletor de
  arquivo, os ajustes do sistema, a tecla de falar (só com a página em foco), os
  aparelhos de áudio, o executor de MODs, e o compartilhar a tela, que passa a ser
  pelo `getDisplayMedia`.
- **Moderar e compartilhar a tela entram** no PC. No celular, continuam fora.
- **Quando compartilhar não dá no PC.** A ponte da F4 só oferece o botão depois
  de uma prova com prazo (no Safari, o `isConfigSupported` aceita perfis que não
  produzem nada), e devolve `{ erro: "compartilhar-indisponivel", motivo }`. As
  frases, no lugar do COMPARTILHAR, vão para o `frases.js` com o P10:

  | `motivo` | frase |
  |---|---|
  | `sem-captura` (não há `getDisplayMedia`) | «Este navegador não compartilha a tela. O app do SEELE compartilha.» |
  | `sem-codificador` (não há `VideoEncoder`, ou ele recusa o perfil) | «Este navegador não codifica vídeo do jeito que a sala recebe. O app do SEELE compartilha.» |
  | `codificador-mudo` (aceita, e não devolve pedaço no prazo) | «Este navegador disse que codificava a tela, e nenhum quadro saiu no prazo. Daqui não dá para compartilhar; o app do SEELE compartilha.» |
  | `captura-negada` (a pessoa recusou na caixa do navegador) | «O navegador não recebeu a permissão. Para compartilhar, toque em COMPARTILHAR de novo e escolha a tela ou a janela.» |

  As três primeiras levam o link BAIXAR O APP DO SEELE (`PAGINA_DO_APP`); a
  quarta não, porque o caminho é tentar de novo.
- **O arquivo, no PC e no celular, vai por uma alça** (combinado com a F4, 09/10):
  a ponte guarda o `File` do arraste, do seletor ou do colar sob uma alça opaca
  (`web-arquivo:N`), emite o mesmo `tauri://drag-drop` com ela, e o
  `descrever_arquivo` e o envio leem dela. O `tela-sessao.js` não muda.

A coluna foi gerada por regra, pelos `invoke` e pelo texto de cada linha. Depois
foi revisada linha a linha onde não deu «igual», e nas linhas que ficam fora no
celular e iguais no PC. Foram 45 correções por id: a regra errava, por exemplo,
com o `connect`, que não muda a tela, e com a gestão de MODs, que só dizia «idem».

## O que fica de fora, a lista canônica

Uma seção de AJUSTES (no celular) e de CONFIGURAÇÕES (no PC) lista o que aquela
casca não faz. **A frase no lugar do controle segue o grupo do item:**
- «**pelo app do SEELE**» para o que só o app instalado faz;
- «**pelo SEELE no computador**» para o que o computador também faz no navegador.

Todas vão para o `frases.js` na Etapa B.

- **No celular**, a seção se chama **O QUE O CELULAR NÃO FAZ**, com dois grupos.
- **No PC**, ela se chama **O QUE O NAVEGADOR NÃO FAZ**, com o primeiro grupo só.

**SÓ NO APP DO SEELE** (nas duas formas):

| item | frase |
|---|---|
| HOSPEDAR UM SERVIDOR | «Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.» E o link para baixar o app |
| A PORTARIA DO SEU SERVIDOR | «Aprovar quem bate, a senha e os convites ficam no app do SEELE que hospeda o servidor.» |
| INSTALAR E LIGAR MODS | «Os MODs deste servidor rodam aqui. Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| PACOTES DE MOD | «O espaço que os pacotes ocupam, as revisões guardadas e o APAGAR ficam no app do SEELE.» |
| LIGAR PELO CÓDIGO | «Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa.» |
| VERSÕES LADO A LADO | «O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE.» |
| A TELA ENTRE PESSOAS | «No navegador, a tela de quem transmite vem sempre pelo servidor, e este aparelho não a repassa a ninguém.» |

**NO COMPUTADOR** (só no celular: o que o PC faz no app e no navegador):

| item | frase |
|---|---|
| MODERAR | «Expulsar, banir, mover pessoas e remover mensagens de outros é pelo SEELE no computador.» |
| SALAS E CANAIS | «Criar, renomear e apagar salas de voz e canais é pelo SEELE no computador.» |
| NOME E IMAGEM DO SERVIDOR | «Pelo SEELE no computador. A aparência do servidor se edita aqui, em ESTE SERVIDOR.» |
| COMPARTILHAR A TELA | «O navegador do celular não grava a tela. Compartilhar é pelo SEELE no computador, e a tela de quem compartilhar aparece na chamada.» |
| TESTAR O MICROFONE FORA DE SALA | «O microfone do celular só abre numa sala de voz. Testar e ouvir a própria voz fora de sala é pelo SEELE no computador.» |
| ESCOLHER A SAÍDA DE SOM | «O navegador do celular não escolhe a saída: troque entre fone, Bluetooth e alto-falante pelo controle de som do celular.» |

## As correções feitas na junção

As seções foram escritas antes de três decisões do dia. Esta junção as aplicou:

1. **ENTRAR EM sem o nome do servidor:** o título é «ENTRAR EM <endereço>?» (N-01).
2. **O apelido tomado não gasta o convite.** A frase do N-06 passa a dizer «O convite
   continua valendo».
3. **PERDA no lugar de DISTÚRBIO, e a telemetria fora de sala em AJUSTES** (B-R01,
   B-R02).

Mais **18 conflitos na tabela dos `invoke`**, resolvidos na §8 pela regra medida: o que
a casca chama sem gesto é atendido pela ponte. Exemplos:
- `conhecidos` estava como recusa numa seção e atendido noutra;
- `plataforma` estava como recusa, mas a ponte responde `"web"`;
- `tecla_de_falar`, `pasta_de_downloads`, `versoes_instaladas` e
  `regras_do_icone_do_server` são chamados na carga;
- `estado_da_porta` é chamado na sessão, sozinho, a cada poucos segundos.

---


## 1. A entrada e o fim

Esta matriz sai do inventário `A-entrada.md`, que é a fonte das âncoras, e segue o desenho e as regras de `00-desenho-e-regras.md` e o ADR 0057 (`docs/adr/0057-o-celular-entra-pelo-navegador.md`). Commit `f7bc902b`.

Os caminhos das âncoras começam em `apps/seele-app/`; `ui/` está implícito para `.js`, `.css` e `index.html`. «Inventário B/D/E» aponta para outra área quando o item mora lá no celular.

**Lugares no celular**, conforme o desenho:
- **tela de início**: o web app aberto sem link (N-08);
- **ENTRAR EM**: a tela do link (N-01);
- **espera**: a tela própria da portaria;
- **aba SERVIDOR**: a trilha, as salas e os canais;
- **AJUSTES**: · SEU PERFIL · ESTE APARELHO · SOBRE · O QUE O CELULAR NÃO FAZ · SAIR DO SERVIDOR;
- **folha**: a folha de baixo;
- **ENLACE ENCERRADO**: tela cheia.

#### As frases de O QUE O CELULAR NÃO FAZ desta área (citadas como NC-n)


- **SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR** «Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.» E o link BAIXAR O APP DO SEELE.
- **SÓ NO APP DO SEELE › VERSÕES LADO A LADO** «O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE.»
- **SÓ NO APP DO SEELE › A TELA ENTRE PESSOAS** «No navegador, a tela de quem transmite vem sempre pelo servidor, e este aparelho não a repassa a ninguém.»
- **SÓ NO APP DO SEELE › LIGAR PELO CÓDIGO** «Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa.»

---

### A matriz


| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| **Entrada** | | | | | | |
| PA-01 | tela | «SEELE» · «voz, vídeo e texto auto-hospedados» · «Voz, vídeo e texto entre pessoas, sem servidor de ninguém no meio. Entre no servidor de alguém, ou ponha o seu no ar a partir desta máquina.» | index.html:255-302 · na carga: `versoes_instaladas`, `abertura`, `apelido_local`, `meu_retrato`, `estado_da_primeira_abertura` | adaptado | Vira a tela de início (N-08). A marca e o propósito ficam, sem a metade «ou ponha o seu no ar a partir desta máquina». A caixa de 480 px já encolhe (`max-width:100%`) | adaptado: a entrada do desktop, com o HOSPEDAR AQUI virando a frase do app e o link para baixá-lo, e o propósito sem a metade «ou ponha o seu no ar a partir desta máquina»: «Entre no servidor de alguém pelo link que a pessoa mandou.» |
| PA-02 | ação | **CONECTAR** (abre ONDE VOCÊ JÁ ESTEVE) | index.html:305; tela-boot.js:831 · `conhecidos` | adaptado | Não há diálogo: a lista SERVIDORES DESTE APARELHO e o botão COLAR UM LINK estão na própria tela de início (N-08, N-09) | igual |
| PA-03 | ação | **HOSPEDAR AQUI** | index.html:306; tela-boot.js:669-701 · `servidores_guardados` | fora na 0.16 | Na tela de início, onde o botão estaria: «Hospedar um servidor é pelo app do SEELE, instalado num computador.» e o link BAIXAR O APP DO SEELE (`PAGINA_DO_APP`, `https://seele.app.br/#baixar`). Em AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-04 | estado | «O que este programa é, por dentro»: «$ seeled 0.0.0.0:8383», «quic/tls1.3 · sobre udp», «ed25519 · impressão digital fixada na primeira visita», «pronto.» | index.html:316-367 | adaptado | Vai para AJUSTES · SOBRE com o que vale no navegador: «webtransport · http/3 até o servidor» e «ed25519 · chave deste aparelho, guardada no navegador». O «$ seeled…» é de quem hospeda e sai | igual |
| PA-05 | ação | «Tenho um nome apontado para esta máquina» → **NOME PÚBLICO (OPCIONAL)**, com o aviso de TOFU | index.html:378-404 · `hospedar` | fora na 0.16 | Coberto por SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR, em SÓ NO APP DO SEELE. Nada no lugar: o bloco só serve a quem hospeda | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-06 | ação/estado | «Hospedar com outra versão do SEELE» (EM USO / REINSTALE PARA USAR / **HOSPEDAR COM ESTA**) e a nota «As conversas da 0.15 ficaram guardadas para o Servidor 0.15…» | index.html:419-430; tela-boot.js:551-621; camada-versoes.js:334-358 · `versoes_instaladas`, `abrir_versao`, `estado_da_primeira_abertura` | fora na 0.16 | SÓ NO APP DO SEELE › VERSÕES LADO A LADO, em SÓ NO APP DO SEELE. A nota da 0.15 é sobre um banco que o celular não tem; nada no lugar | fora na 0.16: «Guardar e abrir outras versões é pelo app do SEELE. O navegador abre a página da versão do servidor que o link pede.» |
| PA-07 | ação | Rodapé **PERFIL**: retrato ou inicial, nome ou «—»; `title` «o nome com que você entra» | index.html:433-440; tela-boot.js:836-876 · `apelido_local`, `meu_retrato` | adaptado | Antes de entrar, o apelido é o campo SEU APELIDO da ENTRAR EM (N-01). Retrato e apelido moram em AJUSTES · SEU PERFIL. O `title` vira a nota visível «o nome com que você entra» | igual |
| PA-08 | ação | Engrenagem (aria «Abrir configurações»; `title` «configuração local desta máquina») | index.html:441; tela-server.js:1482 | adaptado | Aba AJUSTES (inventário E), com rótulo escrito em vez de glifo com `title` | igual |
| PA-09 | estado | Etapas: «LENDO O CONVITE», «LOCALIZANDO PELO PONTO DE ENCONTRO», «AVISANDO O PONTO DE ENCONTRO», «TENTANDO O ENDEREÇO n DE m · <endereço>», «O CAMINHO ATÉ AQUI ABRIU», «DENTRO», «A CHEGADA PAROU AQUI» (`ETAPAS`) | tela-boot.js:326-353; frases.js:298-355 · evento `ConnectStageChanged` | adaptado | Embaixo do ENTRAR da ENTRAR EM, com as mesmas frases. LOCALIZANDO e AVISANDO não acontecem: o navegador não abre UDP e perde o ponto de encontro (ADR 0057 «O alcance encolhe») | igual |
| PA-10 | estado | Erro em vermelho (`#boot-erro`) | index.html:448; tela-boot.js:303-311 | adaptado | Embaixo do ENTRAR da ENTRAR EM, visível sem rolar. Os casos com tela própria (N-03 a N-06) saem dela. Uma impressão de 64 caracteres quebra em várias partes a 360 pt | igual |
| PA-11 | estado | Botão de entrar apagado durante a tentativa, sem cancelar | tela-boot.js:182-187, 232, 313 | igual | O ENTRAR da ENTRAR EM fica apagado até a resposta. Não há cancelar, como no desktop | igual |
| **O caminho de entrar** | | | | | | |
| PA-12 | estado | Pergunta do nome «COMO OS OUTROS VÃO TE CHAMAR?» + `ENTRAR_SEM_NOME` «Você vai entrar com um nome tirado da sua chave. Dá para trocar no perfil.»; nome derivado `pessoa-XXXX` | tela-boot.js:180-211; camada-nomear.js:296-321 · `apelido_local`, `escolher_apelido_local`, `connect` | adaptado | Vira o campo SEU APELIDO da ENTRAR EM, já com o apelido deste aparelho. Vazio, mostra a nota `ENTRAR_SEM_NOME` embaixo. Não há caixa à parte nem teclado aberto sozinho. A ponte deriva o `pessoa-` da chave WebCrypto | igual |
| PA-13 | estado | Conferência de revogação antes de entrar; recusas `EstaVersaoRevogada` «ESTA VERSÃO DO SEELE FOI REVOGADA…» e `ListaDeRevogacaoRecusada` | tela-boot.js:245; frases.js:496-505, 876-878 · `conferir_esta_versao` | adaptado | Quem recusa é a origem: ela não serve pacote revogado (ADR 0057 «Onde a página mora»). A ponte responde que está tudo bem, e a frase da revogação fica para a página que a origem servir no lugar | igual |
| PA-14 | ação | `connect`, sempre com áudio (`audio:true`) | tela-boot.js:249-279 · `connect` | adaptado | Só depois do toque em ENTRAR, por WebTransport direto ao `seeled`. O áudio não abre na entrada; abre ao entrar numa sala de voz (D-52) | adaptado: WebTransport pela ponte; o áudio só abre ao entrar numa sala (D-52) |
| PA-15 | camada | Aceite de MODs (`ModsNaoAceitos`) | tela-boot.js:303; camada-mods.js:172-288 · `aceitar_mods`, `mods_instalados`, `instalar_mod_do_catalogo` | igual | Tela própria dos MODs, que rodam no celular (D-web-1); o desenho é da área de MODs. Esc e clique fora viram RECUSAR visível e o voltar do Android | igual |
| **Servidores conhecidos** | | | | | | |
| PA-16 | camada | **ONDE VOCÊ JÁ ESTEVE** + **FECHAR**, Esc, clique fora; abre com o foco no campo de endereço | index.html:3825-3860; camada-servidores.js:111-126, 200-212 · `conhecidos` | adaptado | Fora de servidor, é a tela de início (N-08). Dentro de um servidor, é a trilha da aba SERVIDOR. Não há caixa a fechar; o voltar do Android serve. Nada recebe foco sozinho, e o teclado só sobe ao tocar COLAR UM LINK | igual |
| PA-17 | estado | «Você ainda não entrou em nenhum servidor. Cole o endereço ou o link que alguém te mandou.» | index.html:3839-3840 | adaptado | Vira a frase da tela de início vazia (N-08), sem «endereço» (ver N-10) | igual |
| PA-18 | ação | Item de servidor: ladrilho, nome, endereço, «há N d»; o apelido daquela visita só no `title` («entrar em X como Y») | camada-servidores.js:55-95, 150-153 · `connect` | adaptado | Item da tela de início com ENTRAR. O apelido daquela visita vira texto no item («como <apelido>»). Com os hashes guardados vencidos, ENTRAR leva a N-03 | igual |
| PA-19 | ação | **REMOVER** (`title` «esquecer <nome>»; vermelho no hover; sem confirmação) | camada-servidores.js:97-104, 139-149 · `esquecer` | adaptado | «REMOVER» escrito no item, sem cor de hover. A confirmação vai numa folha e diz o custo que só o navegador tem: «Para voltar a este servidor, é preciso um link novo de quem hospeda.» Esquecer apaga os hashes, e sem eles não há como discar | igual |
| PA-20 | ação | Campo «outro endereço, ou um link seele://» + **ENTRAR**, com um link | camada-servidores.js:161-198 · `analisar_convite`, `connect` | adaptado | COLAR UM LINK (N-09): aceita o link https da origem e leva à ENTRAR EM, sem conectar. Um `seele://` colado recebe a frase de N-11 | adaptado: aceita o link `https` desta origem e leva à «ENTRAR EM» sem conectar; um `seele://` recebe a frase do N-11 |
| PA-21 | ação | O mesmo campo com um endereço cru | camada-servidores.js:167, 196-197 · `connect` | fora na 0.16 | Embaixo do COLAR UM LINK, a frase de N-10: «UM ENDEREÇO SOZINHO NÃO ABRE CONEXÃO NO NAVEGADOR…». O motivo é que no navegador não há primeiro contato cego (ADR 0057, emenda 0003) | fora na 0.16: a frase do N-10: no navegador não há primeiro contato cego |
| PA-22 | estado | Erros do convite: `EsquemaDesconhecido`, `SemEndereco`, `EnderecoInvalido`, `EnderecoIpv6SemColchetes`, `TokenInvalido`, `VoiceRoomInvalido`, `ImpressaoDigitalInvalida`, `BilheteInvalido` | frases.js:848-864 · `analisar_convite` | igual | As mesmas frases, embaixo do COLAR UM LINK. Exceções: `ImpressaoDigitalInvalida` e hash truncado vão para N-05, e `BilheteInvalido` não acontece (o link do navegador não leva `enc=`) | igual |
| PA-23 | estado | Troca com a sessão aberta: «TROCAR DE SERVIDOR» / «Você sai de X agora…» / **ENTRAR EM <nome>** | camada-servidores.js:228-235; tela-sessao.js:3638-3663, 4361 | adaptado | Pela trilha da aba SERVIDOR, com confirmação na folha. A consequência perde a parte de quem hospeda («…está no ar dentro deste computador…») | igual |
| PA-24 | estado/ação | Servidor de outra versão: confirmação «Este servidor roda o Servidor X. Ele abre noutra janela do SEELE…», **ABRIR** / **BAIXAR E ABRIR** / **VOLTAR**, «baixando…» / «abrindo…» / «abriu noutra janela…», `LinhaNaoOferecida`, recusas do lançador (`FRASES_DA_VERSAO`) | camada-versoes.js:36-184; camada-servidores.js:185-189 · `analisar_convite`, `baixar_versao`, `abrir_versao` | adaptado | Não há outra janela nem download: a página carrega o pacote da versão do servidor pelo `v=` (ADR 0057), e a ENTRAR EM mostra «Servidor X». Para uma versão sem página (0.15 e anteriores), N-04. As recusas do lançador não existem | adaptado: a página carrega o pacote da versão do servidor pelo `v=` (ADR 0057), sem outra janela |
| **Autenticação** | | | | | | |
| PA-25 | tela | «A·02 / ENTRADA NO SERVIDOR» em 3 colunas (≥ 820 px), com **ENTRAR NO SERVIDOR** depois do `connect`: «CONFERINDO IDENTIDADE…», abre o primeiro canal de texto | index.html:1931-2099; tela-auth.js:175-252, 362-501 · `snapshot`, `open_channel`, `lembrar_aparencia_do_servidor` | adaptado | A pergunta vem **antes** de conectar, na ENTRAR EM (N-01), em uma coluna. Depois do toque não há segunda parada: conecta, abre o primeiro canal de texto (a sala de voz, não) e mostra a sessão | igual |
| PA-26 | estado | Cartela «SEM CONEXÃO» / «CONEXÃO NÃO VERIFICADA» / «CONEXÃO SEGURA», com as notas | tela-auth.js:125-138, 261-269 | adaptado | Sai da entrada; o estado fica no cabeçalho da sessão (inventário B). «CONEXÃO NÃO VERIFICADA» não acontece no navegador: sem hash conferido o WebTransport não abre | igual |
| PA-27 | estado | **ENDEREÇO DO SERVIDOR** | index.html:1953-1963 | igual | Na ENTRAR EM, antes do toque | igual |
| PA-28 | estado | «O CONVITE JÁ CONFIRMOU A CHAVE DESTE SERVIDOR» / «A LISTA JÁ CONFIRMOU…» + impressão | tela-auth.js:292-300 | adaptado | Na ENTRAR EM, antes do toque, como «impressão do servidor, trazida pelo link». Quem confere é o navegador, contra os hashes do link, ao abrir a conexão | igual |
| PA-29 | estado | Ficha C·02: nome do servidor, **SALAS DE VOZ** nn, **CANAIS** nn | index.html:2069-2097; tela-auth.js:316-325 | adaptado | Antes do toque não há snapshot, então a ficha sai da entrada; nome, salas e canais aparecem na aba SERVIDOR | igual |
| PA-30 | estado | Vereditos `FirstContactVerified` «PRIMEIRO CONTATO VERIFICADO — O CONVITE CONFIRMOU A CHAVE» e `Known` «CHAVE CONFERIDA — NADA MUDOU DESDE A ÚLTIMA ENTRADA» | tela-auth.js:386-411; tela-sessao.js:237-299 | igual | Na faixa de veredito da sessão (inventário B) e em DETALHES DA CONEXÃO | igual |
| PA-31 | estado | Vereditos `FirstContact` «PRIMEIRO CONTATO — CHAVE FIXADA… Ninguém confirmou que é esta» e `InviteDisagrees` (as três frases, de link e de lista) | tela-sessao.js:242-297 | fora na 0.16 | Não acontecem no navegador. O primeiro contato cego não conecta e vira a frase de N-10. O pino é por impressão, então um link com outra impressão é outro servidor e `InviteDisagrees` não tem como nascer. Nenhuma frase no lugar | fora na 0.16: a frase do N-10: no navegador não há primeiro contato cego |
| PA-32 | estado | Erro ao entrar, escrito só no registro dentro do `<details>` fechado | tela-auth.js:498-500; index.html:2055 | adaptado | O erro aparece na tela, embaixo do botão, e também em DETALHES DA CONEXÃO | igual |
| PA-33 | estado | «SERVIDOR DESTA MÁQUINA — ENTRANDO» (entra sem toque) | tela-auth.js:219-232 | fora na 0.16 | Não acontece: o celular não hospeda (SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR). Nada no lugar | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-34 | estado | «Detalhes da conexão»: registro com hora, «RESOLVENDO <endereço>», «BATENDO DE NOVO», até 200 entradas | index.html:2055-2066; tela-auth.js:333-348 | igual | DETALHES DA CONEXÃO, recolhido embaixo da ENTRAR EM e da espera, em uma coluna | igual |
| PA-35 | estado | Espera `AdmissionPending`: **AGUARDANDO PERMISSÃO** (apagado); «AGUARDANDO APROVAÇÃO DE QUEM HOSPEDA. / Seu pedido está guardado no servidor.»; «Esta tela verifica a aprovação automaticamente enquanto estiver aberta.»; «Seu pedido fica guardado…»; «NOVA TENTATIVA EM n s» / «TENTANDO ENTRAR…» | index.html:2016-2036; tela-auth.js:544-628, 679-749 · `conferir_esta_versao`, `connect` | igual | Tela própria da espera, em uma coluna (no desktop ela já é uma coluna de até 640 px). Bate a cada 15 s, como no desktop | igual |
| PA-36 | estado | Espera, outra falha: **TENTANDO DE NOVO** + a frase da falha | tela-auth.js:556-591 | igual | Na mesma tela da espera | igual |
| PA-37 | estado | `AdmissionDenied`: **ENTRADA NEGADA**; «QUEM HOSPEDA RECUSOU A SUA ENTRADA. / Não foi a senha nem o convite…»; «Tentar de novo dá na mesma. Só quem hospeda pode mudar essa resposta.» | tela-auth.js:576-619; frases.js:79-81 | igual | Na espera. Se a recusa chega da ENTRAR EM, aparece embaixo do botão, a mesma assimetria do desktop | igual |
| PA-38 | estado | «ESPERA PAUSADA — VOLTE À JANELA» | tela-auth.js:681-686, 787-791 | adaptado | Mesma regra (`visibilityState`), sem a palavra «janela»: «ESPERA PAUSADA — VOLTE AO SEELE». Também pausa com a tela bloqueada | igual |
| PA-39 | estado | «PERMISSÃO CONCEDIDA — ENTRANDO NO SERVIDOR» (entra sem toque) | tela-auth.js:233-240 | igual | Na espera; abre só o canal de texto | igual |
| PA-40 | ação | **VOLTAR À ENTRADA** (para de bater; uma resposta tardia é descartada) | index.html:2048; tela-auth.js:554, 762-772 | adaptado | Botão visível que volta à tela de início; o voltar do Android faz o mesmo | igual |
| **Hospedar** | | | | | | |
| PA-41 | tela | **HOSPEDAR**: lista; «O SERVIDOR QUE JÁ ESTAVA AQUI»; **HOSPEDAR**; **APAGAR SERVIDOR** + «Apagar X, suas conversas e os dados dos MODs?…»; **NOVO SERVIDOR**; **VOLTAR** | index.html:465-484; tela-boot.js:681-778 · `servidores_guardados`, `hospedar`, `apagar_servidor` | fora na 0.16 | SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR, em SÓ NO APP DO SEELE. Não se chega aqui: o HOSPEDAR AQUI já é PA-03 | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-42 | tela | **PREPARAR ESTE SERVIDOR**: NOME DESTE SERVIDOR; **ESCOLHER IMAGEM** / **TIRAR**; VERSÃO QUE VAI HOSPEDAR com **USAR ESTA** / **ESCOLHIDA**; **HOSPEDAR**; **VOLTAR** | index.html:509-565; tela-boot.js:896-1132 · `regras_do_icone_do_server`, `linhas_para_hospedar`, `preparar_icone_do_server`, `criar_servidor`, `hospedar`, `renomear_server`, `aplicar_icone_do_server`, `baixar_versao`, `abrir_versao` | fora na 0.16 | SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR, em SÓ NO APP DO SEELE | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-43 | estado/ação | Subir o servidor: link, alcance («ESTA MÁQUINA TEM ENDEREÇO PRÓPRIO.» … «ESTE LINK SÓ FUNCIONA NA SUA REDE.»), `JaHospedando`, `PortaOcupada`, `NaoSubiu`, `NomeRecusado`, e **ABRIR A PORTA AGORA** | tela-boot.js:369-534 · `hospedar`, `abrir_a_porta_no_firewall` | fora na 0.16 | SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR, em SÓ NO APP DO SEELE | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-44 | camada | **A PORTA DESTE SERVIDOR**: CONVITE, **copiar**, **ABRIR CONFIGURAÇÕES**, **ENTENDI**, **FECHAR** | index.html:3770-3801; camada-portaria.js:704-809 | fora na 0.16 | SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR, em SÓ NO APP DO SEELE | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| **Versões e caminho entre pares** (também no inventário E) | | | | | | |
| PA-45 | estado | **GUARDADAS NESTA MÁQUINA**: versões, tamanhos, «conversas do Servidor X — … ficam mesmo apagando o app» | camada-versoes.js:201-277 · `versoes_instaladas`, `ocupacao_do_deposito` | adaptado | Vira ESTE APARELHO · versão da página: que pacote está carregado e para que servidor. Não há depósito nem tamanhos | fora na 0.16: «Guardar e abrir outras versões é pelo app do SEELE. O navegador abre a página da versão do servidor que o link pede.» |
| PA-46 | ação | **ABRIR** · **REMOVER** (versão) · **GUARDAR A MAIS NOVA DO CATÁLOGO** | camada-versoes.js:237-322 · `abrir_versao`, `remover_versao`, `baixar_versao` | fora na 0.16 | SÓ NO APP DO SEELE › VERSÕES LADO A LADO, em SÓ NO APP DO SEELE | fora na 0.16: «Guardar e abrir outras versões é pelo app do SEELE. O navegador abre a página da versão do servidor que o link pede.» |
| PA-47 | ação | **RECEBER A TELA DE QUEM TRANSMITE** · LIGAR/DESLIGAR | index.html:3407-3454; camada-servidores.js:264-324 · `caminho_entre_pares`, `consentir_no_caminho_entre_pares` | fora na 0.16 | Em AJUSTES, onde a escolha estaria: «A tela transmitida vem sempre do servidor neste aparelho.» E SÓ NO APP DO SEELE › A TELA ENTRE PESSOAS. O desenho não cita a malha; a classificação é desta seção | fora na 0.16: «No navegador, a tela de quem transmite vem sempre pelo servidor, e este computador não a repassa a ninguém.» |
| PA-48 | ação | **EMPRESTAR A SUA SUBIDA** · LIGAR/DESLIGAR | camada-servidores.js:326-330 · `consentir_no_caminho_entre_pares` | fora na 0.16 | SÓ NO APP DO SEELE › A TELA ENTRE PESSOAS. O navegador não aceita conexão de entrada | fora na 0.16: «No navegador, a tela de quem transmite vem sempre pelo servidor, e este computador não a repassa a ninguém.» |
| **Perfil fora de servidor** (também nos inventários B e E) | | | | | | |
| PA-49 | camada | **SEU PERFIL** sem sessão (FECHAR) | camada-nomear.js:181-242 · `snapshot`, `apelido_local`, `meu_retrato` | adaptado | AJUSTES · SEU PERFIL, no alto e com capa. Fora de servidor, o apelido é o da ENTRAR EM | igual |
| PA-50 | ação | **ESCOLHER** imagem («PNG · até 8 KiB · 256 px.») | camada-nomear.js:349-362 · `escolher_minha_imagem` | adaptado | Seletor do navegador. A redução que o Rust faz passa para a ponte, e a nota passa a dizer que uma imagem maior é reduzida. `IconNotAPicture` ganha frase; hoje aparece o nome cru | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| PA-51 | ação | **TIRAR** · **APELIDO** + **SALVAR** («Agora você é X.») | camada-nomear.js:364-436 · `tirar_minha_imagem`, `escolher_apelido_local` | igual | AJUSTES · SEU PERFIL | igual |
| PA-52 | ação | **CANCELAR** / FECHAR / Esc / clique fora | camada-nomear.js:441-458 | adaptado | CANCELAR e «‹ AJUSTES» visíveis, mais o voltar do Android. Não há Esc | igual |
| **Recusas da entrada** | | | | | | |
| PA-53 | estado | Falhas de rede e de TLS: «NÃO CONSEGUI RESOLVER ESSE ENDEREÇO», «NADA RESPONDEU NESSE ENDEREÇO», «O SERVIDOR RECEBEU E NÃO RESPONDEU A TEMPO», «OS PACOTES SAÍRAM E NADA VOLTOU… porta UDP… No Mac… Ajustes», «QUEM ATENDEU NÃO PROVOU SER ESTE SERVIDOR» | frases.js:765-801 · `connect` | adaptado | O Safari devolve `WebTransportError` sem motivo (ADR 0057), e a página não separa os casos. Uma frase só, embaixo do ENTRAR: «O SERVIDOR NÃO ABRIU A CONEXÃO.\nEle pode estar fora do ar, ou este link não é mais dele. Confira com quem hospeda.» Só a data do link separa o link vencido (N-03) | adaptado: a mesma frase do celular: o navegador não separa servidor fora do ar de chave trocada |
| PA-54 | estado | `PinChanged` «A CHAVE DO SERVIDOR MUDOU. / fixada… / ofertada…» e `InviteMismatch` («ESTE NÃO É O SERVIDOR DO CONVITE / QUE A LISTA GUARDOU…») | frases.js:506-570 · `connect` | fora na 0.16 | Embaixo do ENTRAR aparece a frase de PA-53. O navegador não mostra o certificado à página, e chave trocada chega igual a link vencido (ADR 0057, emenda 0003) | fora na 0.16: a frase de PA-53: o navegador não mostra o certificado à página |
| PA-55 | estado | `NicknameTaken` «O APELIDO «X» JÁ É DE OUTRA PESSOA NESTE SERVIDOR…» | frases.js:705-710 · `connect` | adaptado | Tela própria (N-06) | adaptado: o desfecho `apelido-tomado`, que não gasta o convite |
| PA-56 | estado | `Refused.*`: «CREDENCIAL RECUSADA», «SERVIDOR LOTADO», `Banned`, `ScheduledMaintenance`, `ServerShuttingDown`, `RateLimited`, `HandshakeTimeout`, `ModsRecusados`, `ModsIndisponiveis`, «SESSÃO RECUSADA»; `AlreadyConnected`, `NotConnected` | frases.js:20-159, 692-711, 763-764 · `connect` | igual | As mesmas frases, embaixo do ENTRAR | igual |
| PA-57 | estado | `Incompatible` «…atualize o SEELE nas duas máquinas…» e `ModsDeOutraApi` «…Atualize o app para entrar.» | frases.js:32-34, 125-127 · `connect` | adaptado | A página é a da versão do servidor, e não há app a atualizar: «ESTA PÁGINA NÃO FALA A VERSÃO DESTE SERVIDOR.\nAbra o link de novo; se continuar, avise quem hospeda.» | adaptado: a frase do celular: a página é a da versão do servidor |
| PA-58 | estado | `IdentityUnavailable` «NÃO CONSEGUI LER OU GRAVAR A IDENTIDADE EM DISCO» | frases.js:802 · `connect` | adaptado | Vira a frase do armazenamento do navegador (N-13) | adaptado: a frase do N-13: o armazenamento do navegador |
| PA-59 | estado | `NoAudioDevice` «SEM DISPOSITIVO DE ÁUDIO» na entrada | frases.js:803; tela-boot.js:256 | adaptado | Não acontece na entrada: o microfone só abre na sala (D-52), e a falta aparece lá (inventário D) | igual |
| PA-60 | estado | `CodigoNaoBate`, `SePassouPorCodigo`, `NaoLocalizou`, que caem em «ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ…» | frases.js:731-756 | fora na 0.16 | A ligação pelo código não existe no celular. SÓ NO APP DO SEELE › LIGAR PELO CÓDIGO, em SÓ NO APP DO SEELE | fora na 0.16: «Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa.» |
| **Fim** | | | | | | |
| PA-61 | tela | **ENLACE ENCERRADO** + motivo (abandona configurações, chamada, moderação e compartilhar); anúncio «Enlace encerrado. <frase>» | index.html:3708-3716; tela-fim.js:43-83 | igual | ENLACE ENCERRADO em tela cheia, por cima das abas e da faixa de voz | igual |
| PA-62 | estado | Motivos `Kicked`, `Banned`, `ServerFull`, `ScheduledMaintenance`, `ServerShuttingDown`, `Timeout`, `ProtocolViolation`, `RateLimited`, `FellBehind`, `CredentialRejected`, `HandshakeTimeout`, `AdmissionPending`, `AdmissionDenied`, `NicknameTaken`, `ModsRecusados`, `ModsMudaram`, `ModsIndisponiveis`, `LinkLost`; sem frase → «ENLACE ENCERRADO» | frases.js:20-159 | igual | As mesmas frases | igual |
| PA-63 | estado | Motivos `Incompatible`, `ModsDeOutraApi`, `ServidorTrocouNaVolta` «QUEM ATENDEU NA VOLTA NÃO É O SERVIDOR DESTA SESSÃO…», `ChaveMudouNaVolta` «A CHAVE DO SERVIDOR MUDOU DURANTE A SESSÃO…» | frases.js:32-34, 125-156 | adaptado | `Incompatible` e `ModsDeOutraApi` usam a frase de PA-57. As duas da volta não têm como ser vistas: a recusa na reconexão chega como falha de conexão, e vale a frase de PA-53 | igual |
| PA-64 | estado | Motivos `ChamadaEncerrada`, `NinguemAtendeu`, `ChamadaRecusada` | frases.js:120-122 | fora na 0.16 | A chamada privada não existe no celular (ADR 0057 «A decisão» 2). SÓ NO APP DO SEELE › LIGAR PELO CÓDIGO e inventário D | fora na 0.16: «Ligar pelo código é pelo app do SEELE: o navegador não abre o caminho que a ligação usa.» |
| PA-65 | ação | **RECONECTAR — <endereço>** (desmonta a sessão e mantém a hospedagem) | tela-fim.js:91-138 · `desmontar_a_sessao`, `conferir_esta_versao`, `connect` | adaptado | O botão diz «RECONECTAR A <nome do servidor>», com o nome da lista deste aparelho e o endereço embaixo. Com os hashes vencidos, leva a N-03. A distinção de manter a hospedagem não existe no celular | adaptado: com os hashes guardados vencidos, leva ao `vencido` |
| PA-66 | ação | **SAIR**, e o SAIR único quando não há destino | tela-fim.js:33-41, 91-106, 141-197 · `disconnect` | igual | Volta à tela de início | igual |
| **Transversais** | | | | | | |
| PA-67 | estado | Abertura pelo lançador `--entrar <link>`, que **conecta sem toque** | tela-boot.js:630-652 · `abertura`, `analisar_convite`, `connect` | adaptado | O link https abre a ENTRAR EM e espera o toque (ADR 0057, emenda 0006) | adaptado: a ponte responde: `plataforma` é `"web"`, e a `abertura` nunca traz `entrar` |
| PA-68 | estado | Abertura pelo lançador `--hospedar [--nome-publico]` | tela-boot.js:653-657 · `abertura`, `hospedar` | fora na 0.16 | SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR. Nada no lugar: não há lançador | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| PA-69 | estado | Recarregar bloqueado (F5, Ctrl+R, ⌘R) e menu de contexto suprimido | base.js:326-380 | adaptado | Não há F5 nem clique-direito. Recarregar, ou puxar para atualizar, derruba a sessão, que no navegador mora na página; ela volta na tela de início | igual |
| PA-70 | estado | Barra da janela sobre todas as telas: minimizar, maximizar, fechar, arrastar, relógio, nome do servidor | index.html:98-113 | adaptado | «não há janela: o sistema desenha a barra». O nome do servidor na sessão é do inventário B | adaptado: a janela é do navegador: os controles e o relógio do Tauri saem |
| PA-71 | estado | Foco e anúncio em cada troca de tela (`data-foco`, `#anuncio`) | base.js:224-276 | igual | Foco em botão não abre teclado. Os dois focos em campo de texto (PA-12, PA-16) saem, como dito naqueles itens | igual |

---

### Casos novos (o desktop não tem)


As frases abaixo são propostas.

| id | tipo | caso | estado | frase proposta | onde / por quê | PC (navegador) |
|---|---|---|---|---|---|---|
| N-01 | tela | O link aberto | novo | Título «ENTRAR EM <endereço>?»: antes de conectar, o nome do servidor é só o que o link diz (decisão da orquestradora, 09/10). Campos «ENDEREÇO», «IMPRESSÃO DO SERVIDOR, TRAZIDA PELO LINK», «SERVIDOR <versão>», «LINK VÁLIDO ATÉ DD/MM», «SEU APELIDO». Botões **ENTRAR** / **AGORA NÃO**. Nota «Este aparelho só se conecta ao servidor depois de ENTRAR.» | A tela do fragmento, depois do `replaceState` (ADR 0057, emenda 0006). AGORA NÃO leva à tela de início | igual |
| N-02 | tela | Link aberto numa aba do Safari | novo | «ABRA PELA TELA DE INÍCIO ANTES DE ENTRAR.\nO convite vale uma vez, e esta aba e o SEELE da Tela de Início guardam identidades separadas. Copie o link, adicione o SEELE à Tela de Início e cole o link lá.» Botões **COPIAR O LINK** / **ENTRAR NESTA ABA** (nota: «A identidade fica só nesta aba.») | Antes de gastar o convite (ADR 0057 «O que isto custa»; bug 181849 do WebKit) | adaptado: pede para instalar só onde a aba e o app instalado guardam a identidade em separado (Safari do Mac e do iPhone) |
| N-03 | tela | Link vencido | novo | «ESTE LINK VENCEU EM DD/MM.\nO navegador só entra com um link em dia. Peça um link novo a quem hospeda.» | Pela data do link, antes de discar. Também no ENTRAR da tela de início e no RECONECTAR quando os hashes guardados venceram | igual |
| N-04 | tela | Link de Servidor 0.15, ou anterior | novo | «ESTE SERVIDOR RODA O SERVIDOR 0.15, QUE NÃO ABRE NO CELULAR.\nEntre pelo app do SEELE, ou peça a quem hospeda para passar à 0.16.» (com a versão lida do `v=`) | Pelo `v=`, antes de discar. Absorve o `LinhaNaoOferecida` do desktop | igual |
| N-05 | tela | Link incompleto | novo | «ESTE LINK CHEGOU INCOMPLETO.\nFalta a parte que confere a chave do servidor. Peça o link inteiro de novo.» | Fp ou hash truncado ou ausente: na forma https os dois são obrigatórios (ADR 0057, emenda 0006) | igual |
| N-06 | tela | `NicknameTaken` | novo | «O APELIDO «X» JÁ É DE OUTRA PESSOA NESTE SERVIDOR.\nSe era você, este aparelho pode ter perdido a identidade: quem hospeda libera o nome. O convite continua valendo: escolha outro apelido e entre de novo.» Nota: «O app do SEELE e este aparelho são duas pessoas para o servidor.» | A F1 confere o apelido antes de gastar o convite (já em `versao/0.16`); o ADR 0057 ainda descreve a ordem antiga | igual |
| N-07 | estado | Resposta do `navigator.storage.persist()` | novo | Concedido: «O NAVEGADOR VAI GUARDAR A IDENTIDADE DESTE APARELHO.» Negado: «O NAVEGADOR NÃO PROMETEU GUARDAR A IDENTIDADE DESTE APARELHO.\nSe ele apagar os dados deste site, este aparelho entra nos servidores como outra pessoa.» | Em ESTE APARELHO · armazenamento, e uma vez logo depois da primeira entrada (ADR 0057 «O cliente web») | igual |
| N-08 | tela | Tela de início, aberta sem link | novo | Título «SERVIDORES DESTE APARELHO», com um item por servidor (nome, endereço, «como <apelido>», «há N d») e **ENTRAR**. Vazia: «Este aparelho ainda não entrou em nenhum servidor.\nAbra o link que alguém te mandou, ou cole aqui.» Mais **COLAR UM LINK** e a frase de PA-03 | O `start_url` do manifest, que não tem fragmento | igual |
| N-09 | ação | COLAR UM LINK | novo | Campo com placeholder «o link que alguém te mandou» + **CONTINUAR**, que leva à ENTRAR EM sem conectar | O teclado só sobe com este toque | igual |
| N-10 | estado | Endereço digitado sem link | novo | «UM ENDEREÇO SOZINHO NÃO ABRE CONEXÃO NO NAVEGADOR.\nO link traz a chave do servidor, e sem ela o navegador não conecta. Peça o link a quem hospeda.» | Embaixo do COLAR UM LINK. No navegador não há primeiro contato cego (ADR 0057, emenda 0003) | igual |
| N-11 | estado | Um `seele://` colado | novo | «ESTE LINK É PARA O APP DO SEELE.\nNo navegador ele não traz o que confere o servidor. Peça a quem hospeda o link do navegador.» | Embaixo do COLAR UM LINK | igual |
| N-12 | estado | Primeira entrada neste aparelho | novo | Nota na ENTRAR EM: «Ao entrar, este aparelho cria a identidade dele. Ela fica neste navegador: sem cópia e sem troca de aparelho.» | A chave só nasce no ato de entrar com convite (ADR 0057, emenda 0017; 0004) | igual |
| N-13 | estado | Armazenamento recusado (navegação privada, dados bloqueados) | novo | «ESTE NAVEGADOR NÃO DEIXOU GUARDAR A IDENTIDADE.\nSem ela não há como entrar. Abra o link numa aba comum, fora da navegação privada, ou pela Tela de Início.» | Embaixo do ENTRAR. É o `IdentityUnavailable` do navegador | igual |
| N-14 | estado | O navegador pede acesso à rede local | novo (não medido) | «O NAVEGADOR PODE PEDIR PARA ACESSAR A REDE LOCAL.\nEste servidor está na sua rede; sem essa permissão, o SEELE não chega a ele.» | Na ENTRAR EM, quando o endereço é de rede local. Chrome 147, segundo o ADR 0057 «O que não foi medido»; não medido | igual |

---

### Achados que importam para o desenho


1. **O desktop conecta antes de perguntar.** O `connect` já fez aperto de mão e sessão quando aparece ENTRAR NO SERVIDOR, e no modo `aperto` não há voltar (tela-boot.js:249-279; tela-auth.css:155). A ENTRAR EM inverte a ordem, então `#tela-auth` não serve de molde.
2. **Se a ponte responder `abertura` com `entrar`, a casca de hoje conecta sem toque** (tela-boot.js:642-645). O fragmento do link não pode chegar por esse caminho.
3. **Cinco `invoke` rodam na carga, sem gesto nenhum**: `versoes_instaladas`, `abertura`, `apelido_local`, `meu_retrato` e `estado_da_primeira_abertura` (tela-boot.js:793-796, 876; camada-versoes.js:358). A ponte tem de responder aos cinco antes de a tela de início existir.
4. **Tentar de novo vai sem o token.** A batida da espera e o RECONECTAR mandam `joinSecret: null` (tela-auth.js:741-749; tela-fim.js:121-138), e o comentário em tela-fim.js:117-119 diz o contrário. Com convite de uso único, convém decidir se a espera do celular precisa do token.
5. **Nenhuma tela desta área foi desenhada abaixo de 720 px.** `#tela-auth` exige ~820 px sem `@media` (tela-auth.css:10-15), e a janela do desktop não desce de `minWidth: 720` (tauri.conf.json:16).
6. **«O produto sabe e não conta», duas vezes.** O erro ao entrar fica num `<details>` fechado (tela-auth.js:498-500; index.html:2055), e a trilha dos candidatos só vai ao console (tela-boot.js:290-293). No celular o console não é alcançável, e o canal de diagnóstico está EM ABERTO (ADR 0057:476-479).
7. **Informação só no `title`, e foco que abre teclado.** Ficam só no `title` o apelido de cada visita (camada-servidores.js:94), o perfil e a engrenagem (index.html:434, 443). Recebem foco ao abrir o campo de endereço (camada-servidores.js:117) e o campo do apelido (camada-nomear.js:306).
8. **REMOVER apaga sem confirmar** (camada-servidores.js:139-149). No desktop isso perde um atalho; no navegador perde os hashes, e voltar ao servidor passa a exigir um link novo.
9. **`audio:true` vai em todo `connect`** (tela-boot.js:256), contra o D-52. A ponte ignora o campo, ou a casca muda. Com isso o `NoAudioDevice` deixa de existir na entrada.
10. **Recarregar é inofensivo no desktop e fatal no celular.** O desktop bloqueia o F5 porque a sessão vive no Rust (base.js:326-380). No navegador a sessão vive na página, e recarregar, ou o sistema descarregar a aba, derruba a sessão.

## 2. A moldura da sessão

Feita a partir de `B-moldura.md` (commit `f7bc902b3`) e de `00-desenho-e-regras.md`, sem rediscutir o desenho. Os ids são os do inventário, com prefixo `B-`. Quando um id cobre vários itens do inventário, é porque é o mesmo item da tela; nenhuma ação nem estado visível ficou de fora. Toda frase entre «» vai para o `frases.js`.

### A matriz


| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| B-S01 | tela | `#tela-sessao` «Operação. Salas de voz, canais e a conversa do canal aberto.»; o foco fica na própria seção | index.html:568–577; base.js:243–257 | adaptado | Quatro abas no pé (SERVIDOR · CONVERSA · PESSOAS · AJUSTES), e cada aba se anuncia ao abrir. Não há tecla de falar para proteger com o foco. | igual |
| B-S02 | estado | (interno) redesenho a 500 ms; histórico lido no 1º quadro; ao sair, tudo zerado | tela-sessao.js:374–380, 3742–3815, 4447–4491 · `snapshot`, `conhecidos`, `disconnect`, `busca_limpar` | igual | A página redesenha pelo estado da sessão. Ao sair, zera tudo e volta à tela de início (os servidores deste aparelho). | igual |
| B-S04 | estado | fim da sessão → tela de fim | tela-sessao.js:369–372, 4449–4452; tela-fim.js:43 | igual | Tela de fim em tela cheia, fora das abas. | igual |
| B-S06 | estado | a voz abre ao conectar; fora de sala, «microfone aberto · ouvindo» | seele-ffi lib.rs:4078–4091; tela-sessao.js:1869–1906, 994–1002 | adaptado | O áudio abre ao entrar numa sala e fecha ao sair (D-52). Fora de sala o microfone não capta, e nada diz «microfone aberto». | igual |
| B-S07 | estado | regiões vivas: `#anuncio`, `#banner` (alert), `#veredito`, `#aviso-fone`, `#bateria`, `#voz-estado`, aviso de MOD | index.html:150; base.js:209–217 | igual | As mesmas regiões vivas, por aba. | igual |
| B-S08 | estado | recarregar bloqueado (F5, Ctrl+R, ⌘R); menu de contexto bloqueado | base.js:326–379 | adaptado | Não há teclado nem menu de contexto. Recarregar é do navegador, derruba a sessão da página e volta à tela de início (achado 9). | adaptado: o navegador recarrega e a sessão cai com a página; com a sessão aberta, a página pergunta antes de sair (`beforeunload`) |
| B-S09 | estado | altura `100vh − 32px`; a página não rola | base.css:60, 72–77 | adaptado | A altura é a área visível do navegador, menos as abas e, em sala, a faixa de voz. | igual |
| B-J01 | tela | `SEELE`; barra que arrasta a janela | index.html:80–113; base.css:524–556 | adaptado | «não há janela: o sistema desenha a barra» | adaptado: a janela é do navegador: os controles e o relógio do Tauri saem |
| B-J02 | estado | nome do servidor (cortado; inteiro só no `title`; sem nome, `—` com title «este servidor não anunciou nome»); esvaziado ao sair | tela-sessao.js:430–441, 464–474 · `snapshot` | adaptado | Nome inteiro escrito no alto da aba SERVIDOR, em até duas linhas. Sem nome, escreve «este servidor não anunciou nome». | igual |
| B-J03 | estado | `SERVIDOR · SEM CONEXÃO` / `· CONEXÃO NÃO VERIFICADA` / `· CONEXÃO SEGURA` | tela-sessao.js:443–462; base.css:599–607 · `snapshot` | igual | Sob o nome, no alto da aba SERVIDOR, com as mesmas palavras e cores. | igual |
| B-J05 | estado | relógio `--:--:--` | base.js:408–411 | adaptado | «não há janela: o sistema desenha a barra» | adaptado: a janela é do navegador: os controles e o relógio do Tauri saem |
| B-J06 | ação | `–` minimizar · `□` maximizar · `×` fechar (sem confirmação) | base.js:420–437 · Tauri `getCurrentWindow()` | adaptado | «não há janela: o sistema desenha a barra» | adaptado: a janela é do navegador: os controles e o relógio do Tauri saem |
| B-J09 | estado | plataforma (recuo de 78px no macOS) | base.js:392–406 · `plataforma` | adaptado | «não há janela: o sistema desenha a barra» | adaptado: a janela é do navegador: os controles e o relógio do Tauri saem |
| B-F01 | camada | `#banner`: `ALERTA` · `SERVIDOR · AVISO` · `INFORMATIVO` / `ATENÇÃO` / `CRÍTICO` · frase de `AVISOS` (`SALA DE VOZ LOTADA`, `ENTRADA NA SALA DE VOZ RECUSADA`, `NÃO DEU: <causa>`, `… N MENSAGENS`…) | index.html:635–705; tela-sessao.js:2592–2640; frases.js:161–226 · `snapshot` | adaptado | Caixa de largura cheia sobre a aba atual, com a cabeça empilhada (ALERTA e a severidade em cima, a frase embaixo) e as mesmas frases. `LastVoiceRoom`, `ScreenShareTaken` e `ScreenShareOverHostUplink` não chegam a quem não apaga sala nem compartilha. | igual |
| B-F03 | ação | `×` «Fechar o alerta» | tela-sessao.js:4301–4306 · `dispensar_aviso` | igual | `×` de 44px na cabeça da caixa; o voltar do Android também fecha. | igual |
| B-F05 | ação | `MODERAR UMA PESSOA` (apagado, com title, para quem não tem permissão) | camada-moderar.js:240–247, 805–811 | fora na 0.16 | No lugar do botão, só para quem tem `may_kick`, `may_ban` ou `may_move_person`: «MODERAR: pelo SEELE no computador.» Sem permissão, nada. Também em NO COMPUTADOR. | igual |
| B-F06 | camada | `#veredito` («PRIMEIRO CONTATO — CHAVE FIXADA…», «PRIMEIRO CONTATO VERIFICADO — O CONVITE / A LISTA CONFIRMOU A CHAVE», «O CONVITE NÃO CORRESPONDE A ESTE SERVIDOR…», esperada/ofertada) · `dispensar` | tela-sessao.js:237–299, 354–362, 4317; tela-auth.js:471 | igual | Faixa laranja no alto da aba SERVIDOR, com `dispensar` e a impressão em linhas próprias. As frases saem do JS para o `frases.js`. | igual |
| B-F08 | camada | `#aviso-fone` (`AVISO_DE_FONE`) · `dispensar` | tela-sessao.js:313–347, 4320; frases.js:285 | igual | Faixa acima da faixa de voz, na primeira entrada numa sala desde que a página abriu, com `dispensar`. | igual |
| B-F08b | camada | `#aviso-audio` (`ABERTURA_DO_AUDIO.nao_abriu_no_prazo.frase`), sem dispensar, acesa enquanto o estado valer | index.html, `#aviso-audio`; tela-sessao.js, `desenharAberturaDoAudio`; frases.js, `ABERTURA_DO_AUDIO` · `snapshot` | igual | A mesma faixa, acima da faixa de voz e junto do aviso de fone, sem dispensar: ela apaga sozinha quando a pessoa sai da sala. Entrou na `versao/0.16` depois do levantamento (`2b32795f`, pendência 52), e chegou à F8 em `c8b3cd47`. | igual |
| B-F08c | estado | a abertura do áudio no rodapé (`ABRINDO`, `SEM ÁUDIO`) e no medidor (`— FORA DE SALA`, `— ABRINDO O ÁUDIO`, `— SEM ÁUDIO NESTA SALA`), lida de `snapshot.abertura_do_audio` | tela-sessao.js, o rodapé; tela-server.js, o medidor; frases.js, `ABERTURA_DO_AUDIO` · `snapshot` | adaptado | A faixa de voz diz ABRINDO enquanto o áudio abre e SEM ÁUDIO quando ele não abriu, no lugar do resumo de quem fala; o medidor de AJUSTES › ÁUDIO diz o mesmo que o do desktop. O que a ponte põe em `abertura_do_audio` quando o navegador recusa o microfone é da F4 (achado 3). | igual |
| B-F10 | camada | `#bateria`: `CONEXÃO PERDIDA · RECONECTANDO` · `MM:SS` · `ATÉ A SESSÃO SER ENCERRADA` · «Sessão mantida em memória local. Áudio suspenso.» · `TENTATIVAS DE RECONEXÃO` · `reconectando…` / `N tentativas` · «O SEELE já está tentando sozinho…»; esgotada, tela de fim | tela-sessao.js:2654–2678; index.html:762–823; camada-bateria.css · `snapshot` | adaptado | Caixa de largura cheia, contagem em cima e frases embaixo; abas e faixa esmaecidas; rolar por baixo continua possível. Esgotada, tela de fim. | igual |
| B-F12 | camada | `#portaria-batendo` («ALGUÉM ESTÁ BATENDO À PORTA DESTE SERVIDOR», `VER QUEM É`, `DEPOIS`) e `#portaria` (`ADMITIR`, `RECUSAR`, `REVOGAR`, `DEFINIR`, `TIRAR`, `GERAR`, `LIGAR`) | camada-portaria.js:160–631; index.html:192–197, 844–924 · `estado_da_porta`, `pedidos_da_portaria`, `decidir_pedido`, `revogar_admissao`, `definir_senha_do_server`, `criar_convite_do_server`, `ligar_portaria` | fora na 0.16 | Nada no lugar: ninguém hospeda no celular. Em SÓ NO APP DO SEELE: «HOSPEDAR E DECIDIR QUEM ENTRA: pelo app do SEELE.» | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| B-F16 | camada | `#avisos-de-mod` (título do MOD; some em 3,5 s ou 10 s; no máximo 4) | mods-superficies.js:740–748; mods-superficies.css:366–389 | igual | Fila acima das abas e da faixa de voz, na largura toda menos 16px, com os mesmos tempos e o mesmo teto. | igual |
| B-F17 | camada | caixa de confirmação: título · consequência · `CANCELAR` · verbo | camada-moderar.js:432–449, 855–877 | adaptado | Folha de baixo com a frase, `CANCELAR` e o verbo; `FECHAR` sempre visível; o voltar do Android fecha. | igual |
| B-T01 | tela | `REDES` (coluna de 60px) | index.html:963–999; tela-sessao.css:246–360 | adaptado | Faixa deitada no alto da aba SERVIDOR, que rola na horizontal. | igual |
| B-T02 | estado | servidor atual (sigla ou imagem, marcado; nome inteiro só no `title`) | tela-sessao.js:557–585 · `icone_do_server` | adaptado | Primeiro item da faixa, marcado, com o nome inteiro escrito sob a sigla ou a imagem. | igual |
| B-T03 | estado | histórico de servidores (sigla do nome ou do endereço; «nome · endereço» só no `title`) | tela-sessao.js:534–543, 587–624 · `conhecidos` | adaptado | Os servidores deste aparelho, lidos do armazenamento da página, com o nome escrito sob cada um. | igual |
| B-T04 | ação | toque → `TROCAR DE SERVIDOR` · `ENTRAR EM <nome ou endereço>` | tela-sessao.js:3638–3711, 4358–4362 · `estado_da_porta`, `disconnect` | adaptado | Folha de baixo `TROCAR DE SERVIDOR` com a consequência e `ENTRAR EM <nome>` (o nome, não o endereço: achado 6). Sem a frase de quem hospeda. | igual |
| B-T05 | ação | `+` «Conectar a outro servidor» → `CONECTAR A OUTRO SERVIDOR` · `SAIR DE <servidor>` | tela-sessao.js:3665–3675, 3733–3739, 4364 · `estado_da_porta`, `disconnect` | adaptado | `+` no fim da faixa. A folha diz `SAIR DE <servidor>` e leva à tela de início (servidores deste aparelho, COLAR UM LINK), não a um campo de endereço. | igual |
| B-T06 | estado | «<servidor> está no ar dentro deste computador: ele cai junto…» | tela-sessao.js:3599–3621 · `estado_da_porta` | fora na 0.16 | Não aparece: o celular nunca hospeda. A frase de HOSPEDAR em SÓ NO APP DO SEELE cobre (B-F12). | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| B-C01 | tela | coluna «Salas de voz e canais» (268px, rola) | index.html:1011–1077 | adaptado | Corpo da aba SERVIDOR, abaixo da faixa REDES: SALAS DE VOZ, CANAIS e DOS MODS, numa lista só que rola. | igual |
| B-C02 | estado | cabeçalhos `SALAS DE VOZ` e `CANAIS` | index.html:1027–1053 | igual | Aba SERVIDOR. | igual |
| B-C03 | ação | `+` → `NOVA SALA DE VOZ` / `CRIAR`; nome da sala → `RENOMEAR A SALA`; `×` → `APAGAR A SALA DE VOZ <sala> ?` (desabilitado na última, com title); `+` → `NOVO CANAL`; `×` → `APAGAR O CANAL #<c> ?` (com a contagem); lápis → `RENOMEAR O CANAL` | tela-sessao.js:717–744, 4230–4285; camada-moderar.js:163–221, 721–803 · `criar_voice_room`, `renomear_voice_room`, `apagar_voice_room`, `criar_linha`, `renomear_linha`, `peso_da_linha`, `apagar_linha`, `snapshot`, `open_channel` | fora na 0.16 | Para quem tem `may_manage_voice_rooms` ou `may_delete_rooms`, uma vez sob SALAS DE VOZ, no lugar dos `+`: «CRIAR, RENOMEAR E APAGAR SALAS E CANAIS: pelo SEELE no computador.» O nome da sala é só texto. Também em NO COMPUTADOR. | igual |
| B-C04 | ação | `?` «Ajuda: o vocabulário do SEELE e as teclas» | camada-ajuda.js:31–60 | adaptado | `?` no cabeçalho SALAS DE VOZ abre a ajuda numa folha, sem a parte das teclas. | igual |
| B-C05 | estado | sala ocupada por nós (borda laranja, fundo); nome; `n/limite`; quem está dentro (`(você)`, `mudo`, `fala`, `ninguém aqui`) | tela-sessao.js:703–760, 894–908 · `snapshot` | igual | Aba SERVIDOR. A sala ocupada também ganha «você está aqui» escrito. | igual |
| B-C08 | estado | sala com senha: nenhuma marca | tela-sessao.js:798 · `snapshot` (`password_required`) | adaptado | «COM SENHA» escrito ao lado de `n/limite`. | igual |
| B-C09 | estado | sala cheia: nenhuma marca, nenhum bloqueio | tela-sessao.js:751, 790–802; frases.js:167 | adaptado | «LOTADA» escrito ao lado de `n/limite` quando `n = limite`. ENTRAR continua, e quem recusa segue sendo o servidor (`SALA DE VOZ LOTADA`). | igual |
| B-C10 | ação | `ENTRAR NA SALA` (senha por `window.prompt("Senha da sala")`) | tela-sessao.js:3433–3498 · `enter_voice_room`, `snapshot` | adaptado | Sala com senha abre a folha `SENHA DA SALA` (campo, ENTRAR, FECHAR). Entrar abre o áudio (D-52), encaixa a faixa de voz e abre a CHAMADA em tela cheia, como o desktop abre a grade. | igual |
| B-C11 | ação | `VER A CHAMADA` (sala ocupada) | tela-sessao.js:3440–3444, 3494 | igual | No item da sala ocupada, na aba SERVIDOR; tocar a faixa de voz faz o mesmo. | igual |
| B-C16 | estado | canais `# nome` (+ `canal.item`), o aberto marcado; sem contagem de não lidas | tela-sessao.js:807–854 | igual | Aba SERVIDOR. | igual |
| B-C17 | ação | tocar canal (rascunho por canal); o 1º abre sozinho | tela-sessao.js:1168–1178, 3461–3483 · `open_channel`, `snapshot` | adaptado | O toque abre o canal e leva à aba CONVERSA (no desktop fecha a gaveta). O rascunho fica com o canal, e o 1º canal abre sozinho. | igual |
| B-C21 | ação | `DOS MODS` (rótulo + origem) → ação ao MOD | tela-sessao.js:2438–2477; base.js:1751–1768 | igual | Aba SERVIDOR, depois de CANAIS. Uma `pagina` de MOD abre na aba CONVERSA. | igual |
| B-O01 | tela | tira do operador, dentro da coluna de canais | index.html:1087–1200 | adaptado | A tira se divide: quem sou e ⚙ vão para AJUSTES; MUDO, SURDO e SAIR para a faixa de voz (em sala); SAIR DO SERVIDOR para o fim de AJUSTES. | igual |
| B-O02 | ação | `OPERADOR` · apelido · avatar → `SEU PERFIL` (`ESCOLHER`, `TIRAR`, apelido, `SALVAR` / `CANCELAR` / `FECHAR`) | tela-sessao.js:910–919; camada-nomear.js:136–458 · `imagem_da_pessoa`, `snapshot`, `meu_retrato`, `escolher_minha_imagem`, `tirar_minha_imagem`, `escolher_apelido` | adaptado | SEU PERFIL no alto de AJUSTES, com capa. `ESCOLHER` usa o seletor de arquivos do navegador. | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| B-O04 | ação | microfone (glifo cortado; `MICROFONE MUDO` / `… · NO AR` só no aria e no title) | tela-sessao.js:942–957, 4322–4326 · `snapshot`, `set_muted` | adaptado | MUDO na faixa de voz e na CHAMADA, com o estado escrito sob o glifo. | igual |
| B-O06 | ação | fone (`OUVINDO` / `ISOLAMENTO TOTAL`; title «não ouvir ninguém, nem o som de uma tela compartilhada») | tela-sessao.js:959–976, 4328–4332 · `snapshot`, `set_total_isolation` | adaptado | SURDO na faixa de voz e na CHAMADA. Ligado, a faixa escreve «não está ouvindo nada, nem a tela». | igual |
| B-O08 | ação | `⚙` «Abrir configurações» | tela-server.js:734–777, 1483 | adaptado | Aba AJUSTES. | igual |
| B-O09 | estado | `#voz-estado` («microfone abre enquanto segura ESPAÇO · ouvindo · no ar»…) | tela-sessao.js:978–1002 | adaptado | A frase vai na faixa de voz. No modo SEGURAR PARA FALAR diz «microfone abre enquanto segura o botão» no lugar do nome da tecla. | igual |
| B-O10 | ação | `SAIR DO SERVIDOR` (direto; com confirmação só para quem hospeda) | tela-chamada.js:765–795 · `estado_da_porta`, `disconnect` | adaptado | `SAIR DO SERVIDOR` no fim de AJUSTES, sem confirmação (o celular nunca hospeda); volta à tela de início. | igual |
| B-O11 | ação | `SAIR DA SALA` | tela-chamada.js:765–780 · `leave_voice_room` | igual | SAIR na faixa de voz e na CHAMADA. Fecha o áudio (D-52), e a faixa sai. | igual |
| B-O12 | ação | `COMPARTILHAR` (só em sala) | tela-chamada.js:730–752 · `fontes_de_tela`, `compartilhar_tela` | fora na 0.16 | Na CHAMADA, no lugar do botão: «COMPARTILHAR A TELA: pelo SEELE no computador.» Também em NO COMPUTADOR. | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida); sem módulo de vídeo a baixar |
| B-O13 | estado | não há botão de falar: só a tecla segurada | index.html:1127–1139; tela-sessao.js:3517–3554, 4407–4443 · `tecla_de_falar`, `set_talking` | adaptado | Botão grande SEGURAR PARA FALAR na faixa de voz. Só fala com o dedo nele, e fecha em touchcancel, ao sair do botão, com a página oculta e na queda. | adaptado: a tecla de falar só vale com a página em foco: o navegador não tem atalho global, e a tela diz isso |
| B-P01 | tela | `PESSOAS` · `N EM SALAS DE VOZ` / `N EM SALAS · M NO SERVIDOR` | index.html:1671–1694; tela-sessao.js:1965–1973 | adaptado | Aba PESSOAS, com a contagem no alto e a lista na largura toda. | igual |
| B-P03 | estado | grupo por sala (nome, `n/limite`, `você está aqui`, `sala.acoes`) | tela-sessao.js:1825–1867, 1977–2006 | igual | Aba PESSOAS. | igual |
| B-P04 | estado | `FORA DE SALA`, com meu cartão (sinal; `MICROFONE ABERTO` quando capto) | tela-sessao.js:1875–1906 | adaptado | Meu cartão no grupo FORA DE SALA, sem a pastilha de microfone: fora de sala o áudio está fechado (D-52). | igual |
| B-P05 | estado | `NO SERVIDOR, FORA DAS SALAS` | tela-sessao.js:1908–1959 | igual | Aba PESSOAS, depois das salas. O FORA DO AR do desenho vem depois dele (achado 7). | igual |
| B-P06 | estado | cartão (`falando`, faixa); `SINAL E ESTADO` recolhível; marca `█` `▒` `░` + número + barra; pastilhas `MUDO` / `TRANSMITINDO` / `MICROFONE ABERTO` / `EM ESCUTA` / `ISOLAMENTO TOTAL` | tela-sessao.js:2235–2378 | igual | Aba PESSOAS. O recolhível abre com um toque, e sozinho nos mesmos casos do desktop. | igual |
| B-P10 | ação | volume por pessoa (invisível até o hover) | tela-sessao.js:2395–2407, 4337–4345; tela-sessao.css:1846–1856 · `set_volume` | adaptado | Um toque no cartão abre a folha da pessoa, com VOLUME sempre visível e o número escrito (§3). | igual |
| B-P11 | ação | nome → `MODERAÇÃO` (`EXPULSAR`; `BANIR` com `ATÉ QUANDO` e `MOTIVO`; `MOVER PARA`) | tela-sessao.js:2256–2267; camada-moderar.js:403–424, 694–698, 813–877 · `snapshot`, `expulsar_pessoa`, `banir_pessoa`, `mover_pessoa` | fora na 0.16 | Na folha da pessoa, para quem tem `may_kick`, `may_ban` ou `may_move_person`: «MODERAR: pelo SEELE no computador.» Também em NO COMPUTADOR. | igual |
| B-P12 | estado | linha desenhada pelo MOD + `⋯` «Detalhes de <nome>»; porta do provedor `PERFIL` | tela-sessao.js:2080–2204 | adaptado | O cartão do MOD fica, com o apelido verificado sempre à vista ao lado (D-nucleo-1). Os detalhes e a ação do provedor vão para a folha da pessoa. | igual |
| B-P14 | estado | `pessoa.identidade`, `pessoa.acoes`, `pessoa.cartao` (adicionar), cartões | tela-sessao.js:2112–2121, 2289–2349, 2391–2393 | igual | No cartão da aba PESSOAS; as ações também na folha da pessoa. | igual |
| B-P15 | estado | nenhum menu da pessoa (clique-direito bloqueado) | base.js:360–366 | adaptado | O toque no cartão abre a folha da pessoa: volume, perfil, moderar-fora. | igual |
| B-R01 | tela | rodapé permanente de 40px, uma fileira que rola na horizontal | index.html:1784–1852; tela-sessao.css:1860–1910, 2416 | adaptado | As métricas moram na fileira de telemetria da CHAMADA e, completas e também fora de sala, em AJUSTES › ESTE SERVIDOR › CONEXÃO (resolve o achado 8). | igual |
| B-R02 | estado | `ATRASO`, `JITTER`, `DISTÚRBIO`, `CODEC` (`OPUS <n>k` / `SEM ÁUDIO`), `UPTIME`, `SALA` | tela-sessao.js:2481–2500 · `snapshot` | adaptado | Na CHAMADA e em AJUSTES › ESTE SERVIDOR › CONEXÃO (também fora de sala). **PERDA** no lugar de DISTÚRBIO: é a palavra do glossário canônico (`docs/glossario.md`, «Perda de pacote · perda»). | igual |
| B-R08 | estado | `SINAL DA SALA` (`—` com title) | tela-sessao.js:2022–2043 | igual | Na faixa de voz e na CHAMADA. Sem sala não há faixa, então o `—` explicado não aparece. | igual |
| B-R09 | estado | `CAMINHO` (`REDE LOCAL` … `FURO DE NAT`, ou `·`) | tela-sessao.js:2514–2517; frases.js:381–429 | igual | Na CHAMADA; sem nome, `·`. | igual |
| B-R10 | estado | `ÁUDIO LOCAL FALHANDO` (title «a máquina, não a rede») | tela-sessao.js:2519 | adaptado | Na faixa de voz e na CHAMADA: «ÁUDIO LOCAL FALHANDO: este aparelho, não a rede». | igual |
| B-R11 | estado | `TROCANDO DE APARELHO` / `SEM APARELHO DE ÁUDIO` / `ÁUDIO AGORA EM <NOME>` / `O APARELHO DE ÁUDIO MUDOU` | tela-sessao.js:2550–2580 · `snapshot` | adaptado | Na faixa de voz e na CHAMADA, com o nome que o navegador der ao aparelho. | igual |
| B-G01 | estado | grade de colunas fixas; abaixo de 900px, gaveta de canais; nenhum ponto abaixo de 900px | tela-sessao.css:227–239, 2419–2480 | adaptado | As colunas viram lista e página: as quatro abas substituem colunas e gavetas. | igual |
| B-G02 | ação | `PESSOAS` (gaveta à direita) | tela-sessao.js:4024–4050 | adaptado | Aba PESSOAS. | igual |
| B-G05 | ação | `CANAIS` (gaveta à esquerda; fecha com Escape, com toque na conversa ou ao escolher) | tela-sessao.js:3969–4008, 4056–4058 | adaptado | Aba SERVIDOR. Escolher sala ou canal leva à aba de destino. | igual |
| B-M01 | camada | `#regioes-dos-mods` (faixa de até 240px) | index.html:1764–1765; base.js:771–808 | adaptado | Região `operador.barra` em AJUSTES (altura limitada, rola por dentro). Esse nome não existe no código do desktop. | igual |
| B-M02 | camada | página de MOD (`VOLTAR`) | index.html:1714; mods-superficies.js:281–284 | igual | Aba CONVERSA. | igual |
| B-M03 | camada | painel de MOD (200–640px) | mods-superficies.js:285–288; tela-sessao.css:2430–2436 | adaptado | Largura cheia sobre a aba CONVERSA, como o desktop abaixo de 900px, com `FECHAR`. | igual |
| B-M04 | camada | diálogo de MOD (`FECHAR`; Escape; Tab preso) | mods-superficies.js:266–280, 398–420 | adaptado | Largura cheia; `FECHAR` visível, e o voltar do Android fecha. | igual |

#### Os 22 atalhos, cada um com o seu controle visível


| id | atalho no desktop | âncora | estado | controle visível no celular | PC (navegador) |
|---|---|---|---|---|---|
| B-K01 | F5, Ctrl+R, ⌘R (bloqueados) | base.js:368–379 | adaptado | Nenhum: não há teclado. Recarregar é do navegador (B-S08). | adaptado: o navegador recarrega; com a sessão aberta, a página pergunta antes de sair |
| B-K02 | clique-direito (bloqueado) | base.js:360–366 | adaptado | Um toque no cartão ou na mensagem abre a folha dela. | igual |
| B-K03 | tecla de falar (Espaço, trocável) | tela-sessao.js:4407–4443 | adaptado | SEGURAR PARA FALAR, na faixa de voz. | adaptado: a tecla de falar só vale com a página em foco, e a tela diz isso |
| B-K04 | `/` abre a busca | tela-sessao.js:4378–4384 | adaptado | Lupa na barra da aba CONVERSA. | igual |
| B-K05 | Enter / Shift+Enter na busca | tela-sessao.js:4391–4397 | adaptado | Próxima e anterior, na barra da busca. | igual |
| B-K06 | Escape na busca | tela-sessao.js:4385–4390 | adaptado | `FECHAR` da busca. | igual |
| B-K07 | Escape fecha a gaveta de canais | tela-sessao.js:4402–4406 | adaptado | Não há gaveta: trocar de aba. | igual |
| B-K08 | Escape fecha o alerta | tela-sessao.js:4311–4316 | adaptado | `×` do alerta, ou o voltar do Android. | igual |
| B-K09 | Escape fecha a confirmação/moderação | camada-moderar.js:881–886 | adaptado | `CANCELAR` / `FECHAR` da folha, ou o voltar do Android. | igual |
| B-K10 | `?` abre a ajuda | camada-ajuda.js:73–78 | adaptado | `?` no cabeçalho SALAS DE VOZ. | igual |
| B-K11 | Escape fecha a ajuda | camada-ajuda.js:79–84 | adaptado | `FECHAR` da folha da ajuda. | igual |
| B-K12 | Escape fecha nomear/perfil | camada-nomear.js:118–127, 449–458 | adaptado | Perfil: `CANCELAR` / `FECHAR` em AJUSTES. Nomear fica fora (B-C03). | igual |
| B-K13 | Escape fecha a lista de servidores / a porta | camada-servidores.js:203–213; camada-portaria.js:800–809 | adaptado | Lista: o `‹` da tela de início. Porta: fora (B-F12). | igual |
| B-K14 | Escape fecha a portaria | camada-portaria.js:588–590 | fora na 0.16 | Coberto por B-F12. | igual |
| B-K15 | Escape volta da chamada | tela-chamada.js:835–846 | adaptado | RECOLHER, na CHAMADA. | igual |
| B-K16 | Escape sai da tela cheia do palco | palco-imagem.js:507–518 | adaptado | O botão de sair da tela cheia, no palco. | igual |
| B-K17 | Escape fecha o compartilhar | camada-compartilhar.js:692–697 | fora na 0.16 | Coberto por B-O12. | igual |
| B-K18 | Escape fecha a configuração | tela-server.js:1562–1567 | adaptado | `‹ AJUSTES` nas subpáginas, ou outra aba. | igual |
| B-K19 | escolher a tecla de falar | tela-server.js:1625–1648 | adaptado | Modo SEGURAR PARA FALAR em AJUSTES · ÁUDIO, sem tecla a escolher. | igual |
| B-K20 | Escape e Tab no diálogo de MOD | mods-superficies.js:398–420 | adaptado | `FECHAR` do diálogo, ou o voltar do Android. Não há Tab. | igual |
| B-K21 | Ctrl+V / ⌘V cola imagem | tela-sessao.js:4079 | adaptado | Colar do sistema no compositor, com toque longo. | igual |
| B-K22 | Enter envia a mensagem | tela-sessao.js:4108 | adaptado | Botão ENVIAR do compositor. | igual |

### O que hoje só existe em hover ou `title`, e como fica visível


| hoje | âncora | no celular |
|---|---|---|
| Volume por pessoa: `opacity: 0` até `:hover` / `:focus-within` | tela-sessao.css:1846–1856 | Folha da pessoa: VOLUME sempre visível, com o número escrito («100%»). |
| Nome do servidor cortado; inteiro só no `title` | tela-sessao.js:466–470 | Escrito inteiro no alto da aba SERVIDOR, em até duas linhas. |
| Nomes da trilha (sigla) só no `title` / `aria-label` | tela-sessao.js:620, 639–651 | Nome escrito sob cada sigla ou imagem na faixa REDES. |
| `—` com motivo no `title` («este servidor não anunciou nome», «você não entrou em nenhuma sala», «esta sala está vazia») | tela-sessao.js:188–192, 473, 2032 | O motivo escrito no lugar do `—`. Os dois do sinal da sala não ocorrem no celular (a métrica só existe em sala). |
| Estado do microfone e do fone só no `aria-label` e no `title` («· reativar», «não ouvir ninguém, nem o som de uma tela compartilhada») | tela-sessao.js:953–956, 960–975 | Estado escrito sob o glifo, na faixa de voz. Com SURDO ligado, a faixa diz «não está ouvindo nada, nem a tela». |
| `ÁUDIO LOCAL FALHANDO` explicado só no `title` | index.html:1821 | «ÁUDIO LOCAL FALHANDO: este aparelho, não a rede». |
| `VER A CHAMADA` explica «você já está dentro» só no `title` | tela-sessao.js:799–801 | «você está aqui» escrito no item da sala ocupada. |
| Diferença entre SAIR DA SALA e SAIR DO SERVIDOR só no `title` | tela-chamada.js:725–728 | Separadas por lugar: SAIR na faixa de voz (só a sala); SAIR DO SERVIDOR no fim de AJUSTES. |
| Moderar pelo nome: só o hover muda a cor | tela-sessao.css:2582–2598 | O toque no cartão abre a folha da pessoa (B-P11). |
| Motivos de botões apagados (`MODERAR UMA PESSOA`, `×` da última sala) só no `title` | camada-moderar.js:170–172, 244–246 | Fora: a frase de B-F05 / B-C03 no lugar. |

### Achados que importam para o desenho


1. A senha de sala é pedida por `window.prompt("Senha da sala")`, e a senha errada só volta depois, como aviso `ENTRADA NA SALA DE VOZ RECUSADA` (tela-sessao.js:3456–3458; frases.js:165).
2. Sala com senha e sala cheia não têm marca na lista, e a cheia não bloqueia o botão. `password_required`, `limit` e `people` já vêm no snapshot (tela-sessao.js:751, 798).
3. No desktop dá para silenciar e se isolar **antes** de entrar numa sala (o operador fica sempre na tela). No celular, MUDO e SURDO moram na faixa, que só existe em sala (index.html:1120–1121; tela-sessao.js:4322–4332).
4. O desktop abre a voz ao conectar e mostra «microfone aberto» e `MICROFONE ABERTO` fora de sala, o oposto de D-52 (seele-ffi lib.rs:4078–4091; tela-sessao.js:1882–1884, 2319–2323).
5. Falar segurando só existe por tecla: o botão de falar saiu, e o `keydown` nem confere se a sessão está na tela. SEGURAR PARA FALAR é controle novo, sem par no desktop (index.html:1127–1139; tela-sessao.js:4407–4411).
6. A troca de servidor pela trilha mostra o **endereço**, porque o botão não grava `data-nome` — embora o código diga preferir o nome (tela-sessao.js:596–597, 3656, 4361).
7. O grupo FORA DO AR do desenho não existe no desktop. O núcleo descarta `Membros`, `MembroNovo` e `MembroBanido` («nenhum servidor os manda ainda») (crates/seele-core/src/state.rs:1629–1635).
8. A telemetria é permanente no desktop. No celular ela mora na CHAMADA, e fora de sala não há onde ver ATRASO, CAMINHO ou CODEC (index.html:1767–1784).
9. O desktop bloqueia recarregar porque recarregar quebra a sessão (base.js:326–352). No navegador do celular, recarregar e puxar para recarregar existem e não se bloqueiam.
10. Muitas frases da moldura são literais no JS, fora do `frases.js`: veredito, subtítulo da conexão, contagens, pastilhas, aparelho, `#voz-estado` e batidas (tela-sessao.js:242–296, 456–462, 994–1002, 1972, 2319–2335, 2563–2576; camada-portaria.js:519–522).

## 3. A conversa

Esta seção sai de `C-conversa.md` (C-01 a C-87, lido no HEAD `f7bc902b3`) com o desenho e as regras de
`00-desenho-e-regras.md` aplicados. A coluna `id` diz de quais linhas do inventário cada item vem, e as
87 estão todas aqui.

- **Âncoras:** `tela-sessao.js:N`, `index.html:N`, `*.css:N` e `base.js:N` são de `apps/seele-app/ui/`;
  `main.rs:N` é de `apps/seele-app/src/`; `seele-ffi …`, `seele-core …` e `seele-web …` são de `crates/`.
- **Onde as coisas moram no celular:**
  - **aba CONVERSA:** o cabeçalho (nome do canal, lupa, contribuição de MOD), a busca, a lista e o compositor;
  - **folha de ações da mensagem:** abre com um toque na mensagem;
  - **folha de anexar:** abre pelo botão de anexar do compositor;
  - **folha do link:** abre com um toque num endereço;
  - **folha de confirmação:** a de remover, a de salvar.
- **Frases:** para os itens *fora*, a frase está entre «» e vai para `frases.js`. Para os *adaptados*,
  quando a adaptação troca uma frase, a troca vem dita na própria linha.

### A matriz


| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| C-01 | tela | a conversa (`#vista-conversa`), que divide a célula com a grade da chamada | index.html:1434 · tela-chamada.js:623, 641 · — | adaptado | A conversa é a **aba CONVERSA**. A chamada deixa de dividir a célula: abre em tela cheia por um toque na faixa de voz, e RECOLHER volta para a aba. | igual |
| C-02 | estado | nome do canal aberto; «—» antes do 1º quadro; «nenhum canal aberto» sem canal | tela-sessao.js:1016–1036 · — (snapshot) | igual | No cabeçalho da aba CONVERSA, cortado com reticências. «nenhum canal aberto» vai para `frases.js`. | igual |
| C-03 | ação | o nome do canal como botão de renomear, com a caixa «RENOMEAR O CANAL» | tela-sessao.js:1086–1124 · `renomear_linha` | fora na 0.16 | O nome do canal é texto para todo mundo. **Só para quem tem `may_manage_voice_rooms`**, um toque no nome abre uma folha com o item «RENOMEAR O CANAL» apagado e a frase «pelo SEELE no computador». Em AJUSTES · NO COMPUTADOR: «Renomear um canal». | igual |
| C-04 | camada | conteúdo e ações de MOD no cabeçalho do canal (`canal.cabecalho`) | tela-sessao.js:1038–1058 · base.js:1721–1773 · — (vai ao MOD) | igual | No cabeçalho da aba CONVERSA, ao lado do nome. Os MODs rodam no celular (D-web-1), e o clique vai ao MOD como no desktop. | igual |
| C-05 · C-10 · C-11 | ação | abrir a busca: a lupa (aria-label «Buscar neste canal», title «procurar no que já foi dito neste canal») ou a tecla `/` | index.html:1460–1464 · tela-sessao.js:3836–3847, 4072, 4374–4384 · — | adaptado | **A lupa no cabeçalho da aba CONVERSA é a única porta**, porque não há tecla `/`. O que o `title` dizia fica escrito no placeholder do campo: «buscar neste canal…». | igual |
| C-12 | ação | digitar no campo de busca (placeholder «buscar…») | tela-sessao.js:3928–3942, 4064 · `buscar`, `busca_limpar` | igual | Na barra de busca, abaixo do cabeçalho da aba CONVERSA. Procura só no que está em memória: as 50 últimas mensagens mais as que chegaram depois. | igual |
| C-13 · C-14 | estado | contador «[n/m]», realce e ocorrência atual; sem resultado, «[0/0]» | tela-sessao.js:3869–3888 · tela-sessao.css:2103–2116, 2224–2236 · — | igual | Na barra de busca. O contador não depende da cor e o «[0/0]» continua aparecendo. | igual |
| C-15 · C-17 | ação | próxima ocorrência: botão (aria-label e title «próxima ocorrência») e Enter no campo | tela-sessao.js:4066–4068, 4391–4397 · `busca_andar {adiante:true}` | adaptado | O botão › da barra de busca, e o Return do teclado virtual dentro do campo de busca (o campo é de uma só linha, então o Return anda). | igual |
| C-16 · C-17 | ação | ocorrência anterior: botão (aria-label e title «ocorrência anterior») e Shift+Enter no campo | tela-sessao.js:4069–4071, 4391–4397 · `busca_andar {adiante:false}` | adaptado | Sem Shift+Enter: **só o botão ‹** da barra de busca. | igual |
| C-18 · C-19 | ação | fechar a busca: «fechar» (title «fechar a busca») ou Esc no campo | tela-sessao.js:3818–3847, 4073, 4385–4390 · `busca_limpar` | adaptado | Só o botão «fechar» da barra de busca, que limpa o termo e o realce como no desktop. Sem Esc. | igual |
| C-20 · C-21 | estado | a busca refeita quando a conversa muda ou quando se troca de canal | tela-sessao.js:3462–3481, 3910–3913, 4470–4476 · `snapshot`, `messages`, `buscar`, `open_channel` | igual | O cursor fica na mesma ocorrência (`resume_at`). A regra está hoje em `seele-core::search` (ver achado 7). | igual |
| C-22 | estado | endereço cortado pelo realce perde o link; casamentos sobrepostos repetem um caractere («aaa» vira «aaaa») | tela-sessao.js:1505–1519, 1561–1568 · — | igual | O defeito vem junto com `corpoComRealce` (`docs/pendencias.md` #14). | igual |
| C-23 | estado | histórico carregando: **nada aparece na tela** | tela-sessao.js:1188–1203 · seele-ffi lib.rs:266, 4733 · `open_channel`, `messages` | igual | Aba CONVERSA. Vêm as 50 últimas, sem indicador de carga e sem «carregar mais antigas» (ver achado 8). | igual |
| C-24 | estado | canal vazio: **nenhuma frase** | tela-sessao.js:1444–1500 · `messages` | igual | Aba CONVERSA. A lista fica vazia, e quem olha não distingue «vazio» de «ainda carregando». | igual |
| C-25 | estado | sem canal aberto: o 1º canal abre sozinho | tela-sessao.js:1168–1180 · `open_channel` | igual | Sem canal aberto, o servidor não entrega nenhuma mensagem. | igual |
| C-26 | estado | conversa escondida: o desenho é adiado | tela-sessao.js:1444–1470, 397–398 · — | igual | Vale quando outra aba ou a CHAMADA cobre a CONVERSA (lista com `clientHeight` 0). Ela é desenhada no tique seguinte em que a aba voltar. | igual |
| C-27 | estado | rolagem: só acompanha o fim se a pessoa já estava lá; nada avisa que chegou mensagem embaixo | tela-sessao.js:1472–1499 · — | igual | Aba CONVERSA. Sem botão de «ir ao fim» e sem contador, como no desktop. | igual |
| C-28 | estado | divisor de dia: «HOJE», «ONTEM», dd/mm/aaaa | tela-sessao.js:1219–1252 · — | igual | Na lista. «HOJE» e «ONTEM» vão para `frases.js`. | igual |
| C-29 | estado | cabeçalho da mensagem: avatar, autor e hora HH:MM; «--:--» sem hora | tela-sessao.js:1265–1311 · base.js:49–53 · — | igual | Na lista. A hora fica sempre à vista. | igual |
| C-30 | estado | continuação de rajada (mesmo autor, até 900 s): a hora vai para a calha | tela-sessao.js:1210, 1286–1293 · — | igual | Na lista. | igual |
| C-31 | estado | mensagem própria (só cor e borda) | tela-sessao.js:1273 · tela-sessao.css:1027, 1046–1049, 1062 · — | igual | Na lista, e continua só por cor e borda. | igual |
| C-32 | estado | retrato no avatar (MOD, cache ou iniciais) | tela-sessao.js:4519–4600 · `imagem_da_pessoa` | igual | Na lista. Quem escreveu e não está em sala nem em `presentes` fica com as iniciais, como no desktop. | igual |
| C-33 | estado | «editada» | tela-sessao.js:1313 · — | igual | No cabeçalho da mensagem. Não há editar, responder nem citar em nenhuma das duas cascas. «editada» vai para `frases.js`. | igual |
| C-34 | estado | corpo da mensagem, cru e com as quebras preservadas | tela-sessao.js:1325–1335 · — | igual | Na lista. | igual |
| C-35 (própria) · C-36 | ação | «remover» a **própria** mensagem, com a confirmação «REMOVER MENSAGEM», CANCELAR e REMOVER | camada-moderar.js:135–146, 700–713, 432–462 · `remover_mensagem {message}` | igual | Na **folha de ações da mensagem** (toque na própria mensagem), item REMOVER. A confirmação é uma folha com a mesma frase, CANCELAR e REMOVER, e um erro fica dentro dela. Fecha pelo FECHAR ou pelo voltar do Android, já que não há Esc. Deixa de depender do hover (camada-moderar.css:329–351). | igual |
| C-35 (outra pessoa) | ação | «remover» a mensagem **de outra pessoa** (`may_remove_message`) | camada-moderar.js:135–146 · tela-sessao.js:385 · `remover_mensagem` | fora na 0.16 | **Só para quem tem `may_remove_message`:** na folha de ações da mensagem de outra pessoa, o item «REMOVER» apagado com «pelo SEELE no computador». Quem não tem a permissão não vê o item. Em AJUSTES · NO COMPUTADOR: «Remover a mensagem de outra pessoa». | igual |
| C-37 | estado | «enviando…» | tela-sessao.js:1387–1399 · — | igual | No selo da mensagem. A frase vai para `frases.js`. | igual |
| C-38 | estado | «não foi gravada: …» com o motivo («não pode escrever neste canal», «este canal não existe mais», «o servidor não conseguiu gravar», «passa de N bytes») | tela-sessao.js:1352–1376, 1412–1416 · — | igual | No selo, em vermelho. As frases de `RECUSAS_DE_MENSAGEM` vão para `frases.js`. | igual |
| C-39 | estado | «o enlace caiu antes da resposta — pode ter sido gravada» | tela-sessao.js:1404–1411 · — | igual | No selo, em laranja. | igual |
| C-40 | ação | «TENTAR DE NOVO» | tela-sessao.js:1419–1422, 2816–2843 · `reenviar_mensagem` | igual | No selo da mensagem, com um alvo de toque maior que os ~18 px do desktop. | igual |
| C-41 | ação | «DESISTIR» (title «devolve o texto ao campo de escrever») | tela-sessao.js:1423–1430, 2816–2843 · `descartar_mensagem` | adaptado | No selo, e o `title` vira texto visível ao lado do botão: «o texto volta ao campo». O foco no campo abre o teclado virtual. | igual |
| C-42 | estado | sem permissão de escrever: só se descobre depois de mandar | tela-sessao.js:1352–1355 · frases.js:1062–1064 · `send_message`, `enviar_anexo` | igual | O compositor fica sempre ativo, e a recusa vem no selo (C-38) ou na caixa do anexo (C-66). | igual |
| C-43 · C-44 | ação | endereço no corpo (title «<url> — abre no navegador»): clique, Enter ou Espaço abre | tela-sessao.js:1547–1625, 1756–1777 · `abrir_no_navegador {url}` | adaptado | O toque no endereço abre a **folha do link**, com o endereço inteiro escrito e o botão ABRIR, e o navegador abre numa aba nova. **O destino aparece antes de abrir, nunca só no `title`.** | adaptado: abre numa aba nova |
| C-45 | estado | link recusado: sublinhado ondulado, motivo só no `title` | tela-sessao.js:1760–1766 · main.rs:2096–2116 · `abrir_no_navegador` | adaptado | O motivo aparece escrito na folha do link, no lugar do ABRIR. A mesma regra de antes vale: só http(s), até 2048 caracteres, sem espaço nem caractere de controle. | adaptado: abre numa aba nova |
| C-46 | estado | link de imagem (.gif/.png/.jpg/.jpeg/.webp, ou tenor, giphy, imgur) pede a prévia uma vez | tela-sessao.js:1626–1708 · main.rs:5239–5315 · `previa_de_link`, `dominios_de_gif` | adaptado | Quem busca é a ponte, e não o Rust da máquina. O que a página consegue repetir das travas do desktop está no achado 5. | igual |
| C-47 · C-48 | ação | convite «imagem de <domínio>» com «MOSTRAR IMAGEM» (title «buscar em <domínio> entrega a esse servidor o endereço desta máquina e a hora») | tela-sessao.js:1716–1748 · `consentir_previa {dominio}`, `previa_de_link` | adaptado | O aviso sai do `title` e fica escrito no próprio convite: «buscar em <domínio> entrega a esse servidor o endereço deste aparelho e a hora». O consentimento vale para o domínio, fica guardado neste aparelho e se desfaz em AJUSTES · PRÉVIAS DE LINK. | igual |
| C-49 | estado | prévia de link desenhada embaixo do link | tela-sessao.js:1603–1612 · — | igual | Na mensagem, com a largura limitada pela tela. | igual |
| C-50 | estado | prévia de link que falhou: **nada na tela**, só `console.warn` | tela-sessao.js:1693–1707 · `previa_de_link` | igual | Fica calada como no desktop (ver achado 4). | igual |
| C-51 | estado | bloco do anexo: nome, tamanho, SALVAR, PRÉVIA e a linha de estado | tela-sessao.js:3228–3276 · — | igual | Na mensagem. Continua sem «abrir» (ADR 0027). | igual |
| C-52 | estado | «ESTE ARQUIVO EXPIROU», com moldura tracejada | tela-sessao.js:3236–3241 · — | igual | Na mensagem. | igual |
| C-53 | ação | «SALVAR» (title «salvar em <pasta>») e a caixa «SALVAR EM DISCO» com `fraseDeSalvar` | tela-sessao.js:3410–3431 · frases.js:1142–1155 · `destino_do_anexo`, `salvar_anexo`, `pasta_de_downloads` | adaptado | SALVAR vira **download do navegador**. A folha de confirmação perde a pasta, o «ao lado» e a quarentena, que não existem na página. Mantém «o SEELE não confere se este arquivo é seguro» e «nenhuma tela do SEELE abre este arquivo». | adaptado: o download do navegador |
| C-54 | camada | recusa de salvar: «SALVAR EM DISCO» com `NAO_SALVOS` (`NomeRecusado`, `AnexoDesconhecido`, `SemPasta`, `Falhou`) | tela-sessao.js:3412–3417 · frases.js:1115–1185 · `destino_do_anexo` | adaptado | Folha de confirmação só com FECHAR. `SemPasta` não acontece, porque não há pasta. As outras frases saem sem «pasta» e sem «Windows, Mac, Linux». | adaptado: o download do navegador |
| C-55 | estado | «SALVANDO n%» | tela-sessao.js:3176–3186 · — (evento `Receiving`) | adaptado | Na linha de estado do bloco, contando os bytes que vêm do servidor até a página entregá-los ao download do navegador. | igual |
| C-56 | estado | «ARQUIVO SALVO — <caminho>» ou `fraseDeNaoSalvo` | tela-sessao.js:3188–3205 · frases.js:1098–1105 · — (evento `Saved`/`NotSaved`) | adaptado | Sem caminho, porque o navegador não diz à página onde guardou: a frase diz só que o arquivo foi entregue ao download do navegador. | igual |
| C-57 | estado | arquivo que não vem (`Unavailable`, normalmente `ANEXOS.Expired`) | tela-sessao.js:3162–3174 · — (evento) | igual | Na linha de estado do bloco. | igual |
| C-58 | ação | «PRÉVIA» (title «baixa o arquivo e desenha, se os bytes forem de imagem»), que vira «BAIXANDO» | tela-sessao.js:3249–3262, 3342–3372 · `prever_anexo`, `regras_de_previa` | adaptado | No bloco do anexo. O `title` vira texto visível junto do botão: «baixa <tamanho>». Ver é baixar, inclusive pelos dados móveis. | igual |
| C-59 · C-61 | ação | prévia aberta com «FECHAR» (title «recolhe esta prévia; reabrir não baixa de novo»); o PRÉVIA que volta (title «mostra de novo o que já foi baixado») | tela-sessao.js:3300–3328, 4179–4184 · — | adaptado | FECHAR sobre a figura. O botão que volta diz por extenso «PRÉVIA · já baixada», que é o que os dois `title` diziam. | igual |
| C-60 | estado | prévia recusada: `fraseDePrevia` (`Disagrees`, `TooBig`, `NotAPicture`, `DidNotArrive`), com FECHAR | tela-sessao.js:3315–3319, 3357–3371 · frases.js:1197–1240 · `prever_anexo` | igual | No bloco do anexo. O «Salve-o» do `TooBig` passa a querer dizer o download do navegador. | igual |
| C-62 | estado | arquivo pendente: nome e «<tamanho> · pronto para enviar com Enter», com «tirar» | tela-sessao.js:3034–3045 · — | adaptado | Acima do compositor. A frase troca «com Enter» por «com ENVIAR», porque o Return quebra a linha. | igual |
| C-63 | ação | «tirar» (title «tirar o arquivo»); durante a subida não cancela | tela-sessao.js:3069–3076 · — | igual | Acima do compositor. Continua sem cancelar a subida (ver achado 6). | igual |
| C-64 | estado | «SUBINDO» e «SUBINDO n%», com barra | tela-sessao.js:3084–3126 · `enviar_anexo` | igual | Acima do compositor. | igual |
| C-65 | estado | entregue: a caixa some, e «ARQUIVO ENTREGUE» sai só para o leitor de tela | tela-sessao.js:3128–3135 · — (evento `Sent`) | igual | Para quem enxerga, a confirmação é a mensagem aparecer na lista (ver achado 4). | igual |
| C-66 | estado | «O SERVIDOR RECUSOU O ARQUIVO» e depois a razão por `fraseDeAnexo` (`NotAllowed`, `TooLarge` com limite, `NoRoom`, `SizeMismatch`, `HashDidNotMatch`, `RateLimited`, `Unavailable`, `NotFound`, `Expired`, `Malformed`) | tela-sessao.js:3138–3160 · frases.js:1061–1088 · — (evento) | igual | Acima do compositor. | igual |
| C-67 | estado | `TRANSFERENCIAS.Fell`: a transferência caiu e recomeça do zero | tela-sessao.js:3101–3104, 3138–3150 · `enviar_anexo` | igual | Acima do compositor. Não há retomada, como no desktop. | igual |
| C-68 | estado | depois de recusa ou queda, o arquivo continua pendente e o texto que foi junto não volta | tela-sessao.js:2768–2778, 3084–3105 · — | igual | Acima do compositor. O defeito vem junto (ver achado 6). | igual |
| C-69 | estado | recusas locais: «ABRA UM CANAL ANTES DE ESCOLHER UM ARQUIVO / DE COLAR UMA IMAGEM / DE ANEXAR UM ARQUIVO», «NÃO CONSEGUI LER ESSE ARQUIVO», «NÃO CONSEGUI LER A IMAGEM COLADA», «ESSA IMAGEM É GRANDE DEMAIS PARA COLAR · MANDE COMO ARQUIVO», «NÃO CONSEGUI GUARDAR A IMAGEM NESTA MÁQUINA», «O QUE FOI COLADO NÃO É UMA IMAGEM» | tela-sessao.js:2901–2944, 2983–3031, 4157–4159 · — | adaptado | Na mesma caixa acima do compositor. O caso de arrastar sai, e «NESTA MÁQUINA» sai porque não há arquivo temporário. As frases em 1ª pessoa passam a seguir a regra de voz e vão para `frases.js` (ver achado 10). | igual |
| C-70 | estado | anexo grande demais: o servidor recusa com `TooLarge` depois que a subida começou; a colagem tem teto local de 25 MiB | main.rs:3416–3426 · `enviar_anexo`, `guardar_colado` | igual | Acima do compositor. Nada confere o tamanho no cliente antes de subir, e o teto de 25 MiB é da travessia em base64, que a página não precisa fazer. | igual |
| C-71 | estado | um segundo Enter ou ENVIAR durante a subida manda o arquivo de novo | tela-sessao.js:2768–2778, 3084–3100 · `enviar_anexo` | igual | O defeito vem junto (ver achado 6). | igual |
| C-72 | estado | campo `textarea` (placeholder «transmitir no canal — Enter envia»), crescendo até 160 px | index.html:1570–1572 · tela-sessao.js:4126–4142 · — | adaptado | No compositor da aba CONVERSA. O placeholder deixa de dizer «Enter envia», e o teto de altura se mede contra o teclado virtual aberto. | igual |
| C-73 · C-74 | atalho | Enter envia; Shift+Enter quebra a linha | tela-sessao.js:4108–4112 · index.html:2914–2937 · `send_message`, `enviar_anexo` | adaptado | **O Return do teclado virtual quebra a linha, e quem envia é o botão ENVIAR.** O ouvinte de Enter do desktop não pode valer no celular. | igual |
| C-75 | ação | «ENVIAR» (aria-label «Enviar a mensagem») | tela-sessao.js:2761–2808, 4074 · `send_message {channel, body}` | igual | No compositor. É **o** jeito de mandar. Sem canal aberto, não faz nada e não diz nada, como no desktop. | igual |
| C-76 | estado | «N de 4096 bytes — corte o texto», com moldura vermelha | tela-sessao.js:2713–2759 · `limite_da_mensagem` | igual | Embaixo do campo, contando bytes UTF-8 e não caracteres. A frase vai para `frases.js`. | igual |
| C-77 | estado | rascunho por canal, só em memória | tela-sessao.js:2696–2712, 3462–3466 · — | igual | Também só em memória, e se perde quando a página recarrega (ver achado 9). | igual |
| C-78 | ação | anexar (aria-label «Escolher um arquivo», title «escolher um arquivo; colar e arrastar também anexam»), pelo seletor nativo | tela-sessao.js:2925–2944, 4168 · main.rs:3540–3563 · `escolher_arquivo` | adaptado | O botão de anexar abre a **folha de anexar**, e ela abre o **seletor do navegador**. O arquivo viaja em bytes pela ponte. O que o `title` dizia vira texto na folha: «colar uma imagem no campo também anexa». | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| C-79 | ação | colar uma imagem (Ctrl/Cmd+V) | tela-sessao.js:2946–3024, 4076–4082 · `guardar_colado {tipo, base64}` | adaptado | Pega **o que o navegador entrega no `paste`**: um item `image/*` em `clipboardData` vira o anexo pendente, sem arquivo temporário. Texto colado continua sendo texto. | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| C-80 | gesto | arrastar um arquivo para a janela (`tauri://drag-drop`), com «UM ARQUIVO POR MENSAGEM; PEGUEI O PRIMEIRO» | tela-sessao.js:4143–4166 · `descrever_arquivo {caminho}` | adaptado | Não existe arrastar: **o botão de anexar cobre**. A frase de mais de um arquivo sai junto. | adaptado: arrastar para a página: o `drop` do navegador entrega o arquivo à ponte |
| C-81 | camada | ferramentas de MOD no compositor (`compositor.ferramentas`) | tela-sessao.js:1060–1074 · — (vai ao MOD) | igual | No compositor, antes do ENVIAR (D-web-1). | igual |
| C-82 | estado | enlace caído: o compositor esmaece e continua escrevendo | tela-sessao.css:2085–2086 · — | igual | No compositor. A mensagem fica em «enviando…» ou na frase de «o enlace caiu…». | igual |
| C-83 | estado | com um arquivo pendente, o texto vai junto pelo `enviar_anexo` e não aparece como pendente | tela-sessao.js:2761–2779 · `enviar_anexo` | igual | O mesmo. Muda só o que atravessa a ponte: os bytes, e não o caminho. | igual |
| C-84 | gesto | clique-direito: o menu do sistema é suprimido fora de campo e de seleção, e não há menu na mensagem | base.js:355–366 · — | adaptado | Não há clique-direito. **Um toque na mensagem abre a folha de ações dela.** Copiar o texto se faz pela seleção nativa do toque longo (ver achado 9). | igual |
| C-85 | atalho | Espaço fora de campo: push-to-talk | tela-sessao.js:4407–4411 · `set_talking` | adaptado | O botão **SEGURAR PARA FALAR** na faixa de voz (área da chamada). O teclado da conversa nunca abre o microfone. | igual |
| C-86 | atalho | Esc fecha a faixa de aviso e a caixa de confirmação | tela-sessao.js:4311–4316 · camada-moderar.js:881–886 · `dispensar_aviso` | adaptado | O botão de fechar de cada uma, e o voltar do Android fecha a folha. | igual |
| C-87 | estado | avisos do servidor sobre a conversa: «PERMISSÃO NEGADA», «O CANAL DE TEXTO QUE VOCÊ LIA FOI APAGADO, COM TUDO QUE HAVIA NELE», retenção «— N MENSAGENS», «VOCÊ FOI CHAMADO» | tela-sessao.js:2592–2645 · frases.js:161–218 · `snapshot` | igual | Na faixa de aviso (área B). Não há realce de menção nem contagem de não lidas, como no desktop. | igual |
| C-06 · C-07 · C-08 · C-09 | ação | as gavetas: «CANAIS» (≤ 900 px), «PESSOAS» (≤ 1240 px), fechar ao tocar a conversa, Esc | tela-sessao.js:3969–4058, 4399–4405 · — | adaptado | Não há gaveta: **as abas SERVIDOR e PESSOAS no pé** fazem o papel das duas portas. | igual |

O que a matriz diz, em números:

- **74 itens**, cobrindo as 87 linhas do inventário;
- **45 iguais**, **27 adaptados** e **2 fora na 0.16**;
- os 2 fora são renomear o canal (C-03) e remover a mensagem de outra pessoa (C-35, outra pessoa). Os dois são de quem administra ou modera, e o resto da conversa é consumo.

### Achados para o desenho


1. **O anexo sai do disco.** `enviar_anexo` recebe um caminho, e o núcleo lê o arquivo (main.rs:3605; seele-ffi lib.rs:1475–1496). No celular os bytes atravessam a ponte, e o tipo tem de sair da mesma tabela de extensões (main.rs:3573), não do `File.type`.
2. **O «remover» só aparece no hover** (`visibility:hidden`, camada-moderar.css:329–351), embora o comentário de tela-sessao.js:1316 diga o contrário. Na própria mensagem ainda pendente (`id` 0) o botão aparece e falha com `UnknownMessage` (camada-moderar.js:135–146; seele-ffi lib.rs:2058).
3. **Enter envia e Shift+Enter quebra** (tela-sessao.js:4108–4112). No celular o ouvinte não pode valer, e três textos que prometem Enter mudam: o placeholder (index.html:1571), o «pronto para enviar com Enter» (tela-sessao.js:3040) e a tabela de atalhos (index.html:2914–2937).
4. **O que quem enxerga não vê:**
   - só no `title`: o destino e a recusa de um link (tela-sessao.js:1586, 1764), o aviso de privacidade da prévia (1724–1725), o DESISTIR (1429), o PRÉVIA (3258–3261) e o «colar também anexa» (index.html:1592);
   - só no `#anuncio`: «ARQUIVO ENTREGUE» (3133) e «PEGUEI O PRIMEIRO» (4164);
   - só no console: a prévia de link que falhou (1693–1707).
5. **A prévia de link confia em travas que estão no Rust:** consentimento, destino privado recusado, 3 saltos, 4 MiB e bytes contra o tipo alegado (main.rs:5239–5315, 5547). Uma página não lê os bytes de outra origem sem CORS, então essas travas não se repetem só com o `fetch` da página.
6. **O anexo tem três falhas silenciosas:**
   - o texto que acompanhava o arquivo some numa recusa ou queda (tela-sessao.js:2768–2778);
   - um segundo toque em ENVIAR durante a subida manda o arquivo de novo (3084–3100);
   - «tirar» não cancela a subida, porque não há esse comando (3069–3076).
   Na rede do celular, quedas são mais comuns.
7. **O `seele-web` depende só do `seele-proto`** (seele-web/Cargo.toml:13–17). A busca (`seele-core::search`), o `judge` da prévia (`seele-core::preview`), o `nome_seguro` (`seele-core::anexo_no_disco`) e a recusa local `TooLong` (seele-ffi lib.rs:1645) estão fora dele.
8. **O histórico é de 50 mensagens**, sem indicador de carga, sem «mais antigas» e sem frase de canal vazio (seele-ffi lib.rs:266; tela-sessao.js:1188–1203, 1444–1500). A busca só alcança o que está em memória (main.rs:8883).
9. **O `base.js` supõe uma janela:**
   - suprime o menu do toque longo fora de campo e de seleção (base.js:360);
   - bloqueia recarregar (368–379).
   Na página, recarregar acontece, e leva o rascunho, que só existe em memória (tela-sessao.js:2696).
10. **As frases da conversa estão quase todas fora do `frases.js`**, escritas em `tela-sessao.js`, e algumas em 1ª pessoa, contra a regra de voz: «NÃO CONSEGUI LER ESSE ARQUIVO» (2907), «NÃO CONSEGUI LER A IMAGEM COLADA» (3002), «NÃO CONSEGUI GUARDAR A IMAGEM NESTA MÁQUINA» (3029) e «PEGUEI O PRIMEIRO» (4164).

## 4. A chamada e a tela

- **Base:** `D-chamada.md`, lido do desktop em `f7bc902b`, e o desenho de
  `00-desenho-e-regras.md`, que não é rediscutido aqui.
- **Os ids** são os do inventário (V-01 a V-101). Linhas que são o mesmo item
  foram juntadas, e todos os ids aparecem.
- **Caminhos curtos:** `ui/` é `apps/seele-app/ui/`; `main.rs` e `lib.rs` como no
  inventário.
- **Contagem:** 69 linhas (30 iguais, 30 adaptadas e 9 fora na 0.16), que
  cobrem os 101 ids.

### A matriz


| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| V-01 | tela | a vista da chamada (`#vista-chamada`) | index.html:1233–1431 · — | adaptado | A **CHAMADA** em tela cheia: grade de cartões, palco e telemetria. Abre com um toque na faixa de voz e fecha com **RECOLHER**, que volta às abas. | igual |
| V-02 | ação | `ENTRAR NA SALA` (com a senha da sala) | tela-sessao.js:789–801, 3433–3458, 3494 · `enter_voice_room` | adaptado | Na aba SERVIDOR. Ao entrar, a faixa de voz aparece e a CHAMADA abre, como `abrirChamada` faz no desktop. A senha vai numa folha de baixo, no lugar do `window.prompt`. O áudio abre aqui (D-52). | igual |
| V-03 | ação | `VER A CHAMADA` (title «voltar para a grade de X; você já está dentro») | tela-sessao.js:789–801, 3440–3444 · — | adaptado | Um toque na faixa de voz. Na aba SERVIDOR, a sala em que se está continua dizendo `VER A CHAMADA`, e o rótulo basta, sem o title. | igual |
| V-04 | ação | tocar num canal `# nome` | tela-sessao.js:3462–3473 · `open_channel` | adaptado | RECOLHER e a aba CONVERSA. Abrir um canal não fecha nada: a CHAMADA já está recolhida. | igual |
| V-05 | ação | `SAIR DA SALA` (title «sair de X: você para de ouvir e de falar nesta sala») | tela-chamada.js:719–728, 765–779 · `leave_voice_room` | adaptado | **SAIR** na faixa de voz e no mesmo lugar na CHAMADA, ao lado de MUDO e SURDO. Sair recolhe a CHAMADA e a faixa some. O que o title dizia fica escrito: o nome da sala aparece na faixa, ao lado de SAIR. | igual |
| V-06 | atalho | tecla `Escape` fecha a vista da chamada | tela-chamada.js:835–847 · — | adaptado | **RECOLHER**, um botão visível no alto da CHAMADA. | igual |
| V-07 | ação | criar canal (fecha a chamada) | tela-sessao.js:4271–4285 · `criar_linha`, `open_channel` | fora na 0.16 | «Criar canal é pelo SEELE no computador.» no lugar do `+` de CANAIS, só para quem tem a permissão, e em NO COMPUTADOR. A frase é da área de canais e está repetida aqui só para o cruzamento. | igual |
| V-08 | estado | uma página de MOD escondida ao abrir a chamada (`mostrarDestinoNativo`) | tela-chamada.js:624, 642; mods-superficies.js:198–201 · — | adaptado | A página de MOD continua na aba CONVERSA. A CHAMADA fica por cima sem desmontá-la. | igual |
| V-09 | estado | redesenho a cada 500 ms com a chamada aberta | tela-chamada.js:672–680, 872–874 · `snapshot` | igual | Só com a CHAMADA aberta. O custo de cada leitura pela ponte está a medir (achado 2). | igual |
| V-10 | estado | as portas `CANAIS` e `PESSOAS` (na barra da conversa) | tela-sessao.js:3969–3973, 4024–4028 · — | adaptado | As abas SERVIDOR e PESSOAS fazem o papel das gavetas. Da CHAMADA chega-se a elas por RECOLHER. | igual |
| V-11 | estado | a sessão acabou com a chamada aberta | tela-fim.js:57, 66; tela-chamada.js:661–664 · — | igual | A CHAMADA fecha e a tela de fim aparece. Ver o achado 6: a tela cheia precisa sair junto. | igual |
| V-12 | estado | `SALA DE VOZ` + nome da sala; `—` com title «fora de uma sala de voz» ou «sem sessão» | index.html:1235–1236; tela-chamada.js:282–288 · `snapshot` | igual | No alto da CHAMADA; o nome também vai na faixa. O `—` com title não acontece: sem sala não há faixa nem CHAMADA. | igual |
| V-13 | estado | `CONEXÃO NO AR` / `CONEXÃO CAIU` / `SEM SESSÃO` | tela-chamada.js:290–302 · `snapshot` | igual | No alto da CHAMADA, com palavra e cor. | igual |
| V-14 | estado | `EM SESSÃO` `HH:MM:SS` | tela-chamada.js:304–316 · `snapshot` | igual | No alto da CHAMADA. | igual |
| V-15 | estado | a barra numa linha sem quebra | tela-sessao.css:2497–2506 · — | adaptado | O alto da CHAMADA em duas linhas: `SALA DE VOZ` + nome em cima; enlace, relógio e RECOLHER embaixo. | igual |
| V-16 | estado | `NINGUÉM ESTÁ COMPARTILHANDO A TELA` + `Nada está sendo transmitido nesta sala.` | tela-chamada.js:142–174 · `snapshot` | igual | No palco da CHAMADA, como faixa de uma linha. Ver o achado 1. | igual |
| V-16b, V-78, V-79 | ação | `COMPARTILHAR` (rodapé do operador) e `COMPARTILHAR ESTA TELA` (palco vazio) | index.html:1181–1182, 1344–1345; tela-chamada.js:729–736, 746–750 · ver V-81 | fora na 0.16 | «Compartilhar a própria tela é pelo SEELE no computador. A tela de quem compartilhar aparece aqui.» no palco da CHAMADA, no lugar dos dois botões. Aparece **para todos** que estão numa sala, com ou sem transmissão em curso. Em NO COMPUTADOR: «COMPARTILHAR A TELA — mostrar um monitor ou uma janela a quem está na sala, com resolução, quadros e som.» | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida) |
| V-16c | estado | `Esta versão não sabe compartilhar tela desta máquina. A de outra pessoa apareceria aqui.` (com `permissao_de_tela` nula ou `NaoSeSabe`) | tela-chamada.js:153–165 · `permissao_de_tela` | fora na 0.16 | Não aparece: no celular, o lugar dela é da frase de V-78. A ponte recusa `permissao_de_tela` com essa frase (achado 8). | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida); sem módulo de vídeo a baixar |
| V-17, V-55 | estado | `SEM SESSÃO` (no palco e na grade) | tela-chamada.js:145–147, 331–339 · — | adaptado | Sem sessão não há faixa e a CHAMADA não abre; a tela de fim ou a entrada cobrem esse estado. | igual |
| V-18, V-57 | estado | fora de sala; `VOCÊ NÃO ESTÁ EM NENHUMA SALA` | tela-chamada.js:152, 168, 336 · — | adaptado | Não acontece: a CHAMADA só abre pela faixa, e a faixa só existe dentro de uma sala. | igual |
| V-19, V-20 | estado | `<APELIDO> ESTÁ COMPARTILHANDO A TELA` / `ALGUÉM DESTA SALA ESTÁ COMPARTILHANDO A TELA` | tela-chamada.js:176–186 · `snapshot` | igual | Na cabeça do palco da CHAMADA. | igual |
| V-19b | estado | nota `Duplo clique na imagem para vê-la em tela cheia.` | tela-chamada.js:205 · — | adaptado | «Deite o aparelho ou toque em TELA CHEIA para ver a imagem inteira.» | igual |
| V-21 | estado | `VOCÊ ESTÁ COMPARTILHANDO A SUA TELA` + `Esta é a imagem que está saindo daqui…` | tela-chamada.js:182–183, 203–204 · — | fora na 0.16 | Não aparece, porque o celular não envia; a frase de V-78 já cobre. | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida) |
| V-22 | estado | `ninguém assistindo` / `1 pessoa assistindo` / `N pessoas assistindo` | tela-chamada.js:195, 275–279 · `snapshot` | igual | Na cabeça do palco. | igual |
| V-23 | estado | a imagem (canvas) | palco-imagem.js:152–175, 518–548 · eventos `ScreenOpened`/`ScreenFrame`/`ScreenClosed` | igual | No palco, na largura da tela. **Depende de medir:** WebCodecs `VideoDecoder` com H.264 **High** no iPhone (achado 4), e o teto do WebKit de 16 MB por conexão, que um fluxo assistido ultrapassa (achado 5). | igual |
| V-24 | estado | parada: `PARADAS.SinalCritico` / `PARADAS.AbaixoDoPiso` | tela-chamada.js:211–213; lib.rs:3695 · — | igual | Morta nos dois lados: `parada` é sempre `None` (achado 9). | igual |
| V-25 | estado | `#palco-aperto` | index.html:1421 · — | igual | Escondido nos dois, porque não há medida. | igual |
| V-26 | estado | a varredura sai de cima da imagem | palco-imagem.js:168, 399 · — | igual | O mesmo `data-vendo-tela`. | igual |
| V-27 | estado | `ESTA JANELA NÃO SABE DECODIFICAR VÍDEO · ATUALIZE O APLICATIVO` | palco-imagem.js:206–210 · — | adaptado | «ESTE NAVEGADOR NÃO DECODIFICA VÍDEO · ATUALIZE O NAVEGADOR OU O SISTEMA DO APARELHO», em `#palco-falha`. | igual |
| V-28, V-30, V-32, V-33, V-35 | estado | `O QUADRO NÃO DISSE EM QUE FORMATO VEIO`; `O DECODIFICADOR DE VÍDEO PAROU · ESPERANDO O PRÓXIMO QUADRO-CHAVE`; `UM QUADRO CHEGOU ILEGÍVEL`; `ESTE VÍDEO NÃO ESTÁ SENDO ACEITO PELO DECODIFICADOR`; a falha some no primeiro quadro | palco-imagem.js:146–149, 253–257, 281–288, 319–327, 378–381 · — | igual | Em `#palco-falha`, no palco. Ver o achado 10 sobre V-30. | igual |
| V-29 | estado | `ESTA MÁQUINA NÃO DECODIFICA avc1.PPCCLL` | palco-imagem.js:268–277 · — | adaptado | «ESTE APARELHO NÃO DECODIFICA avc1.PPCCLL», com o código do perfil junto. | igual |
| V-31 | estado | `NÃO CONSEGUI PREPARAR O VÍDEO NESTA MÁQUINA` | palco-imagem.js:307–309 · — | adaptado | «O VÍDEO NÃO PÔDE SER PREPARADO NESTE APARELHO» (sem primeira pessoa). | igual |
| V-34 | estado | `ALGUÉM ESTÁ COMPARTILHANDO NUMA VERSÃO QUE ESTA NÃO LÊ · ATUALIZE O SEELE` | palco-imagem.js:531–546 · evento `ScreenUnreadable` | adaptado | «ALGUÉM ESTÁ COMPARTILHANDO NUMA VERSÃO QUE ESTA PÁGINA NÃO LÊ · A VERSÃO DA PÁGINA ESTÁ EM AJUSTES › ESTE APARELHO» | igual |
| V-36 | estado | a fileira de escolha escondida (nenhuma transmissão alheia) | palco-imagem.js:601–609 · — | igual | No palco. | igual |
| V-37, V-38, V-39, V-40 | estado | `<APELIDO>` marcado / `<APELIDO> · ABRINDO` / `VER <APELIDO>` / `—` | palco-imagem.js:611–651, 756–759 · — | igual | A fileira no palco, com os mesmos rótulos e o mesmo `aria-pressed`. | igual |
| V-37t, V-38t | estado | os titles «X está no palco» e «pedido; a imagem começa no próximo quadro-chave de X» | palco-imagem.js:647–651 · — | adaptado | Texto visível sob a fileira enquanto está ABRINDO: «A imagem começa no próximo quadro-chave de <APELIDO>.» A marca laranja já diz qual está no palco. | igual |
| V-41 | ação | tocar numa transmissão | palco-imagem.js:652–656, 769–796 · `assistir` ×2 | igual | Um toque, e assiste-se a uma por vez. | igual |
| V-42 | ação | `NÃO VER` | palco-imagem.js:669–679, 731–748 · `assistir` | igual | O último botão da fileira. | igual |
| V-42t | estado | o title de `NÃO VER` («parar de receber a transmissão e ficar só no áudio» / «você não está recebendo transmissão nenhuma») | palco-imagem.js:672–675 · — | adaptado | Com `NÃO VER` marcado, uma nota sob a fileira: «Sem imagem: só o áudio da sala.» | igual |
| V-43, V-44 | estado | a recusa automática da transmissão não escolhida; a escolha que morre com a sala | palco-imagem.js:183–200, 593–599 · `assistir` | igual | A mesma lógica. | igual |
| V-45 | estado | a fileira com botões de padding 6/12 | tela-chamada.css:235–266 · — | adaptado | Botões na altura de um toque, quebrando na largura da tela. | igual |
| V-46 | estado | a barra de controles só com `snapshot.tela` alheia | tela-chamada.js:235–242 · — | igual | Sob a imagem. Ver o achado 1. | igual |
| V-47 | ação | `SILENCIAR` ↔ `OUVIR` (com o title explicando) | tela-chamada.js:251–272, 882–891 · `calar_a_tela`, `som_da_tela` | adaptado | O mesmo botão. O que o title explica vira nota ao lado: «Cala só esta transmissão; as pessoas continuam.» **Depende de medir:** o som da tela hoje vai direto à mistura nativa, e a página precisa de um caminho para ele (achado 3). | igual |
| V-48 | ação | `VOLUME` 0–200 (o som da transmissão) | tela-chamada.js:895–908 · `ajustar_volume_da_tela`, `calar_a_tela` | adaptado | Um controle deslizante na largura do palco, em vez dos 120 px. A releitura pula enquanto o dedo está no controle, em vez de usar o foco. A mesma dependência do som de V-47. | igual |
| V-49 | ação | `PARAR DE ASSISTIR` | tela-chamada.js:913–915; palco-imagem.js:490–492 · `assistir` | igual | Sob a imagem. | igual |
| V-50 | ação | `TELA CHEIA` ↔ `SAIR DA TELA CHEIA` | palco-imagem.js:432–467, 494–496 · `tela_cheia` | adaptado | **O aparelho deitado ou o botão TELA CHEIA.** O corte por CSS faz a tela cheia. `tela_cheia` é recusa nomeada, porque não há janela; hoje a falha só vai para o console (palco-imagem.js:462–466). | igual |
| V-51 | ação | duplo clique no canvas | palco-imagem.js:498–501 · `tela_cheia` | adaptado | Sem duplo clique: o aparelho deitado ou o botão TELA CHEIA. | igual |
| V-52 | atalho | `Escape` sai da tela cheia | palco-imagem.js:503–516 · `tela_cheia` | adaptado | Um botão **SAIR DA TELA CHEIA** visível sobre a imagem, ou pôr o aparelho em pé. | igual |
| V-53 | estado | `ESC PARA SAIR DA TELA CHEIA` (some em 4 s) | tela-chamada.css:956–985 · — | adaptado | A dica vira o próprio botão SAIR DA TELA CHEIA, que não some. | igual |
| V-54 | estado | saída automática da tela cheia quando a transmissão acaba | palco-imagem.js:475–484 · `tela_cheia` | igual | A mesma regra. Ver o achado 1: no estado padrão ela desfaz a tela cheia. | igual |
| V-56 | estado | `ESTA SALA ESTÁ VAZIA` | tela-chamada.js:334–336 · — | igual | Na grade da CHAMADA. | igual |
| V-58 | estado | só eu: um cartão `APELIDO (você)` | tela-chamada.js:347–351, 464–465 · — | igual | Um cartão na grade, sem volume. | igual |
| V-59 | estado | a grade de colunas com mínimo de 304 px | tela-chamada.css:378–389 · — | adaptado | Uma coluna na largura da tela, sem a trilha de 60 px. | igual |
| V-60 | estado | cartões repintados, chaveados por apelido | tela-chamada.js:328–357 · — | igual | A mesma regra (achado 7). | igual |
| V-61 | estado | avatar: iniciais ou retrato (incluindo o `pessoa.avatar` de MOD) | tela-chamada.js:457–463; tela-sessao.js:4535–4598 · `imagem_da_pessoa` | igual | No cartão. | igual |
| V-62 | estado | a capa de MOD (`pessoa.capa`) | ainda não existe; plano F3 Tarefa 6 (`pintarCartao`, `data-pessoa-da-capa`) · — | igual | No cartão da CHAMADA, **animada até o mesmo teto** do desktop (D-capa-1). Quadro estático com `prefers-reduced-motion`; o halo, `FALANDO` e as placas ficam acima da capa. Precisa ser construída nos dois lados. | igual |
| V-63 | estado | `APELIDO` / `APELIDO (você)` | tela-chamada.js:464–465 · — | igual | No cartão. É o apelido do protocolo, que continua à vista (D-nucleo-1). | igual |
| V-64, V-65, V-66, V-67 | estado | `FALANDO` (com halo), `MUDO`, `OUVINDO`, `SEM SOM`, e as frases `está falando agora` / `está com o microfone desligado` / `está só ouvindo` / `não está ouvindo ninguém`… | tela-chamada.js:455, 476–497, 562–571 · — | igual | No cartão, em palavra e em cor. | igual |
| V-68, V-69, V-70, V-71 | estado | `SINAL` `█`/`▒`/`░` + número + barra de 24 blocos (Nominal, Degraded, Critical) | tela-chamada.js:499–506; base.js:80–82 · — | igual | No cartão. A barra de 24 blocos cabe na largura de um cartão de celular. | igual |
| V-72 | estado | o cartão não marca quem compartilha a tela | tela-chamada.js:453–514 · — | igual | Quem compartilha está dito no palco e na fileira. | igual |
| V-73 | estado | `VOLUME` por pessoa (só para outros, com `audio_available`) | tela-chamada.js:511–513 · — | adaptado | Sai do cartão e vai para a **folha da pessoa**, que um toque no cartão abre. Só aparece para outra pessoa e com o áudio aberto (D-52). | igual |
| V-74, V-75, V-76 | ação | `−`, `+`, as dez células (20–200%) | tela-chamada.js:420–443, 810–831 · `set_volume` | adaptado | Na folha da pessoa: `−`, as dez células e `+`, em tamanho de toque. Os aria-labels viram o título visível «VOLUME DE <APELIDO>». | igual |
| V-77 | estado | `NNN%` (o valor que esta janela mandou) | tela-chamada.js:516–539; tela-sessao.js:143 · — | adaptado | Na folha da pessoa, ao lado das células. | igual |
| V-80 | estado | `SAIR DA SALA` ↔ `SAIR DO SERVIDOR` | tela-chamada.js:719–728 · `leave_voice_room` | adaptado | `SAIR` (da sala) na faixa e na CHAMADA; `SAIR DO SERVIDOR` no fim de AJUSTES. | igual |
| V-81, V-82 | camada | `COMPARTILHAR A TELA` (a caixa), com `ESC`, toque no véu e Escape | camada-compartilhar.js:498–535, 552–553, 692–698 · `snapshot`, `permissao_de_tela`, `modulo_de_video_a_baixar`, `fontes_de_tela`, `exclusao_do_som_da_captura` | fora na 0.16 | A caixa não existe no celular. Vale a frase de V-78, no palco, e a entrada de NO COMPUTADOR. | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida); sem módulo de vídeo a baixar |
| V-83, V-84, V-85, V-86, V-87 | estado/ação | a permissão: `ESTE APP AINDA NÃO PEDIU…` + `PEDIR AO SISTEMA`, `O SISTEMA NEGOU A GRAVAÇÃO DE TELA`, `ESTA VERSÃO NÃO SABE COMPARTILHAR TELA`, concedida | camada-compartilhar.js:161–193, 253–263, 576–585 · `permissao_de_tela`, `pedir_permissao_de_tela` | fora na 0.16 | Sem lugar. A frase de V-78 e NO COMPUTADOR cobrem. | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida); sem módulo de vídeo a baixar |
| V-88, V-89 | estado/ação | `FALTA O MÓDULO DE VÍDEO.` + `BAIXAR O MÓDULO` → `BAIXANDO…` | camada-compartilhar.js:236–251, 594–608 · `modulo_de_video_a_baixar`, `baixar_modulo_de_video` | fora na 0.16 | Sem lugar: assistir não exige o módulo (types.rs:1700–1702). Nada a baixar no celular. | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida); sem módulo de vídeo a baixar |
| V-90, V-91, V-92, V-93, V-94, V-95, V-96, V-97, V-98, V-99 | estado/ação | `O QUE MOSTRAR` (lista, vazio, escolher), `RESOLUÇÃO`, `QUADROS`, `INCLUIR SOM` + aviso, `COMPARTILHAR`/`TROCAR`, `PARAR`, os erros `FRASES.Screen*`, o redesenho em `ScreenChanged` | camada-compartilhar.js:273–473, 481–493, 561–567, 610–688 · `fontes_de_tela`, `ajustar_limites_da_tela`, `exclusao_do_som_da_captura`, `compartilhar_tela`, `parar_de_compartilhar` | fora na 0.16 | Sem lugar. A frase de V-78 e a entrada de NO COMPUTADOR cobrem («…com resolução, quadros e som»). | adaptado: compartilhar pelo `getDisplayMedia` do navegador (F4, com medida); sem módulo de vídeo a baixar |
| V-100, V-101 | estado | `AVISOS.ScreenShareTaken`, `AVISOS.ScreenShareOverHostUplink` | frases.js:174–187 · — | fora na 0.16 | Não chegam: os dois avisos são de quem envia. Quem assiste vê só a transmissão fechar, o que é igual a V-23. | igual |
| V-102 | tela, ação | o palco dos MODs (`chamada.palco`): `PALCO DE <MOD>`, **VER A GRADE**, e **VER O PALCO** na barra da chamada | index.html:1323, 1527–1536; tela-chamada.js:143–177 (F3, `c7c64814`) · o `lugar` ao MOD | adaptado | No lugar da grade, do palco da transmissão até a faixa; a barra quebra fileira. VER O PALCO e VER A GRADE com 44. Caso `palco-de-mods` (`375ebc27`). | igual |
| V-103 | estado | a capa de uma pessoa no cartão (`pessoa.capa`, D-capa-1) | tela-chamada.js:610 (`vestirCapa`); tela-chamada.css:472–481 · `ler_imagem_mod` | igual | O cartão do desktop, animado até o mesmo teto, com as placas dele. Caso `capa-na-chamada` (`a0818be6`): acima de 4,5:1 contra as três capas de prova; a placa de quem fala é a laranja do desktop. | igual |

#### As frases novas, para `frases.js`


- «Compartilhar a própria tela é pelo SEELE no computador. A tela de quem compartilhar aparece aqui.» (V-78)
- «COMPARTILHAR A TELA — mostrar um monitor ou uma janela a quem está na sala, com resolução, quadros e som.» (NO COMPUTADOR)
- «Deite o aparelho ou toque em TELA CHEIA para ver a imagem inteira.» (V-19b)
- «A imagem começa no próximo quadro-chave de <APELIDO>.» (V-38t)
- «Sem imagem: só o áudio da sala.» (V-42t)
- «Cala só esta transmissão; as pessoas continuam.» (V-47)
- «ESTE NAVEGADOR NÃO DECODIFICA VÍDEO · ATUALIZE O NAVEGADOR OU O SISTEMA DO APARELHO» (V-27)
- «ESTE APARELHO NÃO DECODIFICA avc1.PPCCLL» (V-29)
- «O VÍDEO NÃO PÔDE SER PREPARADO NESTE APARELHO» (V-31)
- «ALGUÉM ESTÁ COMPARTILHANDO NUMA VERSÃO QUE ESTA PÁGINA NÃO LÊ · A VERSÃO DA PÁGINA ESTÁ EM AJUSTES › ESTE APARELHO» (V-34)
- «VOLUME DE <APELIDO>» (V-74)
- «Criar canal é pelo SEELE no computador.» (V-07; a frase é da área de canais)

### Achados para o desenho


1. **No estado padrão, o palco diz «NINGUÉM ESTÁ COMPARTILHANDO A TELA» com a imagem na tela.** `snapshot.tela` fica nulo enquanto ninguém escolheu, e com ele somem a contagem, SILENCIAR, VOLUME, PARAR DE ASSISTIR e TELA CHEIA; o celular herda (lib.rs:2864–2868, 821–826; tela-chamada.js:142–174).
2. **Cada `ScreenFrame` dispara um `snapshot`.** O ouvinte da sessão chama `atualizar()` para todo evento, e pela ponte isso vira um snapshot por quadro (tela-sessao.js:4447–4478).
3. **O som da transmissão não tem caminho até a página.** Ele vai direto à mistura nativa, sem evento (lib.rs:4162–4177).
4. **H.264 High no iPhone está por medir.** O perfil é lido do SPS e o codificador manda High (`profile_idc` 100); a decodificação por WebCodecs no iPhone não foi conferida (palco-imagem.js:87–95, 268–277).
5. **Assistir esbarra no teto do WebKit.** A conexão trava em 16 MB ou 7600 fluxos, e um fluxo de tela assistida passa disso; a reconexão precisa caber no meio da transmissão (`docs/adr/0057-o-celular-entra-pelo-navegador.md:611–617`).
6. **Uma sessão que acaba em tela cheia esconde a tela de fim.** `display:none !important` cobre `#tela-fim`, e no desktop só o Escape tira dali. A saída da tela cheia tem de ir junto com o fim (palco-imagem.js:432–449; tela-fim.js:43–67).
7. **O volume por pessoa é chaveado por apelido.** O mapa `volumes`, o `set_volume` e o cartão usam o apelido, e a folha da pessoa herda essa chave (tela-chamada.js:810–831; main.rs:7869–7876).
8. **A recusa de `permissao_de_tela` hoje dá a frase errada.** O palco escreve «Esta versão não sabe compartilhar tela desta máquina…», e a ponte precisa dar a frase de V-78 (tela-chamada.js:153–165).
9. **`#palco-parada` está morto nos dois lados.** `parada` é sempre `None`, e não deve ser portado como se funcionasse (lib.rs:3695).
10. **O decodificador que morre não pede quadro-chave.** A imagem só volta quando quem transmite mandar um por conta própria (palco-imagem.js:281–288, 234).

## 5. Configurações, perfil e ajuda

Parte de `E-configuracoes.md` (commit `f7bc902b`, ramo `0.16/celular`) e aplica o
desenho de `00-desenho-e-regras.md`. Os ids `E-nn` são os do inventário; uma entrada que
junta vários ids cobre todos eles. Âncoras: `ui/…` e `src/main.rs` são relativos a
`apps/seele-app/`. Na coluna «no celular», cada frase entre «» é nova e vai para o `frases.js`; a chave proposta
está no §6.

**Uma premissa que muda a leitura das pontes (D-celular-1, docs/superpowers/specs/2026-10-08-0.16-design.md:774):** «o estado,
os `invoke` e as frases ficam nos módulos de hoje». Então o `tela-server.js` roda no
celular, e o que a ponte responde a cada `invoke` tem consequência mesmo nas seções que
o celular não desenha.

---

### A matriz


| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| E-01 | ação | engrenagem da entrada («Abrir configurações», só glifo + `title`) | ui/index.html:441; ui/tela-server.js:1482 · — | adaptado | A tela de início do web app não tem engrenagem: AJUSTES é a quarta aba do pé, dentro do servidor. O apelido de antes de entrar vai para SEU APELIDO de «ENTRAR EM <servidor>?», e o modo começa em VOZ. | igual |
| E-02 | ação | engrenagem da tira do operador | ui/index.html:1122; ui/tela-server.js:1483 · — | adaptado | A aba AJUSTES, sempre no pé. | igual |
| E-03 | ação | «ABRIR CONFIGURAÇÕES» no diálogo da porta | ui/index.html:3796; ui/camada-portaria.js:789-793 · — | fora na 0.16 | Ninguém a vê: o celular nunca hospeda. Em SÓ NO APP DO SEELE › HOSPEDAR UM SERVIDOR: «Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.» | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| E-04, E-06 | ação, estado | «PERFIL» do rodapé da entrada; «COMO OS OUTROS VÃO TE CHAMAR?» antes da 1ª entrada | ui/index.html:433; ui/tela-boot.js:183-205, 870; ui/camada-nomear.js:296-321 · `apelido_local`, `escolher_apelido_local` | adaptado | O campo SEU APELIDO de «ENTRAR EM <servidor>?» faz as duas coisas. Com ele vazio, a nota `ENTRAR_SEM_NOME` aparece embaixo. | igual |
| E-05 | ação | «OPERADOR» + apelido (abre SEU PERFIL) | ui/index.html:1094; ui/camada-nomear.js:444 · — | adaptado | SEU PERFIL fica no alto de AJUSTES, com a capa. Tocar abre a subpágina SEU PERFIL. | igual |
| E-07, E-08 | ação | «?» no cabeçalho das salas de voz; tecla `?` | ui/index.html:1042; ui/camada-ajuda.js:60, 73-78 · — | adaptado | O «?» continua no cabeçalho de SALAS DE VOZ (aba SERVIDOR). A tecla vira esse botão e a entrada O QUE É O QUÊ em AJUSTES › SOBRE. | igual |
| E-09…E-13, E-170…E-174 | ação, camada | «+» nova sala de voz; «+» novo canal; renomear sala ou canal; a camada `#nomear` com CRIAR/RENOMEAR, CANCELAR, recusa | ui/index.html:1034, 1050, 3944-3968; ui/tela-sessao.js:725-741, 862-865, 1086-1112, 4230-4253; ui/camada-moderar.js:745-758; ui/camada-nomear.js:43-127 · `criar_voice_room`, `criar_linha`, `open_channel`, `renomear_voice_room`, `renomear_linha` | fora na 0.16 | É administração, e a camada `#nomear` nunca abre. Quem tem `may_manage_voice_rooms` vê esta frase no lugar do «+», nos cabeçalhos de SALAS DE VOZ e CANAIS (aba SERVIDOR): «Criar e renomear salas de voz e canais é no SEELE do computador.» A mesma frase vai para NO COMPUTADOR. | igual |
| E-20, E-22, E-23 | camada, ação, estado | «CONFIGURAÇÕES»: menu de duas colunas (216 px + corpo), dez botões de seção, título e subtítulo fixos | ui/index.html:2171-2431; ui/tela-server.js:122-176, 1495; ui/tela-server.css:105-110 · por seção | adaptado | AJUSTES vira uma lista de seções: ÁUDIO, APARÊNCIA, ESTE SERVIDOR, PRÉVIAS DE LINK, ESTE APARELHO, O QUE O CELULAR NÃO FAZ, SOBRE, SAIR DO SERVIDOR. Cada uma abre numa página própria com «‹ AJUSTES»; o título vem do `data-titulo` e o subtítulo do `data-sub`. A seção nova abre no topo, como `abrirSecao` já faz. | igual |
| E-21 | estado | grupos ESTE DISPOSITIVO · MODS · ESTE SERVIDOR · APLICATIVO | ui/index.html:2235-2353; ui/tela-server.js:989-1003 · `snapshot` | adaptado | A ordem das seções faz o papel dos grupos: primeiro o que é pessoal, depois ESTE SERVIDOR, ESTE APARELHO e O QUE O CELULAR NÃO FAZ. | igual |
| E-24, E-26, E-27 | ação | «VOLTAR» / «VOLTAR À ENTRADA» / «VOLTAR AO SERVIDOR»; toque no véu; Escape | ui/index.html:2419; ui/tela-server.js:103-106, 889-936, 1493, 1562-1567 · — | adaptado | Cada página tem «‹ AJUSTES», e o botão voltar do Android faz o mesmo. Não há véu nem Escape: sai-se de AJUSTES tocando outra aba. | igual |
| E-25, E-92 | estado | «SAIR SEM SALVAR?» do rascunho de MODs; SALVAR ALTERAÇÕES / DESCARTAR | ui/tela-server.js:900-912; ui/index.html:3211-3217 · — | fora na 0.16 | Não há rascunho no celular, porque aplicar conjunto é administração. A frase em SÓ NO APP DO SEELE › INSTALAR E LIGAR MODS: «Os MODs deste servidor rodam aqui. Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| E-28 | estado | faixa de contexto: «Na sala X · microfone abre enquanto segura ESPAÇO» / «microfone mudo»… | ui/index.html:2445; ui/tela-server.js:848-886 · — | adaptado | A faixa de voz acima das abas já diz a sala e o microfone. No modo SEGURAR PARA FALAR ela diz «microfone abre enquanto segura o botão», e não o nome da tecla. | igual |
| E-29, E-30 | estado | laço de 500 ms e releitura ao abrir (tecla, permissão, aparelhos, retrato) | ui/tela-server.js:734-772, 959-1005, 1576-1578 · `snapshot`, `modo_de_voz_escolhido`, `permissao_de_microfone`, `microfones`, `microfone_escolhido` | igual | O mesmo laço, enquanto AJUSTES › ÁUDIO estiver à vista. A tecla de falar não é relida. | adaptado: o navegador não abre os ajustes do sistema: a frase diz onde fica a permissão |
| E-40 | tela | «MICROFONE E SOM» — «Escolha o microfone e a saída desta máquina» | ui/index.html:2238-2250, 2512 · `controles_da_voz` | adaptado | AJUSTES › ÁUDIO, a primeira seção, com o subtítulo «O microfone e o som deste aparelho». | igual |
| E-41, E-42 | estado, ação | permissão negada (`PERMISSAO_DE_MICROFONE.*`, só Windows) e «ABRIR OS AJUSTES DO SISTEMA» | ui/index.html:2525-2531; ui/tela-server.js:1666-1686; ui/frases.js:253-271; src/main.rs:8089-8124 · `permissao_de_microfone`, `abrir_ajustes_do_microfone` | adaptado | O bloco aparece em ÁUDIO quando o navegador recusou: pela Permissions API, ou por `NotAllowedError` ao entrar na sala. Ele usa a variante nova `NegadaPeloNavegador`, que diz «O NAVEGADOR NÃO DEIXA ESTA PÁGINA USAR O MICROFONE.», com a nota que diz onde liberar no computador, no iPhone e no Android (a tabela é uma para as duas cascas). Não há botão, porque uma página não abre os ajustes do sistema. As três variantes de Windows nunca aparecem. | adaptado: o navegador não abre os ajustes do sistema: a frase diz onde fica a permissão |
| E-43, E-44, E-45 | estado, ação | «DISPOSITIVO DE ENTRADA»: «PADRÃO DA MÁQUINA [· nome]» + um botão por aparelho, marcas EM USO / ESCOLHIDO / PADRÃO; vazia: «ESTA MÁQUINA NÃO LISTOU DISPOSITIVO NENHUM»; tocar escolhe | ui/index.html:2541-2543; ui/tela-server.js:189-331, 706-719; src/main.rs:8266-8364 · `microfones`, `microfone_escolhido`, `escolher_microfone` | adaptado | ÁUDIO › MICROFONE lista o que `enumerateDevices()` dá como `audioinput`. Escolher grava no navegador e, se houver sala, reabre a captura com `deviceId: { exact }`. EM USO vem de `getSettings().deviceId` da faixa aberta. **O que se perde:** antes da primeira permissão os nomes vêm vazios, e a lista diz «Os nomes dos microfones aparecem depois que o navegador deixar a página usar o microfone, na primeira sala de voz.»; o «PADRÃO DA MÁQUINA · <nome>» some, porque o Safari não expõe o padrão, e a primeira opção vira «PADRÃO DO APARELHO»; o id muda se os dados do site forem apagados; no iPhone, a rota do fone Bluetooth é do sistema. A lista vazia diz «ESTE APARELHO NÃO LISTOU MICROFONE NENHUM.» | adaptado: os aparelhos que o navegador lista; a saída só onde ele deixa escolher |
| E-46 | estado | recusa da troca: `FRASES.DispositivoSumiu` / `FRASES.NaoGravei` | ui/index.html:2548; ui/frases.js:987-1003 · `escolher_microfone` | igual | As mesmas frases em ÁUDIO. «Preso a outro programa» vale também no celular, com outro app usando o microfone. | adaptado: os aparelhos que o navegador lista; a saída só onde ele deixa escolher |
| E-47 | estado | nota «A troca vale agora e fica guardada nesta máquina. Escolher pelo nome fixa ele…» | ui/index.html:2589-2591 · — | adaptado | A nota vira «A troca vale na sala de agora e fica guardada neste navegador.» O «fixa ele» sai: ele descreve o AudioUnit do macOS, que a página não tem. | igual |
| E-48 | estado | «NÍVEL DE ENTRADA»: «— SEM SESSÃO DE ÁUDIO» + nota / 26 blocos + % | ui/index.html:2594-2626; ui/tela-server.js:339-364 · `snapshot` | adaptado | Igual com sala. Fora de sala, a nota vira «O medidor mede a voz que está indo para uma sala. Entre numa sala de voz para ver o nível.», porque pela D-52 o microfone só abre na sala. Os 26 blocos cabem em 390 pt na fonte de dados. | igual |
| E-49 | ação | «COMO O MICROFONE ABRE»: TECLA / VOZ / ABERTO | ui/index.html:2640-2658; ui/tela-server.js:387-394, 722-731; src/main.rs:7579-7603 · `set_voice_mode`, `modo_de_voz_escolhido` | adaptado | ÁUDIO › COMO O MICROFONE ABRE, um botão abaixo do outro. O primeiro é «VOZ — abre quando você fala», o padrão. Depois vêm «SEGURAR PARA FALAR — abre só enquanto o dedo está no botão da faixa» (o `PushToTalk` de sempre) e «ABERTO — fica aberto o tempo todo». Embaixo, a nota «Com a tela bloqueada, só o VOZ funciona.» | igual |
| E-50 | ação | «REDUÇÃO DE RUÍDO» — «reduzir ruído de fundo do microfone» | ui/index.html:2677-2692; ui/tela-server.js:574-581; src/main.rs:7626 · `ajustar_reducao_de_ruido` | adaptado (proposta) | ÁUDIO › TRATAMENTO DO MICROFONE, só informativo e sem interruptor: «O navegador trata o microfone deste aparelho: cancela o eco, reduz o ruído de fundo e ajusta o ganho. Vale sempre aqui.» O motivo: o ADR 0057 (:193-195) parte da configuração inteira do navegador, porque tirar uma peça não foi medido, e o filtro F02 do produto (Rust) não entra na página na 0.16. `controles_da_voz` responde `supressao: 1`. | igual |
| E-51, E-52 | ação | «SENSIBILIDADE DA VOZ» (régua, valor «… dBFS · padrão») e «USAR O PADRÃO» | ui/index.html:2713-2728; ui/tela-server.js:517-548, 586-605; src/main.rs:7662, 7689 · `ajustar_sensibilidade_da_voz`, `controles_da_voz` | adaptado | A mesma régua em ÁUDIO, valendo para o portão do modo VOZ da ponte (`voz-web.js`). A frase passa a ser «Mais sensível abre com fala baixa e também com mais ruído. Escolher um valor aqui o deixa fixo. Só vale no modo VOZ.»: sai a parte do corte que acompanha a sala e do teste abaixo. A régua tem 44 pt de alvo. | igual |
| E-53…E-56 | ação, estado | «TESTAR O MICROFONE»: COMEÇAR/PARAR O TESTE, barra com corte e frase «… dBFS · TRANSMITINDO · N aberturas», «OUVIR-ME»/«PARAR DE OUVIR», teste que fecha ao sair | ui/index.html:2742-2761; ui/tela-server.js:607-701, 923; src/main.rs:7728-7786 · `abrir_teste_de_microfone`, `estado_do_teste_de_microfone`, `fechar_teste_de_microfone`, `monitorar_o_microfone` | fora na 0.16 | Pela D-52, o microfone só abre na sala. No lugar do bloco, em ÁUDIO: «O microfone só abre numa sala de voz. Entre numa para ver o nível da sua voz no medidor acima.» Em NO COMPUTADOR: «Testar o microfone fora de uma sala e ouvir a própria voz é no computador.» | o bloco do desktop, sem frase; a ponte atende pelo `getUserMedia`, e abrir e fechar o teste não deixa faixa viva (decisão da orquestradora, 09/10; caso `casca-larga`) |
| E-57 | estado | «FONE»: «USE FONE. O SEELE ainda não cancela eco: sem fone, quem fala com você se ouve de volta.» | ui/index.html:2796-2799; ui/frases.js:285-286 · — | adaptado | ÁUDIO › FONE, com a frase do celular: «O navegador deste aparelho cancela o eco. Com fone, o som do alto-falante não volta para o microfone.» A do desktop seria falsa aqui (achado 6). | igual |
| E-58, E-59 | ação, estado | «DISPOSITIVO DE SAÍDA» (lista, marcas, nota «fixa ele», `<details>` «E quando o sistema troca de aparelho padrão?») | ui/index.html:2783-2822; ui/tela-server.js:84-90; src/main.rs:8376-8420 · `saidas`, `saida_escolhida`, `escolher_saida` | fora na 0.16 | No lugar da lista, em ÁUDIO › SAÍDA DE SOM: «O SOM SAI POR ONDE O CELULAR MANDAR.» e a nota «O navegador não escolhe a saída. Troque entre fone, Bluetooth e alto-falante pela Central de Controle do iPhone ou pelo controle de som do Android.» Em NO COMPUTADOR: «Escolher a saída de som é no computador.» | adaptado: os aparelhos que o navegador lista; a saída só onde ele deixa escolher |
| E-60 | tela | «ATALHOS» — «Teclas para agir sem tirar a mão do teclado» | ui/index.html:2253-2262, 2839-2971 · — | adaptado | Não há seção. Cada atalho tem um controle visível (E-61, E-62), e a ajuda lista os gestos (§4). | igual |
| E-61 | ação | «Falar enquanto segura» + botão da tecla («APERTE UMA TECLA») | ui/index.html:2890-2895; ui/tela-server.js:1586-1653; ui/tela-sessao.js:4407-4420 · `tecla_de_falar`, `escolher_tecla_de_falar` | adaptado | O botão grande SEGURAR PARA FALAR, na faixa de voz. Ele fecha em `touchcancel`, ao sair do botão, com a página oculta e na queda. Não há tecla a escolher. | adaptado: a tecla de falar só vale com a página em foco: o navegador não tem atalho global, e a tela diz isso |
| E-62 | estado | `/` buscar · `ENTER` enviar e andar na busca · `CTRL+V`/`CMD+V` colar imagem · `ESC` fechar | ui/index.html:2897-2968; ui/tela-server.js:790-801 · — | adaptado | `/` vira BUSCAR NESTE CANAL (`#botao-buscar`, ui/index.html:1460). `ENTER` vira ENVIAR (`#botao-enviar`, :1633) e, na busca, os botões de ocorrência anterior e próxima (`#busca-anterior`/`#busca-proxima`, :1485, :1488). Colar imagem vira ANEXAR (`#botao-anexar`, :1590), que abre o seletor do navegador. `ESC` vira o FECHAR de cada folha e o voltar do Android. | igual |
| E-70, E-71 | tela, estado | «APARÊNCIA»: «LARANJA SOBRE NEGRO — EM USO»; «MAIS CONTRASTE — DESLIGADO NO SISTEMA» | ui/index.html:2987-3037 · — | adaptado | AJUSTES › APARÊNCIA (pessoal), com os dois cartões. O estado do contraste é lido de `matchMedia("(prefers-contrast: more)")`: «LIGADO NO CELULAR» ou «DESLIGADO NO CELULAR». Hoje ele é literal (achado 8). | igual |
| E-72 | ação | «ABRIR A ACESSIBILIDADE DO SISTEMA» | ui/index.html:3052-3054; ui/tela-server.js:810-825; src/main.rs:8145-8169 · `ha_ajustes_de_acessibilidade`, `abrir_ajustes_de_acessibilidade` | adaptado | Sem botão: a ponte responde `false`, e o código de hoje já o esconde. No lugar fica a nota «O contraste segue o do celular. No iPhone: Ajustes › Acessibilidade › Tela e Tamanho do Texto › Aumentar Contraste. No Android: Configurações › Acessibilidade.» | adaptado: o navegador não abre os ajustes do sistema: a frase diz onde fica a permissão |
| E-80, E-81, E-82 | tela, estado | «IDENTIDADE»; «APELIDO EM USO» (3 estados); frase da chave «A chave mora nesta máquina, num arquivo só seu…» | ui/index.html:2286-2294, 3057-3092; ui/tela-server.js:406-436 · `snapshot` | adaptado | AJUSTES › ESTE APARELHO › IDENTIDADE. APELIDO EM USO fica igual; o estado «você ainda não entrou num servidor» não ocorre, porque AJUSTES só existe dentro de um. A frase da chave vira «A chave mora neste navegador, guardada pela página, e não sai dele. Apagar os dados do site apaga a chave, e com ela o apelido em todo servidor que você já visitou.» (ADR 0057: chave do WebCrypto, sem backup). | igual |
| E-83, E-84 | estado, ação | «IMAGENS DE LINK»: lista de domínios, vazio «Nenhum domínio liberado…», «ESQUECER» com `title` | ui/index.html:3106-3120; ui/tela-server.js:448-483; src/main.rs:7819-7834 · `previas_consentidas`, `esquecer_previa` | adaptado | AJUSTES › PRÉVIAS DE LINK, com a mesma lista e o ESQUECER. O `title` vira texto sob a lista: «ESQUECER faz as imagens desse domínio voltarem a pedir permissão antes de serem buscadas.» Na frase do bloco, «o endereço desta máquina» vira «o endereço deste aparelho». | igual |
| E-90, E-91 | tela, ação | «MODS» e as abas INSTALADOS · CATÁLOGO · ARMAZENAMENTO · PERMISSÕES · DIAGNÓSTICO | ui/index.html:2297-2321, 3133-3390; ui/tela-server.js:123, 136-138, 989-993 · (camada-mods.js) | adaptado | Vai para AJUSTES › ESTE SERVIDOR › MODS DESTE SERVIDOR. INSTALADOS vira «o que roda», só leitura; PERMISSÕES e DIAGNÓSTICO ficam; ARMAZENAMENTO vai para ESTE APARELHO. CATÁLOGO, instalar de uma pasta e ligar/desligar ficam fora, com a frase de E-25. O detalhe é do levantamento da gestão de MODs. | igual |
| E-100…E-105 | tela, estado, ação | «A PORTA»: «CONEXÃO EXTERNA», link, «copiar», alcance, «ABRIR A PORTA AGORA», «QUEM PODE ENTRAR» / «PORTA · …» | ui/index.html:2324-2340, 2451-2510; ui/camada-portaria.js:200-208, 569, 659-797; ui/tela-boot.js:369-470 · `abrir_a_porta_no_firewall` | fora na 0.16 | Ninguém a vê: a seção só existe para quem hospeda nesta máquina. Em SÓ NO APP DO SEELE, a frase de E-03. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| E-110, E-113 | tela, estado | «SERVIDOR» / «ESTE SERVIDOR»; «O QUE ESTÁ VALENDO» (imagem ou «SEM IMAGEM») | ui/index.html:2341-2351, 3608-3655; ui/tela-server.js:1059-1149 · `snapshot`, `icone_do_server` | adaptado | AJUSTES › ESTE SERVIDOR mostra a todos o nome e a imagem (ou «SEM IMAGEM»), só para leitura, buscados pela mesma revisão. Hoje a seção só aparece para quem administra. | igual |
| E-111, E-112, E-114…E-117 | estado, ação | «NOME DESTE SERVIDOR» (salva ao sair do campo); «ESCOLHER IMAGEM»; «TIRAR A IMAGEM»; a regra da imagem; as recusas; perder a permissão | ui/index.html:3611-3690; ui/tela-server.js:1142-1252, 1514-1529; src/main.rs:3804-4055 · `renomear_server`, `escolher_icone_do_server`, `tirar_icone_do_server`, `regras_do_icone_do_server` | fora na 0.16 | É administração. Quem tem `may_customise_server` vê, sob o nome e a imagem: «O nome e a imagem deste servidor se trocam no SEELE do computador.» A frase some com a permissão, como a seção some hoje. Ela vai também para NO COMPUTADOR. | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| E-120…E-124 | tela, estado, ação | «VERSÕES»: lista, «ABRIR», «REMOVER», «GUARDAR A MAIS NOVA DO CATÁLOGO» | ui/index.html:2362-2370, 3467-3488; ui/camada-versoes.js:201-362 · `versoes_instaladas`, `ocupacao_do_deposito`, `abrir_versao`, `remover_versao`, `baixar_versao` | fora na 0.16 | Em SÓ NO APP DO SEELE › VERSÕES LADO A LADO: «O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE.» | fora na 0.16: «Guardar e abrir outras versões é pelo app do SEELE. O navegador abre a página da versão do servidor que o link pede.» |
| E-130…E-133 | tela, ação, estado | «MALHA» / «CAMINHO ENTRE PARES»: receber direto, emprestar a subida, LIGAR/DESLIGAR, «Guardado…» | ui/index.html:2373-2383, 3407-3455; ui/camada-servidores.js:267-330 · `caminho_entre_pares`, `consentir_no_caminho_entre_pares` | fora na 0.16 | Em SÓ NO APP DO SEELE: «Caminho entre pares: no celular a tela de quem transmite vem sempre pelo servidor, e este aparelho não serve a tela a ninguém.» | fora na 0.16: «No navegador, a tela de quem transmite vem sempre pelo servidor, e este computador não a repassa a ninguém.» |
| E-140…E-146 | tela, ação, estado | «ATUALIZAÇÃO»: «PROCURAR ATUALIZAÇÃO», «VOCÊ ESTÁ NA ÚLTIMA VERSÃO.», «HÁ VERSÃO NOVA…», «ANTES DE INSTALAR», «INSTALAR E REABRIR», «BAIXANDO» + barra, as oito falhas | ui/index.html:2385-2395, 3515-3578; ui/tela-server.js:1295-1345, 1411-1478; src/main.rs:339, 9091-9163 · `abertura`, `procurar_atualizacao`, `instalar_atualizacao` | adaptado | AJUSTES › ESTE APARELHO › VERSÃO DA PÁGINA: «VERSÃO DA PÁGINA · 0.16.0» e «Não há o que instalar. A página vem de <origem>, uma por versão do SEELE: entrar num servidor de versão mais nova carrega a página dessa versão.» Não há PROCURAR nem INSTALAR (ADR 0057: um pacote por versão, escolhido pelo `v=` do link ou pela última visita; o pacote revogado deixa de ser servido). | adaptado: a página vem da origem: não há o que instalar, e uma versão nova chega ao abrir a página de novo |
| E-147 | estado | «A atualização para a X não foi aplicada…» (instalador do Windows) | ui/tela-server.js:1367-1405; src/main.rs:9265 · `atualizacao_que_nao_pegou` | adaptado | Não há instalador: a ponte responde `null`, e nada aparece. | adaptado: a página vem da origem: não há o que instalar, e uma versão nova chega ao abrir a página de novo |
| E-150, E-161 | camada, ação | «SEU PERFIL» com «FECHAR», Escape e o véu | ui/index.html:3862-3868; ui/camada-nomear.js:181-242, 323-340, 443-458 · `snapshot` | adaptado | SEU PERFIL fica no alto de AJUSTES, com a capa (D-capa-1). Tocar abre a subpágina SEU PERFIL; «‹ AJUSTES» e o voltar do Android fazem o papel do FECHAR. | igual |
| E-151 | estado | dois modos: com sessão (`escolher_apelido`) e sem (`escolher_apelido_local`) | ui/camada-nomear.js:201-217, 394-416 · `snapshot`, `apelido_local` | adaptado | Só o modo de dentro do servidor, porque AJUSTES só existe na sessão. O apelido de fora mora em «ENTRAR EM <servidor>?» (E-04). | igual |
| E-152, E-155 | estado, ação | a prévia (imagem ou inicial); «TIRAR» | ui/index.html:3870-3882; ui/camada-nomear.js:139-168, 364-374; src/main.rs:7344, 7405 · `meu_retrato`, `tirar_minha_imagem` | igual | SEU PERFIL. O retrato fica guardado no navegador (na origem), e não em `retrato.png`. | igual |
| E-153, E-154 | estado, ação | nota «PNG · até 8 KiB · 256 px.»; «ESCOLHER» | ui/index.html:3878-3881; ui/camada-nomear.js:349-362; src/main.rs:7287 · `escolher_minha_imagem`, `regras_do_icone_do_server` | adaptado | ESCOLHER abre o seletor de arquivo do navegador (`accept="image/*"`, que no celular oferece fotos e câmera). A imagem é reduzida na página antes de sair. A nota vira a mesma do servidor: «Qualquer imagem. Ela é reduzida a um distintivo de até 256 pixels e 8 KiB antes de sair deste aparelho.», com os números de `regras_do_icone_do_server`. | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| E-156 | estado | recusa da imagem em `#perfil-erro` | ui/camada-nomear.js:358-361 · `escolher_minha_imagem` | igual | O mesmo lugar, com as frases de `fraseDeIcone`. Hoje o perfil escreveria o enum cru (achado 5). | adaptado: o seletor de arquivo do navegador; os bytes vão pela ponte (F4) |
| E-157, E-162 | ação, estado | «APELIDO» (máx. 32) + «O que você já disse continua com o nome de então.»; recusa (`NicknameTaken`…) | ui/index.html:3897-3908; ui/camada-nomear.js:419-423 · `escolher_apelido` | igual | SEU PERFIL, com o teclado do celular. | igual |
| E-158 | estado | `ENTRAR_SEM_NOME` | ui/index.html:3919; ui/frases.js:296 · — | adaptado | Em «ENTRAR EM <servidor>?», sob SEU APELIDO vazio. | igual |
| E-159, E-160 | ação | «SALVAR» / «CANCELAR» | ui/index.html:3935-3938; ui/camada-nomear.js:394-441 · `escolher_apelido` | igual | No pé da subpágina SEU PERFIL. CANCELAR volta a AJUSTES. | igual |
| E-180, E-183 | camada, ação | «O QUE É O QUÊ»; fechar com o botão «ESC», Escape e o véu | ui/index.html:3970-3976; ui/camada-ajuda.js:31-59, 81-84; ui/camada-ajuda.css:120-126 · — | adaptado | Página de tela cheia, com termo e definição empilhados. O botão de fechar diz «FECHAR», como na etiqueta `arquivo/mobile-ios` (docs/revisao-celular-2026-10-05.md:57); o voltar do Android também fecha. | igual |
| E-181 | estado | «AS PALAVRAS» (oito verbetes) | ui/index.html:3979-4004 · — | igual | O verbete FONE leva a frase do celular (E-57). | igual |
| E-182 | estado | «AS TECLAS» (tecla de falar, `/`, ENTER, `?`, ESC) | ui/index.html:4016-4029; ui/camada-ajuda.js:36 · — | adaptado | O bloco some, e entram OS GESTOS e ONDE MORA CADA COISA (§4). | igual |
| E-190 | ação | entrada «Aparência do servidor» (origem «seele/estilo») em DOS MODS | SEELE-MOD-ESTILO/cliente/main.js:917-923; ui/tela-sessao.js:2438-2477 · contribuição `servidor.navegacao` | igual | Na aba SERVIDOR, entre as entradas DOS MODS, só para quem edita (como no desktop). Também em AJUSTES › ESTE SERVIDOR › APARÊNCIA DO SERVIDOR. | igual |
| E-191 | estado | região do MOD: amostras, «NÃO PUBLICADO», «EDITAR APARÊNCIA» / «VER APARÊNCIA» | SEELE-MOD-ESTILO/cliente/main.js:802-818 · região | adaptado | A região mora onde a F3 decidir (outro levantamento), e o botão abre o mesmo formulário. | igual |
| E-192, E-194…E-196 | camada, ação | diálogo «Aparência do servidor» (980 px); «VER SÓ NA MINHA SESSÃO»; abas CORES / FORMA / CONJUNTOS; «RESTAURAR PADRÃO» / «DESCARTAR» / «PUBLICAR PARA O SERVIDOR» | SEELE-MOD-ESTILO/cliente/main.js:588-631, 700-751, 856-905; servidor/main.js:22-36; ui/mods-superficies.js:227-233, 278; ui/mods-regiao.js:1476-1500 · `SeeleUI.superficies`, `SeeleUI.tema`, pedido `save`/`reset` | adaptado | D-estilo-1: o mesmo formulário em tela cheia, numa coluna, com as três ações fixas no pé. «VER NA SESSÃO» recolhe o diálogo e mantém o selo «prévia não publicada». As abas cabem em 390 px, e a cor usa o seletor do navegador mais o campo hexadecimal. Fechar com rascunho pergunta antes (`fecharComAlteracoes`), inclusive pelo voltar do Android. Publicar continua exigindo `ctx.admin` na metade de servidor do MOD. | igual |
| E-193 | estado | quem não edita: «Este é o tema deste servidor…» + amostra + «Revisão N.» | SEELE-MOD-ESTILO/cliente/main.js:681-694 · pedido `view` | igual | Abre só para leitura, pela mesma entrada ou pela mesma região. | igual |
| E-198 | estado, ação | a sala pessoal (F7): a faixa `#toque` (ATENDER, RECUSAR), a linha da sala na entrada e na sessão, e no perfil o SEU CÓDIGO, «receber ligações» e a agenda, todas com `data-sala-pessoal` | index.html:230, 491, 1224, 4098 (`a53c1349`, `086082d0`) · `meu_codigo`, `estado_da_sala`, `toques_agora` | fora na 0.16 | Escondidas pelas duas cascas do navegador (`a3d59f7a`), mesmo quando o script as mostra; SÓ NO APP DO SEELE diz «RECEBER LIGAÇÕES PELO SEU CÓDIGO». Caso `sala-pessoal`. | fora; a mesma frase |
| E-199 | estado | o tema de região do `operador.barra` e a capa de quem usa | tela-sessao.js:1175 (`vestirCapa` na `.operador`); base.js `escreverAsRegioesDoTema` (`f09e7540`) | adaptado | Na placa de SEU PERFIL e na região, em AJUSTES (`CELULAR.conteineresDasRegioes`, `2cfcdde4`); cada contêiner pinta o painel e a tinta da região. Casos `regiao-do-operador` e `capa-do-operador`. | igual ao desktop |

---

### As seções de AJUSTES que esta área alimenta (resumo da matriz)


- **SEU PERFIL** (no alto): E-05, E-150…E-162.
- **ÁUDIO**: E-40…E-59 e E-49 (modos).
- **APARÊNCIA** (pessoal): E-70…E-72.
- **ESTE SERVIDOR**: nome e imagem só para leitura (E-110, E-113), APARÊNCIA DO SERVIDOR (E-190…E-196), MODS DESTE SERVIDOR (E-90, E-91).
- **PRÉVIAS DE LINK**: E-83, E-84.
- **ESTE APARELHO**: IDENTIDADE (E-80…E-82), ARMAZENAMENTO (E-91), VERSÃO DA PÁGINA (E-140…E-147).
- **O QUE O CELULAR NÃO FAZ**, nos dois grupos da lista canônica: em SÓ NO APP DO SEELE, hospedar e a porta, os MODs, as versões lado a lado e a tela entre pessoas; em NO COMPUTADOR, salas e canais, nome e imagem do servidor, o teste do microfone e a saída de som.
- **SOBRE**: O QUE É O QUÊ (E-07, E-180…E-183).

---

### A ajuda no celular


**AS PALAVRAS** fica (E-181), com a frase de FONE do celular. **AS TECLAS** sai, e no
lugar entram dois blocos com termo e definição empilhados:

**OS GESTOS**
- **SEGURAR PARA FALAR**: «Segure o botão grande da faixa de voz para falar; soltar fecha. Só no modo SEGURAR PARA FALAR. Com a tela bloqueada, só o VOZ funciona.»
- **A FAIXA DE VOZ**: «Tocar nela abre a chamada. MUDO, SURDO e SAIR ficam nela, sempre à mão. RECOLHER volta às abas.»
- **BUSCAR**: «BUSCAR NESTE CANAL, no alto da conversa, procura no que já foi dito. Os botões ao lado andam entre as ocorrências.»
- **ENVIAR**: «O botão ENVIAR, ao lado da caixa de mensagem.»
- **ANEXAR**: «Abre o seletor do celular: foto, câmera ou arquivo.»
- **UMA PESSOA**: «Tocar numa pessoa abre a folha dela: volume e perfil.»
- **UMA MENSAGEM**: «Tocar numa mensagem abre as ações dela.»
- **FECHAR**: «Cada folha tem FECHAR. No Android, o botão voltar também fecha.»

**ONDE MORA CADA COISA**
- **SERVIDOR**: «Os servidores deste aparelho, as salas de voz, os canais e as entradas dos MODs.»
- **CONVERSA**: «O canal aberto, ou a página de um MOD.»
- **PESSOAS**: «Quem está em cada sala, e quem está fora do ar.»
- **AJUSTES**: «Seu perfil, o áudio, a aparência, este servidor, este aparelho, e o que o celular não faz.»
- **ESTA AJUDA**: «O «?» no alto das salas de voz, ou AJUSTES › SOBRE.»

O botão de fechar a ajuda diz «FECHAR».

---

### Achados para o desenho


1. O menu de duas colunas fixas (`216px minmax(0,1fr)`) não tem nenhuma `@media` de largura; em 360 pt sobram ~120 pt de corpo — ui/tela-server.css:105-110.
2. Três frases nomeiam a tecla e escreveriam «ESPAÇO» no celular se a lógica for reaproveitada: a faixa de contexto, o estado do operador e a ajuda — ui/tela-server.js:875; ui/tela-sessao.js:948; ui/camada-ajuda.js:36.
3. A permissão do microfone só é lida no Windows. O enum não tem variante para a recusa do navegador, e o botão «ABRIR OS AJUSTES DO SISTEMA» é incondicional — crates/seele-audio/src/device.rs:1494-1502; crates/seele-ffi/src/types.rs:1101-1111; ui/index.html:2528.
4. `desenharDispositivos` faz `Promise.all` das duas listas, e `abrirServer` faz `await` sem `catch`. Uma recusa de `saidas` derruba a lista de entrada e pula o `abrirTela` (foco e anúncio) — ui/tela-server.js:189-191, 766-771.
5. Reduzir a imagem mora no Rust (`icone::encolher`), e o retrato do celular precisa do equivalente no navegador. A nota «PNG · até 8 KiB · 256 px.» já está desatualizada, e `IconNotAPicture` não tem frase em `FRASES`, então o perfil escreveria o enum cru — src/main.rs:3939, 7287; ui/index.html:3879; ui/camada-nomear.js:358-361.
6. `AVISO_DE_FONE` («O SEELE ainda não cancela eco») é falso no celular, onde o eco do navegador está ligado. O guarda amarra as três cópias da frase — ui/frases.js:285; ui/index.html:2798, 4002; tests/frontend.rs:9836; docs/adr/0057:193-195.
7. A edição da aparência do servidor é do MOD ESTILO (API 4, diálogo de 980 px), cujas grades CORES e FORMA são `colunas: 2` sem consulta de contêiner; nenhum cliente consome `TemaMudou` neste commit (é o E-197 do inventário) — SEELE-MOD-ESTILO/cliente/main.js:597, 607-612, 867-870; crates/seele-proto/src/mods.rs:77.
8. «MAIS CONTRASTE — DESLIGADO NO SISTEMA» é literal, e «O SEELE tem um tema só» contradiz o tema do servidor, que no celular mora na seção vizinha — ui/index.html:3003, 3035.
9. Nenhum comando diz a versão em uso, e VERSÃO DA PÁGINA precisa que o pacote a carregue. Como uma página aberta recebe um conserto da mesma versão não está decidido — ui/index.html:3497-3503; docs/adr/0057 «A primeira visita sem `v=`»; plano F4 Tarefa 3.6.
10. O portão do modo VOZ no navegador não tem dono: o VOZ é o padrão do celular, a Tarefa 3.3 da F4 fala de «modo e ganhos» e não do portão, e a régua chama a ponte a cada passo — plano F4:815-835; ui/tela-server.js:586-594.

---

### Frases novas para o `frases.js` (chaves propostas)


| chave | frase |
|---|---|
| `PERMISSAO_DE_MICROFONE.NegadaPeloNavegador.diz` | «O NAVEGADOR NÃO DEIXA ESTA PÁGINA USAR O MICROFONE.» |
| `PERMISSAO_DE_MICROFONE.NegadaPeloNavegador.nota` | «Libere o microfone para esta página nos ajustes do navegador: no computador, no ícone ao lado do endereço da página; no iPhone, em Ajustes › Apps › Safari › Microfone; no Android, em Configurações do site › Microfone. Depois, entre de novo na sala de voz.» Uma nota só para as duas cascas, porque a tabela é uma; no produto desde a resposta da ponte da F4 (`06aaa94b`) |
| `CELULAR.microfonesSemNome` | «Os nomes dos microfones aparecem depois que o navegador deixar a página usar o microfone, na primeira sala de voz.» |
| `CELULAR.microfonesVazio` | «ESTE APARELHO NÃO LISTOU MICROFONE NENHUM.» |
| `CELULAR.trocaDeMicrofone` | «A troca vale na sala de agora e fica guardada neste navegador.» |
| `CELULAR.nivelForaDeSala` | «O medidor mede a voz que está indo para uma sala. Entre numa sala de voz para ver o nível.» |
| `CELULAR.modoSegurar` | «SEGURAR PARA FALAR — abre só enquanto o dedo está no botão da faixa» |
| `CELULAR.telaBloqueada` | «Com a tela bloqueada, só o VOZ funciona.» |
| `CELULAR.tratamentoDoMicrofone` | «O navegador trata o microfone deste aparelho: cancela o eco, reduz o ruído de fundo e ajusta o ganho. Vale sempre aqui.» |
| `CELULAR.sensibilidade` | «Mais sensível abre com fala baixa e também com mais ruído. Escolher um valor aqui o deixa fixo. Só vale no modo VOZ.» |
| `CELULAR.testeNaSala` | «O microfone só abre numa sala de voz. Entre numa para ver o nível da sua voz no medidor acima.» |
| `AVISO_DE_FONE_CELULAR` | «USE FONE. Sem fone, o som do alto-falante pode voltar pelo microfone, e quem fala com você se ouve de volta.» A frase de antes («O navegador deste aparelho cancela o eco…») afirmava o que ninguém mediu: a voz toca pelo Web Audio, e se o cancelador do navegador a pega varia por motor (F4, 09/10). Volta a ser revista com a medida da 3.3 da F4 |
| `CELULAR.saidaDiz` / `CELULAR.saidaNota` | «O SOM SAI POR ONDE O CELULAR MANDAR.» / «O navegador não escolhe a saída. Troque entre fone, Bluetooth e alto-falante pela Central de Controle do iPhone ou pelo controle de som do Android.» |
| `CELULAR.contraste` | «O contraste segue o do celular. No iPhone: Ajustes › Acessibilidade › Tela e Tamanho do Texto › Aumentar Contraste. No Android: Configurações › Acessibilidade.» |
| `CELULAR.chave` | «A chave mora neste navegador, guardada pela página, e não sai dele. Apagar os dados do site apaga a chave, e com ela o apelido em todo servidor que você já visitou.» |
| `CELULAR.esquecerPrevia` | «ESQUECER faz as imagens desse domínio voltarem a pedir permissão antes de serem buscadas.» |
| `CELULAR.regraDoRetrato` | «Qualquer imagem. Ela é reduzida a um distintivo de até {lado} pixels e {KiB} KiB antes de sair deste aparelho.» |
| `CELULAR.versaoDaPagina` | «Não há o que instalar. A página vem de {origem}, uma por versão do SEELE: entrar num servidor de versão mais nova carrega a página dessa versão.» |
| `NO_COMPUTADOR.salasECanais` | «Criar e renomear salas de voz e canais é no SEELE do computador.» |
| `NO_COMPUTADOR.nomeEImagem` | «O nome e a imagem deste servidor se trocam no SEELE do computador.» |
| `SO_NO_APP.hospedar` | «Pôr um servidor no ar, o link de convite, a porta e o firewall ficam no app do SEELE, instalado num computador.» |
| `SO_NO_APP.mods` | «Os MODs deste servidor rodam aqui. Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| `SO_NO_APP.versoes` | «O navegador abre a página da versão do servidor que o link pede. Guardar e abrir outras versões é do app do SEELE.» |
| `SO_NO_APP.telaEntrePessoas` | «No navegador, a tela de quem transmite vem sempre pelo servidor, e este aparelho não a repassa a ninguém.» |
| `NO_COMPUTADOR.testeDoMicrofone` | «Testar o microfone fora de uma sala e ouvir a própria voz é no computador.» |
| `NO_COMPUTADOR.saidaDeSom` | «Escolher a saída de som é no computador.» |
| `AJUDA_CELULAR.*` | as frases de OS GESTOS e ONDE MORA CADA COISA (§4) |

Os caminhos dos ajustes do sistema (iPhone e Android) nas frases de permissão e de
contraste precisam ser conferidos num aparelho antes de entrar.

## 6. Moderação, confirmações, portaria e aceite de MODs

Fonte: `F-moderacao.md` (inventário do mesmo diretório), commit `f7bc902b`. Desenho e regras:
`00-desenho-e-regras.md`. As âncoras são relativas a `apps/seele-app/ui/`, salvo indicação.
Os ids juntam os do inventário quando se trata do mesmo item. Nenhuma ação e nenhum estado
ficaram de fora.

**Toda frase entre «» é proposta para o `frases.js`.** As frases do aceite e as notas da caixa de
moderação estão hoje escritas no `index.html`, e não no `frases.js` (ver achado 8).

---

### A matriz


| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| **Moderação de pessoas** | | | | | | |
| M-01 | estado | sem verbo sobre pessoas: o nome é texto; MODERAR UMA PESSOA desabilitado com title «este servidor não deu a você nenhuma permissão de moderação» | `camada-moderar.js:104-108`, `:122-125`, `:240-247` · `snapshot` | igual | A folha da pessoa não mostra MODERAR, e o alerta não mostra o botão. Nada a dizer: no celular, ninguém modera. | igual |
| M-02 · M-03 · M-04 · M-05 · M-06 · M-11 | camada | porta = nome da pessoa em sala (`button[data-moderar-pessoa]`, title «expulsar, banir ou mover X»); caixa «MODERAÇÃO» com nota «O que você escolher aqui acontece na hora, sem avisar a pessoa.», QUEM (`<select>` «apelido — sala»), um bloco por flag, vazio «Não há mais ninguém em sala nenhuma deste servidor.», pessoa pré-escolhida | `tela-sessao.js:2255-2263`; `camada-moderar.js:259-301`, `:403-424`, `:694-698`, `:853`; `index.html:4276-4304` · `snapshot` | fora na 0.16 | Na **folha da pessoa**, só para quem tem `may_kick`, `may_ban` ou `may_move_person`, no lugar de MODERAR: «MODERAR É NO COMPUTADOR.\nExpulsar, banir e mover pessoas se faz pelo SEELE no computador.» Também em AJUSTES › NO COMPUTADOR (NO COMPUTADOR › MODERAR). | igual |
| M-23 (porta do alerta) | ação | **MODERAR UMA PESSOA** no `#banner` (fecha o alerta e abre a escolha sem alvo) | `index.html:698-701`; `camada-moderar.js:805-811` | fora na 0.16 | No alerta, só para quem tem algum dos três verbos e no lugar do botão, a mesma frase de M-02 em corpo de texto, sem botão. Sem permissão, nada (o `title` do desktop deixa de existir). | igual |
| M-12 | ação | **EXPULSAR** (nota «Encerra a sessão da pessoa agora. Ela pode entrar de novo em seguida — expulsar não barra a volta.») → «EXPULSAR X» | `camada-moderar.js:548-554`, `:813-819`; `index.html:4306-4310` · `expulsar_pessoa` | fora na 0.16 | Coberto pela frase de M-02 (folha da pessoa) e por NO COMPUTADOR › MODERAR. | igual |
| M-13 · M-14 · M-15 | ação | ATÉ QUANDO (para sempre · 1 dia · 7 dias · 30 dias); MOTIVO (placeholder «opcional, e só para o seu registro»); **BANIR** → «BANIR X», com a consequência por prazo | `camada-moderar.js:556-581`, `:821-839`; `index.html:4312-4337` · `banir_pessoa` | fora na 0.16 | Coberto pela frase de M-02 e por NO COMPUTADOR › MODERAR. | igual |
| M-16 | ação | MOVER PARA (`<select>` «sala — n/limite»); **MOVER** → «MOVER X» | `camada-moderar.js:303-322`, `:583-598`, `:841-849`; `index.html:4339-4346` · `mover_pessoa` | fora na 0.16 | Coberto pela frase de M-02 e por NO COMPUTADOR › MODERAR. | igual |
| **Mensagens** | | | | | | |
| M-22a · M-23 | ação | `remover` na **própria** mensagem (aparece no hover ou com `:focus-visible`) → «REMOVER MENSAGEM» → **REMOVER** | `camada-moderar.js:127-145`, `:700-713`; `camada-moderar.css:315-354`; `tela-sessao.js:1323` · `remover_mensagem` | adaptado | O toque na mensagem abre a **folha de ações**, que traz REMOVER para as mensagens da própria pessoa. REMOVER abre a folha de confirmação (C-00) com a mesma frase (autor, hora, corpo entre «»). Sem hover. | igual |
| M-22b | ação | `remover` na mensagem **de outra pessoa** (com `may_remove_message`) | as mesmas âncoras de M-22a · `remover_mensagem` | fora na 0.16 | Na folha de ações da mensagem de outra pessoa, só para quem tem `may_remove_message`, no lugar de REMOVER: «REMOVER MENSAGENS DE OUTRAS PESSOAS É NO COMPUTADOR.\nNeste aparelho, só as suas.» Também em NO COMPUTADOR › MODERAR. | igual |
| **Salas e canais** | | | | | | |
| M-24 · M-25 · M-26 · R-06 | ação | `×` da sala de voz (title «destruir X e pôr para fora quem estiver dentro»); desabilitado na última sala, com o motivo só no title; «APAGAR A SALA DE VOZ X ?» → **APAGAR X**; recusa `LastVoiceRoom` | `camada-moderar.js:147-174`, `:600-621`, `:721-739`; `tela-sessao.js:776`; `frases.js:200-201` · `snapshot`, `apagar_voice_room` | fora na 0.16 | Uma vez, no pé da lista de salas e canais da aba SERVIDOR, só para quem tem `may_delete_rooms` ou `may_manage_voice_rooms`: «APAGAR E RENOMEAR SALAS E CANAIS É NO COMPUTADOR.\nPelo SEELE no computador.» Também em NO COMPUTADOR › SALAS E CANAIS. | igual |
| M-27 · M-28 · M-29 · M-30 | ação | `×` do canal (title «destruir #X e tudo que foi escrito nela»); **peso do canal** (mensagens, autores, desde); recusa «Não consegui contar o que há dentro de #X…»; «APAGAR O CANAL #X ?» → **APAGAR #X** | `camada-moderar.js:176-193`, `:623-677`, `:766-803` · `snapshot`, `peso_da_linha`, `apagar_linha` | fora na 0.16 | A mesma frase de M-24, no mesmo lugar. Também em NO COMPUTADOR › SALAS E CANAIS. | igual |
| M-31 · M-32 · M-33 | ação | lápis do canal (title «trocar o nome de #X»), título do canal na barra e nome da sala viram botão → `#nomear` «RENOMEAR O CANAL» / «RENOMEAR A SALA», sem confirmação | `camada-moderar.js:195-221`, `:741-764`; `tela-sessao.js:715-745`, `:1018-1040`, `:1086-1119` · `renomear_linha`, `renomear_voice_room` | fora na 0.16 | O título do canal (CONVERSA) e o nome da sala continuam texto. A frase de M-24 cobre, no pé da lista em SERVIDOR. Também em NO COMPUTADOR › SALAS E CANAIS. | igual |
| **A folha de confirmação (a caixa `#moderar` como mecanismo)** | | | | | | |
| C-00 (M-00 · M-07 · M-17 · M-18 · M-19 · M-20 · M-21) | camada | caixa `#moderar` em modo confirmação: frase de consequência (`pre-line`), **CANCELAR** com o foco, botão de confirmar com o verbo e o nome (nunca «OK»), FECHAR fixo no rodapé, Escape, toque no véu | `camada-moderar.js:344-371`, `:432-449`, `:482-508`, `:855-886`; `camada-moderar.css:51-82`, `:187-274` · — | adaptado | **Folha de confirmação** de baixo, com corte seco: frase em cima, CANCELAR antes do verbo, FECHAR sempre visível, e o botão voltar do Android fecha. O Escape vira o FECHAR visível, e o foco inicial vira a ordem visual. Fica acima de toda página ou diálogo de MOD (no desktop, `z 80` contra 60/70). | igual |
| M-08 | estado | erro depois de confirmar: a caixa fica aberta com `fraseDeErro` (`role=alert`) | `camada-moderar.js:859-871` | igual | O erro aparece dentro da folha, que não fecha. | igual |
| M-09 | estado | recusa sem ato (`abrirRecusa`): só a frase e CANCELAR | `camada-moderar.js:451-479` | adaptado | Folha só com a frase e FECHAR. Quem a usa no celular é C-01. | igual |
| M-10 | estado | a sessão acaba com a caixa aberta: `abandonarModeracao` descarta o ato | `camada-moderar.js:510-536`; `tela-fim.js:62`; `tela-sessao.js:3772` | igual | A folha some quando a sessão acaba, e nenhum ato armado sobrevive para a próxima. | igual |
| C-04′ | estado | `aoFechar`: quem abriu é avisado nos dois desfechos, para readormecer a pilha de MOD | `camada-moderar.js:41-52`, `:436-443`, `:484-494`; `mods-superficies.js:583-599` | igual | A folha de confirmação avisa ao fechar, confirmando ou cancelando. | igual |
| C-01 | ação | «SALVAR EM DISCO» (`fraseDeSalvar`: pasta, nome ao lado, «não varre vírus», quarentena, «Nenhuma tela do SEELE abre este arquivo.») → **SALVAR EM DISCO**; recusa `fraseDeNaoSalvo` | `tela-sessao.js:3378-3424`; `frases.js:1142-1182` · `destino_do_anexo`, `salvar_anexo` | adaptado | Folha de confirmação; salvar é o download do navegador. A frase perde a pasta e a quarentena e mantém «não varre vírus» e «Nenhuma tela do SEELE abre este arquivo.» O botão vira BAIXAR. A recusa por nome vira folha com FECHAR. O texto final é da área de anexos. | adaptado: o download do navegador |
| C-02 | ação | «TROCAR DE SERVIDOR» (`consequenciaDeTrocar`) → **ENTRAR EM X** | `tela-sessao.js:3599-3662` · (`trocarDeServidor`) | igual | Folha de confirmação, iniciada pela trilha de servidores da aba SERVIDOR. Só existe o ramo sem hospedar, porque o celular nunca hospeda. | igual |
| C-03 | ação | «CONECTAR A OUTRO SERVIDOR» (`consequenciaDeIrParaAEntrada`: «…O campo do endereço fica vazio…») → **SAIR DE X** | `tela-sessao.js:3611-3674` | adaptado | Folha de confirmação. O destino é a tela de início do web app (servidores deste aparelho e COLAR UM LINK). A segunda frase vira: «Você sai de X agora, no meio do que estiver fazendo, e volta à lista de servidores deste aparelho.» | igual |
| C-04 | ação | «FECHAR SEM GRAVAR?» (superfície de MOD com rascunho) → **DESCARTAR E FECHAR** | `mods-superficies.js:572-599` | igual | Folha de confirmação sobre a página ou diálogo do MOD, com C-04′. Os MODs rodam no celular. | igual |
| C-05 | ação | «SAIR DO SERVIDOR» → **SAIR DE X** (só para quem hospeda) | `tela-chamada.js:783-790` | fora na 0.16 | Não há frase própria: o celular não hospeda, então SAIR DO SERVIDOR (AJUSTES, faixa de voz) sai sem esta pergunta, como o desktop faz para quem não hospeda (`tela-chamada.js:792`). HOSPEDAR em SÓ NO APP DO SEELE é de outra área. | fora na 0.16: não acontece: o navegador nunca hospeda |
| C-06 | ação | «APAGAR SERVIDOR?» → **APAGAR SERVIDOR** | `tela-boot.js:735-750` · `apagar_servidor` | fora na 0.16 | São servidores guardados para hospedar; o celular não os tem. Fica coberto por HOSPEDAR em SÓ NO APP DO SEELE (outra área). | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| C-07 · C-09 | ação | «SAIR SEM SALVAR?» (rascunho de MODs) e «APLICAR O CONJUNTO NOVO» → **SALVAR** | `tela-server.js:888-911`; `camada-mods.js:1434-1490` · `aplicar_conjunto_de_mods` | fora na 0.16 | Em SÓ NO APP DO SEELE (SÓ NO APP DO SEELE › INSTALAR E LIGAR MODS). A frase do lugar é da área de MODs. | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| C-08 | ação | «APAGAR MOD?» → **APAGAR MOD** (pacote do cache local) | `camada-mods.js:363-380` · `apagar_pacote_do_cache` | fora na 0.16 | Em SÓ NO APP DO SEELE (SÓ NO APP DO SEELE › INSTALAR E LIGAR MODS): «Instalar e apagar MODs é no app do SEELE.» O dono é a área de MODs, que decide o armazenamento do aparelho. | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| **A porta do servidor (`#portaria`)** | | | | | | |
| P-00 · P-20 · P-21 | camada | «A PORTA DESTE SERVIDOR»; aberta por `portaria-abrir` em CONFIGURAÇÕES · A PORTA · «QUEM PODE ENTRAR»; FECHAR / Escape / véu | `index.html:844-923`, `:2505-2509`; `camada-portaria.js:395-433`, `:569-571`, `:585-590` | fora na 0.16 | Só em AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. Ninguém a veria no celular, porque só a janela que hospeda tem a porta. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-01 · P-02 | estado | rótulo «porta» / «PORTA · N BATENDO» / «PORTA · ABERTA» / «PORTA · FECHADA» | `camada-portaria.js:198-209`, `:460-468` · `estado_da_porta` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-03 | estado | alarme «ESTE SERVIDOR ESTÁ ABERTO E ALCANÇÁVEL DE FORA DA SUA REDE.…» | `camada-portaria.js:57-62`, `:211-234` · `estado_da_porta` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-04 · P-05 · P-06 · P-09 · P-22 | estado | «QUEM ESTÁ BATENDO» / «JÁ DECIDIDOS»; vazios «Ninguém esperando.» / «Nada decidido ainda.»; cartão do pedido (diz chamar-se, como chegou, quando bateu); «Conferir impressão digital»; selo ADMITIDO/RECUSADO; ler a fila cala a faixa | `camada-portaria.js:76-138`, `:236-276` · `pedidos_da_portaria` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-07 · P-08 · P-10 | ação | **ADMITIR** / **RECUSAR** / **REVOGAR** com as confirmações «ADMITIR QUEM BATEU», «RECUSAR QUEM BATEU», «REVOGAR A DECISÃO» | `camada-portaria.js:280-335` · `decidir_pedido`, `revogar_admissao` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-11 · P-12 · P-13 | ação | SENHA DO SERVIDOR «definida» / «nenhuma — a senha não fecha nada»; campo + **DEFINIR**; **TIRAR** → «TIRAR A SENHA DO SERVIDOR» | `camada-portaria.js:182-184`, `:337-357`, `:592-606` · `definir_senha_do_server` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-14 · P-15 | ação | CONVITE DE USO ÚNICO «há convites emitidos» / «nenhum convite emitido»; observação + **GERAR** (o link vai para os campos do link) | `camada-portaria.js:185-187`, `:608-621`, `:642-665` · `criar_convite_do_server` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-16 · P-17 · P-18 | ação | PORTARIA «ligada — …» / «desligada — …»; **LIGAR**; **DESLIGAR** → «DESLIGAR A PORTARIA» | `camada-portaria.js:189-196`, `:359-379`, `:623-632` · `ligar_portaria` | fora na 0.16 | SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| P-19 | estado | erro «ESTA JANELA NÃO ESTÁ HOSPEDANDO NENHUM SERVIDOR.» / «O SERVIDOR DESTA MÁQUINA NÃO RESPONDEU» | `camada-portaria.js:160-178`, `:381-393`; `frases.js:898-906` | fora na 0.16 | Nenhum controle do celular chama a portaria, então a frase não aparece. Se a ponte for chamada, a recusa é nomeada (seção 2). | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| **A faixa de quem bate (`#portaria-batendo`)** | | | | | | |
| B-00 · B-01 · B-02 | camada | faixa «ALGUÉM ESTÁ BATENDO À PORTA DESTE SERVIDOR» / «N PESSOAS ESTÃO BATENDO À PORTA DESTE SERVIDOR»; anunciada uma vez por aparição; nunca toma o foco; volta quando o número sobe | `index.html:170-197`; `camada-portaria.js:470-539`; `camada-portaria.css:246-287` · `estado_da_porta` (a cada 5 s) | adaptado | Faixa fina acima da faixa de voz e das abas, sem foco e sem som, para quem a veria. Texto: «ALGUÉM ESTÁ BATENDO À PORTA DESTE SERVIDOR.\nA decisão é de quem hospeda, pelo app do SEELE.» No plural: «N PESSOAS ESTÃO BATENDO À PORTA DESTE SERVIDOR.\nA decisão é de quem hospeda, pelo app do SEELE.» Falta de onde ler o número (achado 1). | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| B-03 | ação | **VER QUEM É** (abre a `#portaria`) | `index.html:195`; `camada-portaria.js:573-578` | fora na 0.16 | O botão não existe. A segunda frase da faixa diz onde se decide. | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| B-04 | ação | **DEPOIS** (cala estas batidas e volta quando o número sobe) | `index.html:196`; `camada-portaria.js:580-583` | adaptado | **ENTENDI**, com a mesma regra: cala estas batidas e a faixa volta quando o número sobe. | igual |
| B-05 | estado | sem notificação do sistema com a janela fechada | `camada-portaria.js:456-458` | igual | Também não existe no celular. | igual |
| **O link ao hospedar (`#porta`)** | | | | | | |
| L-00 · L-01 · L-02 · L-03 · L-04 · L-05 | camada | «A PORTA DESTE SERVIDOR» ao hospedar; **copiar** (clipboard, `execCommand`, «copie com Cmd+C»); **ABRIR CONFIGURAÇÕES**; **ENTENDI** / FECHAR / Escape; **copiar** em CONFIGURAÇÕES · A PORTA; o link é esquecido ao sair | `index.html:3758-3800`, `:2466-2471`; `camada-portaria.js:642-809`; `tela-boot.js:524-525` | fora na 0.16 | É de quem hospeda e fica coberto por HOSPEDAR em SÓ NO APP DO SEELE (outra área). | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| **O aceite de MODs (`#mods-aceite`)** | | | | | | |
| A-00 | camada | «ESTE SERVIDOR USA MODS» + «1 MOD» / «N MODS» | `index.html:4202-4233`; `camada-mods.css:12-37` | adaptado | **Tela cheia** dentro do fluxo de entrada: depois de ENTRAR em «ENTRAR EM <servidor>?», antes da sessão. | igual |
| A-01 · A-02 | estado | disparo: `connect` falha com `ModsNaoAceitos {mods, conjunto}` (e o núcleo já mandou `RecusarMods`); com o mesmo conjunto já aceito, a tela não abre | `tela-boot.js:283-311`; `camada-mods.js:276-289`; `crates/seele-core/src/client.rs:2279-2321` · `connect` | igual | Mesmo critério. A ponte da F4 lê o sim guardado na origem e responde ao anúncio. | igual |
| A-03 · A-04 | estado | primeira vez (sem aviso); conjunto mudou: «Este servidor mudou a lista de MODs desde a última vez que você disse sim. O aceite anterior não autoriza esta lista.» | `camada-mods.js:112-128`; `index.html:4215-4223` · `aceite_de_mods` | igual | No alto da tela cheia, acima da lista. | igual |
| A-05 | estado | «**MODs são parte deste servidor.** Eles executam código nesta máquina e podem mudar a janela inteira. Leia o que cada um alcança antes de entrar.»; «Servidor: <alvo>» | `index.html:4210-4213` | adaptado | Muda só o lugar de onde se fala: «MODs são parte deste servidor. Eles executam código neste aparelho e podem mudar a tela inteira. Leia o que cada um alcança antes de entrar.» | igual |
| A-06 · A-07 | estado | ficha do MOD: ordem, id, «versão X», REPOSITÓRIO (ou «não declarado»), CONTEÚDO (16 caracteres + «…», inteiro só no `title`), etiquetas de alcance (ou «nada declarado»), «Roda na máquina de quem hospeda: rede de saída, relógio e registro.»; sem selo de procedência | `camada-mods.js:22-26`, `:43-99` | adaptado | Uma ficha por MOD, em coluna. O hash inteiro (64 caracteres, em grupos) aparece **ao tocar** em CONTEÚDO, porque não há `title`. Continua sem selo. | igual |
| A-08 | estado | «Aceitar vale somente para esta combinação de servidor, MODs e versões. Qualquer mudança pergunta de novo, e o sim pode ser desfeito em CONFIGURAÇÕES.» | `index.html:4224-4226` | adaptado | «…e o sim pode ser desfeito em AJUSTES › ESTE SERVIDOR › MODS.» | igual |
| A-09 · A-10 · A-20 | ação | **ACEITAR 1 MOD E ENTRAR** / **ACEITAR N MODS E ENTRAR**; se gravar falha, mostra o erro, o botão volta e não entra; o sim mora em `config_dir`, pela chave do servidor | `camada-mods.js:136-138`, `:172-190`; `crates/seele-ffi/src/mods.rs:306-376` · `aceitar_mods` | adaptado | Mesmo botão, no pé da tela cheia. O sim fica **no armazenamento da origem**, pela ponte da F4, com a mesma chave (a do pin). A falha ao gravar aparece igual. | igual |
| A-11 · A-12 | estado | obtendo: «obtendo <id> (i de n)…», «verificando…»; falhas «não consegui obter <id> <versão>: … Tente de novo, ou recuse a entrada.» / «o que chegou não é o que o servidor exige: … Não vou entrar com outro conteúdo.»; o botão vira **TENTAR DE NOVO** | `camada-mods.js:191-199`, `:206-268` · `mods_instalados`, `instalar_mod_do_catalogo` | adaptado | Mesmas frases e mesma ordem: obter, conferir o hash e só então entrar. De onde vêm os bytes no celular é da F4 e da área de MODs (ADR 0057 D-web-1). | igual |
| A-13 | ação | sucesso: fecha e chama `conectar()` sem argumentos (mesmo alvo e apelido; **o token não é reenviado**) | `camada-mods.js:201-204`; `tela-boot.js:131-135` · `conferir_esta_versao`, `connect` | adaptado | Entra de novo direto, sem voltar a «ENTRAR EM <servidor>?», porque a pessoa já tocou ENTRAR. O convite não é reenviado (seção 3). | igual |
| A-14 · A-15 | ação | **RECUSAR E VOLTAR** (foco inicial; só fecha); Escape e véu recusam | `camada-mods.js:141-155`, `:295-308`; `index.html:4229` | adaptado | RECUSAR E VOLTAR volta à tela de onde se entrou: «ENTRAR EM <servidor>?» ou a lista de servidores deste aparelho. O botão voltar do Android recusa. Não há véu nem Escape. | igual |
| A-16 | estado | no meio da sessão: «OS MODS DESTE SERVIDOR MUDARAM.\nO que você aceitou não é mais o que ele exige: entre de novo para ler a lista nova.»; RECONECTAR leva ao aceite | `crates/seele-ffi/src/lib.rs:5214-5216`; `frases.js:104-109`; `tela-fim.js:121-134` | igual | Na tela de fim. Entrar de novo abre a tela cheia do aceite. | igual |
| A-17a | estado | «ESTE SERVIDOR EXIGE MODS QUE VOCÊ NÃO ACEITOU.…» (`ModsRecusados`); «…QUE ELE NÃO CONSEGUE DESCREVER. Não é a sua entrada: avise quem hospeda.» (`ModsIndisponiveis`) | `frases.js:100-103`, `:113-115` | igual | Mesmas frases, nas mesmas telas de recusa. | igual |
| A-17b | estado | «ESTE SERVIDOR RODA MODS QUE ESTA VERSÃO DO APP NÃO RODA.\nAtualize o app para entrar.» (`ModsDeOutraApi`) | `frases.js:125-127` | adaptado | «ESTE SERVIDOR RODA MODS QUE ESTA VERSÃO DA PÁGINA NÃO RODA.\nA versão da página está em AJUSTES › ESTE APARELHO.» Como a página se atualiza é da F4. | igual |
| A-18 | ação | «SERVIDORES A QUEM VOCÊ DISSE SIM»: «<alvo> · conjunto <12>…» + **DESFAZER**; vazio «VOCÊ AINDA NÃO DISSE SIM A NENHUM» | `camada-mods.js:1834-1891`; `index.html:3320-3335` · `aceites_de_mods`, `esquecer_aceite_de_mods` | adaptado | Subpágina de AJUSTES › ESTE SERVIDOR › MODS › permissões, com os sins guardados **nesta origem**. DESFAZER vale na próxima entrada, sem confirmação, como no desktop. | igual |
| A-19 | estado | `conjunto_exigido_agora` (base do SALVAR da gestão; responde «» sem hospedar) | `src/main.rs:6858-6879`; `camada-mods.js:1594-1605` | fora na 0.16 | Pertence à gestão de MODs: SÓ NO APP DO SEELE › INSTALAR E LIGAR MODS. | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| **Quem sofre a moderação ou espera a porta** | | | | | | |
| R-01 · R-02 | estado | tela de fim «DESCONECTADO POR UM OPERADOR» (`Kicked`) / «ACESSO BARRADO POR UM OPERADOR» (`Banned`) | `frases.js:41-42`; `tela-fim.js:43-75` | igual | Na tela de fim do celular, com as mesmas frases. | igual |
| R-03 | estado | movido por operador (`MovedByOperator`): a faixa diz só «AVISO» | `crates/seele-ffi/src/types.rs:152-153`; `tela-sessao.js:2592-2640`; `tests/frontend.rs:16519` | igual | Hoje mostra o mesmo «AVISO». Quando o desktop ganhar a frase, o celular ganha junto (achado 3). | igual |
| R-04 · R-05 | estado | «A SALA DE VOZ EM QUE VOCÊ ESTAVA FOI APAGADA» / «O CANAL DE TEXTO QUE VOCÊ LIA FOI APAGADO, COM TUDO QUE HAVIA NELE» / «PERMISSÃO NEGADA» | `frases.js:166`, `:198-199` | igual | No alerta do celular, com as mesmas frases. | igual |
| R-07 | estado | espera da portaria: «AGUARDANDO APROVAÇÃO DE QUEM HOSPEDA.\nSeu pedido está guardado no servidor.»; botão de estado «AGUARDANDO PERMISSÃO» / «TENTANDO DE NOVO»; bate a cada 15 s; **VOLTAR À ENTRADA** | `tela-auth.js:112`, `:504-620`; `frases.js:74-76`; `index.html:2016-2050` · `connect` | adaptado | Tela própria do fluxo de entrada, com as mesmas frases e a mesma batida. VOLTAR À ENTRADA vira a volta à lista de servidores deste aparelho. A batida depende de um relógio da página (achado 6). | igual |
| R-08 | estado | «QUEM HOSPEDA RECUSOU A SUA ENTRADA.\nNão foi a senha nem o convite: fale com quem hospeda por outro canal.»; «ENTRADA NEGADA»; «Tentar de novo dá na mesma. Só quem hospeda pode mudar essa resposta.» | `frases.js:79-81`; `tela-auth.js:576-601` | igual | Na mesma tela de R-07. | igual |
| **AJUSTES › O QUE O CELULAR NÃO FAZ (as entradas desta área)** | | | | | | |
| NO COMPUTADOR › MODERAR | estado | (lista) moderar | — | fora na 0.16 | «MODERAR\nExpulsar, banir e mover pessoas, e remover mensagens de outras pessoas, se faz pelo SEELE no computador, com as mesmas permissões.» | igual |
| NO COMPUTADOR › SALAS E CANAIS | estado | (lista) salas e canais | — | fora na 0.16 | «SALAS E CANAIS\nRenomear e apagar salas de voz e canais de texto se faz pelo SEELE no computador.» Criar é de outra área e pode juntar-se a esta entrada. | igual |
| SÓ NO APP DO SEELE › A PORTARIA DO SEU SERVIDOR | estado | (lista) a porta do servidor | — | fora na 0.16 | «A PORTARIA DO SEU SERVIDOR\nAprovar quem bate, a senha e os convites ficam no app do SEELE que hospeda o servidor.» | fora na 0.16: «Hospedar um servidor é pelo app do SEELE.» com o link para baixar o app; em SÓ NO APP DO SEELE |
| SÓ NO APP DO SEELE › INSTALAR E LIGAR MODS | estado | (lista) MODs exigidos | — | fora na 0.16 | «MODS DO SERVIDOR\nInstalar e apagar MODs, e escolher o conjunto que o servidor exige, é no app do SEELE.» A frase é compartilhada com a área de MODs. | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |

---

### Hipótese não medida: o convite gasto antes da pergunta dos MODs


**O que o código mostra:**
- Na `admitir`, o servidor gasta o convite (`admissao::gastar`,
  `crates/seele-server/src/session.rs:983-1006`) **antes** de anunciar os MODs
  (`exigir_aceite_dos_mods`, `:1144`). O ADR 0057:244-246 registra a mesma ordem.
- O app recebe `ModsNaoAceitos`, grava o sim e entra de novo **sem token**
  (`camada-mods.js:204`; `tela-boot.js:133-135`: o token «não é lembrado»).
- Com a portaria desligada, `ja_admitido` responde `false` (`portaria.rs:378-381`), e a política
  exige segredo de novo (`session.rs:744-757`).

**O que a leitura sugere, sem concluir:** num servidor fechado por convite ou senha, com portaria
**desligada** e MOD exigido, a segunda entrada voltaria `CredentialRejected`. Com a portaria
**ligada** (o padrão do HOSPEDAR AQUI), o convite só é gasto ao entrar de fato, e quem já foi
admitido passa sem segredo. Nesse caso o problema não apareceria.

**O que mediria:**
1. Um teste de conformidade em `crates/seele-conformance/tests/aceite_dos_mods.rs`, no molde do
   que hoje usa `join_secret: None` (`:632`):
   - servidor com um convite emitido, portaria desligada e um MOD exigido;
   - primeiro `connect` com o token, esperando `ModsNaoAceitos`;
   - conferir no banco que o convite foi gasto;
   - gravar o aceite e fazer o segundo `connect` **sem** token;
   - registrar a resposta: `Session` ou `CredentialRejected`.
2. As mesmas três variantes: senha no lugar do convite; portaria ligada (controle); segundo
   `connect` **com** o mesmo token (`ConviteGasto`?).
3. No app real: hospedar, desligar a portaria, gerar um convite, exigir um MOD e entrar de outra
   máquina pelo link. Anotar a frase que aparece depois de ACEITAR.

**Por que importa ao celular:** a entrada do celular é **pelo link de convite** («ENTRAR EM
<servidor>?»), e o aceite de MODs é obrigatório. Se a medida confirmar, o caso atinge o celular
na primeira visita.

---

### Achados para o desenho


1. **A faixa B-00 não tem de onde ler.** O número de quem bate só existe no banco da máquina que
   hospeda (`src/main.rs:8657-8701`). No desktop só essa janela vê a faixa
   (`camada-portaria.js:460-468`). Sem um dado novo vindo do servidor, a faixa nunca acende.
2. **O `remover` da própria mensagem só aparece com hover ou Tab**
   (`camada-moderar.css:329-351`). A folha de ações da mensagem tem de trazer REMOVER para as
   mensagens da própria pessoa.
3. **`MovedByOperator` não tem frase.** O celular herda «AVISO», e a caixa do desktop promete
   que a pessoa movida «recebe um aviso» (`types.rs:152-153`; `camada-moderar.js:595`;
   `tests/frontend.rs:16519`).
4. **A folha de confirmação precisa ficar acima das páginas e diálogos de MOD** e avisar ao
   fechar nos dois desfechos (`camada-moderar.css:40-54`; `mods-superficies.js:583-599`).
5. **O aceite mostra o hash inteiro só no `title`**, e a portaria mostra a impressão inteira do
   mesmo jeito. O celular precisa de toque para expandir (`camada-mods.js:69-75`).
6. **A espera da portaria bate por relógio da página, a cada 15 s** (`tela-auth.js:112`).
   Com a página oculta o navegador pode parar esse relógio, e a tela precisa dizer que retoma
   ao voltar.
7. **`RecusarMods` sai antes de a pessoa ler a lista**, e o servidor registra uma recusa em toda
   primeira visita a um servidor com MOD (`crates/seele-core/src/client.rs:2296-2321`).
8. **As frases do aceite e as notas da caixa de moderação estão no `index.html`**
   (`index.html:4206-4230`, `:4288`, `:4307-4345`), e o desenho exige o `frases.js`. As variantes
   do celular («neste aparelho», «AJUSTES») dependem dessa mudança.
9. **`ModsDeOutraApi` manda «Atualize o app»** (`frases.js:125-127`). No celular a versão é a da
   página, e falta saber como ela se atualiza (F4).
10. **As frases de salvar e de ir à entrada falam de pasta, quarentena e «campo do endereço»**
    (`frases.js:1142-1155`; `tela-sessao.js:3611-3617`). Na folha do celular viram download do
    navegador e lista de servidores deste aparelho.

## 7. Os MODs

Fonte: `G-mods.md` (levantamento, HEAD `f7bc902b`; publicado: v0.15.1 `54f542f94`). Desenho e regras: `00-desenho-e-regras.md`, que já está decidido e não é rediscutido aqui.
Estados: **igual** · **adaptado** · **fora na 0.16**. Marcas: [c] lido no código; [medido] rodado; [calc] calculado; [inf] inferido.
O aceite (`#mods-aceite`) está com outra pessoa e não se repete aqui.

### As frases desta área (todas vão para o `frases.js`; as chaves são propostas)


| chave proposta | frase | onde aparece |
|---|---|---|
| `ModsSoNoApp` | «MODs se instalam, se ligam e se aplicam pelo app do SEELE que hospeda este servidor.» | **No lugar** da gestão, no alto de AJUSTES › ESTE SERVIDOR › MODS, só para quem hospeda este servidor no desktop (ver achado 8: falta o sinal). E em AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE |
| `SoNoAppInstalarMod` | «INSTALAR MOD — de uma pasta ou do catálogo, pelo app do SEELE que hospeda o servidor.» | AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE |
| `SoNoAppAplicarMods` | «LIGAR, DESLIGAR E APLICAR MODS — pelo app do SEELE que hospeda o servidor. Aplicar um conjunto novo derruba quem está dentro uma vez, e cada pessoa aceita de novo.» | AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE |
| `SoNoAppPacotesDeMod` | «PACOTES DE MOD — o espaço que ocupam, as revisões guardadas e o APAGAR ficam no app do SEELE.» | AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE |
| `ModsDesteServidorVazio` | «ESTE SERVIDOR NÃO EXIGE MOD NENHUM.» | MODS DESTE SERVIDOR, quando vazio |
| `FaseSemPacoteCelular` | «o servidor exige este MOD e ele não chegou a este aparelho — entre de novo para tentar outra vez» | a fase `sem-pacote` no celular |
| `FaseOutraVersaoCelular` | «o que chegou a este aparelho não é o conteúdo que o servidor exige — entre de novo para tentar outra vez» | a fase `outra-versao` no celular |
| `ContornosCelular` | «Cada contorno leva o nome do lugar como a API o chama. Os contornos não recebem toque: a sessão continua funcionando por baixo. Vale só neste aparelho.» | DIAGNÓSTICO › MODO DE DESENVOLVEDOR |
| `ContornosAvisoCelular` | «MODO DE DESENVOLVEDOR · desligue em AJUSTES › ESTE SERVIDOR › MODS › DIAGNÓSTICO» | aviso sobre a tela, com o modo ligado |
| `ModsDeOutraApiCelular` | «ESTE SERVIDOR RODA MODS QUE ESTA VERSÃO DA PÁGINA NÃO RODA.\nAbra a página de novo para pegar a versão nova.» | recusa de entrada (desktop: `ModsDeOutraApi`, `frases.js:125-127`). Depende de como a página se atualiza (ESTE APARELHO, outra área) |

### A gestão de MODs


Desktop: CONFIGURAÇÕES › MODS (`#painel-mods`, dentro de `#tela-server`, z 24), visível só com sessão aberta pela engrenagem (`tela-server.js:989-993`). Celular: **AJUSTES › ESTE SERVIDOR › MODS**, subpágina com «‹ AJUSTES».

| id | tipo | item (texto da tela) | desktop (âncora · invoke) | estado | no celular | PC (navegador) |
|---|---|---|---|---|---|---|
| G-01 | seção | «MODS» — «O que está instalado nesta máquina e a quem você já disse sim» | `index.html:2297-2320`; `tela-server.js:122-138, 989-993` | **adaptado** | AJUSTES › ESTE SERVIDOR › MODS. Como no desktop, só existe com sessão | igual |
| G-02 | comportamento | redesenho ao vivo quando a fase de um MOD muda | `camada-mods.js:2180-2183` · evento `seele-mods-estado` | **igual** | a subpágina se redesenha enquanto está aberta | igual |
| G-03 | abas | INSTALADOS · CATÁLOGO · ARMAZENAMENTO · PERMISSÕES · DIAGNÓSTICO (←/→, Home, End) | `index.html:3156-3177`; `camada-mods.js:2204-2246` | **adaptado** | ficam três itens da subpágina (MODS DESTE SERVIDOR, PERMISSÕES, DIAGNÓSTICO), cada um em página própria. As setas viram toque no item. CATÁLOGO e ARMAZENAMENTO estão abaixo, como fora | igual |
| G-04 | lista | «MODS DESTA MÁQUINA» (os pacotes em disco) | `camada-mods.js:1520-1648` · `mods_instalados`, `estou_hospedando`, `conjunto_exigido_agora` | **adaptado** | «MODS DESTE SERVIDOR»: o que o servidor exige e o que roda neste aparelho, a partir de `modsExigidos` e `estadoDosMods` (`base.js:2500, 2519`). Não há disco | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-05 | nota | «Estas escolhas valem para: {nome}» | `camada-mods.js:1560-1585` · `servidor_hospedado`, `servidores_guardados` | **fora na 0.16** | só existe para quem hospeda; coberto por `ModsSoNoApp` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-06 | vazio | «NENHUM MOD INSTALADO NESTA MÁQUINA» | `camada-mods.js:1619-1626` | **adaptado** | `ModsDesteServidorVazio` | igual |
| G-07 | estado | MOD exigido que não está aqui + «EXIGIDO POR ESTE SERVIDOR» | `camada-mods.js:489-512` | **igual** | o mesmo, com as fases adaptadas (G-11) | igual |
| G-08 | estado | «não serve: {motivo}» (pacote recusado na pasta) | `camada-mods.js:332-339` · `mods_instalados` | **adaptado** | não há pasta para ler; um pacote recusado aparece como fase `nao-carregou`, com o motivo | igual |
| G-09 | dado | «versão {v} · revisão {hash 8}» | `camada-mods.js:341-343` | **igual** | versão e conteúdo, tirados do anúncio do servidor | igual |
| G-10 | estado | «ATIVO NESTE SERVIDOR» / «SELECIONADO · falta SALVAR» / «SERÁ DESLIGADO · falta SALVAR» / «INSTALADO E DESLIGADO» | `camada-mods.js:381-391` | **adaptado** | só «EXIGIDO POR ESTE SERVIDOR» e a fase. As quatro palavras falam do rascunho de quem hospeda. No desktop, quem só entra vê «INSTALADO E DESLIGADO» junto de «EXIGIDO» [inf] | igual |
| G-11 | fase | as frases de `FASES_DO_MOD` | `camada-mods.js:525-538`; escritas em `base.js:2578` | **igual** · `sem-pacote` e `outra-versao` **adaptadas** | `carregando`, `carregado`, `nao-carregou`, `falhou-rodando`, `obtendo`, `nao-obtive` e `descarregado` ficam iguais. `sem-pacote` → `FaseSemPacoteCelular`; `outra-versao` → `FaseOutraVersaoCelular` (o desktop diz «instalado aqui», e o celular não instala) | igual |
| G-12 | dado | «roda na máquina de quem hospeda» | `camada-mods.js:412-416` | **igual** | — | igual |
| G-13 | selo | «EXIGIDO POR ESTE SERVIDOR» | `camada-mods.js:428-432` | **igual** | — | igual |
| G-14 | ação | «LIGAR» / «DESLIGAR» (mexe no rascunho) | `camada-mods.js:440-472` · nenhum | **fora na 0.16** | `ModsSoNoApp` no lugar, para quem hospeda; `SoNoAppAplicarMods` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-15 | nota | «Somente quem hospeda pode alterar os MODs.» | `index.html:3196`; `camada-mods.js:1553` | **adaptado** | sem interruptor não há o que explicar; quem diz é AJUSTES › O QUE O CELULAR NÃO FAZ › SÓ NO APP DO SEELE | igual |
| G-16 | ação + confirmação | «APAGAR MOD» · «APAGAR MOD?» · «Apagar {id} desta máquina? Os dados dos servidores serão preservados.» | `camada-mods.js:363-380` · `apagar_pacote_do_cache` | **fora na 0.16** | `ModsSoNoApp` no lugar; `SoNoAppPacotesDeMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-17 | agrupamento | «Outras revisões de {nome} ({n})» | `camada-mods.js:1494-1518` | **fora na 0.16** | o aparelho não guarda revisões; `SoNoAppPacotesDeMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-18 | estado | «passa a exigir: … · deixa de exigir: …. Nada disso vale até SALVAR.» | `camada-mods.js:1396-1418` | **fora na 0.16** | `ModsSoNoApp`; `SoNoAppAplicarMods` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-19 | ação + confirmação | «SALVAR ALTERAÇÕES» · «APLICAR O CONJUNTO NOVO» · «SALVAR» | `camada-mods.js:1434-1490` · `aplicar_conjunto_de_mods`, `conjunto_exigido_agora` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-20 | erro | «outra pessoa mudou o conjunto deste servidor enquanto esta tela estava aberta…» | `camada-mods.js:1470-1485` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-21 | ação | «DESCARTAR» | `camada-mods.js:1421-1426` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-22 | confirmação | «SAIR SEM SALVAR?» | `tela-server.js:889-916` | **fora na 0.16** | sem rascunho no celular, não há o que perguntar | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-23 | erro | «não deu para saber quais MODs este servidor exige, então nada foi carregado. Reconecte para tentar de novo.» | `camada-mods.js:1535-1542` | **igual** | no alto de MODS DESTE SERVIDOR | igual |
| G-24 | ação | «INSTALAR MOD DE UMA PASTA» (seletor nativo de pasta) | `camada-mods.js:1894-1923` · `instalar_mod` (`main.rs:7055`, `pick_folder`) | **fora na 0.16** | `ModsSoNoApp` no lugar; `SoNoAppInstalarMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-25 | bloco | «QUEM DESENHA O QUE» (só aparece quando há disputa ou escolha) | `camada-mods.js:689-733` | **igual** | em MODS DESTE SERVIDOR, com a mesma condição | igual |
| G-26 | estado | «apresentado por {id}…», «o SEELE desenha; nenhum MOD substitui este lugar», «Também pediram este lugar e não o receberam: …» | `camada-mods.js:750-796` | **igual** | — | igual |
| G-27 | ação | «USAR {id}» | `camada-mods.js:798-817, 659-680` · nenhum (`localStorage` `seele.mods.apresentacao`, chave `{destino}\u0000{ponto}`) | **igual** | preferência deste aparelho, por servidor, no armazenamento do navegador. O botão do desktop tem 32 px | igual |
| G-28 | ação | «USAR APRESENTAÇÃO DO SEELE» | `camada-mods.js:823-825` | **igual** | — | igual |
| G-29 | ação | «DECIDIR AUTOMATICAMENTE» | `camada-mods.js:828-830` | **igual** | — | igual |
| G-30 | efeito | escolher «o SEELE» em `servidor.aparencia` recusa o próximo tema, mas não apaga o da API 3 | `camada-mods.js:1025-1029`; `base.js:1979-2001` | **adaptado** | o celular só entra em servidor de API 6 (o link de Servidor 0.15 tem tela própria). Na 6, `servidor.aparencia` é a porta de quem vê sobre o tema declarado (rascunho §3) | igual |
| G-31 | ação | «BUSCAR NO CATÁLOGO» | `camada-mods.js:2078-2120` · `catalogo_de_mods` | **fora na 0.16** | `SoNoAppInstalarMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-32 | lista | títulos, versões, «OFICIAL / VERIFICADO / PUBLICADO COM NOTAS», «JÁ INSTALADO» | `camada-mods.js:1955-2075` · `apis_de_mod_aceitas` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-33 | ação | «INSTALAR» / «REINSTALAR» / «INSTALAR {versão}» | `camada-mods.js:2048-2070, 2123-2161` · `instalar_mod_do_catalogo` | **fora na 0.16** | `ModsSoNoApp` no lugar; `SoNoAppInstalarMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-34 | erros | `AssinaturaDoCatalogoRecusada`, `NaoRespondeu`, `NaoEUmCatalogo`, `MaisNovoQueEsteBuild`, `NaoAplicou`, `NaoEstaNoCatalogo`, `ConteudoDivergente`, `Revogada` | `frases.js:608-659, 886-890` | **fora na 0.16** na gestão | as recusas do catálogo voltam no caminho de obter o conjunto aceito, que é do aceite | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-35 | total | «Quanto espaço os pacotes ocupam aqui.» · «ESPAÇO EM DISCO» · «{n} pacotes, {tamanho} no total.» | `camada-mods.js:1672-1704` · `pacotes_no_cache` | **fora na 0.16** | `SoNoAppPacotesDeMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-36 | estado | pacote com versão, tamanho, conteúdo e motivo da recusa por API | `camada-mods.js:1758-1793` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-37 | selo | «EM USO POR ESTE SERVIDOR» | `camada-mods.js:1799-1805` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-38 | ação | «APAGAR» (sem confirmação) | `camada-mods.js:1807-1832` · `apagar_pacote_do_cache` | **fora na 0.16** | `ModsSoNoApp` no lugar; `SoNoAppPacotesDeMod` | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-39 | diagnóstico | contadores «sessão nº N · {e} eventos e {c} comandos recusados… · nativos: …» | `camada-mods.js:1720-1755` · `estado_da_sessao` | **adaptado** | vai para DIAGNÓSTICO. As instâncias passam a ser as do executor do navegador, e não `mods_nativos` | igual |
| G-40 | nota | «Apagar um pacote tira só os bytes que a janela carrega…» | `index.html:3314-3316` | **fora na 0.16** | sai junto com ARMAZENAMENTO | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-41 | erro | `#cache-erro` | `index.html:3300` | **fora na 0.16** | idem | fora na 0.16: «Instalar, ligar, desligar e aplicar MODs no servidor é de quem hospeda, pelo app do SEELE.» |
| G-42 | lista | «SERVIDORES A QUEM VOCÊ DISSE SIM» · «conjunto {12}…» · «VOCÊ AINDA NÃO DISSE SIM A NENHUM» | `camada-mods.js:1835-1879` · `aceites_de_mods` | **igual** | PERMISSÕES, com todos os servidores deste aparelho, como no desktop. Mora em ESTE SERVIDOR porque, como no desktop, só se chega a ela com sessão | igual |
| G-43 | ação | «DESFAZER» (sem confirmação) + «Desfazer um sim faz a lista de MODs daquele servidor ser perguntada de novo na próxima entrada…» | `camada-mods.js:1882-1891` · `esquecer_aceite_de_mods` | **igual** | controle de 44 px | igual |
| G-44 | lista | «QUEM PINTA CADA LUGAR» — os lugares, «na API: {ponto}», um fato por frase | `camada-mods.js:877-1064` | **igual** | os lugares são os do celular (regiões e pontos da API 6). Uma contribuição para uma região ausente é contada com `regiao-ausente` (rascunho §11) | igual |
| G-45 | bloco | «A região e as páginas de cada MOD» | `camada-mods.js:960-969, 1074-1087` | **adaptado** | sem a faixa `regiao` na API 6: só as superfícies de cada MOD | igual |
| G-46 | comportamento | redesenho do DIAGNÓSTICO só com a página à vista | `camada-mods.js:1090-1094, 2260-2265` | **igual** | — | igual |
| G-47 | interruptor | «MODO DE DESENVOLVEDOR» · «contornar na tela cada lugar onde um MOD pode entrar» | `camada-mods.js:1130-1170` (`localStorage` `seele.mods.desenvolvedor`) | **adaptado** | o mesmo interruptor; texto `ContornosCelular` | igual |
| G-48 | camada | contornos + «MODO DE DESENVOLVEDOR · desligue em CONFIGURAÇÕES › MODS › DIAGNÓSTICO» | `camada-mods.js:1115-1127, 1181-1348`; `camada-mods.css:424-476` | **adaptado** | contornos sobre os contêineres do celular. `CONTEINERES_DOS_PONTOS` é marcação do desktop e precisa da tabela do celular. Aviso: `ContornosAvisoCelular`. **Medir:** custo do `MutationObserver` no iPhone; ordem em relação à faixa de voz e às abas | igual |
| G-49 | recusas | `ModsRecusados`, `ModsMudaram`, `ModsIndisponiveis`, `ModsDeOutraApi` | `frases.js:91-127` | **igual** · `ModsDeOutraApi` **adaptada** | `ModsDeOutraApiCelular` | igual |
| G-50 | fronteira | `levarParaOAceiteDeMods` → `#mods-aceite` | `camada-mods.js:281-288`; `tela-boot.js:303` | — | do aceite (outra pessoa) | igual |

### Regiões, pontos e superfícies no celular


API 6 = existe só no rascunho (`2026-10-08-api-6-rascunho.md`), e não no código de hoje.

| região · ponto · superfície | hoje no desktop | no celular (decidido) | o que muda | o que medir |
|---|---|---|---|---|
| `redes` (API 6) | não existe. Área: a trilha de servidores (`index.html:963`) | a faixa deitada de servidores, no alto da aba SERVIDOR | de coluna vertical para faixa deitada que rola de lado | largura dos itens a 390 px |
| `servidor.navegacao` | lista «DOS MODS» na coluna de canais (`index.html:1072-1075`; `tela-sessao.js:2438-2482`), com o rótulo do MOD e a origem; filtra `listarNaBarra` | aba SERVIDOR | a mesma lista; o botão vira um alvo de 44 px | — |
| `canal.item` | dentro do botão de cada canal, depois do nome (`tela-sessao.js:826`); só conteúdo, perfil de cartão | aba SERVIDOR | — | nós por canal × número de canais na rolagem |
| `sala.acoes` | no cabeçalho de cada grupo de sala **da faixa de pessoas** (`.roster-sala`, `tela-sessao.js:1987-1998`): conteúdo e botões | aba SERVIDOR | **muda de coluna**: no desktop mora com as pessoas, no celular com as salas. As ações de 24 px (`mods-controles.css:722`) sobem para 44 | — |
| `operador.barra` (API 6) | não existe. Área: `.operador` (`index.html:1087`), que guarda MUDO, SURDO e SAIR | logo abaixo de SEU PERFIL em AJUSTES, com altura limitada e rolagem por dentro; nenhuma contribuição cobre a gestão de MODs | sai de perto do microfone: MUDO, SURDO e SAIR estão na faixa de voz e não convivem com ela | **a altura (M3)**. O rascunho §11 (`:888`) ainda diz «recusada com `regiao-ausente`», contra este desenho |
| `canal.cabecalho` | `#canal-cabecalho-mods`, entre o nome do canal e a lupa (`index.html:1447`; `tela-sessao.js:1046-1058`); vazio sem canal | o topo da aba CONVERSA | menos largura: o nome do canal, a busca e a contribuição dividem 390 px | o que transborda a 390 px; botões de 44 |
| `conversa.mensagem` (API 6) | não existe. Área: `#lista-mensagens` (`index.html:1494`) | cada mensagem | — | custo por mensagem numa conversa longa |
| `pessoa.avatar` | `[data-pessoa-do-avatar]`: `#operador-inicial`, `.mensagem-avatar` e `.chamada-avatar`, como `background-image` (`tela-sessao.js:919, 1303, 4574-4598`; `tela-chamada.js:463`). A faixa de pessoas do desktop não tem retrato | cada mensagem; a aba PESSOAS e a folha da pessoa | ganha a lista de pessoas, que no desktop não tem retrato | memória dos retratos decodificados no iPhone. O teto hoje é de 20 MiB por instância (`mods-regiao.js:154`) e conta bytes do arquivo, não decodificados |
| `compositor` (API 6) e `compositor.ferramentas` | `#compositor-ferramentas` antes do ENVIAR, só botões (`index.html:1632`; `tela-sessao.js:1066-1076`) | a bandeja do botão de anexar | as ferramentas deixam de estar à vista e passam a ficar a um toque | — |
| `pessoas.lista` (API 6), `pessoa.identidade`, `pessoa.cartao`, `pessoa.acoes` | identidade ao lado do nome (`tela-sessao.js:2293`). O cartão substituído vira um botão do produto, e a fileira nativa desce para `<details class="pessoa-nativo">` «⋯» (`:2084-2121, 2175-2205`). Ações no rodapé, só botões (`:2340-2349`) | a aba PESSOAS e a folha da pessoa | o «⋯» vira a folha da pessoa; as ações vão para a folha com 44 px; o apelido verificado fica à vista (D-nucleo-1) | — |
| `pessoa.detalhes` | **só existe dentro de um cartão substituído** (`tela-sessao.js:2201`); montado com o perfil de cartão, embora declarado `superficie` (`base.js:1386-1391`; `mods-contribuicoes.js:80`) | a folha da pessoa | na folha ele tem lugar sempre; no desktop depende de haver substituição de `pessoa.cartao` | — |
| `pessoa.capa` (API 6) | não existe | o cartão da CHAMADA e o cabeçalho SEU PERFIL; **nunca a faixa de voz** | — | quadros decodificados e o teto de capas animadas (rascunho §4) |
| `status.barra` (API 6) | não existe. Área: `footer.telemetria` (`index.html:1784`) | a fileira de telemetria da CHAMADA | só aparece na CHAMADA | — |
| `servidor.aparencia` | registro que decide quem aplica o tema; o tema é escrito no `style` de `#tela-sessao` (`base.js:1979-2001, 2084-2126`) | AJUSTES › ESTE SERVIDOR | a escolha mora com o ESTILO (D-estilo-1, «VER NA SESSÃO») | — |
| `chamada.palco` (API 6) | não existe. Área: `#chamada-grade` (`index.html:1430`) | vista alternativa à grade, dentro da CHAMADA | — | o som do palco quando a vista alterna (rascunho §11); o arraste por toque na `tela` |
| `pagina` | `#palco-de-paginas`, na célula da conversa, por cima dela (`mods-superficies.css:127-139`). Saída «VOLTAR». Fica oculta (sem calar o som) quando se abre um canal ou se entra numa sala (`tela-chamada.js:624, 642`) | o lugar da CONVERSA | ocupa a aba CONVERSA. A saída de 28 px (`mods-superficies.css:347`) sobe para 44 | o que a esconde ao trocar de aba |
| `pagina` imersiva | `position:fixed; inset: 32px 0 0; z-index:30`, com trava de foco e Escape (`mods-superficies.css:142-146`). **Cobre o operador** (MUDO, SURDO, SAIR) e a telemetria | cobre o conteúdo, **nunca a faixa de voz nem as abas** | o `inset` deixa de cobrir a faixa e as abas, e soma a área segura; o Escape vira o VOLTAR visível e o voltar do Android | a altura da faixa de voz + abas e `safe-area-inset-bottom` |
| `painel` (API 4/5) → `folha` (API 6) | 5ª coluna, de 200 a 640 px; ≤ 900 px empilha sobre a conversa (`tela-sessao.css:2420-2436`). Saída «FECHAR»; sem foco nem Escape | folha de baixo | corte seco, FECHAR sempre visível, o voltar do Android fecha; o teclado empurra a folha (rascunho §5) | — |
| `dialogo` (modal) | z 60, centrado, até 100vw−32 e 100vh−48 px, rodapé fixo para `acoes` fixas; trava de foco, Escape, sem fechar por toque fora; área segura **só no topo** (`mods-superficies.css:263-337`) | tela cheia, com as ações fixas embaixo | ocupa a tela, com as ações acima da barra de gestos | área segura embaixo |
| `dialogo` com `modal:false` | sem véu, `pointer-events:none`, no canto inferior direito, sem trava (`mods-superficies.css:274-282`) | tela cheia, com as ações fixas embaixo | passa a cobrir como o modal | — |
| `aviso` | `#avisos-de-mod`, `fixed` em `right:16px; bottom:16px`, z 70, até 420 px; **sem saída**; expira em 3,5 s ou 10 s; **4 por instância de MOD** (`mods-superficies.js:206, 301-305, 740-747`) | acima da faixa de voz, em fila | sobe para acima da faixa e das abas | quantos cabem; a fila por instância dá 4 × N com N MODs |
| `#regioes-dos-mods` (faixa da API 3) e `SeeleUI.cartoes` | faixa de até 240 px sob as colunas (`tela-sessao.css:2304-2313`); cartões da API 3 nas pessoas | **não existe**: sai com as capacidades `regiao` e `cartoes` da API 6 | — | — |

### O tema no celular


O tema é escrito no `style` do contêiner da sessão (hoje, `#tela-sessao`: `base.js:2084-2126`). No desktop ele não alcança CONFIGURAÇÕES, o aceite, a moderação nem SEU PERFIL, que são filhos de `body`. Pelo mesmo desenho, no celular AJUSTES fica fora do tema, e é por isso que o ESTILO tem o «VER NA SESSÃO».

| variável do tema | token que escreve | onde alcança no desktop | onde alcança no celular |
|---|---|---|---|
| `fundo` | `--seele-negro-absoluto` | o fundo da conversa, das páginas e do corpo dos diálogos, e o texto escuro de todo estado aceso | o fundo das abas SERVIDOR, CONVERSA e PESSOAS, da CHAMADA e das superfícies. **Não alcança** MUDO, SURDO e SAIR |
| `texto` | `--seele-osso` | só onde a regra declara `color: var(--seele-osso)`. O que herda do `body` fica com a cor do produto (`base.css:47-51`) [c] | idem. **Medir** quais textos herdam |
| `acento` | `--seele-laranja-nerv` | hover, estados acesos, MUDO e SURDO apertados (`tela-sessao.css:901-906`), sala em que se está (`:678`), FALANDO (`tela-chamada.css:511`), botão primário de MOD (`mods-controles.css:301`) | os estados acesos; sem hover. **Não alcança** MUDO, SURDO e SAIR |
| `borda` | `--seele-linha` | os separadores | os separadores |
| `painel` | `--seele-negro-painel` | o fundo dos painéis, inclusive o de `.operador`, sob o SAIR | listas, folhas, cabeçalhos e rodapés de diálogo, avisos. **Não alcança** o fundo opaco do núcleo |
| `apagado` | `--seele-rotulo-painel` | os rótulos miúdos; **não** dentro das superfícies, que o redefinem (`mods-superficies.css:24-25`) | idem |
| `arredondamento` | `--seele-raio` | **só `.portaria-caixa`** (`camada-portaria.css:62`), dentro da sessão. Moderar e compartilhar ficam fora [c] | **nada**: a portaria está fora no celular |
| `brilho` | `--seele-sombra` | **só `.portaria-caixa`** (`camada-portaria.css:63`) [c] | **nada** |
| `densidade` | `--seele-celula-x`, `--seele-celula-y` | só o que o MOD desenha (`mods-controles.css`, `mods-superficies.css`) | idem |
| `fonte` | `--seele-mono` | onde a regra declara `font-family: var(--seele-mono)`; o que herda do `body` fica com a do produto | idem |

**MUDO, SURDO e SAIR no celular** leem `--nucleo-*` declaradas na raiz, com fundo e borda opacos do núcleo. **Confirmado [medido]** com uma sonda Node sobre as tabelas reais de `base.js` (`TEMA_DA_API` `:1844-1856`, `NUMEROS_DA_API` `:1885-1887`, `BANDEIRAS_DA_API` `:1898-1904`, `MEDIDAS_DA_API` `:1915-1935`):

- O tema só **escreve** estes 11 nomes: `--seele-negro-absoluto --seele-osso --seele-laranja-nerv --seele-linha --seele-negro-painel --seele-rotulo-painel --seele-raio --seele-sombra --seele-celula-x --seele-celula-y --seele-mono`. **Nenhum começa com `--nucleo-`.** A escrita percorre só as chaves próprias das quatro tabelas (`base.js:2091, 2102, 2107, 2114`).
- **A lista é fechada:** uma chave fora da tabela é recusada pelo nome em `base.js:2038-2040` («a API de tema não conhece «{nome}»»). A sonda recusou `--nucleo-mudo` e `nucleo`.
- **A ressalva:** a aceitação usa `in`, e não `Object.hasOwn` (`base.js:2007-2008, 2018, 2031, 2038`). Por isso nomes do protótipo passam. `{toString:"length"}` e `{densidade:"toString"}` foram **aceitos sem efeito e sem recusa**; `{constructor:"#ffffff"}` foi recusado com uma frase vazia («só aceita , e veio…»). Nada disso escreve fora dos 11 nomes. [inf] Uma `densidade` assim deixa de pé os valores da anterior, porque não há `removeProperty` com valor verdadeiro (`base.js:2114-2124`).
- Hoje não há nenhum `--nucleo-*` na casca (grep vazio).
- [c] Declarar `--nucleo-*` na raiz como `var(--seele-…)` é seguro: o `var()` resolve na raiz e desce resolvido. Um tema escrito no contêiner da sessão não o altera. Prova: o guarda de contraste do rascunho §9 (`:798-802`).

### Achados para o desenho


1. A saída das superfícies tem 28 px (`mods-superficies.css:347`), as ações de MOD 24 px (`mods-controles.css:722`) e o botão fantasma 32 px (`base.css:281`).
2. A área segura só aparece no diálogo, e só no topo: é o único `env(safe-area-*)` da casca (`mods-superficies.css:270`).
3. A página imersiva é `fixed; inset: 32px 0 0; z-index:30` e cobre o operador, com MUDO, SURDO e SAIR, e a telemetria (`mods-superficies.css:142-146`).
4. O SAIR (`#operador-sair`) herda a cor do produto, mas o fundo do tema: um `painel` claro o apaga, e só texto×fundo é conferido (`base.css:47-51, 277-287`; `tela-sessao.css:822-830`; `base.js:2136-2153`).
5. MUDO e SURDO apertados pintam negro sobre o acento sem conferir: acento `#050403` dá 1,00:1 [calc] (`tela-sessao.css:901-906`).
6. `pessoa.detalhes` só monta dentro de um cartão substituído, e com o perfil de cartão em vez do `superficie` declarado (`tela-sessao.js:2201`; `base.js:1386-1391`; `mods-contribuicoes.js:80`).
7. A lista do tema é fechada na escrita, mas aceita nomes do protótipo sem efeito e sem recusa [medido] (`base.js:2007-2008`).
8. A gestão é guardada por `estou_hospedando`, que é da janela. O `Snapshot` não diz quem hospeda; o mais perto é `may_customise_server`. A frase «no lugar, para quem hospeda» precisa desse sinal (`main.rs:7120`; `crates/seele-ffi/src/types.rs:1495`).
9. O aviso não tem saída e a fila é por instância: 4 × N avisos com N MODs (`mods-superficies.js:206, 301-305, 740-747`).
10. O rascunho da API 6 ainda descreve a casca antiga: `operador.barra` «recusada com `regiao-ausente`» (`:888`), `.voz-celular`, `#voz-mudo`/`#voz-sair`, `#origem` e as abas SALAS · CONVERSA · PESSOAS · AJUSTES (`:802, 858-863`).

## 8. Os 159 `invoke`, um por linha

Gerada das sete seções, com os conflitos entre elas resolvidos na junção. **A coluna «sem gesto» é medida**: é o que a casca chamou no Chromium, pelo duble, na carga da página e nos primeiros segundos da sessão, sem toque nenhum (`tools/carga-da-casca.py`, 09/10). O que a casca chama sozinha não pode voltar com recusa: a ponte responde, ainda que vazio.

| invoke | arquivos da `ui/` | estado | ponte | sem gesto | nota | ponte no PC (navegador) |
|---|---|---|---|---|---|---|
| `abertura` | tela-boot.js, tela-server.js | adaptado | atende | carga | É chamado em toda carga (tela-boot.js:630-658). Responde sempre vazio: **nunca `entrar`**, porque tela-boot.js:642-645 conectaria sem toque, e nunca `hospedar` | como no celular |
| `abrir_a_porta_no_firewall` | tela-boot.js | fora | recusa nomeada | — | O navegador não toca no firewall | recusa nomeada (só no app) |
| `abrir_ajustes_de_acessibilidade` | tela-server.js | adaptado | recusa nomeada | — | nunca chamado com o botão escondido | como no celular |
| `abrir_ajustes_do_microfone` | tela-server.js | adaptado | recusa com frase | — | a página não abre ajustes; a frase da nota diz o caminho | como no celular |
| `abrir_no_navegador` | base.js, tela-sessao.js | adaptado | atende | — | Abre numa aba nova (`noopener`, `noreferrer`) depois da mesma conferência de `endereco_que_pode_sair` (main.rs:2096). A tela chama dentro do toque em ABRIR. | como no celular |
| `abrir_teste_de_microfone` | tela-server.js | fora | recusa com frase | — | D-52 | atende pelo `getUserMedia` (decisão da orquestradora, 09/10: a D-52 é do áudio da sessão, e o teste a pessoa abre de propósito) |
| `abrir_versao` | camada-versoes.js, tela-boot.js | fora | recusa nomeada | — | Não há lançador nem outra janela | recusa nomeada (só no app) |
| `aceitar_mods` | camada-mods.js | adaptado | atende | — | Grava ali. A falha aparece com frase (A-10). | como no celular |
| `aceite_de_mods` | base.js, camada-mods.js | adaptado | atende | — | Lê o sim no armazenamento da origem (F4), com a mesma chave do pin. | como no celular |
| `aceites_de_mods` | camada-mods.js | adaptado | atende | — | A lista de aceites deste aparelho, no armazenamento da origem. | como no celular |
| `ajustar_limites_da_tela` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `ajustar_reducao_de_ruido` | tela-server.js | adaptado | recusa nomeada | — | não há interruptor na proposta | como no celular |
| `ajustar_sensibilidade_da_voz` | tela-server.js | adaptado | atende | — | portão do VOZ em `voz-web.js` (achado 10) | como no celular |
| `ajustar_volume_da_tela` | tela-chamada.js | adaptado | atende | — | Mesma dependência; o volume vai de 0.0 a 2.0. | como no celular |
| `analisar_convite` | camada-servidores.js, tela-boot.js | adaptado | atende | — | O mesmo `uri::analisar` compilado para `wasm`, aceitando também a forma https desta origem. Fp e hashes são obrigatórios e o truncado é recusado. Devolve validade e versão. O `para_entrar` vira «há página para esta versão» ou N-04 | como no celular |
| `apagar_linha` | camada-moderar.js | fora | recusa nomeada | — | Administração. | atende (o PC administra) |
| `apagar_pacote_do_cache` | camada-mods.js | fora | recusa com frase | — | idem | recusa nomeada (só no app) |
| `apagar_servidor` | tela-boot.js | fora | recusa nomeada | — | Hospedar | recusa nomeada (só no app) |
| `apagar_voice_room` | camada-moderar.js | fora | recusa nomeada | — | Administração. | atende (o PC administra) |
| `apelido_local` | camada-nomear.js, tela-boot.js | igual | atende | carga | Armazenamento da origem | como no celular |
| `apis_de_mod_aceitas` | camada-mods.js | fora (só o catálogo o usa) | atende | carga | `[6]` | como no celular |
| `aplicar_conjunto_de_mods` | camada-mods.js | fora | recusa com frase | — | `SoNoAppAplicarMods` | recusa nomeada (só no app) |
| `aplicar_icone_do_server` | tela-boot.js | fora | recusa nomeada | — | Hospedar e administrar | atende (o PC administra) |
| `assistir` | palco-imagem.js | igual | atende | — | RECEBER. A escolha fica do lado da ponte, como fica no Rust (`EscolhaDeTela`, lib.rs:2259–2280). | como no celular |
| `atualizacao_que_nao_pegou` | tela-server.js | adaptado | atende | carga | sempre `null`; chamado ao carregar (ui/tela-server.js:1403) | como no celular |
| `baixar_modulo_de_video` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `baixar_versao` | camada-versoes.js, tela-boot.js | fora | recusa nomeada | — | Não há depósito de versões | recusa nomeada (só no app) |
| `banir_pessoa` | camada-moderar.js | fora | recusa nomeada | — | Moderação. | atende (o PC administra) |
| `busca_andar` | tela-sessao.js | igual | atende | — | Dá a volta nas pontas. | como no celular |
| `busca_limpar` | tela-sessao.js | igual | atende | — | Chamado ao sair (área da conversa). | como no celular |
| `buscar` | tela-sessao.js | igual | atende | — | Dobra acento e caixa e guarda o cursor com `resume_at`. Hoje isso é `seele-core::search`, que o `seele-web` não depende (achado 7). | como no celular |
| `calar_a_tela` | tela-chamada.js | adaptado | atende | — | Mesma dependência de `som_da_tela`. | como no celular |
| `caminho_entre_pares` | camada-servidores.js | fora | recusa nomeada | — | A seção não aparece (PA-47) | recusa nomeada (só no app) |
| `catalogo_de_mods` | camada-mods.js | fora | recusa com frase | — | `SoNoAppInstalarMod` | como no celular |
| `codigo_do_mod` | base.js | adaptado | atende | — | devolve `{fonte, capacidades}`, que é o que o `iniciar` do executor do navegador pede (`mods-no-navegador.js:145-195`, spike no branch `0.16/web`). No desktop o nativo ignora o código (`mods-runtime.js:143-145`) | como no celular |
| `compartilhar_tela` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável; a frase de V-78 já está na tela. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `conferir_esta_versao` | tela-boot.js | adaptado | atende | — | Responde que está tudo bem: quem recusa uma versão revogada é a origem. A casca o chama antes de todo `connect`, e um guarda prende essa ordem (main.rs:1808-1809) | como no celular |
| `conhecidos` | camada-servidores.js, tela-sessao.js | adaptado | atende | sessão | Os servidores deste aparelho, do armazenamento da origem, com os hashes guardados por `fp`. A trilha da aba SERVIDOR o chama como o desktop. | como no celular |
| `conjunto_exigido_agora` | camada-mods.js | fora na 0.16 | recusa nomeada | — | Pertence à gestão. No desktop responde «» sem hospedar. | como no celular |
| `connect` | tela-boot.js | adaptado | atende | — | WebTransport, assinatura pelo WebCrypto e `audio` ignorado (D-52). Emite `ConnectStageChanged` sem as etapas do ponto de encontro e devolve `FalhaDaEntrada` com trilha. A chave nasce aqui, na primeira entrada (ADR 0057, emenda 0017) | como no celular |
| `consentir_no_caminho_entre_pares` | camada-servidores.js | fora | recusa nomeada | — | PA-47, PA-48 | recusa nomeada (só no app) |
| `consentir_previa` | tela-sessao.js | adaptado | atende | — | Grava no armazenamento da origem, e não no arquivo de preferências. AJUSTES · PRÉVIAS DE LINK desfaz. | como no celular |
| `controles_da_voz` | tela-server.js | adaptado | atende | carga | `supressao: 1` fixo (o navegador); abertura e extremos do portão da ponte | como no celular |
| `criar_convite_do_server` | camada-portaria.js | fora | recusa nomeada | — | Portaria. | recusa nomeada (só no app) |
| `criar_linha` | tela-sessao.js | fora | recusa nomeada | — | Administração. | atende (o PC administra) |
| `criar_servidor` | tela-boot.js | fora | recusa nomeada | — | Hospedar | recusa nomeada (só no app) |
| `criar_voice_room` | tela-sessao.js | fora | recusa nomeada | — | Administração. | atende (o PC administra) |
| `decidir_pedido` | camada-portaria.js | fora | recusa nomeada | — | Portaria. | recusa nomeada (só no app) |
| `definir_senha_do_server` | camada-portaria.js | fora | recusa nomeada | — | Portaria. | recusa nomeada (só no app) |
| `descartar_mensagem` | tela-sessao.js | igual | atende | — | Devolve o texto para o campo. | como no celular |
| `descrever_arquivo` | tela-sessao.js | adaptado | recusa nomeada | — | **Hoje recebe um caminho de disco.** Só o arraste chama, e o arraste não existe no celular. Se viesse, o `File` do navegador já traz nome e tamanho: não há o que descrever. | como no celular |
| `desmontar_a_sessao` | tela-fim.js | adaptado | atende | — | O mesmo que `disconnect`, porque não há hospedagem a manter | como no celular |
| `destino_do_anexo` | tela-sessao.js | adaptado | atende | — | Devolve só o **nome** que veio no histórico, sem `folder` e sem `beside`, que não existem no download do navegador. Recebe só o número do anexo. | como no celular |
| `disconnect` | tela-fim.js, tela-sessao.js | adaptado | atende | — | Encerra a sessão da página; não há hospedagem a derrubar. | como no celular |
| `dispensar_aviso` | tela-sessao.js | igual | atende | — | `×` do alerta. | como no celular |
| `dominios_de_gif` | tela-sessao.js | igual | atende | carga | Constante `tenor.com`, `giphy.com`, `imgur.com`. | como no celular |
| `enter_voice_room` | tela-sessao.js | adaptado | atende | — | É aqui que o áudio abre (D-52); a senha vem da folha SENHA DA SALA. | como no celular |
| `enviar_anexo` | tela-sessao.js | adaptado | atende | — | **Hoje recebe `caminho`, e o núcleo lê o disco** (seele-ffi lib.rs:1475–1496). A ponte precisaria receber **os bytes** (o `File`/`Blob`, ou a referência a ele), **o nome**, **o tipo alegado**, o tamanho, o `channel` e o `body`. Devolve a chave na hora, e o andamento vem por `TransferChanged`. | como no celular |
| `enviar_imagem_mod` | base.js | igual | atende | — | idem | como no celular |
| `escolher_apelido` | camada-nomear.js | igual | atende | — | `SALVAR`, em SEU PERFIL. | como no celular |
| `escolher_apelido_local` | camada-nomear.js | igual | atende | — | Armazenamento da origem. | como no celular |
| `escolher_arquivo` | tela-sessao.js | adaptado | atende | — | **Hoje devolve um caminho de disco.** A ponte abre o `<input type="file">` do navegador e devolve `{nome, tamanho, tipo}` com uma **referência ao `File` guardado na página** no lugar de `caminho`. A tela chama sem nenhum `await` antes, dentro do gesto (tela-sessao.js:2925–2936). O `tipo` sai da extensão pela mesma tabela de `tipo_alegado` (main.rs:3573), e não do `File.type`. | como no celular |
| `escolher_icone_do_server` | tela-server.js | fora | recusa com frase | — | administração | atende (o PC administra) |
| `escolher_microfone` | tela-server.js | adaptado | atende | — | grava; com sala, reabre a captura com `deviceId: { exact }`; a troca que falha devolve `DispositivoSumiu` | como no celular |
| `escolher_minha_imagem` | camada-nomear.js | adaptado | atende | — | A ponte abre o seletor do navegador dentro do gesto (sem `await` antes) e reduz a imagem como o `icone::encolher` do Rust. | como no celular |
| `escolher_para_o_mod` | base.js | adaptado | atende | — | o seletor do navegador; `papeis`/`extensoes` viram `accept`; o caminho não atravessa (já é assim). HEIC é com o rascunho (`:926`) | como no celular |
| `escolher_saida` | tela-server.js | fora | recusa com frase | — | idem | como no celular |
| `escolher_tecla_de_falar` | tela-server.js | adaptado | recusa nomeada | — | o botão não existe | como no celular |
| `esquecer` | camada-servidores.js | adaptado | atende | — | Apaga também os hashes guardados daquele servidor (PA-19) | como no celular |
| `esquecer_aceite_de_mods` | camada-mods.js | adaptado | atende | — | Apaga o aceite deste aparelho, no armazenamento da origem. | como no celular |
| `esquecer_previa` | tela-server.js | adaptado | atende | — | idem | como no celular |
| `estado_da_porta` | camada-portaria.js, tela-sessao.js | fora | atende | sessão | Chamado sozinho a cada poucos segundos na sessão: responde «não hospeda», e nada de portaria aparece. | como no celular |
| `estado_da_primeira_abertura` | camada-versoes.js | fora | atende | carga | É chamado em toda carga (camada-versoes.js:358). Responde sem dados, e a nota da 0.15 não aparece | atende, vazio (só no app) |
| `estado_da_sessao` | camada-mods.js | adaptado | atende | — | os contadores, sem `mods_nativos`; com as instâncias do executor do navegador | como no celular |
| `estado_do_teste_de_microfone` | tela-server.js | fora | recusa com frase | — | D-52 | atende pelo `getUserMedia` (decisão da orquestradora, 09/10: a D-52 é do áudio da sessão, e o teste a pessoa abre de propósito) |
| `estou_hospedando` | camada-mods.js | igual | atende | — | `false` sempre: a página nunca hospeda | recusa nomeada (só no app) |
| `exclusao_do_som_da_captura` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável. `SemCaptura` seria a resposta verdadeira, se for preciso responder. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `expulsar_pessoa` | camada-moderar.js | fora | recusa nomeada | — | Moderação. | atende (o PC administra) |
| `fechar_teste_de_microfone` | tela-server.js | fora | atende | — | sem efeito: `pararOTesteDeMicrofone` o chama a cada saída de ÁUDIO (ui/tela-server.js:150, 684), e uma recusa sujaria o console a cada troca | atende, e solta o aparelho de verdade: as faixas do `getUserMedia` param |
| `fontes_de_tela` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `guardar_colado` | tela-sessao.js | adaptado | atende | — | **Hoje grava um arquivo temporário e devolve o caminho dele.** A ponte precisaria receber o **`Blob` colado (os bytes) e o tipo**, sem base64 e sem disco, e devolver o mesmo formato com a referência no lugar do caminho. O teto de 25 MiB existia por causa da travessia em base64. | como no celular |
| `ha_ajustes_de_acessibilidade` | tela-server.js | adaptado | atende | carga | responde `false`; o código de hoje esconde o botão | como no celular |
| `hospedar` | tela-boot.js | fora | recusa nomeada | — | Hospedar | recusa nomeada (só no app) |
| `icone_do_server` | tela-server.js | igual | atende | sessão | Só quando `icon_revision` anda. | atende (o PC administra) |
| `imagem_da_pessoa` | tela-sessao.js | igual | atende | sessão | Retratos dos cartões e de SEU PERFIL. | como no celular |
| `instalar_atualizacao` | tela-server.js | adaptado | recusa com frase | — | idem; o canal `seele://atualizacao` nunca é emitido | como no celular |
| `instalar_mod` | camada-mods.js | fora | recusa com frase | — | `SoNoAppInstalarMod`; no desktop é o seletor nativo de pasta | recusa nomeada (só no app) |
| `instalar_mod_do_catalogo` | base.js, camada-mods.js | adaptado | atende | — | Só pelo aceite: busca no catálogo o pacote que o servidor exige e o entrega ao executor do navegador. O INSTALAR da gestão não existe no celular. | como no celular |
| `leave_voice_room` | tela-chamada.js | igual | atende | — | É aqui que o áudio fecha (D-52). | como no celular |
| `lembrar_aparencia_do_servidor` | tela-auth.js | igual | atende | sessão | Grava na lista deste aparelho | como no celular |
| `ler_imagem_mod` | base.js | igual | atende | — | volume (API 5/6). Se a ponte web não tiver, `volume` some do `SeeleUI` (rascunho `:133-139`) | como no celular |
| `ligar_portaria` | camada-portaria.js | fora | recusa nomeada | — | Portaria. | recusa nomeada (só no app) |
| `limite_da_mensagem` | tela-sessao.js | igual | atende | carga | Constante 4096, o `MAX_BODY_LEN` de `seele-proto`, que o `seele-web` já enxerga. | como no celular |
| `linhas_para_hospedar` | tela-boot.js | fora | recusa nomeada | — | Hospedar | recusa nomeada (só no app) |
| `messages` | tela-sessao.js | igual | atende | sessão | O histórico local da sessão, com os campos `estado` e `client_message_id` (este em **texto**, porque passa de 2^53; seele-ffi types.rs:712–775). | como no celular |
| `meu_retrato` | camada-nomear.js, tela-boot.js | igual | atende | carga | Armazenamento da origem | como no celular |
| `microfone_escolhido` | tela-server.js | adaptado | atende | — | `deviceId` guardado na origem | como no celular |
| `microfones` | tela-server.js | adaptado | atende | — | `enumerateDevices()` → `audioinput`; nomes vazios antes da 1ª permissão | como no celular |
| `midia_do_mod` | base.js | igual | atende | — | do pacote obtido | como no celular |
| `midia_em_bytes` | base.js | igual | atende | — | — | como no celular |
| `mod_nativo_ativar` | mods-runtime.js | fora | recusa nomeada | — | **não existem no web.** O executor é o `mods-no-navegador.js` da F4: `executorNoNavegador(id, geracao, hash)`, `nome:"navegador"`, os mesmos quatro verbos de `mods-runtime.js`, falando por `postMessage` com um `iframe` opaco (`preparar`, `subir`, `entregar`, `lido`, `encerrar`). O evento `seele://mod-nativo` também não existe lá | como no celular |
| `mod_nativo_colher` | mods-runtime.js | fora | recusa nomeada | — | **não existem no web.** O executor é o `mods-no-navegador.js` da F4: `executorNoNavegador(id, geracao, hash)`, `nome:"navegador"`, os mesmos quatro verbos de `mods-runtime.js`, falando por `postMessage` com um `iframe` opaco (`preparar`, `subir`, `entregar`, `lido`, `encerrar`). O evento `seele://mod-nativo` também não existe lá | como no celular |
| `mod_nativo_encerrar` | mods-runtime.js | fora | recusa nomeada | — | **não existem no web.** O executor é o `mods-no-navegador.js` da F4: `executorNoNavegador(id, geracao, hash)`, `nome:"navegador"`, os mesmos quatro verbos de `mods-runtime.js`, falando por `postMessage` com um `iframe` opaco (`preparar`, `subir`, `entregar`, `lido`, `encerrar`). O evento `seele://mod-nativo` também não existe lá | como no celular |
| `mod_nativo_entregar` | mods-runtime.js | fora | recusa nomeada | — | **não existem no web.** O executor é o `mods-no-navegador.js` da F4: `executorNoNavegador(id, geracao, hash)`, `nome:"navegador"`, os mesmos quatro verbos de `mods-runtime.js`, falando por `postMessage` com um `iframe` opaco (`preparar`, `subir`, `entregar`, `lido`, `encerrar`). O evento `seele://mod-nativo` também não existe lá | como no celular |
| `mod_nativo_reservar` | mods-runtime.js | fora | recusa nomeada | — | **não existem no web.** O executor é o `mods-no-navegador.js` da F4: `executorNoNavegador(id, geracao, hash)`, `nome:"navegador"`, os mesmos quatro verbos de `mods-runtime.js`, falando por `postMessage` com um `iframe` opaco (`preparar`, `subir`, `entregar`, `lido`, `encerrar`). O evento `seele://mod-nativo` também não existe lá | como no celular |
| `mod_request` | base.js | igual | atende | — | resposta por `ModReply` no `listen` da ponte; mesma fila (8 em voo, 12 KiB, 15 s) | como no celular |
| `modo_de_voz_escolhido` | tela-server.js | igual | atende | — | sem escolha gravada, responde `VoiceActivated` | como no celular |
| `mods_instalados` | base.js, camada-mods.js | adaptado | atende | sessão | É usado pelo aceite para conferir id e hash. O que «instalado» significa no celular é da F4 e da área de MODs. | como no celular |
| `modulo_de_video_a_baixar` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável; assistir não exige o módulo. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `monitorar_o_microfone` | tela-server.js | fora | recusa com frase | — | D-52 | atende pelo `getUserMedia` (decisão da orquestradora, 09/10: a D-52 é do áudio da sessão, e o teste a pessoa abre de propósito) |
| `mover_pessoa` | camada-moderar.js | fora | recusa nomeada | — | Moderação (administração). | atende (o PC administra) |
| `ocupacao_do_deposito` | camada-versoes.js | fora | recusa nomeada | — | Não há depósito | recusa nomeada (só no app) |
| `open_channel` | tela-auth.js, tela-sessao.js | igual | atende | — | — | como no celular |
| `pacotes_no_cache` | camada-mods.js | fora | recusa com frase | — | `SoNoAppPacotesDeMod` | recusa nomeada (só no app) |
| `parar_de_compartilhar` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `pasta_de_downloads` | tela-sessao.js | fora | atende | carga | Chamado na carga: responde vazio. Salvar um anexo é o download do navegador. | como no celular |
| `pedaco_do_escolhido` | base.js | igual | atende | — | — | como no celular |
| `pedidos_da_portaria` | camada-portaria.js | fora | recusa nomeada | — | Portaria (quem hospeda). | recusa nomeada (só no app) |
| `pedir_permissao_de_tela` | camada-compartilhar.js | fora na 0.16 | recusa nomeada | — | Inalcançável: a caixa não abre. | atende, pelo `getDisplayMedia` (F4, com medida) |
| `pedir_quadro_chave` | palco-imagem.js | igual | atende | — | RECEBER. Sai em `ScreenOpened` (palco-imagem.js:234). | como no celular |
| `permissao_de_microfone` | tela-server.js | adaptado | atende | — | Permissions API; precisa da variante `NegadaPeloNavegador` (achado 3) | como no celular |
| `permissao_de_tela` | camada-compartilhar.js | fora na 0.16 | recusa com frase | — | É chamado **ao abrir a chamada** (tela-chamada.js:630). A recusa tem de dar a frase de V-78, e não a de V-16c (achado 8). | atende, pelo `getDisplayMedia` (F4, com medida) |
| `peso_da_linha` | camada-moderar.js | fora | recusa nomeada | — | Só alimenta a caixa de apagar canal. | atende (o PC administra) |
| `plataforma` | base.js | adaptado | atende | carga | Responde `"web"`. A casca do celular já foi escolhida antes, pela marca síncrona da ponte. | como no celular |
| `preparar_icone_do_server` | tela-boot.js | fora | recusa nomeada | — | Hospedar | recusa nomeada (só no app) |
| `prever_anexo` | tela-sessao.js | igual | atende | — | Baixa o anexo do servidor para a memória, até 4 MiB. O `sniff` está em `seele-proto::imagem`; o `judge` está em `seele-core::preview` (achado 7). | como no celular |
| `previa_de_link` | tela-sessao.js | adaptado | atende | — | O consentimento é lido do armazenamento deste aparelho. A busca e a conferência dos bytes estão no achado 5. Devolve `data:` ou os mesmos erros enumerados (`SemConsentimento`…). | como no celular |
| `previas_consentidas` | tela-server.js | adaptado | atende | — | lista guardada na origem | como no celular |
| `procurar_atualizacao` | tela-server.js | adaptado | recusa com frase | — | «Não há o que instalar…» (VERSÃO DA PÁGINA) | como no celular |
| `reenviar_mensagem` | tela-sessao.js | igual | atende | — | Manda com a mesma chave; o servidor é idempotente por ela. | como no celular |
| `registrar_da_janela` | base.js | adaptado | atende | — | não há `seele.log` no aparelho; onde o registro fica é a decidir | como no celular |
| `regras_de_previa` | tela-sessao.js | igual | atende | carga | Constante: 4 MiB e png, jpeg, gif, webp. | como no celular |
| `regras_do_icone_do_server` | tela-boot.js, tela-server.js | fora | atende | carga | Chamado na carga: responde as regras, que nada no celular usa (o ícone do servidor é de quem administra). | como no celular |
| `remover_mensagem` | camada-moderar.js | adaptado | atende | — | A própria mensagem: igual, pela folha da mensagem. A de outra pessoa: fora, com a frase só para quem modera. Quem confere a permissão é o servidor. | atende (o PC administra) |
| `remover_versao` | camada-versoes.js | fora | recusa nomeada | — | Não há depósito | recusa nomeada (só no app) |
| `renomear_linha` | camada-moderar.js, tela-sessao.js | fora | recusa nomeada | — | Administração. | atende (o PC administra) |
| `renomear_server` | tela-boot.js, tela-server.js | fora | recusa nomeada | — | Desta área, só a tela de preparar o chama | atende (o PC administra) |
| `renomear_voice_room` | tela-sessao.js | fora | recusa nomeada | — | Administração. | atende (o PC administra) |
| `revogar_admissao` | camada-portaria.js | fora | recusa nomeada | — | Portaria. | recusa nomeada (só no app) |
| `saida_escolhida` | tela-server.js | fora | recusa com frase | — | idem | como no celular |
| `saidas` | tela-server.js | fora | recusa com frase | — | `desenharDispositivos` faz `Promise.all` dos dois lados: a recusa derruba a lista de entrada (achado 4) | como no celular |
| `salvar_anexo` | tela-sessao.js | adaptado | atende | — | **Hoje grava na pasta de downloads**, sem substituir e com quarentena do sistema (main.rs:3651–3660). A ponte recebe só o número do anexo: baixa os bytes do servidor (`Receiving n%`) e os entrega ao **download do navegador** com o nome do histórico. `Saved` volta **sem caminho**. | como no celular |
| `send_message` | tela-sessao.js | igual | atende | — | Precisa repetir a recusa local `TooLong` acima de 4096 bytes. No desktop, um corpo grande demais derrubava a sessão ao ser codificado (seele-ffi lib.rs:1645–1651). Devolve a chave em texto. | como no celular |
| `servidor_hospedado` | camada-mods.js | fora | recusa nomeada | — | só é chamado com `estou_hospedando` verdadeiro | recusa nomeada (só no app) |
| `servidores_guardados` | camada-mods.js, tela-boot.js | fora | recusa nomeada | — | Hospedar | recusa nomeada (só no app) |
| `set_muted` | tela-sessao.js | adaptado | atende | — | MUDO na faixa de voz. Fora de sala não há faixa (achado 3). | como no celular |
| `set_talking` | tela-sessao.js | adaptado | atende | — | Vem do SEGURAR PARA FALAR; fecha em touchcancel, ao sair do botão, com a página oculta e na queda. | como no celular |
| `set_total_isolation` | tela-sessao.js | adaptado | atende | — | SURDO na faixa de voz. | como no celular |
| `set_voice_mode` | tela-server.js | adaptado | atende | — | os mesmos três nomes; `PushToTalk` = SEGURAR PARA FALAR | como no celular |
| `set_volume` | tela-chamada.js, tela-sessao.js | adaptado | atende | — | Folha da pessoa. A chave é o apelido, não o id. | como no celular |
| `snapshot` | base.js, camada-compartilhar.js, camada-moderar.js, camada-nomear.js, tela-auth.js, tela-chamada.js, tela-server.js, tela-sessao.js | igual | atende | carga | O estado da sessão. O MOD não o lê na API 6: lê o retrato projetado. | como no celular |
| `soltar_escolhido` | base.js | igual | atende | — | — | como no celular |
| `som_da_tela` | tela-chamada.js | adaptado | atende | — | RECEBER. **A medir:** hoje lê a mistura nativa. No celular, o som da tela precisa chegar à voz da página (lib.rs:4162–4177). | como no celular |
| `som_do_mod` | base.js | igual | atende | — | som de MOD só com a tela acesa (rascunho §11) | como no celular |
| `tecla_de_falar` | tela-sessao.js | adaptado | atende | carga | Chamado na carga: responde «sem tecla». Falar apertando é o SEGURAR PARA FALAR da faixa de voz. | como no celular |
| `tela_cheia` | palco-imagem.js | adaptado | recusa nomeada | — | Não há janela. A tela cheia é o corte por CSS (`data-cinema`), mais o aparelho deitado. A recusa hoje só vai ao console (palco-imagem.js:462–466). | como no celular |
| `tirar_icone_do_server` | tela-server.js | fora | recusa com frase | — | administração | atende (o PC administra) |
| `tirar_minha_imagem` | camada-nomear.js | igual | atende | — | Armazenamento da origem. | como no celular |
| `versoes_instaladas` | camada-versoes.js, tela-boot.js | fora | atende | carga | Chamado na carga: responde a lista vazia, e o bloco das versões some (a regra do desktop com menos de duas). | atende, vazio (só no app) |
