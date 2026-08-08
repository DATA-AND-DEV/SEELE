# 08 — Segurança

## Postura

Servidor auto-hospedado, comunidades pequenas, operador confiável mas não necessariamente especialista em segurança. O objetivo é ser **seguro por padrão sem configuração**, e nunca oferecer um caminho não criptografado "para facilitar".

## Modelo de ameaça

| Ameaça | Tratamento em v1 |
|---|---|
| Escuta passiva na rede | TLS 1.3 obrigatório via QUIC |
| Man-in-the-middle no primeiro contato | TOFU com pinning e aviso explícito na troca de chave |
| Cliente malicioso falando em Cage sem permissão | Validação server-side em todo datagram |
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

Recomendação: chave pública como mecanismo primário, com convite por token de uso único para entrada em um Dogma. Senha como fallback opcional configurável pelo operador.

Independente da escolha:
- Rate limiting de tentativas, com backoff por IP e por identidade.
- Mensagem de falha uniforme (não revelar se a conta existe).
- Sessões com expiração e revogação server-side.

## Autorização

Toda ação é verificada no servidor, sempre, mesmo que o cliente já esconda o botão. A interface esconder é conveniência; o servidor negar é a segurança. Cobrir isso com testes explícitos: para cada permissão, um teste de cliente sem ela tentando a ação.

## Privacidade de mídia — caminho para E2EE

O desenho SFU (`01`) foi escolhido em parte por isso: **o servidor já não decodifica áudio**. Adicionar criptografia fim-a-fim é um incremento, não uma reescrita.

Esboço para pós-v1:
- Chave de mídia por Cage, negociada entre participantes.
- Rotação da chave a cada entrada e saída (forward secrecy em relação a quem sai).
- Cabeçalho do datagram permanece em claro (o servidor precisa de `ssrc` e `seq` para encaminhar); apenas o payload Opus é cifrado.
- Verificação de identidade fora de banda entre pilotos (comparação de fingerprint).

**Não prometer E2EE em v1.** Documentar honestamente: em v1, o operador do servidor pode, em teoria, capturar mídia. Isso é aceitável para o modelo de uso (você hospeda para seu próprio grupo), mas precisa estar escrito.

## Práticas de código

- `#![forbid(unsafe_code)]` em todos os crates exceto `magi-ffi` e bindings de áudio, onde `unsafe` é justificado por comentário caso a caso.
- `cargo deny` e `cargo audit` no CI, falhando o build em vulnerabilidade conhecida.
- Nenhum segredo em log. Nenhum payload de mídia em log, nem em nível trace.
- Fuzzing dos parsers de `magi-proto` (`cargo-fuzz`) — é a superfície que recebe bytes de rede não confiáveis.
- Toda entrada de rede tem limite de tamanho antes de alocar.

## Não fazer

- Telemetria enviada a terceiros. O produto não fala com ninguém além do Dogma escolhido.
- Crash reporting automático sem consentimento explícito.
- Qualquer criptografia caseira. Só primitivas de bibliotecas auditadas.
