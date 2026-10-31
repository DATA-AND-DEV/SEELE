# 0.19 — decisões do dono

Uma entrada por decisão, com a data, as palavras do dono e o que mudou no
código. A decisão vale por cima de qualquer regra mais antiga que ela cite.

## 31/10/2026 — fechar com um pedido na portaria pergunta antes

> «Sim, deve confirmar com o usuário. Percebi que muita gente confunde a espera
> com não ter dado certo. Precisa ficar explícito que só está esperando a
> aprovação do host.»

- **Substitui, neste caso, a regra de 23/09** (`docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md`:
  «a confirmação ao fechar vale só para o servidor guardado»). Fechar a janela
  do app — o botão vermelho, o Alt+F4 e o Cmd+Q do macOS, cujo Sair fecha a
  janela (`trocar_o_sair_do_mac`) — com um pedido pendente na portaria de
  alguém (`esperaPendenteNaTela()`) mostra a caixa de `camada-sair-da-espera.js`:
  CONTINUAR ESPERANDO de padrão, «nada deu errado», e que fechar não cancela o
  pedido. Sem servidor no ar e sem pedido, fecha direto; com o servidor no ar,
  vale a pergunta dele, que diz quem cai junto.
- **O que o guarda da credencial continua protegendo** (`a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`):
  as duas credenciais da tentativa são esquecidas antes de qualquer veto, e o
  `close-requested` cru segue proibido. Só a condição do veto mudou: servidor no
  ar, **ou** pedido pendente.
- **Não pergunta:** o Sair do Dock e o logout do macOS (vão direto ao
  `RunEvent::Exit`, sem janela), e o fechar da aba no navegador.
- **O navegador fica sem pergunta, de propósito.** O único jeito é o
  `beforeunload`, e ele só mostra a frase do navegador («sair do site? as
  alterações podem não ser salvas»), sem texto nosso. Ela diz o contrário do
  que é verdade — nada se perde: o pedido continua no servidor e o servidor na
  lista do aparelho — e reforça justamente a confusão que o dono quer tirar. O
  que o navegador ganha é a cabeça explícita da espera, abaixo.
- **A espera diz, acima de tudo e em letra grande, que só espera a aprovação**:
  «Esperando a aprovação de quem hospeda», e embaixo que não é carregamento nem
  erro. Na `#tela-auth` (desktop, casca larga) ela toma o lugar do «Entrar no
  servidor» na espera pendente; na «ENTRAR EM» do celular, vem acima do título,
  no laranja do que se lê, e não no vermelho da falha.

## 31/10/2026 — o pedido da reserva diz que é a reserva (API 8, parte 7)

Decisão da revisão de fechamento (sem palavras do dono; o pedido era «a máquina
de reserva precisa dizer que é a reserva»).

- **O papel vai no fio.** `ServerMessage::PedidoDeMaquina` ganhou `papel:
  PapelDaMaquina` no fim (`seele_proto::maquina`; o `Papel` do servidor passou a
  ser um apelido dele). O protocolo 12 não saiu em release nenhum (conferido com
  o `curl` do CLAUDE.md: a mais nova é a v0.18.3, protocolo 11), então o campo
  cabe, como o `quem` do `AlcanceTunelPor` (R6 da parte 2): só a linha
  `servidor:PedidoDeMaquina` de `fio_2_0.txt` foi trocada (agora com `Reserva`,
  para o byte do papel aparecer) e a semente `servidor-pedido-de-maquina` do
  fuzz regerada.
- **O papel entra no resumo do aceite** (`resumo_da_oferta(mod, hash, servidor,
  papel, oferta)`, rótulo `/2`). Ser a reserva muda o que a pessoa aceita: este
  computador guarda uma cópia do mundo e só roda o jogo se a principal sair. E
  o app **aceita sozinho** um pedido cujo resumo bate com o aceite guardado
  (`decidir`): sem o papel no resumo, o «sim» de reserva ligaria sozinho um
  pedido de principal com a mesma oferta. O aceite guardado leva o papel
  (`AceiteDeMaquina.papel`, ausente = principal), o FFI confere o resumo com ele
  (`PapelInvalido`, `ResumoNaoConfere`), e o `mudou` da tela diz «o papel deste
  computador». *Custo:* aceites guardados com o `/1` (só de builds de
  desenvolvimento) não batem mais, e a tela pergunta de novo uma vez.
- **A tela:** chip RESERVA no alto (ao lado das portas), título «… pede que
  este computador seja a reserva de um servidor de jogo», e a primeira linha
  «como reserva: este computador só roda o jogo se a máquina principal sair,
  com uma cópia do mundo». A principal não muda. Bancada `aceite-da-maquina.cjs`,
  caso 22.
- `api/v8.json` ganhou uma frase no fim de `mundo.maquinas.pedir` (acréscimo),
  com a linha da v8 refeita em `api/congeladas.sha256` e a cópia do contrato no
  MOD de multiplayer.

## 31/10/2026 — «o jogo segue rodando aqui» só com o jogo de pé

A faixa da máquina sem quem hospeda (`sem-anfitriao`) dizia «o jogo segue
rodando aqui» logo abaixo de «o jogo não está rodando aqui» — a reserva que
esperava, ou o jogo que caiu. Agora ela lê `jogosRodando` (o `maquina-jogo` de
`d6fd09f1`): com o jogo de pé, a frase de antes; sem ele, «quem hospeda o SEELE
saiu, e o jogo não está rodando aqui; se ele não chamar esta máquina de volta
em 10 minutos, ela é desligada». O caso 4b de `aceite-da-maquina.cjs` afirmava
a frase contraditória e passou a provar as duas.

## 31/10/2026 — as quatro bancadas de navegador que reprovavam na base da 0.19

As quatro estão no job `bancadas` do `ci.yml` e na bateria do
`empacotar/publicar.sh` (o `release.yml` roda só as de Node puro): reprovadas,
seguravam a publicação. Cada uma decidida entre «a bancada ficou velha» e «o
produto regrediu»; nenhuma deixou de afirmar a promessa dela.

- **`nucleo.cjs` — o produto regrediu (três vezes).**
  1. *O tema alcançava o núcleo.* A pele da 0.19 (`tela-sessao.css`) repintou o
     MUDO e o fone (`.operador-glifo`), o MUDO aceso, o veredito e o «no ar»
     com `--seele-*`, que o tema do servidor escreve. Consertado lendo os
     `--nucleo-*` (dois novos: `--nucleo-tinta-no-acento`, `--nucleo-acento-claro`)
     e devolvendo ao «no ar» a placa opaca do núcleo. O desenho sem tema não
     muda: os `--nucleo-*` resolvem para os mesmos tokens.
  2. *O «no ar» fora da vista.* Desde `b8dec1f8` ele é só do leitor de tela no
     computador (`clip-path`, 1 px), e o guarda 11 o tratava como à vista. **Aqui
     a bancada ficou velha:** ela agora separa o item fora da vista e exige que
     seja só o «no ar», e como `role="status"`.
  3. *Um aviso de MOD cobria o DEPOIS da portaria.* A faixa da portaria ficou
     larga o bastante na 0.19 para entrar na coluna dos avisos (z 70). Os avisos
     sobem acima da faixa enquanto ela está acesa (`--portaria-altura`, medida
     por `camada-portaria.js`; `mods-superficies.css`), sem mexer no z dela (a
     razão de `87908e41` continua).
- **`tema-do-servidor.cjs` — a bancada ficou velha (seletor), e o produto
  regrediu (fundo).** QUEM DESENHA O QUE trocou os botões por um seletor por
  lugar (canvas `Config-mods`): a bancada lê as ações nas opções (`data-rotulo`)
  e escolhe pelo `change`, com as mesmas ações exigidas e o seletor ligado. Mas
  medir a coluna de canais mostrou o defeito: as colunas viraram vidro, e atrás
  delas estava o `body`, com a cor da raiz — **um tema claro deixava o texto
  escuro dele sobre o negro do produto**. `#tela-sessao` (computador) passou a
  ter o fundo do `body` (a cor e a grade), lido onde o tema escreve: sem tema,
  nada muda. A bancada mede «a região não vazou» contra o mesmo tema sem
  regiões, e exige que o fundo do tema chegue atrás da coluna.
- **`navegacao-0-17.cjs` — a bancada ficou velha.** A cena
  `app-entrada-detalhes` abria `.boot-detalhes` e `#boot-nome-publico`, que a
  0.19 tirou de propósito (`01364d19`); `entrada-simples.cjs` prova que não
  voltam. A cena saiu, com o motivo no lugar dela.
- **`hospedagem-config-0174.cjs` — o produto regrediu.** O painel de vidro de
  «Antes de hospedar» crescia com o conteúdo (`flex: 1 0 auto`), a tela inteira
  rolava, e o HOSPEDAR e o VOLTAR ficavam abaixo da janela nas cinco cenas
  longas. Contra o esqueleto `.area` de `base.css` («o conteúdo rola na
  própria linha»). O painel ocupa a altura da janela, e não mais, e quem rola é
  o corpo; a decisão fica no pé dele, como o desenho da 0.19 pede. Só com
  420px de altura ou mais: abaixo disso (o 1024×768 a 200%) a cabeça e a barra
  comeriam o corpo, e a tela rola inteira como antes — `area-util.cjs` mede
  isso, e reprova as mesmas 29 medidas com e sem a mudança.

**As outras bancadas do job `bancadas`.** Rodadas as 88 desta máquina: além das
quatro, dez reprovam igual na base `2f163b18` (`conversa-com-estado`,
`qualidade-da-voz`, `avatares-do-mod`, `diagnostico-de-mods`,
`tema-claro-legivel`, `um-redesenho-por-quadro`, `linhas-pelo-id`,
`aviso-antes-de-atualizar`, `palco-na-troca-de-vista`, `vista-pessoal-0-17`), e
as de navegador com `.wasm`, `seeled` ou o Playwright do repositório não rodam
aqui sem os artefatos do CI. Ficam fora deste lote: seguram a publicação do
mesmo jeito, e cada uma pede a mesma decisão (bancada velha ou produto
regredido).
