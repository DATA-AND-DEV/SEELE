# 0.19 — Limpeza de textos: levantamento

Levantamento das frases que a pessoa vê em `apps/seele-app/ui/`, com o
resultado da limpeza no fim (seção «Resultado»). Os números de linha das seções
A–G são do levantamento; o `index.html` foi redesenhado depois, e os itens dele
foram achados pelo texto.

Legenda: V = cita versão interna · J = jargão técnico · L = longo · D = repetido.

## Critério proposto

1. Nenhuma versão interna («0.15», «0.17», «Servidor 2.0») na frase: diga «antigo» ou «outra versão».
2. Nenhum jargão de rede ou codec (STUN, NAT, WebRTC, codec, HandshakeTimeout, «aperto de mão»).
3. Uma frase do problema e uma do que fazer. O porquê fica na Ajuda.
4. O mesmo caso, a mesma frase, em um só lugar (`frases.js`).

## A. Entrar pelo link no navegador (`frases.js`, FRASES_DO_TRANSPORTE)

| onde | problema | ação |
|---|---|---|
| frases.js:2879-2883, 2719, 2718; ponte-web.js:616 | V,J,D — quatro jeitos de dizer «servidor antigo» | uma só: «ESTE LINK É DE UM SERVIDOR ANTIGO. Abra no app do SEELE.» |
| frases.js:2848 | J — STUN, UDP | «Esta rede bloqueou a conexão. Tente de outra rede, como a do celular.» |
| frases.js:2842-2844 | J — NAT simétrico | «As duas redes não se alcançaram. Quem hospeda pode abrir a porta no roteador.» |
| frases.js:2822-2833 | L — três causas em lista | uma linha + «Peça um link novo.» |
| frases.js:2836-2838 | J — «furar o NAT» | remover a 2ª frase |
| frases.js:2839-2842 | J,L | «Peça um link novo a quem hospeda.» |
| frases.js:2860-2865 | J — HTTPS do ponto | remover os parênteses |
| frases.js:2866 | D — HandshakeTimeout | unificar com AnfitriaoCalado (2859) |
| frases.js:2875 | J — WebRTC | «recursos de conexão» |
| frases.js:2890-2893 | L | «Permita o acesso à rede local quando o navegador perguntar.» |
| frases.js:2723 | J,L | «Link incompleto. Peça o link de novo, copiado inteiro.» |

## B. Atualização e versões

| onde | problema | ação |
|---|---|---|
| frases.js:1930-1937 | V,J,L — 4 frases, «(WebRTC)» | 2 frases, sem WebRTC |
| frases.js:1931, 1932, 1937 | D — três repetem que a versão antiga continua | unificar |
| frases.js:710 | V,L — «mais velha que a 6», «Servidor 0.15 pelo launcher» | revisar junto do corte da API 6 |
| camada-mods.js:2263-2271; frases.js:843, 852, 1897 | V,L,D — «API X, que roda no Servidor 0.15» | uma frase, revisar com o corte da API 6 |
| frases.js:707, 709, 721 | J — texto para autor de MOD | encurtar |
| camada-versoes.js:104, 439; tela-boot.js:1588 | D — três variantes de «o que você escreveu aqui não aparece lá» | uma só |
| camada-versoes.js:80 | V | remover «No Mac Intel e no Linux, o Servidor 0.15 nunca teve pacote.» |
| tela-server.js:1595 | L | «A atualização não foi concluída.» + o que fazer |
| index.html:3835 | D — óbvio | remover |

## C. Voz, microfone e chamada

| onde | problema | ação |
|---|---|---|
| frases.js:426 | L | «O áudio não abriu. Saia da sala e entre de novo.» |
| frases.js:433 + pessoal-web-entrada.js:15 | D — a frase nova do «só ouvindo», em dois lugares | unificar em `frases.js` |
| camada-compartilhar.js:607 | L | «A captura inclui o som do SEELE; use fones ou compartilhe só uma janela.» |
| camada-compartilhar.js:234, 252 | L,D | uma frase |
| frases.js:1382 | J — codec | «módulo de vídeo» |
| index.html:4522 | L | «Baixe o módulo de vídeo para compartilhar.» |
| index.html:3777 | L — a 2ª frase repete a pergunta | encurtar |
| index.html:3800 | D | remover |

## D. Navegador e celular (`frases.js` 2510-2527, 2706, 2774)

- 2510, 2512, 2516, 2515, 2525, 2526 — D: seis jeitos de dizer «só no app»; unificar em «Disponível só no app do SEELE.»
- 2527 — L: encurtar.
- 2706 — remover «Instalar é opcional».
- 2774 — L: encurtar.

## E. Moderação, portaria, MODs e cargos

| onde | problema | ação |
|---|---|---|
| camada-moderar.js:812 | L,J | «Não foi possível contar as mensagens.» |
| camada-moderar.js:661, 615, 636 | L | encurtar |
| camada-portaria.js:332, 337, 366, 253 | L | 1 frase + consequência |
| camada-mods.js:1851 | L | «O servidor mudou. Nada foi gravado.» |
| camada-mods.js:1820 | D — repete index.html:4643 | remover |
| index.html:4640, 4643 | D | unificar |
| index.html:3689 | D | remover a última frase |
| frases.js:2292 | J,L | encurtar |
| frases.js:3078 | J — «EM 6 SEGUNDOS» | remover o prazo |

## F. Perfil e identidade

- tela-server.js:473, 476 e index.html:3435 — L,D: três explicações de chave/apelido; uma frase.
- frases.js:2760 — L: encurtar.

## G. Menores

- index.html:4416 — «aperto de mão».
- Cerca de 20 confirmações de 20 a 25 palavras (index.html; frases.js:2916-3043), dicas de tooltip (index.html:3222, 3281) e o rótulo «OPUS Nk» em tela-sessao.js:3491.


## Resultado da limpeza

Legenda: FEITO · MANTIDO (por quê) · ADIADO (API 6: reescrito junto do corte da API 6).

**A. Link no navegador** — FEITO. Um só «ESTE LINK É DE UM SERVIDOR ANTIGO / Abra no app
do SEELE.» (`SERVIDOR_ANTIGO` em `frases.js`, usado por `servidor-0.15`, `outraLinha`;
`ponte-web.js` diz o mesmo em minúsculas). Sem STUN, NAT, WebRTC, «furar»; `HandshakeTimeout`
passou a ser a mesma frase de `AnfitriaoCalado`; `incompleto` e `vencido` encurtados;
`redeLocal` e `SoPeloEncontro` mantidos. MANTIDO: `SoPeloEncontro` (frases.js ~2447) ainda
diz «NAT simétrico» e a porta, porque `bancada/link-do-navegador.cjs:112` confere o aviso
e a porta UDP é o que a pessoa precisa abrir (listado para uma próxima passada).

**B. Atualização e versões** — FEITO: travessia (2 itens, sem WebRTC nem «Servidor 1.0/2.0»),
aviso de «muda a versão do servidor» (`tela-server.js`), instalador que não terminou,
linha do Mac Intel/Linux, as três variantes de «o que você escreveu aqui não aparece lá»
(agora `CONVERSAS_NAO_PASSAM`), «Servidor X» nas notas de conversas guardadas,
`RECUSA_VERSOES` (ponte-web.js). ADIADO (API 6): `RECUSAS_DE_PACOTE["api-too-old"]`,
`frases.js` ~843/852 («feito para a API X, que roda no Servidor 0.15», «hospede pelo Servidor
0.15»), ~1897 («o Servidor 0.15 roda as APIs…»), `camada-mods.js` ~2263. FEITO: textos
para autor de MOD (`permissoes-pedem-api-nova`, `permissoes-invalidas`, `alcances-demais`,
`alcance-grande-demais`). MANTIDO: `ESTE SERVIDOR É O SERVIDOR 0.15… use o SEELE 0.15`
(frases.js ~1052): o número é o remédio (qual versão instalar). MANTIDO: rótulos
«ABRIR O SERVIDOR X PARA HOSPEDAR» e `data-saida="servidor-0.15"` (seletor de versão,
bancada os usa). `index.html` «Instalar é opcional»: FEITO.

**C. Voz, microfone e chamada** — FEITO: `nao_abriu_no_prazo` («O ÁUDIO NÃO ABRIU. A conversa
por escrito funciona; …» — a reafirmação de que o texto funciona é o que o guarda
`fora_de_sala_a_tela_nao_diz_sem_audio…` protege); «só ouvindo» unificado em
`FRASE_DO_SO_OUVINDO` (frases.js), usado por `ABERTURA_DO_AUDIO.so_ouvindo` e por
`pessoal-web-entrada.js` (index.html carrega frases.js antes; a bancada
`pessoal-web-entrada.cjs` passou a servir `frases.js`); compartilhar (som do SEELE
na captura, permissão de tela); módulo de vídeo sem «codec»; rótulo CODEC/«OPUS Nk» do rodapé
virou VOZ/«64k». MANTIDO: o botão inerte «O SEELE já está tentando sozinho…» (explica por que
não há botão).

**D. Navegador e celular** — FEITO: seis variantes de «só no app» reduzidas a «Disponível só
no app do SEELE» onde a lista dá o nome do recurso; `compartilhar`, `testeDoMicrofone`,
`esperaPausa`, «Instalar é opcional».

**E. Moderação, portaria, MODs e cargos** — FEITO: contagem de mensagens («Não foi possível
contar as mensagens de #canal, e o canal não foi apagado.»), banimento por prazo,
`ConjuntoMudou` («O servidor mudou. Nada foi gravado.»), confirmação de aplicar MODs, nota
duplicada de APAGAR MOD removida do `index.html`, firewall (`frases.js`), «EM 6 SEGUNDOS».
MANTIDO (segurança, guardas pinam): expulsar, banir para sempre («Nenhuma tela deste produto
desfaz»), mover, apagar sala/canal, ADMITIR/RECUSAR/REVOGAR da portaria (impressão digital e
«ninguém pergunta de novo»), frases dos cargos (`cargos_ui.rs`).

**F. Perfil e identidade** — FEITO: `tela-server.js` (frase de chave/apelido, 2 variantes) e
`frases.js` («não guardou o seu apelido»). MANTIDO: `index.html` «Perder a chave cria outra
identidade; um administrador… precisará devolver seus acessos» (consequência).

**G. Menores** — FEITO: «aperto de mão» → «verificação» (Ajuda), «consentimento que não se
retira…» removido, «OPUS Nk». NÃO FEITO (por escolha): as ~20 confirmações de 20 a 25 palavras
e as dicas de tooltip: quase todas dizem a consequência de um ato destrutivo ou de segurança
(regra 5) e a maioria é pinada por guardas; ficam para uma passada com revisão de UX.

### Achados novos: `tela-sessao.js` e Rust

- `tela-sessao.js`: FEITO — «Dá para estar em um servidor/lugar por vez…» (o porquê) e «O campo do
  endereço fica vazio…» saíram das confirmações de trocar de servidor/ligar/ir à entrada; a
  frase de sala acima do limite de subida ficou mais curta. MANTIDO: veredito de impressão
  digital (primeiro contato, convite que não corresponde, lista guardava outra) — segurança.
- Rust (`seele-ffi`, `seele-core`, `seele-sessao`, `apps/seele-app/src`): `snapshot.notice`
  carrega só enums (`NoticeReason`/`AlertReason`), e a frase vem de `frases.js` (já coberta). O
  `operator_text` é texto de quem opera o servidor, não do produto. Strings do Rust mostradas a
  pessoas (`main.rs`: regra de firewall do Windows, «Encerre a hospedagem antes de apagar um
  servidor»; `servidores.rs`) já são uma frase do fato e uma do que fazer: MANTIDAS, nenhum
  arquivo Rust alterado.
