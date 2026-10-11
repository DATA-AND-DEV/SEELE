// Trocar de janela sem parar antes.
//
// O botão sumia enquanto a transmissão era da própria pessoa, e o único caminho
// era PARAR, fechar a caixa, reabrir e escolher de novo. Pedido assim: «quando
// estiver compartilhando o modal de compartilhamento deve permitir a troca de
// janela. Hoje só permite parar a transmissão.»
//
// O que os guardas de texto não alcançam: se o botão realmente aparece, o que
// ele diz em cada estado, e se ele se recusa a recomeçar para a mesma fonte.
//
// **A transmissão é a minha pelo `minha_transmissao` do retrato**, e não pelo
// `tela.e_minha`. Desde `14d9c30f` (22/09/2026, ADR 0054) a casca pergunta por
// `snapshot.minha_transmissao`, e `snapshot.tela` é a que esta pessoa está
// assistindo, que pode ser a de outra. Este roteiro ainda escrevia o retrato de
// antes, e de 22/09 a 10/10/2026 relatou «COMPARTILHAR» em todos os estados,
// verde, sem nenhum `exigir` que o dissesse (lote RT da F5).

function botao() {
  const b = $("compartilhar-comecar");
  return (
    (b.hidden ? "escondido" : b.textContent.trim()) +
    (b.disabled ? " (apagado)" : "") +
    " · modo=" + (b.dataset.modo ?? "—")
  );
}

// O `TelaEmCurso` de `seele-ffi`, com os nomes de lá.
const transmissao = (de, minha) =>
  ({ tela: de, de, e_minha: minha, espectadores: 0, parada: null, pedido: null });

await entrarNaSessao();

// Sem transmissão nenhuma e sem fonte escolhida.
desenharBotoesDeTela({ tela: null, minha_transmissao: null });
relatar("nada escolhido: " + botao());
exigir(botao() === "COMPARTILHAR (apagado) · modo=comecar",
  "sem fonte escolhida, o botão não está apagado: " + botao());

// Uma fonte armada, ainda sem transmitir.
fonteArmada = 3;
desenharBotoesDeTela({ tela: null, minha_transmissao: null });
relatar("fonte escolhida: " + botao());
exigir(botao() === "COMPARTILHAR · modo=comecar",
  "com uma fonte escolhida e nada no ar, o botão não é COMPARTILHAR aceso: " + botao());

// Agora a transmissão é minha, e a fonte no ar é a 3, a mesma armada.
fonteEmCurso = 3;
desenharBotoesDeTela({ tela: null, minha_transmissao: transmissao(1, true) });
const tituloDaMesma = $("compartilhar-comecar").title;
relatar("transmitindo a mesma: " + botao());
relatar("  e o title diz: " + tituloDaMesma);
exigir(botao() === "TROCAR (apagado) · modo=trocar"
    && tituloDaMesma.startsWith("é esta que já está no ar"),
  "transmitindo a fonte armada, o botão não recusa recomeçar para a mesma: " +
    botao() + " · title «" + tituloDaMesma + "»");

// A pessoa escolhe outra fonte na lista.
fonteArmada = 8;
desenharBotoesDeTela({ tela: null, minha_transmissao: transmissao(1, true) });
const tituloDaOutra = $("compartilhar-comecar").title;
relatar("escolhi outra: " + botao());
relatar("  e o title diz: " + tituloDaOutra);
exigir(botao() === "TROCAR · modo=trocar"
    && tituloDaOutra.startsWith("a transmissão de agora para"),
  "transmitindo e com outra fonte escolhida, o botão não oferece TROCAR: " +
    botao() + " · title «" + tituloDaOutra + "»");

// De outra pessoa, e nenhuma minha: COMPARTILHAR, aceso. A frase «cabe uma por
// vez» saiu (R18 da revisão da v15): o servidor guarda várias por sala, e a
// recusa por capacidade chega dele.
desenharBotoesDeTela({ tela: transmissao(2, false), minha_transmissao: null });
relatar("de outra pessoa: " + botao());
exigir(botao() === "COMPARTILHAR · modo=comecar",
  "com a transmissão de outra pessoa na tela, o botão não é COMPARTILHAR aceso: " + botao());

// E a minha acaba por fora: a fonte de agora tem de ser esquecida, ou TROCAR
// recusaria a única que a pessoa quer mandar.
fonteEmCurso = 8;
desenharBotoesDeTela({ tela: null, minha_transmissao: null });
relatar("acabou por fora: fonteEmCurso=" + fonteEmCurso + " · " + botao());
exigir(fonteEmCurso === null,
  "a transmissão acabou por fora e a fonte em curso não foi esquecida: " + fonteEmCurso);
