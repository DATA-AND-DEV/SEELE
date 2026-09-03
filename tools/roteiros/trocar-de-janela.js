// Trocar de janela sem parar antes.
//
// O botão sumia enquanto a transmissão era da própria pessoa, e o único caminho
// era PARAR, fechar a caixa, reabrir e escolher de novo. Pedido assim: «quando
// estiver compartilhando o modal de compartilhamento deve permitir a troca de
// janela. Hoje só permite parar a transmissão.»
//
// O que os guardas de texto não alcançam: se o botão realmente aparece, o que
// ele diz em cada estado, e se ele se recusa a recomeçar para a mesma fonte.

function botao() {
  const b = $("compartilhar-comecar");
  return (
    (b.hidden ? "escondido" : b.textContent.trim()) +
    (b.disabled ? " (apagado)" : "") +
    " · modo=" + (b.dataset.modo ?? "—")
  );
}

document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }

// Sem transmissão nenhuma e sem fonte escolhida.
desenharBotoesDeTela({ tela: null });
relatar("nada escolhido: " + botao());

// Uma fonte armada, ainda sem transmitir.
fonteArmada = 3;
desenharBotoesDeTela({ tela: null });
relatar("fonte escolhida: " + botao());

// Agora a transmissão é minha, e a fonte no ar é a 3.
fonteEmCurso = 3;
desenharBotoesDeTela({ tela: { de: 1, e_minha: true } });
relatar("transmitindo a mesma: " + botao());
relatar("  e o title diz: " + $("compartilhar-comecar").title);

// A pessoa escolhe outra fonte na lista.
fonteArmada = 8;
desenharBotoesDeTela({ tela: { de: 1, e_minha: true } });
relatar("escolhi outra: " + botao());
relatar("  e o title diz: " + $("compartilhar-comecar").title);

// De outra pessoa: continua recusado, com a frase de sempre.
desenharBotoesDeTela({ tela: { de: 2, e_minha: false } });
relatar("de outra pessoa: " + botao());

// E a transmissão acaba por fora: a fonte de agora tem de ser esquecida.
fonteEmCurso = 8;
desenharBotoesDeTela({ tela: null });
relatar("acabou por fora: fonteEmCurso=" + fonteEmCurso + " · " + botao());
