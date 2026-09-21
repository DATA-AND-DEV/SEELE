# Notas da versão — v0.14.0

> **Esta página é o texto para o corpo do release, e o release ainda não
> existe.** A v0.14.0 é um **candidato**. O que foi verificado está dito em cada
> seção, com o número dos testes; o que **não** foi está no fim desta página, e
> a lista é curta e honesta — nenhuma chamada entre duas máquinas nativas
> aconteceu nesta volta.
>
> A tag e a publicação são passo manual de quem opera.

_As mudanças de `v0.13.0` (commit `a6d5882`, publicada em 21/09/2026) até aqui.
O número de commits sai do `git log` na hora de marcar a tag; se este texto for
publicado depois, refaça a conta em vez de copiar uma que envelheceu._

---

## ✅ Esta versão fala com a anterior

**Protocolo 7, o mesmo da v0.12.x e da v0.13.0.** Nenhum commit tocou
`crates/seele-proto` desde a v0.13.0 além de um teto de tamanho, e a janela de
compatibilidade continua em 1. Um par v0.13.0 conversa com um v0.14.0 sem nada
de especial.

**E os MODs que você tem instalados continuam funcionando.** A API de MODs
continua sendo a 4, e a conferência continua não sendo igualdade: um pacote de
API 3 segue sendo executado por este build.

**Um detalhe de tamanho, para quem escreve MOD:** o teto de um arquivo de mídia
dentro de um pacote subiu de 1 MiB para 10 MiB. Um MOD que aproveite isso pede
um cliente v0.14.0 — num v0.13.0 o arquivo é recusado por tamanho. O catálogo
não tem campo de versão mínima de cliente, só de API, então **publique o SEELE
antes dos MODs que dependem disto**.

---

## O que motivou esta versão

Duas coisas, e elas não se parecem.

A primeira foi um relato de campo com um número errado dentro: fotos e banners
do PERFIS falhavam, e a mensagem falava de **4 MB** que ninguém tinha escrito
como limite. Puxar esse fio descobriu três tetos diferentes discordando entre si
e um quarto problema por baixo — a ponte de pedidos de MOD estourando o balde de
quadros do servidor, que é a seção «A falha que o produto sabia e não contava»,
mais abaixo.

A segunda foi a auditoria da v0.13.0 cobrando o que ela mesma tinha deixado: a
gestão de MODs aparecendo onde não devia, explicações estáticas repetidas em
três telas, e o compartilhamento de tela sem nenhuma escolha de resolução depois
que os quatro controles saíram.

---

## O compartilhamento de tela voltou a ter uma escolha

A v0.13.0 tirou os quatro controles de limite — nitidez-ou-movimento, altura,
quadros e teto de banda — e a decisão estava certa: eram quatro perguntas antes
de mostrar a tela. O que ela não previu é que **nenhuma** escolha também é uma
resposta errada, porque 1080p fixo não serve a toda casa.

Agora a caixa pergunta duas: **540p, 720p ou 1080p**, e **30 ou 60 quadros** —
e as duas trocas valem **durante a transmissão**, sem parar e recomeçar. A caixa
nasce em 720p a 60.

`Cadencia` tem 8 e 15 quadros também, e eles ficam fora da lista: existem como
piso automático, para onde a rede desce pela regra «a resolução segura, o quadro
cede». Oferecer 8 a quem escolhe é pedir que a pessoa escolha
«propositalmente travado».

### A escolha deixou de valer só no fim

Ela era **só teto**, e por isso não valia quase nada: a escada decidia o degrau
e a escolha só podia baixá-lo. Escolher 1080p numa sessão nova entregava 540p, e
a escada levava oito janelas de um segundo — mais, na prática — para chegar ao
degrau pedido.

Quem prendia era a perna de quem hospeda. O servidor não põe hipótese no fio, por
desenho: sem medida nem subida declarada, o `HostUplink` vai zero, o cliente lê
zero como ausência, e vale o padrão conservador de 2 Mbps — que dão teto de
1200 kbps, que compram 540p.

Agora, **e só quando não há número real**, esse padrão sai da resolução
escolhida. Havendo `HostUplink` maior que zero — medido ou declarado —, ele
manda, para mais **e** para menos: repetir um palpite por cima de uma medida
seria insistir no que a máquina já desmentiu. O fio não mudou; o cliente supõe
por conta própria, e ninguém promete banda que não conferiu.

A resolução continua sendo teto: a escada continua adaptativa e desce sozinha
quando o cano não compra o que foi pedido. `prioridade` continua `nitidez`, que
foi a correção cara da volta passada e não se mexeu aqui.

### E o piso de 1080p estava curto

Os números de que a sonda parte eram 3, 5 e 8 Mbps, escritos à mão — e o de
1080p **não comprava 1080p**: 8 Mbps dão teto de 4,8 e o limiar são 6,24. Quem
escolhesse 1080p continuaria em 720p, sem erro nenhum a mostrar.

Eles passam a derivar do próprio limiar, e viram 2,6, 4,65 e 10,4 Mbps. Os três
compram o degrau que prometem por construção, e no dia em que um limiar mudar o
piso o acompanha. É a armadilha que `TETO_ESTIMADO_PARA_1080P_BPS` escreve sobre
si mesma no arquivo ao lado: *«duas constantes arredondadas em separado
divergem; uma derivada da outra não pode.»*

**Por isso o padrão da caixa passou a ser 720p.** Com o piso, o padrão deixou de
ser preferência e virou suposição sobre a casa de quem hospeda: a 1080p ele
suporia 10,4 Mbps de todo mundo que nunca abrisse a caixa.

**Do lado do servidor, a hipótese continua em 2 Mbps** — e essa frase custou
uma bateria reprovada para ficar de pé.

Ela chegou a subir para 8 Mbps nesta mesma onda, pelo motivo certo: com 2 Mbps
o teto inicial compra 540p, e a resolução escolhida nasceria cortada. Só que a
subida cegou a medida. Num servidor sem ninguém compartilhando tela,
`permitido_bps` é zero e a **única** porta para uma medida é o piso
demonstrado, que exige entregar mais do que a hipótese. Uma sala de quatro
conversando entrega 6 a 7,2 Mbps: contra 2 Mbps ela ultrapassa e o servidor
aprende o cano; contra 8 Mbps, não — e aí nada chega ao portão de admissão, ao
fio, nem ao disco para o arranque seguinte lembrar.

E o degrau que a subida comprava não era o prometido. O teto é 60% do caminho,
e o limiar de 1080p são 6.240.000 bps, que pedem **10,4 Mbps** de caminho. A
8 Mbps o teto compra 720p; a 2 Mbps ele compra 540p no primeiro segundo e 720p
assim que a primeira janela mede. Era um degrau temporário, pago com a medida
permanente — e a escada dá o mesmo degrau sozinha.

Quem encontrou foram `tests/subida_no_arranque.rs`, que sobem quatro conexões
QUIC de verdade e levam sete segundos. O guarda que faltava entre mexer na
constante e esperar a bateria agora existe em unidade,
`a_hipotese_deixa_o_piso_demonstrado_disparar`, e roda em microssegundos.

---

## A falha que o produto sabia e não contava

O log local de 21/09, às 04:06 UTC, registrava `control frames over budget`
seguido de pedidos de `seele/perfis` sem resposta até o timeout. Quem usava via
outra coisa: a foto não subia, e a frase falava de 4 MB.

**A causa não era o tamanho.** O envio do PERFIS usa fragmentos base64 em
`ModRequest`, pelo controle — caminho diferente do transporte de anexos. Cada
fragmento podia sair assim que a resposta anterior chegasse, e isso ultrapassava
o balde de 20 quadros por segundo do servidor **mesmo abaixo de 1 MB**. O
servidor descartava, e o pedido morria esperando.

A ponte agora distribui **oito pedidos de MOD por segundo**, compartilhados por
todos os MODs, uploads e leituras. A espera acontece **antes** do prazo de
resposta — senão a fila consumiria o próprio prazo — e pedidos de sessões
encerradas são cancelados em vez de entregues a uma sessão que já acabou. O
servidor mantém os limites dele: nada aqui pede que ele afrouxe.

E os três tetos que discordavam foram alinhados: o seletor reaproveitava o
orçamento de mídia da região para limitar o campo de arquivo (daí os 4 MB),
havia um teto de 1 MiB na conversão nativa para exibição, e o máximo de
continuações não alcançava 10 MiB. Agora **10 MiB** na seleção, na leitura e no
orçamento de exibição.

O PERFIS passou a mostrar o estado do envio, impedir uploads concorrentes de
foto e banner, e liberar o arquivo escolhido inclusive quando o `upload-start`
falha.

**O que isto não faz:** não migra uploads de MOD para o fluxo binário de anexos.
O transporte fragmentado continua, e arquivos grandes ainda custam muitas
viagens pelo controle.

---

## Um GIF de Tenor, Giphy ou Imgur vira imagem na conversa

Colar um link **direto** de imagem sempre desenhou a imagem. Uma página que fala
de um GIF, não — e a razão era boa: desenhar um link é buscá-lo, e buscar é
aparecer. Buscar todo link colado entregaria o IP de quem lê a qualquer domínio
que aparecesse na conversa, inclusive a um posto ali só para colher endereços.

Só que `tenor.com/view/…` é uma página, e é assim que quase todo mundo manda um
GIF. O produto parecia não mostrar GIF nenhum.

A partir desta versão o Rust resolve a página até a mídia que ela declara —
`og:image` — para uma **lista fechada de três domínios**: `tenor.com`,
`giphy.com` e `imgur.com`. E:

- a mídia declarada pela página tem de morar nos **mesmos** três domínios, senão
  uma página desta lista escolheria qualquer endereço do mundo para esta janela
  buscar, e a lista fechada não teria fechado nada;
- as duas buscas — a página e a imagem — passam pela mesma porta, então as três
  travas valem para as duas: o esquema conferido, o teto aplicado **enquanto os
  bytes chegam**, e os bytes conferidos contra o tipo alegado;
- a comparação de domínio é por sufixo **com ponto**: `naotenor.com` não é
  Tenor, e `tenor.com.exemplo.br` também não;
- a lista mora só no Rust, e a janela a consulta por `dominios_de_gif`. Duas
  cópias divergiriam algum dia, e divergiriam oferecendo buscar o que o outro
  lado recusa.

**Nenhum domínio novo passa a ser buscado.** `exemplo.br/um-gif-legal` continua
sendo um link clicável.

**Verificado rodando**, e não só em texto-fonte: a bancada
`apps/seele-app/bancada/gif-na-conversa.cjs` carrega o `index.html`, as folhas e
os scripts reais num Chromium, manda um GIF animado de verdade e mede o que o
navegador desenhou. Ela prova o link direto, a página da lista, o anexo
`image/gif`, e os dois jeitos de errar a comparação de domínio — cada asserção
foi conferida revertendo o guarda que ela protege e vendo-a falhar.

---

## A gestão de MODs saiu de onde não devia estar

- Ela aparece **apenas nas configurações de uma sessão conectada**. Uma seção
  lembrada não reaparece na tela inicial, onde ligar e desligar MOD não
  significa nada.
- As explicações estáticas repetidas saíram das configurações e da gestão. O que
  fica é o que muda: estados, erros, consentimento e consequências de exclusão.
- A lista de instalados oferece **APAGAR MOD**. É preciso desligar e salvar
  antes: um pacote que **o servidor desta janela** exige é recusado com
  `exigido-por-este-servidor`, e a pergunta é feita no clique e não com o que a
  tela sabia — entre desenhar a lista e apertar o botão, outro operador pode ter
  ligado aquele pacote. Os **dados** que os MODs gravaram não são apagados junto
  com o pacote: bytes que se baixa de novo não levam junto o que ninguém tem
  como recuperar.
- A lista de hospedagem oferece **APAGAR SERVIDOR**, com confirmação e
  hospedagem parada. Apaga o banco e os dados daquela instância; preserva os
  outros servidores, os pacotes e a identidade TLS da máquina.
- Servidores **visitados** ganharam um **REMOVER** escrito por extenso, no lugar
  do `×` que não dizia o que fazia.

O caso difícil aqui é o servidor legado — o de quem hospedava antes de existir
lista. Ele era a origem da chave TLS das outras instâncias, então apagá-lo
poderia trocar a identidade da máquina inteira. A identidade passa para um banco
próprio antes de o legado sair, e há teste que apaga o legado, cria um servidor
novo e confere que a identidade é a mesma de antes.

---

## Para quem escreve MOD

Quatro acréscimos, todos declarados em `api/v4.json`:

- **`superficies.criar({ tipo: "pagina", imersiva: true })`** ocupa a sessão e
  mantém saída, Escape, foco e restauração. É o que a MESA usa para o objeto em
  tamanho real.
- **`listarNaBarra: false`** numa contribuição `servidor.navegacao` oculta a
  entrada. O padrão continua `true`. O ESTILO usa o resultado autenticado
  `canEdit` para decidir. **Visibilidade não substitui autorização**: esconder a
  entrada não impede ninguém de nada, e quem autoriza continua sendo o servidor.
- **Avisos expiram**: 3,5 s no normal, 10 s no erro. Descartar também libera o
  registro e o temporizador — antes um aviso descartado deixava os dois para
  trás.
- **`bytesDeMidia`** subiu em três lugares, e eles são diferentes: 10 MiB
  somados numa **região**, 20 MiB somados em todos os **cartões de pessoa** de
  um MOD (`api/v3.json` e `api/v4.json`), e 10 MiB por **arquivo de mídia**
  dentro de um pacote. O orçamento de uma superfície continua em 24 MiB.

O ESTILO deixou de emitir a confirmação flutuante redundante de publicação.

E um teto que **não** mudou, porque é outro: o que um MOD escreve como arquivo
de dados no servidor continua em 4 MiB por arquivo. É por isso que a MESA grava
e lê imagens **em partes**, mantendo a leitura do formato antigo.

---

## O que foi verificado

| | |
|---|---|
| `cargo test -p seele-app --test frontend` | 238 |
| `cargo test -p seele-app --bin seele-app` | 108 |
| `cargo test -p seele-core --lib` | 340 |
| `cargo test -p seele-core --lib preview::` | 10 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | limpo |
| `cargo xtask` check-api, check-deps, check-versao, check-vetores | passam |
| `npm test` nos três MODs | ESTILO 23 · PERFIS 40 · MESA 50 |

Bancadas de navegador: `gif-na-conversa`, `ajustes-v013`, `contribuicoes-e-camadas`,
`continuacao-de-midia`, `envio-de-imagens`, `regiao-do-mod` — todas passam.

A `ajustes-v013.cjs` roda em Chromium com HTML e CSS reais e ponte simulada, e
verifica gestão dentro e fora da sessão, resolução aplicada durante a
transmissão, navegação oculta, largura imersiva com Escape, e expiração e
liberação de avisos.

Uma correção na simulação, que vale registrar porque escondia um caminho
inteiro: a `telas.cjs` declarava `regras_de_previa` sem `image/gif` e com o
dobro do teto real. Nenhuma cena de lá teria visto um anexo de GIF deixar de
oferecer prévia.

---

## O que esta versão **não** entrega

- **Nenhuma chamada entre duas máquinas nativas foi feita nesta volta.** Tudo o
  que está acima foi verificado em teste e em bancada de navegador. A qualidade
  visual dos novos arranques de vídeo em rede real exige essa verificação, e ela
  não aconteceu.
- **O empacotamento de Windows e Linux não rodou** nesta volta.
- **Multiserver simultâneo, no modelo do Discord, não existe** — e não é
  esquecimento. `Session` guarda **uma** `Connection`, e `connect` devolve
  `AlreadyConnected` enquanto houver uma. A trilha da esquerda e a lista de onde
  você já esteve deixam trocar de servidor rápido, mas trocar é sair: a sala de
  voz cai, e não há aviso de mensagem num servidor onde você não está. Fazer o
  outro modelo é mexer no que `Session` guarda, não na casca.
- **Uploads de MOD continuam pelo controle**, em fragmentos base64. Arquivos
  grandes custam muitas viagens.
- **A subida declarada não está na interface.** `config.caminho_bps` já
  atravessa o fio e já vence o piso da escolha; falta só o campo numa tela. É a
  resposta certa para quem sabe quanto tem, e não entrou aqui porque esta volta
  existe justamente para não perguntar Mbps a quem não sabe.
- **O portão de admissão do servidor não foi separado da sonda.** Ele admite
  seis cópias com a hipótese de 2 Mbps; separar os dois números fica para quando
  alguém medir que ele atrapalha.
- **O CI do `main` está vermelho** em `test (linux)`, `test (windows)` e
  `clippy (windows)`, e estava vermelho também no commit de onde a v0.13.0 saiu.
  O clippy do workspace passa localmente no macOS, com a linha exata do CI.
  Ninguém investigou os três ainda.
