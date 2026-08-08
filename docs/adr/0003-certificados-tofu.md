# 0003 — TOFU como padrão de certificado

Status: aceito por default
Contexto: `specs/08-seguranca.md` deixa em aberto entre TOFU com certificado auto-assinado e ACME/Let's Encrypt. É critério de aceite de M0.
Decisão: TOFU com pinning por padrão. ACME como opção documentada, não como caminho principal. O aviso de troca de chave é um `Alerta · 警告` bloqueante, impossível de ignorar.
Alternativas: ACME por padrão. Descartado porque exige domínio e portas 80/443 disponíveis, o que contradiz a simplicidade de porta UDP única de `specs/01-arquitetura.md` e o perfil de operador descrito em `08` — confiável, mas não especialista em segurança. O modelo do SSH é o que o público-alvo já tem na cabeça.
Consequências: mais fácil — auto-hospedagem sem domínio, sem renovação, sem porta extra. Mais difícil — exige UX explícita de aceite e de troca de chave, e essa UX precisa existir na TUI (M4) e no app (M5), não só no papel. Não há caminho não criptografado em nenhum dos dois modos.

Custo de reverter: **médio**. Adicionar ACME depois é aditivo; tirar TOFU depois quebra clientes que já pinaram.
