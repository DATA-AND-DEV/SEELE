# 0060 — O código de uma pessoa

Status: **aceito**. Decidido por quem responde pelo projeto em 2026-09-23 («um
código de usuário, que ele manda para um amigo»), e a forma ficou congelada no
contrato de fio da 0.16 (item 12).
Data: 2026-10-08
Sobre `docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md`
(«O código» e «Quem liga confere o código dentro do TLS»).

## Contexto

Ligar para alguém precisa de um nome que a pessoa consiga mandar a um amigo e
que continue valendo amanhã. Um endereço não serve: muda quando a pessoa troca
de rede. Um link `seele://` com impressão e endereço também não: ele envelhece
junto com o endereço (ADR 0047).

A identidade Ed25519 da pessoa (ADR 0004 e ADR 0017) é estável e já é o que o
servidor confere. O código é uma forma curta dela, que dá para ditar, colar e
conferir.

## Decisão

### A forma, congelada para sempre

Mora em `crates/seele-proto/src/codigo.rs`, que é **o único lugar** que deriva,
formata, lê e confere um código.

- **Derivação:** `SHA-256(chave pública Ed25519, 32 bytes crus)`. Os primeiros
  75 bits viram 15 símbolos de base32 Crockford.
- **Verificador:** um 16º símbolo, por Luhn mod 32. Ele pega toda troca de um
  símbolo e toda inversão de dois vizinhos, **exceto a do par de valores 0 e
  31**, que é a fraqueza conhecida do Luhn mod N. Um teste afirma essa exceção.
  É o verificador que deixa dizer «erro de digitação» sem consultar a rede.
- **Exibição:** `XXXX-XXXX-XXXX-XXXX`.
- **Leitura:** maiúsculas ou minúsculas, sem hífen nem espaço; O vale 0, e I e L
  valem 1; U é recusado; exatamente 16 símbolos.
- **As marcas do quarto:** `p` mais os 15 símbolos de dado, e `p…e` para a escuta
  e `p…s` para o socket do servidor. O `p` separa as marcas da sala das marcas
  `fp16` dos servidores, porque não é hexadecimal. A caixa das letras também é
  formato: o ponto compara a marca byte a byte.
- **Os vetores** em `vetores-de-codigo.json`, conferidos por
  `crates/seele-proto/tests/vetores_de_codigo.rs` contra uma implementação de
  referência independente, **são o contrato**.

### Onde o código é conferido

**Dentro do TLS, antes do `Hello`.** O `ConfereCodigo` lê a chave do certificado
da sala, exige que seja Ed25519, confere o código e delega ao `ring` a prova de
posse (o CertificateVerify). Uma chave que não bate vira
`ConnectError::CodigoNaoBate`, e nada foi enviado. O `PinStore` por `host:porta`
não é usado para código.

O SNI é a constante `seele-sala`, e **nunca o código**: o SNI viaja legível no
Initial do QUIC, e contaria para quem se está ligando.

### O certificado da sala também é formato

A sala apresenta um certificado Ed25519 feito da semente da `identity.key`
(`seele_server::tls::Identity::da_semente`). O nome, o número de série, a
validade e o SAN são fixos, e a assinatura é determinística, então o DER sai
igual em toda abertura.

**O DER é formato congelado.** O dono entra na própria sala por
`127.0.0.1:8385`, e o pino do TOFU é por endereço. Um DER que mudasse entre
versões daria «A CHAVE MUDOU» na própria sala. O vetor da semente de zeros está
em `crates/seele-server/tests/identidade_da_sala.rs`, e foi conferido fora do
Rust: a chave do certificado é a da semente, e a autoassinatura confere.

## Força

- Achar uma chave com o mesmo código custa cerca de 2^75 gerações de chave.
  Contra qualquer um de N códigos no ar, custa 2^75/N.
- **Depois da primeira ligação completa, a chave inteira fica pinada** sob
  `@CÓDIGO`, no arquivo de pins. Daí em diante a conferência usa 256 bits, e uma
  chave diferente com o mesmo código dá «ALGUÉM TENTOU SE PASSAR POR …».

## O que a pessoa expõe

- Quem tem o código vê, sem ligar, **se a pessoa está com o SEELE aberto e qual
  é o IP público dela**. As dicas de rede local também vão. É a natureza de ser
  encontrável, e a alavanca é «não receber ligações», que desliga a sala.
- O ponto de encontro aprende quem está online, em qual IP, e quem procura quem.
  Isso vai para a página de privacidade (LGPD), que é do dono.

## Alternativas

1. **Um número curto, distribuído por um servidor central.** Recusada: o SEELE
   não tem servidor central de contas, e um registro de números seria um.
2. **A impressão da chave inteira, em hexadecimal (64 símbolos).** Recusada:
   não se dita, e não tem verificador.
3. **Menos bits, para um código mais curto.** Não foi medido. Os 75 bits são o
   que cabe em 15 símbolos de base32 mais um verificador, em quatro grupos de
   quatro. Quem tenta se passar por alguém depois da primeira ligação esbarra
   no pino de 256 bits, e não nos 75.

## Consequências

- Trocar ou revogar o código exige identidade nova, e fica fora da 1.0.
- O código é da máquina (ADR 0061), então é o mesmo em toda versão aberta.
- No celular, a 0.16 não liga pelo código: o navegador não abre UDP e perde o
  quarto (ADR 0057).

## Custo de reverter

**Não reverte.** Mudar a derivação, o verificador, o alfabeto, as marcas ou o
DER da sala invalida todo código já distribuído, ou faz todo dono ler «A CHAVE
MUDOU» na própria sala. Por isso a forma nasce congelada e com vetores.
