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
