# As medições do dono — F4, o web app do celular

As medições 3 a 6 do ADR 0057 («O que fica pendente») só um aparelho responde.
Este documento diz, para cada uma:
- o que é preciso ter;
- o que fazer, passo a passo;
- o que anotar.

Também traz duas confirmações curtas que já estão prontas para rodar: a da
medição 1 no iPhone, e o custo do executor de MODs no iPhone.

**Tudo o que roda aqui é spike descartável**, em `spikes/`. Nada disso é o
produto, e nenhuma destas páginas fala o protocolo do `seeled`.

**Uma origem provisória basta para medir.** O domínio do web app (D-web-2) é
uma decisão que não tem volta, porque as identidades do celular vão morar nele.
Ele não precisa estar decidido para estas medições.

Para medir, a origem só precisa:
- servir arquivos estáticos;
- ter HTTPS com certificado válido.

Não importa:
- se o host deixa definir cabeçalhos;
- se ele injeta script de terceiros;
- se o endereço é permanente.

As identidades criadas nas medições são de teste e ficam nela. **Não use a
origem provisória para o produto depois**, porque ela já terá identidades de
teste.

## Antes de tudo

- **O repositório**, no branch `0.16/web` (ou em `versao/0.16`, depois do
  merge). Rode a compilação de cada spike uma vez, no Mac:

  ```sh
  (cd spikes/ed25519-do-navegador && cargo build --release)
  (cd spikes/alpn-no-mesmo-socket && cargo build --release)
  (cd spikes/voz-no-navegador && cargo build --release && python3 empacotar_origem.py)
  (cd spikes/mods-no-navegador/motor && cargo build --release --target wasm32-wasip1)
  ```

  O último compila o motor de MODs para `wasm` e, na primeira vez, baixa o
  wasi-sdk (cerca de 100 MB comprimido, 462 MB descompactado, dentro de
  `target/`). Ele precisa de `rustup target add wasm32-wasip1`.
- **O iPhone e o Mac na mesma rede Wi-Fi**, para as medições de bancada (1, 3 e
  o custo dos MODs). O endereço que os scripts imprimem é o IP do Mac na LAN.
- **O aviso de certificado.** As páginas de bancada vêm do Mac por HTTPS com um
  certificado autoassinado. O Safari avisa uma vez: «Mostrar Detalhes →
  visitar este site → Visitar Site». **O aviso é da página, e não do que se
  mede**: o WebTransport entra pelo hash e não depende dessa exceção.
- **O firewall do Mac** estava desligado em 08/10. Se estiver ligado, o macOS
  pergunta se o binário pode receber conexões: responda que sim.
- **Uma porta ocupada.** Em 08/10, um `spike-voz-no-navegador` aberto em 07/10
  ainda estava escutando a porta 4433 no checkout principal (PID 70937). Os
  roteiros abaixo usam a 4433 por padrão. Antes de começar:

  ```sh
  lsof -nP -iUDP:4433
  ```

  Se aparecer um servidor velho, feche-o, ou passe `--porta 4434` ao binário.
- **Os resultados** ficam em arquivos `resultados/` ou `relatorios.jsonl` de
  cada spike. Eles têm endereço e user agent do aparelho, e por isso não vão para
  o git. O que importa vai para o README do spike.

---

## Medição 1, confirmada no iPhone — a assinatura passa no `verify`

**Já medida** no Mac, no Chromium, no Safari 27.0.1 do Mac e no Safari do
Simulador: passa (`spikes/ed25519-do-navegador/README.md`). No iPhone é uma
confirmação de 2 minutos, porque é o mesmo WebKit 27.

**O que é preciso ter:** o iPhone na mesma Wi-Fi do Mac.

**O que fazer:**
1. No Mac:

   ```sh
   cd spikes/ed25519-do-navegador
   python3 prova.py iphone
   ```

2. No iPhone, abra no Safari o endereço impresso (`https://<ip-do-mac>:8093/?nav=iphone`).
   Passe pelo aviso de certificado.
3. A página roda sozinha: assina, guarda a chave, recarrega e assina de novo.
   O terminal imprime o resumo quando as duas fases voltam (até 10 minutos de
   espera).

**O que anotar:**

| | resultado |
|---|---|
| a versão do iOS (Ajustes → Geral → Sobre) | |
| `familias`: quantas passaram no `verify`, de quantas (esperado 184/184) | |
| `distintas_para_a_mesma_mensagem` (esperado 10: assinatura aleatória) | |
| `adulteradas.aceitas` (esperado 0) | |
| `fase2_leu_a_chave` e `fase2_mesma_publica` (esperado `true` e `true`) | |

---

## Medição 3 — o ALPN no iPhone

**O que ela decide** (ADR 0057, emenda ao 0003):
- se o certificado curto pode ter a **mesma chave** do longo;
- quantos hashes um link pode carregar;
- se os temporizadores do `seeled` (20 s de ocioso, 5 s de keepalive) seguram
  a página oculta;
- se um servidor fora do ar se distingue de uma recusa.

**A bancada já mediu** no Chromium e no Safari do Simulador
(`spikes/alpn-no-mesmo-socket/README.md`):
- a mesma chave abre, e 500 hashes também;
- no Safari, a recusa e a porta fechada chegam iguais, e só o silêncio (30 s)
  se distingue;
- um navegador morto sem fechar cai em 24,6 s.

Falta o aparelho.

**O que é preciso ter:**
- o iPhone na mesma Wi-Fi do Mac, com bateria e cerca de 30 minutos;
- **um IP da rede sem máquina nenhuma**, para o caso «ninguém». Um endereço
  fora da faixa que o roteador distribui serve, por exemplo o `.250` da rede, se
  ninguém o usa. Na dúvida, rode no Mac `ping -c 2 <ip>`: o IP serve se ninguém
  responder;
- alguém, ou o próprio dono, para bloquear e desbloquear a tela nos tempos
  pedidos.

**O que fazer:**
1. No Mac:

   ```sh
   cd spikes/alpn-no-mesmo-socket
   python3 iphone.py
   ```

   Ele sobe o servidor da medição (portas UDP 4500 a 4504) e a página (HTTPS,
   porta 8443), e imprime o endereço.
2. No iPhone, abra no Safari o endereço impresso e passe pelo aviso de
   certificado. A página diz «modo: aba do navegador» e, embaixo, as portas e
   «mesma chave = true».
3. Deixe **requireUnreliable** marcado (é como o produto vai sair).
4. Toque em **1 · os casos**. Anote as cinco linhas.
5. Toque em **2 · a lista longa**. Anote até onde abriu.
6. Escreva no campo o **IP sem ninguém** e toque em **3 · fora do ar contra a
   recusa**. Pode levar até 2 minutos, e a tela não deve bloquear nesse tempo.
   Anote os tempos.
7. **A página oculta, sem áudio:** toque em **4 · ficar aberto, sem áudio**.
   Espere a linha «abrir · abriu».
   - Bloqueie a tela por **30 s**, desbloqueie, e anote o que a página e o
     terminal do Mac disseram. O terminal diz «fechou A … durou … · motivo»
     quando o servidor desiste.
   - Toque em **parar** e repita com **2 minutos** de tela bloqueada.
8. **A página oculta, com o áudio do produto:** toque em **5 · ficar aberto, com
   o áudio do produto** e permita o microfone. Ouve-se um tom baixo, que é
   proposital: é a saída pela tag `<audio>` da rodada 5.
   - Bloqueie por **2 minutos**, desbloqueie, anote.
   - Repita com **5 minutos**.
9. **O web app** (opcional, e mais valioso): no Safari, use «Compartilhar →
   Adicionar à Tela de Início». Abra pelo ícone. A página deve dizer «modo: web
   app da Tela de Início», e o aviso de certificado pode voltar. Repita os
   passos 4, 7 e 8.
10. No Mac, `Ctrl+C`. Os arquivos ficam em `spikes/alpn-no-mesmo-socket/resultados/`
    (`iphone.jsonl` e `servidor-iphone.jsonl`).

**O que anotar:**

| caso | aba | web app |
|---|---|---|
| iOS e modelo do iPhone | | |
| A · curto da **mesma chave**: abriu? em quantos ms? | | |
| A · um bit trocado: recusado? | | |
| A2 · chave própria (controle): abriu? | | |
| B · só o longo: recusado? | | |
| lista longa: até quantos hashes abriu? | | |
| recusa: os três tempos | | |
| porta fechada: os três tempos | | |
| mudo: o tempo | | |
| ninguém (`<ip>`): o tempo | | |
| sem áudio, 30 s bloqueado: a sessão sobreviveu? o que o terminal disse? | | |
| sem áudio, 2 min bloqueado: idem | | |
| com áudio, 2 min bloqueado: sobreviveu? os pings continuaram? | | |
| com áudio, 5 min bloqueado: idem | | |

**Como ler o resultado:**
- **A mesma chave recusada e o A2 aceito:** cada janela de certificado ganha
  chave própria, e a lista de hashes vira a única âncora no navegador. O ADR
  0057 precisa escrever esse custo antes do código.
- **Uma lista curta:** o número entra no item 17 do contrato de fio, a mensagem
  dos hashes.
- **A sessão cai bloqueada mesmo com áudio:** a reconexão passa a ser o caminho
  normal do celular, e não a exceção.
- **Fora do ar com o mesmo tempo da recusa:** a tela não tem como separar os
  dois, e diz isso. É o que a bancada já sugere.

---

## Medição 4 — o web app instalado, na origem pública, em 4G

**O que ela decide:**
- se a voz segura **30 minutos** de tela bloqueada, em **4G**, e quanto de
  bateria isso custa (o aceite do M6, `specs/09-roadmap.md:103`);
- **que URL o iOS grava** no ícone da Tela de Início;
- se a **`CryptoKey`** sobrevive a fechar o web app e a reiniciar o iPhone.

**O que é preciso ter:**
- **uma origem provisória**, com HTTPS válido, onde subir a pasta
  `spikes/voz-no-navegador/origem/`, na raiz ou num subcaminho. Basta servir os
  arquivos como estão;
- **um servidor alcançável da internet por UDP**: uma máquina com IP público, ou
  com a porta UDP 4433 aberta e encaminhada. **O Mac atrás de CGNAT não serve**,
  porque o celular em 4G não o alcança. A VPS é de produção, e usá-la é decisão
  do dono;
- o binário `spike-voz-no-navegador` compilado **para o sistema dessa máquina**
  (`cargo build --release` nela);
- o iPhone com 4G, com a carga anotada, e de preferência carregado;
- **alguém do outro lado** para a voz nos dois sentidos: o Mac ou outro
  aparelho abrindo o mesmo link no navegador.

**O que fazer:**
1. No Mac, `python3 empacotar_origem.py` em `spikes/voz-no-navegador/`. Suba a
   pasta `origem/` inteira para a origem provisória. Confira no navegador do Mac
   que `https://<origem>/` abre a página «Voz no navegador».
2. Na máquina pública:

   ```sh
   ./target/release/spike-voz-no-navegador --origem https://<origem> --identidade identidade --publico <ip-publico>
   ```

   Copie o **link público** que ele imprime e guarde num lugar de onde dê para
   copiar no iPhone (Notas, por exemplo). Com `--identidade`, o link vale 12
   dias, mesmo que o servidor seja reiniciado.
3. **No iPhone, desligue o Wi-Fi.** Abra o link no Safari, que abre uma aba.
   Anote:
   - o «Aberta em»;
   - a linha «Servidor» (esperado: «veio do fragmento»);
   - se o fragmento sumiu da barra.
4. Ainda na aba, toque em **criar identidade** e depois em **pedir
   persistência**. Anote a impressão (os 16 hex) e a resposta.
5. **Instale:** «Compartilhar → Adicionar à Tela de Início». Abra pelo ícone.
   Anote:
   - «Modo» (esperado: web app);
   - «Aberta em»: **é a URL que o iOS gravou no ícone**;
   - a linha «Servidor». O esperado é «sem servidor: cole o link», porque no
     iOS o web app não divide o armazenamento com a aba;
   - a linha «Identidade». O esperado é «ausente», pelo mesmo motivo.
6. Cole o link no campo e toque em **usar este link**. Toque em **criar
   identidade** e **pedir persistência** de novo, agora no web app. Anote.
7. **Variante da URL do ícone:** apague o ícone. Abra no Safari o link com
   `?manter=fragmento` antes do `#`, assim: `https://<origem>/?manter=fragmento#a=…`.
   Instale de novo, abra pelo ícone, e anote o «Aberta em». Ele diz se o iOS
   grava o fragmento quando a página não o apaga.
8. **Os 30 minutos:**
   - no web app, toque em **4 · as duas juntas**, depois em 1 · Conectar,
     2 · Ligar áudio e 3 · Falar;
   - do outro lado, abra o mesmo link (no Mac, o Chrome serve) e fale;
   - **anote a bateria**, bloqueie o iPhone pelo botão lateral e deixe 30
     minutos;
   - no meio do tempo, fale do outro lado e confira se o iPhone toca;
   - desbloqueie, toque em **enviar relatório** e anote a bateria.
9. **A identidade fechando o web app:** feche o web app (arraste para cima no
   seletor de apps) e abra pelo ícone. Anote a linha «Identidade» (presente?
   mesma impressão? assina? qual o nº da abertura?).
10. **A identidade reiniciando o iPhone:** reinicie e abra pelo ícone. Anote de
    novo.

O servidor grava tudo em `relatorios.jsonl`, na pasta onde rodou: um relatório
a cada 5 s, com a origem e a identidade. O terminal imprime a cada 5 s quantos
datagramas cada sessão mandou. **Com a tela bloqueada, é o terminal que diz se
a voz continuou.**

**O que anotar:**

| | resultado |
|---|---|
| iOS e modelo; operadora e sinal (4G/5G) | |
| aba: «Aberta em», «Servidor», o fragmento sumiu? | |
| aba: a impressão da identidade; a resposta do `persist()` | |
| web app: «Modo», **«Aberta em» (a URL do ícone)**, «Servidor», «Identidade» | |
| web app com `?manter=fragmento`: «Aberta em» | |
| web app: a impressão nova; a resposta do `persist()` | |
| 30 min bloqueado: o iPhone ouviu? falou? o terminal mostrou datagramas o tempo todo? | |
| bateria antes / depois | |
| fechar e reabrir o web app: identidade presente? mesma impressão? assina? nº da abertura | |
| reiniciar o iPhone: idem | |

---

## Medição 5 — o Chrome do Android

**O que ela decide:** se a voz do web app sobrevive à tela bloqueada no Android.
Pela leitura do código do Chromium em 07/10, provavelmente não sobrevive: a
captura em segundo plano está atrás de uma flag desligada. Se não sobreviver, a
tela do Android diz «voz só com a tela ligada» e pede Wake Lock.

**O que é preciso ter:**
- um Android com o Chrome atualizado;
- o mesmo servidor e a mesma origem da medição 4. Na mesma Wi-Fi do Mac, o link
  de LAN da medição 6 também serve.

**O que fazer:**
1. Abra o link no Chrome e anote «Modo» e «Servidor».
2. Toque em **criar identidade** e **pedir persistência**.
3. Instale («⋮ → Instalar app» ou «Adicionar à tela inicial») e abra pelo
   ícone. Anote «Modo», «Aberta em», «Servidor» e «Identidade». No Android, a
   aba e o app instalado podem dividir o armazenamento: veja se a identidade da
   aba aparece.
4. Rode **4 · as duas juntas** (conectar, áudio, falar), com alguém do outro
   lado. Bloqueie a tela por **2 minutos**, depois por **5**. Anote se ouviu e
   se falou. O terminal do servidor mostra os datagramas.
5. Repita com o **padrão (a rodada 1)**, para comparar.
6. Feche o app, abra de novo, e anote a identidade.

**O que anotar:**

| | resultado |
|---|---|
| modelo, versão do Android e do Chrome | |
| as capacidades que a página lista (WebTransport, Opus, AudioWorklet…) | |
| «Modo», «Servidor», «Identidade» na aba e no app instalado | |
| 2 min bloqueado: ouviu? falou? datagramas no terminal? | |
| 5 min bloqueado: idem | |
| fechar e reabrir: identidade presente? mesma impressão? | |

---

## Medição 6 — a página da origem pública discando para a LAN

**O que ela decide:** se uma página que vem da origem pública consegue abrir
WebTransport para um IP privado (`192.168.x.y`). O Chrome 147 pede permissão de
rede local para isso, e ninguém mediu o Safari. Se o navegador recusar ou
perguntar, o convite de LAN vira um caso que a tela precisa explicar.

**O que é preciso ter:**
- a mesma origem provisória da medição 4, com a pasta `origem/` já publicada;
- o Mac e o iPhone (ou o Android) na mesma Wi-Fi.

**O que fazer:**
1. No Mac:

   ```sh
   cd spikes/voz-no-navegador
   ./target/release/spike-voz-no-navegador --origem https://<origem> --identidade identidade
   ```

   Copie o **link de LAN**: `https://<origem>/#a=192.168.x.y:4433&h=…`.
2. No iPhone, na Wi-Fi, abra o link no Safari e toque em **1 · Conectar**.
   Anote se abriu, se apareceu alguma pergunta (e o texto exato dela) e o erro,
   se houver.
3. Repita no web app instalado: cole o link de LAN.
4. Repita no Chrome do Android, se houver um.

**O que anotar:**

| | iPhone · aba | iPhone · web app | Android · Chrome |
|---|---|---|---|
| abriu? em quantos ms? | | | |
| apareceu pergunta de rede local? o texto | | | |
| o erro, se não abriu | | | |

---

## O custo do executor de MODs, no iPhone

**O que ele decide:** se a forma recomendada pelo spike dos MODs (a AB: o motor
num Worker dentro de um `iframe` opaco) custa o aceitável num iPhone. Se custar
demais, a saída é a A. A spec pede o custo «medido num iPhone»
(`spikes/mods-no-navegador/README.md`).

**O que é preciso ter:** o iPhone na mesma Wi-Fi do Mac, e o motor compilado
(ver «Antes de tudo»).

**O que fazer:**
1. No Mac:

   ```sh
   cd spikes/mods-no-navegador
   python3 prova.py iphone
   ```

2. No iPhone, abra o endereço impresso (`https://<ip-do-mac>:8443/?nav=iphone&fase=1&so=custo`)
   e passe pelo aviso de certificado. A página roda sozinha, e leva cerca de 1
   minuto. Ela escreve «pronto» no fim, e o terminal imprime o resultado.
3. Se der, repita no web app instalado.

**O que anotar** (do JSON que o terminal imprime, em `custo`):

| | A | B | AB |
|---|---|---|---|
| `subida.ms` | | | |
| `api_igual_ao_desktop` (esperado `true`) | | | |
| `cpu_1e6_iteracoes_ms` | | | |
| `json_200_arvores_ms` | | | |
| `dez_ate_pronto_ms` | | | |
| `volta_ms` p50 / p95 | | | |

O MOD hostil não roda no iPhone: as canárias ficam no Mac, fora do alcance
dele, e ele mediria a rede do iPhone em vez do executor. A memória residente
também não tem como ser medida pela página no iPhone.

---

## A tela pelo PC: o que o Chrome, o Edge e o Firefox codificam (Tarefa 3.10)

**Por quê.** No PC, o web app compartilha a tela (D-web-3) com o
`VideoEncoder` do navegador, e o desktop decodifica com o OpenH264. No Chromium
e no Safari do Mac a medida já foi feita (`spikes/tela-do-navegador/README.md`):
- o perfil que serve aos dois é o Baseline restrito 4.0, em Annex-B;
- **o `isConfigSupported` do Safari mente**: ele diz que aceita perfis que não
  produzem nada.

Falta o Chrome, o Edge e o Firefox de verdade, porque o Chromium aberto não é o
Chrome.

**Passos, num Mac ou num PC com o navegador instalado:**
1. Na raiz do repositório:

       python3 spikes/tela-do-navegador/servir.py /tmp/tela-nav manual

2. Abra o endereço que ele imprime em cada navegador, um de cada vez, e espere
   o «feito» na página. O resultado sai no terminal, em JSON.
3. Para cada navegador, anote **o que codificou, e não só o que o
   `isConfigSupported` disse**: a lista `fluxos`, com as unidades e o `erro`.
4. Os fluxos ficam em `/tmp/tela-nav/<navegador>-<perfil>.bin`. Mande a pasta:
   o OpenH264 os lê com o exemplo de `spikes/tela-do-navegador/`.

**O som do `getDisplayMedia`** (à mão, porque pede um gesto e a escolha na caixa
do navegador). Em cada navegador, numa página qualquer, cole no console:

    navigator.mediaDevices.getDisplayMedia({ video: true, audio: true })
      .then(s => console.log("som:", s.getAudioTracks().length))

Escolha uma aba que esteja tocando som, depois a tela inteira, e anote o número
que aparece. Zero quer dizer que aquele navegador não dá o som daquela fonte.

## O eco no navegador: o PC e o iPhone (Tarefa 3.3, pedido da F8)

**Por quê.** O aviso de fone das duas cascas depende de uma pergunta: o
cancelador de eco do navegador cancela o som que a voz do SEELE toca? Até a
medida, as duas cascas pedem o fone sem afirmar o que o navegador faz. Uma
frase que afirmasse o cancelamento sem medida convenceria a tirar o fone
justamente quem faz o outro lado se ouvir. O protocolo está em
`spikes/eco-no-navegador/README.md`.

**O que precisa:** sem fone, num lugar quieto, com o volume do aparelho em uns
dois terços. A página toca um tom de 1 kHz por uns 25 s.

**Passos no Mac**, para o Chrome, o Safari e o Firefox, um de cada vez:
1. Na raiz do repositório:

       python3 spikes/eco-no-navegador/servir.py mac

2. Abra o endereço que ele imprime, aperte MEDIR, permita o microfone e fique
   em silêncio até a página dizer «o resultado foi para o Mac».
3. O veredito sai no terminal e na página. Se sair «sem laço», suba o volume e
   meça de novo: essa medida não diz nada.

**Passos no iPhone**, na mesma rede do Mac:
1. `python3 spikes/eco-no-navegador/servir.py iphone`
2. Abra no Safari do iPhone o endereço `https://…:8443/` que ele imprime. O
   Safari avisa do certificado uma vez: «Mostrar detalhes → visitar este site».
3. Aperte MEDIR, permita o microfone, e espere o resultado, como no Mac.
   **Não vale no Simulador**: ele usa o áudio do Mac.

| navegador | veredito | retorno sem (dB) | retorno com (dB) |
|---|---|---|---|
| Chrome (Mac) | | | |
| Safari (Mac) | | | |
| Firefox (Mac) | | | |
| Safari (iPhone) | | | |

## Depois

Mande as tabelas preenchidas, e os arquivos de resultado se quiser, à
orquestradora. Cada medição muda tarefas do plano da F4
(`docs/superpowers/plans/2026-10-08-0.16-F4-web.md`, Etapa 1), e a orquestradora
repassa.
