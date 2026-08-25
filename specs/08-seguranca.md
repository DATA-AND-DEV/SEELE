# 08 — Segurança

## Postura

Servidor auto-hospedado, comunidades pequenas, operador confiável mas não necessariamente especialista em segurança. O objetivo é ser **seguro por padrão sem configuração**, e nunca oferecer um caminho não criptografado "para facilitar".

## Modelo de ameaça

| Ameaça | Tratamento em v1 |
|---|---|
| Escuta passiva na rede | TLS 1.3 obrigatório via QUIC |
| Man-in-the-middle no primeiro contato | TOFU com pinning e aviso explícito na troca de chave |
| Cliente malicioso falando em VoiceRoom sem permissão | Validação server-side em todo datagram |
| Cliente forjando identidade de outro | `ssrc` atribuído pelo servidor, nunca aceito do cliente |
| Saturação de CPU/banda por flood | Limite de quadros por segundo por remetente, desconexão progressiva |
| Vazamento de histórico por acesso ao disco do servidor | **Fora de escopo em v1** — documentar claramente |
| Operador do servidor lendo o áudio | **Fora de escopo em v1** — E2EE é roadmap, ver abaixo |
| Enumeração de usuários | Mensagens de erro de login uniformes |

## Transporte

TLS 1.3 embutido no QUIC. Não há modo texto puro, nem flag para desabilitar. Cipher suites: padrão do `rustls`, sem downgrade.

**Certificados — [EM ABERTO, decidir em M0]:**

- **TOFU (trust on first use)** com certificado auto-assinado: o cliente memoriza a chave pública na primeira conexão e alerta em voz alta se ela mudar. Amigável para auto-hospedagem, é o modelo do SSH, e o público entende. Exige UX explícita de aceite e de troca de chave.
- **ACME / Let's Encrypt**: confiança de CA pública, exige domínio e porta 80/443 disponíveis.

Recomendação: TOFU como padrão, ACME como opção documentada. O aviso de mudança de chave precisa ser impossível de ignorar — no tema, é literalmente um `Alerta · 警告` bloqueante.

## Autenticação

**[EM ABERTO — escolher em M2]** Duas direções:

1. **Chave pública (Ed25519)** — o cliente gera par de chaves no primeiro uso; o servidor guarda a pública. Desafio-resposta no handshake. Sem senha, sem hash para vazar, e prepara terreno para E2EE. Custo: recuperação de conta e uso em múltiplos dispositivos precisam de fluxo próprio.
2. **Senha** — Argon2id com parâmetros modernos, salt por usuário. Familiar, fácil de usar em vários dispositivos. Traz todos os problemas conhecidos de senha.

**Decidido em M5, exatamente como recomendado** (ADR 0021). Chave pública Ed25519 como identidade; entrada no Dogma por **convite de uso único** (160 bits, sete dias, consumido atomicamente) ou por **senha do Dogma** em Argon2id, à escolha do operador. Um Dogma sem nenhum dos dois é aberto, que segue sendo o padrão para rede local — e o `seeled` avisa em voz alta ao subir assim escutando fora do loopback.

O segredo viaja no `Hello`, antes do desafio-resposta: gastar verificação de assinatura com quem não devia estar batendo à porta é trabalho de graça para quem varre a internet. A recusa é sempre `CredentialRejected`, uniforme, com o motivo real só no log do operador.

O convite é o que torna um link compartilhável defensável — ver o esquema `seele://` no ADR 0006, que carrega endereço, impressão digital do certificado e convite, mas **nunca** a senha.

Independente da escolha:
- Rate limiting de tentativas, com backoff por IP e por identidade.
- Mensagem de falha uniforme (não revelar se a conta existe).
- Sessões com expiração e revogação server-side.

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
recusada: a chave teria de estar viva no mesmo disco, porque o Dogma precisa
servir o arquivo a quem tem permissão a qualquer momento. Isso protege contra
notebook roubado e contra nada mais — e contra notebook roubado a cifra de disco
inteiro do sistema protege melhor, é o que a pessoa já tem, e ela pode ligar
hoje.

Em trânsito nada muda: TLS 1.3 dentro do QUIC, sem modo claro. **O arquivo não é
legível na rede. É legível no disco de quem hospeda.**

**Um Dogma não varre vírus.** Não há motor, não há base de assinaturas e não há
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
- Fuzzing dos parsers de `seele-proto` (`cargo-fuzz`) — é a superfície que recebe bytes de rede não confiáveis.
- Toda entrada de rede tem limite de tamanho antes de alocar.

## Não fazer

- Telemetria enviada a terceiros. O produto não fala com ninguém além do Dogma escolhido.
- Crash reporting automático sem consentimento explícito.
- Qualquer criptografia caseira. Só primitivas de bibliotecas auditadas.
