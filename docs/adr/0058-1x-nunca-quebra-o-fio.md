# 0058 — 1.x nunca quebra o fio

Status: **aceito** — decidido por quem responde pelo projeto em 2026-10-08
(spec-mestre da 0.16, D-fio-1 a D-fio-7).
Data: 2026-10-08
Sobre `docs/superpowers/specs/2026-10-08-0.16-design.md` (§1 e §3) e o adendo
`docs/superpowers/specs/2026-10-08-0.16-adendo-negociacao-de-versao.md`, que
é o desenho. **Este ADR é a definição da versão de servidor.**

> **Emendado pelo [0063](0063-a-linha-2-fala-webrtc.md) em 2026-10-24.**
>
> - A 0.17 é a linha 2: protocolo 10, Servidor 2.0, transporte WebRTC.
> - A tabela do item 1 passa a ser `9 → linha 1` (o único número dela) e `10.. → linha 2`.
> - A frase do item 7 «uma 2.0 continua aceitando o `seele/1`» **não vale para a linha 2**: não há QUIC nela, e a recusa à linha 1 sai pelo link, pelo ENC/3 e pelo launcher.
> - O resto continua: a regra da mudança só por acréscimo vale dentro da linha 2, e o `[c, 11, 0]` continua congelado.

## Contexto

O protocolo quebrou quatro vezes em 17 dias: 2→3, 3→6, 6→7 e 7→8 (análise
de 22/09, §3.2). A janela N−1 de `specs/10-convencoes.md` valia só para quem
ouvia: todo quadro saía carimbado com o número global, e o par mais velho o
recusava antes de ler o corpo. A recusa aparecia como «NADA RESPONDEU NESSE
ENDEREÇO», e a frase «VERSÃO INCOMPATÍVEL» era inalcançável. A quebra 7→8 era
quase toda evitável: as listas só cresciam no fim, e o servidor já calava o que
o par não entendia.

A 0.16 cria duas numerações: a do app (0.16, 0.16.1…) e a do servidor
(Servidor 1.0, 1.1…). Alguma regra precisa dizer o que é «a mesma linha».

## Decisão

**O Servidor X.Y é a linha X. Dentro de uma linha, toda versão conversa com
toda versão, nos dois sentidos.** O que torna isso verdade:

1. **O número do fio é um só e só cresce.** O Servidor 1.0 é o 9. Uma versão
   menor de servidor (1.1, 1.2…) sobe o número uma vez, e um conserto
   (`seeled` da 0.16.1) não sobe. A tabela de linhas é congelada:
   `1..=8` → antes da 1.0, `9..` → linha 1.
2. **Toda 1.x fala o 9.** A faixa de uma 1.x é `[9, N]`. O `Hello` leva a
   faixa, e os dois lados falam o menor dos máximos.
3. **A mudança é só por acréscimo:**
   - variante nova vai no fim da lista;
   - motivo novo vai no fim do enum, e sai só para quem o entende;
   - **campo novo numa mensagem que já existe, nunca**, porque o `postcard` lê
     por posição. Por isso os campos que a linha 1 vai precisar entram no 9,
     inclusive o assento do E2EE;
   - nada se remove nem se reordena;
   - qualquer coisa fora disso é a 2.0.
4. **A mensagem desconhecida da lista de cima é pulada, contada e registrada
   uma vez.** Ela é rede de segurança para um portão esquecido, e não licença
   para mandar o que o par não entende.
5. **A voz e a tela têm números próprios**, que sobem só quando os bytes delas
   mudam.
6. **A recusa por versão é `[c, 11, 0]`**, legível por toda build desde a 0.11.
   Para um par de antes da 1.0, `c` é o carimbo dele. Para um par da linha 1 em
   diante, `c` é o número de quem recusa, e a linha viaja no byte 0.
7. **Ficam congelados para sempre:** o byte 0 como número do protocolo, o
   ordinal 11 de `ServerMessage` (`Disconnecting`), o `Incompatible = 0` de
   `DisconnectReason` e o ALPN `seele/1`. Uma 2.0 continua aceitando o
   `seele/1` para poder recusar uma 1.x com legibilidade.
8. **Os bytes-ouro de `crates/seele-proto/tests/fio_1_0.rs` passam a
   `# estado: congelado` no commit que publica a 0.16.** Daí em diante, a
   regeração só acrescenta, e um quadro da linha 1 que mude de bytes reprova.

## Alternativas

- **Byte 0 igual à linha, com a versão menor dentro do `Hello`.** Recusada
  porque o número que viaja deixaria de crescer, e o `Unidade.protocol` do
  launcher deixaria de significar o que diz. A tabela congelada dá a linha sem
  isso.
- **Um envelope `Extensao { tipo, corpo }`, anunciado por capacidade.**
  Recusado porque é uma segunda forma de mandar mensagem, com regras próprias.
  O quadro já tem comprimento, e pular o desconhecido já é barato.
- **A janela `[P−2, P]` com 60 dias** (anexo da análise). Recusada porque
  «1.x nunca quebra» é mais simples de prometer e de guardar: o piso da linha é
  fixo e não desliza.
- **Reescrever o byte de versão da voz por destinatário.** Recusada porque um
  número próprio, como o da tela, não custa nada a cada datagrama e não depende
  do servidor.

## Consequências

- **A 0.16 não fala com a 0.15**, e isso é o decidido. Quem precisa da 0.15
  abre a linha legada pelo launcher.
- O guarda da release publicada (`control.rs`,
  `a_publicada_ou_e_da_mesma_linha_ou_le_a_recusa_desta_build`) mede contra a
  publicada de verdade. **Quem publica uma versão de servidor nova atualiza a
  `PUBLICADA` dele.** Com o número de antes (3), ele passava sem medir nada.
- Uma 2.0 tem de manter o ALPN `seele/1` e a recusa `[c, 11, 0]`, mesmo que não
  fale mais nada da linha 1.
- O servidor de uma 1.x precisa calar, para quem negociou abaixo, toda variante
  e todo motivo acima. Hoje isso mora em `session::entende_a_mensagem`. A
  sessão do Plano 3 o traz para uma tabela no `seele-proto`.
- `specs/10-convencoes.md` deixa de prometer a janela N−1 e aponta para cá.
