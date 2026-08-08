# 0005 — Porta padrão 8383/UDP

Status: proposto
Contexto: `specs/01-arquitetura.md` marca "8383 [EM ABERTO: confirmar]" e `specs/04-servidor-magi.md` repete `escuta = "0.0.0.0:8383"`. Mas o protótipo entregue em `design/` mostra `magi://toquio-3.dogma.central:7743` na tela `02 AUTENTICAÇÃO`. Duas fontes de verdade divergem.
Decisão: **pendente.** Recomendação: manter `8383`, porque aparece em dois documentos de spec contra um artefato de design, e porque o design não tem motivo técnico para preferir `7743`.
Alternativas: `7743`, alinhando a spec ao design.
Consequências: baixas em qualquer direção. O que não pode acontecer é a divergência sobreviver até a porta entrar em documentação de usuário ou em `magid.toml` de exemplo.

Não está `aceito por default` porque não há trabalho de M0/M1 bloqueado por ela — nada abre socket antes de M2. Decidir junto com ADR 0006 (esquema de URI).

Custo de reverter: **baixo** até M2, alto depois que houver instalação em produção.
