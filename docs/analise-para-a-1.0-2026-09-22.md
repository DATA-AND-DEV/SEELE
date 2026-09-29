# O SEELE a caminho da 1.0

Análise de 22/09/2026, feita sobre o HEAD `e2fac4d`, que **é** a v0.15.0
publicada. O corpo do release grava esse commit, e `git log` confere.

Foram nove frentes. As quatro primeiras são as features pedidas: persistência,
URL amigável, conexão simultânea e calls privadas. As outras cinco medem a
distância até uma 1.0: estabilidade, segurança, distribuição, a primeira noite
de um amigo e outras opções.

Cada frente foi lida por um analista e depois revista por um cético, que tentou
derrubar as afirmações de carga. Esse cético derrubou duas. Os achados que
sustentam as recomendações foram conferidos de novo à mão antes de entrar aqui
(ver [§1](#1-o-que-foi-medido-nesta-análise)). O material inteiro de cada
frente, com toda a evidência `arquivo:linha`, está em
[`analise-para-a-1.0-2026-09-22-anexo.md`](analise-para-a-1.0-2026-09-22-anexo.md).

---

## 0. Em uma página

**O SEELE tem mais construído do que a 1.0 precisa e menos provado do que a
1.0 exige.** Quase nada do que falta é funcionalidade grande. O que falta se
divide em três grupos:

- caminhos que o código tem e que não funcionam;
- promessas escritas que o produto não cumpre;
- uma decisão de compatibilidade que ainda não foi tomada.

Os cinco achados que definem a ordem do trabalho:

1. **O link que "volta a funcionar amanhã" não volta, e ninguém sabe.** O ADR
   0047 o dá como construído e diz que só faltava atualizar a VPS. A VPS foi
   atualizada: medi hoje, e `MORO`/`QUEM` respondem. Mas o caminho do cliente
   tem **três defeitos independentes**, todos silenciosos, presentes desde a
   v0.10.2. Para quem hospeda atrás de CGNAT ou sem UPnP, o link antigo e a
   volta pela trilha continuam mortos depois de um reinício
   ([§2.1](#21-persistência-de-servidores-para-a-url-não-mudar)).
2. **Três falhas de segurança tornam arriscado abrir para estranhos.**
   - Um anexo pode gravar fora da pasta de Downloads.
   - Um servidor malicioso pode entrar como você em outro servidor.
   - A volta pela trilha aceita um impostor.

   A primeira é conserto de horas. A segunda pede subir o protocolo
   ([§3.1](#31-segurança-três-bloqueantes)).
3. **O protocolo quebrou a compatibilidade quatro vezes em 18 dias**, e cada
   quebra obriga o grupo inteiro a atualizar junto. Uma 1.0 sem promessa de
   compatibilidade é só mais uma 0.x. A saída que as notas da 0.15 oferecem,
   guardar a 0.14.2 ao lado, não funciona
   ([§3.2](#32-protocolo-a-decisão-que-define-o-que-é-uma-10)).
4. **Nada prova automaticamente que o código publicado funciona.**
   - A CI está vermelha desde 16/08 e, desde `6f970c9`, só roda à mão.
   - O workflow de Release nunca passou.
   - A página publicada da v0.15.0 diz «Esta versão não traz nenhuma mudança de
     produto», logo abaixo do aviso de que ela não conversa com a 0.14.

   Ver [§3.3](#33-processo-nada-roda-sozinho).
5. **A primeira noite de um amigo esbarra em cinco bordas, e nenhuma é
   funcionalidade nova.**
   - Binário sem assinatura.
   - Link que não se clica.
   - Tecla de falar que a tela não nomeia.
   - Nenhum aviso de fone.
   - Fechar a janela de quem hospeda derruba todo mundo sem explicar.

   Ver [§3.5](#35-a-primeira-noite-de-um-amigo).

Das quatro features pedidas:

| feature | estado real | recomendação para a 1.0 |
|---|---|---|
| **Persistência / URL que não muda** | quase toda construída, com três elos quebrados | **consertar os três elos e medir em campo**; não derrubar o servidor sem perguntar; impedir o sono. Bandeja e serviço ficam para a 1.1 |
| **URL amigável** | link com nome existe, com seis furos; nada é clicável | **link clicável** (esquema registrado e confirmação antes de entrar) **mais uma página https** que só repassa o fragmento; consertar o link com nome |
| **Conexão simultânea** | nada construído; a superfície triplicou desde o ADR 0031 | **na 1.0, só "visitar outro servidor sem derrubar o seu"**; conexões em segundo plano na 1.1 |
| **Calls privadas** | nada construído | **decisão sua primeiro**: "ligar direto" (fora de um servidor) ou "chamada 1:1 dentro do servidor". Nas duas, dizer quem pode ouvir |

---

## 1. O que foi medido nesta análise

O `CLAUDE.md` pede que se meça antes de concluir. Estes são os fatos de carga,
e quem os conferiu:

| fato | como | quem |
|---|---|---|
| v0.15.0 = `e2fac4d` = HEAD | corpo do release na API de SEELE-RELEASES | coordenador |
| O ponto de encontro em produção responde `MORO`/`QUEM` | `cargo run -p seele-encontro --example sondar -- encontro.seele.app.br:8384` → «o quarto está vivo» | coordenador |
| `"encontro.seele.app.br".to_socket_addrs()` falha e `…:8384` resolve | programa `rustc` de 6 linhas: `Err invalid socket address` × `Ok [216.128.168.216:8384]` | coordenador (e o revisor, de forma independente) |
| O link sai com `enc=` sem porta, e o cliente o passa cru ao `lookup_host` | `alcance/encontro.rs:67`, `:409`; `main.rs:1012`; `seele-core/src/encontro.rs:355` | coordenador (LIDO) |
| O anfitrião registra a escuta como `anfitriao`, e o convidado pergunta pela impressão digital | `alcance/encontro.rs:447`, `:816`; `seele-ffi/src/lib.rs:7806-7814` | coordenador (LIDO) |
| A volta pela trilha não passa a impressão guardada, então não sai `LEVE` | `main.rs:1079` (`expected_fingerprint: esperada.clone()`); `seele-core/src/encontro.rs:97-99` (o `?` sobre `None`) | coordenador (LIDO) |
| Salvar anexo grava em `pasta/<nome escolhido pelo remetente>` | `ui/tela-sessao.js:3303`; `seele-ffi/src/lib.rs:1467`; `seele-core/src/client.rs:2114`; `seele-proto/src/attachment.rs:126` só recusa vazio e NUL | coordenador (LIDO) |
| A prova de identidade assina o nonce cru | `seele-core/src/client.rs` (`signing_key.sign(&nonce)`); `seele-server/src/session.rs` (`verify(&nonce, …)`); `export_keying_material`: 0 ocorrências | coordenador (LIDO) |
| `install.sh` baixa de um repositório parado na v0.10.0 | `install.sh:31` `REPO="DATA-AND-DEV/SEELE"`; `github.com/DATA-AND-DEV/SEELE/releases/latest` → `302 …/tag/v0.10.0` | coordenador (MEDIDO) |
| A página da v0.15.0 diz «não traz nenhuma mudança de produto» | HTML da página do release | coordenador (MEDIDO) |
| CI sem execução verde desde 16/08; 5 de 9 jobs vermelhos sobre o código da 0.15 | API `/actions/runs` | analista de estabilidade (MEDIDO) |
| `cargo deny check advisories` limpo; rustls 0.23.45 já traz a correção da RUSTSEC-2026-0285 | rodado | analista de segurança (MEDIDO) |
| Quatro quebras de protocolo publicadas em 18 dias | `git show <tag>:crates/seele-proto/src/version.rs` | revisor de distribuição (MEDIDO) |

O resto vem de leitura de código revista por um cético, e o anexo marca cada
afirmação como MEDIDO, LIDO ou INFERIDO. **Não houve nenhuma sessão de rede
real entre duas máquinas nesta análise.** Onde o texto diz que algo "não
funciona" em campo, trata-se de dedução a partir de código lido, e cada caso
leva uma proposta de medida.

---

## 2. As quatro features pedidas

### 2.1 Persistência de servidores, para a URL não mudar

O pedido tem duas metades: **o link sobreviver** e **o servidor sobreviver**.

#### O link sobreviver: a cadeia inteira existe, e três elos estão quebrados

O que está de pé:

- a identidade TLS mora no banco e é a mesma entre reinícios (`tls.rs:66-88`);
- a porta de escuta é fixa, 8383;
- o UPnP tenta 8383 antes de aceitar outra porta (`503b1db`);
- o histórico de quem entra guarda `fp`, `caminhos` e `bilhete`
  (`seele-core/src/conhecidos.rs:41-107`);
- o quarto responde em produção.

Três defeitos, independentes e silenciosos, impedem o reencontro no caso que
existe para ser coberto: anfitrião atrás de CGNAT ou sem UPnP, depois de
reiniciar.

1. **A pergunta ao quarto nunca sai.** `PONTO_PADRAO = "encontro.seele.app.br"`
   não tem porta (`alcance/encontro.rs:67`), e o `enc=` do link sai assim. O
   cliente passa esse texto cru a `tokio::net::lookup_host`
   (`seele-core/src/encontro.rs:355`), que recusa nome sem porta (MEDIDO). O
   `.ok()?` engole o erro, e `onde_mora_hoje` devolve `(None, None)` sempre.
   O `LEVE`, ao lado, funciona porque usa `Bilhete::ponto()`, que aplica a
   porta padrão (`uri.rs:295-297`), e isso esconde o defeito.
2. **Mesmo que a pergunta saísse, a resposta sobre a escuta não viria.** O
   anfitrião registra a escuta de avisos com a marca fixa `anfitriao`
   (`alcance/encontro.rs:447`, `:816`), igual para todo anfitrião do mundo. O
   convidado pergunta pelos 16 primeiros dígitos da impressão digital
   (`seele-ffi/src/lib.rs:7806-7814`), e ninguém registra essa marca. Efeito
   colateral: a vaga `anfitriao` pertence ao primeiro anfitrião que a ocupou,
   e o quarto entrega o endereço dele a quem perguntar.
3. **Quem volta pela trilha não fura o NAT.** A impressão esperada só vem de um
   link colado nesta sessão (`main.rs:928-946`, `:1079`). Sem ela,
   `Batida::preparar` devolve `None` (`seele-core/src/encontro.rs:97-99`), e o
   `LEVE` não sai, nem na entrada nem na reconexão (`enlace.rs:994-1001`,
   `:2707-2715`). A impressão guardada existe (`main.rs:981`) e só serve para
   formar a marca da pergunta. Isso também é falha de segurança, ver
   [§3.1](#31-segurança-três-bloqueantes).

Três fatos explicam por que ninguém viu:

- `seele-conformance/tests/quarto.rs` usa `127.0.0.1:<porta>` e faz o `MORO`
  à mão;
- o `sondar` usa a marca `sondagem` e recebe o ponto com porta;
- a CI não roda.

É o «existir não é funcionar» do `CLAUDE.md` em estado puro. O ADR 0047 diz,
em §2.1, «isto foi medido, não deduzido», e a medida não exercitava o caminho
que o link carrega.

Cenário por cenário, hoje:

| cenário | v0.15.0 |
|---|---|
| volta pela trilha, mesma casa | funciona (8383 na rede local é fixa) |
| volta pela trilha, anfitrião com UPnP na 8383 e o mesmo IP público | funciona (o `alt` público está guardado) |
| volta pela trilha ou link antigo colado, anfitrião em CGNAT ou sem UPnP, depois de reiniciar | **não funciona**: os três elos |
| o IP público do anfitrião mudou | **não funciona**, salvo por sorte no degrau 3 |
| link com nome (DNS do usuário, `c42a30f`) | funciona fora de CGNAT, mas GERAR CONVITE descarta o nome e o nome não é lembrado ([§2.2](#22-url-amigável)) |
| convidado novo com o link simples, depois de o anfitrião ter emitido **qualquer** convite de uso único | **recusado sem aviso** (CREDENCIAL RECUSADA). `aceita_convites` conta convites gastos e vencidos (`admissao.rs:94-107`) |
| anfitrião fechou o app | não funciona, e o convidado lê 5 minutos de RECONECTANDO |

Achados menores da mesma frente:

- O histórico do convidado é chaveado pelo **endereço de rede local** do
  anfitrião (`conhecidos.rs:170-171`). Dois amigos com `192.168.0.10:8383` se
  sobrescrevem.
- Todos os servidores guardados de uma máquina compartilham chave e porta
  (`servidores.rs:233-297`). **O link identifica a máquina, não o servidor**,
  e o link antigo da «Mesa de RPG» abre o servidor que estiver no ar.
- Hospedar em outra versão (VERSÃO QUE VAI HOSPEDAR) usa outro `SEELE_HOME`,
  logo outra identidade: todo link anterior é recusado por `fp` divergente
  (`versoes.rs:218-223`).
- O link carrega `v=` da versão de quem hospeda. Depois da primeira
  atualização, uma URL que não muda passa a carregar uma versão velha.

#### O servidor sobreviver

- **Fechar a janela** derruba o servidor sem perguntar. Isso também não chama
  `Hospedagem::encerrar`, então a regra UPnP fica no roteador por até 1 h
  (`porta.rs:86`). O servidor fecha sem mandar motivo, e o cliente trataria
  `ServerShuttingDown` como recuperável de qualquer forma
  (`enlace.rs:5150-5156`). Os convidados leem 5 minutos de «CONEXÃO PERDIDA ·
  RECONECTANDO», e nada diz que quem hospeda desligou. **SAIR e trocar de
  servidor já perguntam** (`tela-chamada.js:776-784`,
  `tela-sessao.js:3508-3552`): o analista tinha dito que não, e o revisor
  corrigiu.
- Não há bandeja, não há "iniciar com o sistema" e nada impede o sono enquanto
  se hospeda (grep vazio).
- **O `seeled` não é o servidor persistente que parece.**
  - O banco é relativo à pasta de onde se chama.
  - Ele não sobe a escada (sem UPnP, sem degrau 4, sem quarto).
  - Ao subir, manda rodar `connection`, que não existe desde o ADR 0039.
  - `install.sh` instala a v0.10.0, protocolo 2/3, que nenhum cliente atual
    alcança.
  - O `seeled` não tem atualizador.

#### Recomendação para a 1.0

1. **Consertar os três elos (P a M)**, e mudar os dois lados:
   - o cliente: `onde_mora_hoje` passa a usar `Bilhete::ponto()`, e a impressão
     esperada passa a ser `esperada.or(impressao_guardada)`;
   - o anfitrião: escrever `:8384` no `enc=`, para que links novos funcionem
     também com clientes 0.15, e fazer o `MORO` da escuta com a marca da
     impressão;
   - o eco do próprio `MORO` precisa ser filtrado, para não virar furo contra
     si mesmo;
   - o primeiro `MORO` sai na subida, e não 15 s depois
     (`alcance/encontro.rs:790-791`).
2. **O guarda que faltava**: um teste de conformidade que monte o ponto
   **exatamente como o link o carrega** (`Encontro::bilhete()`, sem porta), leve
   `alcance::encontro::abrir` até `onde_mora_hoje` e daí a uma conexão pela
   trilha. **Provar que ele fica vermelho** revertendo cada um dos três
   consertos.
3. **Medida de campo**:
   - o anfitrião em CGNAT fecha e reabre;
   - o convidado no 5G volta pela trilha e cola o link de ontem.

   Só depois trocar a frase `FuroDeNat` («se fechar, gere outro»,
   `frases.js:768-770`).
4. **Parar de derrubar sem avisar (M)**:
   - pedir confirmação ao fechar a janela enquanto se hospeda;
   - chamar `encerrar`;
   - difundir `ServerShuttingDown` e dar a ele uma faixa própria no cliente,
     porque a frase existe e hoje não é alcançada.
5. **Impedir o sono enquanto se hospeda (M)**: `IOPMAssertion`,
   `SetThreadExecutionState` e `inhibit`.
6. **Emitir um convite não pode fechar a porta para sempre** (P, decisão sua).
   Opções: contar só convites válidos, ou mandar para a fila da portaria quem
   chega sem segredo.
7. **Na 1.1**:
   - bandeja com "continuar hospedando" e iniciar com o sistema; o
     `--hospedar` já existe e só falta `--servidor <id>`;
   - `seeled` que sobe a escada e abre o **mesmo** banco do app;
   - unidades de serviço (launchd, systemd e Windows);
   - exportar e importar servidor, para migrar para uma VPS mantendo o `fp`.

### 2.2 URL amigável

Hoje o convite é um `seele://` de 100 a 255 caracteres, que **não abre o app com
um clique**:

- não há `CFBundleURLTypes`, registro no instalador próprio do Windows nem
  `MimeType` no `.desktop`;
- não há instância única;
- `RunEvent::Opened` não é tratado.

No WhatsApp o convite é texto para copiar. O campo CONECTAR só o reconhece se o
texto colado **começar** com `seele://` (`camada-servidores.js:168`): colar
«entra aí: seele://…» falha.

O **link com nome** (`c42a30f`) funciona no caso feliz e tem seis furos:

- GERAR CONVITE, o caminho normal com a portaria ligada, descarta o nome
  (`hospedagem.rs:322-329`);
- o nome não é salvo por servidor, então cada hospedagem volta ao link
  numérico, sem aviso;
- só o primeiro endereço do DNS é usado (`seele-ffi/src/lib.rs:3705-3711`);
- `2001:db8::1`, `casa:abc` e `x:99999` passam na conferência e produzem um
  link que o outro lado recusa **inteiro**, alternativas numéricas incluídas
  (`uri.rs:1187-1193` × `:582-595`);
- o `seeled` não tem nome;
- nada avisa que um nome apontado para uma casa em CGNAT não alcança ninguém.

As opções, contra a linha que o ADR 0022 traçou (nada no meio vê conteúdo):

| | custo | o que o terceiro passa a saber | identidade | veredito |
|---|---|---|---|---|
| **página https com fragmento**, `https://seele.app.br/e#<corpo>` | M | o host da página vê o clique (IP, navegador, hora), **nunca o fragmento** | o `fp` viaja inteiro | **1.0** |
| código curto resolvido pelo ponto de encontro | G | código↔fp↔IP de forma durável | **o operador pode se passar pelo servidor**, a menos que o código derive do `fp`, e aí não é curto: ~80–100 bits | não na 1.0; a variante autocertificante é 1.x |
| DDNS do usuário (já existe) | P | nada | o `fp` fica no link | **1.0, com os seis consertos** |
| nomes legíveis registrados no ponto de encontro | GG | tudo o que o código curto sabe, mais o nome | o operador vira autoridade de nomes | não |

**Recomendação para a 1.0: "amigável" é clicável, e o `fp` manda.**

1. **Pré-requisito de segurança**: conferir o `fp` **dentro do TLS**, antes do
   `Hello` ([§3.1](#31-segurança-três-bloqueantes)). Qualquer coisa que resolva
   *onde* por um terceiro só é segura se o `fp` decidir *quem* antes de o
   segredo sair.
2. **Esquema clicável (M)**:
   - registro no SO (`tauri-plugin-deep-link` no macOS e no `.deb`;
     `registro.rs` do instalador próprio no Windows) e instância única;
   - uma tela «ENTRAR EM `<servidor>`?» que **nunca conecta sozinha**:
     `--entrar` conecta sem perguntar (`tela-boot.js:579-582`), e reusá-lo
     violaria a pendência 10;
   - com sessão ou hospedagem aberta, dizer o preço.
3. **Página `/e` (M, fora deste repositório)**:
   - página estática com `default-src 'none'`, `no-referrer` e `noindex`;
   - só `location.replace("seele://" + fragmento)`, mais "Baixar" e "Copiar
     link direto";
   - **pré-condição medida pelo revisor**: `seele.app.br` hoje injeta o beacon
     do Cloudflare Web Analytics e não manda CSP. É preciso desligar o beacon ao
     menos em `/e`;
   - `uri::analisar` passa a aceitar o prefixo https, e só ele: um analisador
     só.
4. **Consertar o link com nome (P)**: os seis furos acima, mais
   `seeled convite --nome`.
5. **1.x**: um link curto e estável **derivado do `fp`**, resolvido pelo `QUEM`.
   Exige `MORO` assinado pela chave do servidor, porque sem isso ocupar a marca
   vira negação de serviço.

### 2.3 Conexão simultânea

Nada foi construído. `main.rs:899` ainda devolve `AlreadyConnected`. A
superfície que a multiconexão teria de tocar mais que triplicou desde a
estimativa do ADR 0031 (18/08), segundo recontagem do analista no HEAD:

| medida | ADR 0031 | HEAD |
|---|---|---|
| `#[tauri::command]` | 51 | 152 |
| comandos que resolvem `.connection()` | 23 | 61 resoluções em 60 comandos |
| ouvintes do canal de eventos, sem saber de qual sessão vem o evento | — | 8 |
| variáveis de módulo JS que são estado de sessão | 23 | ~62 |

A geração E2 dos MODs é um `AtomicU64` global e o tema mora em `#tela-sessao`:
tudo supõe uma sessão só. **Tirar o `AlreadyConnected` pelo caminho curto
produz perda de eventos calada**, mascarada pelo laço de snapshot de 500 ms.

Três achados valem por si:

- **Vazamento entre servidores, já hoje.** Rascunhos são chaveados pelo id do
  canal, que é `INTEGER PRIMARY KEY` **por banco**: o canal 1 é `geral` em todo
  servidor. O texto escrito na `#geral` de A reaparece no campo da `#geral` de
  B e sai com um Enter (`tela-sessao.js:2580-2595`). O volume por pessoa é
  guardado por apelido e nunca reaplicado: depois de qualquer reconexão, o
  deslizante mostra 150% e o áudio sai a 100%.
- **Já existe multiconexão de fato, sem coordenação.** O lançador do ADR 0046
  abre outra versão «sem fechar esta janela». Dois processos ficam em dois
  servidores, cada um com o próprio microfone aberto e escrevendo nos mesmos
  `conhecidos` e `servidores.json`.
- **O microfone abre ao conectar**, não ao entrar numa sala de voz
  (`seele-ffi/src/lib.rs:3930-3955`). Quem entra só para ler deixa o indicador
  do microfone aceso.

As alternativas, pelo caso de uso:

| recorte | entrega | esforço (INFERIDO) |
|---|---|---|
| **(d) visitar outro servidor sem derrubar o próprio** | quem hospeda sai para ver o servidor de um amigo, e o seu continua no ar | M, de 2 a 4 dias. Não é trivial: hoje não há caminho de volta ao próprio servidor na trilha (`main.rs:1216`), `hospedar` devolve `JaHospedando`, e falta o indicador "você continua hospedando" |
| **(b) conexões quentes em segundo plano, voz só na da frente, placa de não lidas** | saber o que aconteceu em B enquanto se está em A | G, de 1,5 a 2,5 semanas. `Session::connection()` continua devolvendo a da frente, então os 60 comandos ficam intactos |
| **(a) o ADR 0031 inteiro**: a voz fica na sala de A enquanto a tela mostra B | falar em A lendo B | GG, de 3 a 4 semanas |
| (c) reconexão periódica para contar não lidas | — | **recusar**: cada volta difunde entrada e saída para todos |

**Recomendação.**

- **Na 1.0**:
  - o recorte (d);
  - a limpeza de rascunhos e volumes por servidor (P), com um guarda que liste
    todo `new Map()` de `tela-sessao.js`;
  - um teste que tranque o `AlreadyConnected` até existir id de sessão na
    `Bridge`.

  O (d) depende do conserto da assinatura em [§3.1](#31-segurança-três-bloqueantes).
  Com o servidor de quem hospeda no ar enquanto ele visita outro, o repasse de
  identidade passa a alcançar o posto de Comandante dele.
- **Na 1.1**: o (b).
- **Na 1.2**: o (a).
- **Uma coisa para decidir antes da 1.1**: o lançador do ADR 0046, um processo
  por versão, e a multiconexão numa janela só não convivem. Uma janela de
  protocolo real ([§3.2](#32-protocolo-a-decisão-que-define-o-que-é-uma-10))
  tira o lançador do caminho crítico.

### 2.4 Calls privadas

Nada foi construído. Não há verbo de ligar, mensagem direta, sussurro nem sala
temporária. **O pedido admite duas leituras, e o código favorece coisas
diferentes em cada uma.**

**Leitura A: ligar para alguém fora de um servidor.** É o que você pediu em
15/09, nas palavras registradas no ADR 0047: «conexão P2P "sem server" para
conversa privada». O ADR 0047 §4.3 já tem o desenho: a chamada é a **hospedagem
embutida de quem liga**, com uma sala, convite de uso único e nenhum verbo novo
de protocolo.

- *A favor*: não há protocolo novo, e quem hospeda **é** um dos dois. Então
  «só nós dois ouvimos» é verdade sem E2EE.
- *Contra*:
  - a vaga de hospedagem é única (`main.rs:145`) e a porta é fixa, então quem
    já hospeda o servidor do grupo não consegue ligar sem derrubá-lo;
  - a portaria nasce ligada, e um convite válido **não** aprova sozinho
    (`portaria.rs:65`), o que obrigaria a aprovar à mão a pessoa para quem se
    acabou de ligar;
  - o `AlreadyConnected` tira o chamado do servidor em que ele está;
  - não há "toque": o link mandado pelo WhatsApp é o toque, e ele precisa ser
    clicável ([§2.2](#22-url-amigável)).
- `par.rs` **não** serve: ele carrega só tela, e o `fp` do par é anunciado pelo
  servidor. É plano de dados, não plano de controle.

**Leitura B: chamada 1:1 dentro de um servidor**, como uma chamada do Discord
entre membros.

- *A favor*: o chamado já está com a conexão aberta, então tocar é trivial. O
  servidor já cria a tarefa de sala sob demanda (`voice_room.rs:1238`), senta
  com teto (`assentar(…, Some(2))`) e tem presença.
- *Contra*:
  - é protocolo novo: `Ligar`, `Atender`, `Recusar`, `Chamando` e `Chamada`;
  - é preciso filtrar a difusão, porque hoje ocupação e «falando» vão para o
    servidor inteiro (`session.rs:4842-4856`, `:4888`). Isso reverte uma
    decisão escrita em `session.rs:4830-4839`;
  - quem hospeda recebe a voz **em claro**, porque o QUIC termina no servidor.
  - Estimativa do analista: cerca de 2 semanas.
- Achados de passagem:
  - a senha de sala é conferida pelo servidor, mas nada fora dos testes
    consegue defini-la;
  - `MovePerson` passa por cima da senha e do teto, e não confere se a sala de
    destino existe: um Operador pode sentar alguém numa sala-fantasma;
  - o mapa de tarefas de sala nunca encolhe;
  - `Presence::DoNotDisturb` existe no protocolo e no servidor e não está
    ligado.

**Nas duas leituras, a honestidade vem primeiro, e custa horas.** O README diz
«O TLS é ponta a ponta» (`README.md:105`), a interface mostra «CONEXÃO
SEGURA», e `specs/01-arquitetura.md:55` afirma que o servidor não tem acesso ao
áudio. Na verdade quem hospeda recebe o Opus em claro e o encaminha. O
`specs/08` manda que isso esteja escrito. Qualquer coisa rotulada «privada»
precisa dizer se isso vale em relação aos outros membros ou também em relação
a quem hospeda.

**E2EE 1:1 (1.1, G)**:

- *No servidor*: zero linhas para a voz, porque ele lê só `ssrc` e `seq`
  (`voice_room.rs:1057-1098`).
- *No cliente*:
  - X25519 efêmera assinada pela Ed25519, com separação de domínio;
  - HKDF com uma chave por sentido;
  - ChaCha20-Poly1305 com o cabeçalho como AAD;
  - nonce explícito, porque o `seq` é u16 e dá a volta em cerca de 22 min;
  - número de segurança lido em voz alta.
- *Revisão*: a composição é nossa e precisa de revisão externa antes de se
  chamar E2EE.
- *Protocolo*: se a 1.0 subir o protocolo, **reserve já o campo de material de
  chave**, opcional e sem uso.

**Recomendação.** A leitura A é a mais barata *se* o link clicável da §2.2
entrar. Ela reaproveita a hospedagem inteira, e a privacidade em relação a quem
hospeda vem de graça. Só que ela disputa a vaga com "hospedar o servidor do
grupo". A leitura B é a mais natural para quem já está num servidor, e custa
protocolo e duas semanas. **A escolha é sua** ([§6](#6-decisões-que-são-suas));
as duas estão desenhadas no anexo, com etapas.

---

## 3. O que separa a v0.15 de uma 1.0

### 3.1 Segurança: três bloqueantes

A base é mais cuidadosa que a média:

- TLS 1.3 em tudo e portaria ligada no HOSPEDAR AQUI;
- prévias de link buscadas pelo cliente, com consentimento por domínio e
  anti-SSRF;
- CSP estrita e nenhum `innerHTML`;
- MODs só do catálogo assinado;
- `cargo deny check advisories` limpo (MEDIDO).

Três falhas, porém, precisam sair antes de gente estranha usar o produto:

| # | falha | onde | conserto |
|---|---|---|---|
| **S1** | **Salvar anexo grava onde o remetente quiser.** `destino = pasta/nome`, e o nome é o `file_name` escolhido por quem enviou. `../.zshrc` ou `..\…\Startup\x.bat` viram execução no próximo login ou terminal. Um nome benigno repetido sobrescreve sem avisar | `ui/tela-sessao.js:3303` → `seele-ffi/src/lib.rs:1467` → `client.rs:2114` (`File::create`); `attachment.rs:126` só recusa vazio e NUL; o teste em `:303` carrega `../../etc/passwd` e aceita | **P**: o Rust decide o destino. Nome base apenas, recusar separadores e `..`, `create_new` com sufixo. Um teste que reprove se o conserto for revertido |
| **S2** | **Um servidor malicioso entra como você em outro servidor.** O cliente assina o nonce cru que o servidor mandar, sem o `fp` do servidor, sem vínculo com o canal e sem separação de domínio. Quem já foi admitido entra sem segredo. O servidor M repassa o nonce de X e a sua assinatura, e entra em X com a sua conta e os seus papéis | `client.rs` (`sign(&nonce)`); `session.rs` (`verify(&nonce, …)`); nenhum `export_keying_material` | **protocolo**: assinar `rótulo ‖ fp do servidor ‖ nonce`, ou material exportado do TLS. Entra na subida da 1.0 ([§3.2](#32-protocolo-a-decisão-que-define-o-que-é-uma-10)) |
| **S2b** | **O `fp` do link é conferido tarde demais.** O `TofuVerifier` aceita qualquer certificado no primeiro contato, e a conferência contra o `fp` só acontece depois de o `Hello` (com convite ou senha) e a assinatura terem saído. A corrida de candidatos do ADR 0037 faz isso **em todo candidato** que fechar o TLS, inclusive o IP privado do anfitrião na rede de quem recebe | `tofu.rs:227`; `enlace.rs:812-870`, `:1263-1291` | **P, sem protocolo**: passar o `fp` esperado ao verificador e falhar no TLS |
| **S3** | **A volta pela trilha aceita um impostor.** Sem `fp` esperado, o endereço dado pelo quarto é fixado às cegas, com a faixa PRIMEIRO CONTATO. A marca está em todo link, e no quarto fica quem escreveu primeiro: basta o anfitrião ficar fora mais de 60 s. O endereço do impostor é **persistido** em `conhecidos` | `main.rs:1079`; `seele-encontro/src/lib.rs:113-139`; `main.rs:1264-1269` | **P**: é o mesmo `esperada.or(impressao_guardada)` do elo 3 da [§2.1](#21-persistência-de-servidores-para-a-url-não-mudar). Com ele, ocupar o quarto volta a ser só negação de serviço, que é a premissa do ADR 0047 §2.4 |

Importantes, que não bloqueiam:

- A portaria **abre** se o banco falhar ao ler a senha:
  `Politica::carregar` faz `.ok()` e `.unwrap_or(0)` (`admissao.rs:85-101`).
  Conserto de uma linha, no molde de `portaria.rs:118-128`.
- O ponto de encontro não tem limitação de taxa. Cerca de 70 `MORO`/s mantêm as
  4096 marcas cheias, e todo anfitrião novo é recusado em silêncio: é negação de
  serviço da feature de persistência. Falta também uma página de privacidade
  (LGPD) dizendo o que `encontro.seele.app.br` aprende: IPs, quem está no ar e
  quem procura quem.
- O limitador pré-autenticação é por IP exato. Um único `/64` IPv6 enche a
  tabela e inunda a fila da portaria.
- Não há como aceitar uma troca legítima de chave pela interface. `esquecer`
  não apaga o pino, e o pino é por `host:porta`.
- O `seeled` nasce aberto, e o primeiro a conectar vira Comandante (ADR 0030).
- O fuzz está parado desde 07/08. O corpus tem byte de versão 0 ou 1, e os
  alvos não cobrem o `seele://` nem o UDP do ponto de encontro.
- A chave do atualizador tem custódia única. A revogação assinada de versões
  existe no leitor e não é consultada.

### 3.2 Protocolo: a decisão que define o que é uma 1.0

Quebras publicadas, medidas nas tags:

| subida | versão | data |
|---|---|---|
| 2→3 | v0.10.5-1 | 05/09 |
| 3→6 | v0.11.0 | 18/09 |
| 6→7 | v0.12.0 | 20/09 |
| 7→8 | v0.15.0 | 23/09 UTC |

Como o fio se comporta hoje:

- `encode` carimba a versão **global** em todo quadro (`control.rs:2301`), e o
  par mais velho recusa antes de ler o corpo. A janela N−1 só vale para quem
  ouve.
- O cabeçalho de voz exige a versão **exata** (`media.rs:185`), e o descarte é
  contado num `drops()` que só testes leem.
- A recusa aparece como «NADA RESPONDEU NESSE ENDEREÇO» ou «PROTOCOLO
  VIOLADO». A frase «VERSÃO INCOMPATÍVEL» existe e é inalcançável
  (`session.rs:488-491`).
- As notas da 0.15 oferecem guardar a 0.14.2 ao lado. O app só baixa a mais
  nova, o `latest.json` publicado tem uma versão só e não traz `executable`, e
  no Windows lado a lado não existe.
- O atualizador só procura versão quando alguém aperta o botão.

**A quebra 7→8 era em boa parte evitável.** As listas de mensagens só cresceram
por acréscimo no fim, e o servidor já deixa de mandar o que o par não entende
(`session.rs:4177-4231`). Cada quadro leva o próprio comprimento, então pular
uma mensagem desconhecida não desalinha a leitura.

**Proposta: uma última subida, «v9 = 1.0», que junte tudo, e depois o
congelamento.**

- **Negociação de faixa mínima–máxima no `Hello`**, com o carimbo da versão
  negociada em cada quadro. Carimbo negociado sozinho resolve só "cliente velho
  → servidor novo". O caso "servidor persistente que ficou um dia atrás" exige
  que o cliente desça de versão.
- **Mensagem desconhecida é ignorada, contada e registrada uma vez**, em vez de
  encerrar a sessão. Alternativa: um envelope `Extensao { tipo, corpo }`
  anunciado por capacidade.
- **Byte de versão do cabeçalho de voz reescrito por destinatário**, sem custo:
  o servidor já copia por assinante.
- **Recusa legível**: `Disconnecting{Incompatible}` carimbado com o byte que o
  par anunciou. Isso conserta até clientes 0.11–0.15 já em campo.
- **Tudo o que precisa de fio entra nesta mesma subida**:
  - a assinatura vinculada ao servidor (S2);
  - os verbos de chamada, se for a leitura B;
  - `EditMessage` e `UnbanPerson`/`SetRole`;
  - a confirmação de entrada em sala (pendências 37 e 45);
  - o campo reservado para E2EE.
- **Guarda**: bytes-ouro de cada mensagem em `seele-proto/tests/fio_1_0.rs`,
  mais um teste que reprove se `PROTOCOL_VERSION` mudar sem subir a versão
  maior. Hoje o guarda `a_release_publicada_recusa_o_carimbo_desta_build`
  compara com `PUBLICADA = 3` e passa por vácuo. Os enums aninhados não têm
  guarda nenhum.
- **Um ADR e uma linha em `specs/10`**: «1.x nunca quebra o fio».

### 3.3 Processo: nada roda sozinho

- **CI.** Não há execução verde desde 16/08. A última, sobre o mesmo código da
  0.15, reprovou em 5 de 9 jobs nos três sistemas: `clippy` no macOS e no
  Windows, `test` no Linux, no macOS e no Windows. Parte disso é ambiente: o
  macOS entrou na matriz com o cache frio e morreu em menos de 2 minutos. Mas
  `clippy (windows)` e `test (windows/linux)` já reprovavam antes, e os logs
  exigem login para ler. Desde `6f970c9` o push não dispara nada. **O
  repositório é público** (API: `private=false`), e os minutos de Actions são
  grátis: o motivo de `1974d07` para tirar deny e fuzz da CI deixou de valer.
- **Publicação.**
  - O workflow de Release nunca passou (0 de 69). Ele exige segredos do Azure
    e publica no repositório errado (`SEELE` em vez de `SEELE-RELEASES`).
  - As versões saem do `publicar.sh`, no seu Mac com o Windows por SSH, que
    tem bateria própria mas aceita `--sem-bateria` e não grava no release se a
    usou.
  - O classificador de notas só aceita `feat:`, `fix:` e `perf:`
    (`publicar.sh:516-531`). Os commits deste repositório são em português e
    sem prefixo, então a página da v0.15.0 diz «não traz nenhuma mudança de
    produto» (MEDIDO), e o guarda `xtask/tests/empacotamento.rs:1763` prende
    essa frase.
  - A mesma página diz «Dentro de cada um vão três programas», e o `connection`
    não existe mais.
- **Voz.** A 0.15 liga por padrão a redução de ruído na força máxima
  (`SUPRESSAO_PADRAO = 1_000`, `voice.rs:688`), calibrada só com sinal
  sintético. A própria revisão v15 (`review-v15:268`) recomendava esperar a
  avaliação acústica.
- **Travamentos sem frase.**
  - A abertura do áudio não tem prazo e prende o conectar
    (`seele-ffi/src/lib.rs:3938-3955`).
  - Não há gancho de pânico no app: um pânico na thread de voz some sem deixar
    linha no `seele.log`.
  - Os contadores de perda só são lidos em teste.
  - Não há «COPIAR DIAGNÓSTICO»: todo relato de campo chega sem dado.
- **O registro de pendências parou em 18/09.** O índice ainda diz que o job
  `windows-2022` nunca rodou, e ele roda e reprova desde 18/09.

### 3.4 Distribuição e plataformas

- **Plataformas.** Publicam-se só macOS Apple Silicon e Windows x64. Não há Mac
  Intel nem Linux, e o README ainda promete `.deb`.
- **`install.sh` e `install.ps1`.** Instalam o `seeled` 0.10.0 (MEDIDO), e no
  Linux dão 404. O segundo endpoint do atualizador aponta para o mesmo
  repositório parado.
- **Assinatura.**
  - *macOS*: assinatura ad hoc e sem notarização. O atalho "botão direito →
    Abrir" que o README ensina já não é o caminho do macOS 15. Há suspeita, a
    medir, de que cada atualização faça o sistema pedir de novo microfone e
    gravação de tela, porque o requisito designado é o `cdhash`.
  - *Windows*: o Smart App Control bloqueia sem contorno, como o repositório já
    registrou num teste real (`docs/assinatura-e-atualizacao.md:84-90`).
    Segundo a documentação da Microsoft consultada pelo analista, o Azure
    Artifact Signing não atende pessoa física no Brasil. A alternativa é um
    certificado OV de CA.
- **Licença.** Indefinida, com o repositório público (`license=null`). As
  dependências só restringem uma escolha, GPL-2.0-only (`cargo deny` MEDIDO).
  Faltam os avisos de terceiros no pacote e as condições do binário OpenH264 da
  Cisco: a frase de atribuição, o controle para desligar e a licença
  reproduzida.
- **Downgrade.** `migrate()` não recusa um banco migrado por uma versão mais
  nova. Hoje é seguro por acaso, porque a 0.15 não trouxe migração.

### 3.5 A primeira noite de um amigo

O que faria alguém desistir, em ordem de probabilidade:

1. **«Ninguém me ouve».**
   - O padrão é TECLA, e a linha de estado diz «MICROFONE ABRE NA TECLA» sem
     nomear ESPAÇO.
   - A tecla só vale com a janela em foco e fora da caixa de texto, que é o
     caso comum logo depois de mandar uma mensagem.
   - No macOS, o microfone negado não aparece em lugar nenhum: a checagem só
     existe no Windows e saiu da entrada.
2. **Eco sem explicação.** Não há cancelamento de eco, e nenhuma tela manda usar
   fone: só o README diz. Se o padrão virar VOZ para resolver o item 1, isto
   vira o problema principal.
3. **O anfitrião fecha a janela.** Ver
   [§2.1](#21-persistência-de-servidores-para-a-url-não-mudar).
4. **Ninguém vê quem bate.** Não há notificação do sistema. Com a portaria
   ligada, o amigo bate a cada 15 s enquanto o anfitrião joga em tela cheia. A
   contagem de não lidas existe no core (`state.rs:741`) e não chega à tela.
5. **Primeiro uso sem nome.** O amigo bate como `pessoa-3f2a`, e quem hospeda
   tem de adivinhar quem é.
6. **Instalar exige vencer o sistema operacional.** Ver
   [§3.4](#34-distribuição-e-plataformas).

---

## 4. Outras opções

Ordenadas por valor para a 1.0 dividido pelo esforço, **dado o que já existe**:

| candidato | o que já existe | esforço | onde |
|---|---|---|---|
| **Ligar o que está pronto** | edição de mensagem: persistência, evento, difusão e aplicação no cliente prontos, falta o verbo (`messages.rs:500`); desbanir e promover a Operador prontos no servidor (`permissions.rs:423`, `:553`), e dá para expor pelo `seeled` sem protocolo; não lidas no core; `DoNotDisturb` no protocolo e no servidor; a frase «VOCÊ FOI CHAMADO» | P a M cada | 1.0 |
| **Notificação do sistema** para portaria, menção e mensagem com a janela minimizada | a frase, a lógica de menção a escrever e `tauri-plugin-notification` (adendo ao ADR 0020) | M | 1.0 |
| **Diagnóstico de alcance**: o anfitrião vê «alguém com o seu link tentou e o caminho não abriu», e o convidado lê a causa certa | o anfitrião já recebe o aviso e só o registra no log (`alcance/encontro.rs:841`) | M | 1.0. É o que dá dado para decidir o degrau 5 |
| **Cancelamento de eco** | hoje há AEC3 em Rust puro: `aec3` 0.4.0 (de 16/09) e `sonora`. O motivo do ADR 0007, dependência C/C++, caiu. O ponto de inserção é claro (entre `voice.rs:1980` e `:1994`) | spike de 3 dias, depois G | 1.0 se o spike passar (sugestão: ERLE ≥ 20 dB num notebook com alto-falante, +10 ms no máximo); senão aviso de fone e 1.1 |
| **Falar sem a janela em foco** | nada | M | decisão: um atalho registrado "toma" a tecla dos outros apps; um gancho de baixo nível pede permissão de Acessibilidade no macOS e não funciona no Wayland |
| **Exportar e importar servidor e identidade** | backup citado só em comentário | M | 1.1. É o que permite "migrar para uma VPS sem trocar o link" |
| Busca no histórico inteiro (FTS5) | busca local com dobra de acento | M | 1.1 |
| Mudo forçado e registro de moderação | coluna no banco | M | 1.1 |
| Relé próprio numa VPS do usuário, o degrau 5 "com o conteúdo em casa" | ADR 0022 o exclui | G | 1.2, se o diagnóstico de alcance mostrar que o caso é frequente |
| mDNS "há um SEELE aqui" | nada | M | 1.2 e só por escolha explícita; contraria a discrição |
| **Mobile** | nada | GG | **tirar formalmente da 1.0**: `specs/00` e `specs/09` ainda o preveem |
| E2EE geral, reações, soundboard, música, gravação | — | — | fora: não-objetivos ou pós-v1 escritos nas specs |

---

## 5. Plano proposto

Ordem por risco. Os esforços são inferências dos analistas e servem como ordem
de grandeza, não como prazo.

### Fase 1: parar de quebrar e de prometer o que não há

Sem mudança de protocolo, cabe numa 0.15.x.

- [ ] Os três elos do quarto, com o teste de ponta a ponta provado por reversão
      ([§2.1](#21-persistência-de-servidores-para-a-url-não-mudar)).
- [ ] S1, anexo (P). S2b, `fp` dentro do TLS (P). S3, `fp` guardado como
      esperado (P, o mesmo do elo 3).
- [ ] A portaria que abre com erro de banco passa a fechar (P).
- [ ] `install.sh` e `install.ps1` passam a apontar para `SEELE-RELEASES`, com
      um teste que reprove se voltarem. Acertar o segundo endpoint do
      atualizador (P).
- [ ] O `seeled` passa a imprimir um `seele://` com `fp`, e não `connection`
      (P).
- [ ] Textos publicados (P):
  - o README: `connection`, `.deb`, «tela ainda não construída», o contorno do
    macOS e «TLS ponta a ponta»;
  - o corpo do release da v0.15.0 e o classificador do `publicar.sh`;
  - a promessa da «0.14.2 ao lado»;
  - `specs/01:55`.
- [ ] Confirmar ao fechar enquanto se hospeda, chamar `encerrar` e difundir
      `ServerShuttingDown` com uma faixa própria no cliente (M).
- [ ] Nomear a tecla na linha de estado e na ajuda; aviso de fone; pedir o nome
      no primeiro uso; a checagem do microfone no macOS (P).
- [ ] Rascunhos e volumes por servidor (P).
- [ ] Prazo na abertura do áudio e gancho de pânico com linha no log (M).
- [ ] CI:
  - abrir os logs dos 5 jobs vermelhos e classificar cada falha como produto,
    ambiente ou teste frágil;
  - devolver o gatilho de push, porque os minutos são grátis;
  - trazer de volta o deny e o fuzz, este com corpus novo;
  - gravar no corpo do release que bateria rodou (M).

### Fase 2: o protocolo da 1.0

Uma subida só, a v9, de 1 a 2 semanas.

- [ ] Negociação de faixa, carimbo da versão negociada, mensagem desconhecida
      ignorada, versão da voz reescrita por destinatário e recusa legível.
- [ ] A assinatura do desafio vinculada ao servidor (S2).
- [ ] Os verbos que a 1.0 levar: edição, desbanir e papel, chamada (se for a
      leitura B), confirmação de entrada em sala e o campo reservado para
      E2EE.
- [ ] Bytes-ouro, o ADR «1.x nunca quebra o fio» e `specs/10`.

### Fase 3: as features da 1.0

- [ ] Link clicável: esquema registrado, instância única e a tela «ENTRAR
      EM…?» (M).
- [ ] A página `https://seele.app.br/e#…` sem beacon e com CSP (M, no site).
- [ ] O link com nome: os seis furos e `seeled convite --nome` (P).
- [ ] Visitar outro servidor sem derrubar o seu (M). Depende de S2.
- [ ] Calls privadas, conforme a sua decisão ([§6](#6-decisões-que-são-suas)).
- [ ] Impedir o sono enquanto se hospeda (M).
- [ ] Notificações do sistema (M).
- [ ] Spike de AEC (3 dias). Com base nele, AEC na 1.0 ou aviso e 1.1.

### Fase 4: provar

- [ ] `1.0.0-rc.1` com o protocolo congelado.
- [ ] Duas sessões de campo registradas em `docs/m1-medicoes.md`:
  - montagens Windows↔Windows e Windows↔Mac;
  - 30 minutos de voz com a redução de ruído no padrão: fala baixa,
    ventilador, teclado, fone e notebook;
  - trocar o fone no meio;
  - tela Windows↔Windows (a pendência 33 está aberta desde a 0.8.5) e tela
    inteira com som no Windows;
  - expulsar;
  - **anfitrião em CGNAT fecha e reabre, e o convidado volta pela trilha e cola
    o link de ontem**;
  - derrubar e voltar a rede.

  Se a supressão soar mal, baixar `SUPRESSAO_PADRAO`. O que falhar vira
  conserto ou sai do anúncio, por exemplo «tela entre dois Windows: beta».
- [ ] De 3 a 7 dias de uso pelo grupo **sem nenhuma mudança de fio**, e então a
      `1.0.0`, de um SHA com CI verde.

### Na 1.1

- Conexões quentes em segundo plano, o recorte (b).
- Bandeja e iniciar com o sistema; `seeled` que sobe a escada e abre o mesmo
  banco do app; unidades de serviço.
- AEC, se ficou fora.
- E2EE 1:1.
- Exportar e importar servidor e identidade.
- Link curto autocertificante.
- A leitura de chamada que não entrou na 1.0.

---

## 6. Decisões que são suas

Estas mudam o plano, e nenhuma se resolve por código:

1. **Calls privadas: qual leitura?**
   - (A) ligar direto para um amigo, fora de um servidor;
   - (B) chamada 1:1 entre membros de um servidor.

   E «privada» quer dizer «ninguém mais no servidor» ou «nem quem hospeda»? A
   segunda leitura é E2EE, que leva semanas e precisa de revisão externa.
2. **Conexão simultânea: qual caso de uso?**
   - (i) falar em A lendo B;
   - (ii) saber que algo aconteceu em B;
   - (iii) hospedar o seu e visitar outro.

   A recomendação entrega (iii) na 1.0 e (ii) na 1.1.
3. **Protocolo congelado na 1.0?** Isso significa aceitar **mais uma quebra**,
   em que a 0.15 e a 1.0 não conversam, em troca de «1.x nunca quebra».
4. **Um link identifica a máquina ou o servidor?** Hoje todos os servidores
   guardados compartilham chave e porta.
5. **Depois de emitir um convite de uso único**, o link simples continua
   mandando gente nova para a fila da portaria? Hoje ele recusa calado.
6. **Plataformas da 1.0.** Só macOS Apple Silicon e Windows x64? Mac Intel,
   com build universal, e Linux, «voz e texto, sem tela»?
7. **Assinatura.** A Apple custa US$ 99 por ano. O Windows pede um certificado
   OV de CA, pois o Azure não atende pessoa física no Brasil. Ou a 1.0 sai sem
   assinatura, com instrução, sabendo que o Smart App Control não tem
   contorno?
8. **Licença.** O repositório é público e sem licença, e cada contribuição
   aceita encarece a decisão.
9. **O ponto de encontro guarda metadado vivo.** Aceita publicar uma página de
   privacidade dizendo isso? E um segundo ponto, para redundância, antes de
   escrever na tela que o link sobrevive?

---

## 7. Limites desta análise

- **Nenhuma sessão de rede entre duas máquinas foi feita.** Os "não funciona"
  da §2.1 vêm de código lido e de uma medida local de resolução de nome, não de
  uma tentativa real atrás de CGNAT. A Fase 4 existe para isso.
- **Os logs da CI exigem login**, então não se sabe se as falhas são do
  produto.
- **Os esforços (P, M, G, GG) são inferência.**
- As contagens de superfície da §2.3 são do analista, e o revisor reproduziu
  parte delas. O anexo diz onde elas divergem.
- O revisor refutou duas afirmações do analista de persistência: «o conserto é
  só no anfitrião» e «todo o resto da cadeia funciona». Corrigiu uma terceira:
  «SAIR derruba sem perguntar». O texto acima já usa a versão corrigida.
