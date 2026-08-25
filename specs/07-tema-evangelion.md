# 07 — Tema

> **Nota — 2026-08-21, ADR 0033.** A **camada de linguagem deste documento foi
> retirada da interface.** O vocabulário abaixo — Server Central, VoiceRoom, Linha,
> Pessoa, inserir connection, ejetar, A.T. Field, Taxa de Sincronização, PADRÃO: AZUL,
> e o japonês decorativo — **não é mais o que aparece na tela**. A autoridade
> sobre a palavra que a pessoa lê passou a ser `docs/glossario.md`.
>
> **O resto desta spec continua valendo, e é a maior parte dela:** a regra de
> ouro da densidade de informação, a hierarquia de comando, os tokens de cor, a
> tipografia, o canto reto, a ausência de sombra e de gradiente. Sai a língua,
> fica o desenho.
>
> O texto abaixo fica **inalterado de propósito**, como registro do que o
> produto foi. Quem precisar do vocabulário vigente lê o glossário; quem
> precisar saber por que o tema é assim lê isto.

O tema é **vocabulário de produto**, não skin. É definido aqui uma vez e aplicado em todo lugar: interface, mensagens de erro, logs, documentação, nome de binário. Não se redesenha por tela.

## Regra de ouro

A referência não é "laranja e preto com fonte futurista". É a **densidade de informação** e a **hierarquia de comando** das telas da NERV: muito rótulo pequeno, muitos números vivos, quase nenhuma decoração, e a sensação de que tudo está sendo monitorado.

Segunda regra: **o tema nunca custa clareza**. Se um usuário precisa saber o que "Distúrbio harmônico" significa para resolver um problema, a interface falhou. Nomeação temática vem sempre acompanhada do dado concreto — "Distúrbio harmônico · perda 8,4%".

## Glossário canônico

Estes termos são obrigatórios e consistentes em toda a superfície do produto.

| Conceito | Termo | Nota |
|---|---|---|
| Instância de servidor | **Server Central** | Plural: Servers |
| Daemon | **seeled** | |
| Cliente | **SEELE** / `connection` | |
| Canal de voz | **VoiceRoom** | |
| Canal de texto | **Linha** | |
| Usuário | **Pessoa** | |
| Entrar em canal de voz | **Inserir connection** | |
| Sair | **Ejetar** | |
| Qualidade de conexão | **Taxa de Sincronização** | 0–100%, ver `02` |
| Latência | **Atraso de sinal** | ms |
| Perda de pacote | **Distúrbio harmônico** | |
| Mudo (microfone) | **A.T. Field** ativo | |
| Surdo (alto-falante) | **Isolamento total** | |
| Sessão verificada | **PADRÃO: AZUL** | |
| Sessão não verificada | **PADRÃO: LARANJA** | |
| Reconectando | **Bateria interna** | contagem de 04:59 |
| Notificação crítica | **Alerta · 警告** | |
| Configurações | **Terminal Server** | |
| Papéis | Comandante, Operador, Pessoa, Observador | |
| Subsistemas | PERMISSIONS, MEDIA, PERSISTENCE | ver `04` |

## O elemento assinatura

**A Taxa de Sincronização por pessoa.** Cada pessoa no roster tem um percentual vivo derivado do RTT, jitter e perda daquela conexão. Nenhum concorrente mostra isso; aqui é a coisa mais visível da tela. É a métrica que dá caráter ao produto e, não por acaso, é genuinamente útil — quando alguém fica difícil de entender, todo mundo já sabe por quê.

## A bateria interna

Quando a conexão cai, o cliente não fecha nem mostra um spinner. Ele entra em **bateria interna**: contagem regressiva de 5 minutos em vermelho, tentativas de reconexão listadas, interface esmaecida mas ainda legível — o histórico continua ali para leitura.

Funcionalmente é um período de graça de sessão, sustentado pela migração de conexão do QUIC (ver `01`). Narrativamente é exato. Este é o melhor casamento entre tema e engenharia no projeto — proteger de simplificações.

## Tokens de cor

Valores definitivos saem do trabalho no Claude Design; estas são as **restrições** que aquele trabalho precisa respeitar:

| Papel | Regra |
|---|---|
| Fundo | Preto quase absoluto, nunca cinza-carvão neutro |
| Acento primário | Laranja NERV — cor institucional, **não** cor de sucesso |
| Alerta | Vermelho, uso exclusivo para erro e queda. Se aparece, algo está errado |
| Nominal / telemetria | Verde de fósforo |
| Identidade verificada | Azul (PADRÃO: AZUL) |
| Texto corrido | Off-white levemente amarelado. Branco puro é errado |

Faixas da Taxa de Sincronização: **≥ 85** nominal (fósforo) · **60–84** degradado (laranja NERV) · **< 60** crítico (vermelho).

Eram quatro — `≥ 90` nominal, `70–89` aceitável em off-white, `40–69` degradado, `< 40` crítico. O comp v2 (`design/SEELE v2.dc.html`) banda o mesmo número em três, corta em 85 e 60, e **não usa osso em escala de sincronia nenhuma**; o comp é posterior a esta tabela e o dono decidiu que ele vence. A consequência que importa: 80 lia-se como "fora do nominal, mas tudo bem" e agora se lê como degradado — laranja, a cor de ir olhar. É o objetivo da mudança, não um efeito colateral dela.

## Tipografia

Monoespaçada para todo dado, número, endereço e log. Display condensada e pesada, caixa alta e tracking apertado, para cabeçalhos e cartelas de alerta. Face japonesa para os fragmentos em kanji.

**Regra sobre o japonês:** kanji é acento tipográfico, sempre secundário. Nunca carrega informação necessária para operar o produto. Um usuário que não lê japonês não perde nada. Fragmentos aprovados: 警告 (alerta), 同期率 (taxa de sincronização), 第3新東京市, 発令.

## Movimento

Só a sequência de boot é generosa. No resto, movimento é diagnóstico: a barra de sincronização respira, o indicador de fala pulsa com a voz, a contagem da bateria desce. Sem transição decorativa, com **uma exceção nomeada**: a varredura — a faixa que desce sobre a scanline, herdada do comp v2 e aceita em M5 pelo ADR 0014. Ela não diagnostica nada, e o que a torna admissível é ser inofensiva: `pointer-events: none`, `aria-hidden`, e sob `prefers-reduced-motion` ela para sem sumir. É a única; qualquer outra volta a ser erro, e abrir a segunda exige emendar este parágrafo de novo. `prefers-reduced-motion` respeitado, e a TUI oferece desligar animação por completo.

Nenhuma animação pode atrasar o usuário. Se a conexão fecha em 200 ms, o boot dura 200 ms.

## Voz da interface

Operacional, fria, factual. A interface **reporta**; não pede desculpa, não é simpática, não usa primeira pessoa.

- Certo: `PADRÃO AZUL NÃO ESTABELECIDO · credencial rejeitada`
- Certo: `VOICE_ROOM-02 vazio. Insira o connection para iniciar.`
- Errado: `Ops! Não conseguimos te conectar 😥`
- Errado: `Nenhuma mensagem ainda!`

Erro sempre diz **o que aconteceu** e **o que fazer**. Tela vazia é convite à ação, não piada.

## Cuidado com direitos

Evangelion é propriedade da Khara. Usar a **linguagem visual** (paleta, densidade, tipo de cartela) e vocabulário genérico é uma coisa; reproduzir logotipos da NERV/SEELE, arte oficial, trilha sonora ou nomes de personagens como marca do produto é outra. **[EM ABERTO]** — se o projeto for público, decidir o quanto se aproxima. Recomendação: inspiração estética sim, ativos e logos oficiais não.
