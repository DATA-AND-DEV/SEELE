# 0007 — Sem DSP externo em v1: fone de ouvido obrigatório

Status: aceito por default
Contexto: `specs/03-audio.md` marca três `[EM ABERTO]` no mesmo pipeline — ganho automático, supressão de ruído e cancelamento de eco. As três moram na mesma biblioteca (`webrtc-audio-processing`, C++), então são uma decisão só. Não dá para escrever o pipeline de M1 sem ela.
Decisão: nenhuma dependência de DSP em C/C++ em v1. Fone de ouvido é requisito documentado. A arquitetura deixa pronto um seam de feature de compilação (`--features aec`) para plugar `webrtc-audio-processing` depois, sem reescrita.
Alternativas: (a) `webrtc-audio-processing` já em v1 — traz AEC, AGC e supressão de qualidade, ao custo de dobrar o risco de build multiplataforma dentro do milestone mais arriscado do projeto; (b) `speexdsp` — mais leve, qualidade inferior, e ainda assim é dependência C.
Consequências: mais fácil — M1 não carrega risco de toolchain C++, e o público-alvo já usa fone. Mais difícil — quem usar alto-falante terá eco, e isso precisa estar escrito na documentação de forma honesta, não escondido.

Efeito imediato no aceite de M1: os testes de `specs/09-roadmap.md` são executados **com fone**. Modo alto-falante é explicitamente não suportado em v1.

Custo de reverter: **baixo**, desde que o seam de feature exista desde M1. Sem o seam, alto.
