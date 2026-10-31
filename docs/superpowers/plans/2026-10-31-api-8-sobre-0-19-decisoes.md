# A API 8 sobre a 0.19 — decisões da fusão

`desenho/api-8-partes` (c9e908a4, partes 1 a 6) fundida em
`desenho/api-8-sobre-0.19`, que nasceu de `entrega/0.19` em 01ac840f. Uma linha
por decisão, com o custo se estiver errada.

Conferido em 31/10/2026 no release (`curl` do CLAUDE.md): a mais nova é a
v0.18.3, protocolo 11. Nem o protocolo 12 nem a API 8 saíram; as duas linhas
podiam se juntar num 12 só, e a v8 segue emendável.

## O fio

- **Um 12 só, e as variantes da API 8 antes das do perfil**, em `ClientMessage`
  e em `ServerMessage`, como o §8 de `docs/0.19/perfis-oficiais.md` decidiu (a
  API 8 chegou ao 12 primeiro). Os bytes-ouro da API 8 em `tests/fio_2_0.txt`
  ficaram iguais aos da `desenho/api-8-partes`, byte a byte; os do perfil
  (`GravarPerfil` 0x36, `TirarImagemDoPerfil` 0x37, `Perfis` 0x3e …
  `PendenciaDoPerfil` 0x41) foram regerados pelo modo congelado, que só
  acrescenta. Se errada: um build de desenvolvimento da `entrega/0.19` fala o
  perfil com outros ordinais e não conversa com este; nenhum foi publicado.
- **`desde_do_cliente` é uma função só**, a união dos braços que os dois lados
  escreveram com a mesma assinatura. `CONHECIDAS` passa a 56 (cliente) e 66
  (servidor).
- **A migração 20 é a do perfil, e a das plataformas virou a 21** (o §8 também):
  só bancos de desenvolvimento da `desenho/api-8-partes` chegaram à 20 de
  plataformas. Se errada: um desses bancos, aberto por este build, acha a 20
  aplicada e pula a dos perfis — apague-o.

## Os tetos do volume (o choque semântico)

- **A imagem segue a 0.19: até 100 MiB por item** (`TETO_DE_ARQUIVO`,
  5c904d79), e o `.mrpack` segue em 64 MiB (`TETO_DO_PACK`). A parte 6 tinha
  imagem e som em 10 MiB; isso caiu.
- **O fio leva o maior dos dois** (`volume::TETO_DO_FIO_DE_VOLUME`, calculado em
  `const`, hoje 100 MiB): o `PedidoDeImagem::Enviar`, a `RespostaDeImagem` e a
  leitura do cliente (`read_to_end`). O §8 dizia «fica o `TETO_DO_PACK`», mas
  isso foi escrito antes dos 100 MiB: com ele, uma imagem de 80 MiB passaria
  pela espera e morreria no fio.
- **O teto continua por tipo, antes do primeiro byte.** Antes do sniff vale o da
  espera: o maior tipo que ela aceita (100 MiB com imagem, 64 só com
  `mrpack`), contra o total declarado e no corte. **Lidos os 12 bytes do sniff,
  vale o do tipo**: o total declarado acima dele morre ali, antes de o primeiro
  byte ir ao disco (`AcimaDoTeto`, com o tamanho e o teto). Para isso o sniff do
  fluxo da 0.19 (dc75be7e) reconhece, além das quatro imagens, o começo de um
  zip (`PK\x03\x04`) quando a espera aceita `mrpack`; o índice é conferido no
  fim, como na parte 6. Se errada: um `.mrpack` com dados antes do primeiro
  cabeçalho local (um zip autoextraível) é recusado como formato desconhecido.
- **O seletor faz o mesmo**: `teto_do_pedido` é 64 MiB quando o MOD pede só o
  papel `pack`, e 100 quando pede o pack e outro papel; `tipo_do_escolhido_para`
  confere o teto do tipo nos bytes, o do pack inclusive.
- **A `api/v8.json` passa a dizê-lo** (emenda em `congeladas.sha256`), e o guia
  do autor também.

## As telas da API 8 no desenho 0.19

- **O aceite da máquina é a caixa do aceite dos MODs** (`Mods-aceite.dc.html`):
  opaca no `negro-painel` (o `#12100E` do canvas não é token), 32px de raio,
  a linha miúda com a marca, o MOD e o servidor (dados do pedido, sem frase
  nova) e as portas num chip; o título grande em Inter Tight (32px), **em
  caixa de frase** como todo título do canvas — a frase era escrita em
  maiúsculas (`FRASES_DA_MAQUINA.titulo`) e o CSS ainda a forçava; agora é
  «Fulano pede para rodar um servidor de jogo neste computador», as mesmas
  palavras, e a bancada a lê assim. Numa janela baixa (até 760px de altura) o
  título desce a 24px e os dois avisos encolhem o respiro, para sobrar corpo;
  os dois seguem inteiros e à vista; o aviso da
  conta num cartão laranja com o escudo traçado, **fora da área que rola**,
  como antes; «isto mudou desde o seu aceite» na caixa laranja do canvas (era
  azul); o que vai rodar num cartão de vidro com o ícone traçado e as linhas
  com seta; cada termo num cartão; as ações à direita, o vidro e o laranja.
  Nenhuma frase mudou.
- **As duas faixas são a faixa da Porta** (`Porta.dc.html`): 18px de raio, a
  marca na frente, Inter, a ação em pílula de vidro na ponta. A da máquina é
  verde de pé, vidro neutro quando a parada foi pedida (por você, pelo
  servidor, ao sair) e **vermelha quando ninguém a pediu** (a metade falhou, o
  pacote não confere, um motivo desconhecido, sem aceite) ou quando DESLIGAR
  falhou — por isso o bloco ganhou `data-motivo`. Opaca, porque é fixa e
  cobre o que passa por baixo. A das portas é verde aberta e vermelha fechada
  ou recusada; as notas (a porta trocada, a queda) vão embaixo, alinhadas ao
  texto.
- **A lista de jogos aceitos nas configurações** já usava as linhas de
  gestão da 0.19; o estado virou um chip (verde ligada, neutro desligada), em
  minúsculas porque é a frase do produto e as bancadas a leem.
- **`familia: 'sans'` passou a valer.** Desde a API 4 ela virava
  `var(--seele-sans)`, que nunca existiu: o estilo era `unset` e o texto do
  MOD herdava o mono da casca, sem aviso. Agora é `--seele-pilha-sans`, e
  `toda_variavel_do_estilo_de_mod_esta_declarada` exige que toda variável das
  palavras do estilo esteja declarada (reprova com a linha velha). Se errada:
  um MOD que pedia sans e se acostumou ao mono muda de letra.
- **Sem um nome `alerta` no tema da API 8.** `TEMA_DA_API` serve às duas
  portas do tema — a que cita (`{ tema }`) e a que publica (`mundo.tema`,
  `SeeleUI.tema`) —, e um nome novo nela vira chave que um MOD de tema escreve,
  com contraste e derivação próprios; o guia diz que o vermelho é do produto,
  derivado do tema, e não chave dele. Não seria só aditivo. O MULTIPLAYER usa o
  vermelho do canvas em hexadecimal. Se errada: a recusa de um MOD não segue o
  vermelho que o produto deriva sob um tema; o caminho é um nome só de
  citação, por API.

## A parte 7 (a reserva) sobre a 0.19

- **Fundida em b3ef3a24** (`ee29f50c` e `f26c1051`). O único conflito de texto
  foi `api/congeladas.sha256`: as duas notas de emenda ficaram (a dos tetos da
  0.19, depois a da reserva) e a linha da v8 foi refeita sobre a `v8.json`
  fundida, que segue com a imagem em 100 MiB. **R0 conferido**: nada mudou em
  `seele-proto`, `seele-web` nem nos bytes-ouro desde `c9e908a4`. O prelúdio
  segue com 16 tipos da máquina. Se errada: nenhuma conhecida.
- **A reserva no desenho 0.19** é do MOD (`DECISOES.md` dele): chips para o
  aceite, a dona, a passagem e a cópia; o tempo perdido no aviso laranja do
  canvas; PASSAR como controle do produto. Nada da tela do SEELE mudou na parte 7.

## Sem quem hospeda (depois da parte 7)

Decisões inteiras em `2026-10-31-api-8-sem-anfitriao-decisoes.md`. Em uma linha
cada:

- **M18 revista (S1–S5):** a metade de máquina sobrevive à queda do enlace,
  órfã; o pedido igual ao aceito a religa sem subir de novo, e o
  `maquina.aoReconectar({ fora })` dela serve de novo o túnel e pergunta a
  posse; a órfã que ninguém chama em 10 min para com `sem-volta`. Se errada:
  uma máquina sem sessão gasta até 10 min de CPU.
- **Os túneis (S6–S9):** sem quem hospeda, o direto segue e a estrela acaba
  com `AnfitriaoSaiu`; nada novo abre sem ele; na volta, quem joga pede entrada
  de novo e a máquina dá 30 s para cada túnel ser apresentado de novo. Fechar o
  SEELE de propósito ainda corta todos. Se errada: quem perdeu a permissão com
  quem hospeda fora joga pela malha até ele voltar.
- **O livro (S10):** chamar quem não está é lembrado, e `MaquinaVoltou` chega
  quando a pessoa chega. Se errada: um momento a mais à metade de servidor.
- **A faixa (0.19):** a órfã é a faixa da Porta em laranja (`sem-anfitriao`,
  com a frase de até quando ela espera), e `sem-volta` é vermelha, entre as
  paradas que ninguém pediu. A classe nova é `.maquina-sem-anfitriao` (a
  `.maquina-aviso` já era do aceite), e `celular-texto.css` foi regerado.

## O modal de um MOD com a gramática dos modais do app (31/10, pedido do dono)

- Ruling: o diálogo modal comum de um MOD (não a tela, não o celular) usa a caixa dos modais do produto (`camada-maquina.css`, o aceite): uma superfície opaca, 32px de raio, 36/44 de respiro, sem faixa de cabeçalho nem de rodapé; a linha miúda com a marca do SEELE, a origem e o FECHAR; o título em Inter Tight 32; as ações à direita no pé — porque o dono viu o Multiplayer «não conversar» com os outros modais — se errada: um MOD que contava com o corpo preto (`negro-absoluto`) sob o conteúdo vê o painel (`negro-painel`).
- Ruling: `superficie-titulo` atualiza também o `aria-label` da superfície — o leitor de tela anunciava o título da criação depois de o MOD trocá-lo — se errada: nenhuma conhecida.
- Ruling: o furo do véu (API 6 §10) fica: o modal de um MOD nunca cobre a barra do operador, e por isso começa à direita dela, e não no centro da janela como os modais do produto — o dono decidiu (31/10) manter a barra livre em vez de centralizar cobrindo o microfone e o sair — se errada: o modal do MOD segue um pouco à direita do centro.
- Medido: `bancada/nucleo.cjs` e `bancada/tema-do-servidor.cjs` falham também na `entrega/0.19` (74732140), antes e depois desta mudança; não são desta mudança.
