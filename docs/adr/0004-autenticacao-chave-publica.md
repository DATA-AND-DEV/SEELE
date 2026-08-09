# 0004 — Autenticação por chave pública Ed25519

Status: aceito por default
Contexto: `specs/09-roadmap.md` exige a decisão em M0; `specs/08-seguranca.md` diz "escolher em M2". Contradição de cronograma (C1 do plano). A *direção* precisa vencer agora porque determina se `seele-proto` carrega o formato desafio-resposta e se o schema de CASPER tem colunas de senha — decidir depois força bump de versão de protocolo.
Decisão: chave pública Ed25519 como mecanismo primário, com convite por token de uso único para entrada em um Dogma. Senha (Argon2id) como fallback opcional habilitado pelo operador. **Implementação em M2/M3; só a direção vale agora.**
Alternativas: senha como primário. Descartado porque traz hash para vazar, não prepara terreno para E2EE, e `08` já recomenda o contrário.
Consequências: mais fácil — sem segredo do lado do servidor para vazar, desafio-resposta natural no handshake, caminho aberto para E2EE pós-v1. Mais difícil — recuperação de conta e uso em múltiplos dispositivos precisam de fluxo próprio, **que ainda não existe em spec nenhuma**.

Sinal de apoio: o protótipo em `design/` já assume este caminho — a tela `02 AUTENTICAÇÃO` mostra campo `ed25519-…`.

Pendência bloqueante para M5: `specs/06-clientes-gui.md` e `specs/09-roadmap.md` exigem "mesma sessão retomável entre TUI e app". Com par de chaves por cliente, dois clientes são duas identidades. O fluxo de vínculo de dispositivos precisa ser especificado antes de M5. Custo de reverter esta ADR depois de M2: **alto**.
