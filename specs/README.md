# SEELE — Especificações

Sistema de comunicação por voz e texto auto-hospedado. O desenho original era operado via terminal, com clientes gráficos opcionais; o cliente de terminal saiu no ADR 0039 e o produto tem uma casca gráfica só.
Estética de console de operação: densidade de informação e hierarquia de comando. O vocabulário é do `docs/glossario.md` desde o ADR 0033.

## Como usar esta pasta

Estes documentos são o desenho original, escrito antes do código. Eles descrevem **o que** construir e **por quê**; não contêm código de produção. Cada um abre com um bloco «Estado na 1.0» que diz o que continua valendo e o que mudou. Leia na ordem numérica na primeira passada, e leia «Estado», no fim desta página, antes de confiar num detalhe.

| Arquivo | Assunto |
|---|---|
| `00-visao-geral.md` | Produto, escopo, não-objetivos, critérios de sucesso |
| `01-arquitetura.md` | Workspace de crates, decisões técnicas e trade-offs |
| `02-protocolo.md` | Formato de mensagens, handshake, canais de dados do WebRTC (até a 0.16, QUIC) |
| `03-audio.md` | Pipeline de captura, codec, jitter buffer, mixagem |
| `04-servidor-seele.md` | Estado, permissões, subsistemas, persistência |
| `06-clientes-gui.md` | O app desktop, o celular, a camada FFI e as regras de acessibilidade |
| `07-estetica.md` | Densidade, tokens de cor, tipografia, movimento, voz da interface |
| `08-seguranca.md` | Transporte, autenticação, caminho para E2EE |
| `09-roadmap.md` | Milestones M0–M6 com critérios de aceite |
| `10-convencoes.md` | Estilo, testes, CI, commits, versionamento |

Não há `05`: a `05-cliente-tui.md` saiu com o cliente de terminal (ADR 0039), e as regras de acessibilidade dela foram para a `06`.

## Estado

O produto está implementado e a caminho da 1.0. **As specs são o desenho original. Onde o código divergiu, vale o código e o ADR correspondente**, e o bloco «Estado na 1.0» no topo de cada spec aponta para os dois.

As marcas **[EM ABERTO]** que sobraram no corpo das specs são do desenho. Cada uma que foi decidida virou um ADR em `docs/adr/`; o bloco do topo diz quais.

Divergências conferidas contra o código em 10/11/2026:

| Spec | O que mudou | Quem manda agora |
|---|---|---|
| 00, 01, 06, 09, 10 | O cliente de terminal (`seele-tui`) saiu; o produto tem uma casca só | ADR 0039; `Cargo.toml` da raiz |
| 01, 02, 08, 09 | O transporte é WebRTC com canais de dados (ICE, DTLS, SCTP) num socket UDP só, e não QUIC. Não há RTP | ADR 0063; `crates/seele-enlace` |
| 01, 02, 08 | O anfitrião é ICE completo e controlado, e não ICE-lite | ADR 0063, emenda de 26/10/2026; `crates/seele-enlace/src/ponta.rs` |
| 01, 08 | Não há TURN nem relay | ADR 0063, emenda de 26/10/2026 |
| 02 | A serialização é `postcard`; a faixa de versões da linha 2 é de 10 a 12; há os canais `t` e `j` | ADR 0001; `crates/seele-proto/src/version.rs` e `transport.rs` |
| 02, 10 | O quadro é carimbado com a versão negociada, e não com a global | `crates/seele-proto/src/version.rs` (`control::encode_em`) |
| 03 | Opus pelo `shiguredo_opus`; ativação por voz sem `webrtc-vad`; ganho e supressão de ruído existem; jitter buffer e mixer em `seele-voz` | ADRs 0008, 0015 e 0055; `crates/seele-audio`, `crates/seele-voz` |
| 04 | Não há arquivo TOML, endpoint de saúde nem métricas. A operação é por linha de comando e `SEELE_DB`, com `backup`, `restore` e encerramento limpo em `SIGTERM` e `SIGINT` | `crates/seele-server/src/main.rs` e `copia.rs` |
| 00, 09 | Compartilhamento de tela existe (H.264), apesar de listado como não-objetivo e pós-v1 | ADRs 0040 e 0041; `crates/seele-video` |
| 00 | MODs existem, com API versionada | ADR 0045; `api/` |
| 06, 09 | O celular entra pelo navegador, sem hospedar. Não há Flutter nem app nativo, e o `uniffi` não entrou | ADRs 0057 e 0018; `crates/seele-web` |
| 07 | A queda não se apoia em migração de conexão; `--seele-raio` vale `16px`, e não `0` | ADR 0063; `apps/seele-app/ui/tokens.css` (sem ADR) |
| 08, 10 | As exceções ao `unsafe` são seis crates com `deny`, e a `seele-ffi` não é uma delas | `Cargo.toml` de cada crate; ADR 0041 |
| 09 | Os releases publicam macOS arm64 e Windows x64; no Linux compila-se do código, sem captura de tela | `crates/seele-video/src/captura/mod.rs` |
| 10 | Há código em português nos crates novos, contra a regra do inglês | `crates/seele-enlace` (sem ADR; o 0023 registra o começo) |

Esta tabela é um retrato, e não um guarda: nada no CI a confere. Quem mudar uma decisão estrutural atualiza o bloco da spec e a linha daqui no mesmo commit do ADR.
