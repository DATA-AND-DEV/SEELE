# Fila do ciclo seguinte

Levantada em 21/08/2026, depois de o degrau 4 fechar. Ordem de ataque dada
por quem pediu: **bugs primeiro**, depois telas, depois features.

## Bugs

1. **Sincronização dentro do Dogma.** O estado entre quem está logado no
   mesmo Dogma não fecha. Dois sintomas conhecidos:
   - não dá para sair de uma jaula e deixá-la vazia;
   - A e B estão na mesma jaula, e B não vê A nem consegue falar com ele.

   Alvo: navegação o mais limpa possível.

## Telas a ajustar

2. **Entrada num Dogma tem cliques demais.** Hoje: um clique para conferir
   identidade, outro para entrar, mais uma tela para quem ainda não tem
   permissão — que é **toda** primeira vez de alguém novo.

   Desenho pedido: a tela de conferir identidade **é** a tela de espera. Ela
   fica conferindo até o dono liberar, e aí entra. Cortar tela, reaproveitar
   tela.

3. **Tela inicial.** Quem já usou tem de conseguir ver os Dogmas antigos.

## Legendas a remover

4. «Os nomes deste Dogma são vinculados a chaves: ninguém consegue usar o
   nome de outra pessoa.»
5. «Pergunta à página de releases se saiu versão mais nova. Nada é baixado
   nem trocado agora.»
6. «Nada é consultado sozinho — só quando você pedir»

## Features

7. **Personalização do Dogma pelo anfitrião:** ícone (o da esquerda) e nome.
8. **Multi-Dogma.** Histórico à esquerda, como no Discord. Entrar num outro
   desconecta do anterior.
9. **O `+` embaixo do `C`** passa a ser "conectar a um Dogma novo".
10. **Compartilhamento de tela** — escolher app ou monitor, a la Discord.
    Pedido desde o começo deste ciclo e adiado desde então.
11. **Prévia de documentos:** PDF, XLSX, PPTX, DOCX, TXT.

---

# Avaliação de UX do Fable (importada de claude.ai/design)

Projeto `48eb9d51`, arquivo `SEELE - Avaliação UX v2.dc.html`. Cerca de 30
achados sobre as 6 telas e 4 camadas, mais 3 redesenhos.

## O conflito que precisa de decisão

A recomendação **número 1** da avaliação é «explicar o vocabulário onde ele
aparece: uma linha de nota sob cada rótulo». Oito achados dependem disso.

É o oposto do que foi pedido hoje: legendas removidas, «deixar o app mais
limpo de texto», «não gosto de nada que indique a documentação». Três saíram
neste ciclo e uma delas tinha teste próprio.

Saída proposta: **a ajuda na tecla `?`**, que a própria avaliação pede em
outro achado. O vocabulário inteiro mora lá, num lugar só, e a tela
permanente não ganha texto nenhum. Paridade com o `?` do `plug`, que é
critério que a TUI já adota.

## O que não conflita e pode ser feito já

- rótulo de botão é **verbo**, não estado: `SILENCIAR`, `ENSURDECER`,
  `SEGURE ESPAÇO PARA FALAR`. Não é texto novo, é palavra melhor;
- vermelho volta a ser reserva: `SAIR DA JAULA` em contorno, sólido só no
  hover e no foco;
- contraste AA nos rótulos pequenos — uma linha em `tokens.css`, e a
  pendência já está em `docs/tokens-achados.md`;
- agrupar mensagens consecutivas do mesmo autor e pôr divisor de dia;
- pontos de quebra: a conversa nunca é a primeira a apertar;
- estado vazio da Linha escrito no meio da área de mensagens;
- menção vira faixa de canto, não modal de tela cheia;
- painel fixo mostra **gente**, não medida;
- `ENLACE ENCERRADO` ganha `RECONECTAR` como principal;
- o `+` morto — que já está na fila como «conectar a um Dogma novo»;
- um léxico por camada (Padrão Azul / No Ar / Nominal / MAGI são quatro);
- traduzir o que informa, manter em inglês o que é cenário.

## Sobreposição com o que já estava na fila

O item 9 (o `+` vira conectar) e o item 3 (tela inicial com Dogmas antigos)
são o mesmo terreno do achado «a coluna REDES tem 60px e um `+` morto». O
item 2 (cortar telas na entrada) é o mesmo terreno do achado ALTO sobre
`VERIFICAR IDENTIDADE`.
