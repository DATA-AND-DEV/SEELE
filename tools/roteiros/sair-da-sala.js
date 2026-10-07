// SAIR DA SALA, pelos dois lados em que ele falhava.
//
// 1. **O rótulo a cada quadro.** O rótulo do rodapé só era reescrito pelo
//    desenho da chamada, e só com a chamada na frente: quem chegava já dentro
//    de uma sala via SAIR DO SERVIDOR até abrir a grade. O quadro do dublê
//    começa com a pessoa dentro da PONTE, então o primeiro rodapé já tem de
//    dizer SAIR DA SALA.
// 2. **A grade fecha.** O botão do operador foi fundido com o SAIR DA SALA da
//    grade, e a volta aos canais não veio junto: quem saía ficava na grade
//    lendo «VOCÊ NÃO ESTÁ EM NENHUMA SALA».
//
// Os dois foram achados no simulador do iOS em 05/10/2026, e valem igual no
// desktop. O dublê não tira a pessoa da sala no `leave_voice_room`, e por isso
// o rótulo depois de sair não se mede aqui: o que se mede é a grade fechando.

await entrarNaSessao();
await espera(700);

const sair = document.getElementById("operador-sair");
relatar("recém-chegado, dentro da PONTE: " + sair.textContent.trim());
exigir(
  sair.textContent.trim() === "SAIR DA SALA",
  "quem chega já dentro de uma sala vê «" + sair.textContent.trim() +
    "» no rodapé, e não SAIR DA SALA: o rótulo só é desenhado com a chamada na frente",
);

// A grade, pela fileira da sala em que se está.
const fileira = document.querySelector("#lista-voice_rooms button[data-dentro='sim']");
exigir(fileira !== null, "não achei a fileira da sala em que o quadro diz que se está");
fileira.click();
await ate(() => visivel("vista-chamada") === "VISIVEL", 2000);
exigir(visivel("vista-chamada") === "VISIVEL", "a grade não abriu pela fileira da sala");

sair.click();
await ate(() => visivel("vista-chamada") === "escondida", 2000);
relatar("depois de SAIR DA SALA: vista-chamada " + visivel("vista-chamada"));
exigir(
  visivel("vista-chamada") === "escondida",
  "SAIR DA SALA deixou a grade da chamada aberta, e quem saiu de propósito fica " +
    "olhando uma sala em que não está",
);
