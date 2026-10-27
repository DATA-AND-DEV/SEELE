# 0063 — A linha 2 fala WebRTC

Status: **aceito**. É a orientação de quem responde pelo projeto em 24/10/2026: a 0.17 quebra a compatibilidade, troca o QUIC pelo WebRTC, e da 0.16 para trás é legado. Os detalhes são as decisões D1 a D9 do contrato da fase 2 da 0.17.
Data: 2026-10-24

**Emenda** o [0002](0002-regra-de-dependencia.md) (o crate de transporte comum), o [0022](0022-alcancar-um-dogma-pela-internet.md) (o TURN, que é o degrau 5: desde 26/10/2026 fora da linha 2, por decisão do dono), o [0057](0057-o-celular-entra-pelo-navegador.md) (o navegador entra por WebRTC, e não por WebTransport), o [0058](0058-1x-nunca-quebra-o-fio.md) (a linha 2 sem recusador `seele/1`) e o [0062](0062-o-registro-assinado-no-ponto-de-encontro.md) (o ENC/3).

## Contexto

O navegador só alcançava um servidor direto (`empacotar/notas/0.16.0.md`, «A página alcança o servidor direto, e não pelo ponto de encontro»), porque a WebTransport não fura NAT. O WebRTC fura, e é o mesmo nos dois lados. Ter dois transportes ao mesmo tempo, QUIC e WebRTC, dobraria a superfície que recebe bytes não confiáveis e os testes. O dono escolheu a quebra a ter os dois.

## Decisão

1. **Um transporte só, WebRTC.**
   - O `str0m` roda no crate novo `seele-enlace`, que é comum ao `seele-core` e ao `seele-server`, com um provedor de cripto local passado por instância. O DTLS (`dimpl`) usa o backend **`aws-lc-rs`** desde a emenda de 25/10/2026, abaixo; até ali usava o `rust-crypto`.
   - O anfitrião é **ICE completo e controlado** com DTLS passivo, num socket UDP só (8383); quem liga (o nativo e a página) é o controlador e nomeia. O socket separa pelo primeiro byte (RFC 7983): STUN de 0 a 3, DTLS de 20 a 63, o resto é o texto do ENC/3. *Emenda de 26/10/2026:* até aqui o anfitrião era ICE-lite. Um anfitrião lite não checa (RFC 8445 2.5), e atrás de um NAT de casa que muda a porta conforme o destino quem liga nunca aprendia a porta certa (medido no iPhone em 5G, num ensaio com um TURN temporário da homologação, já removido: o anfitrião falava com o encontro por 14178 e o furo dele para o relay saía por 14224; o navegador só checava 14178). Agora o `CHAMADO` do ENC/3 cria o par já com o U e os candidatos de quem liga (pelos mesmos portões, tetos e lápide do Binding), o ICE do str0m checa cada um com o MESSAGE-INTEGRITY do U, e quem liga aprende o peer-reflexive (RFC 8445 7.3.1.3). A página deixou de anunciar `a=ice-lite`. O fio publicado (ENC/3, SEELE-SINAL/1, o envelope) não mudou, nem o DTLS, os pinos e a apresentação.
   - Quem liga sintetiza a descrição remota a partir do convite, sem troca de SDP.
   - O QUIC, a WebTransport e os certificados curtos saem.
   - **Hospedar continua só nos nativos.**
2. **O fio SEELE por cima é o mesmo.** Os mesmos quadros, enquadramento, listas, recusa `[c, 11, 0]` e prova.
   - O canal `ctl` (id 0) leva o controle, e o `voz` (id 1, sem ordem e sem retransmissão) leva os datagramas de voz.
   - Os fluxos extras são `u` e `b`, com `protocol = "seele/2"` no DCEP.
   - Todo canal menos a voz tem um envelope de 1 byte: dados, fim, corte e apresentação.
3. **A identidade é a de sempre.**
   - O certificado DTLS do `seeled` **é** o certificado longo P-256 que ele já guarda. Então a `a=fingerprint` é o `fp=` do convite, e **os pinos da 0.16 continuam valendo** para o mesmo banco.
   - A sala pessoal (Ed25519) usa um DTLS P-256 efêmero e manda a assinatura da sala sobre a impressão DTLS, na apresentação.
   - A prova do cliente ganha um rótulo novo, que amarra também a impressão DTLS do cliente.
4. **Linha 2, a partir do 10.** O protocolo é o 10, o Servidor é o 2.0 e a linha é `"2"`.
   - A tabela congelada passa a `8→"0.15"`, `9→"1"`, `10..→"2"` (`seele_proto::version`).
   - A linha 1 **não recebe versão nova**: o 9 é o único número dela, e foi o único publicado. Por isso o 10 pôde ser da linha 2 sem que número nenhum trocasse de linha em campo.
5. **Sem recusador `seele/1`.**
   - O 0058 (`:52-53`) pedia que uma 2.0 aceitasse o `seele/1` só para recusar uma 1.x de forma legível. Isso exigiria manter o QUIC, e a ordem foi não manter.
   - A recusa da linha 1 sai por quatro caminhos, todos antes de discar:
     - o `v=` do link, que a 0.16 lê como «mais nova»;
     - a magia do ENC/3, porque o anfitrião 0.17 não publica `MORO2`;
     - a frase da página;
     - o launcher.
   - O ALPN `seele/1` e o `[c, 11, 0]` continuam congelados. Na linha 2, a recusa `[c, 11, 0]` é o primeiro quadro do `ctl`.
6. **O NAT pelo ponto de encontro (ENC/3).**
   - Quem liga manda um `CHAMA` com o próprio candidato: o navegador por HTTPS, o nativo por UDP. O anfitrião fura e responde `ATENDE`.
   - O STUN padrão é o do ponto do convite, e é o único servidor ICE. *Emenda de 26/10/2026, decisão do dono:* **sem TURN** — «SEELE é código na máquina de cada um; a VPS é só ponto de encontro». A página não tem mais configuração de TURN nem espera candidato relay, e o produto não oferece relay como caminho. O caminho é direto: o furo do ENC/3 e o ICE completo do anfitrião (o peer-reflexive). O que o direto não alcança fica sem caminho, e isso é dito, não contornado por um relay.

## Alternativas

- **QUIC entre nativos, WebRTC só no navegador.** Recusada pelo dono: são dois transportes para manter, medir e defender.
- **Manter um recusador QUIC mínimo** para cumprir o 0058 ao pé da letra. Recusada pela mesma ordem. O custo é a limitação abaixo.
- **Um gateway que termina o TLS** (já recusado no 0057, alternativa 4). O TURN não termina o DTLS e por isso não lê o conteúdo.

## Consequências

- **A 0.17 não conversa com a 0.16 em nenhuma direção.**
  - Quem precisa da 0.16 a abre pelo launcher (0046). A primeira abertura da 0.17 guarda os dados da 0.16 para a linha 1 (`apps/seele-app/src/versoes.rs`).
  - **Limitação:** uma 0.16 que disca um endereço digitado, sem link, para um anfitrião 0.17 vê «nada respondeu».
- **O botão de atualizar avisa antes de cruzar a linha** (`apps/seele-app/src/atualizador.rs`). Isso só vale da 0.17 em diante. Quem está na 0.16 lê só as notas do `latest.json` antes de instalar, e não há diálogo (`docs/versoes-lado-a-lado.md`).
- O guarda PUBLICADA mede a v0.16.0, e os bytes-ouro da linha 1 viraram registro congelado (`crates/seele-proto/tests/fio_1_0.rs`). Os da linha 2 estão em rascunho até a 0.17 sair.
- **O navegador não diz por que o DTLS falhou.** A página só sabe que a verificação da conexão segura falhou, e não a causa. «A impressão é diferente» é afirmado só no nativo, que vê o certificado, e no alerta TOFU.
- **Regressão aberta: a voz durante uma transferência.** No spike, a voz perdeu de 8 a 9 % na subida durante uma rajada (`docs/evidencias/spike-webrtc-0.17.md`). Fica registrada como regressão até ser remedida com o limitador real (R4, em `specs/08-seguranca.md`).
- **A sonda de subida do [0044](0044-o-portao-divide-a-subida-medida.md) muda.** Ela lia do QUIC os bytes enviados, as perdas, os eventos de congestionamento e o RTT, e o SCTP do WebRTC não dá os mesmos números. A decisão é do native-transport (B e E): um comportamento conservador, sem essa telemetria.
  - **Pendente:** a decisão entra aqui, com a evidência dela.
  - Se a qualidade da tela mudar de forma material, entra também nas notas da 0.17.
- **Não está medido**, e não pode ser afirmado nas notas sem a medida:
  - a travessia de NAT real entre duas casas (sem relay: o que o caminho direto não atravessa fica sem caminho);
  - o navegador do iPhone;
  - a latência da voz com uma transferência grande na mesma associação (R4 da revisão do coordenador).

## Emenda de 25/10/2026 — o DTLS passa ao AWS-LC (RUSTSEC-2026-0387)

**O que mudou.** O `dimpl` 0.7.6 (o DTLS do `str0m` 0.24.1) deixou a feature `rust-crypto` e passou à `aws-lc-rs`, e só a ela: as duas não se somam (`crates/seele-enlace/Cargo.toml`). O backend é nomeado por instância (`Config::builder().with_crypto_provider(dimpl::crypto::aws_lc_rs::default_provider())`, em `crates/seele-enlace/src/cripto.rs`), sem provedor instalado no processo e sem a feature `rcgen` do `dimpl`. O resto ficou igual: o provedor local do `str0m` (SRTP, SHA-256 e o HMAC-SHA1 do STUN, em RustCrypto), o certificado gerado pelo `rcgen` com o `ring`, o certificado longo P-256 do `seeled`, a impressão (SHA-256 do DER) e a prova amarrada.

**Por quê.** É um gate de segurança, e não preferência. O `cargo deny check advisories` reprova no RUSTSEC-2026-0387 (o `crypto-bigint` < 0.7.6, sem tempo constante; aviso de 23/10/2026). A cadeia era `dimpl/rust-crypto` → `ecdsa`/`p256`/`p384` → `elliptic-curve` 0.13 → `crypto-bigint` 0.5.5, e o `p256` assina o aperto DTLS com ele. A correção está numa faixa semver que a família `elliptic-curve` 0.13 não aceita. Recusados: um `ignore` no `deny.toml` (calaria uma vulnerabilidade, o que a lista do `deny.toml` nunca aceitou), um fork ou backport de cripto e cripto escrita aqui. A escolha anterior do RustCrypto foi uma decisão técnica desta rodada (não trazer C ao build, como o `rustls` do `seele-server` registra), e não uma proibição de quem responde pelo projeto.

**O custo.** Entram o `aws-lc-rs` 1.18 e o `aws-lc-sys` 0.46, que compila o AWS-LC (C e assembly) no build:

- um compilador C em toda máquina que compila: o clang no macOS (as ferramentas de linha de comando), o gcc ou o clang no Linux, o MSVC no Windows;
- no Windows x86_64, o NASM — **dispensado** pela feature `prebuilt-nasm`, que o `dimpl` liga: sem NASM no `PATH`, o `aws-lc-sys` usa os objetos pré-montados que traz (e, com NASM instalado, usa o NASM). O Windows ARM64 exigiria o `clang-cl`, e não é alvo;
- o CMake **não** é exigido: com as ligações pré-geradas dos quatro alvos do `deny.toml`, o `aws-lc-sys` usa o builder `cc`. O CMake só entra se alguém pedir (`AWS_LC_SYS_CMAKE_BUILDER=1`) ou desligar o assembly;
- o build fica mais longo (a primeira compilação do AWS-LC) e o binário carrega uma segunda biblioteca de cripto em C ao lado do `ring`;
- licenças: ISC, Apache-2.0, MIT, MIT-0 e BSD-3-Clause, já admitidas pelo `deny.toml`, que não mudou.

**A prova.** `cargo tree -i crypto-bigint` e `cargo tree -i elliptic-curve` (`--workspace --all-features`) dão vazio; `cargo deny check advisories licenses bans sources` passa. Revalidados com o binário novo, no macOS: os testes do `seele-enlace` (o aperto, a impressão errada recusada, os portões, o crédito, a pilha dupla), a identidade da sala e do `seeled` e a prova amarrada no `seele-server`, a recusa pela impressão no `seele-core`, a reconexão, a voz e a tela integradas no `seele-conformance`, e as bancadas `rtc-eco`, `rtc-credito`, `rtc-sinal` e `credito-no-navegador` no Chromium e no WebKit. **O Windows e o Linux não foram executados localmente nesta emenda**: o que os prova é o CI (`clippy` e `test` nas três plataformas). As medidas anteriores de interoperabilidade e de transporte são do backend antigo.
