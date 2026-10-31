# API 8, revisão final — decisões dos consertos do lado do SEELE

Sobre `desenho/api-8-revisao` a partir de `5d053dbd`. A revisão é
`revisao-final-api8.md` (I1–I10, M1–M13); aqui ficam só os achados do lado do
SEELE (o I2, o I10, o M1, o M10, o M11 e o M12 são do MOD de multiplayer).
Spec: `docs/superpowers/specs/2026-10-29-api-8-e-mod-multiplayer-design.md`.
Conferido em 31/10/2026 pelo `curl` do CLAUDE.md: a mais nova é a v0.18.3, e
a v8 (protocolo 12) segue sem release.

Cada decisão no formato `Ruling: <o quê> — <por quê> — <custo se errada>`.

## Decisões

- **Ruling R1 (I1): o que roda é o que foi aceito, por caminhos presos pela
  política, sem estado.** A política prende o comando (a pasta de cima
  inteira quando ele mora numa — `java/` para `java/bin/java` —, ou o arquivo
  com e sem `.exe` quando mora na raiz) e cada argumento que é caminho
  (`server.jar`; `@arquivo` sem a arroba; o que começa com `-` não). Uma
  escrita que não traz os bytes de um artefato aceito — `baixar` pela segunda
  porta com um hash que não é o de um artefato, `escrever`, `trazer`,
  `empacotar`, `receberCopia`, `desempacotar` de um pacote cujos bytes não são
  os de um artefato — não cai sobre um preso, dentro de uma pasta presa, nem
  sobre uma pasta que contém um (por cima, cada entrada é conferida antes de
  qualquer uma entrar); a recusa é `reservado-ao-aceite`, com `caminho` e
  `reservado`. A comparação é pelo caminho real (link dentro da raiz), sem
  caixa e sem os pontos e espaços do fim. O pacote é «aceito» pelos bytes
  (tamanho + hash do mesmo algoritmo de um artefato desta máquina), e não por
  quem o baixou. **E o `rodar` só roda na raiz** (`pasta-fora-do-aceite`): os
  argumentos se leem a partir da pasta, e uma pasta livre rodaria o
  `server.jar` que o MOD escreveu nela. — Por que não «o `rodar` confere a
  proveniência»: a proveniência é estado, e um estado da memória não
  atravessa uma reabertura do app (um `server.jar` plantado na sessão 1 rodaria
  na 2); re-hashear o comando antes de `rodar` não serve ao Java aberto de um
  pacote (a tela mostra o hash do pacote, não o de `bin/java`), nem ao
  `fabric-server-launch.jar` que o instalador cria. A regra pela política vale
  na primeira escrita, em qualquer ordem e depois de reabrir. — Custo: (a) uma
  receita cujo comando mora numa pasta que também guarda dados (o mundo, os
  mods) não deixa a metade escrever ali: o runtime tem de ter a pasta dele; (b)
  um argumento aceito que é uma pasta (`--world world`) prende a pasta inteira
  contra troca; (c) o que o aceite não nomeia segue livre pela decisão Q1 — os
  `mods/*.jar` de um pack e as bibliotecas que um `@unix_args.txt` lista são
  código Java que o Java aceito carrega (é o I2, a tela do MOD); (d) o
  processo do jogo, que já é o código aceito, escreve onde quiser; (e) nomes
  curtos do Windows (`JAVA~1`) e formas Unicode diferentes de um nome não-ASCII
  só se resolvem quando o caminho já existe (o `canonicalize`); (f) a
  `pasta` de `rodar` deixa de ser uma escolha: um MOD que rodava numa
  subpasta passa a ser recusado (nenhum dos de referência roda).
- **Ruling R2 (I3): a máquina serve a porta que ela pediu, e recusa a
  concessão que traz outra.** `servindo` guarda o protocolo e a porta do
  `ServirTunel` ao lado da resposta; a concessão é registrada com eles, e uma
  com outra porta ou outro protocolo não registra nada, manda `PararTunel` ao
  servidor e responde `fora-do-aceite` a quem pediu, com as duas portas no
  log. — Por que recusar, e não só registrar a pedida: uma concessão que não
  confere é um servidor quebrado ou hostil, e servir calado esconderia isso;
  e `fora-do-aceite` já é o nome que a metade conhece para «servir o que não
  foi aceito». — Custo: um servidor que um dia reescreva a porta de propósito
  (não há caso hoje) faria a máquina parar de servir até os dois concordarem.
- **Ruling R3 (I4): o túnel que o servidor para é dito à máquina.**
  `maquinas::parar_tuneis` (a reserva que assume a posse, o aceite que muda os
  túneis, outro pedido) passa por `Tuneis::parar_pelo_servidor`, que manda
  `TunelEncerrado { MaquinaParou }` também à máquina; o `parar` que a própria
  máquina pede (`PararTunel`) segue sem eco. No cliente, esse aviso já chama
  `Maquina::parar`, que fecha as ligações diretas. — Por que não um
  `TunelRetirado` por pessoa: a retirada tira uma pessoa e deixa o túnel; aqui
  o túnel inteiro acaba, e o `MaquinaParou` é o motivo que a metade conhece.
  — Custo: a máquina que ainda é jogadora do próprio túnel também o vê acabar
  (é o que aconteceu); um `PararTunel` seguido de `ServirTunel` da máquina não
  passa por aqui, então não há eco atrasado que pare o serviço novo.
- **Ruling R4 (I5): servir de novo o mesmo protocolo e a mesma porta reusa o
  `Servido` vivo; outro para o de antes.** As ligações diretas que seguiram
  sem quem hospeda seguram o `Arc<Servido>` de antes; reusá-lo faz o `parar`
  seguinte chegar a elas e mantém as medidas num lugar só. Uma porta ou um
  protocolo novos param o de antes (`MaquinaParou`) antes de o trocar. — Custo:
  um servido de pé reusado leva junto as medidas de antes da volta (é o que se
  quer contar); um `Servido` já parado nunca é reusado.
- **Ruling R5 (I8): o teto de disco deixa de fora só os parciais vivos, e
  `:` no meio de um nome é recusado.** `Interior::parciais` guarda o parcial de
  cada download em andamento (entra ao ser criado, sai ao ser solto); a conta
  de `baixar`, `escrever`, `trazer`, `desempacotar`, `empacotar` e
  `receberCopia` pula só esses. `dentro_da_raiz` e as entradas de um pacote
  recusam `:` dentro de um pedaço (`caminho-invalido` / `pacote-invalido`): no
  NTFS, `x:s` é um fluxo alternativo que nenhuma listagem vê. — Por que não
  recusar o nome `*.seele-parcial`: recusar o nome fecha o MOD, mas não o
  parcial que um app fechado no meio deixou para trás, que também saía da
  conta; contar pelo que está vivo fecha os dois. — Custo: um parcial deixado
  por um app que caiu passa a contar no teto até alguém o apagar (o lado
  seguro); um nome de arquivo com `:` no Mac ou no Linux é recusado também.
- **Ruling R6 (I7): a conexão velha que cai de quem segue aqui por outra
  publica `Voltou` logo depois do `Parou`.** `Maquinas::voltas_de_quem_segue`
  tira de `ausentes` os lugares de quem ainda tem uma conexão no 12, e
  `maquinas::saiu` os publica; o prazo da máquina que sumiu abre como antes
  (é o mesmo par `Parou` → prazo → `Voltou` da volta comum). — Por que aqui, e
  não no `chegou`: o `chegou` da conexão nova rodou com o lugar ocupado, e
  nada mais roda depois. — Custo: enquanto a velha ainda vive (o zumbi), um
  pedido de novo volta `JaAceito` e a metade na conexão nova não é religada até
  a velha cair (o item 4 do achado) — fica, porque só o fim da velha diz que
  ela era zumbi.
- **Ruling R7 (I6): o portão da ponta conta quem o armou, e a ligação de
  outra impressão vai a quem a espera.** `PontaDaSessao` ganha um `Balcao`:
  `passar_a_atender` soma e `parar_de_atender` subtrai, e só o último fecha o
  portão; cada `atender` deixa ali uma caixa pela impressão que espera, e a
  ligação que chega com a impressão de outro que atende na mesma ponta vai à
  caixa dele, em vez de ser fechada como impostora. Vale para os túneis e para
  a tela emprestada, que passam pelo mesmo `atender`. — Por que não um laço de
  aceite só por ponta: o mesmo efeito com uma tarefa a mais para viver e
  morrer com a ponta; com a caixa, quem já espera na fila é quem despacha, e
  nada roda quando ninguém atende. — Custo: uma ligação que chega com uma
  impressão esperada por um `atender` que acabou de sair é fechada (como
  antes), e quem disca tenta de novo até o prazo dele; um `passar_a_atender`
  sem o seu `parar_de_atender` (só os testes fazem) deixa a ponta armada.
