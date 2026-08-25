# SEELE — P2P Networking Development Specification

## 1. Objetivo

O SEELE é um sistema de comunicação P2P em Rust, inspirado conceitualmente em um "Discord de bolso".

O objetivo desta etapa é reestruturar a camada de networking para permitir que dois usuários estabeleçam uma conexão **direta entre seus dispositivos**, mesmo quando ambos estão atrás de NATs distintos.

A infraestrutura central do SEELE **não deve transportar áudio, voz ou dados da aplicação**.

O servidor central deve atuar exclusivamente como **Rendezvous / Discovery Daemon**, ajudando os peers a se encontrarem e coordenando o estabelecimento da conexão P2P.

A comunicação após o estabelecimento da conexão deve ser:

```text
Peer A <══════════════════> Peer B
             P2P
```

O servidor deve ficar completamente fora do caminho dos dados.

---

# 2. Requisitos fundamentais

A implementação deve respeitar os seguintes princípios.

## 2.1 P2P obrigatório

O áudio deve sempre seguir:

```text
Microphone
    ↓
Opus
    ↓
SEELE protocol
    ↓
QUIC
    ↓
UDP
    ↓
Internet
    ↓
Peer
```

Nunca:

```text
Peer A
   ↓
SEELE Daemon
   ↓
Peer B
```

O servidor central não deve funcionar como relay.

---

## 2.2 Zero tráfego de áudio no servidor

O Rendezvous Daemon pode receber:

- registro de peers;
- informações de sessão;
- descoberta de peers;
- heartbeat;
- candidatos de conexão;
- mensagens de coordenação para NAT traversal.

O Rendezvous Daemon NÃO pode receber:

- áudio;
- voz;
- mensagens de texto da aplicação;
- streams;
- arquivos;
- screen sharing;
- dados da aplicação.

O servidor deve permanecer no **control plane**.

Os peers devem permanecer no **data plane**.

---

# 3. Arquitetura desejada

A arquitetura final esperada é:

```text
                         SEELE RENDEZVOUS
                         ┌───────────────┐
                         │               │
                         │ Peer Discovery│
                         │ NAT Discovery │
                         │ Coordination  │
                         │               │
                         └───────┬───────┘
                                 │
                          control plane only
                           /             \
                          /               \
                         ▼                 ▼
                  ┌────────────┐   ┌────────────┐
                  │   Peer A   │   │   Peer B   │
                  │            │   │            │
                  │    NAT A   │   │    NAT B   │
                  └─────┬──────┘   └──────┬─────┘
                        │                   │
                        │ UDP hole punching│
                        │                   │
                        └═══════════════════┘
                              DIRECT P2P
                                  │
                                QUIC
                                  │
                                Opus
```

Após a conexão:

```text
Rendezvous
    │
    └── connection established
             │
             ▼
           idle
```

O Rendezvous Daemon não deve participar da chamada.

---

# 4. Estado atual

O SEELE atualmente utiliza Rust e QUIC/UDP como base do transporte.

A aplicação possui conceitos de:

- `connection`
- `seeled`
- identidade dos peers
- salas
- conexão QUIC
- TLS
- Opus
- comunicação de voz

O networking atual apresenta problemas quando os peers estão em redes diferentes, especialmente quando ambos estão atrás de NAT.

O principal problema a ser resolvido é:

```text
Peer A
  ↓
NAT A
  ↓
Internet
  ↓
NAT B
  ↓
Peer B
```

O fato de o Rendezvous Daemon conhecer os peers não significa que um peer consiga simplesmente conectar diretamente ao outro.

É necessário implementar uma estratégia apropriada de **NAT traversal**.

---

# 5. Não assumir que IP/porta descobertos são suficientes

Um erro arquitetural a evitar:

```text
Peer A → Daemon

Daemon:
A = 200.100.10.20:50000
B = 177.20.30.40:50001

A → B
```

Isso não é suficiente para garantir conectividade.

O sistema deve considerar:

- NAT behavior;
- UDP mappings;
- endpoint-dependent NAT;
- symmetric NAT;
- CGNAT;
- firewall;
- redes corporativas;
- mudança de IP;
- mudança de porta;
- múltiplas interfaces;
- IPv4;
- IPv6.

---

# 6. Endpoint discovery

O Rendezvous Daemon deve ser capaz de observar o endpoint público utilizado pelo peer.

O cliente não deve simplesmente informar:

```text
my_ip = X
my_port = Y
```

O servidor deve determinar o endpoint a partir do socket recebido.

Conceitualmente:

```text
Peer
  │
  │ UDP REGISTER
  ▼
Rendezvous
  │
  ├── source IP
  ├── source port
  │
  ▼
Observed Endpoint
```

O servidor deve devolver ao peer algo equivalente a:

```text
OBSERVED_ENDPOINT
ip = X
port = Y
```

Esse mecanismo deve ser tratado como parte da descoberta de candidatos.

---

# 7. Candidate discovery

Um peer pode possuir múltiplos candidatos.

Exemplo:

```text
Candidate {
    type: Host,
    address: 192.168.1.10:50000
}

Candidate {
    type: ServerReflexive,
    address: 200.10.20.30:61234
}
```

O sistema deve considerar pelo menos:

### Host candidates

Interfaces locais:

```text
127.0.0.1
192.168.x.x
10.x.x.x
172.16.x.x
IPv6 local
```

### Daemon-reflexive candidates

Endpoint observado pelo Rendezvous Daemon.

### Futuras extensões

A arquitetura deve permitir adicionar novos tipos de candidate posteriormente.

---

# 8. NAT traversal

O objetivo é implementar **UDP hole punching** ou utilizar uma biblioteca madura que forneça esse mecanismo.

O agente deve investigar antes de implementar do zero:

- STUN;
- ICE;
- UDP hole punching;
- simultaneous open;
- NAT behavior discovery;
- QUIC NAT traversal;
- iroh;
- outras bibliotecas Rust maduras.

A decisão deve ser baseada em:

1. maturidade;
2. estabilidade;
3. suporte a Rust;
4. compatibilidade com QUIC;
5. capacidade de operar sem relay;
6. controle sobre identidade;
7. controle sobre criptografia;
8. capacidade de manter o SEELE protocol;
9. licença;
10. manutenção do projeto.

Não substituir a arquitetura existente simplesmente porque uma biblioteca oferece uma solução completa.

---

# 9. Preferência por biblioteca especializada

NAT traversal é uma área complexa.

O projeto deve preferencialmente reutilizar uma implementação madura quando isso não comprometer a arquitetura do SEELE.

Uma das tecnologias que deve ser investigada é:

```text
iroh
```

Porém, NÃO assumir que iroh deve obrigatoriamente ser utilizado.

O agente deve comparar alternativas antes de tomar a decisão arquitetural.

A análise deve responder:

- É possível utilizar iroh somente para estabelecimento da conexão?
- É possível manter o protocolo SEELE sobre a conexão?
- É possível impedir relay?
- É possível utilizar somente direct connections?
- Como identificar uma conexão `direct`?
- Como detectar falha de NAT traversal?
- Qual controle o SEELE mantém sobre identidade e autenticação?
- O QUIC atual baseado em Quinn pode ser mantido?
- A utilização da biblioteca introduz dependências ou comportamentos indesejados?

---

# 10. QUIC

O SEELE utiliza QUIC como transporte.

A implementação NÃO deve remover QUIC sem uma justificativa técnica forte.

O fluxo desejado é:

```text
NAT Traversal
      ↓
UDP path established
      ↓
QUIC
      ↓
TLS
      ↓
SEELE Protocol
      ↓
Opus / Application Data
```

O NAT traversal deve ser considerado uma camada anterior ao transporte de aplicação.

---

# 11. Connection Manager

Criar uma abstração responsável por controlar o ciclo de vida da conexão P2P.

Estados sugeridos:

```text
DISCONNECTED
    ↓
DISCOVERING
    ↓
CANDIDATES_FOUND
    ↓
PUNCHING
    ↓
PATH_ESTABLISHED
    ↓
QUIC_CONNECTING
    ↓
CONNECTED
```

Estados de erro:

```text
DISCOVERY_FAILED
NAT_TRAVERSAL_FAILED
QUIC_FAILED
TIMEOUT
PEER_UNAVAILABLE
```

O Connection Manager deve esconder os detalhes de networking do restante da aplicação.

A camada de áudio não deve precisar saber:

- qual NAT existe;
- qual IP público foi utilizado;
- qual candidato venceu;
- como ocorreu o hole punching.

Ela deve simplesmente receber uma conexão funcional.

---

# 12. Falhas esperadas

O sistema deve assumir explicitamente que P2P não pode ser garantido em 100% das redes.

Casos que devem ser investigados e testados:

## Caso A — ambos com IP público

Esperado:

```text
DIRECT
```

## Caso B — um atrás de NAT comum

Esperado:

```text
DIRECT
```

## Caso C — ambos atrás de NAT comum

Esperado:

```text
DIRECT
```

na maioria dos casos.

## Caso D — ambos na mesma LAN

Deve tentar conexão local diretamente.

## Caso E — CGNAT

Pode funcionar ou falhar dependendo do comportamento do NAT.

## Caso F — Symmetric NAT

Pode falhar.

## Caso G — dois NATs restritivos

Pode falhar.

## Caso H — firewall bloqueando UDP

Deve falhar claramente.

## Caso I — rede corporativa

Pode bloquear UDP/QUIC.

## Caso J — IPv6 disponível

Investigar utilização de IPv6 como caminho preferencial quando disponível.

---

# 13. Não implementar relay

O projeto NÃO deve adicionar relay como fallback nesta etapa.

Quando NAT traversal falhar:

```text
NAT_TRAVERSAL_FAILED
```

deve ser retornado ao usuário.

O comportamento esperado é:

```text
Trying direct connection...
       ↓
NAT traversal...
       ↓
Success
       ↓
DIRECT CONNECTION
```

ou:

```text
Trying direct connection...
       ↓
NAT traversal...
       ↓
Failed
       ↓
Unable to establish direct connection
```

Não enviar áudio através do Rendezvous Daemon.

---

# 14. Segurança

O Rendezvous Daemon não deve ser considerado confiável para os dados da aplicação.

A arquitetura deve manter:

```text
Peer A
   │
   │ encrypted
   ▼
Peer B
```

O servidor deve apenas facilitar a descoberta.

O agente deve preservar a identidade e autenticação existentes do SEELE.

Não introduzir confiança adicional no Rendezvous Daemon sem justificativa.

O servidor deve conhecer apenas o mínimo necessário para realizar:

- peer discovery;
- room membership;
- endpoint discovery;
- connection coordination.

---

# 15. Privacidade

O Rendezvous Daemon deve armazenar o mínimo possível.

Idealmente:

```text
Room
Peer ID
Public endpoint
Last seen
```

com expiração automática.

Informações temporárias devem possuir TTL.

Quando o peer sair:

```text
PEER_LEFT
```

seu endpoint deve deixar de ser anunciado.

---

# 16. Custo

Um requisito fundamental é:

> O aumento do número de chamadas P2P não deve aumentar proporcionalmente o tráfego da VPS.

Exemplo desejado:

```text
100 calls

VPS:
discovery / coordination traffic

Peers:
100 × P2P audio connections
```

Não:

```text
100 calls

VPS:
100 × audio streams
```

O Rendezvous Daemon deve permanecer extremamente leve.

---

# 17. Reconexão

A conexão P2P deve tolerar mudanças de rede.

Exemplos:

```text
Wi-Fi → 4G
4G → Wi-Fi
Ethernet → Wi-Fi
VPN ativada
VPN desativada
IP público alterado
NAT mapping alterado
```

O sistema deve detectar perda do caminho e tentar:

```text
candidate rediscovery
        ↓
new NAT traversal
        ↓
new QUIC connection
```

sem exigir reinício completo do SEELE.

---

# 18. Métricas de conexão

O SEELE deve ser capaz de informar ao usuário:

```text
Connection: DIRECT
RTT: 34ms
Packet Loss: 0.2%
Jitter: 4ms
```

Também deve existir informação suficiente para diagnosticar:

```text
NAT traversal failed
Candidate A failed
Candidate B failed
UDP unreachable
QUIC handshake timeout
```

Isso será essencial para debugging.

---

# 19. Diagnóstico

Criar uma ferramenta ou modo de diagnóstico que permita testar:

```text
$ seele network test
```

e produzir algo semelhante a:

```text
Network Test
────────────

IPv4:              OK
IPv6:              AVAILABLE

UDP:               OK
QUIC:              OK

Local endpoint:
192.168.1.20:50000

Observed endpoint:
200.100.30.40:61234

NAT:
UNKNOWN / CONE / SYMMETRIC

Rendezvous:
OK

Peer discovery:
OK

Hole punching:
SUCCESS / FAILED
```

Isso deve ser considerado parte importante do desenvolvimento.

---

# 20. Testes

O agente deve criar uma estratégia de testes envolvendo diferentes redes.

Testes mínimos:

### Teste 1

```text
PC A — Wi-Fi residencial
PC B — Wi-Fi residencial
```

### Teste 2

```text
PC A — Ethernet residencial
PC B — Wi-Fi residencial
```

### Teste 3

```text
PC A — residencial
PC B — 4G/5G
```

### Teste 4

```text
PC A — residencial
PC B — CGNAT
```

### Teste 5

```text
PC A — CGNAT
PC B — CGNAT
```

### Teste 6

```text
PC A — mesma LAN
PC B — mesma LAN
```

### Teste 7

```text
PC A — rede corporativa
PC B — residencial
```

### Teste 8

```text
IPv4
vs
IPv6
```

O objetivo não é fazer todos os casos funcionarem.

O objetivo é **identificar precisamente quais casos funcionam e quais não funcionam**.

---

# 21. Critérios de sucesso

A implementação será considerada bem-sucedida quando:

- [ ] dois peers atrás de NAT residencial conseguem estabelecer conexão P2P;
- [ ] áudio nunca passa pelo Rendezvous Daemon;
- [ ] Rendezvous Daemon não funciona como relay;
- [ ] o endpoint público dos peers é descoberto automaticamente;
- [ ] peers descobrem candidatos uns dos outros;
- [ ] UDP hole punching ocorre automaticamente;
- [ ] QUIC é estabelecido sobre o caminho P2P;
- [ ] a aplicação não precisa conhecer IP público manualmente;
- [ ] não existe port forwarding manual para o usuário;
- [ ] não é necessário configurar firewall do roteador manualmente na maioria dos cenários;
- [ ] falhas de NAT traversal são detectadas e reportadas claramente;
- [ ] o servidor central não precisa transportar áudio;
- [ ] o tráfego do servidor permanece restrito ao control plane;
- [ ] reconexão após mudança de rede é suportada;
- [ ] o sistema possui logs suficientes para diagnosticar problemas de conectividade.

---

# 22. O que NÃO fazer

Não:

- implementar relay de áudio;
- mover o áudio para a VPS;
- depender de port forwarding manual;
- exigir IP público do usuário;
- assumir que IP privado é alcançável pela Internet;
- assumir que QUIC sozinho resolve NAT traversal;
- assumir que STUN sozinho garante conectividade;
- assumir que todos os NATs permitem hole punching;
- remover QUIC sem análise;
- substituir o protocolo SEELE inteiro por uma biblioteca externa sem necessidade;
- armazenar dados desnecessários no Rendezvous Daemon;
- esconder falhas de NAT como erros genéricos.

---

# 23. Resultado arquitetural esperado

Ao final desta etapa, o SEELE deve funcionar conceitualmente assim:

```text
                    ┌──────────────────────┐
                    │ SEELE RENDEZVOUS     │
                    │                      │
                    │ Discovery            │
                    │ Endpoint Discovery   │
                    │ Coordination         │
                    └──────────┬───────────┘
                               │
                    control plane only
                         /           \
                        /             \
                       ▼               ▼
                 ┌──────────┐   ┌──────────┐
                 │  Connection A  │   │  Connection B  │
                 │          │   │          │
                 │ NAT A    │   │ NAT B    │
                 └────┬─────┘   └────┬─────┘
                      │              │
                      │ UDP Hole     │
                      │ Punching     │
                      ▼              ▼
                 ═════════════════════════
                         DIRECT P2P
                 ═════════════════════════
                             │
                            QUIC
                             │
                            TLS
                             │
                         SEELE Protocol
                             │
                            Opus
```

A regra fundamental é:

> **The Rendezvous Daemon tells peers where to find each other. It never carries the conversation.**

---

# 24. Instrução para o agente de desenvolvimento

Antes de modificar o código:

1. analisar completamente a implementação atual de networking;
2. mapear o fluxo atual `connection → seeled → VPS`;
3. identificar exatamente onde o estabelecimento QUIC ocorre;
4. identificar como endpoints são atualmente determinados;
5. identificar onde NAT traversal atualmente falha;
6. analisar as dependências Rust existentes;
7. investigar soluções maduras de NAT traversal;
8. comparar implementação própria vs biblioteca especializada;
9. propor uma arquitetura concreta;
10. produzir um plano de implementação incremental;
11. definir mudanças por crate/módulo/arquivo;
12. definir estratégia de testes;
13. somente depois iniciar a implementação.

Não implementar uma solução parcial antes de produzir o plano.

O plano deve priorizar:

```text
P2P
↓
NAT traversal
↓
Direct QUIC
↓
Zero relay
↓
Zero application traffic through server
```

O objetivo final não é simplesmente "fazer uma conexão funcionar".

O objetivo é construir uma camada de networking **P2P, independente, controlável pelo usuário e sem custo de banda proporcional ao uso das chamadas no servidor central**.