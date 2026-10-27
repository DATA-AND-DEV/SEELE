# 08 — Segurança

## Postura

Servidor auto-hospedado, comunidades pequenas, operador confiável mas não necessariamente especialista em segurança. O objetivo é ser **seguro por padrão sem configuração**, e nunca oferecer um caminho não criptografado "para facilitar".

## Modelo de ameaça

| Ameaça | Tratamento em v1 |
|---|---|
| Escuta passiva na rede | TLS 1.3 obrigatório via QUIC (linha 1); DTLS do WebRTC desde a 0.17 (linha 2, ver «Transporte») |
| Man-in-the-middle no primeiro contato | TOFU com pinning e aviso explícito na troca de chave |
| Cliente malicioso falando em VoiceRoom sem permissão | Validação servidor-side em todo datagram |
| Cliente forjando identidade de outro | `ssrc` atribuído pelo servidor, nunca aceito do cliente |
| Saturação de CPU/banda por flood | Limite de quadros por segundo por remetente, desconexão progressiva |
| Vazamento de histórico por acesso ao disco do servidor | **Fora de escopo em v1** — documentar claramente |
| Operador do servidor lendo o áudio | **Fora de escopo em v1** — E2EE é roadmap, ver abaixo |
| Enumeração de usuários | Mensagens de erro de login uniformes |

## Transporte

**Desde a 0.17 (linha 2, ADR 0063): o DTLS do WebRTC**, pelo `str0m` com um provedor de cripto local (`crates/seele-enlace/src/cripto.rs`), passado por instância. Não há modo texto puro, nem flag para desabilitar.

- **O DTLS é o `dimpl` com o backend `aws-lc-rs`, e só ele** (emenda de 25/10/2026 ao ADR 0063). Até ali era o `rust-crypto` do `dimpl`, que trazia o `crypto-bigint` 0.5.5 por baixo do `p256`/`p384` e caiu no RUSTSEC-2026-0387 (aritmética sem tempo constante). O `cargo deny` reprova nisso e **continua reprovando**: nada de `ignore` para vulnerabilidade. `cargo tree -i crypto-bigint --workspace --all-features` tem de dar vazio.
- O resto do provedor local (o SRTP, o SHA-256 e o HMAC-SHA1 do STUN) continua em RustCrypto, e o certificado continua do `rcgen` com o `ring`: a impressão e a prova abaixo não mudaram.
- **Custo de build:** o `aws-lc-sys` compila C e assembly, então quem compila precisa de um compilador C (clang, gcc ou MSVC). O NASM do Windows x86_64 é dispensado pelos objetos pré-montados (`prebuilt-nasm`), e o CMake não é exigido. Ver `docs/windows.md`.

O que a linha 1 garantia pelo TLS do QUIC continua, assim:

- **A identidade do servidor.**
  - O certificado DTLS do `seeled` é o certificado longo **P-256** que ele já guarda (`seele-server/src/tls.rs:54-63`). A `a=fingerprint` é o `fp=` do convite, e **os pinos TOFU de quem já entrou continuam valendo** para o mesmo banco.
  - Quem liga **escreve** a impressão esperada, e ela não vem da rede. O certificado DTLS do par é conferido contra ela antes do `Hello`.
  - **O que cada lado sabe dizer quando a conferência falha.**
    - O nativo sabe qual impressão veio, e é só ele que diz «impressão diferente», no alerta TOFU, com a impressão vista.
    - O navegador não expõe o motivo de uma falha de DTLS. A página só sabe que **a verificação da conexão segura falhou**, sem saber a causa, e não pode afirmar impressão diferente.
  - A sinalização (ENC/3 e `/sinal/1`) é terceiro não confiável. Ela só leva endereços e a credencial ICE, e uma resposta falsa custa não entrar.
  - Pendente no A: um banco antigo cuja chave guardada não seja P-256 é recusado com erro nomeado, e nunca regerado calado. Regerar troca a chave e dispara «A CHAVE MUDOU».
- **A sala pessoal (Ed25519).** O DTLS P-256 da sala é efêmero. A identidade vai na apresentação `0x03` do `ctl`: a chave e a assinatura da sala sobre `rótulo ‖ fp_dtls`, conferidas pelo `ConfereCodigo` antes do `Hello`.
- **A prova do cliente.**
  - O rótulo novo é `SEELE/2 prova-de-cliente\0`, e a assinatura cobre `rótulo ‖ fp_servidor ‖ fp_dtls_cliente ‖ nonce`.
  - O servidor confere o `fp_dtls_cliente` contra o certificado DTLS que viu.
  - Uma prova feita para uma conexão não vale para outra, nem para outro servidor.
- **As credenciais ICE não são segredo.** O `U` é sorteado por quem liga. A segurança é o DTLS mais a prova.
- **Limites antes de alocar, no enlace.**
  - 64 canais por conexão.
  - Fila de entrada de 1 MiB por canal; quem estoura é cortado e contado.
  - Mensagem SCTP de até 16385 B, voz de até 1100 B, apresentação de até 512 B.
  - O `MAX_FRAME_LEN` do controle continua valendo por cima do envelope.
- **Taxa antes de criar estado.**
  - O balde por endereço (ADR 0025) é consultado antes de criar um `Rtc` para um Binding novo, com teto de pendentes (64) e teto por IP (8).
  - Antes dele, o balde de criações do enlace: no máximo 8 apertos **que não terminaram** por IP em 10 s. Para o laço de criações que nunca terminam (U novos sem DTLS, o navegador recriando o par depois de um alerta fatal). Um aperto que termina devolve a vaga: muitas pessoas atrás do mesmo NAT são limitadas pelo balde do servidor (30 de rajada, 30 por minuto, `seele-server/src/taxa.rs`), e não por este. Cada recusa do enlace tem contador e `tracing` próprios (`ContadoresDaPonta::recusa_*`).
  - O balde de controle depois do `Hello` é o de sempre.
- **TURN.**
  - É opcional, configurado por quem usa (URL, usuário e credencial), nunca no link, nunca em log, e sem padrão.
  - O relay não termina o DTLS e não lê o conteúdo, mas vê os dois endereços, o volume e o horário.
  - O operador do relay é a pessoa ou quem ela escolheu, e não este projeto.

### Riscos novos do anfitrião ICE-lite (revisão do coordenador)

- **R2: reflexão por Indications.**
  - O anfitrião manda Binding Indications aos candidatos do `CHAMA`, que **não são autenticados**: um terceiro os escolhe. A amplificação é baixa (de 20 a 28 B), mas é um refletor.
  - Exigido do A: um teto de Indications por `CHAMADO` (5 por candidato, até 3 candidatos), um teto de `CHAMADO`s por minuto no anfitrião, e o `serve_de_candidato` também no anfitrião.
  - O fuzz `datagrama_do_encontro` prende o teto e a regra dos candidatos lidos.
  - **O comportamento do anfitrião ainda não foi conferido.**
- **R3: estado criado por um STUN não autenticável.**
  - O `U` é de quem liga, então qualquer origem UDP, inclusive forjada, cria um `Rtc` pendente. Com os tetos de 64 e 8, origens forjadas bloqueiam os legítimos por até 10 s.
  - Mitigações, da mais barata para a mais cara:
    - (a) responder o primeiro Binding sem estado e só criar o `Rtc` quando chegar DTLS daquela origem;
    - (b) despejar o pendente mais velho;
    - (c) um prazo de 2 s para o pendente que não começou o DTLS.
  - O exigido é pelo menos (b) e (c). **Ainda não conferido.**
- **R3b: laço de recriação depois de um alerta DTLS fatal.**
  - Medido no spike C com a **página**, no Chromium 149 e no WebKit 26.5, com a impressão adulterada na resposta sintetizada (`docs/evidencias/spike-webrtc-0.17.md`, seção «Para o enlace de produto»):
    - o navegador manda o alerta fatal 46;
    - o navegador continua mandando Bindings de consentimento a cada 2 a 3 s;
    - o anfitrião recriava o `Rtc` a cada um deles, com o mesmo `U`: foram 4 `Rtc` em 70 s no WebKit e 2 em 30 s no Chromium.
  - Correção pedida ao A: um tombstone do `U` por um prazo, um backoff por IP e **nunca** um segundo `Rtc` para um `U` vivo, com um teste de regressão que reprova sem a correção.
  - **Ainda não conferido.**
- **R4: justiça entre a voz e a transferência.**
  - Uma associação SCTP leva tudo. Um anexo ou a tela enchendo o buffer põe a voz atrás de até 1 MiB, cerca de 4 s a 2 Mbit/s.
  - Exigido nos dois lados (o nativo e o `rtc-web.js`):
    - a voz e o `ctl` saem na hora;
    - os canais `u` e `b` só escrevem enquanto o `buffered_amount` **total** está abaixo de 64 KiB;
    - os canais de volume se revezam (round-robin).
  - Critério: o p95 da voz durante uma rajada de 32 MiB fica abaixo de 100 ms acima do repouso, em loopback. **Ainda não medido.**
  - **Regressão aberta.** No spike, a voz perdeu de 8 a 9 % na subida durante uma rajada (`docs/evidencias/spike-webrtc-0.17.md`). Ela continua registrada como regressão até o browser-signaling remedir com o limitador real.

### A linha 1 (até a 0.16)

TLS 1.3 embutido no QUIC. Não há modo texto puro, nem flag para desabilitar. Cipher suites: padrão do `rustls`, sem downgrade.

**Certificados — [EM ABERTO, decidir em M0]:**

- **TOFU (trust on first use)** com certificado auto-assinado: o cliente memoriza a chave pública na primeira conexão e alerta em voz alta se ela mudar. Amigável para auto-hospedagem, é o modelo do SSH, e o público entende. Exige UX explícita de aceite e de troca de chave.
- **ACME / Let's Encrypt**: confiança de CA pública, exige domínio e porta 80/443 disponíveis.

Recomendação: TOFU como padrão, ACME como opção documentada. O aviso de mudança de chave precisa ser impossível de ignorar — no tema, é literalmente um `Alerta · 警告` bloqueante.

## Autenticação

**[EM ABERTO — escolher em M2]** Duas direções:

1. **Chave pública (Ed25519)** — o cliente gera par de chaves no primeiro uso; o servidor guarda a pública. Desafio-resposta no handshake. Sem senha, sem hash para vazar, e prepara terreno para E2EE. Custo: recuperação de conta e uso em múltiplos dispositivos precisam de fluxo próprio.
2. **Senha** — Argon2id com parâmetros modernos, salt por usuário. Familiar, fácil de usar em vários dispositivos. Traz todos os problemas conhecidos de senha.

**Decidido em M5, exatamente como recomendado** (ADR 0021). Chave pública Ed25519 como identidade; entrada no servidor por **convite de uso único** (160 bits, sete dias, consumido atomicamente) ou por **senha do servidor** em Argon2id, à escolha do operador. Um servidor sem nenhum dos dois é aberto, que segue sendo o padrão para rede local — e o `seeled` avisa em voz alta ao subir assim escutando fora do loopback.

O segredo viaja no `Hello`, antes do desafio-resposta: gastar verificação de assinatura com quem não devia estar batendo à porta é trabalho de graça para quem varre a internet. A recusa é sempre `CredentialRejected`, uniforme, com o motivo real só no log do operador.

O convite é o que torna um link compartilhável defensável — ver o esquema `seele://` no ADR 0006, que carrega endereço, impressão digital do certificado e convite, mas **nunca** a senha.

Independente da escolha:
- Rate limiting de tentativas, com backoff por IP e por identidade.
- Mensagem de falha uniforme (não revelar se a conta existe).
- Sessões com expiração e revogação servidor-side.

**Limitação de taxa decidida em M5** (ADR 0025), em balde de fichas e não em janela fixa. Antes de autenticar, por endereço de origem: trinta apertos de mão de rajada, trinta por minuto, consultados antes de o `Hello` ser lido — é o que impede que cada pacote de quem varre a rede compre um Argon2id de CPU do anfitrião. Depois de autenticar, por **conexão** e não por identidade: a mesma pessoa em duas máquinas são duas conexões legítimas, e quem abre conexões em série para diluir o limite esbarra antes no balde por endereço. Sessenta quadros de controle de rajada, vinte por segundo; o primeiro excedente rende `AlertReason::RateLimited` e o ducentésimo derruba com `DisconnectReason::RateLimited` — avisar antes de derrubar, porque derrubar calado é como um produto passa a parecer quebrado. O limite de quadros de mídia da tabela de ameaças acima usa o mesmo balde, descartando em vez de desconectar.

## Autorização

Toda ação é verificada no servidor, sempre, mesmo que o cliente já esconda o botão. A interface esconder é conveniência; o servidor negar é a segurança. Cobrir isso com testes explícitos: para cada permissão, um teste de cliente sem ela tentando a ação.

## Privacidade de mídia — caminho para E2EE

O desenho SFU (`01`) foi escolhido em parte por isso: **o servidor já não decodifica áudio**. Adicionar criptografia fim-a-fim é um incremento, não uma reescrita.

Esboço para pós-v1:
- Chave de mídia por VoiceRoom, negociada entre participantes.
- Rotação da chave a cada entrada e saída (forward secrecy em relação a quem sai).
- Cabeçalho do datagram permanece em claro (o servidor precisa de `ssrc` e `seq` para encaminhar); apenas o payload Opus é cifrado.
- Verificação de identidade fora de banda entre pessoas (comparação de fingerprint).

**Não prometer E2EE em v1.** Documentar honestamente: em v1, o operador do servidor pode, em teoria, capturar mídia. Isso é aceitável para o modelo de uso (você hospeda para seu próprio grupo), mas precisa estar escrito.

## Anexos: uma diferença de esforço que merece frase própria

O ADR 0027 pôs arquivos no disco de quem hospeda, e isso não é uma ameaça nova —
a linha «vazamento de histórico por acesso ao disco do servidor» acima já cobre
o direito. É uma diferença de **esforço**, e é a diferença que faz alguém se
surpreender: o histórico de texto está dentro de um SQLite e exige saber
perguntar; um diretório `anexos/` é navegável por qualquer gerenciador de
arquivos, e uma foto se lê de relance.

**Guardado em claro, e documentado.** Cifra em repouso foi considerada e
recusada: a chave teria de estar viva no mesmo disco, porque o servidor precisa
servir o arquivo a quem tem permissão a qualquer momento. Isso protege contra
notebook roubado e contra nada mais — e contra notebook roubado a cifra de disco
inteiro do sistema protege melhor, é o que a pessoa já tem, e ela pode ligar
hoje.

Em trânsito nada muda: o DTLS do WebRTC desde a 0.17 (até a 0.16, o TLS 1.3 dentro do QUIC), sem modo claro. **O arquivo não é
legível na rede. É legível no disco de quem hospeda.**

**Um servidor não varre vírus.** Não há motor, não há base de assinaturas e não há
caminho de atualização para uma. O que ele confere é se o arquivo chegou
inteiro — tamanho contra o declarado, conteúdo contra o hash declarado — e é a
única pergunta que ele consegue responder. **Não há lista de extensões
proibidas**, de propósito: uma lista é contornada com um `rename`, quebra usos
legítimos, e — pior que as duas coisas — faz o que passou parecer conferido.

O que o produto pode fazer, e faz: ao gravar, marca o arquivo com a quarentena do
próprio sistema (`com.apple.quarantine`, o fluxo `Zone.Identifier`), que é o que
faz o Gatekeeper e o SmartScreen pararem o arquivo na frente de quem for
abri-lo. E **nenhum cliente do SEELE abre arquivo**: salvar é um ato de quem
recebeu, num lugar que a pessoa escolheu.

## Práticas de código

- `#![forbid(unsafe_code)]` em todos os crates exceto `seele-ffi` e bindings de áudio, onde `unsafe` é justificado por comentário caso a caso.
- `cargo deny` e `cargo audit` no CI, falhando o build em vulnerabilidade conhecida.
- Nenhum segredo em log. Nenhum payload de mídia em log, nem em nível trace.
- Fuzzing dos parsers de `seele-proto` (`cargo-fuzz`) — é a superfície que recebe bytes de rede não confiáveis. Desde a 0.17, também o ENC/3 (no alvo `datagrama_do_encontro`), o corpo do `/sinal/1` (`sinal_do_encontro`) e o envelope dos canais do `seele-enlace` (`envelope`).
- Toda entrada de rede tem limite de tamanho antes de alocar.

## Não fazer

- Telemetria enviada a terceiros. O produto não fala com ninguém além do servidor escolhido.
- Crash reporting automático sem consentimento explícito.
- Qualquer criptografia caseira. Só primitivas de bibliotecas auditadas.
