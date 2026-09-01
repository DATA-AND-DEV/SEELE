// «Mostra o microfone escolhido, mas o outro como em uso.»
//
// Uma máquina com **um** microfone e o padrão da máquina escolhido: a linha do
// padrão é a escolhida, e a do aparelho é a que abriu. Duas linhas, duas
// marcas, e o mesmo aparelho embaixo das duas.
const MIC = { id: "wasapi:{0.0.1.00000000}", name: "Microfone (fifine)", default: true };
SEELE_RESPOSTAS.microfones = [MIC];
SEELE_RESPOSTAS.microfone_escolhido = null;
SEELE_RESPOSTAS.saidas = [];
SEELE_RESPOSTAS.saida_escolhida = null;
SEELE_QUADRO.capture = { id: MIC.id, name: MIC.name };
window.__SEELE_EM_SESSAO = true;

await abrirServer("tela-boot");
await espera(600);
for (const linha of document.querySelectorAll("#lista-microfones .server-dispositivo")) {
  relatar(
    "[" + linha.querySelector(".server-dispositivo-nome").textContent + "]" +
    "  →  " + linha.querySelector(".server-dispositivo-marca").textContent,
  );
}
