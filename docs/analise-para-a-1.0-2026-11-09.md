# Mapa para a 1.0 — revisão de 09/11/2026

Revisão do repositório inteiro no commit `f7ea590e`, que é o release publicado
v0.19.6 (conferido na API do `SEELE-RELEASES` antes de começar). Sucede a
`analise-para-a-1.0-2026-09-22.md`.

## Como ler

Oito frentes leram o código em paralelo: segurança do servidor, segurança do
cliente e da distribuição, streaming e mídia, camada de rede, conectividade
entre pessoas, design e responsividade, qualidade de código, e produto.
Nada foi alterado no repositório; nenhum build ou teste foi rodado.

Cada item traz uma marca de confiança:

- **[C]** conferido por mim depois do relatório: reli o trecho, rodei o comando
  ou vi a captura.
- **[R]** relatado pela frente com `arquivo:linha`, não reconferido.
- **[E]** estimativa ou inferência; precisa de medida.

Custo: **P** (dias), **M** (uma a duas semanas), **G** (mais que isso).

## Veredito

O núcleo está mais sólido do que o volume de pendências sugere. Autenticação,
cargos, moderação, sandbox de MOD e a higiene de XSS foram relidos e não têm
furo. Não há achado crítico de segurança.

O que separa a 0.19.6 de uma 1.0 não é um defeito grave. São quatro coisas:

1. **Nada é verificado automaticamente.** A CI não roda desde 22/09 e as oito
   últimas execuções falharam. [C]
2. **O streaming de tela é estruturalmente limitado**: vai num canal confiável e
   ordenado, sem realimentação de perda. É o motivo real de ele parecer
   defasado. [C]
3. **O produto ainda cala onde mais importa**: quando o NAT não abre, a pessoa
   lê o nome interno do erro. [C]
4. **A «mesa mínima» de um chat está pela metade**: editar, responder, mencionar,
   notificar e desbanir existem no fio ou no banco e não chegam à interface. [R]

## 1. Bloqueia a 1.0

| # | Item | Evidência | Custo |
|---|---|---|---|
| B1 | CI de volta ao automático e verde nos três sistemas; 1.0 só sai de SHA verde | `ci.yml:46` é só `workflow_dispatch`; última execução 22/09, `failure` [C] | M |
| B2 | Licença no repositório | não existe `LICENSE` [C] | P (decisão) |
| B3 | Derrubar sessão alheia pelo túnel: teto em `locais` e erro de codificação que não encerre a sessão de quem recebe | `control.rs:3683`, `pares.rs:208`, `tunel.rs:255`, `session.rs:4962` [C, lido; não reproduzido] | P |
| B4 | SQLite com advisory aberto | `cargo deny` reprova: RUSTSEC-2026-0459, `libsqlite3-sys 0.38.1` [C] | P |
| B5 | CSP ausente em caminho desconhecido do seele.sh | `/x-nao-existe` responde 200 sem CSP; `web/_headers` só cobre `/` e `/index.html` [C, medido em produção] | P |
| B6 | Frases de conexão que faltam, com guarda de todas as variantes | `NatNaoAbriu` não tem frase; `frases.js:882` mostra o texto cru [C] | P |
| B7 | Três defeitos visuais na janela mínima e na chamada | mensagens sobrepostas a 720×480; botão «SALAS E CANAIS» sem estilo a 1280; barra da chamada cobre nomes a 720×480 [C, capturas; a 3ª é R] | P |
| B8 | Specs e README reconciliados com o código | `specs/README.md:30` «Nada implementado»; `README.md:47` diz QUIC [C] | M |
| B9 | Travessia entre duas casas medida de verdade | gate declarado e aberto em `docs/alcance-pela-internet.md:18` [R] | M |
| B10 | `seeled backup` / `restore`, encerramento limpo em sinal | nenhum `ctrl_c`/`signal::` no crate do servidor [C]; backup inexistente [R] | P |
| B11 | Mesa mínima de texto: editar, responder, mencionar, desbanir | `replies_to` e `MessageEdited` no fio sem UI; sem verbo de desbanir [R] | M |
| B12 | Áudio da transmissão com codificador próprio | usa o codificador de voz: mono, VoIP, DTX (`bomba.rs:432`, `codec.rs:231`) [C] | P |

## 2. Segurança

### Servidor

Nenhum crítico. Quase tudo é negação de serviço.

- **Alto — sessão alheia derrubada pelo túnel** (B3). [C]
- **Alto — estado sem teto no motor do enlace**: Binding STUN com `U` vivo de
  origens ilimitadas (`ponta.rs:1128`). Sem conta. A magnitude dentro do str0m
  precisa de medida. [R/E]
- **Médio — Argon2 com o banco travado, antes da autenticação**
  (`session.rs:948`, `admissao.rs:443`). Em servidor com senha, cada tentativa
  trava o banco de todos. Mover para `spawn_blocking` fora do `lock`. [C]
- **Médio — balde de apertos por IP inteiro** (`taxa.rs:239`): um /64 IPv6 dá
  baldes ilimitados. Chavear por /64. [C]
- **Médio — banco sem permissão restrita**: nenhum `chmod` fora de teste no
  crate; `seele.db` guarda a chave do certificado e os convites. [C]
- **Médio — ponto de encontro**: `CHAMA` sem cookie, `MORA3` sem prova de posse,
  orçamento de 30 `CHAMADO` por minuto esgotável por quem tem o link. [R]
- **Médio — portaria sem teto nem validade**, campainha com três vagas
  monopolizáveis, tela sem balde de bytes no servidor. [R]
- **Médio — `mundo.buscar` de MOD**: sem prazo total e sem filtro de loopback ou
  rede privada. Só explorável se um MOD repassar URL do cliente. [R]
- **Fuzz**: sem alvo para STUN, o HTTP de `sinal.rs`, o demux do motor,
  `screen.rs`, manifesto de MOD e respostas SSDP/PCP. [R]

Bem feito e relido: aperto de mão (nonce novo, prova amarrada às duas
impressões), motor de cargos sem escalada, moderação conferida no banco a cada
pedido, SQL todo parametrizado, sandbox do rquickjs com teto de memória e
interrupção.

### Cliente e distribuição

Nenhum crítico nem alto.

- **Médio — CSP ausente fora de `/`** (B5). Publicar um `404.html` ou mover a
  CSP para `/*`. [C]
- **Médio — release com dependências mutáveis no job que assina**: actions por
  tag, `cargo install artifact-signing-cli` sem versão, `contents: write` para
  todos os jobs (`release.yml:46`, `:561`). A chave do atualizador não tem
  rotação. [C]
- **Médio — manifesto de versões não assinado como um todo**
  (`manifesto.rs:294`): quem escreve no repositório de releases pode rotular um
  pacote antigo e assinado como versão nova. [R; o cenário de rollback não foi
  seguido até o fim]
- **Médio — servidor escolhe a linha do cliente** (downgrade com um clique, por
  desenho, ADR 0046). Pede política de revogar linhas antigas. [R]
- **Médio — «metade de máquina» de MOD roda processo nativo sem sandbox depois
  do aceite**. A tela de aceite precisa dizer isso com essas palavras. [R]
- **Baixo**: `install.sh` confere a soma vinda da mesma origem do binário;
  prévia de link não reconfere consentimento no redirecionamento; regra de
  firewall do instalador larga; identidade do desktop em arquivo claro. [R]

Bem feito: zero `innerHTML`/`eval` na UI, CSP do Tauri sem inline nem remoto,
MOD web em iframe de origem opaca, chave do navegador não extraível.

## 3. Streaming e mídia

**O fato central: nenhuma mídia vai por RTP.** O str0m serve de ICE, DTLS e
SCTP; tudo trafega em DataChannels. A voz vai num canal sem ordem e sem
retransmissão, o que está certo. A tela vai num canal **confiável e ordenado**
(`ponta.rs:2090`). [C]

Consequências:

- Com perda, o SCTP retransmite e a fila bloqueia: a latência cresce em vez de a
  imagem degradar. [E]
- Não se pode descartar um quadro atrasado, então o servidor **corta** o
  espectador lento da transmissão inteira (`seele-server/src/tela.rs:853`). [C]
- A sonda de banda só enxerga RTT de um `Ping` a 2 Hz; não vê perda. [R]
- O teto de bitrate é dividido por N espectadores: um espectador ruim rebaixa
  todos. [R]

Outros pontos:

- **Quadro recebido cruza a ponte do Tauri em base64 dentro de JSON** e é
  convertido byte a byte no fio principal (`palco-imagem.js:435`). O IPC binário
  já é usado em outro ponto do app. [C]
- **Duas a três cópias de quadro na CPU** antes do codificador de hardware
  (I420 intermediário no macOS e no Windows). [R]
- **Política de quadro-chave diferente por codificador**: a cada 2 s no
  VideoToolbox, só sob demanda no OpenH264. [R]
- **Sem carimbo de tempo no fio**: não há sincronia A/V nem ritmo de
  apresentação. [R]
- **Voz**: FEC in-band desligado, PLC de um quadro, sem cancelamento de eco no
  nativo, supressão por subtração espectral. Deriva de relógio e jitter
  adaptativo são pontos fortes. [R]
- **Linux não captura tela**, por decisão registrada (`captura/mod.rs:37`). [C]

### Caminho para «streaming de nível 1.0»

| Ordem | O quê | Custo | Quando |
|---|---|---|---|
| 1 | Áudio da tela: estéreo, modo `Audio`, sem DTX, ~96 kbps | P | 1.0 |
| 2 | Quadro até a WebView por IPC binário | P/M | 1.0 |
| 3 | Carimbo de tempo no cabeçalho de quadro; política de chave única | P | 1.0 |
| 4 | Tirar o vídeo do canal confiável: trilhas RTP do str0m (NACK, PLI, TWCC e estimativa de banda juntos) ou canal não confiável com relatório do receptor | G | 1.0, é a peça estrutural |
| 5 | Duas camadas por espectador, para o lento degradar em vez de cair | G | 1.0 em forma mínima |
| 6 | Entrada NV12 direto no codificador de hardware | M | 1.0 no Windows |
| 7 | AEC nativo e FEC adaptativo pela perda que o servidor já mede | M | 1.0 |
| 8 | AV1/HEVC, 1440p, supressão neural, E2EE de mídia | M/G | depois |

A API de mídia do str0m 0.24.1 não foi conferida; o item 4 começa por aí.

## 4. Rede: onde há duas engines

**A comunicação com o servidor já é uniforme.** Desktop, navegador e `seeled`
usam o mesmo fio: DataChannel num socket UDP, mesmas mensagens `postcard`. Não
há QUIC sobrando em código nem em dependência. [R]

As duplicidades reais:

1. **Conversa pessoal — a pior.** No desktop ela roda pelo WebRTC da WebView
   (`pessoal-web-entrada.js:232`), e as salas pelo núcleo em Rust. Há uma
   segunda implementação nativa mantida como recuo (`:338`), que o próprio
   código chama de «sala QUIC» (`main.rs:3705`). São cerca de 4,3 mil e 5 mil
   linhas. O mesmo app tem duas engines de voz e tela, com DSP e reconexão
   diferentes. [C]
2. **Motor de sessão escrito duas vezes.** `seele-web` não depende de
   `seele-core`; aperto e comandos são reimplementados e mantidos iguais por
   testes. «Um núcleo, três cascas» vale para protocolo e estado, não para o
   motor. Unificar num crate sem E/S é G. [R]
3. **HTTP espalhado**: um crate, seis montagens com prazos de 15 s, 20 s, 300 s
   e nenhum (`main.rs:7954` usa `reqwest::get` sem prazo). [C]
4. **Três pilhas de cripto no binário**: `ring`, `aws-lc-rs` e RustCrypto, mais
   `sha2` 0.10/0.11 e `hmac` 0.12/0.13. [R]
5. **Atualização por quatro caminhos**: updater do Tauri, lançador, instalador e
   scripts. [R]
6. **Tempos e limites divergentes** entre nativo e web sem motivo: discagem de
   4/10 s contra 20 s, espera pelo ponto de 250 ms contra 1500 ms. [R]

Peso morto a remover: `set_priority` é um no-op que devolve `Ok` e ainda é
chamado em seis lugares (`conexao.rs:1011`) [C]; constantes da linha 1 usadas só
por testes; consulta ENC/1 no cliente; `ratatui` em `seele-conformance` com zero
usos [C]; 270 menções a QUIC em comentários.

Passos: tabela única de tempos exportada ao JS pelo WASM (P); fábrica única de
cliente HTTP (P); limpar o peso morto (P); um só caminho para a conversa
pessoal (M, decisão); motor de sessão comum (G).

## 5. Conectividade entre pessoas

O anfitrião é ICE completo desde 26/10 (ADR 0063); as specs ainda dizem
ICE-lite. Não há relay, por decisão. [C]

**Nada disso foi medido entre duas casas pelo projeto.** A estimativa da
revisão, a partir de números externos de operadores de WebRTC: 2 a 5 % dos
convidados ficam de fora quando o anfitrião tem porta aberta, e 15 a 30 % dos
pares quando ele está em CGNAT sem UPnP. Hoje essas pessoas veem cinco minutos
de tentativas e uma frase. [E]

Lacunas, em ordem de ganho:

| # | Problema | Proposta | Custo |
|---|---|---|---|
| 1 | `NatNaoAbriu` e `NaoLocalizou` sem frase [C] | escrever, com guarda de todas as variantes, provado revertendo | P |
| 2 | Sala pessoal diz «ponto desligado neste computador» quando o registro não vem (`sala.rs:285`) [C a frase; R a causa] | tentar de novo com recuo e dizer «não respondeu» | P |
| 3 | Furo IPv6 ausente: o nativo manda zero candidatos (`core/encontro.rs:252`) [C]. **Em 12/11: feito do lado de quem chama** (quando o IPv4 não abre, uma segunda chamada com o IPv6 global, e o anfitrião fura por ele sem mudar); **o `ATENDE` não tem onde levar um IPv6 global sem mudar o ponto e o fio**, e fica como decisão de implantação | candidato IPv6 global no `CHAMA` e no `ATENDE` | M |
| 4 | Alcance é suposto, não testado de fora | o ponto sonda a porta mapeada e classifica o NAT | M |
| 5 | A volta só disca o endereço vencedor: quem entrou pela LAN e saiu de casa falha por 5 min [R] | refazer a corrida inteira após duas ou três falhas | P/M |
| 6 | Anfitrião não reage a troca de rede nem resolve o DNS do ponto de novo [R] | vigiar interfaces e resubir a escada | M |
| 7 | Ponto de encontro único [R] | dois pontos no convite | M |
| 8 | Página de diagnóstico de rede sem nenhum link que leve a ela [C] | ligar à tela de erro; «copiar diagnóstico» | P |
| 9 | NAT-PMP/PCP em IPv4 ausente [R] | as dependências já estão na árvore | P |

Lado social: `seele://` só é registrado no instalador do Windows; não há QR,
descoberta na LAN, presença de amigos nem mensagem para quem está offline;
desktop e navegador têm identidades separadas. [R]

## 6. Design, responsividade e celular

A UI foi renderizada de verdade em dez tamanhos (77 capturas, com o dublê de
`bancada/telas.cjs`). As capturas estão fora do repositório, em
`/private/tmp/claude-501/-Users-dev-alexandre--claude-spares-spare-OWyYQg/b0b9afa4-db7b-4741-b0b1-f3ac4f231aed/scratchpad/dr/`.
Usam dados falsos; os três defeitos abaixo merecem confirmação no app real.

Defeitos vistos:

- **Mensagens sobrepostas na janela mínima** (720×480, o mínimo de
  `tauri.conf.json`): sobram cerca de 100 px para a conversa. [C]
- **«SALAS E CANAIS» como botão nativo branco na chamada a 1280**: o estilo mora
  dentro de um `@media` que não vale nessa largura (`tela-sessao.css:3830`). [C]
- **Sem teto de largura**: a 2560 a linha de mensagem atravessa a tela inteira e
  nada escala. [C]
- **Paisagem no celular**: uma única mensagem visível. [R]
- **Barra da chamada cobre os nomes** a 720×480. [R]

Sistema de design:

- Cor está toda em tokens e passa AA no texto. Bordas de campo ficam em
  1,3:1, abaixo dos 3:1 pedidos para limite de controle. [R]
- 28 tamanhos de fonte reais para 7 tokens; mínimo de 9 px no desktop. [R]
- 17 diálogos com 24 classes de caixa, cerca de 26 variantes de botão, 26
  valores de `z-index`. [R]
- `specs/07` manda canto reto e sem sombra; a 0.19 é vidro arredondado. [R]
- Ações destrutivas sem afordância: × e lixeira de 24 px ao lado do nome do
  canal; EXPULSAR e BANIR com o estilo de FECHAR. [R]

Celular:

- A casca é escolhida uma vez na carga, por toque **ou** largura menor que 900
  (`ponte-web.js:398`): tablet e notebook com toque recebem a casca de telefone,
  e redimensionar não troca. Há dois desenhos «estreitos» a manter. [C]
- Não é instalável: sem manifest, ícone ou `theme-color`. Sem Wake Lock e sem
  notificação. [C para o manifest]
- Resolvido desde a revisão de 05/10: alvos de 44 px, rolagem lateral zero,
  saída sempre alcançável. [R]

Pontos fortes a preservar: a tela de queda, a telemetria como assinatura, e a
lista «o que o celular não faz» dita em vez de calada.

## 7. Qualidade de código e processo

- **Arquivos gigantes**: `main.rs` 23,6 mil linhas (69 % teste, 234 comandos),
  `seele-ffi/lib.rs` 17 mil, `enlace.rs` 12 mil, `session.rs` 8 mil. [R]
- **`run_session` tem cerca de 3.200 linhas numa função só**
  (`session.rs:1939`–`5148`). É onde vive a autorização de cada comando. [C]
- **Testes que leem o texto do código**: 454 leituras de fonte em
  `tests/frontend.rs`. Travam a divisão dos arquivos grandes, então vêm antes
  dela. [C a contagem]
- **Saídas de emergência no release**: `sem_validar`, `--sem-bateria`,
  `--validado`. As notas da 0.19.3 a 0.19.5 dizem que a bateria não rodou. [R]
- **Front-end sem rede**: 59 `<script>` clássicos em ordem manual, 1.835
  declarações num escopo global, sem ESLint nem checagem de tipos. [R]
- **Falha silenciosa remanescente**: 56 `catch` vazios e 70 `.catch(() => {})`
  no JS; oito frases mandam ler o `seele.log` e nenhum comando o abre. [R]
- **`pendencias.md`** tem 10,5 mil linhas, cerca de 43 abertas, e parou em
  24/10. [R]
- **Nomes que confundem**: `seele-enlace` (transporte) e `seele-core/enlace.rs`
  (reconexão); `seele-ffi` não é FFI, é a fachada do app. [R]
- **Linux**: a infraestrutura promete e o release não entrega; os assets são só
  macOS arm64 e Windows x64. [C]

## 8. Produto e features

Faltas de mesa mínima [R]: editar e responder (o fio já tem), menções com aviso,
não lidas por canal, notificação do sistema, reações, atalho global da tecla de
falar, desbanir e expulsão que segura, markdown mínimo, presença visível.

Faltas de operação [R]: backup e restore, guia de deploy (systemd, Docker),
saúde e métricas, retenção configurável, log de auditoria, matriz de
compatibilidade cliente × servidor, política escrita de estabilidade do fio e da
API de MOD.

Features que aproveitam o que já existe:

| Feature | Onde encaixa | Custo |
|---|---|---|
| Editar e responder de ponta a ponta | `ClientMessage`, `session.rs`, `tela-sessao.js` | P |
| `seeled backup/restore/doctor` | `VACUUM INTO` no binário do servidor | P |
| Soundboard e música como MOD | API 8 existente | P |
| Menções, não lidas por canal, notificação nativa | `Mentioned` e `nao_lidas` já existem | M |
| Painel do anfitrião (sessões, banda, descartes, disco) | contadores de `voice_room.rs` | M |
| Log de auditoria visível | migração nova, `camada-moderar.js` | M |
| Reações como MOD oficial | contribuições da API 8 | M |
| Catálogo de máquinas (Terraria, Valheim, Factorio) | repositório do MOD Multiplayer | M por jogo |
| Túnel genérico «abrir porta para a sala» | `tuneis.rs` | M |
| Tecla de falar global | plugin de atalho global do Tauri | M |
| Identidade portátil por QR ou arquivo cifrado | `identity.rs`, `identidade-web.js` | M |
| Clipe dos últimos 30 s, local e com aviso | `seele-core/voice.rs` | M |
| Webcam como segundo fluxo | `tela.rs`, limite por sala | G |

## 9. Decisões que são suas

1. **Licença.** Qual.
2. **Relay.** Nenhum (hoje); TURN de quem usa (não resolve o CGNAT); relay
   cifrado pelo ponto de encontro (cobre quase tudo, contradiz «nenhum servidor
   no meio»); relay por um par da sala (não resolve o primeiro convidado).
3. **Conversa pessoal.** Qual dos dois caminhos fica.
4. **Linux.** Suportar, declarar «compila do fonte, sem tela», ou cortar.
5. **Assinatura dos binários** no macOS e no Windows.
6. **Idioma.** 1.0 só em pt-BR, ou estrutura para um segundo.
7. **O que a 1.0 congela.** Fio, esquema do SQLite, API de MOD, URI de convite e
   modelo de identidade. O ponto de encontro e a identidade «uma por aparelho»
   não estão prontos para congelar.
8. **Nome.** Já existem «Servidor 1.0» e «Servidor 2.0» no protocolo; um «app
   1.0» sobre «Servidor 2.0» vai confundir.

## 10. Ordem sugerida

1. **Rede de proteção** (B1, B4, pins do release): sem CI verde, todo o resto é
   consertado no escuro.
2. **Consertos pequenos e certos** (B3, B5, B6, B7, B12, Argon2 fora do `lock`,
   permissão do banco, balde por /64).
3. **Dizer a verdade** (B8, frases, «copiar diagnóstico», página de diagnóstico
   ligada).
4. **Medir** (B9, e o streaming sob perda), antes de escolher entre RTP e canal
   não confiável.
5. **Estrutural** (vídeo fora do canal confiável, um caminho para a conversa
   pessoal, furo IPv6).
6. **Mesa mínima e operação** (B10, B11, notificações).
7. **Refatoração** (guardas de texto, depois os arquivos gigantes).

## O que esta revisão não cobriu

- Nenhum build, teste ou medição em execução; nenhum teste em aparelho real, em
  Windows ou em Safari.
- Os achados marcados [R] não foram reconferidos um a um.
- A maior parte dos 234 comandos de `main.rs`; `tunel.rs`, `tofu.rs`, `mods.rs`
  e `catalogo.rs` do cliente; o código das linhas antigas que o lançador abre.
- Internos do str0m, do `dimpl` e do `tauri-plugin-updater`.
- Telas de auth, compartilhar, MODs, sessões e perfis não foram renderizadas.
- Leitor de tela e ordem de foco.
